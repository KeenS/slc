//! x86-64 for straight-line code, both match forms, and escaping captures.
//!
//! The ELF is machine code. The control-flow graph stays in this process.
//! Capture, invoke, and resume call the runtime copier. The encoder does not
//! copy frames itself.

mod encode;
mod lower;

#[cfg(test)]
mod link_tests;

use std::collections::HashMap;

use slc_core::term::Term;
use slc_core::types::Type;
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
    /// Compare two words. Used by the one inlined `__gt`.
    CmpRR {
        left: Dest,
        right: Dest,
        cond: Cond,
        target: usize,
    },
    /// Escaping `μ`. The stack is not mutated. `dst` receives the `Kont`.
    Capture {
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
    Force {
        tail: bool,
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
    /// `rdi` is the frame. `arg` selects `VAL` or `ENV` for `rsi`.
    CallRt {
        symbol: String,
        arg: RtArg,
        noreturn: bool,
    },
}

/// Which protocol register is the second argument of a runtime call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtArg {
    Val,
    Env,
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
) -> Result<Compiled, String> {
    let module = lower::lower(defs, specs, payloads)?;
    let object = encode(&module);
    Ok(Compiled { module, object })
}
