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
    Pair(Box<Value>, Box<Value>),
    Inl(Box<Value>),
    Inr(Box<Value>),
    Builtin(String),
    /// A builtin that has already received some arguments.
    PartialBuiltin(String, Vec<Value>),
    /// A list value (v0.1: built via list builtins).
    List(Vec<Value>),
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
            (Value::Unit, Value::Unit) | (Value::Never, Value::Never) => true,
            (Value::Pair(a1, a2), Value::Pair(b1, b2)) => a1 == b1 && a2 == b2,
            (Value::Inl(a), Value::Inl(b)) | (Value::Inr(a), Value::Inr(b)) => a == b,
            (Value::Builtin(a), Value::Builtin(b)) => a == b,
            (Value::PartialBuiltin(a, args1), Value::PartialBuiltin(b, args2)) => {
                a == b && args1 == args2
            }
            (Value::List(a), Value::List(b)) => a == b,
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
            Value::Char(_) => Type::Pos(slc_core::types::Base::Unit),
            Value::Unit | Value::Never => Type::One,
            Value::Pair(a, b) => Type::Tensor(Box::new(a.type_of()), Box::new(b.type_of())),
            Value::Inl(a) => Type::Sum(Box::new(a.type_of()), Box::new(Type::Bottom)),
            Value::Inr(a) => Type::Sum(Box::new(Type::Bottom), Box::new(a.type_of())),
            Value::List(items) => {
                Type::List(Box::new(items.first().map(|v| v.type_of()).unwrap_or(Type::One)))
            }
            Value::Closure { .. }
            | Value::Continuation(_)
            | Value::Builtin(_)
            | Value::PartialBuiltin(..) => Type::Bottom,
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
            Value::Pair(a, b) => format!("({}, {})", a.display(), b.display()),
            Value::Inl(a) => format!("inl({})", a.display()),
            Value::Inr(a) => format!("inr({})", a.display()),
            Value::Closure { .. } => "<closure>".to_string(),
            Value::Continuation(_) => "<continuation>".to_string(),
            Value::Builtin(s) => format!("<builtin {s}>"),
            Value::PartialBuiltin(s, args) => {
                format!("<partial {s} with {} args>", args.len())
            }
            Value::List(items) => {
                let inner: Vec<String> = items.iter().map(|v| v.display()).collect();
                format!("[{}]", inner.join(", "))
            }
            Value::Never => "<never>".to_string(),
        }
    }
}

/// Install the standard library builtins into an environment.
pub fn install_stdlib(env: &mut Env) {
    let builtins = [
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
        "char_at",
        "list_len",
        "list_push",
        "list_get",
        "is_digit",
        "is_ws",
        "skip_digits",
        "find_char",
        "skip_ws",
        "substring",
        "str_to_int",
        "str_eq",
    ];
    env.push();
    for b in builtins {
        env.define(b, Value::Builtin(b.to_string()));
    }
}
