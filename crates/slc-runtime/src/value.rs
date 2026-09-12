//! Runtime values for the Slant interpreter.

use slc_core::types::Type;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// A persistent environment in three layers.
///
/// The compiler resolves every lexical binder to a de Bruijn index, so those
/// bindings live in the **positional** chain (`locals`), reached by counting
/// from the head — the innermost binder is index 0. What the compiler could
/// not resolve — a global, a literal, or a `match` arm's pattern variables,
/// which the pattern engine injects at run time — is reached by name: the
/// named **overlay** (`overlay`) first, then the shared **globals** table.
///
/// Keeping pattern injections in their own by-name overlay is what lets de
/// Bruijn indices be stable: an injected binding never shifts the positional
/// chain, yet, being part of the environment, it is still captured by a
/// closure that escapes the arm. Every layer is a cons of `Rc` links, so
/// cloning the environment — which the machine does on nearly every step —
/// bumps refcounts rather than copying, and extending one chain leaves a
/// clone taken earlier with its own view.
#[derive(Debug, Clone)]
pub struct Env {
    globals: Rc<RefCell<HashMap<String, Value>>>,
    locals: Option<Rc<Slot>>,
    overlay: Option<Rc<Named>>,
}

#[derive(Debug)]
struct Slot {
    value: Value,
    parent: Option<Rc<Slot>>,
}

#[derive(Debug)]
struct Named {
    name: String,
    value: Value,
    parent: Option<Rc<Named>>,
}

impl Env {
    pub fn new() -> Self {
        Self { globals: Rc::new(RefCell::new(HashMap::new())), locals: None, overlay: None }
    }

    /// Kept for the existing push/define call shape; bindings link
    /// individually, so a frame boundary is nothing to open or unwind.
    pub fn push(&mut self) {}
    pub fn pop(&mut self) {}

    /// Push a lexical binding onto the positional chain. It becomes de Bruijn
    /// index 0 for the scope that follows.
    pub fn define_local(&mut self, v: Value) {
        self.locals = Some(Rc::new(Slot { value: v, parent: self.locals.take() }));
    }

    /// The value at de Bruijn index `i` (0 = innermost), if the chain is that
    /// deep.
    pub fn local(&self, i: usize) -> Option<Value> {
        let mut slot = self.locals.as_deref();
        for _ in 0..i {
            slot = slot?.parent.as_deref();
        }
        slot.map(|s| s.value.clone())
    }

    /// Inject a binding reachable by name (a `match` arm's pattern variable).
    pub fn define_named(&mut self, name: impl Into<String>, v: Value) {
        self.overlay =
            Some(Rc::new(Named { name: name.into(), value: v, parent: self.overlay.take() }));
    }

    /// Define a global (top-level) binding, visible from every env derived
    /// from this one via clone.
    pub fn define_global(&mut self, name: impl Into<String>, v: Value) {
        self.globals.borrow_mut().insert(name.into(), v);
    }

    /// Resolve a name: the overlay first, then the globals table. The
    /// positional chain is never consulted by name.
    pub fn lookup(&self, name: &str) -> Option<Value> {
        let mut scope = self.overlay.as_deref();
        while let Some(s) = scope {
            if s.name == name {
                return Some(s.value.clone());
            }
            scope = s.parent.as_deref();
        }
        self.globals.borrow().get(name).cloned()
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}

/// Runtime value.
#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Char(char),
    Unit,
    Closure {
        body: crate::chunk::NodeId,
        env: Env,
    },
    /// A captured continuation: the machine's frame stack, reified. It can
    /// be reinstated any number of times, at any time — activating it
    /// replaces the current stack, which is what makes the jump.
    Kont(crate::machine::Kont),
    Pair(Box<Value>, Box<Value>),
    Builtin(String),
    /// The marker a call with no arguments applies its callee to. It is not
    /// unit: `f()` passes nothing, while `f(())` passes the unit value.
    NoArguments,
    /// An open file handle: an id into the runtime's handle registry,
    /// produced by `open_file` and spent by `close_file`.
    File(u64),
    /// An effect operation: applying it performs the effect, capturing the
    /// continuation up to the nearest handler for `effect`. An operation of
    /// several parameters collects them first — calls are curried, so it
    /// would otherwise perform on its first argument and apply the rest to
    /// the handler's answer.
    Operation {
        effect: String,
        op: String,
        arity: usize,
        collected: Vec<Value>,
    },
    /// A delimited, composable continuation — a handler's `resume`. Applying
    /// it prepends its captured frames onto the current stack, so control
    /// runs the captured work and then re-enters the handler.
    Resume(crate::machine::Kont),
    /// An `enum` value: a variant label and its payload.
    Tagged(String, Box<Value>),
    /// A negative additive consumer (`select`): the branches of a core
    /// `μ̃[…]` co-term together with the environment they closed over.
    /// Branch bodies are held unevaluated; activation runs exactly one.
    CoCase {
        co: crate::chunk::NodeId,
        env: Env,
    },
    /// A menu value (`μ[…]`): the branches of a core menu term together
    /// with the environment they closed over. Branch bodies are held
    /// unevaluated; the request that arrives runs exactly one.
    Menu {
        node: crate::chunk::NodeId,
        env: Env,
    },
    /// A consumer of a product (`μ̃(x, y). c`): it binds every component of
    /// the value it is given, `arity` of them.
    CoTensor {
        co: crate::chunk::NodeId,
        env: Env,
    },
    /// A builtin that has already received some arguments.
    PartialBuiltin(String, Vec<Value>),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Char(a), Value::Char(b)) => a == b,
            (Value::Unit, Value::Unit) | (Value::NoArguments, Value::NoArguments) => true,
            (Value::File(a), Value::File(b)) => a == b,
            (Value::Operation { op: a, .. }, Value::Operation { op: b, .. }) => a == b,
            (Value::Resume(a), Value::Resume(b)) => crate::machine::Kont::ptr_eq(a, b),
            (Value::Kont(a), Value::Kont(b)) => crate::machine::Kont::ptr_eq(a, b),
            (Value::Pair(a1, a2), Value::Pair(b1, b2)) => a1 == b1 && a2 == b2,
            (Value::Builtin(a), Value::Builtin(b)) => a == b,
            (Value::PartialBuiltin(a, args1), Value::PartialBuiltin(b, args2)) => {
                a == b && args1 == args2
            }
            (Value::Tagged(a, pa), Value::Tagged(b, pb)) => a == b && pa == pb,
            _ => false,
        }
    }
}

impl Value {
    pub fn type_of(&self) -> Type {
        match self {
            Value::Int(_) => Type::Pos(slc_core::types::Base::I64),
            Value::Float(_) => Type::Pos(slc_core::types::Base::Unit),
            Value::Str(_) => Type::Pos(slc_core::types::Base::Str),
            Value::Bool(_) => Type::Pos(slc_core::types::Base::Bool),
            Value::Char(_) => Type::Pos(slc_core::types::Base::Char),
            Value::File(_) => Type::Pos(slc_core::types::Base::File),
            Value::Operation { .. } => Type::One,
            Value::Resume(_) => Type::Bottom,
            Value::Unit | Value::NoArguments => Type::One,
            Value::Pair(a, b) => Type::Tensor(Box::new(a.type_of()), Box::new(b.type_of())),
            Value::Closure { .. } | Value::Builtin(_) | Value::PartialBuiltin(..) => Type::Bottom,
            Value::Tagged(label, _) => Type::Named(
                label.split_once("::").map(|(owner, _)| owner.to_string()).unwrap_or_default(),
                Vec::new(),
            ),
            Value::CoCase { .. } | Value::CoTensor { .. } => Type::Bottom,
            Value::Menu { .. } => Type::Bottom,
            Value::Kont(_) => Type::Bottom,
        }
    }

    pub fn display(&self) -> String {
        match self {
            Value::Int(n) => format!("{n}"),
            Value::Float(n) => format!("{n}"),
            Value::Str(s) => format!("{s:?}"),
            Value::Bool(b) => format!("{b}"),
            Value::Char(c) => format!("{c:?}"),
            Value::Unit => "()".to_string(),
            Value::NoArguments => "<no arguments>".to_string(),
            Value::File(id) => format!("<file@{id}>"),
            Value::Operation { op, .. } => format!("<operation {op}>"),
            Value::Resume(_) => "<resume>".to_string(),
            Value::Pair(a, b) => format!("({}, {})", a.display(), b.display()),
            Value::Closure { .. } => "<closure>".to_string(),
            Value::Kont(_) => "<continuation>".to_string(),
            Value::Builtin(s) => format!("<builtin {s}>"),
            Value::PartialBuiltin(s, args) => {
                format!("<partial {s} with {} args>", args.len())
            }
            Value::Tagged(label, payload) => match payload.as_ref() {
                Value::Unit => label.clone(),
                payload => format!("{label}({})", payload.display()),
            },
            Value::CoCase { .. } => "<select>".to_string(),
            Value::CoTensor { .. } => "<consumer>".to_string(),
            Value::Menu { .. } => "<menu>".to_string(),
        }
    }
}

/// Install the standard library builtins into an environment.
pub fn install_stdlib(env: &mut Env) {
    let builtins = [
        "EXIT",
        "__index",
        "parse_int",
        "println",
        "print",
        "format",
        "neg",
        "add",
        "sub",
        "mul",
        "div",
        "rem",
        "eq",
        "ne",
        "lt",
        "gt",
        "le",
        "ge",
        "str_len",
        "str_concat",
        "int_to_str",
        "read_file",
        "open_file",
        "read_line",
        "close_file",
        "write_file",
        "file_exists",
        "__if_dispatch",
        "__match_dispatch",
        "__handle",
        "char_at",
        "is_digit",
        "is_ws",
        "skip_digits",
        "find_char",
        "skip_ws",
        "substring",
        "str_eq",
    ];
    for b in builtins {
        env.define_global(b, Value::Builtin(b.to_string()));
    }
}
