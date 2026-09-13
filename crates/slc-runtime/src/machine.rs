//! The abstract machine: the evaluator with its continuation as data.
//!
//! λ̄μμ̃ is a machine calculus — a command ⟨t ∥ e⟩ is a state with the
//! control on the left and the continuation on the right — and this module
//! runs it that way, over the flat `Chunk` (`crate::chunk`). The machine's
//! instruction pointer is a `NodeId`: an index into the program's one node
//! vector, resolved with `chunk::node`. Lexical binders are already de Bruijn
//! indices, so a variable reference is a count into the positional
//! environment, not a walk comparing names.
//!
//! The continuation used to be Rust stack frames, which made a captured
//! continuation an escape marker: one shot, upward only, dead once its `mu`
//! returned. Here the rest of the computation is the `Kont` stack, a value
//! like any other:
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

use crate::chunk::{Node, NodeId, node};
use crate::eval::{
    EvalError, bind_components, builtin_arity, collect_args, is_applicable, run_builtin_function,
};
use crate::matching::{
    Descriptor, parse_runtime_pattern, pattern_matches, split_match_payload, unwrap_match_arm,
};
use crate::value::{Env, Value};
use std::rc::Rc;

/// What the machine is doing right now. A `NodeId` is the instruction pointer
/// into the current chunk.
pub(crate) enum State {
    Term(NodeId, Env),
    Command(NodeId, Env),
    Apply { callee: Value, arg: Value },
    Return(Value),
}

/// One saved step of the computation: what happens to the value being
/// produced. The whole stack is the continuation.
#[derive(Debug, Clone)]
pub enum Frame {
    /// `(v ⊗ _)` — the first component is done; evaluate the second term.
    PairRight(NodeId, Env),
    /// `(v1 ⊗ v2)` — both components done; build the pair.
    PairDone(Value),
    WrapTag(String),
    /// `⟨ _ ∥ e ⟩` — the term side is done; consume with co-term `e`.
    Consume(NodeId, Env),
    /// The callee is evaluated; the argument is being computed.
    ApplyCallee(Value),
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

/// Run the term at `root` to a value with an empty continuation. A chunk must
/// be installed (the entry points wrap the run in `chunk::with_chunk`).
pub(crate) fn run_term(root: NodeId, env: &Env, fuel: &mut usize) -> Result<Value, EvalError> {
    run(State::Term(root, env.clone()), Kont::empty(), fuel)
}

/// Run the command at `root` to a value with an empty continuation.
pub(crate) fn run_command(root: NodeId, env: &Env, fuel: &mut usize) -> Result<Value, EvalError> {
    run(State::Command(root, env.clone()), Kont::empty(), fuel)
}

/// Apply a value to an argument with an empty continuation.
pub(crate) fn run_apply(callee: Value, arg: Value, fuel: &mut usize) -> Result<Value, EvalError> {
    run(State::Apply { callee, arg }, Kont::empty(), fuel)
}

/// The same, under the runtime's own handler for `IO`. This is what makes
/// the runtime the outermost handler a program has: `main` may leave `IO`
/// undischarged, and it arrives here. A handler the program installs sits
/// nearer the operation and answers first, so `IO` can be mocked.
pub(crate) fn run_apply_under_io(
    callee: Value,
    arg: Value,
    fuel: &mut usize,
) -> Result<Value, EvalError> {
    let clauses: std::collections::HashMap<String, Value> = crate::value::IO_CLAUSES
        .iter()
        .map(|(op, clause)| ((*op).to_string(), Value::Builtin((*clause).to_string())))
        .collect();
    let mut kont = Kont::empty();
    kont.push(Frame::Prompt { clauses: Rc::new(clauses), ret: Value::Builtin("__io_done".into()) });
    run(State::Apply { callee, arg }, kont, fuel)
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
            State::Term(t, env) => step_term(t, env, &mut kont)?,
            State::Command(c, env) => step_command(c, env, &mut kont)?,
            State::Apply { callee, arg } => step_apply(callee, arg, &mut kont, fuel)?,
            State::Return(v) => match kont.pop() {
                None => return Ok(v),
                Some(frame) => step_frame(frame, v, &mut kont)?,
            },
        };
    }
}

fn step_term(t: NodeId, env: Env, kont: &mut Kont) -> Result<State, EvalError> {
    Ok(match node(t) {
        Node::Local(i) => State::Return(
            env.local(i).ok_or_else(|| EvalError::Unbound(format!("de Bruijn local #{i}")))?,
        ),
        Node::Dynamic(name) => State::Return(crate::eval::literal_or_lookup(&name, &env)?),
        Node::Lam(body) => State::Return(Value::Closure { body, env }),
        Node::Mu(command) => {
            // The μ: bind the co-variable (positional slot 0) to the
            // continuation itself. Capturing the stack is one `Rc` bump.
            let mut env2 = env;
            env2.define_local(Value::Kont(kont.clone()));
            State::Command(command, env2)
        }
        Node::Pair(t1, t2) => {
            kont.push(Frame::PairRight(t2, env.clone()));
            State::Term(t1, env)
        }
        Node::Tag(label, payload) => {
            kont.push(Frame::WrapTag(label.to_string()));
            State::Term(payload, env)
        }
        // A menu closes over its environment; its branch bodies stay
        // unevaluated until a request chooses one.
        Node::CoMatch(_) => State::Return(Value::Menu { node: t, env }),
        Node::Co(co) => State::Return(reify_coterm(co, &env)?),
        other => {
            return Err(EvalError::TypeMismatch(format!("expected a term, found {other:?}")));
        }
    })
}

/// A co-term seen as a value — the reification `co(e)`.
///
/// A consumer closes over its environment with its branch bodies
/// unevaluated; a co-variable is already a value in the environment; and a
/// request `.d(e)` becomes a labelled value carrying its own continuation
/// reified, which is what lets `match` take a continuation apart with the
/// same machinery that takes an `enum` value apart.
fn reify_coterm(co: NodeId, env: &Env) -> Result<Value, EvalError> {
    Ok(match node(co) {
        Node::CoCase(_) => Value::CoCase { co, env: env.clone() },
        Node::MuTildeTensor(..) | Node::MuTilde(_) => Value::CoTensor { co, env: env.clone() },
        Node::CoLocal(i) => {
            env.local(i).ok_or_else(|| EvalError::Unbound(format!("de Bruijn co-local #{i}")))?
        }
        Node::CoDynamic(a) => {
            env.lookup(&a).ok_or_else(|| EvalError::Unbound(format!("co-variable `{a}`")))?
        }
        Node::Dtor(label, e) => Value::Tagged(label.to_string(), Box::new(reify_coterm(e, env)?)),
        other => {
            return Err(EvalError::TypeMismatch(format!(
                "cannot reify this co-term as a value: {other:?}"
            )));
        }
    })
}

fn step_command(c: NodeId, env: Env, kont: &mut Kont) -> Result<State, EvalError> {
    Ok(match node(c) {
        Node::Cut(t, e) => {
            kont.push(Frame::Consume(e, env.clone()));
            State::Term(t, env)
        }
        other => {
            return Err(EvalError::TypeMismatch(format!("expected a command, found {other:?}")));
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
        Frame::WrapTag(label) => State::Return(Value::Tagged(label, Box::new(v))),
        Frame::Consume(e, env) => step_consume(v, e, env, kont)?,
        Frame::ApplyCallee(callee) => State::Apply { callee, arg: v },
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

/// Does sending `v` to `consumer` run it? A consumer does; so does a bundle
/// of exits when what arrives is an alternative of a sum, which picks the
/// exit. Anything else a co-variable holds only names where the value goes.
fn activates(consumer: &Value, v: &Value) -> bool {
    is_applicable(consumer)
        || matches!((consumer, v), (Value::Pair(..), Value::Tagged(label, _)) if label == "|0" || label == "|1")
}

/// ⟨ v ∥ e ⟩ with the value in hand, `e` the co-term node.
fn step_consume(v: Value, e: NodeId, env: Env, kont: &mut Kont) -> Result<State, EvalError> {
    Ok(match node(e) {
        // ⟨v ∥ α⟩ sends v to α. When α names a consumer — a continuation
        // parameter, a `select` consumer, a captured continuation — the cut
        // activates it. A co-variable that only names the ambient
        // continuation, as the lowering of `let`, blocks, and applications
        // does, delivers the value onward.
        Node::CoLocal(i) => match env.local(i) {
            Some(consumer) if activates(&consumer, &v) => State::Apply { callee: consumer, arg: v },
            _ => State::Return(v),
        },
        Node::CoDynamic(a) => match env.lookup(&a) {
            Some(consumer) if activates(&consumer, &v) => State::Apply { callee: consumer, arg: v },
            _ => State::Return(v),
        },
        // ⟨ f ∥ v · e ⟩ — application: evaluate the argument, apply `f` to
        // it, and send the result on to `e`. A co-variable tail names the
        // onward continuation, so the result simply flows out to the ambient
        // stack — under a handler that is the live, possibly-resumed stack,
        // not the one the enclosing μ captured, so no frame is pushed.
        Node::App(arg, tail) => {
            if !matches!(node(tail), Node::CoLocal(_) | Node::CoDynamic(_)) {
                kont.push(Frame::Consume(tail, env.clone()));
            }
            kont.push(Frame::ApplyCallee(v));
            State::Term(arg, env)
        }
        // ⟨ v ∥ μ̃x. c ⟩ → c[v/x] — a binder: `let`, a discarded block
        // expression, or any other form that names a value.
        Node::MuTilde(c2) => {
            let mut env2 = env;
            env2.define_local(v);
            State::Command(c2, env2)
        }
        // ⟨ menu ∥ .d(e) ⟩ — a request: the label chooses one branch of the
        // menu, which runs with the request's continuation bound.
        Node::Dtor(label, e) => {
            let Value::Menu { node: menu, env: menu_env } = v else {
                return Err(EvalError::TypeMismatch(format!(
                    "a request needs a menu value, got {}",
                    v.display()
                )));
            };
            let Node::CoMatch(branches) = node(menu) else {
                return Err(EvalError::TypeMismatch("a menu value must be `μ[…]`".into()));
            };
            let Some(branch) = branches.iter().find(|b| *b.label == *label) else {
                return Err(EvalError::TypeMismatch(format!("no `{label}` branch in menu")));
            };
            let mut branch_env = menu_env;
            branch_env.define_local(reify_coterm(e, &env)?);
            State::Command(branch.body, branch_env)
        }
        // ⟨ v₁ ⊗ v₂ ∥ μ̃(x, y). c ⟩ — a direct cut against a product
        // consumer binds every component and runs the body.
        Node::MuTildeTensor(arity, body) => {
            let mut env2 = env;
            bind_components(arity, v, &mut env2)?;
            State::Command(body, env2)
        }
        // ⟨ L(v) ∥ μ̃[…] ⟩ — a direct cut against a labelled consumer: the
        // label chooses the branch, exactly as activation through a value
        // does.
        Node::CoCase(branches) => {
            let Value::Tagged(label, payload) = v else {
                return Err(EvalError::TypeMismatch(format!(
                    "a labelled consumer requires a labelled value, got {}",
                    v.display()
                )));
            };
            let Some(branch) = branches.iter().find(|b| *b.label == label) else {
                return Err(EvalError::TypeMismatch(format!("no `{label}` alternative in select")));
            };
            let mut env2 = env;
            bind_components(branch.arity, *payload, &mut env2)?;
            State::Command(branch.body, env2)
        }
        // ⟨ v ∥ prj:index ⟩ → the index-th spine component of v. A struct is
        // a tagged product, so unwrap the tag first; then walk `index` tails
        // and take the head, or the whole remainder when it is the bare last.
        Node::Prj(index) => State::Return(project_value(v, index)?),
        _ => State::Return(v),
    })
}

/// The `index`-th spine component of a right-nested product value. A struct
/// is a tagged product, so its tag is unwrapped first.
fn project_value(value: Value, index: usize) -> Result<Value, EvalError> {
    let mut current = match value {
        Value::Tagged(_, payload) => *payload,
        other => other,
    };
    for _ in 0..index {
        let Value::Pair(_, tail) = current else {
            return Err(EvalError::TypeMismatch(format!(
                "projection of component {index} ran off a {}",
                current.display()
            )));
        };
        current = *tail;
    }
    match current {
        Value::Pair(head, _) => Ok(*head),
        last => Ok(last),
    }
}

/// One application step: a cut against something that consumes.
fn step_apply(
    callee: Value,
    arg: Value,
    kont: &mut Kont,
    fuel: &mut usize,
) -> Result<State, EvalError> {
    Ok(match callee {
        Value::Closure { body, env } => {
            let mut call_env = env;
            call_env.define_local(arg);
            State::Term(body, call_env)
        }
        // Activating a labelled consumer runs exactly one branch.
        Value::CoCase { co, env } => {
            let Node::CoCase(branches) = node(co) else {
                return Err(EvalError::TypeMismatch("a `select` value must be a co-case".into()));
            };
            let Value::Tagged(label, payload) = arg else {
                return Err(EvalError::TypeMismatch(format!(
                    "activating a `select` consumer requires a labelled value, got {}",
                    arg.display()
                )));
            };
            let Some(branch) = branches.iter().find(|b| *b.label == label) else {
                return Err(EvalError::TypeMismatch(format!("no `{label}` alternative in select")));
            };
            let mut branch_env = env;
            bind_components(branch.arity, *payload, &mut branch_env)?;
            State::Command(branch.body, branch_env)
        }
        // A request applied to a menu: the mirror of activating a labelled
        // consumer — the request's label chooses the branch, and its
        // continuation (the tagged payload) is what the branch binds.
        Value::Menu { node: menu, env } => {
            let Value::Tagged(label, payload) = arg else {
                return Err(EvalError::TypeMismatch(format!(
                    "activating a menu requires a request, got {}",
                    arg.display()
                )));
            };
            menu_dispatch(menu, env, &label, *payload)?
        }
        // The same interaction with the sides swapped: a computed-consumer
        // cut evaluates the request first, so the request is the callee and
        // the menu the argument.
        Value::Tagged(label, payload) if matches!(arg, Value::Menu { .. }) => {
            let Value::Menu { node: menu, env } = arg else { unreachable!("matched above") };
            menu_dispatch(menu, env, &label, *payload)?
        }
        // Activating a product consumer binds every component.
        Value::CoTensor { co, env } => {
            let (arity, body) = match node(co) {
                Node::MuTildeTensor(arity, body) => (arity, body),
                Node::MuTilde(body) => (1, body),
                other => {
                    return Err(EvalError::TypeMismatch(format!(
                        "a product consumer must be `μ̃(…)`, found {other:?}"
                    )));
                }
            };
            let mut branch_env = env;
            bind_components(arity, arg, &mut branch_env)?;
            State::Command(body, branch_env)
        }
        // The jump: reinstate the captured stack and deliver the value.
        Value::Kont(frames) => {
            *kont = frames;
            State::Return(arg)
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
            // clause is `λpayload. λresume. body`: apply to the arguments the
            // perform packed, then to resume.
            kont.push(Frame::ApplyTo(resume));
            State::Apply { callee: clause, arg }
        }
        // Resuming a delimited continuation: run the captured work (with its
        // reinstated handler) to a value and deliver that. A nested run is
        // what lets the clause compose `resume(a) + resume(b)` and resume
        // more than once.
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
        // A bundle of exits consumes a sum, since the consumer of `(A | B)` is
        // `(-A & -B)`: the left alternative takes the first exit, and the
        // right one hands its payload to the rest — both nested to the right
        // alike.
        Value::Pair(first, rest) => {
            let (exit, payload) = match arg {
                Value::Tagged(label, payload) if label == "|0" => (*first, *payload),
                Value::Tagged(label, payload) if label == "|1" => (*rest, *payload),
                arg => {
                    return Err(EvalError::TypeMismatch(format!(
                        "a bundle of exits consumes an alternative of a sum, got {}",
                        arg.display()
                    )));
                }
            };
            State::Apply { callee: exit, arg: payload }
        }
        other => {
            return Err(EvalError::TypeMismatch(format!(
                "cannot activate continuation: {}",
                other.display()
            )));
        }
    })
}

/// One request meeting one menu: the label chooses a branch, which runs
/// with the request's continuation bound.
fn menu_dispatch(menu: NodeId, env: Env, label: &str, payload: Value) -> Result<State, EvalError> {
    let Node::CoMatch(branches) = node(menu) else {
        return Err(EvalError::TypeMismatch("a menu value must be `μ[…]`".into()));
    };
    let Some(branch) = branches.iter().find(|b| *b.label == *label) else {
        return Err(EvalError::TypeMismatch(format!("no `{label}` branch in menu")));
    };
    let mut branch_env = env;
    branch_env.define_local(payload);
    Ok(State::Command(branch.body, branch_env))
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
            Some(Value::Tagged(label, inner)) if label == "__clauses" => *inner,
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
    // `println` and `print` reach the outside world, so they do not write:
    // they render, and then perform the `IO` operation that writes. The
    // handler is the runtime's own unless the program installed a nearer
    // one, which is what lets a program mock its output.
    if let Some(op) = match name {
        "println" => Some("write_line"),
        "print" => Some("write"),
        _ => None,
    } {
        let text = args.first().map(|v| v.display()).unwrap_or_default();
        return Ok(State::Apply {
            callee: Value::Operation { effect: "IO".into(), op: op.into() },
            arg: Value::Str(text),
        });
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
        // The pattern matches; the guard decides. Pattern variables are
        // injected by name (the overlay), leaving de Bruijn slots untouched.
        if let Value::Closure { body, env } = &guard {
            let mut guard_env = env.clone();
            for (name, value) in &bindings {
                guard_env.define_named(name.clone(), value.clone());
            }
            guard_env.define_local(Value::Unit);
            kont.push(Frame::MatchGuard { scrutinee, thunk, bindings, remaining: arms });
            return Ok(State::Term(*body, guard_env));
        }
        if guard != Value::Bool(true) {
            continue;
        }
        return run_match_thunk(&thunk, &bindings);
    }
    Err(EvalError::TypeMismatch("non-exhaustive match".into()))
}

/// The chosen arm's body, with the pattern's bindings in scope. The bindings
/// go in the named overlay (the compiler left pattern variables as
/// `Dynamic`); the thunk's own `__match_arg` parameter is the positional
/// slot the body's de Bruijn indices are relative to.
fn run_match_thunk(thunk: &Value, bindings: &[(String, Value)]) -> Result<State, EvalError> {
    let Value::Closure { body, env } = thunk else {
        return Err(EvalError::TypeMismatch("match arm body must be a thunk".into()));
    };
    let mut call_env = env.clone();
    for (name, value) in bindings {
        call_env.define_named(name.clone(), value.clone());
    }
    call_env.define_local(Value::Unit);
    Ok(State::Term(*body, call_env))
}
