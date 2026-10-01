//! x86-64 System V encoder. Virtual temps stay in caller-saved registers or frame slots.
//! Protocol registers are never assigned a temp.

use crate::{Cond, Dest, Function, Inst, Module};
use slc_abi::{
    FRAME_CONT_PREV, FRAME_HANDLER_PREV, FRAME_MAP_FLAGS, FRAME_SLOT0, FRAME_SPILL_ENV,
    FRAME_SPILL_HANDLERS, FRAME_SPILL_VAL, MAP_EMPTY, SLC_FUEL, SLC_POOL_PTRS, SLC_PROGRAM_ENTRY,
    SLC_RT_ALLOC, SLC_RT_FAIL, SLC_RT_POLL, SLC_TEXT, Safepoint,
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
            // The IO prompt is the handler chain. Zeroing rbx would hide `write_line`.
            self.rr(true, 0x89, R12, RBX);
            self.set_map(func.map_id);
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
            // A returning main drops its frame first, so r12 is the IO prompt.
            let flag = self.rip_ext(0x83, 7);
            self.buf.push(0);
            self.rels.push(Rel {
                at: flag,
                symbol: "slc_sweep_on_exit".into(),
                kind: PC32,
                addend: -5,
            });
            let skip = self.jcc_hole(Cond::E);
            self.rr(true, 0x89, R12, RDI);
            self.call_plt("slc_rt_sweep");
            self.patch_at(skip, self.buf.len());
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
            Inst::CallSlc { symbol, callee_frame_words, arg_is_pointer } => {
                self.call_slc(func, symbol, *callee_frame_words, *arg_is_pointer)
            }
            Inst::Tail { symbol, arg_is_pointer, .. } => {
                // The safepoint sees the outgoing word. A scalar must not sit in a traced `r13`.
                self.park_scalar(func, *arg_is_pointer);
                self.safepoint(func.map_id);
                self.unpark_scalar(func, *arg_is_pointer);
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
            Inst::LeaSym { dst, symbol } => {
                let reg = self.reg_keep(loc(*dst, func));
                let at = self.rip(true, 0x8D, reg);
                self.syms.push(FixSym { at, symbol: symbol.clone() });
                self.store_reg(loc(*dst, func), reg);
            }
            Inst::SymWords { .. } | Inst::SymMap { .. } => {
                panic!("symbol immediate was not patched");
            }
            Inst::CmpRR { left, right, cond, target } => {
                let lreg = self.reg(loc(*left, func));
                let rreg = if lreg == RAX { RCX } else { RAX };
                match loc(*right, func) {
                    Loc::Reg(reg) if reg != rreg => self.rr(true, 0x89, reg, rreg),
                    Loc::Reg(_) => {}
                    Loc::Mem(base, off) => self.mem(true, 0x8B, rreg, base, off),
                }
                self.rr(true, 0x39, rreg, lreg);
                self.jcc(fi, *cond, *target);
            }
            Inst::Capture { dst } => {
                self.safepoint(func.map_id);
                self.spill(func.map_id);
                self.rr(true, 0x89, R12, RDI);
                self.call_plt("slc_rt_capture");
                self.reload_frame();
                self.store_reg(loc(*dst, func), RAX);
            }
            Inst::Invoke { image } => {
                self.safepoint(func.map_id);
                self.spill(func.map_id);
                // Reload after the poll. A park slot is not a root, so it would keep a moved address.
                self.load_dest(*image, func, 8);
                self.rr(true, 0x89, R12, RDI);
                self.rr(true, 0x89, 8, RSI);
                self.call_plt("slc_rt_invoke");
                self.rr(true, 0x89, RAX, R12);
                self.mem(true, 0x8B, R15, R12, FRAME_CONT_PREV as i32);
                self.ret();
            }
            Inst::Resume { image, tail } => {
                if *tail {
                    // Pop only the body. The image is loaded first; the parent park is a different slot.
                    self.load_dest(*image, func, 8);
                    self.mem(true, 0x8B, R12, R12, FRAME_CONT_PREV as i32);
                    self.mem(true, 0x8B, R15, R12, FRAME_CONT_PREV as i32);
                } else {
                    self.safepoint(func.map_id);
                    self.spill(func.map_id);
                    self.load_dest(*image, func, 8);
                }
                self.rr(true, 0x89, R12, RDI);
                self.rr(true, 0x89, 8, RSI);
                self.call_plt("slc_rt_resume");
                self.rr(true, 0x89, RAX, R12);
                self.mem(true, 0x8B, R15, R12, FRAME_CONT_PREV as i32);
                self.ret();
            }
            Inst::InstallPrompt { clauses, ret_closure, thunk, done, prompt_map } => {
                self.install_prompt(fi, func, *clauses, *ret_closure, *thunk, *done, *prompt_map);
            }
            Inst::Perform { op, tail, arg_is_pointer, after, apply_map, cont_map } => {
                self.perform(fi, func, *op, *tail, *arg_is_pointer, *after, *apply_map, *cont_map);
            }
            Inst::Force { tail, slot } => self.force(func, *tail, *slot),
            Inst::Adapt { slot, map_id } => self.adapt(func, *slot, *map_id),
            Inst::CallClosure { closure, tail, arg_is_pointer } => {
                self.call_closure(func, *closure, *tail, *arg_is_pointer);
            }
            Inst::Activate { consumer, tail, arg_is_pointer } => {
                self.activate(func, *consumer, *tail, *arg_is_pointer);
            }
            Inst::CallRt { symbol, arg, noreturn } => {
                self.safepoint(func.map_id);
                self.spill(func.map_id);
                self.rr(true, 0x89, R12, RDI);
                match arg {
                    crate::RtArg::Val => self.rr(true, 0x89, R13, RSI),
                    crate::RtArg::Env => self.rr(true, 0x89, R14, RSI),
                }
                self.call_plt(symbol);
                if !noreturn {
                    self.reload_frame();
                }
            }
        }
    }

    fn reload_frame(&mut self) {
        self.mem(true, 0x8B, R13, R12, FRAME_SPILL_VAL as i32);
        self.mem(true, 0x8B, R14, R12, FRAME_SPILL_ENV as i32);
        self.mem(true, 0x8B, RBX, R12, FRAME_SPILL_HANDLERS as i32);
        self.mem(true, 0x8B, R15, R12, FRAME_CONT_PREV as i32);
    }

    fn shl3(&mut self, reg: u8) {
        self.rex(true, 0, reg);
        self.buf.push(0xC1);
        self.buf.push(0xE0 | (reg & 7));
        self.buf.push(3);
    }

    /// Tag in `rax`. `0xffff` when `reg` is not a heap object, so a scalar is not a header.
    fn word_tag(&mut self, reg: u8) {
        if reg != RDI {
            self.rr(true, 0x89, reg, RDI);
        }
        self.call_plt("slc_rt_word_tag");
    }

    fn and_imm(&mut self, reg: u8, imm: u32) {
        self.rex(true, 0, reg);
        self.buf.push(0x81);
        self.buf.push(0xE0 | (reg & 7));
        self.buf.extend_from_slice(&imm.to_le_bytes());
    }

    fn jmp_reg(&mut self, reg: u8) {
        self.rex(false, 0, reg);
        self.buf.push(0xFF);
        self.buf.push(0xE0 | (reg & 7));
    }

    /// `rax` = `r12 + [r12+48] * 8`, the frame above the current one.
    fn lea_above(&mut self) {
        self.mem(true, 0x8B, RAX, R12, slc_abi::FRAME_FRAME_WORDS as i32);
        self.shl3(RAX);
        self.rr(true, 0x01, R12, RAX);
    }

    fn store_imm(&mut self, base: u8, offset: i32, value: i64) {
        self.imm(RCX, value);
        self.mem(true, 0x89, RCX, base, offset);
    }

    // One operand per `InstallPrompt` field. A struct would have a single caller.
    #[allow(clippy::too_many_arguments)]
    fn install_prompt(
        &mut self,
        fi: usize,
        func: &Function,
        clauses: Dest,
        ret_closure: Dest,
        thunk: Dest,
        done: usize,
        prompt_map: u32,
    ) {
        // Operands are slots. A collecting `fresh_prompt_id` updates those slots;
        // a copy in the untraced park would keep the old address.
        self.safepoint(func.map_id);
        self.spill(func.map_id);
        self.rr(true, 0x89, R12, RDI);
        self.call_plt("slc_rt_fresh_prompt_id");
        self.rr(true, 0x89, RAX, 8); // r8 = id
        self.load_dest(clauses, func, 9);
        self.load_dest(ret_closure, func, 10);
        self.load_dest(thunk, func, 11);
        self.lea_above();
        let ret_at = self.rip(true, 0x8D, RCX);
        self.blocks.push(FixBlock { at: ret_at, func: fi, block: done });
        // The trampoline is not `done`. `done` is offset 0, the continuation of `do`.
        // CallSlc of the thunk writes the trampoline into the thunk frame.
        self.mem(true, 0x89, RCX, RAX, 0);
        self.mem(true, 0x89, R12, RAX, FRAME_CONT_PREV as i32);
        self.store_imm(RAX, FRAME_SPILL_ENV as i32, 0);
        self.mem(true, 0x89, RBX, RAX, FRAME_SPILL_HANDLERS as i32);
        self.store_imm(RAX, FRAME_SPILL_VAL as i32, 0);
        self.imm(RCX, i64::from(prompt_map));
        self.mem(true, 0x89, RCX, RAX, FRAME_MAP_FLAGS as i32);
        // PROMPT flag in the high half. set_map on this frame preserves it once it is set.
        self.mem(true, 0x8B, RCX, RAX, FRAME_MAP_FLAGS as i32);
        self.imm(RDX, i64::from(slc_abi::FRAME_FLAG_PROMPT) << 32);
        self.rr(true, 0x09, RDX, RCX); // or rcx, rdx
        self.mem(true, 0x89, RCX, RAX, FRAME_MAP_FLAGS as i32);
        self.store_imm(RAX, slc_abi::FRAME_FRAME_WORDS as i32, 13);
        self.mem(true, 0x89, 8, RAX, slc_abi::FRAME_PROMPT_ID as i32); // r8 id
        self.mem(true, 0x89, RBX, RAX, FRAME_HANDLER_PREV as i32);
        self.mem(true, 0x89, 9, RAX, FRAME_SLOT0 as i32);
        self.mem(true, 0x89, 10, RAX, FRAME_SLOT0 as i32 + 8);
        self.store_imm(RAX, FRAME_SLOT0 as i32 + 16, 0);
        self.rr(true, 0x89, RAX, R12);
        self.mem(true, 0x8B, R15, R12, FRAME_CONT_PREV as i32);
        self.rr(true, 0x89, R12, RBX);
        // Thunk closure is in r11. Arg is unit.
        self.zero(R13);
        self.dyn_call_closure(11, true);
        // Thunk `Ret` lands here: `r12` is the prompt, `r13` is the body result.
        // The return closure runs, then the prompt's own `Ret` uses offset 0 (`done`).
        self.mem(true, 0x8B, 11, R12, FRAME_SLOT0 as i32 + 8);
        self.dyn_call_closure(11, true);
        self.ret();
    }

    fn load_dest(&mut self, dest: Dest, func: &Function, reg: u8) {
        match loc(dest, func) {
            Loc::Reg(src) if src != reg => self.rr(true, 0x89, src, reg),
            Loc::Reg(_) => {}
            Loc::Mem(base, off) => self.mem(true, 0x8B, reg, base, off),
        }
    }

    // One operand per `Perform` field. A struct would have a single caller.
    #[allow(clippy::too_many_arguments)]
    fn perform(
        &mut self,
        fi: usize,
        func: &Function,
        op: u32,
        tail: bool,
        arg_is_pointer: bool,
        after: usize,
        apply_map: u32,
        cont_map: u32,
    ) {
        if !tail {
            self.lea_above();
            let at = self.rip(true, 0x8D, RCX);
            self.blocks.push(FixBlock { at, func: fi, block: after });
            self.mem(true, 0x89, RCX, RAX, 0);
            self.mem(true, 0x89, R12, RAX, FRAME_CONT_PREV as i32);
            self.mem(true, 0x89, R14, RAX, FRAME_SPILL_ENV as i32);
            self.mem(true, 0x89, RBX, RAX, FRAME_SPILL_HANDLERS as i32);
            self.mem(true, 0x89, R13, RAX, FRAME_SPILL_VAL as i32);
            // Nine words: header only. A pointer payload is the spilled VAL, not a slot.
            let map = if arg_is_pointer { cont_map } else { MAP_EMPTY };
            self.store_imm(RAX, FRAME_MAP_FLAGS as i32, i64::from(map));
            self.store_imm(RAX, slc_abi::FRAME_FRAME_WORDS as i32, 9);
            self.store_imm(RAX, slc_abi::FRAME_PROMPT_ID as i32, 0);
            self.store_imm(RAX, FRAME_HANDLER_PREV as i32, 0);
            self.rr(true, 0x89, RAX, R12);
            self.mem(true, 0x8B, R15, R12, FRAME_CONT_PREV as i32);
        }
        let map_now = if tail {
            func.map_id
        } else if arg_is_pointer {
            cont_map
        } else {
            MAP_EMPTY
        };
        self.park_scalar(func, arg_is_pointer || !tail);
        self.safepoint(map_now);
        self.spill(map_now);
        if tail {
            self.unpark_scalar(func, arg_is_pointer);
        }
        self.rr(true, 0x89, R12, RDI);
        self.imm(RSI, i64::from(op));
        self.call_plt("slc_rt_split");
        self.rr(true, 0x89, RAX, R12);
        // Resume and clause are not in a frame yet. Hold them in callee-saved
        // registers until ApplyTo's map is stored, and spill the resume as ENV
        // (that word is always a root).
        self.rip_load(RBX, "slc_split_handlers");
        self.rip_load(R14, "slc_split_resume");
        self.rip_load(R15, "slc_split_clause");
        self.lea_above();
        self.store_imm(RAX, 0, 0);
        self.mem(true, 0x89, R12, RAX, FRAME_CONT_PREV as i32);
        self.mem(true, 0x89, R14, RAX, FRAME_SPILL_ENV as i32);
        self.mem(true, 0x89, RBX, RAX, FRAME_SPILL_HANDLERS as i32);
        self.mem(true, 0x89, R13, RAX, FRAME_SPILL_VAL as i32);
        self.store_imm(RAX, FRAME_MAP_FLAGS as i32, i64::from(apply_map));
        self.store_imm(RAX, slc_abi::FRAME_FRAME_WORDS as i32, 11);
        self.store_imm(RAX, slc_abi::FRAME_PROMPT_ID as i32, 0);
        self.store_imm(RAX, FRAME_HANDLER_PREV as i32, 0);
        self.mem(true, 0x89, R14, RAX, FRAME_SLOT0 as i32);
        self.mem(true, 0x89, R15, RAX, FRAME_SLOT0 as i32 + 8);
        self.rr(true, 0x89, RAX, R12);
        self.mem(true, 0x8B, R15, R12, FRAME_CONT_PREV as i32);
        self.safepoint(apply_map);
        self.spill(apply_map);
        // The map names the slots. Drop the statics so the resume is not immortal.
        self.zero(RAX);
        self.rip_store(RAX, "slc_split_resume");
        self.rip_store(RAX, "slc_split_clause");
        self.rip_store(RAX, "slc_split_handlers");
        self.mem(true, 0x8B, 11, R12, FRAME_SLOT0 as i32 + 8);
        self.dyn_call_closure(11, true);
        // r13 = inner closure, r12 = ApplyTo.
        self.rr(true, 0x89, R12, RDI);
        self.mem(true, 0x8B, RSI, R12, FRAME_SLOT0 as i32);
        self.call_plt("slc_rt_resume_cont");
        self.rr(true, 0x89, RAX, 10); // r10 = continuation of do
        self.mem(true, 0x8B, 9, R12, FRAME_SLOT0 as i32); // r9 = resume
        self.mem(true, 0x8B, R12, R12, FRAME_CONT_PREV as i32);
        self.mem(true, 0x8B, R15, R12, FRAME_CONT_PREV as i32);
        self.lea_above();
        self.mem(true, 0x89, 10, RAX, 0);
        self.mem(true, 0x89, R12, RAX, FRAME_CONT_PREV as i32);
        self.store_imm(RAX, FRAME_SPILL_ENV as i32, 0);
        self.mem(true, 0x89, RBX, RAX, FRAME_SPILL_HANDLERS as i32);
        self.mem(true, 0x89, 9, RAX, FRAME_SPILL_VAL as i32);
        self.mem(true, 0x8B, 11, R13, slc_abi::CLOSURE_CODE as i32);
        self.mem(true, 0x8B, R14, R13, slc_abi::CLOSURE_ENV as i32);
        self.mem(true, 0x8B, 8, R13, slc_abi::CLOSURE_FRAME_WORDS as i32);
        self.mem(true, 0x8B, RCX, R13, 40);
        self.mem(true, 0x89, RCX, RAX, FRAME_MAP_FLAGS as i32);
        self.mem(true, 0x89, 8, RAX, slc_abi::FRAME_FRAME_WORDS as i32);
        self.store_imm(RAX, slc_abi::FRAME_PROMPT_ID as i32, 0);
        self.store_imm(RAX, FRAME_HANDLER_PREV as i32, 0);
        self.rr(true, 0x89, 9, R13);
        self.rr(true, 0x89, RAX, R12);
        self.mem(true, 0x8B, R15, R12, FRAME_CONT_PREV as i32);
        self.mem(true, 0x89, R14, R12, FRAME_SPILL_ENV as i32);
        self.mem(true, 0x89, RBX, R12, FRAME_SPILL_HANDLERS as i32);
        self.mem(true, 0x89, R13, R12, FRAME_SPILL_VAL as i32);
        self.jmp_reg(11);
    }

    /// Call the closure in `reg`. Argument is `r13`. `keep_env` loads ENV from the closure.
    fn dyn_call_closure(&mut self, reg: u8, keep_env: bool) {
        self.mem(true, 0x8B, 10, reg, slc_abi::CLOSURE_ENV as i32);
        self.mem(true, 0x8B, 9, reg, slc_abi::CLOSURE_FRAME_WORDS as i32);
        self.mem(true, 0x8B, 11, reg, slc_abi::CLOSURE_CODE as i32);
        if keep_env {
            self.rr(true, 0x89, 10, R14);
        }
        self.lea_above();
        let ret_at = self.rip(true, 0x8D, RCX);
        self.mem(true, 0x89, RCX, RAX, 0);
        self.mem(true, 0x89, R12, RAX, FRAME_CONT_PREV as i32);
        self.store_imm(RAX, FRAME_SPILL_ENV as i32, 0);
        self.store_imm(RAX, FRAME_SPILL_HANDLERS as i32, 0);
        self.store_imm(RAX, FRAME_SPILL_VAL as i32, 0);
        self.store_imm(RAX, FRAME_MAP_FLAGS as i32, i64::from(MAP_EMPTY));
        self.mem(true, 0x89, 9, RAX, slc_abi::FRAME_FRAME_WORDS as i32);
        self.store_imm(RAX, FRAME_HANDLER_PREV as i32, 0);
        self.rr(true, 0x89, RAX, R12);
        self.mem(true, 0x8B, R15, R12, FRAME_CONT_PREV as i32);
        self.jmp_reg(11);
        self.patch_at(ret_at, self.buf.len());
    }

    fn call_closure(&mut self, func: &Function, closure: Dest, tail: bool, arg_is_pointer: bool) {
        self.park_scalar(func, arg_is_pointer);
        self.safepoint(func.map_id);
        self.spill(func.map_id);
        self.unpark_scalar(func, arg_is_pointer);
        self.load_dest(closure, func, 11);
        if tail {
            self.tail_jump_closure(11);
        } else {
            self.dyn_call_closure(11, true);
            self.reload_frame();
        }
    }

    /// `reg` is a closure. Code moves through `r15` so `grow` cannot drop it.
    fn tail_jump_closure(&mut self, reg: u8) {
        self.mem(true, 0x8B, R14, reg, slc_abi::CLOSURE_ENV as i32);
        self.mem(true, 0x8B, RSI, reg, slc_abi::CLOSURE_FRAME_WORDS as i32);
        self.mem(true, 0x8B, R15, reg, slc_abi::CLOSURE_CODE as i32);
        self.rr(true, 0x89, R12, RDI);
        self.call_plt("slc_rt_grow_frame");
        self.rr(true, 0x89, RAX, R12);
        // The segment may have moved. The spill was rebased; `r14` is a heap env.
        self.mem(true, 0x8B, RBX, R12, FRAME_SPILL_HANDLERS as i32);
        self.rr(true, 0x89, R15, 11);
        self.mem(true, 0x8B, R15, R12, FRAME_CONT_PREV as i32);
        self.jmp_reg(11);
    }

    fn force(&mut self, func: &Function, _tail: bool, slot: u16) {
        let off = FRAME_SLOT0 as i32 + i32::from(slot) * 8;
        // Later iterations see the adapter's or the delay body's result, not the slot.
        self.mem(true, 0x8B, R13, R12, off);
        let again = self.buf.len();
        self.word_tag(R13);
        self.cmp_imm(RAX, i64::from(slc_abi::TAG_ADAPTED));
        let not_adapted = self.jcc_hole(Cond::Ne);
        // The slot keeps the object across the poll. Caller-saved temps do not.
        self.mem(true, 0x89, R13, R12, off);
        self.safepoint(func.map_id);
        self.spill(func.map_id);
        self.mem(true, 0x8B, RAX, R12, off);
        self.mem(true, 0x8B, 11, RAX, slc_abi::CLOSURE_CODE as i32);
        self.mem(true, 0x8B, R13, RAX, slc_abi::CLOSURE_ENV as i32);
        self.dyn_call_closure(11, true);
        // `Ret` left the adapter result in `r13`. The spill still holds the Adapted.
        self.jmp_hole_to(again);
        self.patch_at(not_adapted, self.buf.len());
        self.word_tag(R13);
        self.cmp_imm(RAX, i64::from(slc_abi::TAG_DELAY));
        let done = self.jcc_hole(Cond::Ne);
        self.mem(true, 0x89, R13, R12, off);
        self.safepoint(func.map_id);
        self.spill(func.map_id);
        self.mem(true, 0x8B, RAX, R12, off);
        self.mem(true, 0x8B, R14, RAX, slc_abi::CLOSURE_ENV as i32);
        self.mem(true, 0x8B, R15, RAX, slc_abi::CLOSURE_CODE as i32);
        self.mem(true, 0x8B, 9, RAX, 32);
        self.rip_load(R13, "slc_rt_unit");
        // Tail and value position both return here. A pure tail would skip a
        // body that yields another delay or an `Adapted`. `rbx` is copied into
        // the new frame; this path does not move the segment.
        self.lea_above();
        let ret_at = self.rip(true, 0x8D, RCX);
        self.mem(true, 0x89, RCX, RAX, 0);
        self.mem(true, 0x89, R12, RAX, FRAME_CONT_PREV as i32);
        self.store_imm(RAX, FRAME_SPILL_ENV as i32, 0);
        self.mem(true, 0x89, RBX, RAX, FRAME_SPILL_HANDLERS as i32);
        self.store_imm(RAX, FRAME_SPILL_VAL as i32, 0);
        self.store_imm(RAX, FRAME_MAP_FLAGS as i32, i64::from(MAP_EMPTY));
        self.mem(true, 0x89, 9, RAX, slc_abi::FRAME_FRAME_WORDS as i32);
        self.store_imm(RAX, FRAME_HANDLER_PREV as i32, 0);
        self.rr(true, 0x89, RAX, R12);
        self.rr(true, 0x89, R15, 11);
        self.mem(true, 0x8B, R15, R12, FRAME_CONT_PREV as i32);
        self.mem(true, 0x89, R14, R12, FRAME_SPILL_ENV as i32);
        self.jmp_reg(11);
        self.patch_at(ret_at, self.buf.len());
        // `Ret` restored this frame and left the body result in `r13`.
        // Reloading the spill would put the delay back.
        self.jmp_hole_to(again);
        self.patch_at(done, self.buf.len());
    }

    fn jmp_hole_to(&mut self, target: usize) {
        let at = self.jmp_hole();
        self.patch_at(at, target);
    }

    fn adapt(&mut self, func: &Function, slot: u16, map_id: u32) {
        let off = FRAME_SLOT0 as i32 + i32::from(slot) * 8;
        self.mem(true, 0x8B, RDI, R12, off);
        self.mem(true, 0x8B, RDI, RDI, 32);
        self.call_plt("slc_rt_word_tag");
        self.cmp_imm(RAX, i64::from(slc_abi::TAG_DELAY));
        let not_delay = self.jcc_hole(Cond::Ne);
        let make = self.buf.len();
        self.safepoint(func.map_id);
        self.spill(func.map_id);
        self.rr(true, 0x89, R12, RDI);
        self.imm(RSI, 2);
        self.imm(RDX, i64::from(slc_abi::TAG_ADAPTED));
        self.imm(RCX, i64::from(map_id));
        self.call_plt("slc_rt_alloc");
        self.reload_frame();
        self.mem(true, 0x8B, RCX, R12, off);
        self.mem(true, 0x8B, 10, RCX, 24);
        self.mem(true, 0x8B, 11, RCX, 32);
        self.mem(true, 0x89, 10, RAX, 16);
        self.mem(true, 0x89, 11, RAX, 24);
        self.rr(true, 0x89, RAX, R13);
        self.ret();
        self.patch_at(not_delay, self.buf.len());
        self.mem(true, 0x8B, RDI, R12, off);
        self.mem(true, 0x8B, RDI, RDI, 32);
        self.call_plt("slc_rt_word_tag");
        self.cmp_imm(RAX, i64::from(slc_abi::TAG_ADAPTED));
        let not_adapted = self.jcc_hole(Cond::Ne);
        self.jmp_hole_to(make);
        self.patch_at(not_adapted, self.buf.len());
        // Poll before the loads. `grow` preserves r13–r15; the code sits in r15 across it.
        self.safepoint(func.map_id);
        self.spill(func.map_id);
        self.mem(true, 0x8B, RAX, R12, off);
        self.mem(true, 0x8B, 11, RAX, 24);
        self.mem(true, 0x8B, R13, RAX, 32);
        self.mem(true, 0x8B, R14, 11, slc_abi::CLOSURE_ENV as i32);
        self.mem(true, 0x8B, RSI, 11, slc_abi::CLOSURE_FRAME_WORDS as i32);
        self.mem(true, 0x8B, R15, 11, slc_abi::CLOSURE_CODE as i32);
        self.rr(true, 0x89, R12, RDI);
        self.call_plt("slc_rt_grow_frame");
        self.rr(true, 0x89, RAX, R12);
        // The segment may have moved. The spill was rebased; `r13` is the argument.
        self.mem(true, 0x8B, RBX, R12, FRAME_SPILL_HANDLERS as i32);
        self.rr(true, 0x89, R15, 11);
        self.mem(true, 0x8B, R15, R12, FRAME_CONT_PREV as i32);
        self.jmp_reg(11);
    }

    fn activate(&mut self, func: &Function, consumer: Dest, tail: bool, arg_is_pointer: bool) {
        self.park_scalar(func, arg_is_pointer);
        self.safepoint(func.map_id);
        self.spill(func.map_id);
        self.unpark_scalar(func, arg_is_pointer);
        self.load_dest(consumer, func, 11);
        self.mem(true, 0x8B, RAX, 11, 0);
        self.and_imm(RAX, 0xffff);
        self.cmp_imm(RAX, i64::from(slc_abi::TAG_KONT));
        let not_kont = self.jcc_hole(Cond::Ne);
        self.rr(true, 0x89, R12, RDI);
        self.rr(true, 0x89, 11, RSI);
        self.call_plt("slc_rt_invoke");
        self.rr(true, 0x89, RAX, R12);
        self.mem(true, 0x8B, R15, R12, FRAME_CONT_PREV as i32);
        self.ret();
        self.patch_at(not_kont, self.buf.len());
        self.load_dest(consumer, func, 11);
        self.mem(true, 0x8B, RAX, 11, 0);
        self.and_imm(RAX, 0xffff);
        self.cmp_imm(RAX, i64::from(slc_abi::TAG_RESUME));
        let not_resume = self.jcc_hole(Cond::Ne);
        if tail {
            self.mem(true, 0x8B, R12, R12, FRAME_CONT_PREV as i32);
            self.mem(true, 0x8B, R15, R12, FRAME_CONT_PREV as i32);
        }
        self.rr(true, 0x89, R12, RDI);
        self.rr(true, 0x89, 11, RSI);
        self.call_plt("slc_rt_resume");
        self.rr(true, 0x89, RAX, R12);
        self.mem(true, 0x8B, R15, R12, FRAME_CONT_PREV as i32);
        self.ret();
        self.patch_at(not_resume, self.buf.len());
        self.load_dest(consumer, func, 11);
        self.mem(true, 0x8B, RAX, 11, 0);
        self.and_imm(RAX, 0xffff);
        self.cmp_imm(RAX, i64::from(slc_abi::TAG_CLOSURE));
        let not_closure = self.jcc_hole(Cond::Ne);
        if tail {
            self.tail_jump_closure(11);
        } else {
            self.dyn_call_closure(11, true);
            self.reload_frame();
        }
        let after_closure = self.jmp_hole();
        self.patch_at(not_closure, self.buf.len());
        self.buf.extend_from_slice(&[0x0F, 0x0B]);
        self.patch_at(after_closure, self.buf.len());
    }

    fn call_slc(&mut self, func: &Function, symbol: &str, callee_words: u32, arg_is_pointer: bool) {
        self.park_scalar(func, arg_is_pointer);
        self.safepoint(func.map_id);
        // Spill while `r13` is zero so the caller's traced slot does not keep the scalar.
        self.spill(func.map_id);
        self.unpark_scalar(func, arg_is_pointer);
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
        // The fast path still spills. `Ret` reloads `rbx` from this word, and a
        // capture copies it. A poll-only spill leaves the handler chain at 0.
        self.spill(map_id);
        let jmp = self.jmp_hole();
        let slow = self.buf.len();
        self.patch_at(jz, slow);
        self.patch_at(jae, slow);
        let fuel0 = self.rip_ext(0x83, 7);
        self.buf.push(0);
        self.rel_fuel(fuel0);
        let poll = self.jcc_hole(Cond::Ne);
        // Fuel is zero. `slc_rt_fail` prints the diverged diagnostic and returns
        // to `slc_rt_start`; it does not come back to this safepoint.
        self.call_plt(SLC_RT_FAIL);
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

    fn hide_off(func: &Function) -> i32 {
        FRAME_SLOT0 as i32 + i32::from(func.spill_base) * 8
    }

    /// `r13` holds a scalar while the map traces it. Park the word in the untraced slot.
    fn park_scalar(&mut self, func: &Function, arg_is_pointer: bool) {
        if arg_is_pointer || !func.val_is_pointer {
            return;
        }
        self.mem(true, 0x89, R13, R12, Self::hide_off(func));
        self.zero(R13);
    }

    fn unpark_scalar(&mut self, func: &Function, arg_is_pointer: bool) {
        if arg_is_pointer || !func.val_is_pointer {
            return;
        }
        self.mem(true, 0x8B, R13, R12, Self::hide_off(func));
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
        // `sub`/`cmp` imm8 follows the disp32. RIP-relative is from the next
        // instruction, one byte past the usual end of a disp32 operand.
        self.rels.push(Rel { at, symbol: SLC_FUEL.into(), kind: PC32, addend: -5 });
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

    fn rip_store(&mut self, reg: u8, symbol: &str) {
        let at = self.rip(true, 0x89, reg);
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
        Dest::Slot(slot) => Loc::Mem(R12, FRAME_SLOT0 as i32 + i32::from(slot) * 8),
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
