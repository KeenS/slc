//! Tree-walking evaluator over the core IR.

use crate::value::{Env, Value};
use slc_core::command::Command;
use slc_core::coterm::CoTerm;
use slc_core::term::Term;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
pub enum EvalError {
    Unbound(String),
    TypeMismatch(String),
    Diverged,
    NoReduction,
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvalError::Unbound(x) => write!(f, "unbound variable: {x}"),
            EvalError::TypeMismatch(m) => write!(f, "type mismatch: {m}"),
            EvalError::Diverged => write!(f, "evaluation diverged (fuel exhausted)"),
            EvalError::NoReduction => write!(f, "no applicable reduction"),
        }
    }
}

impl std::error::Error for EvalError {}

/// Evaluate a term to a value.
pub fn eval(t: &Term, env: &mut Env, fuel: &mut usize) -> Result<Value, EvalError> {
    if *fuel == 0 {
        return Err(EvalError::Diverged);
    }
    *fuel -= 1;

    match t {
        Term::Var(x) => {
            // Builtins
            match x.as_str() {
                n if n.starts_with("int_") => {
                    let v: i64 = n[4..].parse().map_err(|_| EvalError::Unbound(x.clone()))?;
                    return Ok(Value::Int(v));
                }
                n if n.starts_with("str_") => {
                    let s = &n[4..];
                    let s = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')).unwrap_or(s);
                    let s = s.replace("\\n", "\n").replace("\\t", "\t");
                    return Ok(Value::Str(s));
                }
                "true" => return Ok(Value::Bool(true)),
                "false" => return Ok(Value::Bool(false)),
                "unit" => return Ok(Value::Unit),
                _ => {}
            }
            env.lookup(x).cloned().ok_or(EvalError::Unbound(x.clone()))
        }

        Term::Lam(param, body) => Ok(Value::Closure {
            param: param.clone(),
            body: Rc::new((**body).clone()),
            env: env.clone(),
        }),

        Term::Mu(a, command) => {
            // μ abstraction: capture the environment as a continuation
            let _ = a;
            Ok(Value::Continuation(crate::value::Cont {
                env: env.clone(),
                command: Rc::new((**command).clone()),
            }))
        }

        Term::Pair(t1, t2) => {
            let v1 = eval(t1, env, fuel)?;
            let v2 = eval(t2, env, fuel)?;
            Ok(Value::Pair(Box::new(v1), Box::new(v2)))
        }

        Term::Inl(t) => Ok(Value::Inl(Box::new(eval(t, env, fuel)?))),
        Term::Inr(t) => Ok(Value::Inr(Box::new(eval(t, env, fuel)?))),
    }
}

/// Evaluate a command.
pub fn eval_command(c: &Command, env: &mut Env, fuel: &mut usize) -> Result<Value, EvalError> {
    if *fuel == 0 {
        return Err(EvalError::Diverged);
    }
    *fuel -= 1;

    match c {
        Command::Cut(t, e) => {
            let v = eval(t, env, fuel)?;
            match e {
                CoTerm::Covar(a) => {
                    // Result goes to the co-variable; for top-level, return it
                    let _ = a;
                    Ok(v)
                }
                CoTerm::CoLam(x, c2) => {
                    let mut env2 = env.clone();
                    env2.push();
                    env2.define(x.clone(), v);
                    let r = eval_command(c2, &mut env2, fuel)?;
                    Ok(r)
                }
                CoTerm::MuTilde(_, _) => {
                    // Already reduced by β; here treat as return
                    Ok(v)
                }
                _ => Ok(v),
            }
        }
        Command::Command(x, t) => {
            let v = eval(t, env, fuel)?;
            let _ = x;
            Ok(v)
        }
        Command::Activate(k, v) => {
            // k(v): activate the continuation k with value v
            let kv = eval(k, env, fuel)?;
            let _vv = eval(v, env, fuel)?;
            match kv {
                Value::Continuation(cont) => {
                    let mut env2 = cont.env;
                    eval_command(&cont.command, &mut env2, fuel)
                }
                other => Err(EvalError::TypeMismatch(format!(
                    "cannot activate non-continuation: {}",
                    other.display()
                ))),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_int_literal() {
        let t = Term::Var("int_42".into());
        let mut env = Env::new();
        let mut fuel = 100;
        assert_eq!(eval(&t, &mut env, &mut fuel).unwrap(), Value::Int(42));
    }

    #[test]
    fn eval_string_literal() {
        let t = Term::Var("str_hello".into());
        let mut env = Env::new();
        let mut fuel = 100;
        assert_eq!(eval(&t, &mut env, &mut fuel).unwrap(), Value::Str("hello".into()));
    }

    #[test]
    fn eval_bool() {
        let t = Term::Var("true".into());
        let mut env = Env::new();
        let mut fuel = 100;
        assert_eq!(eval(&t, &mut env, &mut fuel).unwrap(), Value::Bool(true));
    }

    #[test]
    fn eval_lambda() {
        let t = Term::Lam("x".into(), Box::new(Term::Var("x".into())));
        let mut env = Env::new();
        let mut fuel = 100;
        let v = eval(&t, &mut env, &mut fuel).unwrap();
        assert!(matches!(v, Value::Closure { .. }));
    }

    #[test]
    fn eval_pair() {
        let t =
            Term::Pair(Box::new(Term::Var("int_1".into())), Box::new(Term::Var("int_2".into())));
        let mut env = Env::new();
        let mut fuel = 100;
        assert_eq!(
            eval(&t, &mut env, &mut fuel).unwrap(),
            Value::Pair(Box::new(Value::Int(1)), Box::new(Value::Int(2)))
        );
    }

    #[test]
    fn eval_unbound() {
        let t = Term::Var("missing".into());
        let mut env = Env::new();
        let mut fuel = 100;
        assert!(matches!(eval(&t, &mut env, &mut fuel), Err(EvalError::Unbound(_))));
    }

    #[test]
    fn eval_fuel_exhaustion() {
        let t = Term::Var("int_1".into());
        let mut env = Env::new();
        let mut fuel = 0;
        assert!(matches!(eval(&t, &mut env, &mut fuel), Err(EvalError::Diverged)));
    }

    #[test]
    fn eval_cut_returns_term() {
        let c = Command::Cut(Term::Var("int_7".into()), CoTerm::Covar("k".into()));
        let mut env = Env::new();
        let mut fuel = 100;
        assert_eq!(eval_command(&c, &mut env, &mut fuel).unwrap(), Value::Int(7));
    }

    #[test]
    fn eval_colam_binds() {
        // ⟨ int_5 ∥ λ̄x. ⟨ x ∥ k ⟩ ⟩ → 5
        let inner = Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into()));
        let c = Command::Cut(Term::Var("int_5".into()), CoTerm::CoLam("x".into(), Box::new(inner)));
        let mut env = Env::new();
        let mut fuel = 100;
        assert_eq!(eval_command(&c, &mut env, &mut fuel).unwrap(), Value::Int(5));
    }
}
