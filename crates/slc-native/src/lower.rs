//! Non-escaping straight-line code, and both match forms as a decision tree.
//! A λ, a delay, or a constructor that closes over a μ is not this compiler.

use std::collections::HashMap;

use slc_abi::{
    CLOSURE_CODE, CLOSURE_ENV, FRAME_FRAME_WORDS, FRAME_SLOT0, MAP_EMPTY, SLC_PROGRAM_ENTRY,
    TAG_DELAY, TAG_TAGGED, TAG_TUPLE, TAGGED_LABEL, TAGGED_PAYLOAD,
};
use slc_core::command::Command;
use slc_core::coterm::{CoCaseBranch, CoTerm};
use slc_core::term::{DELAY_BINDER, Term};
use slc_core::types::{Base, Type};
use slc_syntax::lower::Specialization;
use slc_syntax::pattern::{self, Descriptor, Pat};

use crate::{Block, Cond, Dest, Function, Inst, MapRecord, Module};

const SCRATCHES: u16 = 4;
const MATCH_TEMPS: u16 = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Value,
    Tail,
}

#[derive(Clone, Debug)]
enum Place {
    Val,
    Slot(u16),
}

#[derive(Clone)]
struct Row {
    pats: Vec<Pat>,
    places: Vec<Place>,
    arm: usize,
    binds: Vec<(String, Place)>,
}

enum Kind {
    Test,
    Switch,
    Tuple,
}

struct Child {
    block: usize,
    rows: Vec<Row>,
    object: Place,
    /// Non-empty when the constructor has fields. Nullary rows already dropped the column.
    fields: Vec<Pat>,
    label: String,
    col: usize,
}

struct Builder {
    labels: Vec<String>,
    pool: HashMap<String, u32>,
    pool_len: u32,
    roots: Vec<(u32, u32)>,
    /// Callee parameter word, and the word the callee returns.
    func_param: HashMap<String, bool>,
    func_result: HashMap<String, bool>,
    /// Symbol → slot name → the type of the word stored there.
    fn_types: HashMap<String, HashMap<String, Type>>,
    /// Variant or record label → payload field types.
    payloads: HashMap<String, Vec<Type>>,
    /// Type of the value in `r13` while a dispatch match is compiled, when it is known.
    val_ty: Option<Type>,
    /// `r13` at this function's safepoints is the parameter, which may be a pointer.
    val_ptr: bool,
    maps: Vec<MapRecord>,
    next_map: u32,
    edges: Vec<(String, String)>,
    slots: HashMap<String, u16>,
    pointer_slots: Vec<u16>,
    blocks: Vec<Block>,
    cur: usize,
    join: Option<usize>,
    scratch_base: u16,
    scratch_top: u16,
    temp_base: u16,
    temp_used: u16,
    arm_bodies: Vec<Term>,
    current: String,
}

pub fn lower(
    defs: &[(String, Term)],
    specs: &[Specialization],
    payloads: &HashMap<String, Vec<Type>>,
) -> Result<Module, String> {
    let mut builder = Builder::new(defs, specs, payloads);
    let mut funcs = Vec::new();
    for (name, term) in defs {
        if matches!(term, Term::Lam(binder, _) if binder != DELAY_BINDER) {
            funcs.push(builder.lower_fn(name, term)?);
        }
    }
    inflate(&mut funcs, &builder.edges);
    builder.assign_maps(&mut funcs);
    patch(&mut funcs);
    let main_words = funcs
        .iter()
        .find(|func| func.symbol == "main")
        .map(|func| func.frame_words)
        .ok_or_else(|| "no main".to_string())?;
    funcs.push(builder.build_entry(main_words));
    Ok(Module {
        functions: funcs,
        labels: builder.labels,
        pool_len: builder.pool_len,
        maps: builder.maps,
    })
}

fn inflate(funcs: &mut [Function], edges: &[(String, String)]) {
    let base: Vec<u32> = funcs.iter().map(|func| func.frame_words).collect();
    let names: Vec<String> = funcs.iter().map(|func| func.symbol.clone()).collect();
    loop {
        let snapshot: Vec<u32> = funcs.iter().map(|func| func.frame_words).collect();
        let mut changed = false;
        for (index, func) in funcs.iter_mut().enumerate() {
            let mut words = base[index];
            for (src, dst) in edges {
                if src == &func.symbol
                    && let Some(callee) = names.iter().position(|item| item == dst)
                {
                    words = words.max(snapshot[callee]);
                }
            }
            if words != func.frame_words {
                func.frame_words = words;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

fn patch(funcs: &mut [Function]) {
    let sizes: HashMap<String, u32> =
        funcs.iter().map(|func| (func.symbol.clone(), func.frame_words)).collect();
    let maps: HashMap<String, u32> =
        funcs.iter().map(|func| (func.symbol.clone(), func.map_id)).collect();
    for func in funcs {
        if let Some(Inst::Imm { value, .. }) =
            func.blocks.first_mut().and_then(|block| block.insts.first_mut())
        {
            *value = i64::from(func.frame_words);
        }
        for block in &mut func.blocks {
            for inst in &mut block.insts {
                match inst {
                    Inst::Safepoint { map_id } if *map_id == 0 => *map_id = func.map_id,
                    Inst::CallSlc { symbol, callee_frame_words, .. } => {
                        *callee_frame_words = sizes[symbol];
                    }
                    Inst::Tail { symbol, frame_words, map_id, .. } => {
                        *frame_words = sizes[symbol];
                        *map_id = maps[symbol];
                    }
                    _ => {}
                }
            }
        }
    }
}

fn type_is_pointer(ty: &Type) -> bool {
    match ty {
        // The word is the thunk, whatever it produces when forced.
        Type::Delayed(_, _) => true,
        Type::Rowed(inner, _) | Type::Dual(inner) => type_is_pointer(inner),
        Type::Pos(Base::Str)
        | Type::Neg(_)
        | Type::Named(_, _)
        | Type::Par(_)
        | Type::With(_)
        | Type::Sum(_) => true,
        Type::Pos(_) | Type::Var(_) | Type::Param(_) => false,
        Type::Tensor(items) => !items.is_empty(),
    }
}

fn slot_off(slot: u16) -> i32 {
    FRAME_SLOT0 as i32 + i32::from(slot) * 8
}

fn irrefutable(pat: &Pat) -> bool {
    match pat {
        Pat::Wildcard => true,
        Pat::Binding(_, inner) => irrefutable(inner),
        _ => false,
    }
}

fn terminated(insts: &[Inst]) -> bool {
    matches!(insts.last(), Some(Inst::Ret | Inst::Jmp { .. } | Inst::Tail { .. } | Inst::Ud2))
}

impl Builder {
    fn new(
        defs: &[(String, Term)],
        specs: &[Specialization],
        payloads: &HashMap<String, Vec<Type>>,
    ) -> Self {
        let mut builder = Self {
            labels: Vec::new(),
            pool: HashMap::new(),
            pool_len: 0,
            roots: Vec::new(),
            func_param: HashMap::new(),
            func_result: HashMap::new(),
            fn_types: HashMap::new(),
            payloads: payloads.clone(),
            val_ty: None,
            val_ptr: false,
            maps: Vec::new(),
            next_map: 2,
            edges: Vec::new(),
            slots: HashMap::new(),
            pointer_slots: Vec::new(),
            blocks: Vec::new(),
            cur: 0,
            join: None,
            scratch_base: 0,
            scratch_top: 0,
            temp_base: 0,
            temp_used: 0,
            arm_bodies: Vec::new(),
            current: String::new(),
        };
        for (name, term) in defs {
            if let Term::Tag(label, payload) = term
                && matches!(payload.as_ref(), Term::Var(unit) if unit == "$unit")
            {
                let id = builder.intern(label);
                builder.pool.insert(name.clone(), builder.pool_len);
                builder.roots.push((id, builder.pool_len));
                builder.pool_len += 1;
            }
            builder.intern_term(term);
            if let Term::Lam(param, _) = term
                && param != DELAY_BINDER
            {
                let spec = specs.iter().find(|spec| spec.symbol == *name);
                // Several parameters arrive as one tuple in `__args`, even when each is a scalar.
                let param_ptr = spec.is_some_and(|spec| {
                    (param == "__args" && spec.binders.len() >= 2)
                        || spec
                            .binders
                            .iter()
                            .chain(spec.locals.iter())
                            .find(|(binder, _)| binder == param)
                            .is_some_and(|(_, ty)| type_is_pointer(ty))
                });
                let result_ptr = spec.is_some_and(|spec| type_is_pointer(&spec.result));
                builder.func_param.insert(name.clone(), param_ptr);
                builder.func_result.insert(name.clone(), result_ptr);
                if let Some(spec) = spec {
                    let mut words = HashMap::new();
                    for (binder, ty) in spec.binders.iter().chain(spec.locals.iter()) {
                        words.insert(binder.clone(), ty.clone());
                    }
                    builder.fn_types.insert(name.clone(), words);
                }
            }
        }
        builder
    }

    fn intern(&mut self, label: &str) -> u32 {
        if let Some(index) = self.labels.iter().position(|item| item == label) {
            return index as u32;
        }
        let id = self.labels.len() as u32;
        self.labels.push(label.to_string());
        id
    }

    fn intern_term(&mut self, term: &Term) {
        match term {
            Term::Tag(label, payload) => {
                self.intern(label);
                self.intern_term(payload);
            }
            Term::Lam(_, body) => self.intern_term(body),
            Term::Mu(_, command) => self.intern_command(command),
            Term::Tuple(items) => {
                for item in items {
                    self.intern_term(item);
                }
            }
            Term::Co(coterm) => self.intern_coterm(coterm),
            Term::CoMatch { branches, .. } => {
                for branch in branches {
                    self.intern(&branch.label);
                    self.intern_command(&branch.body);
                }
            }
            Term::Var(_) => {}
        }
    }

    fn intern_command(&mut self, command: &Command) {
        let Command::Cut(term, coterm) = command;
        self.intern_term(term);
        self.intern_coterm(coterm);
    }

    fn intern_coterm(&mut self, coterm: &CoTerm) {
        match coterm {
            CoTerm::Covar(_) | CoTerm::Prj(_) => {}
            CoTerm::App(term, coterm) => {
                self.intern_term(term);
                self.intern_coterm(coterm);
            }
            CoTerm::Dtor(_, coterm) => self.intern_coterm(coterm),
            CoTerm::MuTilde(_, command) | CoTerm::MuTildeTensor(_, command) => {
                self.intern_command(command);
            }
            CoTerm::CoCase { branches, .. } => {
                for branch in branches {
                    self.intern(&branch.label);
                    self.intern_command(&branch.body);
                }
            }
        }
    }

    fn emit(&mut self, inst: Inst) {
        self.blocks[self.cur].insts.push(inst);
    }

    fn new_block(&mut self) -> usize {
        let id = self.blocks.len();
        self.blocks.push(Block { insts: Vec::new() });
        id
    }

    fn finish(&mut self, mode: Mode) {
        if terminated(&self.blocks[self.cur].insts) {
            return;
        }
        match mode {
            Mode::Tail => self.emit(Inst::Ret),
            Mode::Value => {
                if let Some(join) = self.join {
                    self.emit(Inst::Jmp { target: join });
                }
            }
        }
    }

    fn add_slot(&mut self, name: &str) {
        if name.starts_with("__discarded")
            || name == "__unused"
            || name == "__match_arg"
            || name == DELAY_BINDER
            || name == "$unit"
            || name.starts_with("$int_")
            || name.starts_with("$str_")
            || name.starts_with("$float_")
            || name.starts_with("$char_")
            || self.slots.contains_key(name)
        {
            return;
        }
        let index = self.slots.len() as u16;
        self.slots.insert(name.to_string(), index);
    }

    fn word_ty(&self, name: &str) -> Option<&Type> {
        self.fn_types.get(&self.current)?.get(name)
    }

    fn word_ptr(&self, name: &str) -> bool {
        self.word_ty(name).is_some_and(type_is_pointer)
    }

    fn place_ty(&self, place: &Place) -> Option<Type> {
        match place {
            Place::Val => self.val_ty.clone(),
            Place::Slot(slot) => self
                .slots
                .iter()
                .find(|(_, index)| *index == slot)
                .and_then(|(name, _)| self.word_ty(name).cloned()),
        }
    }

    /// Pointer bit of a match occurrence. A binder uses its recorded type. With no binder, `word`
    /// is the payload or tuple component type. A literal scalar is not a pointer; a string, tag,
    /// or tuple is. A wildcard without a type is not treated as a pointer.
    fn occurrence_ptr(&self, pats: &[&Pat], word: Option<&Type>) -> bool {
        fn binder(pat: &Pat) -> Option<&str> {
            match pat {
                Pat::Binding(name, _) => Some(name.as_str()),
                Pat::Or(alts) => alts.iter().find_map(binder),
                _ => None,
            }
        }
        if let Some(name) = pats.iter().copied().find_map(binder) {
            return self.word_ptr(name);
        }
        if let Some(ty) = word {
            return type_is_pointer(ty);
        }
        fn shaped(pat: &Pat) -> bool {
            match pat {
                Pat::Str(_) | Pat::Tagged(_, _) | Pat::Tuple(_) => true,
                Pat::Or(alts) => alts.iter().any(shaped),
                _ => false,
            }
        }
        pats.iter().copied().any(shaped)
    }

    fn mark_word(&mut self, name: &str, slot: u16, computed: bool) {
        if computed || self.word_ptr(name) {
            self.pointer_slots.push(slot);
        }
    }

    fn store_slot(&mut self, src: Dest, slot: u16) {
        self.emit(Inst::Store { src, base: Dest::Frame, offset: slot_off(slot), width: 8 });
    }

    fn load_slot(&mut self, dst: Dest, slot: u16) {
        self.emit(Inst::Load { dst, base: Dest::Frame, offset: slot_off(slot), width: 8 });
    }

    fn push_scratch(&mut self, src: Dest) -> Result<u16, String> {
        if self.scratch_top >= self.scratch_base + SCRATCHES {
            return Err("too many live pointers".into());
        }
        let slot = self.scratch_top;
        self.scratch_top += 1;
        self.store_slot(src, slot);
        Ok(slot)
    }

    fn pop_scratch(&mut self) {
        self.scratch_top -= 1;
    }

    fn alloc_temp(&mut self, pointer: bool) -> Result<u16, String> {
        if self.temp_used >= MATCH_TEMPS {
            return Err("too many match temps".into());
        }
        let slot = self.temp_base + self.temp_used;
        self.temp_used += 1;
        if pointer {
            self.pointer_slots.push(slot);
        }
        Ok(slot)
    }

    fn heap_map(&mut self, slots: &[u16]) -> u32 {
        if slots.is_empty() {
            return MAP_EMPTY;
        }
        if let Some(map) = self.maps.iter().find(|map| map.frame_words == 0 && map.slots == slots) {
            return map.map_id;
        }
        let id = self.next_map;
        self.next_map += 1;
        self.maps.push(MapRecord {
            map_id: id,
            frame_words: 0,
            val_is_pointer: false,
            slots: slots.to_vec(),
        });
        id
    }

    fn lower_fn(&mut self, symbol: &str, term: &Term) -> Result<Function, String> {
        let Term::Lam(param, body) = term else {
            return Err(format!("{symbol} is not a function"));
        };
        self.current = symbol.to_string();
        self.slots.clear();
        self.pointer_slots.clear();
        self.blocks.clear();
        self.join = None;
        self.temp_used = 0;
        self.arm_bodies.clear();
        self.add_slot(param);
        self.collect_term(body);
        let named = self.slots.len() as u16;
        self.scratch_base = named;
        self.scratch_top = named;
        self.temp_base = named + SCRATCHES;
        let slot_count = named + SCRATCHES + MATCH_TEMPS;
        for index in 0..SCRATCHES {
            self.pointer_slots.push(self.scratch_base + index);
        }
        let param_ptr = self.func_param.get(symbol).copied().unwrap_or(false);
        self.val_ptr = param_ptr;
        if param_ptr && let Some(&slot) = self.slots.get(param.as_str()) {
            self.pointer_slots.push(slot);
        }
        // `spill_base` is the untraced slot a scalar argument uses when `r13` would be traced.
        let frame_words = 9 + u32::from(slot_count) + 1;
        self.blocks.push(Block { insts: Vec::new() });
        self.cur = 0;
        // The encoder installs the map id before this block. Frame size has to be
        // visible to the safepoint that follows the slot clears.
        self.emit(Inst::Imm { dst: Dest::V(0), value: i64::from(frame_words) });
        self.emit(Inst::Store {
            src: Dest::V(0),
            base: Dest::Frame,
            offset: FRAME_FRAME_WORDS as i32,
            width: 8,
        });
        for slot in 0..slot_count {
            self.emit(Inst::Imm { dst: Dest::V(0), value: 0 });
            self.store_slot(Dest::V(0), slot);
        }
        if let Some(&slot) = self.slots.get(param.as_str()) {
            self.store_slot(Dest::Val, slot);
        }
        // Real id is filled in once frame maps exist. Zero aborts a collecting poll.
        self.emit(Inst::Safepoint { map_id: 0 });
        if let Term::Lam(inner, _) = body.as_ref()
            && inner != DELAY_BINDER
        {
            return Err(format!("{symbol}: escaping closure"));
        }
        self.compile_term(body, Mode::Tail)?;
        self.finish(Mode::Tail);
        Ok(Function {
            symbol: symbol.to_string(),
            map_id: 0,
            frame_words,
            val_is_pointer: param_ptr,
            pointer_slots: std::mem::take(&mut self.pointer_slots),
            spill_base: slot_count,
            blocks: std::mem::take(&mut self.blocks),
            entry: false,
        })
    }

    fn collect_term(&mut self, term: &Term) {
        match term {
            Term::Lam(name, body) if name != DELAY_BINDER => {
                self.add_slot(name);
                self.collect_term(body);
            }
            Term::Lam(_, _) => {}
            Term::Mu(_, command) => self.collect_command(command),
            Term::Tag(_, payload) => self.collect_term(payload),
            Term::Tuple(items) => {
                for item in items {
                    self.collect_term(item);
                }
            }
            Term::Co(coterm) => self.collect_coterm(coterm),
            Term::Var(_) | Term::CoMatch { .. } => {}
        }
    }

    fn collect_command(&mut self, command: &Command) {
        if self.collect_dispatch(command) {
            return;
        }
        let Command::Cut(term, coterm) = command;
        self.collect_term(term);
        self.collect_coterm(coterm);
    }

    /// Pattern binders of a dispatch match are names in the descriptor, not core binders.
    fn collect_dispatch(&mut self, command: &Command) -> bool {
        let Command::Cut(Term::Var(name), CoTerm::App(payload, _)) = command else {
            return false;
        };
        if name != "__match_dispatch" {
            return false;
        }
        let Term::Tuple(items) = payload else { return false };
        for item in items {
            self.note_pattern_slots(item);
            self.collect_term(item);
        }
        true
    }

    fn note_pattern_slots(&mut self, term: &Term) {
        let Term::Tag(label, inner) = term else { return };
        if label != "__match_arm" {
            return;
        }
        let Term::Tuple(pair) = inner.as_ref() else { return };
        let Some(Term::Var(desc)) = pair.first() else { return };
        let text = desc.strip_prefix("$str_").unwrap_or(desc);
        let Descriptor::Pattern(pat) = pattern::parse_pattern(text) else { return };
        fn walk(builder: &mut Builder, pat: &Pat) {
            match pat {
                Pat::Binding(name, inner) => {
                    builder.add_slot(name);
                    walk(builder, inner);
                }
                Pat::Tagged(_, fields) | Pat::Tuple(fields) | Pat::Or(fields) => {
                    for field in fields {
                        walk(builder, field);
                    }
                }
                Pat::Range(lo, hi) => {
                    walk(builder, lo);
                    walk(builder, hi);
                }
                _ => {}
            }
        }
        walk(self, &pat);
    }

    fn collect_coterm(&mut self, coterm: &CoTerm) {
        match coterm {
            CoTerm::Covar(_) | CoTerm::Prj(_) => {}
            CoTerm::App(term, next) => {
                self.collect_term(term);
                self.collect_coterm(next);
            }
            CoTerm::Dtor(_, next) => self.collect_coterm(next),
            CoTerm::MuTilde(name, command) => {
                self.add_slot(name);
                self.collect_command(command);
            }
            CoTerm::MuTildeTensor(names, command) => {
                for name in names {
                    self.add_slot(name);
                }
                self.collect_command(command);
            }
            CoTerm::CoCase { branches, .. } => {
                for branch in branches {
                    for name in &branch.binders {
                        self.add_slot(name);
                    }
                    self.collect_command(&branch.body);
                }
            }
        }
    }

    fn compile_term(&mut self, term: &Term, mode: Mode) -> Result<bool, String> {
        match term {
            Term::Var(name) => self.compile_var(name, mode),
            Term::Lam(name, body) if name == DELAY_BINDER => self.compile_delay(body),
            Term::Lam(_, _) => Err("escaping closure".into()),
            Term::Tag(label, payload) => self.compile_tag(label, payload, mode),
            Term::Tuple(items) => self.compile_tuple(items, mode),
            Term::Mu(name, command) => self.compile_mu(name, command, mode),
            Term::Co(_) | Term::CoMatch { .. } => {
                Err("handlers and consumers are not a test of this compiler".into())
            }
        }
    }

    fn compile_command(&mut self, command: &Command, mode: Mode) -> Result<bool, String> {
        let Command::Cut(term, coterm) = command;
        match coterm {
            CoTerm::Covar(_) => self.compile_term(term, mode),
            CoTerm::CoCase { branches, .. } => self.compile_cocase(term, branches, mode),
            CoTerm::MuTildeTensor(binders, body) => self.compile_tensor(term, binders, body, mode),
            CoTerm::MuTilde(binder, body) => self.compile_bind(term, binder, body, mode),
            CoTerm::App(_, _) | CoTerm::Prj(_) | CoTerm::Dtor(_, _) => {
                Err("not a straight-line command".into())
            }
        }
    }

    fn compile_mu(&mut self, mu: &str, command: &Command, mode: Mode) -> Result<bool, String> {
        let Command::Cut(value, coterm) = command;
        match coterm {
            CoTerm::MuTilde(binder, body) => self.compile_bind(value, binder, body, mode),
            CoTerm::MuTildeTensor(binders, body) => self.compile_tensor(value, binders, body, mode),
            CoTerm::CoCase { branches, .. } => self.compile_cocase(value, branches, mode),
            CoTerm::App(arg, cont) => {
                if let Term::Var(name) = value
                    && name == "__match_dispatch"
                {
                    return self.compile_dispatch(arg, mode);
                }
                if let CoTerm::Covar(cov) = cont.as_ref()
                    && cov == mu
                {
                    return self.compile_call(value, arg, mode);
                }
                Err(format!("not a direct call in {mu}"))
            }
            CoTerm::Covar(_) => self.compile_term(value, mode),
            CoTerm::Prj(_) | CoTerm::Dtor(_, _) => Err("not straight-line".into()),
        }
    }

    fn compile_bind(
        &mut self,
        value: &Term,
        binder: &str,
        body: &Command,
        mode: Mode,
    ) -> Result<bool, String> {
        let pointer = self.compile_term(value, Mode::Value)?;
        if let Some(&slot) = self.slots.get(binder) {
            self.store_slot(Dest::Val, slot);
            let kept = pointer || self.word_ptr(binder);
            self.mark_word(binder, slot, pointer);
            // A scalar left in `r13` would be traced at the next safepoint.
            if !kept && self.val_ptr {
                self.emit(Inst::Imm { dst: Dest::Val, value: 0 });
            }
        }
        self.compile_command(body, mode)
    }

    fn compile_var(&mut self, name: &str, mode: Mode) -> Result<bool, String> {
        let pointer = if let Some(text) = name.strip_prefix("$int_") {
            let value = text.parse::<i64>().map_err(|_| format!("bad integer {name}"))?;
            self.emit(Inst::Imm { dst: Dest::Val, value });
            false
        } else if name == "$unit" {
            self.emit(Inst::Imm { dst: Dest::Val, value: 0 });
            false
        } else if let Some(&index) = self.pool.get(name) {
            self.emit(Inst::LeaPool { dst: Dest::V(0), index });
            self.emit(Inst::Load { dst: Dest::Val, base: Dest::V(0), offset: 0, width: 8 });
            true
        } else if let Some(&slot) = self.slots.get(name) {
            self.load_slot(Dest::Val, slot);
            self.pointer_slots.contains(&slot)
        } else {
            return Err(format!("unbound {name}"));
        };
        if mode == Mode::Tail {
            self.finish(Mode::Tail);
        }
        Ok(pointer)
    }

    fn compile_delay(&mut self, _body: &Term) -> Result<bool, String> {
        // Forcing is a later compiler. Entering the thunk is a bug in the test.
        let code = self.new_block();
        let saved = self.cur;
        self.cur = code;
        self.emit(Inst::Ud2);
        self.cur = saved;
        self.emit(Inst::LeaBlock { dst: Dest::V(0), block: code });
        let scratch = self.push_scratch(Dest::V(0))?;
        self.emit(Inst::CallAlloc { words: 2, tag: TAG_DELAY, map_id: MAP_EMPTY, dst: Dest::V(0) });
        self.load_slot(Dest::V(1), scratch);
        self.emit(Inst::Store {
            src: Dest::V(1),
            base: Dest::V(0),
            offset: CLOSURE_CODE as i32,
            width: 8,
        });
        self.emit(Inst::Imm { dst: Dest::V(1), value: 0 });
        self.emit(Inst::Store {
            src: Dest::V(1),
            base: Dest::V(0),
            offset: CLOSURE_ENV as i32,
            width: 8,
        });
        self.pop_scratch();
        self.emit(Inst::Mov { dst: Dest::Val, src: Dest::V(0) });
        Ok(true)
    }

    fn compile_tag(&mut self, label: &str, payload: &Term, mode: Mode) -> Result<bool, String> {
        let pointer = self.compile_term(payload, Mode::Value)?;
        let scratch = if pointer { Some(self.push_scratch(Dest::Val)?) } else { None };
        let id = self.intern(label);
        let map = if pointer { self.heap_map(&[1]) } else { MAP_EMPTY };
        self.emit(Inst::CallAlloc { words: 2, tag: TAG_TAGGED, map_id: map, dst: Dest::V(0) });
        self.emit(Inst::Imm { dst: Dest::V(1), value: i64::from(id) });
        self.emit(Inst::Store {
            src: Dest::V(1),
            base: Dest::V(0),
            offset: TAGGED_LABEL as i32,
            width: 8,
        });
        if let Some(slot) = scratch {
            self.load_slot(Dest::V(1), slot);
            self.pop_scratch();
        } else {
            self.emit(Inst::Mov { dst: Dest::V(1), src: Dest::Val });
        }
        self.emit(Inst::Store {
            src: Dest::V(1),
            base: Dest::V(0),
            offset: TAGGED_PAYLOAD as i32,
            width: 8,
        });
        self.emit(Inst::Mov { dst: Dest::Val, src: Dest::V(0) });
        if mode == Mode::Tail {
            self.finish(Mode::Tail);
        }
        Ok(true)
    }

    fn compile_tuple(&mut self, items: &[Term], mode: Mode) -> Result<bool, String> {
        let mut saved = Vec::new();
        let mut heap_slots = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let pointer = self.compile_term(item, Mode::Value)?;
            let slot = if pointer {
                self.push_scratch(Dest::Val)?
            } else {
                // A scalar component is live across the allocation but must not be traced.
                let slot = self.alloc_temp(false)?;
                self.store_slot(Dest::Val, slot);
                slot
            };
            if pointer {
                heap_slots.push(index as u16 + 1);
            }
            saved.push((slot, pointer));
        }
        let map = self.heap_map(&heap_slots);
        self.emit(Inst::CallAlloc {
            words: 1 + items.len() as u32,
            tag: TAG_TUPLE,
            map_id: map,
            dst: Dest::V(0),
        });
        self.emit(Inst::Imm { dst: Dest::V(1), value: items.len() as i64 });
        self.emit(Inst::Store { src: Dest::V(1), base: Dest::V(0), offset: 16, width: 8 });
        for (index, (slot, _)) in saved.iter().enumerate() {
            self.load_slot(Dest::V(1), *slot);
            self.emit(Inst::Store {
                src: Dest::V(1),
                base: Dest::V(0),
                offset: 24 + 8 * index as i32,
                width: 8,
            });
        }
        for (_, pointer) in saved.iter().rev() {
            if *pointer {
                self.pop_scratch();
            }
        }
        self.emit(Inst::Mov { dst: Dest::Val, src: Dest::V(0) });
        if mode == Mode::Tail {
            self.finish(Mode::Tail);
        }
        Ok(true)
    }

    fn compile_call(&mut self, callee: &Term, arg: &Term, mode: Mode) -> Result<bool, String> {
        let Term::Var(symbol) = callee else {
            return Err("indirect call is not a test of this compiler".into());
        };
        if !self.func_param.contains_key(symbol) {
            return Err(format!("unknown function {symbol}"));
        }
        self.compile_term(arg, Mode::Value)?;
        let param_ptr = self.func_param[symbol];
        let result_ptr = self.func_result.get(symbol).copied().unwrap_or(false);
        if param_ptr {
            // The call's safepoint runs inside the encoder, after this store.
            // The scratch stays in the map; only the counter is released below.
            self.push_scratch(Dest::Val)?;
        }
        match mode {
            Mode::Tail => {
                self.edges.push((self.current.clone(), symbol.clone()));
                self.emit(Inst::Tail {
                    symbol: symbol.clone(),
                    frame_words: 0,
                    map_id: 0,
                    arg_is_pointer: param_ptr,
                });
            }
            Mode::Value => {
                self.emit(Inst::CallSlc {
                    symbol: symbol.clone(),
                    callee_frame_words: 0,
                    arg_is_pointer: param_ptr,
                });
            }
        }
        if param_ptr {
            self.pop_scratch();
        }
        Ok(result_ptr)
    }

    fn compile_cocase(
        &mut self,
        scrutinee: &Term,
        branches: &[CoCaseBranch],
        mode: Mode,
    ) -> Result<bool, String> {
        self.compile_term(scrutinee, Mode::Value)?;
        let saved = self.join;
        let join = match mode {
            Mode::Value => Some(self.new_block()),
            Mode::Tail => None,
        };
        self.join = join;
        self.emit(Inst::Load {
            dst: Dest::V(0),
            base: Dest::Val,
            offset: TAGGED_LABEL as i32,
            width: 4,
        });
        let mut arm_blocks = Vec::new();
        for branch in branches {
            let block = self.new_block();
            arm_blocks.push(block);
            let id = self.intern(&branch.label);
            self.emit(Inst::CmpJcc {
                left: Dest::V(0),
                right: i64::from(id),
                cond: Cond::E,
                target: block,
            });
        }
        let miss = self.new_block();
        self.emit(Inst::Jmp { target: miss });
        self.cur = miss;
        self.emit(Inst::Ud2);
        let mut result_ptr = false;
        for (branch, block) in branches.iter().zip(arm_blocks) {
            self.cur = block;
            let scratch = self.scratch_top;
            self.bind_payload(&branch.binders);
            result_ptr |= self.compile_command(&branch.body, mode)?;
            self.finish(mode);
            self.scratch_top = scratch;
        }
        if let Some(join) = join {
            self.cur = join;
        }
        self.join = saved;
        Ok(result_ptr)
    }

    fn bind_payload(&mut self, binders: &[String]) {
        let real = binders.iter().any(|name| self.slots.contains_key(name));
        if !real {
            return;
        }
        if binders.len() == 1 {
            if let Some(&slot) = self.slots.get(binders[0].as_str()) {
                self.emit(Inst::Load {
                    dst: Dest::V(0),
                    base: Dest::Val,
                    offset: TAGGED_PAYLOAD as i32,
                    width: 8,
                });
                self.store_slot(Dest::V(0), slot);
                self.mark_word(&binders[0], slot, false);
            }
            return;
        }
        self.emit(Inst::Load {
            dst: Dest::V(1),
            base: Dest::Val,
            offset: TAGGED_PAYLOAD as i32,
            width: 8,
        });
        for (index, name) in binders.iter().enumerate() {
            if let Some(&slot) = self.slots.get(name.as_str()) {
                self.emit(Inst::Load {
                    dst: Dest::V(0),
                    base: Dest::V(1),
                    offset: 24 + 8 * index as i32,
                    width: 8,
                });
                self.store_slot(Dest::V(0), slot);
                self.mark_word(name, slot, false);
            }
        }
    }

    fn compile_tensor(
        &mut self,
        scrutinee: &Term,
        binders: &[String],
        body: &Command,
        mode: Mode,
    ) -> Result<bool, String> {
        self.compile_term(scrutinee, Mode::Value)?;
        for (index, name) in binders.iter().enumerate() {
            if let Some(&slot) = self.slots.get(name.as_str()) {
                self.emit(Inst::Load {
                    dst: Dest::V(0),
                    base: Dest::Val,
                    offset: 24 + 8 * index as i32,
                    width: 8,
                });
                self.store_slot(Dest::V(0), slot);
                self.mark_word(name, slot, false);
            }
        }
        self.compile_command(body, mode)
    }

    fn compile_dispatch(&mut self, payload: &Term, mode: Mode) -> Result<bool, String> {
        let Term::Tuple(items) = payload else {
            return Err("match dispatch payload".into());
        };
        let (scrutinee, arms) = items.split_first().ok_or("empty match")?;
        let saved_ty = self.val_ty.take();
        self.val_ty = match scrutinee {
            Term::Var(name) => self.word_ty(name).cloned(),
            _ => None,
        };
        self.compile_term(scrutinee, Mode::Value)?;
        let mut bodies = Vec::new();
        let mut rows = Vec::new();
        for (index, arm) in arms.iter().enumerate() {
            let Term::Tag(_, inner) = arm else {
                return Err("match arm".into());
            };
            let Term::Tuple(pair) = inner.as_ref() else {
                return Err("match arm tuple".into());
            };
            let (Term::Var(desc), Term::Lam(_, body)) = (&pair[0], &pair[1]) else {
                return Err("match arm shape".into());
            };
            let text = desc.strip_prefix("$str_").unwrap_or(desc);
            let Descriptor::Pattern(pat) = pattern::parse_pattern(text) else {
                return Err("match rest".into());
            };
            bodies.push(body.as_ref().clone());
            rows.push(Row {
                pats: vec![pat],
                places: vec![Place::Val],
                arm: index,
                binds: Vec::new(),
            });
        }
        let saved_arms = std::mem::replace(&mut self.arm_bodies, bodies);
        let saved_join = self.join;
        let join = match mode {
            Mode::Value => Some(self.new_block()),
            Mode::Tail => None,
        };
        self.join = join;
        let result_ptr = self.compile_matrix(rows, mode)?;
        if let Some(join) = join {
            if !terminated(&self.blocks[self.cur].insts) {
                self.finish(Mode::Value);
            }
            self.cur = join;
        }
        self.join = saved_join;
        self.arm_bodies = saved_arms;
        self.val_ty = saved_ty;
        Ok(result_ptr)
    }

    fn compile_matrix(&mut self, mut rows: Vec<Row>, mode: Mode) -> Result<bool, String> {
        if rows.is_empty() {
            self.emit(Inst::Ud2);
            return Ok(false);
        }
        explode_bindings(&mut rows);
        if rows[0].pats.iter().all(irrefutable) {
            return self.emit_row(&rows[0], mode);
        }
        let col = choose_column(&rows);
        match column_kind(&rows, col)? {
            Kind::Test => self.compile_test(rows, col, mode),
            Kind::Switch => self.compile_switch(rows, col, mode),
            Kind::Tuple => {
                self.expand_tuples(&mut rows, col)?;
                self.compile_matrix(rows, mode)
            }
        }
    }

    fn emit_row(&mut self, row: &Row, mode: Mode) -> Result<bool, String> {
        let scratch = self.scratch_top;
        for (name, place) in &row.binds {
            if let Some(&slot) = self.slots.get(name.as_str()) {
                match place {
                    Place::Val => self.store_slot(Dest::Val, slot),
                    Place::Slot(src) => {
                        self.load_slot(Dest::V(0), *src);
                        self.store_slot(Dest::V(0), slot);
                    }
                }
                self.mark_word(name, slot, false);
            }
        }
        let body = self.arm_bodies[row.arm].clone();
        let result = self.compile_term(&body, mode)?;
        self.finish(mode);
        self.scratch_top = scratch;
        Ok(result)
    }

    fn compile_test(&mut self, rows: Vec<Row>, col: usize, mode: Mode) -> Result<bool, String> {
        let place = rows[0].places[col].clone();
        let outcome = rows[0].pats[col].clone();
        let (success, failure) = split_outcome(&rows, col, &outcome);
        match &outcome {
            Pat::Int(value) => {
                let fail = self.new_block();
                self.cmp_place(&place, *value, Cond::Ne, fail);
                let mut result = self.compile_matrix(success, mode)?;
                self.finish(mode);
                self.cur = fail;
                result |= self.compile_matrix(failure, mode)?;
                self.finish(mode);
                Ok(result)
            }
            Pat::Range(lo, hi) => {
                let (Pat::Int(lo), Pat::Int(hi)) = (lo.as_ref(), hi.as_ref()) else {
                    return Err("only integer ranges".into());
                };
                let fail = self.new_block();
                self.emit_range(&place, *lo, *hi, fail);
                let mut result = self.compile_matrix(success, mode)?;
                self.finish(mode);
                self.cur = fail;
                result |= self.compile_matrix(failure, mode)?;
                self.finish(mode);
                Ok(result)
            }
            Pat::Or(alts) => {
                let body = self.new_block();
                let fail = self.new_block();
                self.emit_alternatives(alts, &place, body)?;
                self.emit(Inst::Jmp { target: fail });
                self.cur = body;
                let mut result = self.compile_matrix(success, mode)?;
                self.finish(mode);
                self.cur = fail;
                result |= self.compile_matrix(failure, mode)?;
                self.finish(mode);
                Ok(result)
            }
            other => Err(format!("match test {other:?} is not a test of this compiler")),
        }
    }

    fn emit_alternatives(
        &mut self,
        alts: &[Pat],
        place: &Place,
        body: usize,
    ) -> Result<(), String> {
        for alt in alts {
            // Bindings are stored only after the alternative matches, so a miss
            // does not have to roll a slot back.
            let (bare, binds) = peel_binds(alt);
            let hit = if binds.is_empty() { body } else { self.new_block() };
            match &bare {
                Pat::Int(value) => self.cmp_place(place, *value, Cond::E, hit),
                Pat::Range(lo, hi) => {
                    let (Pat::Int(lo), Pat::Int(hi)) = (lo.as_ref(), hi.as_ref()) else {
                        return Err("only integer ranges".into());
                    };
                    let next = self.new_block();
                    self.emit_range(place, *lo, *hi, next);
                    self.emit(Inst::Jmp { target: hit });
                    self.cur = next;
                }
                Pat::Or(inner) => self.emit_alternatives(inner, place, hit)?,
                Pat::Tagged(label, fields) if fields.is_empty() => {
                    self.load_label(place);
                    let id = self.intern(label);
                    self.emit(Inst::CmpJcc {
                        left: Dest::V(0),
                        right: i64::from(id),
                        cond: Cond::E,
                        target: hit,
                    });
                }
                Pat::Wildcard => self.emit(Inst::Jmp { target: hit }),
                other => return Err(format!("or alternative {other:?}")),
            }
            if hit != body {
                let saved = self.cur;
                self.cur = hit;
                self.store_bound_names(&binds, place);
                self.emit(Inst::Jmp { target: body });
                self.cur = saved;
            }
        }
        Ok(())
    }

    fn store_bound_names(&mut self, names: &[String], place: &Place) {
        for name in names {
            let Some(&slot) = self.slots.get(name.as_str()) else { continue };
            match place {
                Place::Val => self.store_slot(Dest::Val, slot),
                Place::Slot(src) => {
                    self.load_slot(Dest::V(0), *src);
                    self.store_slot(Dest::V(0), slot);
                }
            }
            self.mark_word(name, slot, false);
        }
    }

    fn compile_switch(&mut self, rows: Vec<Row>, col: usize, mode: Mode) -> Result<bool, String> {
        let object = rows
            .iter()
            .find_map(|row| match &row.pats[col] {
                Pat::Tagged(_, _) => Some(row.places[col].clone()),
                _ => None,
            })
            .ok_or_else(|| "empty switch".to_string())?;
        self.load_label(&object);
        let mut ctors = Vec::new();
        for row in &rows {
            if let Pat::Tagged(label, _) = &row.pats[col]
                && !ctors.contains(label)
            {
                ctors.push(label.clone());
            }
        }
        let mut children = Vec::new();
        for label in &ctors {
            let block = self.new_block();
            let id = self.intern(label);
            self.emit(Inst::CmpJcc {
                left: Dest::V(0),
                right: i64::from(id),
                cond: Cond::E,
                target: block,
            });
            let fields = rows
                .iter()
                .find_map(|row| match &row.pats[col] {
                    Pat::Tagged(name, fields) if name == label => Some(fields.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            let mut child_rows = Vec::new();
            for row in &rows {
                match &row.pats[col] {
                    Pat::Tagged(name, found) if name == label => {
                        child_rows.push(project(row, col, found));
                    }
                    Pat::Wildcard => {
                        child_rows.push(project(row, col, &vec![Pat::Wildcard; fields.len()]))
                    }
                    _ => {}
                }
            }
            children.push(Child {
                block,
                rows: child_rows,
                object: object.clone(),
                fields,
                label: label.clone(),
                col,
            });
        }
        let miss = self.new_block();
        self.emit(Inst::Jmp { target: miss });
        let mut result = false;
        for child in children {
            self.cur = child.block;
            let mut rows = child.rows;
            if !child.fields.is_empty() {
                let temps = self.materialize_fields(
                    &child.object,
                    &child.label,
                    &rows,
                    child.col,
                    child.fields.len(),
                )?;
                for row in &mut rows {
                    for (offset, temp) in temps.iter().enumerate() {
                        row.places[child.col + offset] = Place::Slot(*temp);
                    }
                }
            }
            result |= self.compile_matrix(rows, mode)?;
            self.finish(mode);
        }
        self.cur = miss;
        let default: Vec<Row> = rows
            .iter()
            .filter(|row| matches!(row.pats[col], Pat::Wildcard))
            .map(|row| drop_col(row, col))
            .collect();
        if default.is_empty() {
            self.emit(Inst::Ud2);
        } else {
            result |= self.compile_matrix(default, mode)?;
            self.finish(mode);
        }
        Ok(result)
    }

    fn materialize_fields(
        &mut self,
        object: &Place,
        label: &str,
        rows: &[Row],
        col: usize,
        width: usize,
    ) -> Result<Vec<u16>, String> {
        fn args_of<'a>(ty: &'a Type, owner: &str) -> Option<&'a [Type]> {
            match ty {
                Type::Named(name, args) if name == owner => Some(args),
                Type::Rowed(inner, _) | Type::Dual(inner) => args_of(inner, owner),
                _ => None,
            }
        }
        let owner_name = label.split_once("::").map(|(name, _)| name).unwrap_or(label);
        let owner = self.place_ty(object);
        let words: Vec<Option<Type>> = (0..width)
            .map(|index| {
                let declared = self.payloads.get(label)?.get(index)?;
                Some(match owner.as_ref().and_then(|ty| args_of(ty, owner_name)) {
                    Some(args) => declared.instantiate(args),
                    None => declared.clone(),
                })
            })
            .collect();
        if width == 1 {
            let pats: Vec<&Pat> = rows.iter().map(|row| &row.pats[col]).collect();
            let bit = self.occurrence_ptr(&pats, words.first().and_then(Option::as_ref));
            let temp = self.alloc_temp(bit)?;
            self.load_from_place(object, TAGGED_PAYLOAD as i32, Dest::V(0));
            self.store_slot(Dest::V(0), temp);
            return Ok(vec![temp]);
        }
        let tuple = self.alloc_temp(true)?;
        self.load_from_place(object, TAGGED_PAYLOAD as i32, Dest::V(0));
        self.store_slot(Dest::V(0), tuple);
        let mut temps = Vec::new();
        for index in 0..width {
            let pats: Vec<&Pat> = rows.iter().map(|row| &row.pats[col + index]).collect();
            let bit = self.occurrence_ptr(&pats, words.get(index).and_then(Option::as_ref));
            let temp = self.alloc_temp(bit)?;
            self.load_from_place(&Place::Slot(tuple), 24 + 8 * index as i32, Dest::V(0));
            self.store_slot(Dest::V(0), temp);
            temps.push(temp);
        }
        Ok(temps)
    }

    fn expand_tuples(&mut self, rows: &mut [Row], col: usize) -> Result<(), String> {
        fn component(ty: &Type, index: usize) -> Option<&Type> {
            match ty {
                Type::Tensor(items) => items.get(index),
                Type::Rowed(inner, _) | Type::Dual(inner) => component(inner, index),
                _ => None,
            }
        }
        let width = rows
            .iter()
            .find_map(|row| match &row.pats[col] {
                Pat::Tuple(items) => Some(items.len()),
                _ => None,
            })
            .ok_or_else(|| "tuple column".to_string())?;
        let place = rows[0].places[col].clone();
        let owner = self.place_ty(&place);
        let mut comps = Vec::new();
        for index in 0..width {
            let pats: Vec<Pat> = rows
                .iter()
                .filter_map(|row| match &row.pats[col] {
                    Pat::Tuple(items) => items.get(index).cloned(),
                    _ => None,
                })
                .collect();
            let word = owner.as_ref().and_then(|ty| component(ty, index)).cloned();
            let refs: Vec<&Pat> = pats.iter().collect();
            let bit = self.occurrence_ptr(&refs, word.as_ref());
            let temp = self.alloc_temp(bit)?;
            self.load_from_place(&place, 24 + 8 * index as i32, Dest::V(0));
            self.store_slot(Dest::V(0), temp);
            comps.push(Place::Slot(temp));
        }
        for row in rows.iter_mut() {
            let pats = match &row.pats[col] {
                Pat::Tuple(items) if items.len() == width => items.clone(),
                Pat::Wildcard => vec![Pat::Wildcard; width],
                other => return Err(format!("tuple pattern {other:?}")),
            };
            row.pats.splice(col..=col, pats);
            row.places.splice(col..=col, comps.clone());
        }
        Ok(())
    }

    fn load_from_place(&mut self, place: &Place, offset: i32, dst: Dest) {
        match place {
            Place::Val => {
                self.emit(Inst::Load { dst, base: Dest::Val, offset, width: 8 });
            }
            Place::Slot(slot) => {
                self.load_slot(Dest::V(1), *slot);
                self.emit(Inst::Load { dst, base: Dest::V(1), offset, width: 8 });
            }
        }
    }

    fn load_label(&mut self, place: &Place) {
        match place {
            Place::Val => self.emit(Inst::Load {
                dst: Dest::V(0),
                base: Dest::Val,
                offset: TAGGED_LABEL as i32,
                width: 4,
            }),
            Place::Slot(slot) => {
                self.load_slot(Dest::V(1), *slot);
                self.emit(Inst::Load {
                    dst: Dest::V(0),
                    base: Dest::V(1),
                    offset: TAGGED_LABEL as i32,
                    width: 4,
                });
            }
        }
    }

    fn cmp_place(&mut self, place: &Place, value: i64, cond: Cond, target: usize) {
        let left = match place {
            Place::Val => Dest::Val,
            Place::Slot(slot) => {
                self.load_slot(Dest::V(0), *slot);
                Dest::V(0)
            }
        };
        self.emit(Inst::CmpJcc { left, right: value, cond, target });
    }

    fn emit_range(&mut self, place: &Place, lo: i64, hi: i64, fail: usize) {
        let src = match place {
            Place::Val => Dest::Val,
            Place::Slot(slot) => {
                self.load_slot(Dest::V(0), *slot);
                Dest::V(0)
            }
        };
        self.emit(Inst::InRange { src, lo, hi, fail });
    }

    fn assign_maps(&mut self, funcs: &mut [Function]) {
        for func in funcs {
            func.pointer_slots.sort_unstable();
            func.pointer_slots.dedup();
            func.map_id = self.next_map;
            self.maps.push(MapRecord {
                map_id: self.next_map,
                frame_words: func.frame_words,
                val_is_pointer: func.val_is_pointer,
                slots: func.pointer_slots.clone(),
            });
            self.next_map += 1;
        }
    }

    fn build_entry(&mut self, main_words: u32) -> Function {
        self.blocks.clear();
        self.blocks.push(Block { insts: Vec::new() });
        self.cur = 0;
        let roots = self.roots.clone();
        for (label, index) in roots {
            self.emit(Inst::CallAlloc {
                words: 2,
                tag: TAG_TAGGED,
                map_id: MAP_EMPTY,
                dst: Dest::V(0),
            });
            self.emit(Inst::Imm { dst: Dest::V(1), value: i64::from(label) });
            self.emit(Inst::Store {
                src: Dest::V(1),
                base: Dest::V(0),
                offset: TAGGED_LABEL as i32,
                width: 8,
            });
            self.emit(Inst::Imm { dst: Dest::V(1), value: 0 });
            self.emit(Inst::Store {
                src: Dest::V(1),
                base: Dest::V(0),
                offset: TAGGED_PAYLOAD as i32,
                width: 8,
            });
            self.emit(Inst::StorePool { src: Dest::V(0), index });
        }
        self.emit(Inst::Imm { dst: Dest::Val, value: 0 });
        self.emit(Inst::CallSlc {
            symbol: "main".into(),
            callee_frame_words: main_words,
            arg_is_pointer: false,
        });
        Function {
            symbol: SLC_PROGRAM_ENTRY.to_string(),
            map_id: MAP_EMPTY,
            frame_words: 9,
            val_is_pointer: false,
            pointer_slots: Vec::new(),
            spill_base: 0,
            blocks: std::mem::take(&mut self.blocks),
            entry: true,
        }
    }
}

fn explode_bindings(rows: &mut [Row]) {
    for row in rows {
        for col in 0..row.pats.len() {
            while let Pat::Binding(name, inner) = &row.pats[col] {
                let name = name.clone();
                let inner = inner.as_ref().clone();
                let place = row.places[col].clone();
                row.binds.push((name, place));
                row.pats[col] = inner;
            }
        }
    }
}

fn choose_column(rows: &[Row]) -> usize {
    let width = rows[0].pats.len();
    let mut best = 0;
    let mut best_score = usize::MAX;
    let mut found = false;
    for col in 0..width {
        if irrefutable(&rows[0].pats[col]) {
            continue;
        }
        let score = branch_count(rows, col);
        if score < best_score {
            best_score = score;
            best = col;
            found = true;
        }
    }
    if found { best } else { 0 }
}

fn branch_count(rows: &[Row], col: usize) -> usize {
    let mut seen = Vec::new();
    for row in rows {
        let key = match &row.pats[col] {
            Pat::Tagged(label, _) => label.clone(),
            Pat::Int(value) => value.to_string(),
            Pat::Wildcard | Pat::Binding(_, _) => continue,
            other => format!("{other:?}"),
        };
        if !seen.contains(&key) {
            seen.push(key);
        }
    }
    seen.len().max(1)
}

fn column_kind(rows: &[Row], col: usize) -> Result<Kind, String> {
    let mut test = false;
    let mut tagged = false;
    let mut tuple = false;
    for row in rows {
        match &row.pats[col] {
            Pat::Wildcard | Pat::Binding(_, _) => {}
            Pat::Int(_)
            | Pat::Float(_)
            | Pat::Str(_)
            | Pat::Char(_)
            | Pat::Range(_, _)
            | Pat::Or(_) => test = true,
            Pat::Tagged(_, _) => tagged = true,
            Pat::Tuple(_) => tuple = true,
        }
    }
    if test {
        Ok(Kind::Test)
    } else if tuple && !tagged {
        Ok(Kind::Tuple)
    } else if tagged && !tuple {
        Ok(Kind::Switch)
    } else {
        Err("mixed match column".into())
    }
}

/// Success keeps every still-active row that accepts `outcome`, in source order.
/// The column becomes a wildcard when the row accepts every such value, so the
/// tested row is not tested again. Failure keeps rows that can still match
/// something `outcome` rejects.
fn split_outcome(rows: &[Row], col: usize, outcome: &Pat) -> (Vec<Row>, Vec<Row>) {
    let mut success = Vec::new();
    let mut failure = Vec::new();
    for row in rows {
        let pat = &row.pats[col];
        if intersects(pat, outcome) {
            let mut taken = row.clone();
            if subsumes(pat, outcome) || pat == outcome {
                taken.pats[col] = Pat::Wildcard;
            }
            success.push(taken);
        }
        if !subsumes(outcome, pat) {
            failure.push(row.clone());
        }
    }
    (success, failure)
}

fn peel_binds(pat: &Pat) -> (Pat, Vec<String>) {
    let mut names = Vec::new();
    let mut pat = pat.clone();
    while let Pat::Binding(name, inner) = pat {
        names.push(name);
        pat = *inner;
    }
    (pat, names)
}

fn int_bounds(pat: &Pat) -> Option<(i64, i64)> {
    match pat {
        Pat::Int(value) => Some((*value, *value)),
        Pat::Range(lo, hi) => match (lo.as_ref(), hi.as_ref()) {
            (Pat::Int(lo), Pat::Int(hi)) => Some((*lo.min(hi), *lo.max(hi))),
            _ => None,
        },
        Pat::Binding(_, inner) => int_bounds(inner),
        _ => None,
    }
}

/// Every value that matches `inner` matches `outer`.
fn subsumes(outer: &Pat, inner: &Pat) -> bool {
    match inner {
        Pat::Binding(_, pattern) => return subsumes(outer, pattern),
        Pat::Or(alts) => return alts.iter().all(|alt| subsumes(outer, alt)),
        _ => {}
    }
    match outer {
        Pat::Wildcard => true,
        Pat::Binding(_, pattern) => subsumes(pattern, inner),
        Pat::Or(alts) => alts.iter().any(|alt| subsumes(alt, inner)),
        Pat::Int(value) => int_bounds(inner) == Some((*value, *value)),
        Pat::Range(lo, hi) => match (lo.as_ref(), hi.as_ref(), int_bounds(inner)) {
            (Pat::Int(lo), Pat::Int(hi), Some((start, end))) => {
                *lo.min(hi) <= start && end <= *lo.max(hi)
            }
            _ => false,
        },
        Pat::Tagged(label, fields) => match inner {
            Pat::Tagged(other, inner_fields) => {
                label == other
                    && fields.len() == inner_fields.len()
                    && fields.iter().zip(inner_fields).all(|(outer, inner)| subsumes(outer, inner))
            }
            _ => false,
        },
        Pat::Tuple(fields) => match inner {
            Pat::Tuple(inner_fields) => {
                fields.len() == inner_fields.len()
                    && fields.iter().zip(inner_fields).all(|(outer, inner)| subsumes(outer, inner))
            }
            _ => false,
        },
        Pat::Float(_) | Pat::Str(_) | Pat::Char(_) => outer == inner,
    }
}

fn intersects(left: &Pat, right: &Pat) -> bool {
    match (left, right) {
        (Pat::Wildcard, _) | (_, Pat::Wildcard) => true,
        (Pat::Binding(_, pattern), right) => intersects(pattern, right),
        (left, Pat::Binding(_, pattern)) => intersects(left, pattern),
        (Pat::Or(alts), right) => alts.iter().any(|alt| intersects(alt, right)),
        (left, Pat::Or(alts)) => alts.iter().any(|alt| intersects(left, alt)),
        (Pat::Tagged(label, fields), Pat::Tagged(other, inner_fields)) => {
            label == other
                && fields.len() == inner_fields.len()
                && fields.iter().zip(inner_fields).all(|(left, right)| intersects(left, right))
        }
        (Pat::Tuple(fields), Pat::Tuple(inner_fields)) => {
            fields.len() == inner_fields.len()
                && fields.iter().zip(inner_fields).all(|(left, right)| intersects(left, right))
        }
        _ => match (int_bounds(left), int_bounds(right)) {
            (Some((lo, hi)), Some((start, end))) => lo <= end && start <= hi,
            _ => false,
        },
    }
}

fn project(row: &Row, col: usize, fields: &[Pat]) -> Row {
    let mut row = row.clone();
    if fields.is_empty() {
        row.pats.remove(col);
        row.places.remove(col);
        return row;
    }
    let places = vec![row.places[col].clone(); fields.len()];
    row.pats.splice(col..=col, fields.to_vec());
    row.places.splice(col..=col, places);
    row
}

fn drop_col(row: &Row, col: usize) -> Row {
    let mut row = row.clone();
    if col < row.pats.len() {
        row.pats.remove(col);
        row.places.remove(col);
    }
    row
}
