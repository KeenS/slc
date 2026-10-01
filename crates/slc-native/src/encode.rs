//! x86-64 System V encoder. Virtual temps stay in caller-saved registers or frame slots.
//! Protocol registers are never assigned a temp.

use crate::{Cond, Dest, Function, Inst, Module};
use slc_abi::{
    FRAME_CONT_PREV, FRAME_HANDLER_PREV, FRAME_MAP_FLAGS, FRAME_SLOT0, FRAME_SPILL_ENV,
    FRAME_SPILL_HANDLERS, FRAME_SPILL_VAL, MAP_EMPTY, SLC_FUEL, SLC_POOL_PTRS, SLC_PROGRAM_ENTRY,
    SLC_RT_ALLOC, SLC_RT_POLL, SLC_TEXT, Safepoint,
};

const SLC_BYTES_SINCE_GC: &str = "slc_bytes_since_gc";
const SLC_WATERMARK: &str = "slc_watermark";

const RAX: u8 = 0;
const RCX: u8 = 1;
const RDX: u8 = 2;
const RBX: u8 = 3;
const RSI: u8 = 6;
const RDI: u8 = 7;
const R12: u8 = 12;
const R13: u8 = 13;
const R14: u8 = 14;
const R15: u8 = 15;

const CALLER_SAVED: [u8; 9] = [RAX, RCX, RDX, RSI, RDI, 8, 9, 10, 11];

const PC32: u32 = 2;
const PLT32: u32 = 4;

struct Rel {
    at: usize,
    symbol: String,
    kind: u32,
    addend: i64,
}

struct FixBlock {
    at: usize,
    func: usize,
    block: usize,
}

struct FixSym {
    at: usize,
    symbol: String,
}

struct Encoder {
    buf: Vec<u8>,
    rels: Vec<Rel>,
    blocks: Vec<FixBlock>,
    syms: Vec<FixSym>,
    block_at: Vec<Vec<usize>>,
    func_at: Vec<(String, usize, usize)>,
    safepoints: Vec<Safepoint>,
}

pub fn encode(module: &Module) -> Vec<u8> {
    let mut enc = Encoder {
        buf: Vec::new(),
        rels: Vec::new(),
        blocks: Vec::new(),
        syms: Vec::new(),
        block_at: Vec::new(),
        func_at: Vec::new(),
        safepoints: Vec::new(),
    };
    for (fi, func) in module.functions.iter().enumerate() {
        enc.function(fi, func);
    }
    enc.patch();
    enc.safepoints.sort_by_key(|sp| sp.text_offset);
    elf(module, &enc.buf, &enc.safepoints, &enc.rels, &enc.func_at)
}

impl Encoder {
    fn function(&mut self, fi: usize, func: &Function) {
        let start = self.buf.len();
        self.block_at.push(vec![0; func.blocks.len()]);
        if func.entry {
            // Five pushes from `%rsp ≡ 8` land on `%rsp ≡ 0`, which `call` requires.
            self.push(RBX);
            self.push(R12);
            self.push(R13);
            self.push(R14);
            self.push(R15);
            self.rr(true, 0x89, RDI, R12);
            self.mem(true, 0x8B, R15, R12, FRAME_CONT_PREV as i32);
            self.zero(R13);
            self.zero(R14);
            self.zero(RBX);
        } else {
            assert!(func.map_id != 0, "{}", func.symbol);
            self.set_map(func.map_id);
        }
        for (bi, block) in func.blocks.iter().enumerate() {
            self.block_at[fi][bi] = self.buf.len();
            for inst in &block.insts {
                self.inst(fi, func, inst);
            }
        }
        if func.entry {
            self.rr(true, 0x89, R13, RAX);
            self.pop(R15);
            self.pop(R14);
            self.pop(R13);
            self.pop(R12);
            self.pop(RBX);
            self.buf.push(0xC3);
        }
        self.func_at.push((func.symbol.clone(), start, self.buf.len() - start));
    }

    fn inst(&mut self, fi: usize, func: &Function, inst: &Inst) {
        match inst {
            Inst::Imm { dst, value } => self.imm_loc(loc(*dst, func), *value),
            Inst::Mov { dst, src } => self.move_loc(loc(*dst, func), loc(*src, func)),
            Inst::Load { dst, base, offset, width } => {
                self.load(loc(*dst, func), loc(*base, func), *offset, *width)
            }
            Inst::Store { src, base, offset, width } => {
                self.store(loc(*src, func), loc(*base, func), *offset, *width)
            }
            Inst::CmpJcc { left, right, cond, target } => {
                let reg = self.reg(loc(*left, func));
                self.cmp_imm(reg, *right);
                self.jcc(fi, *cond, *target);
            }
            Inst::Jmp { target } => self.jmp_block(fi, *target),
            Inst::CallSlc { symbol, callee_frame_words } => {
                self.call_slc(func, symbol, *callee_frame_words)
            }
            Inst::Tail { symbol, .. } => {
                self.safepoint(func.map_id);
                self.zero(R14);
                self.jmp_sym(symbol);
            }
            Inst::Ret => self.ret(),
            Inst::Safepoint { map_id } => {
                assert!(*map_id != 0);
                self.safepoint(*map_id);
            }
            Inst::CallAlloc { words, tag, map_id, dst } => {
                self.call_alloc(func, *words, *tag, *map_id, loc(*dst, func))
            }
            Inst::Ud2 => self.buf.extend_from_slice(&[0x0F, 0x0B]),
            Inst::InRange { src, lo, hi, fail } => {
                let reg = self.reg(loc(*src, func));
                self.cmp_imm(reg, *lo);
                self.jcc(fi, Cond::L, *fail);
                self.cmp_imm(reg, *hi);
                self.jcc(fi, Cond::G, *fail);
            }
            Inst::LeaBlock { dst, block } => {
                let reg = self.reg_keep(loc(*dst, func));
                let at = self.rip(true, 0x8D, reg);
                self.blocks.push(FixBlock { at, func: fi, block: *block });
                self.store_reg(loc(*dst, func), reg);
            }
            Inst::LeaPool { dst, index } => {
                let reg = self.reg_keep(loc(*dst, func));
                let at = self.rip(true, 0x8D, reg);
                self.rels.push(Rel {
                    at,
                    symbol: SLC_POOL_PTRS.into(),
                    kind: PC32,
                    addend: i64::from(*index) * 8 - 4,
                });
                self.store_reg(loc(*dst, func), reg);
            }
            Inst::StorePool { src, index } => {
                let reg = self.reg(loc(*src, func));
                let at = self.rip(true, 0x89, reg);
                self.rels.push(Rel {
                    at,
                    symbol: SLC_POOL_PTRS.into(),
                    kind: PC32,
                    addend: i64::from(*index) * 8 - 4,
                });
            }
        }
    }

    fn call_slc(&mut self, func: &Function, symbol: &str, callee_words: u32) {
        self.safepoint(func.map_id);
        self.spill(func.map_id);
        let bytes = func.frame_words as i32 * 8;
        self.mem(true, 0x8D, RAX, R12, bytes);
        let lea = self.rip(true, 0x8D, RCX);
        self.mem(true, 0x89, RCX, RAX, 0);
        self.mem(true, 0x89, R12, RAX, FRAME_CONT_PREV as i32);
        self.imm(RDX, i64::from(MAP_EMPTY));
        self.mem(true, 0x89, RDX, RAX, FRAME_MAP_FLAGS as i32);
        self.imm(RDX, i64::from(callee_words));
        self.mem(true, 0x89, RDX, RAX, slc_abi::FRAME_FRAME_WORDS as i32);
        self.imm(RDX, 0);
        self.mem(true, 0x89, RDX, RAX, FRAME_HANDLER_PREV as i32);
        self.rr(true, 0x89, RAX, R12);
        self.mem(true, 0x8B, R15, R12, FRAME_CONT_PREV as i32);
        self.jmp_sym(symbol);
        self.patch_at(lea, self.buf.len());
    }

    fn call_alloc(&mut self, func: &Function, words: u32, tag: u16, map_id: u32, dst: Loc) {
        assert!(map_id != 0, "alloc map");
        self.safepoint(func.map_id);
        self.spill(func.map_id);
        self.rr(true, 0x89, R12, RDI);
        self.imm(RSI, i64::from(words));
        self.imm(RDX, i64::from(tag));
        self.imm(RCX, i64::from(map_id));
        self.call_plt(SLC_RT_ALLOC);
        self.mem(true, 0x8B, R13, R12, FRAME_SPILL_VAL as i32);
        self.mem(true, 0x8B, R14, R12, FRAME_SPILL_ENV as i32);
        self.mem(true, 0x8B, RBX, R12, FRAME_SPILL_HANDLERS as i32);
        self.mem(true, 0x8B, R15, R12, FRAME_CONT_PREV as i32);
        self.store_reg(dst, RAX);
    }

    fn ret(&mut self) {
        self.mem(true, 0x8B, RAX, R12, 0);
        self.mem(true, 0x8B, R12, R12, FRAME_CONT_PREV as i32);
        self.mem(true, 0x8B, R15, R12, FRAME_CONT_PREV as i32);
        self.mem(true, 0x8B, R14, R12, FRAME_SPILL_ENV as i32);
        self.mem(true, 0x8B, RBX, R12, FRAME_SPILL_HANDLERS as i32);
        self.buf.push(0xFF);
        self.buf.push(0xE0);
    }

    fn safepoint(&mut self, map_id: u32) {
        assert!(map_id != 0, "safepoint map");
        let fuel = self.rip_ext(0x83, 5);
        self.buf.push(1);
        self.rel_fuel(fuel);
        let jz = self.jcc_hole(Cond::E);
        self.rip_load(RAX, SLC_BYTES_SINCE_GC);
        self.rip_load(RCX, SLC_WATERMARK);
        self.rr(true, 0x39, RCX, RAX);
        let jae = self.jcc_hole(Cond::Ae);
        let jmp = self.jmp_hole();
        let slow = self.buf.len();
        self.patch_at(jz, slow);
        self.patch_at(jae, slow);
        let fuel0 = self.rip_ext(0x83, 7);
        self.buf.push(0);
        self.rel_fuel(fuel0);
        let poll = self.jcc_hole(Cond::Ne);
        self.buf.extend_from_slice(&[0x0F, 0x0B]);
        let call = self.buf.len();
        self.patch_at(poll, call);
        self.spill(map_id);
        self.rr(true, 0x89, R12, RDI);
        self.call_plt(SLC_RT_POLL);
        self.safepoints.push(Safepoint { text_offset: call as u32, map_id });
        self.rr(true, 0x89, RAX, R12);
        self.mem(true, 0x8B, R13, R12, FRAME_SPILL_VAL as i32);
        self.mem(true, 0x8B, R14, R12, FRAME_SPILL_ENV as i32);
        self.mem(true, 0x8B, RBX, R12, FRAME_SPILL_HANDLERS as i32);
        self.mem(true, 0x8B, R15, R12, FRAME_CONT_PREV as i32);
        self.patch_at(jmp, self.buf.len());
    }

    fn spill(&mut self, map_id: u32) {
        self.mem(true, 0x89, R14, R12, FRAME_SPILL_ENV as i32);
        self.mem(true, 0x89, RBX, R12, FRAME_SPILL_HANDLERS as i32);
        self.mem(true, 0x89, R13, R12, FRAME_SPILL_VAL as i32);
        self.set_map(map_id);
    }

    /// Keep the prompt flag in the high half and install `map_id`.
    fn set_map(&mut self, map_id: u32) {
        self.mem(true, 0x8B, RAX, R12, FRAME_MAP_FLAGS as i32);
        self.buf.push(0x48);
        self.buf.push(0xC1);
        self.buf.push(0xE8);
        self.buf.push(32);
        self.buf.push(0x48);
        self.buf.push(0xC1);
        self.buf.push(0xE0);
        self.buf.push(32);
        self.buf.push(0x48);
        self.buf.push(0x81);
        self.buf.push(0xC8);
        self.buf.extend_from_slice(&(map_id as i32).to_le_bytes());
        self.mem(true, 0x89, RAX, R12, FRAME_MAP_FLAGS as i32);
    }

    fn rel_fuel(&mut self, at: usize) {
        self.rels.push(Rel { at, symbol: SLC_FUEL.into(), kind: PC32, addend: -4 });
    }

    fn call_plt(&mut self, symbol: &str) {
        self.buf.push(0xE8);
        let at = self.buf.len();
        self.buf.extend_from_slice(&0i32.to_le_bytes());
        self.rels.push(Rel { at, symbol: symbol.into(), kind: PLT32, addend: -4 });
    }

    fn jmp_sym(&mut self, symbol: &str) {
        self.buf.push(0xE9);
        let at = self.buf.len();
        self.buf.extend_from_slice(&0i32.to_le_bytes());
        self.syms.push(FixSym { at, symbol: symbol.into() });
    }

    fn jmp_block(&mut self, fi: usize, block: usize) {
        self.buf.push(0xE9);
        let at = self.buf.len();
        self.buf.extend_from_slice(&0i32.to_le_bytes());
        self.blocks.push(FixBlock { at, func: fi, block });
    }

    fn jcc(&mut self, fi: usize, cond: Cond, block: usize) {
        let at = self.jcc_hole(cond);
        self.blocks.push(FixBlock { at, func: fi, block });
    }

    fn jcc_hole(&mut self, cond: Cond) -> usize {
        let cc = match cond {
            Cond::E => 0x84,
            Cond::Ne => 0x85,
            Cond::B => 0x82,
            Cond::Ae => 0x83,
            Cond::L => 0x8C,
            Cond::Ge => 0x8D,
            Cond::Le => 0x8E,
            Cond::G => 0x8F,
        };
        self.buf.push(0x0F);
        self.buf.push(cc);
        let at = self.buf.len();
        self.buf.extend_from_slice(&0i32.to_le_bytes());
        at
    }

    fn jmp_hole(&mut self) -> usize {
        self.buf.push(0xE9);
        let at = self.buf.len();
        self.buf.extend_from_slice(&0i32.to_le_bytes());
        at
    }

    fn patch_at(&mut self, at: usize, target: usize) {
        let disp = target as i32 - (at as i32 + 4);
        self.buf[at..at + 4].copy_from_slice(&disp.to_le_bytes());
    }

    fn patch(&mut self) {
        let blocks = std::mem::take(&mut self.blocks);
        for fix in blocks {
            let target = self.block_at[fix.func][fix.block];
            self.patch_at(fix.at, target);
        }
        let syms = std::mem::take(&mut self.syms);
        for fix in syms {
            let target = self
                .func_at
                .iter()
                .find(|(name, _, _)| name == &fix.symbol)
                .unwrap_or_else(|| panic!("no symbol {}", fix.symbol))
                .1;
            self.patch_at(fix.at, target);
        }
    }

    fn reg(&mut self, loc: Loc) -> u8 {
        match loc {
            Loc::Reg(reg) => reg,
            Loc::Mem(base, off) => {
                self.mem(true, 0x8B, RAX, base, off);
                RAX
            }
        }
    }

    fn reg_keep(&mut self, loc: Loc) -> u8 {
        match loc {
            Loc::Reg(reg) => reg,
            Loc::Mem(_, _) => RAX,
        }
    }

    fn store_reg(&mut self, loc: Loc, reg: u8) {
        match loc {
            Loc::Reg(dest) if dest != reg => self.rr(true, 0x89, reg, dest),
            Loc::Reg(_) => {}
            Loc::Mem(base, off) => self.mem(true, 0x89, reg, base, off),
        }
    }

    fn move_loc(&mut self, dst: Loc, src: Loc) {
        match (dst, src) {
            (Loc::Reg(d), Loc::Reg(s)) if d != s => self.rr(true, 0x89, s, d),
            (Loc::Reg(_), Loc::Reg(_)) => {}
            (Loc::Reg(d), Loc::Mem(b, off)) => self.mem(true, 0x8B, d, b, off),
            (Loc::Mem(b, off), Loc::Reg(s)) => self.mem(true, 0x89, s, b, off),
            (Loc::Mem(bd, od), Loc::Mem(bs, os)) => {
                self.mem(true, 0x8B, RAX, bs, os);
                self.mem(true, 0x89, RAX, bd, od);
            }
        }
    }

    fn load(&mut self, dst: Loc, base: Loc, offset: i32, width: u8) {
        let wide = width != 4;
        let reg = self.reg_keep(dst);
        match base {
            Loc::Reg(b) => self.mem(wide, 0x8B, reg, b, offset),
            Loc::Mem(b, off) => {
                self.mem(true, 0x8B, RCX, b, off);
                self.mem(wide, 0x8B, reg, RCX, offset);
            }
        }
        self.store_reg(dst, reg);
    }

    fn store(&mut self, src: Loc, base: Loc, offset: i32, width: u8) {
        let wide = width != 4;
        let reg = match src {
            Loc::Reg(reg) => reg,
            Loc::Mem(b, off) => {
                self.mem(wide, 0x8B, RAX, b, off);
                RAX
            }
        };
        match base {
            Loc::Reg(b) => self.mem(wide, 0x89, reg, b, offset),
            Loc::Mem(b, off) => {
                let addr = if reg == RCX { RDX } else { RCX };
                self.mem(true, 0x8B, addr, b, off);
                self.mem(wide, 0x89, reg, addr, offset);
            }
        }
    }

    fn imm_loc(&mut self, loc: Loc, value: i64) {
        match loc {
            Loc::Reg(reg) => self.imm(reg, value),
            Loc::Mem(base, off) => {
                self.imm(RAX, value);
                self.mem(true, 0x89, RAX, base, off);
            }
        }
    }

    fn cmp_imm(&mut self, reg: u8, imm: i64) {
        if imm == i64::from(imm as i8) {
            self.rex(true, 0, reg);
            self.buf.push(0x83);
            self.buf.push(0xF8 | (reg & 7));
            self.buf.push(imm as u8);
        } else if imm == i64::from(imm as i32) {
            self.rex(true, 0, reg);
            self.buf.push(0x81);
            self.buf.push(0xF8 | (reg & 7));
            self.buf.extend_from_slice(&(imm as i32).to_le_bytes());
        } else {
            let tmp = if reg == RAX { RCX } else { RAX };
            self.imm(tmp, imm);
            self.rr(true, 0x39, tmp, reg);
        }
    }

    fn imm(&mut self, reg: u8, value: i64) {
        if value == i64::from(value as i32) {
            self.rex(true, 0, reg);
            self.buf.push(0xC7);
            self.buf.push(0xC0 | (reg & 7));
            self.buf.extend_from_slice(&(value as i32).to_le_bytes());
        } else {
            self.rex(true, 0, reg);
            self.buf.push(0xB8 | (reg & 7));
            self.buf.extend_from_slice(&value.to_le_bytes());
        }
    }

    fn zero(&mut self, reg: u8) {
        self.rex(false, reg, reg);
        self.buf.push(0x31);
        self.buf.push(0xC0 | ((reg & 7) << 3) | (reg & 7));
    }

    fn push(&mut self, reg: u8) {
        if reg >= 8 {
            self.buf.push(0x41);
        }
        self.buf.push(0x50 | (reg & 7));
    }

    fn pop(&mut self, reg: u8) {
        if reg >= 8 {
            self.buf.push(0x41);
        }
        self.buf.push(0x58 | (reg & 7));
    }

    fn rr(&mut self, w: bool, opcode: u8, reg: u8, rm: u8) {
        self.rex(w, reg, rm);
        self.buf.push(opcode);
        self.buf.push(0xC0 | ((reg & 7) << 3) | (rm & 7));
    }

    fn mem(&mut self, w: bool, opcode: u8, reg: u8, base: u8, disp: i32) {
        self.rex(w, reg, base);
        self.buf.push(opcode);
        let low = base & 7;
        let sib = low == 4;
        let (mode, dlen) = if disp == 0 && low != 5 {
            (0, 0)
        } else if (-128..128).contains(&disp) {
            (1, 1)
        } else {
            (2, 4)
        };
        self.buf.push((mode << 6) | ((reg & 7) << 3) | if sib { 4 } else { low });
        if sib {
            self.buf.push((4 << 3) | low);
        }
        if dlen == 1 {
            self.buf.push(disp as u8);
        }
        if dlen == 4 {
            self.buf.extend_from_slice(&disp.to_le_bytes());
        }
    }

    fn rip(&mut self, w: bool, opcode: u8, reg: u8) -> usize {
        self.rex(w, reg, 0);
        self.buf.push(opcode);
        self.buf.push(((reg & 7) << 3) | 5);
        let at = self.buf.len();
        self.buf.extend_from_slice(&0i32.to_le_bytes());
        at
    }

    fn rip_ext(&mut self, opcode: u8, ext: u8) -> usize {
        self.buf.push(0x48);
        self.buf.push(opcode);
        self.buf.push((ext << 3) | 5);
        let at = self.buf.len();
        self.buf.extend_from_slice(&0i32.to_le_bytes());
        at
    }

    fn rip_load(&mut self, reg: u8, symbol: &str) {
        let at = self.rip(true, 0x8B, reg);
        self.rels.push(Rel { at, symbol: symbol.into(), kind: PC32, addend: -4 });
    }

    fn rex(&mut self, w: bool, reg: u8, base: u8) {
        let bits = u8::from(w) << 3 | ((reg >> 3) & 1) << 2 | ((base >> 3) & 1);
        if bits != 0 {
            self.buf.push(0x40 | bits);
        }
    }
}

#[derive(Clone, Copy)]
enum Loc {
    Reg(u8),
    Mem(u8, i32),
}

fn loc(dest: Dest, func: &Function) -> Loc {
    match dest {
        Dest::Val => Loc::Reg(R13),
        Dest::Env => Loc::Reg(R14),
        Dest::Frame => Loc::Reg(R12),
        Dest::V(index) => {
            if (index as usize) < CALLER_SAVED.len() {
                Loc::Reg(CALLER_SAVED[index as usize])
            } else {
                let slot = u32::from(func.spill_base) + u32::from(index - 9);
                Loc::Mem(R12, FRAME_SLOT0 as i32 + slot as i32 * 8)
            }
        }
    }
}

fn elf(
    module: &Module,
    text: &[u8],
    safepoints: &[Safepoint],
    rels: &[Rel],
    funcs: &[(String, usize, usize)],
) -> Vec<u8> {
    let safe = safepoint_bytes(safepoints);
    let maps = map_bytes(&module.maps);
    let pool_words = if module.pool_len == 0 { 1 } else { module.pool_len };
    let pool = vec![0u8; pool_words as usize * 8];
    let scalars = vec![0u8];
    let labels = label_bytes(&module.labels);

    let mut names = StrTab::new();
    let mut defined: Vec<Sym> = Vec::new();
    // Locals first. Only the C entry is global; `main` would clash with the driver's `main`.
    for (name, start, size) in funcs.iter().filter(|(name, _, _)| name != SLC_PROGRAM_ENTRY) {
        defined.push(Sym {
            name: names.add(name),
            info: 2,
            shndx: 1,
            value: *start as u64,
            size: *size as u64,
        });
    }
    let first_global = 1 + defined.len() as u32;
    for (name, start, size) in funcs.iter().filter(|(name, _, _)| name == SLC_PROGRAM_ENTRY) {
        defined.push(Sym {
            name: names.add(name),
            info: (1 << 4) | 2,
            shndx: 1,
            value: *start as u64,
            size: *size as u64,
        });
    }
    defined.push(Sym {
        name: names.add(SLC_POOL_PTRS),
        info: (1 << 4) | 1,
        shndx: 4,
        value: 0,
        size: pool.len() as u64,
    });
    let mut undef_names = Vec::new();
    for rel in rels {
        if !undef_names.iter().any(|n| n == &rel.symbol)
            && !funcs.iter().any(|(n, _, _)| n == &rel.symbol)
            && rel.symbol != SLC_POOL_PTRS
        {
            undef_names.push(rel.symbol.clone());
        }
    }
    let mut symbols = vec![Sym { name: 0, info: 0, shndx: 0, value: 0, size: 0 }];
    let defined_count = defined.len();
    symbols.extend(defined);
    let mut undef_at = std::collections::HashMap::new();
    for name in &undef_names {
        undef_at.insert(name.clone(), symbols.len());
        symbols.push(Sym { name: names.add(name), info: 1 << 4, shndx: 0, value: 0, size: 0 });
    }
    let pool_sym = 1 + defined_count - 1;
    let mut rela = Vec::new();
    for rel in rels {
        let sym = if rel.symbol == SLC_POOL_PTRS { pool_sym } else { undef_at[&rel.symbol] };
        rela.extend_from_slice(&(rel.at as u64).to_le_bytes());
        rela.extend_from_slice(&(((sym as u64) << 32) | u64::from(rel.kind)).to_le_bytes());
        rela.extend_from_slice(&rel.addend.to_le_bytes());
    }

    let mut shstr = StrTab::new();
    let sec_names = [
        shstr.add(""),
        shstr.add(SLC_TEXT),
        shstr.add(slc_abi::SLC_SAFEPOINTS),
        shstr.add(slc_abi::SLC_MAPS),
        shstr.add(SLC_POOL_PTRS),
        shstr.add(slc_abi::SLC_POOL_SCALARS),
        shstr.add(slc_abi::SLC_LABELS),
        shstr.add(".rela.slc_text"),
        shstr.add(".shstrtab"),
        shstr.add(".strtab"),
        shstr.add(".symtab"),
    ];

    let mut sym_bytes = Vec::new();
    for sym in &symbols {
        sym.write(&mut sym_bytes);
    }

    let mut off = 64u64;
    let mut layout = Vec::new();
    let data = [
        text,
        safe.as_slice(),
        maps.as_slice(),
        pool.as_slice(),
        scalars.as_slice(),
        labels.as_slice(),
        rela.as_slice(),
        shstr.bytes(),
        names.bytes(),
        sym_bytes.as_slice(),
    ];
    let aligns = [16u64, 4, 4, 8, 1, 1, 8, 1, 1, 8];
    for (bytes, align) in data.iter().zip(aligns) {
        off = (off + align - 1) & !(align - 1);
        layout.push(off);
        off += bytes.len() as u64;
    }
    off = (off + 7) & !7;
    let shoff = off;

    let mut out = vec![0u8; 64];
    out[0..4].copy_from_slice(&[0x7F, b'E', b'L', b'F']);
    out[4] = 2;
    out[5] = 1;
    out[6] = 1;
    put_u16(&mut out, 16, 1);
    put_u16(&mut out, 18, 62);
    put_u32(&mut out, 20, 1);
    put_u64(&mut out, 32, 0);
    put_u64(&mut out, 40, shoff);
    put_u32(&mut out, 48, 0);
    put_u16(&mut out, 52, 64);
    put_u16(&mut out, 54, 0);
    put_u16(&mut out, 56, 0);
    put_u16(&mut out, 58, 64);
    put_u16(&mut out, 60, 11);
    put_u16(&mut out, 62, 8);

    // `.rela` carries `SHF_INFO_LINK`; the info field is the text section.
    let flags = [6u64, 2, 2, 3, 2, 2, 0x40, 0, 0, 0];
    let kinds = [1u32, 1, 1, 1, 1, 1, 4, 3, 3, 2];
    let ents = [0u64, 0, 0, 0, 0, 0, 24, 0, 0, 24];
    let links = [0u32, 0, 0, 0, 0, 0, 10, 0, 0, 9];
    let infos = [0u32, 0, 0, 0, 0, 0, 1, 0, 0, first_global];
    for (i, bytes) in data.iter().enumerate() {
        let end = layout[i] + bytes.len() as u64;
        if out.len() < end as usize {
            out.resize(end as usize, 0);
        }
        out[layout[i] as usize..end as usize].copy_from_slice(bytes);
    }
    out.resize(shoff as usize, 0);
    let mut sh = Vec::new();
    shdr(&mut sh, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
    for i in 0..10 {
        shdr(
            &mut sh,
            sec_names[i + 1],
            kinds[i],
            flags[i],
            0,
            layout[i],
            data[i].len() as u64,
            links[i],
            infos[i],
            aligns[i],
            ents[i],
        );
    }
    out.extend_from_slice(&sh);
    out
}

struct Sym {
    name: u32,
    info: u8,
    shndx: u16,
    value: u64,
    size: u64,
}

impl Sym {
    fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.name.to_le_bytes());
        out.push(self.info);
        out.push(0);
        out.extend_from_slice(&self.shndx.to_le_bytes());
        out.extend_from_slice(&self.value.to_le_bytes());
        out.extend_from_slice(&self.size.to_le_bytes());
    }
}

struct StrTab {
    buf: Vec<u8>,
}

impl StrTab {
    fn new() -> Self {
        Self { buf: vec![0] }
    }
    fn add(&mut self, text: &str) -> u32 {
        if text.is_empty() {
            return 0;
        }
        let at = self.buf.len() as u32;
        self.buf.extend_from_slice(text.as_bytes());
        self.buf.push(0);
        at
    }
    fn bytes(&self) -> &[u8] {
        &self.buf
    }
}

#[allow(clippy::too_many_arguments)]
fn shdr(
    out: &mut Vec<u8>,
    name: u32,
    kind: u32,
    flags: u64,
    addr: u64,
    offset: u64,
    size: u64,
    link: u32,
    info: u32,
    align: u64,
    entsize: u64,
) {
    out.extend_from_slice(&name.to_le_bytes());
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&addr.to_le_bytes());
    out.extend_from_slice(&offset.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&link.to_le_bytes());
    out.extend_from_slice(&info.to_le_bytes());
    out.extend_from_slice(&align.to_le_bytes());
    out.extend_from_slice(&entsize.to_le_bytes());
}

fn put_u16(out: &mut [u8], at: usize, value: u16) {
    out[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn put_u32(out: &mut [u8], at: usize, value: u32) {
    out[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put_u64(out: &mut [u8], at: usize, value: u64) {
    out[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

fn safepoint_bytes(sps: &[Safepoint]) -> Vec<u8> {
    let mut out = Vec::new();
    for sp in sps {
        out.extend_from_slice(&sp.text_offset.to_le_bytes());
        out.extend_from_slice(&sp.map_id.to_le_bytes());
    }
    if out.is_empty() {
        out.push(0);
    }
    out
}

fn map_bytes(maps: &[crate::MapRecord]) -> Vec<u8> {
    if maps.is_empty() {
        return vec![0];
    }
    let mut out = Vec::new();
    for map in maps {
        out.extend_from_slice(&map.map_id.to_le_bytes());
        out.extend_from_slice(&map.frame_words.to_le_bytes());
        out.push(u8::from(map.val_is_pointer));
        out.push(0);
        out.extend_from_slice(&(map.slots.len() as u16).to_le_bytes());
        for slot in &map.slots {
            out.extend_from_slice(&slot.to_le_bytes());
        }
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }
    out
}

fn label_bytes(labels: &[String]) -> Vec<u8> {
    if labels.is_empty() {
        return vec![0];
    }
    let mut out = Vec::new();
    for label in labels {
        out.extend_from_slice(label.as_bytes());
        out.push(0);
    }
    out
}
