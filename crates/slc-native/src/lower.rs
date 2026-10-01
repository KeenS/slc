//! Straight-line code, both match forms, and escaping captures.
//! Non-escaping `μ` stays `CallSlc`, `Tail`, or `Ret`.

use std::collections::{HashMap, HashSet};

use slc_abi::{
    CLOSURE_CODE, CLOSURE_ENV, CLOSURE_FRAME_WORDS, FRAME_FRAME_WORDS, FRAME_SLOT0, MAP_EMPTY,
    SLC_PROGRAM_ENTRY, STRING_BYTE_LEN, STRING_BYTES, STRING_CHAR_LEN, TAG_CLAUSES, TAG_CLOSURE,
    TAG_DELAY, TAG_ENV, TAG_STRING, TAG_TAGGED, TAG_TUPLE, TAGGED_LABEL, TAGGED_PAYLOAD,
};
use slc_core::command::Command;
use slc_core::coterm::{CoCaseBranch, CoTerm};
use slc_core::substitution::free_vars_term;
use slc_core::term::{DELAY_BINDER, Term};
use slc_core::types::{Base, Type};
use slc_syntax::lower::Specialization;
use slc_syntax::pattern::{self, Descriptor, Pat};

use crate::{Block, Cond, Dest, Function, Inst, MapRecord, Module, RtArg};

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
    /// Co-variables whose cut is a return. `__tail` starts here.
    forwards: HashSet<String>,
    /// Names used as co-terms in the function being compiled. Those slots are pointers.
    consumers: HashSet<String>,
    lifted: Vec<Function>,
    lift_index: u32,
    type_stack: Vec<HashMap<String, Type>>,
    apply_scalar: u32,
    apply_ptr: u32,
    prompt_map: u32,
    cont_map: u32,
    /// `main`'s body is `λexit`. Entry calls that closure with the exit stub.
    proc_main: bool,
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
    funcs.append(&mut builder.lifted);
    builder.push_io(&mut funcs);
    inflate(&mut funcs, &builder.edges);
    builder.assign_maps(&mut funcs);
    patch(&mut funcs);
    let main_words = funcs
        .iter()
        .find(|func| func.symbol == "main")
        .map(|func| func.frame_words)
        .ok_or_else(|| "no main".to_string())?;
    funcs.push(builder.build_entry(main_words, &funcs));
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
                    Inst::SymWords { dst, symbol } => {
                        let value = i64::from(sizes[symbol]);
                        *inst = Inst::Imm { dst: *dst, value };
                    }
                    Inst::SymMap { dst, symbol } => {
                        let value = i64::from(maps[symbol]);
                        *inst = Inst::Imm { dst: *dst, value };
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
    matches!(
        insts.last(),
        Some(
            Inst::Ret
                | Inst::Jmp { .. }
                | Inst::Tail { .. }
                | Inst::Ud2
                | Inst::Invoke { .. }
                | Inst::Resume { .. }
                | Inst::Adapt { .. }
                | Inst::InstallPrompt { .. }
                | Inst::Perform { tail: true, .. }
                | Inst::CallClosure { tail: true, .. }
                | Inst::Activate { tail: true, .. }
                | Inst::CallRt { noreturn: true, .. }
        )
    )
}

struct Suspended {
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
    val_ptr: bool,
    val_ty: Option<Type>,
    forwards: HashSet<String>,
    consumers: HashSet<String>,
}

/// `lem`: a tagged consumer whose body names the continuation being captured.
struct Lem<'a> {
    label: &'a str,
    param: &'a str,
    body: &'a Command,
}

fn hand_fn(
    symbol: &str,
    frame_words: u32,
    val_is_pointer: bool,
    pointer_slots: &[u16],
    spill_base: u16,
    insts: Vec<Inst>,
) -> Function {
    Function {
        symbol: symbol.to_string(),
        map_id: 0,
        frame_words,
        val_is_pointer,
        pointer_slots: pointer_slots.to_vec(),
        spill_base,
        blocks: vec![Block { insts }],
        entry: false,
    }
}

fn func_layout(funcs: &[Function], symbol: &str) -> (u32, u32) {
    let func = funcs.iter().find(|func| func.symbol == symbol).unwrap_or_else(|| {
        panic!("missing {symbol}");
    });
    (func.frame_words, func.map_id)
}

/// `$str_` payload, quotes included. Same unescape order as the interpreter.
fn decode_lit(text: &str) -> String {
    let text = text.strip_prefix('"').and_then(|text| text.strip_suffix('"')).unwrap_or(text);
    text.replace("\\n", "\n").replace("\\t", "\t").replace("\\\"", "\"").replace("\\\\", "\\")
}

fn escapes(name: &str, command: &Command) -> bool {
    escape_command(name, command, false)
}

/// `inside` is a `λ`, a delay, or a constructor. A co-variable use there is closed over.
fn escape_command(name: &str, command: &Command, inside: bool) -> bool {
    let Command::Cut(term, coterm) = command;
    escape_term(name, term, inside) || escape_coterm(name, coterm, inside)
}

fn escape_term(name: &str, term: &Term, inside: bool) -> bool {
    match term {
        Term::Var(var) => var == name,
        Term::Lam(binder, body) => binder != name && escape_term(name, body, true),
        // A nested `μ` is still this frame. It does not by itself close over `name`.
        Term::Mu(binder, command) => binder != name && escape_command(name, command, inside),
        Term::Tag(_, payload) => escape_term(name, payload, true),
        Term::Tuple(items) => items.iter().any(|item| escape_term(name, item, true)),
        Term::Co(coterm) => escape_coterm(name, coterm, true),
        Term::CoMatch { branches, .. } => branches
            .iter()
            .any(|branch| branch.binder != name && escape_command(name, &branch.body, true)),
    }
}

fn escape_coterm(name: &str, coterm: &CoTerm, inside: bool) -> bool {
    match coterm {
        CoTerm::Covar(var) => inside && var == name,
        CoTerm::App(term, next) => {
            escape_term(name, term, inside) || escape_coterm(name, next, inside)
        }
        CoTerm::MuTilde(binder, command) => binder != name && escape_command(name, command, inside),
        CoTerm::MuTildeTensor(binders, command) => {
            !binders.iter().any(|binder| binder == name) && escape_command(name, command, inside)
        }
        CoTerm::Prj(_) => false,
        CoTerm::Dtor(_, next) => escape_coterm(name, next, inside),
        CoTerm::CoCase { branches, .. } => branches.iter().any(|branch| {
            !branch.binders.iter().any(|binder| binder == name)
                && escape_command(name, &branch.body, inside)
        }),
    }
}

/// Co-variable uses, with binders of this term removed so a nested parameter
/// does not mark a slot of the enclosing frame.
fn covar_names(term: &Term) -> HashSet<String> {
    let mut out = HashSet::new();
    covar_term(term, &mut out);
    out
}

fn covar_term(term: &Term, out: &mut HashSet<String>) {
    match term {
        Term::Var(_) => {}
        Term::Lam(binder, body) => {
            let mut inner = HashSet::new();
            covar_term(body, &mut inner);
            inner.remove(binder);
            out.extend(inner);
        }
        Term::Mu(binder, command) => {
            let mut inner = HashSet::new();
            covar_command(command, &mut inner);
            inner.remove(binder);
            out.extend(inner);
        }
        Term::Tuple(items) => {
            for item in items {
                covar_term(item, out);
            }
        }
        Term::Tag(_, payload) => covar_term(payload, out),
        Term::Co(coterm) => covar_coterm(coterm, out),
        Term::CoMatch { branches, .. } => {
            for branch in branches {
                let mut inner = HashSet::new();
                covar_command(&branch.body, &mut inner);
                inner.remove(&branch.binder);
                out.extend(inner);
            }
        }
    }
}

fn covar_coterm(coterm: &CoTerm, out: &mut HashSet<String>) {
    match coterm {
        CoTerm::Covar(name) => {
            out.insert(name.clone());
        }
        CoTerm::App(term, next) => {
            covar_term(term, out);
            covar_coterm(next, out);
        }
        CoTerm::MuTilde(binder, command) => {
            let mut inner = HashSet::new();
            covar_command(command, &mut inner);
            inner.remove(binder);
            out.extend(inner);
        }
        CoTerm::MuTildeTensor(binders, command) => {
            let mut inner = HashSet::new();
            covar_command(command, &mut inner);
            for binder in binders {
                inner.remove(binder);
            }
            out.extend(inner);
        }
        CoTerm::Prj(_) => {}
        CoTerm::Dtor(_, next) => covar_coterm(next, out),
        CoTerm::CoCase { branches, .. } => {
            for branch in branches {
                let mut inner = HashSet::new();
                covar_command(&branch.body, &mut inner);
                for binder in &branch.binders {
                    inner.remove(binder);
                }
                out.extend(inner);
            }
        }
    }
}

fn covar_command(command: &Command, out: &mut HashSet<String>) {
    let Command::Cut(term, coterm) = command;
    covar_term(term, out);
    covar_coterm(coterm, out);
}

fn param_is_pointer(param: &str, body: &Term) -> bool {
    if param == DELAY_BINDER
        || param == "__handle_thunk"
        || param == "__op_arg"
        || param == "__no_args"
        || param == "__unused"
    {
        return false;
    }
    if covar_names(body).contains(param) || matches!(body, Term::Var(name) if name == param) {
        return true;
    }
    cuts_tensor(param, body)
}

fn cuts_tensor(param: &str, term: &Term) -> bool {
    match term {
        Term::Var(_) => false,
        Term::Lam(binder, body) => binder != param && cuts_tensor(param, body),
        Term::Mu(binder, command) => binder != param && cuts_command_tensor(param, command),
        Term::Tuple(items) => items.iter().any(|item| cuts_tensor(param, item)),
        Term::Tag(_, payload) => cuts_tensor(param, payload),
        Term::Co(coterm) => cuts_coterm_tensor(param, coterm),
        Term::CoMatch { branches, .. } => branches
            .iter()
            .any(|branch| branch.binder != param && cuts_command_tensor(param, &branch.body)),
    }
}

fn cuts_command_tensor(param: &str, command: &Command) -> bool {
    let Command::Cut(term, coterm) = command;
    if let (Term::Var(var), CoTerm::MuTildeTensor(_, _)) = (term, coterm)
        && var == param
    {
        return true;
    }
    cuts_tensor(param, term) || cuts_coterm_tensor(param, coterm)
}

fn cuts_coterm_tensor(param: &str, coterm: &CoTerm) -> bool {
    match coterm {
        CoTerm::Covar(_) | CoTerm::Prj(_) => false,
        CoTerm::App(term, next) => cuts_tensor(param, term) || cuts_coterm_tensor(param, next),
        CoTerm::MuTilde(binder, command) => binder != param && cuts_command_tensor(param, command),
        CoTerm::MuTildeTensor(binders, command) => {
            !binders.iter().any(|binder| binder == param) && cuts_command_tensor(param, command)
        }
        CoTerm::Dtor(_, next) => cuts_coterm_tensor(param, next),
        CoTerm::CoCase { branches, .. } => branches.iter().any(|branch| {
            !branch.binders.iter().any(|binder| binder == param)
                && cuts_command_tensor(param, &branch.body)
        }),
    }
}

/// `__handle` after `call_curried`: clauses first, then the thunk.
fn handle_parts(term: &Term) -> Option<(&Term, &Term)> {
    let Term::Mu(outer, outer_cmd) = term else { return None };
    if outer != "__call" {
        return None;
    }
    let Command::Cut(Term::Mu(inner, inner_cmd), CoTerm::App(thunk, outer_cont)) =
        outer_cmd.as_ref()
    else {
        return None;
    };
    if inner != "__call" {
        return None;
    }
    let CoTerm::Covar(outer_name) = outer_cont.as_ref() else { return None };
    if outer_name != "__call" {
        return None;
    }
    let Command::Cut(Term::Var(handle), CoTerm::App(clauses, inner_cont)) = inner_cmd.as_ref()
    else {
        return None;
    };
    if handle != "__handle" {
        return None;
    }
    let CoTerm::Covar(inner_name) = inner_cont.as_ref() else { return None };
    if inner_name != "__call" {
        return None;
    }
    Some((clauses, thunk))
}

fn lem_shape<'a>(name: &str, command: &'a Command) -> Option<Lem<'a>> {
    let mut command = command;
    loop {
        match command {
            Command::Cut(Term::Mu(_, inner), CoTerm::Covar(cov)) if cov == name => {
                command = inner;
            }
            Command::Cut(Term::Tag(label, payload), CoTerm::Covar(cov)) if cov == name => {
                let Term::Co(coterm) = payload.as_ref() else { return None };
                let CoTerm::MuTilde(param, body) = coterm.as_ref() else { return None };
                let body = body.as_ref();
                let mentioned =
                    free_vars_term(&Term::Mu(String::new(), Box::new(body.clone()))).contains(name);
                if !mentioned {
                    return None;
                }
                return Some(Lem { label, param, body });
            }
            _ => return None,
        }
    }
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
            forwards: HashSet::new(),
            consumers: HashSet::new(),
            lifted: Vec::new(),
            lift_index: 0,
            type_stack: Vec::new(),
            apply_scalar: 0,
            apply_ptr: 0,
            prompt_map: 0,
            cont_map: 0,
            proc_main: false,
        };
        builder.prepare_maps();
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
        for frame in self.type_stack.iter().rev() {
            if let Some(ty) = frame.get(name) {
                return Some(ty);
            }
        }
        self.fn_types.get(&self.current)?.get(name)
    }

    /// Prompt, continuation, and apply frames are fixed before any function map is numbered.
    fn prepare_maps(&mut self) {
        self.apply_scalar = self.push_frame_map(11, false, &[0, 1]);
        self.apply_ptr = self.push_frame_map(11, true, &[0, 1]);
        self.prompt_map = self.push_frame_map(13, true, &[0, 1, 2]);
        self.cont_map = self.push_frame_map(9, true, &[]);
    }

    fn push_frame_map(&mut self, words: u32, val_is_pointer: bool, slots: &[u16]) -> u32 {
        let id = self.next_map;
        self.next_map += 1;
        self.maps.push(MapRecord {
            map_id: id,
            frame_words: words,
            val_is_pointer,
            slots: slots.to_vec(),
        });
        id
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
        if computed || self.word_ptr(name) || self.consumers.contains(name) {
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
        // A proc is two lambdas. The outer one returns the closure entry calls with `exit`.
        if symbol == "main" && matches!(body.as_ref(), Term::Lam(inner, _) if inner != DELAY_BINDER)
        {
            self.proc_main = true;
        }
        let param_ptr =
            self.func_param.get(symbol).copied().unwrap_or_else(|| param_is_pointer(param, body));
        self.compile_function(symbol, param, param_ptr, body, &[])
    }

    fn collect_term(&mut self, term: &Term) {
        match term {
            // A nested λ is its own frame. Its binders are not slots of this one.
            Term::Lam(_, _) => {}
            Term::Mu(name, command) => {
                if escapes(name, command) {
                    self.add_slot(name);
                }
                self.collect_command(command);
            }
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
            // Arm bodies are compiled inline, so their binders belong to this frame.
            if let Term::Tag(label, inner) = item
                && label == "__match_arm"
                && let Term::Tuple(pair) = inner.as_ref()
                && let Some(Term::Lam(_, body)) = pair.get(1)
            {
                self.collect_term(body);
            } else {
                self.collect_term(item);
            }
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
        if let Some((clauses, thunk)) = handle_parts(term) {
            return self.compile_handle(clauses, thunk, mode);
        }
        match term {
            Term::Var(name) => self.compile_var(name, mode),
            Term::Lam(name, body) if name == DELAY_BINDER => self.compile_delay(body),
            Term::Lam(param, body) => self.compile_lambda(param, body, mode),
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
            CoTerm::Covar(name) => self.deliver(term, name, mode),
            CoTerm::CoCase { branches, .. } => self.compile_cocase(term, branches, mode),
            CoTerm::MuTildeTensor(binders, body) => self.compile_tensor(term, binders, body, mode),
            CoTerm::MuTilde(binder, body) => self.compile_bind(term, binder, body, mode),
            CoTerm::App(_, _) | CoTerm::Prj(_) | CoTerm::Dtor(_, _) => {
                Err("not a straight-line command".into())
            }
        }
    }

    fn compile_mu(&mut self, mu: &str, command: &Command, mode: Mode) -> Result<bool, String> {
        if let Some(lem) = lem_shape(mu, command) {
            return self.compile_lem(mu, lem);
        }
        if escapes(mu, command) {
            let Some(&slot) = self.slots.get(mu) else {
                return Err(format!("escaping {mu} has no slot"));
            };
            // The copy is taken before the slot is stored, so the image does not alias it.
            self.emit(Inst::Capture { dst: Dest::Val });
            self.store_slot(Dest::Val, slot);
            self.mark_word(mu, slot, true);
        }
        // A cut against this binder is still the μ's own return. The heap copy is for later uses.
        let fresh = self.forwards.insert(mu.to_string());
        let result = self.compile_mu_command(mu, command, mode);
        if fresh {
            self.forwards.remove(mu);
        }
        result
    }

    fn compile_mu_command(
        &mut self,
        mu: &str,
        command: &Command,
        mode: Mode,
    ) -> Result<bool, String> {
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
            CoTerm::Covar(name) => self.deliver(value, name, mode),
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
        if terminated(&self.blocks[self.cur].insts) {
            return Ok(pointer);
        }
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
        } else if let Some(text) = name.strip_prefix("$str_") {
            self.emit_string(&decode_lit(text))?;
            true
        } else if let Some(&index) = self.pool.get(name) {
            self.emit(Inst::LeaPool { dst: Dest::V(0), index });
            self.emit(Inst::Load { dst: Dest::Val, base: Dest::V(0), offset: 0, width: 8 });
            true
        } else if self.func_param.contains_key(name) {
            // A known function used as a value is a closure, not a call.
            self.emit_code_object(name, &[], 4, true)?;
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

    fn compile_delay(&mut self, body: &Term) -> Result<bool, String> {
        let symbol = format!("{}__delay_{}", self.current, self.lift_index);
        self.lift_index += 1;
        let captures = self.capture_list(body);
        // The body is not entered here. Force jumps to it with the caller's handlers.
        self.lift_function(&symbol, DELAY_BINDER, false, body, &captures)?;
        self.emit_code_object(&symbol, &captures, 3, false)?;
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
        if symbol == "__gt" {
            return self.compile_gt(arg, mode);
        }
        if symbol == "$force" {
            return self.compile_force(arg, mode);
        }
        if symbol == "$adapt" {
            return self.compile_adapt(arg);
        }
        if !self.func_param.contains_key(symbol) {
            if self.slots.contains_key(symbol) {
                return self.compile_indirect(symbol, arg, mode);
            }
            let pointer = self.compile_term(arg, Mode::Value)?;
            if terminated(&self.blocks[self.cur].insts) {
                return Ok(pointer);
            }
            let op = self.intern(symbol);
            return self.emit_perform(op, mode == Mode::Tail, pointer);
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

    fn compile_function(
        &mut self,
        symbol: &str,
        param: &str,
        param_ptr: bool,
        body: &Term,
        captures: &[(String, bool)],
    ) -> Result<Function, String> {
        self.current = symbol.to_string();
        self.slots.clear();
        self.pointer_slots.clear();
        self.blocks.clear();
        self.join = None;
        self.temp_used = 0;
        self.arm_bodies.clear();
        self.val_ty = None;
        self.forwards.clear();
        self.forwards.insert("__tail".to_string());
        self.consumers = covar_names(body);
        for (name, ptr) in captures {
            self.add_slot(name);
            if *ptr && let Some(&slot) = self.slots.get(name) {
                self.pointer_slots.push(slot);
            }
        }
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
        self.val_ptr = param_ptr;
        if param_ptr && let Some(&slot) = self.slots.get(param) {
            self.pointer_slots.push(slot);
        }
        let frame_words = 9 + u32::from(slot_count) + 1;
        self.blocks.push(Block { insts: Vec::new() });
        self.cur = 0;
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
        if let Some(&slot) = self.slots.get(param) {
            self.store_slot(Dest::Val, slot);
        }
        // Captures come from `r14` and must land before the first poll.
        self.load_captures(captures);
        self.emit(Inst::Safepoint { map_id: 0 });
        let types = self.fn_types.get(symbol).cloned().unwrap_or_default();
        self.type_stack.push(types);
        let compiled = self.compile_term(body, Mode::Tail);
        self.type_stack.pop();
        compiled?;
        self.finish(Mode::Tail);
        // A tail perform of a pointer stores that word in a scratch. The scratch
        // is already a root, so the prologue map stays the parameter's bit.
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

    fn load_captures(&mut self, captures: &[(String, bool)]) {
        if captures.is_empty() {
            return;
        }
        if captures.len() == 1 && captures[0].1 {
            if let Some(&slot) = self.slots.get(&captures[0].0) {
                self.store_slot(Dest::Env, slot);
            }
            return;
        }
        for (index, (name, _)) in captures.iter().enumerate() {
            if let Some(&slot) = self.slots.get(name) {
                self.emit(Inst::Load {
                    dst: Dest::V(0),
                    base: Dest::Env,
                    offset: 16 + 8 * index as i32,
                    width: 8,
                });
                self.store_slot(Dest::V(0), slot);
            }
        }
    }

    fn lift_function(
        &mut self,
        symbol: &str,
        param: &str,
        param_ptr: bool,
        body: &Term,
        captures: &[(String, bool)],
    ) -> Result<(), String> {
        let saved = self.suspend();
        let result = self.compile_function(symbol, param, param_ptr, body, captures);
        self.restore(saved);
        self.lifted.push(result?);
        Ok(())
    }

    fn suspend(&mut self) -> Suspended {
        Suspended {
            slots: std::mem::take(&mut self.slots),
            pointer_slots: std::mem::take(&mut self.pointer_slots),
            blocks: std::mem::take(&mut self.blocks),
            cur: self.cur,
            join: self.join.take(),
            scratch_base: self.scratch_base,
            scratch_top: self.scratch_top,
            temp_base: self.temp_base,
            temp_used: self.temp_used,
            arm_bodies: std::mem::take(&mut self.arm_bodies),
            current: std::mem::take(&mut self.current),
            val_ptr: self.val_ptr,
            val_ty: self.val_ty.take(),
            forwards: std::mem::take(&mut self.forwards),
            consumers: std::mem::take(&mut self.consumers),
        }
    }

    fn restore(&mut self, saved: Suspended) {
        self.slots = saved.slots;
        self.pointer_slots = saved.pointer_slots;
        self.blocks = saved.blocks;
        self.cur = saved.cur;
        self.join = saved.join;
        self.scratch_base = saved.scratch_base;
        self.scratch_top = saved.scratch_top;
        self.temp_base = saved.temp_base;
        self.temp_used = saved.temp_used;
        self.arm_bodies = saved.arm_bodies;
        self.current = saved.current;
        self.val_ptr = saved.val_ptr;
        self.val_ty = saved.val_ty;
        self.forwards = saved.forwards;
        self.consumers = saved.consumers;
    }

    fn capture_list(&self, term: &Term) -> Vec<(String, bool)> {
        let mut names: Vec<String> =
            free_vars_term(term).into_iter().filter(|name| self.slots.contains_key(name)).collect();
        names.sort();
        let covars = covar_names(term);
        names
            .into_iter()
            .map(|name| {
                let slot = self.slots[&name];
                let ptr = self.pointer_slots.contains(&slot)
                    || self.word_ptr(&name)
                    || covars.contains(&name);
                (name, ptr)
            })
            .collect()
    }

    fn compile_lambda(&mut self, param: &str, body: &Term, mode: Mode) -> Result<bool, String> {
        let symbol = format!("{}__lam_{}", self.current, self.lift_index);
        self.lift_index += 1;
        let whole = Term::Lam(param.to_string(), Box::new(body.clone()));
        let captures = self.capture_list(&whole);
        let param_ptr = param_is_pointer(param, body);
        self.lift_function(&symbol, param, param_ptr, body, &captures)?;
        self.emit_code_object(&symbol, &captures, 4, true)?;
        if mode == Mode::Tail {
            self.finish(Mode::Tail);
        }
        Ok(true)
    }

    /// `words` is 4 for a closure (code, env, frame size, map) and 3 for a delay.
    fn emit_code_object(
        &mut self,
        symbol: &str,
        captures: &[(String, bool)],
        words: u32,
        closure: bool,
    ) -> Result<(), String> {
        let env_slot = self.materialize_env(captures)?;
        let tag = if closure { TAG_CLOSURE } else { TAG_DELAY };
        self.emit(Inst::CallAlloc { words, tag, map_id: MAP_EMPTY, dst: Dest::V(0) });
        self.emit(Inst::LeaSym { dst: Dest::V(1), symbol: symbol.to_string() });
        self.emit(Inst::Store {
            src: Dest::V(1),
            base: Dest::V(0),
            offset: CLOSURE_CODE as i32,
            width: 8,
        });
        if let Some(slot) = env_slot {
            self.load_slot(Dest::V(1), slot);
        } else {
            self.emit(Inst::Imm { dst: Dest::V(1), value: 0 });
        }
        self.emit(Inst::Store {
            src: Dest::V(1),
            base: Dest::V(0),
            offset: CLOSURE_ENV as i32,
            width: 8,
        });
        self.emit(Inst::SymWords { dst: Dest::V(1), symbol: symbol.to_string() });
        self.emit(Inst::Store {
            src: Dest::V(1),
            base: Dest::V(0),
            offset: CLOSURE_FRAME_WORDS as i32,
            width: 8,
        });
        if closure {
            // The collector reads this id when the closure's frame is entered.
            self.emit(Inst::SymMap { dst: Dest::V(1), symbol: symbol.to_string() });
            self.emit(Inst::Store { src: Dest::V(1), base: Dest::V(0), offset: 40, width: 8 });
        }
        self.emit(Inst::Mov { dst: Dest::Val, src: Dest::V(0) });
        if env_slot.is_some() {
            self.pop_scratch();
        }
        Ok(())
    }

    fn materialize_env(&mut self, captures: &[(String, bool)]) -> Result<Option<u16>, String> {
        if captures.is_empty() {
            return Ok(None);
        }
        if captures.len() == 1 && captures[0].1 {
            let slot = self.slots[&captures[0].0];
            self.load_slot(Dest::Val, slot);
            return Ok(Some(self.push_scratch(Dest::Val)?));
        }
        let mut ptrs = Vec::new();
        let mut saved = Vec::new();
        for (index, (name, ptr)) in captures.iter().enumerate() {
            let slot = self.slots[name];
            self.load_slot(Dest::Val, slot);
            let hold = if *ptr {
                ptrs.push(index as u16);
                self.push_scratch(Dest::Val)?
            } else {
                let temp = self.alloc_temp(false)?;
                self.store_slot(Dest::Val, temp);
                temp
            };
            saved.push((hold, *ptr));
        }
        let map = self.heap_map(&ptrs);
        self.emit(Inst::CallAlloc {
            words: captures.len() as u32,
            tag: TAG_ENV,
            map_id: map,
            dst: Dest::V(0),
        });
        for (index, (hold, _)) in saved.iter().enumerate() {
            self.load_slot(Dest::V(1), *hold);
            self.emit(Inst::Store {
                src: Dest::V(1),
                base: Dest::V(0),
                offset: 16 + 8 * index as i32,
                width: 8,
            });
        }
        for (_, ptr) in saved.iter().rev() {
            if *ptr {
                self.pop_scratch();
            }
        }
        self.emit(Inst::Mov { dst: Dest::Val, src: Dest::V(0) });
        Ok(Some(self.push_scratch(Dest::Val)?))
    }

    fn emit_string(&mut self, text: &str) -> Result<(), String> {
        let bytes = text.as_bytes();
        let chunks = bytes.len().div_ceil(8);
        self.emit(Inst::CallAlloc {
            words: 2 + chunks as u32,
            tag: TAG_STRING,
            map_id: MAP_EMPTY,
            dst: Dest::V(0),
        });
        self.emit(Inst::Imm { dst: Dest::V(1), value: bytes.len() as i64 });
        self.emit(Inst::Store {
            src: Dest::V(1),
            base: Dest::V(0),
            offset: STRING_BYTE_LEN as i32,
            width: 8,
        });
        self.emit(Inst::Imm { dst: Dest::V(1), value: text.chars().count() as i64 });
        self.emit(Inst::Store {
            src: Dest::V(1),
            base: Dest::V(0),
            offset: STRING_CHAR_LEN as i32,
            width: 8,
        });
        for (index, chunk) in bytes.chunks(8).enumerate() {
            let mut word = 0u64;
            for (place, byte) in chunk.iter().enumerate() {
                word |= u64::from(*byte) << (8 * place);
            }
            self.emit(Inst::Imm { dst: Dest::V(1), value: word as i64 });
            self.emit(Inst::Store {
                src: Dest::V(1),
                base: Dest::V(0),
                offset: STRING_BYTES as i32 + 8 * index as i32,
                width: 8,
            });
        }
        self.emit(Inst::Mov { dst: Dest::Val, src: Dest::V(0) });
        Ok(())
    }

    fn deliver(&mut self, value: &Term, covar: &str, mode: Mode) -> Result<bool, String> {
        if self.forwards.contains(covar) {
            return self.compile_term(value, mode);
        }
        let pointer = self.compile_term(value, Mode::Value)?;
        if terminated(&self.blocks[self.cur].insts) {
            return Ok(pointer);
        }
        if let Some(&slot) = self.slots.get(covar) {
            self.emit(Inst::Activate {
                consumer: Dest::Slot(slot),
                tail: mode == Mode::Tail,
                arg_is_pointer: pointer,
            });
            return Ok(false);
        }
        let op = self.intern(covar);
        self.emit_perform(op, mode == Mode::Tail, pointer)
    }

    fn emit_perform(&mut self, op: u32, tail: bool, arg_ptr: bool) -> Result<bool, String> {
        // A pointer payload stays in a scratch, which the frame map already traces.
        // A scalar stays in `r13` and is parked when the parameter map would trace it.
        if arg_ptr {
            self.push_scratch(Dest::Val)?;
        }
        let after = if tail { 0 } else { self.new_block() };
        self.emit(Inst::Perform {
            op,
            tail,
            arg_is_pointer: arg_ptr,
            after,
            apply_map: if arg_ptr { self.apply_ptr } else { self.apply_scalar },
            cont_map: self.cont_map,
        });
        if !tail {
            self.cur = after;
            if arg_ptr {
                self.pop_scratch();
            }
        }
        Ok(false)
    }

    fn compile_gt(&mut self, arg: &Term, mode: Mode) -> Result<bool, String> {
        self.compile_term(arg, Mode::Value)?;
        if terminated(&self.blocks[self.cur].insts) {
            return Ok(false);
        }
        self.emit(Inst::Load { dst: Dest::V(0), base: Dest::Val, offset: 24, width: 8 });
        self.emit(Inst::Load { dst: Dest::V(1), base: Dest::Val, offset: 32, width: 8 });
        let yes = self.new_block();
        let no = self.new_block();
        self.emit(Inst::CmpRR { left: Dest::V(0), right: Dest::V(1), cond: Cond::G, target: yes });
        self.emit(Inst::Jmp { target: no });
        let join = match mode {
            Mode::Value => Some(self.new_block()),
            Mode::Tail => None,
        };
        self.cur = yes;
        self.load_pool("Bool::True")?;
        self.finish_branch(mode, join);
        self.cur = no;
        self.load_pool("Bool::False")?;
        self.finish_branch(mode, join);
        if let Some(join) = join {
            self.cur = join;
        }
        Ok(true)
    }

    fn finish_branch(&mut self, mode: Mode, join: Option<usize>) {
        if terminated(&self.blocks[self.cur].insts) {
            return;
        }
        match mode {
            Mode::Tail => self.emit(Inst::Ret),
            Mode::Value => {
                if let Some(join) = join {
                    self.emit(Inst::Jmp { target: join });
                }
            }
        }
    }

    fn load_pool(&mut self, name: &str) -> Result<(), String> {
        let index = *self.pool.get(name).ok_or_else(|| format!("missing {name}"))?;
        self.emit(Inst::LeaPool { dst: Dest::V(0), index });
        self.emit(Inst::Load { dst: Dest::Val, base: Dest::V(0), offset: 0, width: 8 });
        Ok(())
    }

    fn compile_force(&mut self, arg: &Term, mode: Mode) -> Result<bool, String> {
        // The scratch that holds the delay is traced for the call. This bit is only
        // the forced word. Peel every `Delayed` layer: the tag loop does the same.
        let result_ptr = match arg {
            Term::Var(name) => match self.word_ty(name) {
                Some(ty) => {
                    let mut current = ty;
                    loop {
                        match current {
                            Type::Rowed(inner, _) | Type::Dual(inner) | Type::Delayed(inner, _) => {
                                current = inner
                            }
                            other => break type_is_pointer(other),
                        }
                    }
                }
                None => true,
            },
            _ => true,
        };
        self.compile_term(arg, Mode::Value)?;
        if terminated(&self.blocks[self.cur].insts) {
            return Ok(false);
        }
        let slot = self.push_scratch(Dest::Val)?;
        self.emit(Inst::Force { tail: mode == Mode::Tail, slot });
        if mode != Mode::Tail {
            self.pop_scratch();
        }
        Ok(result_ptr)
    }

    fn compile_adapt(&mut self, arg: &Term) -> Result<bool, String> {
        // Payload slot 1 is the value. The tag rule traces the adapter at offset 16.
        let value_ptr = match arg {
            Term::Tuple(items) => items.get(1).is_none_or(|item| match item {
                Term::Var(name)
                    if name == "$unit"
                        || name.starts_with("$int_")
                        || name.starts_with("$float_")
                        || name.starts_with("$char_") =>
                {
                    false
                }
                Term::Var(name)
                    if name.starts_with("$str_")
                        || self.pool.contains_key(name)
                        || self.func_param.contains_key(name)
                        || self.word_ptr(name) =>
                {
                    true
                }
                Term::Var(name) => {
                    self.slots.get(name).is_some_and(|slot| self.pointer_slots.contains(slot))
                }
                _ => true,
            }),
            _ => true,
        };
        let map_id = if value_ptr { self.heap_map(&[1]) } else { MAP_EMPTY };
        self.compile_term(arg, Mode::Value)?;
        if terminated(&self.blocks[self.cur].insts) {
            return Ok(true);
        }
        let slot = self.push_scratch(Dest::Val)?;
        self.emit(Inst::Adapt { slot, map_id });
        Ok(true)
    }

    fn compile_indirect(&mut self, symbol: &str, arg: &Term, mode: Mode) -> Result<bool, String> {
        let slot = self.slots[symbol];
        let pointer = self.compile_term(arg, Mode::Value)?;
        if terminated(&self.blocks[self.cur].insts) {
            return Ok(pointer);
        }
        if pointer {
            self.push_scratch(Dest::Val)?;
        }
        // A slot may be a closure, a `Resume`, or a `Kont`. The tag decides.
        self.emit(Inst::Activate {
            consumer: Dest::Slot(slot),
            tail: mode == Mode::Tail,
            arg_is_pointer: pointer,
        });
        if pointer && mode != Mode::Tail {
            self.pop_scratch();
        }
        Ok(false)
    }

    fn compile_handle(
        &mut self,
        clauses: &Term,
        thunk: &Term,
        _mode: Mode,
    ) -> Result<bool, String> {
        let Term::Tag(tag, inner) = clauses else {
            return Err("handler clauses".into());
        };
        if tag != "__clauses" {
            return Err("handler clauses".into());
        }
        let Term::Tuple(entries) = inner.as_ref() else {
            return Err("handler clauses".into());
        };
        let saved = self.scratch_top;
        let mut pairs = Vec::new();
        for entry in entries {
            let Term::Tuple(pair) = entry else {
                return Err("clause pair".into());
            };
            let (Term::Var(label), closure) = (&pair[0], &pair[1]) else {
                return Err("clause pair".into());
            };
            let op = decode_lit(label.strip_prefix("$str_").unwrap_or(label));
            self.compile_term(closure, Mode::Value)?;
            if terminated(&self.blocks[self.cur].insts) {
                return Err("clause did not produce a closure".into());
            }
            let slot = self.push_scratch(Dest::Val)?;
            pairs.push((op, slot));
        }
        let ptr_slots: Vec<u16> = (0..pairs.len()).map(|index| 2 + index as u16 * 2).collect();
        let map = if ptr_slots.is_empty() { MAP_EMPTY } else { self.heap_map(&ptr_slots) };
        self.emit(Inst::CallAlloc {
            words: 1 + pairs.len() as u32 * 2,
            tag: TAG_CLAUSES,
            map_id: map,
            dst: Dest::V(0),
        });
        self.emit(Inst::Imm { dst: Dest::V(1), value: pairs.len() as i64 });
        self.emit(Inst::Store { src: Dest::V(1), base: Dest::V(0), offset: 16, width: 8 });
        let mut ret_slot = None;
        for (index, (op, slot)) in pairs.iter().enumerate() {
            let id = i64::from(self.intern(op));
            self.emit(Inst::Imm { dst: Dest::V(1), value: id });
            self.emit(Inst::Store {
                src: Dest::V(1),
                base: Dest::V(0),
                offset: 24 + 16 * index as i32,
                width: 8,
            });
            self.load_slot(Dest::V(1), *slot);
            self.emit(Inst::Store {
                src: Dest::V(1),
                base: Dest::V(0),
                offset: 32 + 16 * index as i32,
                width: 8,
            });
            if op == "return" {
                ret_slot = Some(*slot);
            }
        }
        let Some(ret_slot) = ret_slot else {
            return Err("handler has no return clause".into());
        };
        let clauses_slot = self.push_scratch(Dest::V(0))?;
        self.compile_term(thunk, Mode::Value)?;
        if terminated(&self.blocks[self.cur].insts) {
            return Err("handler thunk".into());
        }
        let thunk_slot = self.push_scratch(Dest::Val)?;
        let done = self.new_block();
        self.emit(Inst::InstallPrompt {
            clauses: Dest::Slot(clauses_slot),
            ret_closure: Dest::Slot(ret_slot),
            thunk: Dest::Slot(thunk_slot),
            done,
            prompt_map: self.prompt_map,
        });
        self.cur = done;
        self.scratch_top = saved;
        Ok(false)
    }

    fn compile_lem(&mut self, name: &str, lem: Lem<'_>) -> Result<bool, String> {
        let symbol = format!("{}__lam_{}", self.current, self.lift_index);
        self.lift_index += 1;
        let body = Term::Mu("__tail".into(), Box::new(lem.body.clone()));
        // One pointer capture: the continuation, filled in after the copy exists.
        let captures = vec![(name.to_string(), true)];
        self.lift_function(&symbol, lem.param, false, &body, &captures)?;
        self.emit_code_object(&symbol, &[], 4, true)?;
        let closure_slot = self.push_scratch(Dest::Val)?;
        let id = self.intern(lem.label);
        let map = self.heap_map(&[1]);
        self.emit(Inst::CallAlloc { words: 2, tag: TAG_TAGGED, map_id: map, dst: Dest::V(0) });
        self.emit(Inst::Imm { dst: Dest::V(1), value: i64::from(id) });
        self.emit(Inst::Store {
            src: Dest::V(1),
            base: Dest::V(0),
            offset: TAGGED_LABEL as i32,
            width: 8,
        });
        self.load_slot(Dest::V(1), closure_slot);
        self.emit(Inst::Store {
            src: Dest::V(1),
            base: Dest::V(0),
            offset: TAGGED_PAYLOAD as i32,
            width: 8,
        });
        self.emit(Inst::Mov { dst: Dest::Val, src: Dest::V(0) });
        let tagged_slot = self.push_scratch(Dest::Val)?;
        let Some(&kslot) = self.slots.get(name) else {
            return Err(format!("escaping {name} has no slot"));
        };
        // The tagged word is already in a traced slot, so the copy holds the cycle.
        self.emit(Inst::Capture { dst: Dest::Val });
        self.store_slot(Dest::Val, kslot);
        self.mark_word(name, kslot, true);
        self.load_slot(Dest::V(0), closure_slot);
        self.emit(Inst::Store {
            src: Dest::Val,
            base: Dest::V(0),
            offset: CLOSURE_ENV as i32,
            width: 8,
        });
        self.load_slot(Dest::Val, tagged_slot);
        self.emit(Inst::Invoke { image: Dest::Slot(kslot) });
        Ok(true)
    }

    fn push_io(&mut self, funcs: &mut Vec<Function>) {
        funcs.push(self.io_return());
        funcs.push(self.io_line_inner());
        funcs.push(self.io_line_outer());
        funcs.push(self.exit_stub());
    }

    fn io_return(&self) -> Function {
        // The string result stays in `r13`. Tracing `VAL` keeps it across the prologue poll.
        hand_fn(
            "slc_io_return",
            10,
            true,
            &[],
            0,
            vec![
                Inst::Imm { dst: Dest::V(0), value: 10 },
                Inst::Store {
                    src: Dest::V(0),
                    base: Dest::Frame,
                    offset: FRAME_FRAME_WORDS as i32,
                    width: 8,
                },
                Inst::Safepoint { map_id: 0 },
                Inst::Ret,
            ],
        )
    }

    fn io_line_inner(&self) -> Function {
        hand_fn(
            "slc_io_line_inner",
            11,
            true,
            &[0],
            1,
            vec![
                Inst::Imm { dst: Dest::V(0), value: 11 },
                Inst::Store {
                    src: Dest::V(0),
                    base: Dest::Frame,
                    offset: FRAME_FRAME_WORDS as i32,
                    width: 8,
                },
                Inst::Imm { dst: Dest::V(0), value: 0 },
                Inst::Store { src: Dest::V(0), base: Dest::Frame, offset: slot_off(0), width: 8 },
                Inst::Store { src: Dest::Val, base: Dest::Frame, offset: slot_off(0), width: 8 },
                Inst::Safepoint { map_id: 0 },
                Inst::CallRt {
                    symbol: "slc_rt_write_line".into(),
                    arg: RtArg::Env,
                    noreturn: false,
                },
                Inst::Imm { dst: Dest::Val, value: 0 },
                Inst::Resume { image: Dest::Slot(0), tail: true },
            ],
        )
    }

    fn io_line_outer(&self) -> Function {
        let symbol = "slc_io_line_inner";
        hand_fn(
            "slc_io_line_outer",
            11,
            true,
            &[0],
            1,
            vec![
                Inst::Imm { dst: Dest::V(0), value: 11 },
                Inst::Store {
                    src: Dest::V(0),
                    base: Dest::Frame,
                    offset: FRAME_FRAME_WORDS as i32,
                    width: 8,
                },
                Inst::Imm { dst: Dest::V(0), value: 0 },
                Inst::Store { src: Dest::V(0), base: Dest::Frame, offset: slot_off(0), width: 8 },
                Inst::Store { src: Dest::Val, base: Dest::Frame, offset: slot_off(0), width: 8 },
                Inst::Safepoint { map_id: 0 },
                Inst::CallAlloc { words: 4, tag: TAG_CLOSURE, map_id: MAP_EMPTY, dst: Dest::V(0) },
                Inst::LeaSym { dst: Dest::V(1), symbol: symbol.into() },
                Inst::Store {
                    src: Dest::V(1),
                    base: Dest::V(0),
                    offset: CLOSURE_CODE as i32,
                    width: 8,
                },
                Inst::Load { dst: Dest::V(1), base: Dest::Frame, offset: slot_off(0), width: 8 },
                Inst::Store {
                    src: Dest::V(1),
                    base: Dest::V(0),
                    offset: CLOSURE_ENV as i32,
                    width: 8,
                },
                Inst::SymWords { dst: Dest::V(1), symbol: symbol.into() },
                Inst::Store {
                    src: Dest::V(1),
                    base: Dest::V(0),
                    offset: CLOSURE_FRAME_WORDS as i32,
                    width: 8,
                },
                Inst::SymMap { dst: Dest::V(1), symbol: symbol.into() },
                Inst::Store { src: Dest::V(1), base: Dest::V(0), offset: 40, width: 8 },
                Inst::Mov { dst: Dest::Val, src: Dest::V(0) },
                Inst::Ret,
            ],
        )
    }

    fn exit_stub(&self) -> Function {
        hand_fn(
            "slc_exit_stub",
            9,
            false,
            &[],
            0,
            vec![
                Inst::Imm { dst: Dest::V(0), value: 9 },
                Inst::Store {
                    src: Dest::V(0),
                    base: Dest::Frame,
                    offset: FRAME_FRAME_WORDS as i32,
                    width: 8,
                },
                Inst::Safepoint { map_id: 0 },
                Inst::CallRt { symbol: "slc_rt_exit".into(), arg: RtArg::Val, noreturn: true },
            ],
        )
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

    /// IO prompt: clauses, return closure, traced scratch, untraced park.
    fn build_entry(&mut self, main_words: u32, funcs: &[Function]) -> Function {
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
        let (ret_words, ret_map) = func_layout(funcs, "slc_io_return");
        let (outer_words, outer_map) = func_layout(funcs, "slc_io_line_outer");
        self.emit_raw_closure("slc_io_return", ret_words, ret_map);
        self.store_slot(Dest::Val, 1);
        self.emit_raw_closure("slc_io_line_outer", outer_words, outer_map);
        self.store_slot(Dest::Val, 2);
        let write_line = self.intern("write_line");
        let ret_op = self.intern("return");
        let clauses_map = self.heap_map(&[2, 4]);
        self.emit(Inst::CallAlloc {
            words: 5,
            tag: TAG_CLAUSES,
            map_id: clauses_map,
            dst: Dest::V(0),
        });
        self.emit(Inst::Imm { dst: Dest::V(1), value: 2 });
        self.emit(Inst::Store { src: Dest::V(1), base: Dest::V(0), offset: 16, width: 8 });
        self.emit(Inst::Imm { dst: Dest::V(1), value: i64::from(write_line) });
        self.emit(Inst::Store { src: Dest::V(1), base: Dest::V(0), offset: 24, width: 8 });
        self.load_slot(Dest::V(1), 2);
        self.emit(Inst::Store { src: Dest::V(1), base: Dest::V(0), offset: 32, width: 8 });
        self.emit(Inst::Imm { dst: Dest::V(1), value: i64::from(ret_op) });
        self.emit(Inst::Store { src: Dest::V(1), base: Dest::V(0), offset: 40, width: 8 });
        self.load_slot(Dest::V(1), 1);
        self.emit(Inst::Store { src: Dest::V(1), base: Dest::V(0), offset: 48, width: 8 });
        self.store_slot(Dest::V(0), 0);
        self.emit(Inst::Imm { dst: Dest::Val, value: 0 });
        self.emit(Inst::CallSlc {
            symbol: "main".into(),
            callee_frame_words: main_words,
            arg_is_pointer: false,
        });
        if self.proc_main {
            // `main` returned `λexit`. A tail jump would reuse the IO prompt as that frame.
            self.store_slot(Dest::Val, 2);
            let (exit_words, exit_map) = func_layout(funcs, "slc_exit_stub");
            self.emit_raw_closure("slc_exit_stub", exit_words, exit_map);
            self.emit(Inst::CallClosure {
                closure: Dest::Slot(2),
                tail: false,
                arg_is_pointer: true,
            });
        }
        Function {
            symbol: SLC_PROGRAM_ENTRY.to_string(),
            map_id: self.prompt_map,
            frame_words: 13,
            val_is_pointer: true,
            pointer_slots: vec![0, 1, 2],
            spill_base: 3,
            blocks: std::mem::take(&mut self.blocks),
            entry: true,
        }
    }

    fn emit_raw_closure(&mut self, symbol: &str, words: u32, map_id: u32) {
        self.emit(Inst::CallAlloc {
            words: 4,
            tag: TAG_CLOSURE,
            map_id: MAP_EMPTY,
            dst: Dest::V(0),
        });
        self.emit(Inst::LeaSym { dst: Dest::V(1), symbol: symbol.to_string() });
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
        self.emit(Inst::Imm { dst: Dest::V(1), value: i64::from(words) });
        self.emit(Inst::Store {
            src: Dest::V(1),
            base: Dest::V(0),
            offset: CLOSURE_FRAME_WORDS as i32,
            width: 8,
        });
        self.emit(Inst::Imm { dst: Dest::V(1), value: i64::from(map_id) });
        self.emit(Inst::Store { src: Dest::V(1), base: Dest::V(0), offset: 40, width: 8 });
        self.emit(Inst::Mov { dst: Dest::Val, src: Dest::V(0) });
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
