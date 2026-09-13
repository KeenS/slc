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
    /// A tuple: its components, in order.
    Tuple(Vec<Value>),
    Builtin(String),

    /// An open file handle: an id into the runtime's handle registry,
    /// produced by `fs::open` and spent by `fs::close`.
    File(u64),
    /// An effect operation: applying it performs the effect, capturing the
    /// continuation up to the nearest handler for `effect`. Its arguments
    /// arrive packed, as any call's do, so one application performs it.
    Operation {
        effect: String,
        op: String,
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
            (Value::Unit, Value::Unit) => true,
            (Value::File(a), Value::File(b)) => a == b,
            (Value::Operation { op: a, .. }, Value::Operation { op: b, .. }) => a == b,
            (Value::Resume(a), Value::Resume(b)) => crate::machine::Kont::ptr_eq(a, b),
            (Value::Kont(a), Value::Kont(b)) => crate::machine::Kont::ptr_eq(a, b),
            (Value::Tuple(a), Value::Tuple(b)) => a == b,
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
            Value::Operation { .. } => Type::ONE,
            Value::Resume(_) => Type::BOTTOM,
            Value::Unit => Type::ONE,
            Value::Tuple(items) => Type::Tensor(items.iter().map(Value::type_of).collect()),
            Value::Closure { .. } | Value::Builtin(_) | Value::PartialBuiltin(..) => Type::BOTTOM,
            Value::Tagged(label, _) => Type::Named(
                label.split_once("::").map(|(owner, _)| owner.to_string()).unwrap_or_default(),
                Vec::new(),
            ),
            Value::CoCase { .. } | Value::CoTensor { .. } => Type::BOTTOM,
            Value::Menu { .. } => Type::BOTTOM,
            Value::Kont(_) => Type::BOTTOM,
        }
    }

    pub fn display(&self) -> String {
        match self {
            Value::Int(n) => format!("{n}"),
            Value::Float(n) => format!("{n}"),
            Value::Str(s) => format!("{s:?}"),
            Value::Bool(b) => format!("{b}"),
            Value::Char(c) => format!("{c:?}"),
            Value::Unit => "(,)".to_string(),

            Value::File(id) => format!("<file@{id}>"),
            Value::Operation { op, .. } => format!("<operation {op}>"),
            Value::Resume(_) => "<resume>".to_string(),
            Value::Tuple(items) => {
                format!("({})", items.iter().map(Value::display).collect::<Vec<_>>().join(", "))
            }
            Value::Closure { .. } => "<closure>".to_string(),
            Value::Kont(_) => "<continuation>".to_string(),
            Value::Builtin(s) => format!("<builtin {s}>"),
            Value::PartialBuiltin(s, args) => {
                format!("<partial {s} with {} args>", args.len())
            }
            // An alternative of an anonymous sum, by its position.
            Value::Tagged(label, payload) if alternative_index(label).is_some() => {
                format!("::{}({})", &label[1..], payload.display())
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

/// The position an anonymous sum's label names: `|2` is its third
/// alternative.
pub(crate) fn alternative_index(label: &str) -> Option<usize> {
    label.strip_prefix('|')?.parse().ok()
}

/// Install the standard library builtins into an environment.
pub fn install_stdlib(env: &mut Env) {
    let builtins = [
        "EXIT",
        "__index",
        "parse_int",
        "__display",
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
        "__read_file",
        "__open_file",
        "__read_line",
        "__close_file",
        "__write_file",
        "__file_exists",
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

/// The operations of `IO` and the runtime clause that answers each. The
/// prelude declares the effect; this is the handler the runtime installs
/// around `main`.
pub const IO_CLAUSES: [(&str, &str); 2] =
    [("write", "__io_write"), ("write_line", "__io_write_line")];
