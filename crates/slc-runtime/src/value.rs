//! Runtime values for the Slant interpreter.

use slc_core::types::Type;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// A persistent environment: chain of frames.
#[derive(Debug, Clone)]
pub struct Env {
    /// Shared global definitions, visible in every frame.
    globals: Rc<RefCell<HashMap<String, Value>>>,
    frames: Vec<HashMap<String, Value>>,
}

impl Env {
    pub fn new() -> Self {
        Self { globals: Rc::new(RefCell::new(HashMap::new())), frames: vec![] }
    }

    pub fn push(&mut self) {
        self.frames.push(HashMap::new());
    }

    pub fn pop(&mut self) {
        self.frames.pop();
    }

    pub fn define(&mut self, name: impl Into<String>, v: Value) {
        if let Some(frame) = self.frames.last_mut() {
            frame.insert(name.into(), v);
        } else {
            self.globals.borrow_mut().insert(name.into(), v);
        }
    }

    /// Define a global (top-level) binding, visible from every env
    /// derived from this one via clone.
    pub fn define_global(&mut self, name: impl Into<String>, v: Value) {
        self.globals.borrow_mut().insert(name.into(), v);
    }

    pub fn lookup(&self, name: &str) -> Option<Value> {
        for frame in self.frames.iter().rev() {
            if let Some(v) = frame.get(name) {
                return Some(v.clone());
            }
        }
        self.globals.borrow().get(name).cloned()
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}

/// A continuation value: what to do with a result.
#[derive(Clone)]
pub struct Cont {
    pub env: Env,
    pub command: Rc<slc_core::command::Command>,
}

impl std::fmt::Debug for Cont {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Cont(<command>)")
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
        param: String,
        body: Rc<slc_core::term::Term>,
        env: Env,
    },
    Continuation(Cont),
    /// A closure returned from a μ command; activating it may escape.
    EscapedClosure {
        escape_id: usize,
        closure: Box<Value>,
    },
    Pair(Box<Value>, Box<Value>),
    Inl(Box<Value>),
    Inr(Box<Value>),
    Builtin(String),
    /// The marker a call with no arguments applies its callee to. It is not
    /// unit: `f()` passes nothing, while `f(())` passes the unit value.
    NoArguments,
    /// An `enum` value: a variant label and its payload.
    Tagged(String, Box<Value>),
    /// A negative additive consumer (`select`): the branches of a core
    /// `μ̃[…]` co-term together with the environment they closed over.
    /// Branch bodies are held unevaluated; activation runs exactly one.
    CoCase {
        branches: Rc<Vec<slc_core::coterm::CoCaseBranch>>,
        env: Env,
    },
    /// A consumer of a product (`μ̃(x, y). c`): it binds every component of
    /// the value it is given.
    CoTensor {
        binders: Rc<Vec<String>>,
        body: Rc<slc_core::command::Command>,
        env: Env,
    },
    /// A μ abstraction waiting for the continuation to bind its co-variable.
    /// Applying it binds the co-variable to the supplied continuation.
    CoAbs {
        covar: String,
        body: Rc<slc_core::term::Term>,
        env: Env,
    },
    /// A builtin that has already received some arguments.
    PartialBuiltin(String, Vec<Value>),
    /// A list value (v0.1: built via list builtins).
    List(Vec<Value>),
    /// A map value.
    Map(Vec<(Value, Value)>),
    /// A set value.
    Set(Vec<Value>),
    Never,
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Char(a), Value::Char(b)) => a == b,
            (Value::Unit, Value::Unit)
            | (Value::Never, Value::Never)
            | (Value::NoArguments, Value::NoArguments) => true,
            (Value::Pair(a1, a2), Value::Pair(b1, b2)) => a1 == b1 && a2 == b2,
            (Value::Inl(a), Value::Inl(b)) | (Value::Inr(a), Value::Inr(b)) => a == b,
            (Value::Builtin(a), Value::Builtin(b)) => a == b,
            (Value::PartialBuiltin(a, args1), Value::PartialBuiltin(b, args2)) => {
                a == b && args1 == args2
            }
            (Value::List(a), Value::List(b)) => a == b,
            (Value::Map(a), Value::Map(b)) => a == b,
            (Value::Set(a), Value::Set(b)) => a == b,
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
            Value::Unit | Value::Never | Value::NoArguments => Type::One,
            Value::Pair(a, b) => Type::Tensor(Box::new(a.type_of()), Box::new(b.type_of())),
            Value::Inl(a) => Type::Sum(Box::new(a.type_of()), Box::new(Type::Bottom)),
            Value::Inr(a) => Type::Sum(Box::new(Type::Bottom), Box::new(a.type_of())),
            Value::List(items) => {
                Type::List(Box::new(items.first().map(|v| v.type_of()).unwrap_or(Type::One)))
            }
            Value::Map(entries) => {
                Type::List(Box::new(entries.first().map(|(k, _)| k.type_of()).unwrap_or(Type::One)))
            }
            Value::Set(items) => {
                Type::List(Box::new(items.first().map(|v| v.type_of()).unwrap_or(Type::One)))
            }
            Value::Closure { .. }
            | Value::Continuation(_)
            | Value::Builtin(_)
            | Value::PartialBuiltin(..) => Type::Bottom,
            Value::Tagged(label, _) => Type::Named(
                label.split_once("::").map(|(owner, _)| owner.to_string()).unwrap_or_default(),
            ),
            Value::CoCase { .. } | Value::CoTensor { .. } | Value::CoAbs { .. } => Type::Bottom,
            Value::EscapedClosure { .. } => Type::Bottom,
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
            Value::Pair(a, b) => format!("({}, {})", a.display(), b.display()),
            Value::Inl(a) => format!("inl({})", a.display()),
            Value::Inr(a) => format!("inr({})", a.display()),
            Value::Closure { .. } => "<closure>".to_string(),
            Value::EscapedClosure { .. } => "<continuation>".to_string(),
            Value::Continuation(_) => "<continuation>".to_string(),
            Value::Builtin(s) => format!("<builtin {s}>"),
            Value::PartialBuiltin(s, args) => {
                format!("<partial {s} with {} args>", args.len())
            }
            Value::List(items) => {
                let inner: Vec<String> = items.iter().map(|v| v.display()).collect();
                format!("[{}]", inner.join(", "))
            }
            Value::Map(entries) => {
                let inner: Vec<String> = entries
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k.display(), v.display()))
                    .collect();
                format!("{{{}}}", inner.join(", "))
            }
            Value::Set(items) => {
                let inner: Vec<String> = items.iter().map(|v| v.display()).collect();
                format!("{{{}}}", inner.join(", "))
            }
            Value::Never => "<never>".to_string(),
            Value::Tagged(label, payload) => match payload.as_ref() {
                Value::Unit => label.clone(),
                payload => format!("{label}({})", payload.display()),
            },
            Value::CoCase { branches, .. } => {
                let inner: Vec<String> =
                    branches.iter().map(|branch| branch.label.clone()).collect();
                format!("select {{{}}}", inner.join(" | "))
            }
            Value::CoTensor { binders, .. } => format!("consumer({})", binders.join(", ")),
            Value::CoAbs { covar, .. } => format!("<continuation μ{covar}>"),
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
        "write_file",
        "file_exists",
        "__if_dispatch",
        "__match_dispatch",
        "char_at",
        "list_new",
        "list_len",
        "list_push",
        "list_get",
        "map_new",
        "map_insert",
        "map_get",
        "map_len",
        "set_new",
        "set_insert",
        "set_contains",
        "set_len",
        "path_join",
        "path_basename",
        "path_dirname",
        "path_extension",
        "is_digit",
        "is_ws",
        "skip_digits",
        "find_char",
        "skip_ws",
        "substring",
        "str_eq",
    ];
    env.push();
    for b in builtins {
        env.define(b, Value::Builtin(b.to_string()));
    }
}
