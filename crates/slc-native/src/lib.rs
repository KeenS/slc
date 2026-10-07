//! x86-64 for straight-line code, both match forms, and escaping captures.
//!
//! The ELF is machine code. The control-flow graph stays in this process.
//! Capture, invoke, and resume call the runtime copier. The encoder does not
//! copy frames itself.

mod encode;
mod lower;

#[cfg(all(test, target_arch = "x86_64", target_os = "linux"))]
mod link_tests;

use std::collections::HashMap;

use slc_core::command::Command;
use slc_core::coterm::CoTerm;
use slc_core::term::{CoMatchBranch, Term};
use slc_core::types::Type;
use slc_runtime::fold::{Fold, embed, try_fold};
use slc_syntax::lower::Specialization;

pub use encode::encode;

/// Where a word sits. Virtual temps are `V`; the encoder never puts one in a protocol register.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dest {
    V(u16),
    /// `r13`.
    Val,
    /// `r14`.
    Env,
    /// `r12`.
    Frame,
    /// Named or scratch slot. Survives a runtime call; a virtual temp does not.
    Slot(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cond {
    E,
    Ne,
    L,
    Ge,
    Le,
    G,
    B,
    Ae,
}

/// Checked `i64`. The encoder emits the `jno` sequence; there is no `slc_rt_add`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum I64Op {
    Add,
    Sub,
    Mul,
    Neg,
}

/// Inline bitwise or IEEE arithmetic. Float negation flips the sign bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Xor,
    WrappingMul,
    FAdd,
    FSub,
    FMul,
    FDiv,
    FNeg,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inst {
    Imm {
        dst: Dest,
        value: i64,
    },
    Mov {
        dst: Dest,
        src: Dest,
    },
    Load {
        dst: Dest,
        base: Dest,
        offset: i32,
        width: u8,
    },
    Store {
        src: Dest,
        base: Dest,
        offset: i32,
        width: u8,
    },
    CmpJcc {
        left: Dest,
        right: i64,
        cond: Cond,
        target: usize,
    },
    Jmp {
        target: usize,
    },
    CallSlc {
        symbol: String,
        callee_frame_words: u32,
        /// The callee's parameter, not the caller's. A scalar must not be traced as the live `r13`.
        arg_is_pointer: bool,
    },
    Tail {
        symbol: String,
        frame_words: u32,
        map_id: u32,
        /// The callee's parameter, not the caller's. A scalar must not be traced as the live `r13`.
        arg_is_pointer: bool,
    },
    Ret,
    Safepoint {
        map_id: u32,
    },
    CallAlloc {
        words: u32,
        tag: u16,
        map_id: u32,
        dst: Dest,
    },
    Ud2,
    /// Inclusive. Fall through on success; `fail` is the miss.
    InRange {
        src: Dest,
        lo: i64,
        hi: i64,
        fail: usize,
    },
    LeaBlock {
        dst: Dest,
        block: usize,
    },
    /// Address of `slc_pool_ptrs[index]`, not the word stored there.
    LeaPool {
        dst: Dest,
        index: u32,
    },
    StorePool {
        src: Dest,
        index: u32,
    },
    /// Address of a compiled function. Not a pool slot.
    LeaSym {
        dst: Dest,
        symbol: String,
    },
    /// Filled with that function's `frame_words` once inflation finishes.
    SymWords {
        dst: Dest,
        symbol: String,
    },
    /// Filled with that function's stack-map id once maps are assigned.
    SymMap {
        dst: Dest,
        symbol: String,
    },
    /// Compare two words. Used by inlined integer and `Bool` compares.
    CmpRR {
        left: Dest,
        right: Dest,
        cond: Cond,
        target: usize,
    },
    /// `op` then `jno`. On overflow, `slc_rt_fail_overflow` and no return. Result in `r13`.
    CheckedI64 {
        op: I64Op,
        left: Dest,
        right: Dest,
    },
    /// Inline result in `r13`. `right` is unused for `FNeg`.
    Bin {
        op: BinOp,
        left: Dest,
        right: Dest,
    },
    /// IEEE compare. Unordered is not less, greater, or equal.
    FCmp {
        left: Dest,
        right: Dest,
        cond: Cond,
        target: usize,
    },
    /// Inclusive float range. `fail` is the miss, including NaN.
    FInRange {
        src: Dest,
        lo: i64,
        hi: i64,
        fail: usize,
    },
    /// Store a word at the address of `symbol`. Publishes pool bools for `slc_rt_str_cmp`.
    StoreAbs {
        src: Dest,
        symbol: String,
    },
    /// Escaping `μ`. The stack is not mutated. `dst` receives the `Kont`.
    /// In tail position the function's return address is that continuation.
    Capture {
        dst: Dest,
    },
    /// Value-position escaping `μ`. The copy's top frame returns to `block`,
    /// so invoking the binder resumes after the `μ` instead of returning from
    /// the function. The body keeps running on the parent frame.
    CaptureJoin {
        block: usize,
        dst: Dest,
    },
    /// `Kont::jump`, then deliver `VAL`. The image is the heap object.
    Invoke {
        image: Dest,
    },
    /// Append the slice. A tail resume pops only the body frame first.
    Resume {
        image: Dest,
        tail: bool,
    },
    /// Push a prompt and call the handled thunk. `done` is offset 0 of the prompt.
    InstallPrompt {
        clauses: Dest,
        ret_closure: Dest,
        thunk: Dest,
        done: usize,
        prompt_map: u32,
    },
    /// The six perform steps. `after` receives a resumed value when this is not tail.
    Perform {
        op: u32,
        tail: bool,
        arg_is_pointer: bool,
        after: usize,
        /// ApplyTo frame: slots 0 and 1, eleven words.
        apply_map: u32,
        /// Nine-word continuation under a non-tail perform. Traces `VAL` when the payload is a pointer.
        cont_map: u32,
    },
    /// `slot` holds the delay or adapted object across the call. It is a traced slot.
    /// Tail and value position both return into the peel loop; `finish` emits `Ret`.
    Force {
        slot: u16,
    },
    /// Tuple of adapter and value sits in `slot` (and in `VAL` on entry).
    Adapt {
        slot: u16,
        map_id: u32,
    },
    /// Indirect call. The closure object is `closure`; the argument is `VAL`.
    CallClosure {
        closure: Dest,
        tail: bool,
        arg_is_pointer: bool,
    },
    /// Tag dispatch for a consumer whose kind is not known statically.
    Activate {
        consumer: Dest,
        tail: bool,
        arg_is_pointer: bool,
    },
    /// `rdi` is the frame. A returning call leaves the value in `rax` and does not move the segment.
    CallRt {
        symbol: String,
        arg: RtArg,
        noreturn: bool,
        /// `rax` is the result, moved into `VAL` after the spills reload.
        returns: bool,
    },
    /// Offering call. `rax` is the discriminant, stored in the untraced `disc` slot.
    /// `rdx` moves to `VAL`. A safepoint would drop both if they stayed in caller-saved registers.
    CallOffer {
        symbol: String,
        arg: RtArg,
        disc: u16,
        /// The value arguments, not the continuation. A scalar must not be traced as the live `r13`.
        arg_is_pointer: bool,
    },
}

/// Arguments after the frame pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtArg {
    Val,
    Env,
    /// `VAL` in `rsi`, this immediate in `rdx`.
    ValImm(i64),
    /// Tuple fields at offsets 24 and 32, then this immediate in `rcx`.
    PairImm(i64),
    /// Tuple fields at offsets 24, 32, and 40.
    Triple,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub insts: Vec<Inst>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub symbol: String,
    pub map_id: u32,
    pub frame_words: u32,
    pub val_is_pointer: bool,
    pub pointer_slots: Vec<u16>,
    /// First frame slot past the named slots, scratches, and match temps.
    /// A scalar argument is parked here when `val_is_pointer` would trace it.
    pub spill_base: u16,
    /// `map_id` plus the park slot. A peeled delay is rooted there for one poll;
    /// the ordinary map leaves the slot untraced so a scalar parked afterwards
    /// is not rebased.
    pub hide_map: u32,
    pub blocks: Vec<Block>,
    /// C-callable transfer from `slc_rt_start`. No SLC prologue.
    pub entry: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapRecord {
    pub map_id: u32,
    pub frame_words: u32,
    pub val_is_pointer: bool,
    pub slots: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub functions: Vec<Function>,
    /// Interned in first-appearance order. A tagged object's label is this index.
    pub labels: Vec<String>,
    pub pool_len: u32,
    pub maps: Vec<MapRecord>,
}

impl Module {
    pub fn function(&self, symbol: &str) -> &Function {
        self.functions.iter().find(|f| f.symbol == symbol).unwrap_or_else(|| {
            panic!(
                "no {symbol} in {}",
                self.functions.iter().map(|f| f.symbol.as_str()).collect::<Vec<_>>().join(", ")
            )
        })
    }
}

pub struct Compiled {
    pub module: Module,
    pub object: Vec<u8>,
}

/// Lower `defs` and write a relocatable object. `specs` supply binder types for stack maps.
/// `payloads` are variant and record field types, so a wildcard occurrence is traced from
/// the word it holds.
pub fn compile(
    defs: &[(String, Term)],
    specs: &[Specialization],
    payloads: &HashMap<String, Vec<Type>>,
    traits: &slc_syntax::traits::TraitInfo,
    operations: &[String],
    fuel: usize,
) -> Result<Compiled, String> {
    let folded =
        defs.iter().map(|(name, term)| (name.clone(), fold_term(term, fuel))).collect::<Vec<_>>();
    let module = lower::lower(&folded, specs, payloads, traits, operations)?;
    let object = encode(&module);
    Ok(Compiled { module, object })
}

/// A closed call re-embeds. A match stays, so a literal scrutinee still compiles its compare.
fn fold_term(term: &Term, fuel: usize) -> Term {
    if fuel > 0
        && let Term::Mu(binder, command) = term
        && let Command::Cut(_, CoTerm::App(_, tail)) = command.as_ref()
        && matches!(tail.as_ref(), CoTerm::Covar(name) if name == binder)
        && let Fold::Value(value) = try_fold(term, fuel)
    {
        return embed(&value);
    }
    match term {
        Term::Var(name) => Term::Var(name.clone()),
        Term::Lam(name, body) => Term::Lam(name.clone(), Box::new(fold_term(body, fuel))),
        Term::Mu(name, command) => Term::Mu(name.clone(), Box::new(fold_command(command, fuel))),
        Term::Tuple(items) => Term::Tuple(items.iter().map(|item| fold_term(item, fuel)).collect()),
        Term::Tag(label, payload) => Term::Tag(label.clone(), Box::new(fold_term(payload, fuel))),
        Term::CoMatch { owner, branches } => Term::CoMatch {
            owner: owner.clone(),
            branches: branches
                .iter()
                .map(|branch| CoMatchBranch {
                    label: branch.label.clone(),
                    binder: branch.binder.clone(),
                    body: Box::new(fold_command(&branch.body, fuel)),
                })
                .collect(),
        },
        Term::Co(co) => Term::Co(Box::new(fold_coterm(co, fuel))),
    }
}

fn fold_command(command: &Command, fuel: usize) -> Command {
    let Command::Cut(term, co) = command;
    Command::Cut(fold_term(term, fuel), fold_coterm(co, fuel))
}

fn fold_coterm(co: &CoTerm, fuel: usize) -> CoTerm {
    match co {
        CoTerm::Covar(name) => CoTerm::Covar(name.clone()),
        CoTerm::App(arg, tail) => {
            CoTerm::App(fold_term(arg, fuel), Box::new(fold_coterm(tail, fuel)))
        }
        CoTerm::MuTilde(name, command) => {
            CoTerm::MuTilde(name.clone(), Box::new(fold_command(command, fuel)))
        }
        CoTerm::Prj(index) => CoTerm::Prj(*index),
        CoTerm::CoCase { owner, branches } => CoTerm::CoCase {
            owner: owner.clone(),
            branches: branches
                .iter()
                .map(|branch| slc_core::coterm::CoCaseBranch {
                    label: branch.label.clone(),
                    binders: branch.binders.clone(),
                    body: Box::new(fold_command(&branch.body, fuel)),
                })
                .collect(),
        },
        CoTerm::MuTildeTensor(names, command) => {
            CoTerm::MuTildeTensor(names.clone(), Box::new(fold_command(command, fuel)))
        }
        CoTerm::Dtor(label, tail) => CoTerm::Dtor(label.clone(), Box::new(fold_coterm(tail, fuel))),
    }
}
