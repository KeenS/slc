//! The abstract machine: the evaluator with its continuation as data.
//!
//! λ̄μμ̃ is a machine calculus — a command ⟨t ∥ e⟩ is a state with the
//! control on the left and the continuation on the right — and this module
//! finally runs it that way. The interpreter used to hold "the rest of the
//! computation" as Rust stack frames, which made a captured continuation an
//! escape marker: one shot, upward only, dead once its `mu` returned. Here
//! the rest of the computation is the `Kont` stack, a value like any other:
//!
//! - `mu(k)` captures the frame stack into `Value::Kont`;
//! - activating a `Kont` *replaces* the frame stack and delivers the value —
//!   however deep the machine is, however long ago the capture returned,
//!   however many times it has been used before.
//!
//! The stack is a persistent cons (`Kont`) with the top at the head, so a
//! capture — `mu`, or a handler's `resume` — clones one `Rc`: O(1), no
//! matter how deep, and repeatable. Pushing a frame never disturbs a stack
//! already captured, so a resumed continuation walks its own copy.

use crate::eval::{
    EvalError, bind_components, builtin_arity, collect_args, is_applicable, run_builtin_function,
};
use crate::matching::{
    Descriptor, parse_runtime_pattern, pattern_matches, split_match_payload, unwrap_match_arm,
};
use crate::value::{Env, Value};
use slc_core::command::Command;
use slc_core::coterm::CoTerm;
use slc_core::term::Term;
use std::rc::Rc;

/// What the machine is doing right now.
pub(crate) enum State {
    Term(Rc<Term>, Env),
    Command(Rc<Command>, Env),
    Apply { callee: Value, arg: Value },
    Return(Value),
}

/// One saved step of the computation: what happens to the value being
/// produced. The whole stack is the continuation.
#[derive(Debug, Clone)]
pub enum Frame {
    /// `(v ⊗ _)` — the first component is done; evaluate the second.
    PairRight(Rc<Term>, Env),
    /// `(v1 ⊗ v2)` — both components done; build the pair.
    PairDone(Value),
    WrapInl,
    WrapInr,
    WrapTag(String),
    /// `⟨ _ ∥ e ⟩` — the term side is being evaluated; consume with `e`.
    Consume(Rc<CoTerm>, Env),
    /// The callee is evaluated; the argument is being computed.
    ApplyCallee(Value),
    /// `k(v)`: the consumer is being evaluated; `v` comes next.
    ActivateArg(Rc<Term>, Env),
    /// `k(v)`: the consumer is known; the value is being evaluated.
    ActivateWith(Value),
    /// A handler delimiter: the `return` clause, and the operation clauses
    /// of one effect. Sits on the stack under the body it handles.
    Prompt {
        clauses: std::rc::Rc<std::collections::HashMap<String, Value>>,
        ret: Value,
    },
    /// `resume(v)` then apply the produced closure to this value: a handler
    /// clause is `λarg. λresume. body`, so after `clause(arg)` we apply the
    /// result to `resume`.
    ApplyTo(Value),
    /// A match in progress: the guard of a candidate arm is being evaluated.
    MatchGuard {
        scrutinee: Value,
        thunk: Value,
        bindings: Vec<(String, Value)>,
        remaining: Vec<Value>,
    },
}

/// The continuation as a persistent stack: a shared cons of frames with the
/// top at the head. Capturing it — `mu`'s `Value::Kont`, or a handler's
/// `resume` — clones one `Rc`, regardless of depth, and the clone walks
/// independently of the live stack. That is what makes a jump and a
/// multi-shot `resume` cheap.
#[derive(Clone, Debug, Default)]
pub struct Kont(Option<Rc<KontNode>>);

#[derive(Debug)]
struct KontNode {
    frame: Frame,
    tail: Option<Rc<KontNode>>,
}

impl Kont {
    pub(crate) fn empty() -> Self {
        Kont(None)
    }

    /// Push a frame onto the top. A stack already captured elsewhere keeps
    /// its own view — the push only extends this handle.
    pub(crate) fn push(&mut self, frame: Frame) {
        let tail = self.0.take();
        self.0 = Some(Rc::new(KontNode { frame, tail }));
    }

    /// Pop the top frame. A shared node is cloned out rather than unwrapped,
    /// so another handle onto the same stack is left intact.
    pub(crate) fn pop(&mut self) -> Option<Frame> {
        let node = self.0.take()?;
        match Rc::try_unwrap(node) {
            Ok(node) => {
                self.0 = node.tail;
                Some(node.frame)
            }
            Err(shared) => {
                self.0 = shared.tail.clone();
                Some(shared.frame.clone())
            }
        }
    }

    /// Two captured stacks are equal when they are the same node.
    pub(crate) fn ptr_eq(a: &Kont, b: &Kont) -> bool {
        match (&a.0, &b.0) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
    }

    /// Split at the nearest handler above that handles `op`: hand back its
    /// clause and the delimited continuation (the work up to and including
    /// that `Prompt`, which therefore reinstates it on resume), and truncate
    /// `self` to what lay below the handler. `None` if nothing above handles
    /// `op`.
    fn split_at_handler(&mut self, op: &str) -> Option<(Value, Kont)> {
        let mut prefix: Vec<Frame> = Vec::new();
        let mut cursor = self.0.clone();
        loop {
            let node = cursor?;
            match &node.frame {
                Frame::Prompt { clauses, .. } if clauses.contains_key(op) => {
                    let clause = clauses.get(op).cloned().expect("checked present");
                    prefix.push(node.frame.clone());
                    let mut captured = Kont::empty();
                    for frame in prefix.into_iter().rev() {
                        captured.push(frame);
                    }
                    self.0 = node.tail.clone();
                    return Some((clause, captured));
                }
                _ => {
                    prefix.push(node.frame.clone());
                    cursor = node.tail.clone();
                }
            }
        }
    }
}

/// Run a term to a value with an empty continuation.
pub(crate) fn run_term(t: &Term, env: &Env, fuel: &mut usize) -> Result<Value, EvalError> {
    run(State::Term(Rc::new(t.clone()), env.clone()), Kont::empty(), fuel)
}

/// Run a command to a value with an empty continuation.
pub(crate) fn run_command(c: &Command, env: &Env, fuel: &mut usize) -> Result<Value, EvalError> {
    run(State::Command(Rc::new(c.clone()), env.clone()), Kont::empty(), fuel)
}

/// Apply a value to an argument with an empty continuation.
pub(crate) fn run_apply(callee: Value, arg: Value, fuel: &mut usize) -> Result<Value, EvalError> {
    run(State::Apply { callee, arg }, Kont::empty(), fuel)
}

fn run(start: State, kont: Kont, fuel: &mut usize) -> Result<Value, EvalError> {
    let mut state = start;
    let mut kont = kont;
    loop {
        if *fuel == 0 {
            return Err(EvalError::Diverged);
        }
        *fuel -= 1;
        state = match state {
            State::Term(t, env) => step_term(&t, env, &mut kont)?,
            State::Command(c, env) => step_command(&c, env, &mut kont)?,
            State::Apply { callee, arg } => step_apply(callee, arg, &mut kont, fuel)?,
            State::Return(v) => match kont.pop() {
                None => return Ok(v),
                Some(frame) => step_frame(frame, v, &mut kont)?,
            },
        };
    }
}

fn step_term(t: &Term, env: Env, kont: &mut Kont) -> Result<State, EvalError> {
    Ok(match t {
        Term::Var(x) => State::Return(crate::eval::literal_or_lookup(x, &env)?),
        Term::Lam(param, body) => State::Return(Value::Closure {
            param: param.clone(),
            body: Rc::new((**body).clone()),
            env,
        }),
        Term::Mu(a, command) => {
            // The μ: bind the co-variable to the continuation itself. The
            // clone is the capture.
            let mut env2 = env;
            env2.push();
            env2.define(a, Value::Kont(kont.clone()));
            State::Command(Rc::new((**command).clone()), env2)
        }
        Term::Pair(t1, t2) => {
            kont.push(Frame::PairRight(Rc::new((**t2).clone()), env.clone()));
            State::Term(Rc::new((**t1).clone()), env)
        }
        Term::Inl(inner) => {
            kont.push(Frame::WrapInl);
            State::Term(Rc::new((**inner).clone()), env)
        }
        Term::Inr(inner) => {
            kont.push(Frame::WrapInr);
            State::Term(Rc::new((**inner).clone()), env)
        }
        Term::Tag(label, payload) => {
            kont.push(Frame::WrapTag(label.clone()));
            State::Term(Rc::new((**payload).clone()), env)
        }
        Term::CoAbs(covar, body) => State::Return(Value::CoAbs {
            covar: covar.clone(),
            body: Rc::new((**body).clone()),
            env,
        }),
        Term::Co(coterm) => State::Return(match coterm.as_ref() {
            // A negative additive consumer closes over its environment. Its
            // branch bodies stay unevaluated: activation runs exactly one.
            CoTerm::CoCase(branches) => Value::CoCase { branches: Rc::new(branches.clone()), env },
            CoTerm::MuTildeTensor(binders, body) => Value::CoTensor {
                binders: Rc::new(binders.clone()),
                body: Rc::new((**body).clone()),
                env,
            },
            // `μ̃x. c` binds the whole value: a product consumer of one part.
            CoTerm::MuTilde(binder, body) => Value::CoTensor {
                binders: Rc::new(vec![binder.clone()]),
                body: Rc::new((**body).clone()),
                env,
            },
            other => Value::Continuation(crate::value::Cont {
                env,
                command: Rc::new(Command::Cut(Term::Var("__co_arg".into()), other.clone())),
            }),
        }),
    })
}

fn step_command(c: &Command, env: Env, kont: &mut Kont) -> Result<State, EvalError> {
    Ok(match c {
        Command::Cut(t, e) => {
            kont.push(Frame::Consume(Rc::new(e.clone()), env.clone()));
            State::Term(Rc::new(t.clone()), env)
        }
        Command::Command(_, t) => State::Term(Rc::new(t.clone()), env),
        Command::Activate(k, v) => {
            kont.push(Frame::ActivateArg(Rc::new(v.clone()), env.clone()));
            State::Term(Rc::new(k.clone()), env)
        }
    })
}

fn step_frame(frame: Frame, v: Value, kont: &mut Kont) -> Result<State, EvalError> {
    Ok(match frame {
        Frame::PairRight(t2, env) => {
            kont.push(Frame::PairDone(v));
            State::Term(t2, env)
        }
        Frame::PairDone(first) => State::Return(Value::Pair(Box::new(first), Box::new(v))),
        Frame::WrapInl => State::Return(Value::Inl(Box::new(v))),
        Frame::WrapInr => State::Return(Value::Inr(Box::new(v))),
        Frame::WrapTag(label) => State::Return(Value::Tagged(label, Box::new(v))),
        Frame::Consume(e, env) => step_consume(v, &e, env, kont)?,
        Frame::ApplyCallee(callee) => State::Apply { callee, arg: v },
        Frame::ActivateArg(value_term, env) => {
            kont.push(Frame::ActivateWith(v));
            State::Term(value_term, env)
        }
        Frame::ActivateWith(consumer) => State::Apply { callee: consumer, arg: v },
        Frame::Prompt { ret, .. } => {
            // The handled body returned normally: its value goes to `return`.
            State::Apply { callee: ret, arg: v }
        }
        Frame::ApplyTo(arg) => State::Apply { callee: v, arg },
        Frame::MatchGuard { scrutinee, thunk, bindings, remaining } => {
            if v == Value::Bool(true) {
                run_match_thunk(&thunk, &bindings)?
            } else {
                next_match_arm(scrutinee, remaining, kont)?
            }
        }
    })
}

/// ⟨ v ∥ e ⟩ with the value in hand.
fn step_consume(v: Value, e: &CoTerm, env: Env, kont: &mut Kont) -> Result<State, EvalError> {
    Ok(match e {
        CoTerm::Covar(a) => {
            // ⟨v ∥ α⟩ sends v to α. When α is bound to a consumer — a
            // continuation parameter, a `select` consumer, a captured
            // continuation — the cut activates it. A co-variable that only
            // names the ambient continuation, as the lowering of `let`,
            // blocks, and applications does, delivers the value onward.
            match env.lookup(a) {
                Some(consumer) if is_applicable(&consumer) => {
                    State::Apply { callee: consumer, arg: v }
                }
                _ => State::Return(v),
            }
        }
        // ⟨ f ∥ λ̄x. c ⟩ — application: `c` computes the argument.
        CoTerm::CoLam(x, c2) => {
            let mut env2 = env;
            env2.push();
            env2.define(x.clone(), v.clone());
            if is_applicable(&v) || matches!(v, Value::PartialBuiltin(..)) {
                kont.push(Frame::ApplyCallee(v));
                // The argument command has the form ⟨ arg ∥ __call ⟩.
                match c2.as_ref() {
                    Command::Cut(t, CoTerm::Covar(_)) => State::Term(Rc::new(t.clone()), env2),
                    other => State::Command(Rc::new(other.clone()), env2),
                }
            } else {
                State::Command(Rc::new((**c2).clone()), env2)
            }
        }
        // ⟨ v ∥ μ̃x. c ⟩ → c[v/x] — a binder: `let`, a discarded block
        // expression, or any other form that names a value.
        CoTerm::MuTilde(x, c2) => {
            let mut env2 = env;
            env2.push();
            env2.define(x.clone(), v);
            State::Command(Rc::new((**c2).clone()), env2)
        }
        _ => State::Return(v),
    })
}

/// One application step: a cut against something that consumes.
fn step_apply(
    callee: Value,
    arg: Value,
    kont: &mut Kont,
    fuel: &mut usize,
) -> Result<State, EvalError> {
    Ok(match callee {
        Value::Closure { param, body, env } => {
            let mut call_env = env;
            call_env.push();
            call_env.define(param, arg);
            State::Term(body, call_env)
        }
        // Activating a labelled consumer runs exactly one branch.
        Value::CoCase { branches, env } => {
            let Value::Tagged(label, payload) = arg else {
                return Err(EvalError::TypeMismatch(format!(
                    "activating a `select` consumer requires a labelled value, got {}",
                    arg.display()
                )));
            };
            let Some(branch) = branches.iter().find(|b| b.label == label) else {
                return Err(EvalError::TypeMismatch(format!("no `{label}` alternative in select")));
            };
            let mut branch_env = env;
            branch_env.push();
            bind_components(&branch.binders, *payload, &mut branch_env)?;
            State::Command(Rc::new(branch.body.as_ref().clone()), branch_env)
        }
        // Activating a product consumer binds every component.
        Value::CoTensor { binders, body, env } => {
            let mut branch_env = env;
            branch_env.push();
            bind_components(&binders, arg, &mut branch_env)?;
            State::Command(body, branch_env)
        }
        // Applying a co-abstraction binds its continuation parameter.
        Value::CoAbs { covar, body, env } => {
            let mut call_env = env;
            call_env.push();
            call_env.define(covar, arg);
            State::Term(body, call_env)
        }
        Value::Continuation(cont) => State::Command(cont.command.clone(), cont.env.clone()),
        // The jump: reinstate the captured stack and deliver the value.
        Value::Kont(frames) => {
            *kont = frames;
            State::Return(arg)
        }
        // A trait method dispatches on the runtime type of its argument.
        Value::Method { method, impls } => {
            let key = crate::value::type_key(&arg);
            match impls.get(&key) {
                Some(closure) => State::Apply { callee: closure.clone(), arg },
                None => {
                    return Err(EvalError::TypeMismatch(format!(
                        "no impl of method `{method}` for a value of type {key}"
                    )));
                }
            }
        }
        // Performing an operation: find the nearest handler, capture the
        // delimited continuation, and run the matching clause with `resume`.
        Value::Operation { op, .. } => {
            // Split at the handler: the captured continuation includes the
            // Prompt, so resuming re-installs it (a deep handler); the clause
            // runs below it.
            let Some((clause, captured)) = kont.split_at_handler(&op) else {
                return Err(EvalError::TypeMismatch(format!("no handler for operation `{op}`")));
            };
            let resume = Value::Resume(captured);
            // clause is `λarg. λresume. body`: apply to arg, then to resume.
            kont.push(Frame::ApplyTo(resume));
            State::Apply { callee: clause, arg }
        }
        // Resuming a delimited continuation: prepend its frames and deliver.
        // A delimited continuation behaves as a function: run the captured
        // work (with its reinstated handler) to a value and deliver that. A
        // nested run is what lets the clause compose `resume(a) + resume(b)`
        // and resume more than once.
        Value::Resume(frames) => {
            let result = run(State::Return(arg), frames, fuel)?;
            State::Return(result)
        }
        Value::Builtin(name) => {
            let mut args = Vec::new();
            if name == "__match_dispatch" {
                args.extend(split_match_payload(&arg));
            } else {
                collect_args(&arg, &mut args);
            }
            builtin_step(&name, args, kont)?
        }
        Value::PartialBuiltin(name, mut collected) => {
            let mut single = if name == "__match_dispatch" {
                split_match_payload(&arg)
            } else {
                let mut out = Vec::new();
                collect_args(&arg, &mut out);
                out
            };
            collected.append(&mut single);
            builtin_step(&name, collected, kont)?
        }
        other => {
            return Err(EvalError::TypeMismatch(format!(
                "cannot activate continuation: {}",
                other.display()
            )));
        }
    })
}

/// Run a builtin once its arguments are in — or wait for more.
fn builtin_step(name: &str, args: Vec<Value>, kont: &mut Kont) -> Result<State, EvalError> {
    let arity = builtin_arity(name);
    if args.len() < arity && name != "__match_dispatch" {
        return Ok(State::Return(Value::PartialBuiltin(name.to_string(), args)));
    }
    if name == "__if_dispatch" {
        let mut it = args.into_iter();
        let cond = it.next().unwrap_or(Value::Bool(false));
        let then_v = it.next().unwrap_or(Value::Unit);
        let else_v = it.next().unwrap_or(Value::Unit);
        let chosen = match cond {
            Value::Bool(true) => then_v,
            Value::Bool(false) => else_v,
            other => {
                return Err(EvalError::TypeMismatch(format!(
                    "if condition must be bool, got {}",
                    other.display()
                )));
            }
        };
        // Branches are thunks; run the chosen one.
        return Ok(match chosen {
            Value::Closure { .. } => State::Apply { callee: chosen, arg: Value::Unit },
            other => State::Return(other),
        });
    }
    if name == "__handle" {
        // args: [clauses, body_thunk]. Decode the clause tree into the
        // operation map and the return closure, push the prompt, force body.
        let mut it = args.into_iter();
        let clauses_value = match it.next() {
            Some(Value::Inl(inner)) => *inner,
            other => other.unwrap_or(Value::Unit),
        };
        let body_thunk = it.next().unwrap_or(Value::Unit);
        let mut clauses = std::collections::HashMap::new();
        let mut ret = Value::Unit;
        let mut rest = clauses_value;
        while let Value::Pair(head, tail) = rest {
            if let Value::Pair(op, closure) = *head
                && let Value::Str(op) = *op
            {
                if op == "return" {
                    ret = *closure;
                } else {
                    clauses.insert(op, *closure);
                }
            }
            rest = *tail;
        }
        kont.push(Frame::Prompt { clauses: Rc::new(clauses), ret });
        return Ok(State::Apply { callee: body_thunk, arg: Value::Unit });
    }
    if name == "__match_dispatch" {
        let mut it = args.into_iter();
        let scrutinee = it.next().unwrap_or(Value::Unit);
        let mut arms = it.collect::<Vec<_>>();
        // The lowered arm spine ends in unit; that terminator is not an arm.
        if arms.last() == Some(&Value::Unit) {
            arms.pop();
        }
        return next_match_arm(scrutinee, arms, kont);
    }
    // A builtin that offers its outcome activates one of its continuations;
    // the rest return a value.
    match crate::eval::run_offering_builtin(name, &args)? {
        Some((consumer, outcome)) => Ok(State::Apply { callee: consumer, arg: outcome }),
        None => Ok(State::Return(run_builtin_function(name, args)?)),
    }
}

/// Try the arms in order. A pattern that matches hands over to its guard —
/// evaluated in this same machine, so a jump inside a guard is a jump — and
/// a guard that holds runs the arm's thunk.
fn next_match_arm(
    scrutinee: Value,
    mut arms: Vec<Value>,
    kont: &mut Kont,
) -> Result<State, EvalError> {
    while !arms.is_empty() {
        let arm = unwrap_match_arm(&arms.remove(0));
        let Value::Pair(a, b) = &arm else {
            return Err(EvalError::TypeMismatch(format!("malformed match arm: {}", arm.display())));
        };
        let (descriptor, guard, thunk) = match (a.as_ref(), b.as_ref()) {
            (Value::Str(descriptor), Value::Pair(guard, thunk)) => {
                (descriptor.clone(), guard.as_ref().clone(), thunk.as_ref().clone())
            }
            _ => {
                return Err(EvalError::TypeMismatch(format!(
                    "malformed match arm payload: {}, {}",
                    a.display(),
                    b.display()
                )));
            }
        };
        let descriptor = parse_runtime_pattern(&descriptor);
        let mut bindings: Vec<(String, Value)> = Vec::new();
        if let Descriptor::Pattern(pattern) = &descriptor
            && !pattern_matches(pattern, &scrutinee, &mut bindings)
        {
            continue;
        }
        // The pattern matches; the guard decides.
        if let Value::Closure { param, body, env } = &guard {
            let mut guard_env = env.clone();
            guard_env.push();
            for (name, value) in &bindings {
                guard_env.define(name.clone(), value.clone());
            }
            guard_env.define(param, Value::Unit);
            kont.push(Frame::MatchGuard { scrutinee, thunk, bindings, remaining: arms });
            return Ok(State::Term(body.clone(), guard_env));
        }
        if guard != Value::Bool(true) {
            continue;
        }
        return run_match_thunk(&thunk, &bindings);
    }
    Err(EvalError::TypeMismatch("non-exhaustive match".into()))
}

/// The chosen arm's body, with the pattern's bindings in scope.
fn run_match_thunk(thunk: &Value, bindings: &[(String, Value)]) -> Result<State, EvalError> {
    let Value::Closure { param, body, env } = thunk else {
        return Err(EvalError::TypeMismatch("match arm body must be a thunk".into()));
    };
    let mut call_env = env.clone();
    call_env.push();
    for (name, value) in bindings {
        call_env.define(name.clone(), value.clone());
    }
    call_env.define(param, Value::Unit);
    Ok(State::Term(body.clone(), call_env))
}
