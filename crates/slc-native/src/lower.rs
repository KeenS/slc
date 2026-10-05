//! Straight-line code, both match forms, and escaping captures.
//! Non-escaping `μ` stays `CallSlc`, `Tail`, or `Ret`.

use std::collections::{HashMap, HashSet};

use slc_abi::{
    CLOSURE_BIRTH, CLOSURE_CODE, CLOSURE_ENV, CLOSURE_FRAME_WORDS, FRAME_FRAME_WORDS, FRAME_SLOT0,
    MAP_EMPTY, SLC_PROGRAM_ENTRY, STRING_BYTE_LEN, STRING_BYTES, STRING_CHAR_LEN, TAG_CLAUSES,
    TAG_CLOSURE, TAG_DELAY, TAG_ENV, TAG_STRING, TAG_TAGGED, TAG_TUPLE, TAGGED_LABEL,
    TAGGED_PAYLOAD,
};
use slc_core::command::Command;
use slc_core::coterm::{CoCaseBranch, CoTerm};
use slc_core::substitution::free_vars_term;
use slc_core::term::{CoMatchBranch, DELAY_BINDER, Term};
use slc_core::types::{Base, Type};
use slc_syntax::lower::Specialization;
use slc_syntax::pattern::{self, Descriptor, Pat};
use slc_syntax::traits::TraitInfo;

use crate::{BinOp, Block, Cond, Dest, Function, I64Op, Inst, MapRecord, Module, RtArg};

const SCRATCHES: u16 = 32;
const MATCH_TEMPS: u16 = 32;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Value,
    Tail,
}

/// A direct cut leaves the μ. `__tail` is not here: a `let` body cuts there
/// and, in value position, has to keep going.
#[derive(Clone, Copy)]
enum ContExit {
    Join(usize),
    Ret,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Int,
    Float,
    Str,
    Bool,
    Char,
    File,
}

enum CmpKind {
    Words(Cond),
    Float(Cond),
    Labels(Cond),
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
    /// Trait name → method names in declaration order. Dictionary slots use this index.
    method_order: HashMap<String, Vec<String>>,
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
    /// Where a cut against this μ goes. A branch join is not that exit:
    /// otherwise the rest of the μ body still runs.
    cont_exit: HashMap<String, ContExit>,
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
    /// Non-lambda defs whose terms are data. A use compiles the literal.
    literals: HashMap<String, Term>,
    /// Effect operations. Any other unknown callee is unbound, not a perform.
    operations: HashSet<String>,
}

pub fn lower(
    defs: &[(String, Term)],
    specs: &[Specialization],
    payloads: &HashMap<String, Vec<Type>>,
    traits: &TraitInfo,
    operations: &[String],
) -> Result<Module, String> {
    let mut builder = Builder::new(defs, specs, payloads, traits, operations);
    let mut funcs = Vec::new();
    // A use of `__display` as a value is this closure. A call still goes to the builtin.
    builder.func_param.insert("__display".into(), true);
    builder.func_result.insert("__display".into(), true);
    let display = Term::Lam(
        "value".into(),
        Box::new(Term::Mu(
            "__call".into(),
            Box::new(Command::Cut(
                Term::Var("__display".into()),
                CoTerm::App(Term::Var("value".into()), Box::new(CoTerm::Covar("__call".into()))),
            )),
        )),
    );
    funcs.push(builder.lower_fn("__display", &display)?);
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

/// A prelude impl whose body is the matching builtin. Other impls keep their call.
fn prelude_primitive(symbol: &str) -> Option<&'static str> {
    let mut parts = symbol.split('#');
    let trait_name = parts.next()?;
    let key = parts.next()?;
    let method = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let numeric = matches!(key, "i64" | "i32" | "i8" | "u64" | "u32" | "u8" | "f64" | "f32");
    let ordered = numeric || matches!(key, "char" | "String" | "Bool");
    match (trait_name, method) {
        ("Add", "add") if numeric || key == "String" => Some("__add"),
        ("Sub", "sub") if numeric => Some("__sub"),
        ("Mul", "mul") if numeric => Some("__mul"),
        ("Div", "div") if numeric => Some("__div"),
        ("Rem", "rem") if numeric => Some("__rem"),
        ("Neg", "neg") if matches!(key, "i64" | "i32" | "i8" | "f64" | "f32") => Some("__neg"),
        ("Sqrt", "sqrt") if matches!(key, "f64" | "f32") => Some("__sqrt"),
        ("Abs", "abs") if matches!(key, "f64" | "f32") => Some("__abs"),
        ("Floor", "floor") if matches!(key, "f64" | "f32") => Some("__floor"),
        ("Ceil", "ceil") if matches!(key, "f64" | "f32") => Some("__ceil"),
        ("Eq", "eq") if ordered => Some("__eq"),
        ("Eq", "ne") if ordered => Some("__ne"),
        ("Ord", "lt") if ordered => Some("__lt"),
        ("Ord", "gt") if ordered => Some("__gt"),
        ("Ord", "le") if ordered => Some("__le"),
        ("Ord", "ge") if ordered => Some("__ge"),
        _ => None,
    }
}

/// The runtime symbol for a width conversion. Integer destinations with no
/// known class stay on the integer path. A float destination does not guess.
fn width_symbol(name: &str, class: Option<Class>) -> Result<&'static str, String> {
    let from_float = match class {
        Some(Class::Float) => true,
        Some(Class::Int) => false,
        None if matches!(name, "__to_f32" | "__to_f64") => {
            return Err(format!("{name} needs a known integer or float"));
        }
        None => false,
        Some(other) => return Err(format!("{name} of {other:?}")),
    };
    match (name, from_float) {
        ("__to_i8", true) => Ok("slc_rt_f_to_i8"),
        ("__to_i8", false) => Ok("slc_rt_to_i8"),
        ("__to_i32", true) => Ok("slc_rt_f_to_i32"),
        ("__to_i32", false) => Ok("slc_rt_to_i32"),
        ("__to_i64", true) => Ok("slc_rt_f_to_i64"),
        ("__to_i64", false) => Ok("slc_rt_to_i64"),
        ("__to_u8", true) => Ok("slc_rt_f_to_u8"),
        ("__to_u8", false) => Ok("slc_rt_to_u8"),
        ("__to_u32", true) => Ok("slc_rt_f_to_u32"),
        ("__to_u32", false) => Ok("slc_rt_to_u32"),
        ("__to_u64", true) => Ok("slc_rt_f_to_u64"),
        ("__to_u64", false) => Ok("slc_rt_to_u64"),
        ("__to_f64", true) => Ok("slc_rt_f_to_f64"),
        ("__to_f64", false) => Ok("slc_rt_i_to_f64"),
        ("__to_f32", true) => Ok("slc_rt_f_to_f32"),
        ("__to_f32", false) => Ok("slc_rt_i_to_f32"),
        _ => Err(format!("builtin {name}")),
    }
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
    cont_exit: HashMap<String, ContExit>,
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
        hide_map: 0,
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

/// `__enter_poly(thunk)(aware)`. The thunk is the body. `aware` names the
/// concrete operations this call catches; anything else tunnels.
fn poly_thunk(term: &Term) -> Option<(&Term, &Term)> {
    let Term::Mu(outer, outer_cmd) = term else { return None };
    if outer != "__call" {
        return None;
    }
    let Command::Cut(Term::Mu(inner, inner_cmd), CoTerm::App(aware, outer_cont)) =
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
    let Command::Cut(Term::Var(fun), CoTerm::App(thunk, inner_cont)) = inner_cmd.as_ref() else {
        return None;
    };
    if fun != "__enter_poly" {
        return None;
    }
    let CoTerm::Covar(inner_name) = inner_cont.as_ref() else { return None };
    if inner_name != "__call" { None } else { Some((thunk, aware)) }
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
        traits: &TraitInfo,
        operations: &[String],
    ) -> Self {
        let mut builder = Self {
            labels: Vec::new(),
            pool: HashMap::new(),
            pool_len: 0,
            roots: Vec::new(),
            method_order: traits
                .traits
                .iter()
                .map(|(name, methods)| {
                    (name.clone(), methods.iter().map(|method| method.name.clone()).collect())
                })
                .collect(),
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
            cont_exit: HashMap::new(),
            consumers: HashSet::new(),
            lifted: Vec::new(),
            lift_index: 0,
            type_stack: Vec::new(),
            apply_scalar: 0,
            apply_ptr: 0,
            prompt_map: 0,
            cont_map: 0,
            proc_main: false,
            literals: HashMap::new(),
            operations: operations.iter().cloned().collect(),
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
            // A negative function is the consumer it returns, not a `λ`. The
            // interpreter installs that value; a use has to compile it too.
            if !matches!(term, Term::Lam(param, _) if param != DELAY_BINDER) {
                builder.literals.insert(name.clone(), term.clone());
            }
        }
        // `def M = N` where `N` is data. The use compiles the literal.
        loop {
            let mut progressed = false;
            for (name, term) in defs {
                if builder.literals.contains_key(name) {
                    continue;
                }
                if let Term::Var(other) = term
                    && let Some(lit) = builder.literals.get(other).cloned()
                {
                    builder.literals.insert(name.clone(), lit);
                    progressed = true;
                }
            }
            if !progressed {
                break;
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
            return Err(format!("too many live pointers in {}", self.current));
        }
        let slot = self.scratch_top;
        self.scratch_top += 1;
        self.store_slot(src, slot);
        Ok(slot)
    }

    fn pop_scratch(&mut self) {
        self.scratch_top -= 1;
    }

    /// Sibling arms reuse scratch and match temps. Pointer marks stay: one map
    /// covers every safepoint, including the arm that stored the pointer.
    fn restore_arm(&mut self, scratch: u16, temp: u16) {
        self.scratch_top = scratch;
        self.temp_used = temp;
    }

    fn alloc_temp(&mut self, pointer: bool) -> Result<u16, String> {
        if self.temp_used >= MATCH_TEMPS {
            return Err(format!("too many match temps in {}", self.current));
        }
        let slot = self.temp_base + self.temp_used;
        self.temp_used += 1;
        if pointer && !self.pointer_slots.contains(&slot) {
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
            Term::CoMatch { .. } => {}
            // An unread sequencing binder has no slot. A read is a real local.
            Term::Var(name)
                if name.starts_with("__discarded") && !self.slots.contains_key(name) =>
            {
                let index = self.slots.len() as u16;
                self.slots.insert(name.clone(), index);
            }
            Term::Var(_) => {}
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
        // Enter first, so closures this body allocates are born under the barrier.
        if let Some((thunk, aware)) = poly_thunk(term) {
            let aware_ptr = self.compile_term(aware, Mode::Value)?;
            if terminated(&self.blocks[self.cur].insts) {
                return Ok(aware_ptr);
            }
            if aware_ptr {
                self.push_scratch(Dest::Val)?;
            }
            self.emit(Inst::CallRt {
                symbol: "slc_rt_enter_poly".into(),
                arg: RtArg::Val,
                noreturn: false,
                returns: false,
            });
            if aware_ptr {
                self.pop_scratch();
            }
            return self.compile_call(thunk, &Term::Var("$unit".into()), mode);
        }
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
            Term::Co(coterm) => self.compile_co(coterm, mode),
            Term::CoMatch { branches, .. } => self.compile_menu(branches, mode),
        }
    }

    fn compile_command(&mut self, command: &Command, mode: Mode) -> Result<bool, String> {
        let Command::Cut(term, coterm) = command;
        match coterm {
            CoTerm::Covar(name) => self.deliver(term, name, mode),
            CoTerm::CoCase { branches, .. } => self.compile_cocase(term, branches, mode),
            CoTerm::MuTildeTensor(binders, body) => self.compile_tensor(term, binders, body, mode),
            CoTerm::MuTilde(binder, body) => self.compile_bind(term, binder, body, mode),
            CoTerm::Prj(index) => self.compile_prj(term, *index, mode),
            CoTerm::App(arg, cont) => self.compile_app(term, arg, cont, mode),
            CoTerm::Dtor(label, cont) => self.compile_dtor(term, label, cont, mode),
        }
    }

    fn compile_mu(&mut self, mu: &str, command: &Command, mode: Mode) -> Result<bool, String> {
        if let Some(lem) = lem_shape(mu, command) {
            return self.compile_lem(mu, lem);
        }
        let escaping = escapes(mu, command);
        let mut join_after = None;
        if escaping {
            let Some(&slot) = self.slots.get(mu) else {
                return Err(format!("escaping {mu} has no slot"));
            };
            // The copy is taken before the slot is stored, so the image does not alias it.
            // Tail position: the caller is already the continuation. Value position: the
            // continuation is the code after this μ, which still lives in this frame.
            if mode == Mode::Value {
                let after = self.new_block();
                self.emit(Inst::CaptureJoin { block: after, dst: Dest::Val });
                self.store_slot(Dest::Val, slot);
                self.mark_word(mu, slot, true);
                join_after = Some(after);
            } else {
                self.emit(Inst::Capture { dst: Dest::Val });
                self.store_slot(Dest::Val, slot);
                self.mark_word(mu, slot, true);
            }
        }
        // `__tail` follows the surrounding mode. A `let` body cuts there.
        if mu == "__tail" {
            let saved_join = join_after.and_then(|block| self.join.replace(block));
            let fresh = self.forwards.insert(mu.to_string());
            let result = self.compile_mu_command(mu, command, mode);
            if fresh {
                self.forwards.remove(mu);
            }
            if join_after.is_some() {
                self.join = saved_join;
            }
            let result = result?;
            if let Some(block) = join_after {
                if !terminated(&self.blocks[self.cur].insts) {
                    self.emit(Inst::Jmp { target: block });
                }
                self.cur = block;
            }
            return Ok(result);
        }
        let after = match mode {
            Mode::Value => Some(join_after.unwrap_or_else(|| self.new_block())),
            Mode::Tail => None,
        };
        let exit = match after {
            Some(block) => ContExit::Join(block),
            None => ContExit::Ret,
        };
        let saved_exit = self.cont_exit.insert(mu.to_string(), exit);
        let saved_join = if escaping && mode == Mode::Value {
            self.join.replace(after.expect("value μ has a join"))
        } else {
            None
        };
        let fresh = self.forwards.insert(mu.to_string());
        let result = self.compile_mu_command(mu, command, mode);
        if fresh {
            self.forwards.remove(mu);
        }
        match saved_exit {
            Some(prev) => {
                self.cont_exit.insert(mu.to_string(), prev);
            }
            None => {
                self.cont_exit.remove(mu);
            }
        }
        if escaping && mode == Mode::Value {
            self.join = saved_join;
        }
        let result = result?;
        if let Some(block) = after {
            if !terminated(&self.blocks[self.cur].insts) {
                self.emit(Inst::Jmp { target: block });
            }
            self.cur = block;
        }
        Ok(result)
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
                self.compile_app(value, arg, cont, mode)
            }
            CoTerm::Covar(name) => self.deliver(value, name, mode),
            CoTerm::Prj(index) => self.compile_prj(value, *index, mode),
            CoTerm::Dtor(label, cont) => self.compile_dtor(value, label, cont, mode),
        }
    }

    /// A direct cut against this co-variable returns. A slot is a consumer to activate.
    /// `__arm` names the ambient continuation and is never bound.
    fn name_returns(&self, name: &str) -> bool {
        self.forwards.contains(name) || (!self.slots.contains_key(name) && name.starts_with("__"))
    }

    fn cont_is_return(&self, cont: &CoTerm) -> bool {
        let CoTerm::Covar(name) = cont else { return false };
        self.name_returns(name)
    }

    fn compile_app(
        &mut self,
        callee: &Term,
        arg: &Term,
        cont: &CoTerm,
        mode: Mode,
    ) -> Result<bool, String> {
        if self.cont_is_return(cont) {
            if let CoTerm::Covar(name) = cont
                && let Some(exit) = self.cont_exit.get(name).copied()
            {
                let call_mode = match exit {
                    ContExit::Ret => Mode::Tail,
                    ContExit::Join(_) => Mode::Value,
                };
                let pointer = self.compile_call(callee, arg, call_mode)?;
                self.finish_exit(exit);
                return Ok(pointer);
            }
            return self.compile_call(callee, arg, mode);
        }
        let pointer = self.compile_call(callee, arg, Mode::Value)?;
        if terminated(&self.blocks[self.cur].insts) {
            return Ok(pointer);
        }
        self.deliver_ready(cont, pointer, mode)
    }

    fn compile_dtor(
        &mut self,
        menu: &Term,
        label: &str,
        cont: &CoTerm,
        mode: Mode,
    ) -> Result<bool, String> {
        let pointer = self.compile_term(menu, Mode::Value)?;
        if terminated(&self.blocks[self.cur].insts) {
            return Ok(pointer);
        }
        // A `let` of a menu binds the computation. The request is what runs it.
        self.demand_value()?;
        self.activate_menu(label, cont, mode)
    }

    /// A request or a projection runs a delayed computation, then uses the value.
    /// Any other tag is already that value.
    fn demand_value(&mut self) -> Result<(), String> {
        let slot = self.push_scratch(Dest::Val)?;
        self.emit(Inst::Force { slot });
        self.pop_scratch();
        Ok(())
    }

    /// `r13` is the menu. The request is the label over the reified continuation.
    fn activate_menu(&mut self, label: &str, cont: &CoTerm, mode: Mode) -> Result<bool, String> {
        let menu_slot = self.push_scratch(Dest::Val)?;
        let payload_ptr = self.reify(cont)?;
        if terminated(&self.blocks[self.cur].insts) {
            self.pop_scratch();
            return Ok(payload_ptr);
        }
        let temp_mark = self.temp_used;
        let payload_slot = if payload_ptr {
            self.push_scratch(Dest::Val)?
        } else {
            let slot = self.alloc_temp(false)?;
            self.store_slot(Dest::Val, slot);
            if self.val_ptr {
                self.emit(Inst::Imm { dst: Dest::Val, value: 0 });
            }
            slot
        };
        let id = self.intern(label);
        let map = if payload_ptr { self.heap_map(&[1]) } else { MAP_EMPTY };
        self.emit(Inst::CallAlloc { words: 2, tag: TAG_TAGGED, map_id: map, dst: Dest::V(0) });
        self.emit(Inst::Imm { dst: Dest::V(1), value: i64::from(id) });
        self.emit(Inst::Store {
            src: Dest::V(1),
            base: Dest::V(0),
            offset: TAGGED_LABEL as i32,
            width: 8,
        });
        self.load_slot(Dest::V(1), payload_slot);
        self.emit(Inst::Store {
            src: Dest::V(1),
            base: Dest::V(0),
            offset: TAGGED_PAYLOAD as i32,
            width: 8,
        });
        self.emit(Inst::Mov { dst: Dest::Val, src: Dest::V(0) });
        if payload_ptr {
            self.pop_scratch();
        } else if self.temp_used == temp_mark + 1 {
            self.temp_used = temp_mark;
        }
        self.emit(Inst::Activate {
            consumer: Dest::Slot(menu_slot),
            tail: mode == Mode::Tail,
            arg_is_pointer: true,
        });
        self.pop_scratch();
        Ok(false)
    }

    /// The value is already in `r13`. Send it to `cont`.
    fn deliver_ready(&mut self, cont: &CoTerm, pointer: bool, mode: Mode) -> Result<bool, String> {
        match cont {
            CoTerm::Covar(name) => {
                if let Some(exit) = self.cont_exit.get(name).copied() {
                    self.finish_exit(exit);
                    return Ok(pointer);
                }
                if self.name_returns(name) {
                    if mode == Mode::Tail {
                        self.finish(Mode::Tail);
                    }
                    return Ok(pointer);
                }
                if let Some(&slot) = self.slots.get(name) {
                    self.emit(Inst::Activate {
                        consumer: Dest::Slot(slot),
                        tail: mode == Mode::Tail,
                        arg_is_pointer: pointer,
                    });
                    return Ok(false);
                }
                let op = self.intern(name);
                self.emit_perform(op, mode == Mode::Tail, pointer)
            }
            CoTerm::MuTilde(binder, body) => {
                if let Some(&slot) = self.slots.get(binder.as_str()) {
                    self.store_slot(Dest::Val, slot);
                    let kept = pointer || self.word_ptr(binder);
                    self.mark_word(binder, slot, pointer);
                    if !kept && self.val_ptr {
                        self.emit(Inst::Imm { dst: Dest::Val, value: 0 });
                    }
                }
                self.compile_command(body, mode)
            }
            CoTerm::MuTildeTensor(binders, body) => {
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
            CoTerm::CoCase { branches, .. } => self.compile_cocase_ready(branches, mode),
            CoTerm::App(arg, next) => {
                let callee_slot = self.push_scratch(Dest::Val)?;
                let arg_ptr = self.compile_term(arg, Mode::Value)?;
                if terminated(&self.blocks[self.cur].insts) {
                    self.pop_scratch();
                    return Ok(arg_ptr);
                }
                self.emit(Inst::CallClosure {
                    closure: Dest::Slot(callee_slot),
                    tail: false,
                    arg_is_pointer: arg_ptr,
                });
                self.pop_scratch();
                self.deliver_ready(next, true, mode)
            }
            CoTerm::Dtor(label, tail) => {
                self.demand_value()?;
                self.activate_menu(label, tail, mode)
            }
            CoTerm::Prj(index) => {
                self.demand_value()?;
                self.emit(Inst::Load {
                    dst: Dest::Val,
                    base: Dest::Val,
                    offset: 24 + 8 * *index as i32,
                    width: 8,
                });
                if mode == Mode::Tail {
                    self.finish(Mode::Tail);
                }
                Ok(true)
            }
        }
    }

    fn compile_co(&mut self, coterm: &CoTerm, mode: Mode) -> Result<bool, String> {
        match coterm {
            CoTerm::Covar(name) => self.reify_covar(name, mode),
            CoTerm::CoCase { owner, branches } => {
                let body = Term::Mu(
                    "__go".into(),
                    Box::new(Command::Cut(
                        Term::Var("__scrut".into()),
                        CoTerm::CoCase { owner: owner.clone(), branches: branches.clone() },
                    )),
                );
                // The argument is the labelled value.
                self.lift_consumer("__scrut", true, &body, mode)
            }
            CoTerm::MuTilde(binder, cmd) => {
                let body = Term::Mu("__go".into(), cmd.clone());
                let ptr = param_is_pointer(binder, &body);
                self.lift_consumer(binder, ptr, &body, mode)
            }
            CoTerm::MuTildeTensor(binders, cmd) => {
                let body = Term::Mu(
                    "__go".into(),
                    Box::new(Command::Cut(
                        Term::Var("__prod".into()),
                        CoTerm::MuTildeTensor(binders.clone(), cmd.clone()),
                    )),
                );
                self.lift_consumer("__prod", true, &body, mode)
            }
            CoTerm::Dtor(label, tail) => self.reify_dtor(label, tail, mode),
            CoTerm::App(_, _) | CoTerm::Prj(_) => {
                Err("cannot reify an application or a projection".into())
            }
        }
    }

    fn compile_menu(&mut self, branches: &[CoMatchBranch], mode: Mode) -> Result<bool, String> {
        let cases = branches
            .iter()
            .map(|branch| CoCaseBranch {
                label: branch.label.clone(),
                binders: vec![branch.binder.clone()],
                body: branch.body.clone(),
            })
            .collect();
        let body = Term::Mu(
            "__go".into(),
            Box::new(Command::Cut(
                Term::Var("__req".into()),
                CoTerm::CoCase { owner: String::new(), branches: cases },
            )),
        );
        // A request is a tagged object. The branch binds its continuation.
        self.lift_consumer("__req", true, &body, mode)
    }

    fn lift_consumer(
        &mut self,
        param: &str,
        param_ptr: bool,
        body: &Term,
        mode: Mode,
    ) -> Result<bool, String> {
        let symbol = format!("{}__lam_{}", self.current, self.lift_index);
        self.lift_index += 1;
        // `collect` also reserves this binder on the parent. It is the argument,
        // not a closed-over slot: copying that reservation would clobber it.
        let captures: Vec<_> =
            self.capture_list(body).into_iter().filter(|(name, _)| name != param).collect();
        self.lift_function(&symbol, param, param_ptr, body, &captures)?;
        self.emit_code_object(&symbol, &captures, 4, true, false)?;
        if mode == Mode::Tail {
            self.finish(Mode::Tail);
        }
        Ok(true)
    }

    fn reify(&mut self, coterm: &CoTerm) -> Result<bool, String> {
        self.compile_co(coterm, Mode::Value)
    }

    fn reify_covar(&mut self, name: &str, mode: Mode) -> Result<bool, String> {
        if self.slots.contains_key(name) {
            return self.compile_var(name, mode);
        }
        // No slot: this names the continuation that is current right now.
        if self.forwards.contains(name) || name.starts_with("__") {
            self.emit_code_object("slc_id_kont", &[], 4, true, false)?;
            if mode == Mode::Tail {
                self.finish(Mode::Tail);
            }
            return Ok(true);
        }
        self.compile_var(name, mode)
    }

    fn reify_dtor(&mut self, label: &str, tail: &CoTerm, mode: Mode) -> Result<bool, String> {
        let payload_ptr = self.reify(tail)?;
        if terminated(&self.blocks[self.cur].insts) {
            return Ok(payload_ptr);
        }
        let scratch = if payload_ptr { Some(self.push_scratch(Dest::Val)?) } else { None };
        let id = self.intern(label);
        let map = if payload_ptr { self.heap_map(&[1]) } else { MAP_EMPTY };
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
        } else if let Some(text) = name.strip_prefix("$float_") {
            let value: f64 = text.parse().map_err(|_| format!("bad float {name}"))?;
            self.emit(Inst::Imm { dst: Dest::Val, value: value.to_bits() as i64 });
            false
        } else if let Some(text) = name.strip_prefix("$char_") {
            let value = text.chars().next().ok_or_else(|| format!("bad char {name}"))?;
            self.emit(Inst::Imm { dst: Dest::Val, value: value as i64 });
            false
        } else if let Some(text) = name.strip_prefix("$str_") {
            self.emit_string(&decode_lit(text))?;
            true
        } else if let Some(&index) = self.pool.get(name) {
            self.emit(Inst::LeaPool { dst: Dest::V(0), index });
            self.emit(Inst::Load { dst: Dest::Val, base: Dest::V(0), offset: 0, width: 8 });
            true
        } else if let Some(&slot) = self.slots.get(name) {
            // A local shadows a function of the same name (`index` is both).
            self.load_slot(Dest::Val, slot);
            self.pointer_slots.contains(&slot)
        } else if self.func_param.contains_key(name) {
            // A known function used as a value is a closure, not a call.
            self.emit_code_object(name, &[], 4, true, true)?;
            true
        } else if let Some(rest) = name.strip_prefix("__dict_") {
            // The interpreter installs these after lowering. A projection's index is
            // the trait's method, so an impl that writes `gt` first still slots `lt` at 0.
            let (trait_name, key) =
                rest.split_once('_').ok_or_else(|| format!("unbound {name}"))?;
            let order =
                self.method_order.get(trait_name).ok_or_else(|| format!("unbound {name}"))?;
            let mut methods = Vec::new();
            for method in order {
                let symbol = format!("{trait_name}#{key}#{method}");
                if !self.func_param.contains_key(&symbol) {
                    return Err(format!("unbound {name}"));
                }
                methods.push(symbol);
            }
            if methods.is_empty() {
                return Err(format!("unbound {name}"));
            }
            if methods.len() == 1 {
                self.emit_code_object(&methods[0], &[], 4, true, true)?;
            } else {
                let mut saved = Vec::new();
                for sym in &methods {
                    self.emit_code_object(sym, &[], 4, true, true)?;
                    saved.push(self.push_scratch(Dest::Val)?);
                }
                let heap: Vec<u16> = (0..methods.len()).map(|index| index as u16 + 1).collect();
                let map = self.heap_map(&heap);
                self.emit(Inst::CallAlloc {
                    words: 1 + methods.len() as u32,
                    tag: TAG_TUPLE,
                    map_id: map,
                    dst: Dest::V(0),
                });
                self.emit(Inst::Imm { dst: Dest::V(1), value: methods.len() as i64 });
                self.emit(Inst::Store { src: Dest::V(1), base: Dest::V(0), offset: 16, width: 8 });
                for (index, slot) in saved.iter().enumerate() {
                    self.load_slot(Dest::V(1), *slot);
                    self.emit(Inst::Store {
                        src: Dest::V(1),
                        base: Dest::V(0),
                        offset: 24 + 8 * index as i32,
                        width: 8,
                    });
                }
                for _ in &saved {
                    self.pop_scratch();
                }
                self.emit(Inst::Mov { dst: Dest::Val, src: Dest::V(0) });
            }
            true
        } else if let Some(term) = self.literals.get(name).cloned() {
            return self.compile_term(&term, mode);
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
        self.emit_code_object(&symbol, &captures, 3, false, false)?;
        Ok(true)
    }

    fn compile_tag(&mut self, label: &str, payload: &Term, mode: Mode) -> Result<bool, String> {
        // A handler value is the clauses object paired with its return closure.
        // `do body h` unpacks that pair; an inline `handle` compiles the same tag.
        if label == "__clauses" {
            let Term::Tuple(entries) = payload else {
                return Err("handler clauses".into());
            };
            let saved_scratch = self.scratch_top;
            let saved_temp = self.temp_used;
            let mut pairs = Vec::new();
            for entry in entries {
                let Term::Tuple(pair) = entry else {
                    return Err("clause pair".into());
                };
                let (Term::Var(op_name), closure) = (&pair[0], &pair[1]) else {
                    return Err("clause pair".into());
                };
                let op = decode_lit(op_name.strip_prefix("$str_").unwrap_or(op_name));
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
            let tuple_map = self.heap_map(&[1, 2]);
            self.emit(Inst::CallAlloc {
                words: 3,
                tag: TAG_TUPLE,
                map_id: tuple_map,
                dst: Dest::V(0),
            });
            self.emit(Inst::Imm { dst: Dest::V(1), value: 2 });
            self.emit(Inst::Store { src: Dest::V(1), base: Dest::V(0), offset: 16, width: 8 });
            self.load_slot(Dest::V(1), clauses_slot);
            self.emit(Inst::Store { src: Dest::V(1), base: Dest::V(0), offset: 24, width: 8 });
            self.load_slot(Dest::V(1), ret_slot);
            self.emit(Inst::Store { src: Dest::V(1), base: Dest::V(0), offset: 32, width: 8 });
            self.emit(Inst::Mov { dst: Dest::Val, src: Dest::V(0) });
            self.restore_arm(saved_scratch, saved_temp);
            if mode == Mode::Tail {
                self.finish(Mode::Tail);
            }
            return Ok(true);
        }
        // Folded bools re-embed as tags. Pointer equality needs the pool singletons.
        if matches!(label, "Bool::True" | "Bool::False")
            && matches!(payload, Term::Var(name) if name == "$unit")
        {
            self.load_pool(label)?;
            if mode == Mode::Tail {
                self.finish(Mode::Tail);
            }
            return Ok(true);
        }
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
        // Scalar components only need a temp until they are copied into the tuple.
        let mark = self.temp_used;
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
                if self.val_ptr {
                    self.emit(Inst::Imm { dst: Dest::Val, value: 0 });
                }
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
        let scalars = saved.iter().filter(|(_, pointer)| !pointer).count() as u16;
        // A nested match may have kept a pointer temp. Those slots stay reserved.
        if self.temp_used == mark + scalars {
            self.temp_used = mark;
        }
        self.emit(Inst::Mov { dst: Dest::Val, src: Dest::V(0) });
        if mode == Mode::Tail {
            self.finish(Mode::Tail);
        }
        Ok(true)
    }

    /// `callee` is not a direct symbol. The argument is evaluated first.
    fn call_closure_term(&mut self, callee: &Term, arg: &Term, mode: Mode) -> Result<bool, String> {
        let arg_ptr = self.compile_term(arg, Mode::Value)?;
        if terminated(&self.blocks[self.cur].insts) {
            return Ok(arg_ptr);
        }
        let arg_mark = self.temp_used;
        let arg_slot = if arg_ptr {
            self.push_scratch(Dest::Val)?
        } else {
            let slot = self.alloc_temp(false)?;
            self.store_slot(Dest::Val, slot);
            if self.val_ptr {
                self.emit(Inst::Imm { dst: Dest::Val, value: 0 });
            }
            slot
        };
        let closure_ptr = self.compile_term(callee, Mode::Value)?;
        if terminated(&self.blocks[self.cur].insts) {
            if arg_ptr {
                self.pop_scratch();
            } else if self.temp_used == arg_mark + 1 {
                self.temp_used = arg_mark;
            }
            return Ok(closure_ptr);
        }
        let closure_slot = self.push_scratch(Dest::Val)?;
        self.load_slot(Dest::Val, arg_slot);
        self.emit(Inst::CallClosure {
            closure: Dest::Slot(closure_slot),
            tail: mode == Mode::Tail,
            arg_is_pointer: arg_ptr,
        });
        self.pop_scratch();
        if arg_ptr {
            self.pop_scratch();
        } else if self.temp_used == arg_mark + 1 {
            self.temp_used = arg_mark;
        }
        Ok(true)
    }

    fn compile_call(&mut self, callee: &Term, arg: &Term, mode: Mode) -> Result<bool, String> {
        if let Term::Mu(binder, command) = callee
            && binder == "__call"
            && let Command::Cut(Term::Var(name), CoTerm::App(values, cont)) = command.as_ref()
            && let CoTerm::Covar(cov) = cont.as_ref()
            && cov == "__call"
        {
            let spec: Option<(&str, u8, &[bool])> = match name.as_str() {
                "parse_int" => Some(("slc_rt_parse_int", 1, &[false, true, true])),
                "char_at" => Some(("slc_rt_char_at", 2, &[false, true])),
                "find_char" => Some(("slc_rt_find_char", 3, &[false, true])),
                "__read_file" => Some(("slc_rt_read_file", 1, &[true, true])),
                "__open_file" => Some(("slc_rt_open_file", 1, &[false, true])),
                "__read_line" => Some(("slc_rt_read_line", 1, &[true, false])),
                "__write_file" => Some(("slc_rt_write_file", 2, &[false, true])),
                _ => None,
            };
            if let Some((symbol, width, arms)) = spec {
                return self.compile_offering(symbol, width, arms, values, arg, mode);
            }
        }
        let Term::Var(symbol) = callee else {
            return self.call_closure_term(callee, arg, mode);
        };
        // A dictionary is the method closure (or the tuple of them), applied to
        // the impl's bound dictionaries. It is not an effect.
        if symbol.starts_with("__dict_") {
            return self.call_closure_term(callee, arg, mode);
        }
        if symbol == "$force" {
            return self.compile_force(arg, mode);
        }
        if symbol == "$adapt" {
            return self.compile_adapt(arg);
        }
        if matches!(
            symbol.as_str(),
            "__add"
                | "__sub"
                | "__mul"
                | "__div"
                | "__rem"
                | "__neg"
                | "__eq"
                | "__ne"
                | "__lt"
                | "__gt"
                | "__le"
                | "__ge"
                | "__xor"
                | "__wrapping_mul"
                | "__display"
                | "int_to_str"
                | "__to_i8"
                | "__to_i32"
                | "__to_i64"
                | "__to_u8"
                | "__to_u32"
                | "__to_u64"
                | "__to_f32"
                | "__to_f64"
                | "__sqrt"
                | "__abs"
                | "__floor"
                | "__ceil"
                | "__argument_count"
                | "__argument_at"
                | "__monotonic_ns"
                | "str_len"
                | "__index"
                | "char_to_code"
                | "substring"
                | "skip_digits"
                | "skip_ws"
                | "is_digit"
                | "is_ws"
                | "str_eq"
                | "__close_file"
                | "__file_exists"
        ) {
            return self.compile_builtin(symbol, arg, mode);
        }
        // Prelude `impl Add for i64` is `<… | __add>`. Calling that function
        // allocates the operand tuple again and runs a full prologue. An unknown
        // operand kind keeps the call, which still has the right body.
        if let Some(builtin) = prelude_primitive(symbol)
            && self.operand_class(arg).is_some()
        {
            return self.compile_primitive(builtin, arg, mode);
        }
        if self.slots.contains_key(symbol.as_str()) {
            return self.compile_indirect(symbol, arg, mode);
        }
        if !self.func_param.contains_key(symbol) {
            if !self.operations.contains(symbol) {
                // Lookup fails before the argument runs.
                let id = self.intern(symbol);
                self.emit(Inst::Imm { dst: Dest::Val, value: i64::from(id) });
                self.emit(Inst::CallRt {
                    symbol: "slc_rt_unbound".into(),
                    arg: RtArg::Val,
                    noreturn: true,
                    returns: false,
                });
                return Ok(false);
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
        if terminated(&self.blocks[self.cur].insts) {
            return Ok(true);
        }
        self.compile_cocase_ready(branches, mode)
    }

    /// The labelled value is already in `r13`.
    fn compile_cocase_ready(
        &mut self,
        branches: &[CoCaseBranch],
        mode: Mode,
    ) -> Result<bool, String> {
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
            let temp = self.temp_used;
            self.bind_payload(&branch.binders);
            result_ptr |= self.compile_command(&branch.body, mode)?;
            self.finish(mode);
            self.restore_arm(scratch, temp);
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
        let saved_temp = self.temp_used;
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
        self.restore_arm(scratch, saved_temp);
        Ok(result)
    }

    fn compile_split(
        &mut self,
        success: Vec<Row>,
        fail: usize,
        failure: Vec<Row>,
        mode: Mode,
    ) -> Result<bool, String> {
        let scratch = self.scratch_top;
        let temp = self.temp_used;
        let mut result = self.compile_matrix(success, mode)?;
        self.finish(mode);
        self.restore_arm(scratch, temp);
        self.cur = fail;
        result |= self.compile_matrix(failure, mode)?;
        self.finish(mode);
        self.restore_arm(scratch, temp);
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
                self.compile_split(success, fail, failure, mode)
            }
            Pat::Char(value) => {
                let fail = self.new_block();
                self.cmp_place(&place, i64::from(*value as u32), Cond::Ne, fail);
                self.compile_split(success, fail, failure, mode)
            }
            Pat::Float(value) => {
                let fail = self.new_block();
                self.emit(Inst::Imm { dst: Dest::V(1), value: value.to_bits() as i64 });
                let src = self.place_dest(&place);
                self.emit(Inst::FCmp {
                    left: src,
                    right: Dest::V(1),
                    cond: Cond::Ne,
                    target: fail,
                });
                self.compile_split(success, fail, failure, mode)
            }
            Pat::Range(lo, hi) => {
                let fail = self.new_block();
                match (lo.as_ref(), hi.as_ref()) {
                    (Pat::Int(lo), Pat::Int(hi)) => self.emit_range(&place, *lo, *hi, fail),
                    (Pat::Char(lo), Pat::Char(hi)) => {
                        self.emit_range(&place, i64::from(*lo as u32), i64::from(*hi as u32), fail)
                    }
                    (Pat::Float(lo), Pat::Float(hi)) => {
                        let src = self.place_dest(&place);
                        self.emit(Inst::FInRange {
                            src,
                            lo: lo.to_bits() as i64,
                            hi: hi.to_bits() as i64,
                            fail,
                        });
                    }
                    _ => return Err("only integer, float, or char ranges".into()),
                }
                self.compile_split(success, fail, failure, mode)
            }
            Pat::Or(alts) => {
                let body = self.new_block();
                let fail = self.new_block();
                self.emit_alternatives(alts, &place, body)?;
                self.emit(Inst::Jmp { target: fail });
                self.cur = body;
                self.compile_split(success, fail, failure, mode)
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
                Pat::Char(value) => self.cmp_place(place, i64::from(*value as u32), Cond::E, hit),
                Pat::Float(value) => {
                    self.emit(Inst::Imm { dst: Dest::V(1), value: value.to_bits() as i64 });
                    let src = self.place_dest(place);
                    self.emit(Inst::FCmp {
                        left: src,
                        right: Dest::V(1),
                        cond: Cond::E,
                        target: hit,
                    });
                }
                Pat::Range(lo, hi) => {
                    let next = self.new_block();
                    match (lo.as_ref(), hi.as_ref()) {
                        (Pat::Int(lo), Pat::Int(hi)) => self.emit_range(place, *lo, *hi, next),
                        (Pat::Char(lo), Pat::Char(hi)) => self.emit_range(
                            place,
                            i64::from(*lo as u32),
                            i64::from(*hi as u32),
                            next,
                        ),
                        (Pat::Float(lo), Pat::Float(hi)) => {
                            let src = self.place_dest(place);
                            self.emit(Inst::FInRange {
                                src,
                                lo: lo.to_bits() as i64,
                                hi: hi.to_bits() as i64,
                                fail: next,
                            });
                        }
                        _ => return Err("only integer, float, or char ranges".into()),
                    }
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
        let scratch = self.scratch_top;
        let temp = self.temp_used;
        let mut result = false;
        for child in children {
            self.cur = child.block;
            self.restore_arm(scratch, temp);
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
        self.restore_arm(scratch, temp);
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

    fn place_dest(&mut self, place: &Place) -> Dest {
        match place {
            Place::Val => Dest::Val,
            Place::Slot(slot) => {
                self.load_slot(Dest::V(0), *slot);
                Dest::V(0)
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
            hide_map: 0,
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
            cont_exit: std::mem::take(&mut self.cont_exit),
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
        self.cont_exit = saved.cont_exit;
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
        self.emit_code_object(&symbol, &captures, 4, true, false)?;
        if mode == Mode::Tail {
            self.finish(Mode::Tail);
        }
        Ok(true)
    }

    /// `words` is 4 for a closure (code, env, frame size, map) and 3 for a delay.
    /// `declaration` stores birth 0 so a `func_param` or dictionary method adopts
    /// its caller. A lambda keeps the origin `call_alloc` wrote.
    fn emit_code_object(
        &mut self,
        symbol: &str,
        captures: &[(String, bool)],
        words: u32,
        closure: bool,
        declaration: bool,
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
            if declaration {
                self.emit(Inst::Imm { dst: Dest::V(1), value: 0 });
                self.emit(Inst::Store {
                    src: Dest::V(1),
                    base: Dest::V(0),
                    offset: CLOSURE_BIRTH as i32,
                    width: 8,
                });
            }
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
        // Scalar captures only need a temp until they are copied into the env.
        let mark = self.temp_used;
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
        let scalars = saved.iter().filter(|(_, ptr)| !ptr).count() as u16;
        if self.temp_used == mark + scalars {
            self.temp_used = mark;
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

    /// The value is already evaluated, or `finish_exit` runs after `compile_term`.
    fn finish_exit(&mut self, exit: ContExit) {
        if terminated(&self.blocks[self.cur].insts) {
            return;
        }
        match exit {
            ContExit::Join(block) => self.emit(Inst::Jmp { target: block }),
            ContExit::Ret => self.finish(Mode::Tail),
        }
    }

    fn deliver(&mut self, value: &Term, covar: &str, mode: Mode) -> Result<bool, String> {
        // A branch that cuts to the μ must not join and then run the rest of the body.
        if let Some(exit) = self.cont_exit.get(covar).copied() {
            let call_mode = match exit {
                ContExit::Ret => Mode::Tail,
                ContExit::Join(_) => Mode::Value,
            };
            let pointer = self.compile_term(value, call_mode)?;
            self.finish_exit(exit);
            return Ok(pointer);
        }
        // `__arm` is the ambient continuation of a menu or select arm.
        if self.name_returns(covar) {
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

    fn compile_prj(&mut self, base: &Term, index: usize, mode: Mode) -> Result<bool, String> {
        let pointer = self.compile_term(base, Mode::Value)?;
        if terminated(&self.blocks[self.cur].insts) {
            return Ok(pointer);
        }
        self.demand_value()?;
        self.emit(Inst::Load {
            dst: Dest::Val,
            base: Dest::Val,
            offset: 24 + 8 * index as i32,
            width: 8,
        });
        if mode == Mode::Tail {
            self.finish(Mode::Tail);
        }
        Ok(true)
    }

    fn operand_class(&self, term: &Term) -> Option<Class> {
        fn from_type(ty: &Type) -> Option<Class> {
            match ty {
                Type::Rowed(inner, _) | Type::Dual(inner) | Type::Delayed(inner, _) => {
                    from_type(inner)
                }
                Type::Pos(base) | Type::Neg(base) => match base {
                    Base::F32 | Base::F64 => Some(Class::Float),
                    Base::Str => Some(Class::Str),
                    Base::Char => Some(Class::Char),
                    Base::File => Some(Class::File),
                    Base::I64 | Base::I32 | Base::I8 | Base::U8 | Base::U32 | Base::U64 => {
                        Some(Class::Int)
                    }
                },
                Type::Named(name, _) if name == "Bool" => Some(Class::Bool),
                Type::Tensor(items) => items.first().and_then(from_type),
                _ => None,
            }
        }
        fn from_term(builder: &Builder, term: &Term) -> Option<Class> {
            match term {
                Term::Tuple(items) => items.first().and_then(|item| from_term(builder, item)),
                Term::Var(name) if name.starts_with("$int_") => Some(Class::Int),
                Term::Var(name) if name.starts_with("$float_") => Some(Class::Float),
                Term::Var(name) if name.starts_with("$str_") => Some(Class::Str),
                Term::Var(name) if name.starts_with("$char_") => Some(Class::Char),
                Term::Var(name) => builder.word_ty(name).and_then(from_type),
                _ => None,
            }
        }
        from_term(self, term)
    }

    fn compile_builtin(&mut self, name: &str, arg: &Term, mode: Mode) -> Result<bool, String> {
        if matches!(name, "__eq" | "__ne" | "__lt" | "__gt" | "__le" | "__ge") {
            let class = self.operand_class(arg).unwrap_or(Class::Int);
            let cond = match name {
                "__eq" => Cond::E,
                "__ne" => Cond::Ne,
                "__lt" => Cond::L,
                "__gt" => Cond::G,
                "__le" => Cond::Le,
                _ => Cond::Ge,
            };
            return match class {
                Class::Str => {
                    let op = match name {
                        "__eq" => 0,
                        "__ne" => 1,
                        "__lt" => 2,
                        "__gt" => 3,
                        "__le" => 4,
                        _ => 5,
                    };
                    self.compile_term(arg, Mode::Value)?;
                    if !terminated(&self.blocks[self.cur].insts) {
                        self.emit(Inst::CallRt {
                            symbol: "slc_rt_str_cmp".into(),
                            arg: RtArg::PairImm(op),
                            noreturn: false,
                            returns: true,
                        });
                    }
                    Ok(true)
                }
                Class::Float => self.compile_cmp(arg, CmpKind::Float(cond), mode),
                Class::Bool if matches!(name, "__lt" | "__gt" | "__le" | "__ge") => {
                    self.compile_cmp(arg, CmpKind::Labels(cond), mode)
                }
                _ => self.compile_cmp(arg, CmpKind::Words(cond), mode),
            };
        }
        let class = self.operand_class(arg);
        self.compile_term(arg, Mode::Value)?;
        if terminated(&self.blocks[self.cur].insts) {
            return Ok(false);
        }
        match name {
            "str_len" => {
                self.emit(Inst::Load {
                    dst: Dest::Val,
                    base: Dest::Val,
                    offset: STRING_CHAR_LEN as i32,
                    width: 8,
                });
                Ok(false)
            }
            "char_to_code" => Ok(false),
            "int_to_str" => {
                self.emit(Inst::CallRt {
                    symbol: "slc_rt_int_to_str".into(),
                    arg: RtArg::Val,
                    noreturn: false,
                    returns: true,
                });
                Ok(true)
            }
            "__display" => {
                // A tuple's first word is not the tuple. Shape 5 prints the value.
                let dynamic = match arg {
                    Term::Tuple(_) | Term::Tag(_, _) => true,
                    Term::Var(name) if name == "$unit" => true,
                    Term::Var(name) => match self.word_ty(name) {
                        Some(Type::Tensor(_)) | Some(Type::Delayed(_, _)) => true,
                        Some(Type::Named(owner, _)) => owner != "Bool",
                        _ => false,
                    },
                    _ => false,
                };
                let shape = if dynamic {
                    5
                } else {
                    match class {
                        Some(Class::Float) => 1,
                        Some(Class::Str) => 2,
                        Some(Class::Char) => 3,
                        Some(Class::File) => 4,
                        Some(Class::Int) => 0,
                        // A delay has no class. The runtime prints `<delayed>` and does not force it.
                        None => 5,
                        other => {
                            return Err(format!("__display of {other:?} in {}", self.current));
                        }
                    }
                };
                self.emit(Inst::CallRt {
                    symbol: "slc_rt_display".into(),
                    arg: RtArg::ValImm(shape),
                    noreturn: false,
                    returns: true,
                });
                Ok(true)
            }
            "__to_i8" | "__to_i32" | "__to_i64" | "__to_u8" | "__to_u32" | "__to_u64"
            | "__to_f32" | "__to_f64" => {
                // The word is untagged. The operand's class picks the symbol:
                // an integer and a float that share a destination are different
                // runtime functions.
                let symbol = width_symbol(name, class)?;
                self.emit(Inst::CallRt {
                    symbol: symbol.into(),
                    arg: RtArg::Val,
                    noreturn: false,
                    returns: true,
                });
                Ok(false)
            }
            "__sqrt" | "__abs" | "__floor" | "__ceil" => {
                let symbol = match name {
                    "__sqrt" => "slc_rt_sqrt",
                    "__abs" => "slc_rt_abs",
                    "__floor" => "slc_rt_floor",
                    _ => "slc_rt_ceil",
                };
                self.emit(Inst::CallRt {
                    symbol: symbol.into(),
                    arg: RtArg::Val,
                    noreturn: false,
                    returns: true,
                });
                Ok(false)
            }
            "__argument_count" => {
                self.emit(Inst::CallRt {
                    symbol: "slc_rt_argument_count".into(),
                    arg: RtArg::Val,
                    noreturn: false,
                    returns: true,
                });
                Ok(false)
            }
            "__argument_at" => {
                self.emit(Inst::CallRt {
                    symbol: "slc_rt_argument_at".into(),
                    arg: RtArg::Val,
                    noreturn: false,
                    returns: true,
                });
                Ok(true)
            }
            "__monotonic_ns" => {
                self.emit(Inst::CallRt {
                    symbol: "slc_rt_monotonic_ns".into(),
                    arg: RtArg::Val,
                    noreturn: false,
                    returns: true,
                });
                Ok(false)
            }
            "__index" => {
                self.emit(Inst::CallRt {
                    symbol: "slc_rt_index".into(),
                    arg: RtArg::PairImm(0),
                    noreturn: false,
                    returns: true,
                });
                Ok(false)
            }
            "substring" => {
                self.emit(Inst::CallRt {
                    symbol: "slc_rt_substring".into(),
                    arg: RtArg::Triple,
                    noreturn: false,
                    returns: true,
                });
                Ok(true)
            }
            "skip_digits" | "skip_ws" => {
                let symbol =
                    if name == "skip_digits" { "slc_rt_skip_digits" } else { "slc_rt_skip_ws" };
                self.emit(Inst::CallRt {
                    symbol: symbol.into(),
                    arg: RtArg::PairImm(0),
                    noreturn: false,
                    returns: true,
                });
                Ok(false)
            }
            "str_eq" => {
                self.emit(Inst::CallRt {
                    symbol: "slc_rt_str_eq".into(),
                    arg: RtArg::PairImm(0),
                    noreturn: false,
                    returns: true,
                });
                Ok(true)
            }
            "__close_file" => {
                self.emit(Inst::CallRt {
                    symbol: "slc_rt_close_file".into(),
                    arg: RtArg::Val,
                    noreturn: false,
                    returns: true,
                });
                Ok(false)
            }
            "__file_exists" => {
                self.emit(Inst::CallRt {
                    symbol: "slc_rt_file_exists".into(),
                    arg: RtArg::Val,
                    noreturn: false,
                    returns: true,
                });
                Ok(true)
            }
            "is_digit" | "is_ws" => self.compile_char_class(name, mode),
            "__add" | "__sub" | "__mul" | "__div" | "__rem" | "__neg" | "__xor"
            | "__wrapping_mul" => self.compile_arith(name, class, true),
            other => Err(format!("builtin {other}")),
        }
    }

    fn compile_char_class(&mut self, name: &str, mode: Mode) -> Result<bool, String> {
        let yes = self.new_block();
        let no = self.new_block();
        if name == "is_digit" {
            self.emit(Inst::InRange {
                src: Dest::Val,
                lo: i64::from('0' as u32),
                hi: i64::from('9' as u32),
                fail: no,
            });
            self.emit(Inst::Jmp { target: yes });
        } else {
            // `char::is_whitespace`, not `skip_ws`. The latter is only space, tab, CR, and LF.
            let miss = self.new_block();
            self.emit(Inst::InRange { src: Dest::Val, lo: 0x9, hi: 0xD, fail: miss });
            self.emit(Inst::Jmp { target: yes });
            self.cur = miss;
            for code in [0x20, 0x85, 0xA0, 0x1680, 0x202F, 0x205F, 0x3000] {
                self.emit(Inst::CmpJcc {
                    left: Dest::Val,
                    right: code,
                    cond: Cond::E,
                    target: yes,
                });
            }
            let miss = self.new_block();
            self.emit(Inst::InRange { src: Dest::Val, lo: 0x2000, hi: 0x200A, fail: miss });
            self.emit(Inst::Jmp { target: yes });
            self.cur = miss;
            self.emit(Inst::InRange { src: Dest::Val, lo: 0x2028, hi: 0x2029, fail: no });
            self.emit(Inst::Jmp { target: yes });
        }
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

    fn compile_offering(
        &mut self,
        symbol: &str,
        width: u8,
        arms: &[bool],
        values: &Term,
        conts: &Term,
        mode: Mode,
    ) -> Result<bool, String> {
        let cont_ptr = self.compile_term(conts, Mode::Value)?;
        if terminated(&self.blocks[self.cur].insts) {
            return Ok(cont_ptr);
        }
        let cont_slot = self.push_scratch(Dest::Val)?;
        let val_ptr = self.compile_term(values, Mode::Value)?;
        if terminated(&self.blocks[self.cur].insts) {
            self.pop_scratch();
            return Ok(val_ptr);
        }
        if val_ptr {
            self.push_scratch(Dest::Val)?;
        }
        let disc = self.alloc_temp(false)?;
        let arg = match width {
            1 => RtArg::Val,
            2 => RtArg::PairImm(0),
            _ => RtArg::Triple,
        };
        self.emit(Inst::CallOffer { symbol: symbol.into(), arg, disc, arg_is_pointer: val_ptr });
        if val_ptr {
            self.pop_scratch();
        }
        // The payload's pointer bit depends on the arm. Park it untraced until that branch.
        let payload = self.alloc_temp(false)?;
        self.store_slot(Dest::Val, payload);
        if self.val_ptr {
            self.emit(Inst::Imm { dst: Dest::Val, value: 0 });
        }
        if self.scratch_top + 2 > self.scratch_base + SCRATCHES {
            return Err(format!("too many live pointers in {}", self.current));
        }
        let rooted = self.scratch_top;
        let consumer = self.scratch_top + 1;
        self.scratch_top += 2;
        self.emit(Inst::Imm { dst: Dest::V(0), value: 0 });
        self.store_slot(Dest::V(0), rooted);

        let mut blocks = Vec::new();
        for _ in arms {
            blocks.push(self.new_block());
        }
        let join = match mode {
            Mode::Value => Some(self.new_block()),
            Mode::Tail => None,
        };
        let saved = self.join;
        self.join = join;
        for (index, block) in blocks.iter().enumerate().take(arms.len() - 1) {
            self.load_slot(Dest::V(0), disc);
            self.emit(Inst::CmpJcc {
                left: Dest::V(0),
                right: index as i64,
                cond: Cond::E,
                target: *block,
            });
        }
        self.emit(Inst::Jmp { target: blocks[arms.len() - 1] });
        for (index, block) in blocks.iter().enumerate() {
            self.cur = *block;
            let pointer = arms[index];
            self.load_slot(Dest::Val, payload);
            if pointer {
                self.store_slot(Dest::Val, rooted);
            }
            self.load_slot(Dest::V(0), cont_slot);
            self.emit(Inst::Load {
                dst: Dest::V(1),
                base: Dest::V(0),
                offset: 24 + 8 * index as i32,
                width: 8,
            });
            self.store_slot(Dest::V(1), consumer);
            self.load_slot(Dest::Val, payload);
            self.emit(Inst::Activate {
                consumer: Dest::Slot(consumer),
                tail: mode == Mode::Tail,
                arg_is_pointer: pointer,
            });
            self.finish(mode);
        }
        self.scratch_top -= 2;
        self.pop_scratch();
        if let Some(join) = join {
            self.cur = join;
        }
        self.join = saved;
        Ok(true)
    }

    /// `Eq#i64#eq` and the other prelude wrappers. The operand tuple is not
    /// allocated when both words can sit in registers. Integer division, float
    /// remainder, and string order still call the runtime through that tuple.
    fn compile_primitive(&mut self, name: &str, arg: &Term, mode: Mode) -> Result<bool, String> {
        let class = self.operand_class(arg);
        let needs_tuple = matches!(
            (name, class),
            ("__add", Some(Class::Str))
                | ("__div" | "__rem", Some(Class::Int))
                | ("__rem", Some(Class::Float))
                | ("__eq" | "__ne" | "__lt" | "__gt" | "__le" | "__ge", Some(Class::Str))
        );
        if needs_tuple {
            return self.compile_builtin(name, arg, mode);
        }
        let Term::Tuple(items) = arg else {
            return self.compile_builtin(name, arg, mode);
        };
        if name == "__neg" {
            if items.len() != 1 {
                return self.compile_builtin(name, arg, mode);
            }
            self.compile_term(&items[0], Mode::Value)?;
            if terminated(&self.blocks[self.cur].insts) {
                return Ok(false);
            }
            return self.compile_arith(name, class, false);
        }
        if items.len() != 2 || !self.place_pair(&items[0], &items[1])? {
            if items.len() != 2 {
                return self.compile_builtin(name, arg, mode);
            }
            return Ok(false);
        }
        if matches!(name, "__eq" | "__ne" | "__lt" | "__gt" | "__le" | "__ge") {
            let cond = match name {
                "__eq" => Cond::E,
                "__ne" => Cond::Ne,
                "__lt" => Cond::L,
                "__gt" => Cond::G,
                "__le" => Cond::Le,
                _ => Cond::Ge,
            };
            let kind = match class {
                Some(Class::Float) => CmpKind::Float(cond),
                Some(Class::Bool) if matches!(name, "__lt" | "__gt" | "__le" | "__ge") => {
                    CmpKind::Labels(cond)
                }
                _ => CmpKind::Words(cond),
            };
            return self.compile_cmp_ready(kind, mode);
        }
        self.compile_arith(name, class, false)
    }

    /// Leave the two components in `V(0)` and `V(1)`. `false` means a component diverged.
    fn place_pair(&mut self, left: &Term, right: &Term) -> Result<bool, String> {
        let mark = self.temp_used;
        let mut saved = Vec::new();
        for item in [left, right] {
            let pointer = self.compile_term(item, Mode::Value)?;
            if terminated(&self.blocks[self.cur].insts) {
                return Ok(false);
            }
            let slot = if pointer {
                self.push_scratch(Dest::Val)?
            } else {
                let slot = self.alloc_temp(false)?;
                self.store_slot(Dest::Val, slot);
                if self.val_ptr {
                    self.emit(Inst::Imm { dst: Dest::Val, value: 0 });
                }
                slot
            };
            saved.push((slot, pointer));
        }
        self.load_slot(Dest::V(0), saved[0].0);
        self.load_slot(Dest::V(1), saved[1].0);
        for (_, pointer) in saved.iter().rev() {
            if *pointer {
                self.pop_scratch();
            }
        }
        let scalars = saved.iter().filter(|(_, pointer)| !pointer).count() as u16;
        if self.temp_used == mark + scalars {
            self.temp_used = mark;
        }
        Ok(true)
    }

    fn compile_arith(
        &mut self,
        name: &str,
        class: Option<Class>,
        from_tuple: bool,
    ) -> Result<bool, String> {
        let class =
            class.ok_or_else(|| format!("{name} in {} has no operand kind", self.current))?;
        let binary = name != "__neg";
        if from_tuple
            && binary
            && !matches!(
                (name, class),
                ("__add", Class::Str) | ("__div" | "__rem", Class::Int) | ("__rem", Class::Float)
            )
        {
            self.emit(Inst::Load { dst: Dest::V(0), base: Dest::Val, offset: 24, width: 8 });
            self.emit(Inst::Load { dst: Dest::V(1), base: Dest::Val, offset: 32, width: 8 });
        }
        match (name, class) {
            ("__add", Class::Str) => {
                self.emit(Inst::CallRt {
                    symbol: "slc_rt_str_concat".into(),
                    arg: RtArg::PairImm(0),
                    noreturn: false,
                    returns: true,
                });
                Ok(true)
            }
            ("__div" | "__rem", Class::Int) => {
                let symbol =
                    if name == "__div" { "slc_rt_wrapping_div" } else { "slc_rt_wrapping_rem" };
                self.emit(Inst::CallRt {
                    symbol: symbol.into(),
                    arg: RtArg::PairImm(0),
                    noreturn: false,
                    returns: true,
                });
                Ok(false)
            }
            ("__rem", Class::Float) => {
                self.emit(Inst::CallRt {
                    symbol: "slc_rt_frem".into(),
                    arg: RtArg::PairImm(0),
                    noreturn: false,
                    returns: true,
                });
                Ok(false)
            }
            ("__add" | "__sub" | "__mul", Class::Int) | ("__neg", Class::Int) => {
                let op = match name {
                    "__sub" => I64Op::Sub,
                    "__mul" => I64Op::Mul,
                    "__neg" => I64Op::Neg,
                    _ => I64Op::Add,
                };
                let (left, right) =
                    if binary { (Dest::V(0), Dest::V(1)) } else { (Dest::Val, Dest::Val) };
                self.emit(Inst::CheckedI64 { op, left, right });
                Ok(false)
            }
            ("__add" | "__sub" | "__mul" | "__div", Class::Float) | ("__neg", Class::Float) => {
                let op = match name {
                    "__sub" => BinOp::FSub,
                    "__mul" => BinOp::FMul,
                    "__div" => BinOp::FDiv,
                    "__neg" => BinOp::FNeg,
                    _ => BinOp::FAdd,
                };
                let (left, right) =
                    if binary { (Dest::V(0), Dest::V(1)) } else { (Dest::Val, Dest::Val) };
                self.emit(Inst::Bin { op, left, right });
                Ok(false)
            }
            ("__xor", _) => {
                self.emit(Inst::Bin { op: BinOp::Xor, left: Dest::V(0), right: Dest::V(1) });
                Ok(false)
            }
            ("__wrapping_mul", _) => {
                self.emit(Inst::Bin {
                    op: BinOp::WrappingMul,
                    left: Dest::V(0),
                    right: Dest::V(1),
                });
                Ok(false)
            }
            _ => Err(format!("{name} on {class:?} in {}", self.current)),
        }
    }

    fn compile_cmp(&mut self, arg: &Term, kind: CmpKind, mode: Mode) -> Result<bool, String> {
        self.compile_term(arg, Mode::Value)?;
        if terminated(&self.blocks[self.cur].insts) {
            return Ok(true);
        }
        self.emit(Inst::Load { dst: Dest::V(0), base: Dest::Val, offset: 24, width: 8 });
        self.emit(Inst::Load { dst: Dest::V(1), base: Dest::Val, offset: 32, width: 8 });
        self.compile_cmp_ready(kind, mode)
    }

    /// The two words are already in `V(0)` and `V(1)`.
    fn compile_cmp_ready(&mut self, kind: CmpKind, mode: Mode) -> Result<bool, String> {
        let yes = self.new_block();
        let no = self.new_block();
        match kind {
            CmpKind::Words(cond) => {
                self.emit(Inst::CmpRR { left: Dest::V(0), right: Dest::V(1), cond, target: yes })
            }
            CmpKind::Float(cond) => {
                self.emit(Inst::FCmp { left: Dest::V(0), right: Dest::V(1), cond, target: yes })
            }
            CmpKind::Labels(cond) => {
                // `false` is interned before `true`, so the label id is the boolean order.
                self.emit(Inst::Load {
                    dst: Dest::V(2),
                    base: Dest::V(0),
                    offset: TAGGED_LABEL as i32,
                    width: 8,
                });
                self.emit(Inst::Load {
                    dst: Dest::V(3),
                    base: Dest::V(1),
                    offset: TAGGED_LABEL as i32,
                    width: 8,
                });
                self.emit(Inst::CmpRR { left: Dest::V(2), right: Dest::V(3), cond, target: yes });
            }
        }
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
        self.emit(Inst::Force { slot });
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
        let saved_scratch = self.scratch_top;
        let saved_temp = self.temp_used;
        // `(clauses, return closure)`. A name or an inline tag both compile to that pair.
        let produced = self.compile_term(clauses, Mode::Value)?;
        if terminated(&self.blocks[self.cur].insts) {
            return Ok(produced);
        }
        let packed = self.push_scratch(Dest::Val)?;
        self.load_slot(Dest::V(0), packed);
        self.emit(Inst::Load { dst: Dest::V(1), base: Dest::V(0), offset: 24, width: 8 });
        let clauses_slot = self.push_scratch(Dest::V(1))?;
        self.load_slot(Dest::V(0), packed);
        self.emit(Inst::Load { dst: Dest::V(1), base: Dest::V(0), offset: 32, width: 8 });
        let ret_slot = self.push_scratch(Dest::V(1))?;
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
        self.restore_arm(saved_scratch, saved_temp);
        Ok(false)
    }

    fn compile_lem(&mut self, name: &str, lem: Lem<'_>) -> Result<bool, String> {
        let symbol = format!("{}__lam_{}", self.current, self.lift_index);
        self.lift_index += 1;
        let body = Term::Mu("__tail".into(), Box::new(lem.body.clone()));
        // One pointer capture: the continuation, filled in after the copy exists.
        let captures = vec![(name.to_string(), true)];
        self.lift_function(&symbol, lem.param, false, &body, &captures)?;
        self.emit_code_object(&symbol, &[], 4, true, false)?;
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
        // `print` performs `write`. `println` performs `write_line`.
        funcs.push(self.io_text_inner("slc_io_write_inner", "slc_rt_write"));
        funcs.push(self.io_text_outer("slc_io_write_outer", "slc_io_write_inner"));
        funcs.push(self.io_text_inner("slc_io_line_inner", "slc_rt_write_line"));
        funcs.push(self.io_text_outer("slc_io_line_outer", "slc_io_line_inner"));
        funcs.push(self.exit_stub());
        // Identity must not safepoint: the answer may be a scalar or a pointer,
        // and a tail activation has left it only in `r13`.
        funcs.push(hand_fn("slc_id_kont", 9, false, &[], 0, vec![Inst::Ret]));
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

    fn io_text_inner(&self, symbol: &str, rt: &str) -> Function {
        hand_fn(
            symbol,
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
                    symbol: rt.into(),
                    arg: RtArg::Env,
                    noreturn: false,
                    returns: false,
                },
                Inst::Imm { dst: Dest::Val, value: 0 },
                Inst::Resume { image: Dest::Slot(0), tail: true },
            ],
        )
    }

    fn io_text_outer(&self, symbol: &str, inner: &str) -> Function {
        hand_fn(
            symbol,
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
                Inst::LeaSym { dst: Dest::V(1), symbol: inner.into() },
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
                Inst::SymWords { dst: Dest::V(1), symbol: inner.into() },
                Inst::Store {
                    src: Dest::V(1),
                    base: Dest::V(0),
                    offset: CLOSURE_FRAME_WORDS as i32,
                    width: 8,
                },
                Inst::SymMap { dst: Dest::V(1), symbol: inner.into() },
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
                Inst::CallRt {
                    symbol: "slc_rt_exit".into(),
                    arg: RtArg::Val,
                    noreturn: true,
                    returns: false,
                },
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
            let mut hide_slots = func.pointer_slots.clone();
            hide_slots.push(func.spill_base);
            hide_slots.sort_unstable();
            hide_slots.dedup();
            func.hide_map = self.next_map;
            self.maps.push(MapRecord {
                map_id: func.hide_map,
                frame_words: func.frame_words,
                val_is_pointer: func.val_is_pointer,
                slots: hide_slots,
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
        for (name, symbol) in
            [("Bool::True", "slc_rt_bool_true"), ("Bool::False", "slc_rt_bool_false")]
        {
            if let Some(&index) = self.pool.get(name) {
                self.emit(Inst::LeaPool { dst: Dest::V(0), index });
                self.emit(Inst::Load { dst: Dest::V(1), base: Dest::V(0), offset: 0, width: 8 });
                self.emit(Inst::StoreAbs { src: Dest::V(1), symbol: symbol.into() });
            }
        }
        let (ret_words, ret_map) = func_layout(funcs, "slc_io_return");
        let (write_words, write_map) = func_layout(funcs, "slc_io_write_outer");
        let (line_words, line_map) = func_layout(funcs, "slc_io_line_outer");
        self.emit_raw_closure("slc_io_return", ret_words, ret_map);
        self.store_slot(Dest::Val, 1);
        self.emit_raw_closure("slc_io_write_outer", write_words, write_map);
        self.store_slot(Dest::Val, 2);
        let write = self.intern("write");
        let write_line = self.intern("write_line");
        let ret_op = self.intern("return");
        // Payload words: count, then (op, closure) three times. Closures are 2, 4, 6.
        let clauses_map = self.heap_map(&[2, 4, 6]);
        self.emit(Inst::CallAlloc {
            words: 7,
            tag: TAG_CLAUSES,
            map_id: clauses_map,
            dst: Dest::V(0),
        });
        // Root the table before the next closure alloc. `write` is copied in
        // before slot 2 is reused for `write_line`.
        self.store_slot(Dest::V(0), 0);
        self.emit(Inst::Imm { dst: Dest::V(1), value: 3 });
        self.emit(Inst::Store { src: Dest::V(1), base: Dest::V(0), offset: 16, width: 8 });
        self.emit(Inst::Imm { dst: Dest::V(1), value: i64::from(write) });
        self.emit(Inst::Store { src: Dest::V(1), base: Dest::V(0), offset: 24, width: 8 });
        self.load_slot(Dest::V(1), 2);
        self.emit(Inst::Store { src: Dest::V(1), base: Dest::V(0), offset: 32, width: 8 });
        self.emit(Inst::Imm { dst: Dest::V(1), value: i64::from(write_line) });
        self.emit(Inst::Store { src: Dest::V(1), base: Dest::V(0), offset: 40, width: 8 });
        self.emit_raw_closure("slc_io_line_outer", line_words, line_map);
        self.store_slot(Dest::Val, 2);
        self.load_slot(Dest::V(0), 0);
        self.load_slot(Dest::V(1), 2);
        self.emit(Inst::Store { src: Dest::V(1), base: Dest::V(0), offset: 48, width: 8 });
        self.emit(Inst::Imm { dst: Dest::V(1), value: i64::from(ret_op) });
        self.emit(Inst::Store { src: Dest::V(1), base: Dest::V(0), offset: 56, width: 8 });
        self.load_slot(Dest::V(1), 1);
        self.emit(Inst::Store { src: Dest::V(1), base: Dest::V(0), offset: 64, width: 8 });
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
        let hide_map = self.push_frame_map(13, true, &[0, 1, 2, 3]);
        Function {
            symbol: SLC_PROGRAM_ENTRY.to_string(),
            map_id: self.prompt_map,
            frame_words: 13,
            val_is_pointer: true,
            pointer_slots: vec![0, 1, 2],
            spill_base: 3,
            hide_map,
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

fn float_bounds(pat: &Pat) -> Option<(f64, f64)> {
    match pat {
        Pat::Float(value) => Some((*value, *value)),
        Pat::Range(lo, hi) => match (lo.as_ref(), hi.as_ref()) {
            (Pat::Float(lo), Pat::Float(hi)) if lo <= hi => Some((*lo, *hi)),
            (Pat::Float(lo), Pat::Float(hi)) => Some((*hi, *lo)),
            _ => None,
        },
        Pat::Binding(_, inner) => float_bounds(inner),
        _ => None,
    }
}

fn int_bounds(pat: &Pat) -> Option<(i64, i64)> {
    match pat {
        Pat::Int(value) => Some((*value, *value)),
        Pat::Char(value) => Some((i64::from(*value as u32), i64::from(*value as u32))),
        Pat::Range(lo, hi) => match (lo.as_ref(), hi.as_ref()) {
            (Pat::Int(lo), Pat::Int(hi)) => Some((*lo.min(hi), *lo.max(hi))),
            (Pat::Char(lo), Pat::Char(hi)) => {
                let lo = i64::from(*lo as u32);
                let hi = i64::from(*hi as u32);
                Some((lo.min(hi), lo.max(hi)))
            }
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
        // Char ranges share `int_bounds` with integers. Missing that case left
        // the tested row in the failure set, and the same `or` was compiled again.
        Pat::Range(_, _) => {
            if let (Some((lo, hi)), Some((start, end))) = (int_bounds(outer), int_bounds(inner)) {
                lo <= start && end <= hi
            } else if let (Some((lo, hi)), Some((start, end))) =
                (float_bounds(outer), float_bounds(inner))
            {
                lo <= start && end <= hi
            } else {
                false
            }
        }
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
            _ => match (float_bounds(left), float_bounds(right)) {
                (Some((lo, hi)), Some((start, end))) => lo <= end && start <= hi,
                _ => false,
            },
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
