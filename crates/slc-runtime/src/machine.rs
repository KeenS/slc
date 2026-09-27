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
//! - activating a `Kont` *replaces* the frame stack down to the nearest
//!   handler prompt the two stacks hold — the same installation, or a
//!   resumption's copy of it — and delivers the value, however deep the
//!   machine is, however long ago the capture returned, however many times
//!   it has been used before. Meeting first a prompt the captured stack does
//!   not hold is an error: the continuation belongs outside that handler.
//!
//! The stack is a persistent cons (`Kont`) with the top at the head, so a
//! capture — `mu`, or a handler's `resume` — clones one `Rc`: O(1), no
//! matter how deep, and repeatable. Pushing a frame never disturbs a stack
//! already captured, so a resumed continuation walks its own copy. Resuming
//! pushes the captured slice onto the running stack, so what it performs
//! reaches every handler the program has, and its result flows on into the
//! clause that resumed it.

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
    Force,
    /// `(v₀, …, _, …)` — the components before `next` are done; evaluate
    /// the one at `next`.
    Tuple {
        done: Vec<Value>,
        items: Rc<Vec<NodeId>>,
        next: usize,
        env: Env,
    },
    WrapTag(String),
    /// `⟨ _ ∥ e ⟩` — the term side is done; consume with co-term `e`.
    Consume(NodeId, Env),
    /// The callee is evaluated; the argument is being computed.
    ApplyCallee(Value),
    /// A handler delimiter: the `return` clause, and the operation clauses
    /// of one effect. Sits on the stack under the body it handles. `id` is
    /// fresh for each installation and kept by a resumption's copy, so a
    /// `mu` continuation captured under it recognizes the copy.
    Prompt {
        id: u64,
        clauses: std::rc::Rc<std::collections::HashMap<String, Value>>,
        ret: Value,
    },
    /// `resume(v)` then apply the produced closure to this value: a handler
    /// clause is `λarg. λresume. body`, so after `clause(arg)` we apply the
    /// result to `resume`.
    ApplyTo(Value),
    /// Entry of a row-polymorphic function. `id` increases over the run.
    /// A handler above this frame belongs to that function. `aware` is the
    /// operations named concretely in its type. A closure born before `id`
    /// passes through handlers for any other operation: that effect arrived
    /// through the row parameter.
    Barrier {
        id: u32,
        aware: std::rc::Rc<std::collections::HashSet<String>>,
    },
    /// Birth barrier of the code now running. The nearest frame wins.
    Origin {
        birth: u32,
    },
}

/// The continuation as a persistent stack: a shared cons of frames with the
/// top at the head. Capturing it — `mu`'s `Value::Kont`, or a handler's
/// `resume` — clones one `Rc`, regardless of depth, and the clone walks
/// independently of the live stack. That is what makes a jump and a
/// multi-shot `resume` cheap.
#[derive(Clone, Debug, Default)]
pub struct Kont(Option<Rc<KontNode>>);

/// A node knows how many frames lie at and below it, so a jump lines two
/// stacks up without walking either to the bottom.
#[derive(Debug)]
struct KontNode {
    frame: Frame,
    depth: usize,
    tail: Option<Rc<KontNode>>,
}

fn depth(link: &Option<Rc<KontNode>>) -> usize {
    link.as_ref().map_or(0, |node| node.depth)
}

/// A fresh id for a prompt being installed.
pub(crate) fn fresh_prompt_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// A fresh id for the barrier of a row-polymorphic call. Zero is reserved
/// for code born under no barrier, so the first call is one.
fn fresh_barrier_id() -> u32 {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl Kont {
    pub(crate) fn empty() -> Self {
        Kont(None)
    }

    /// Push a frame onto the top. A stack already captured elsewhere keeps
    /// its own view — the push only extends this handle.
    pub(crate) fn push(&mut self, frame: Frame) {
        let tail = self.0.take();
        self.0 = Some(Rc::new(KontNode { frame, depth: depth(&tail) + 1, tail }));
    }

    /// Jump to `target`, a `mu` continuation: replace the running stack down
    /// to the nearest prompt both stacks hold. The running stack is walked
    /// from the top, and the first of these decides:
    ///
    /// - a frame `target` shares: the jump stays within one extent, and the
    ///   running stack becomes `target`;
    /// - a prompt `target` holds, itself or a resumption's copy of it:
    ///   `target`'s frames above that prompt go on it;
    /// - a prompt `target` does not hold: `target` was captured outside the
    ///   handler running now, and the jump is refused;
    /// - the bottom: `target` replaces the whole stack.
    ///
    /// A shared frame only shortens the walk — every frame below it is the
    /// same on both stacks, its prompts included. The two stacks are lined up
    /// by depth, so the walk costs the frames the jump removes.
    fn jump(&mut self, target: &Kont) -> Result<(), EvalError> {
        let mut ours = self.0.clone();
        let mut theirs = target.0.clone();
        while let Some(node) = ours {
            while depth(&theirs) > node.depth {
                theirs = theirs.and_then(|t| t.tail.clone());
            }
            if theirs.as_ref().is_some_and(|t| Rc::ptr_eq(t, &node)) {
                break;
            }
            if let Frame::Prompt { id, .. } = &node.frame {
                let above = target.frames_above_prompt(*id).ok_or(EvalError::ForeignPrompt)?;
                *self = Kont(Some(node));
                for frame in above.into_iter().rev() {
                    self.push(frame);
                }
                return Ok(());
            }
            ours = node.tail.clone();
        }
        *self = target.clone();
        Ok(())
    }

    /// The frames above the prompt `id` on this stack, top first, or `None`
    /// when the stack holds no such prompt.
    fn frames_above_prompt(&self, id: u64) -> Option<Vec<Frame>> {
        let mut above = Vec::new();
        let mut cursor = self.0.as_ref();
        while let Some(node) = cursor {
            if matches!(node.frame, Frame::Prompt { id: found, .. } if found == id) {
                return Some(above);
            }
            above.push(node.frame.clone());
            cursor = node.tail.as_ref();
        }
        None
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

    /// Push every frame of `slice` onto the top, so the slice's top frame is
    /// the new top and its bottom frame sits on what was here. The slice is
    /// shared, so its frames are cloned and it stays reusable: a clause may
    /// resume it again.
    pub(crate) fn append(&mut self, slice: &Kont) {
        let mut frames = Vec::new();
        let mut cursor = slice.0.as_ref();
        while let Some(node) = cursor {
            frames.push(node.frame.clone());
            cursor = node.tail.as_ref();
        }
        for frame in frames.into_iter().rev() {
            self.push(frame);
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

    /// The birth barrier of the code running at the top. Zero when none.
    fn origin_birth(&self) -> u32 {
        let mut cursor = self.0.clone();
        while let Some(node) = cursor {
            if let Frame::Origin { birth } = &node.frame {
                return *birth;
            }
            cursor = node.tail.clone();
        }
        0
    }

    /// The nearest barrier under `prompt` is a row-polymorphic call that
    /// began after `origin` and does not name `op` in its type. The prompt
    /// belongs to that call, and `op` arrived through its row parameter.
    fn tunneled(prompt_tail: &Option<Rc<KontNode>>, origin: u32, op: &str) -> bool {
        let mut cursor = prompt_tail.clone();
        while let Some(node) = cursor {
            if let Frame::Barrier { id, aware } = &node.frame {
                return *id > origin && !aware.contains(op);
            }
            cursor = node.tail.clone();
        }
        false
    }

    /// Split at the nearest handler above that handles `op` and is aware of
    /// the code performing it: hand back its clause and the delimited
    /// continuation (the work up to and including that `Prompt`, which
    /// therefore reinstates it on resume), and truncate `self` to what lay
    /// below the handler. A handler installed inside a row-polymorphic call
    /// is not aware of a closure born before that call. `None` if nothing
    /// aware handles `op`.
    fn split_at_handler(&mut self, op: &str) -> Option<(Value, Kont)> {
        let origin = self.origin_birth();
        let mut prefix: Vec<Frame> = Vec::new();
        let mut cursor = self.0.clone();
        loop {
            let node = cursor?;
            match &node.frame {
                Frame::Prompt { clauses, .. }
                    if clauses.contains_key(op) && !Self::tunneled(&node.tail, origin, op) =>
                {
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
    kont.push(Frame::Prompt {
        id: fresh_prompt_id(),
        clauses: Rc::new(clauses),
        ret: Value::Builtin("__io_done".into()),
    });
    // Closures built while the program runs are born here, after every
    // declaration's closure. A declaration stays at generation zero and
    // adopts whoever calls it.
    let id = fresh_barrier_id();
    kont.push(Frame::Barrier { id, aware: Rc::new(std::collections::HashSet::new()) });
    kont.push(Frame::Origin { birth: id });
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
            State::Apply { callee, arg } => step_apply(callee, arg, &mut kont)?,
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
        Node::Lam(body) => State::Return(Value::Closure { body, env, birth: kont.origin_birth() }),
        Node::Delay(body) => State::Return(Value::Delayed { body, env }),
        Node::Mu(command) => {
            // The μ: bind the co-variable (positional slot 0) to the
            // continuation itself. Capturing the stack is one `Rc` bump.
            let mut env2 = env;
            env2.define_local(Value::Kont(kont.clone()));
            State::Command(command, env2)
        }
        Node::Tuple(items) => match items.first().copied() {
            Some(first) => {
                kont.push(Frame::Tuple { done: Vec::new(), items, next: 1, env: env.clone() });
                State::Term(first, env)
            }
            None => State::Return(Value::Unit),
        },
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
/// reified, which is what lets `of` take a continuation apart with the
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
            // A cut into a co-variable that only forwards is a tail position:
            // the value would be handed to the stack underneath unchanged, so
            // no frame is pushed and a loop through it stays flat. That holds
            // when the name is unbound (it names the ambient continuation) or
            // when it holds the very stack running now, where jumping to it
            // is returning.
            if !forwards_to_current(e, &env, kont) {
                kont.push(Frame::Consume(e, env.clone()));
            }
            State::Term(t, env)
        }
        other => {
            return Err(EvalError::TypeMismatch(format!("expected a command, found {other:?}")));
        }
    })
}

/// Whether consuming a value with co-term `e` would only deliver it to
/// `kont` as it stands: `e` names a continuation that is unbound, or that is
/// `kont` itself. Such a cut needs no frame.
fn forwards_to_current(e: NodeId, env: &Env, kont: &Kont) -> bool {
    let bound = match node(e) {
        Node::Forward => return true,
        Node::CoLocal(i) => env.local(i),
        Node::CoDynamic(a) => env.lookup(&a),
        _ => return false,
    };
    match bound {
        None => true,
        Some(Value::Kont(captured)) => Kont::ptr_eq(&captured, kont),
        Some(_) => false,
    }
}

fn step_frame(frame: Frame, v: Value, kont: &mut Kont) -> Result<State, EvalError> {
    Ok(match frame {
        Frame::Force => force_value(v, kont),
        Frame::Tuple { mut done, items, next, env } => {
            done.push(v);
            match items.get(next).copied() {
                Some(item) => {
                    kont.push(Frame::Tuple { done, items, next: next + 1, env: env.clone() });
                    State::Term(item, env)
                }
                None => State::Return(Value::Tuple(done)),
            }
        }
        Frame::WrapTag(label) => State::Return(Value::Tagged(label, Box::new(v))),
        Frame::Consume(e, env) => step_consume(v, e, env, kont)?,
        Frame::ApplyCallee(callee) => State::Apply { callee, arg: v },
        Frame::Prompt { ret, .. } => {
            // The handled body returned normally: its value goes to `return`.
            State::Apply { callee: ret, arg: v }
        }
        Frame::ApplyTo(arg) => State::Apply { callee: v, arg },
        // Both are markers for effect search. A value returns through them.
        Frame::Barrier { .. } | Frame::Origin { .. } => State::Return(v),
    })
}

/// Does sending `v` to `consumer` run it? A consumer does; so does a bundle
/// of exits when what arrives is an alternative of a sum, which picks the
/// exit. Anything else a co-variable holds only names where the value goes.
fn activates(consumer: &Value, v: &Value) -> bool {
    is_applicable(consumer)
        || matches!((consumer, v), (Value::Tuple(..), Value::Tagged(label, _)) if crate::value::alternative_index(label).is_some())
}

/// Run a delayed computation: its one slot is the unit it is run with.
fn run_delayed(body: NodeId, env: Env) -> State {
    let mut env = env;
    env.define_local(Value::Unit);
    State::Term(body, env)
}

fn force_value(value: Value, kont: &mut Kont) -> State {
    let mut current = value;
    loop {
        match current {
            Value::Adapted { adapter, value } => {
                kont.push(Frame::Force);
                kont.push(Frame::ApplyCallee(*adapter));
                current = *value;
            }
            Value::Delayed { body, env } => {
                kont.push(Frame::Force);
                return run_delayed(body, env);
            }
            other => return State::Return(other),
        }
    }
}

/// ⟨ v ∥ e ⟩ with the value in hand, `e` the co-term node.
fn step_consume(v: Value, e: NodeId, env: Env, kont: &mut Kont) -> Result<State, EvalError> {
    // A request or projection demands what a delayed computation produces:
    // run it, and send the demand to the result.
    if matches!(v, Value::Delayed { .. } | Value::Adapted { .. })
        && matches!(node(e), Node::Dtor(..) | Node::Prj(..))
    {
        kont.push(Frame::Consume(e, env));
        return Ok(force_value(v, kont));
    }
    Ok(match node(e) {
        Node::Forward => State::Return(v),
        // ⟨v ∥ α⟩ sends v to α. When α names a consumer — a continuation
        // parameter, a `mu` consumer, a captured continuation — the cut
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
        // ⟨ v ∥ prj:index ⟩ → the index-th component of v. A struct is
        // a tagged product, so unwrap the tag first; then walk `index` tails
        // and take the head, or the whole remainder when it is the bare last.
        Node::Prj(index) => State::Return(project_value(v, index)?),
        _ => State::Return(v),
    })
}

/// The `index`-th component of a tuple value. A struct is a tagged product,
/// so its tag is unwrapped first.
fn project_value(value: Value, index: usize) -> Result<Value, EvalError> {
    let product = match value {
        Value::Tagged(_, payload) => *payload,
        other => other,
    };
    match product {
        Value::Tuple(mut items) if index < items.len() => Ok(items.swap_remove(index)),
        other => Err(EvalError::TypeMismatch(format!(
            "projection of component {index} from {}",
            other.display()
        ))),
    }
}

/// One application step: a cut against something that consumes.
fn step_apply(callee: Value, arg: Value, kont: &mut Kont) -> Result<State, EvalError> {
    Ok(match callee {
        Value::Adapted { adapter, value } => {
            kont.push(Frame::ApplyTo(arg));
            force_value(Value::Adapted { adapter, value }, kont)
        }
        Value::Closure { body, env, birth } => {
            // A declaration's closure is born at generation zero and adopts
            // the caller's origin, so a helper the function itself calls is
            // the function's own code. A closure built during the run keeps
            // the generation it was built at.
            if birth != 0 {
                kont.push(Frame::Origin { birth });
            }
            let mut call_env = env;
            call_env.define_local(arg);
            State::Term(body, call_env)
        }
        // Applying a delayed computation demands it: run it, then apply what
        // it produced. It runs again at the next demand.
        Value::Delayed { body, env } => {
            kont.push(Frame::ApplyTo(arg));
            run_delayed(body, env)
        }
        // Activating a labelled consumer runs exactly one branch.
        Value::CoCase { co, env } => {
            let Node::CoCase(branches) = node(co) else {
                return Err(EvalError::TypeMismatch("a `mu` value must be a co-case".into()));
            };
            let Value::Tagged(label, payload) = arg else {
                return Err(EvalError::TypeMismatch(format!(
                    "activating a `mu` consumer requires a labelled value, got {}",
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
        // A request demands the menu a delayed computation produces: run it,
        // then send the request to the result.
        Value::Tagged(label, payload)
            if matches!(arg, Value::Delayed { .. } | Value::Adapted { .. }) =>
        {
            kont.push(Frame::ApplyCallee(Value::Tagged(label, payload)));
            force_value(arg, kont)
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
        // The jump: reinstate the captured stack, down to the nearest prompt
        // it shares with the running one, and deliver the value.
        Value::Kont(frames) => {
            kont.jump(&frames)?;
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
        // Resuming a delimited continuation: push the captured work, with its
        // reinstated handler, onto the running stack and deliver the value.
        // What it performs reaches every handler below, its result flows on
        // into the clause's own pending work, and the slice stays shared, so
        // a clause may resume it again.
        Value::Resume(frames) => {
            kont.append(&frames);
            State::Return(arg)
        }
        Value::Builtin(name) if name == "$force" => force_value(arg, kont),
        Value::Builtin(name) if name == "$adapt" => {
            let Value::Tuple(mut parts) = arg else {
                return Err(EvalError::TypeMismatch(
                    "an adapter needs its function and value".into(),
                ));
            };
            if parts.len() != 2 {
                return Err(EvalError::TypeMismatch("an adapter needs two components".into()));
            }
            let value = parts.pop().expect("two components");
            let adapter = parts.pop().expect("two components");
            if matches!(value, Value::Delayed { .. } | Value::Adapted { .. }) {
                State::Return(Value::Adapted { adapter: Box::new(adapter), value: Box::new(value) })
            } else {
                State::Apply { callee: adapter, arg: value }
            }
        }
        Value::Builtin(name) => {
            let mut args = Vec::new();
            if name == "__match_dispatch" {
                args.extend(split_match_payload(&arg));
            } else {
                collect_args(&name, &arg, &mut args);
            }
            builtin_step(&name, args, kont)?
        }
        Value::PartialBuiltin(name, mut collected) => {
            let mut single = if name == "__match_dispatch" {
                split_match_payload(&arg)
            } else {
                let mut out = Vec::new();
                collect_args(&name, &arg, &mut out);
                out
            };
            collected.append(&mut single);
            builtin_step(&name, collected, kont)?
        }
        // A bundle of exits consumes a sum, since the consumer of `(A | B)` is
        // `(-A & -B)`: the alternative's position picks the exit.
        Value::Tuple(mut exits) => {
            let (exit, payload) = match arg {
                Value::Tagged(label, payload)
                    if crate::value::alternative_index(&label).is_some_and(|i| i < exits.len()) =>
                {
                    let index = crate::value::alternative_index(&label).expect("a position");
                    (exits.swap_remove(index), *payload)
                }
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
        let entries = match clauses_value {
            Value::Tuple(entries) => entries,
            other => vec![other],
        };
        for entry in entries {
            if let Value::Tuple(parts) = entry
                && let [Value::Str(op), closure] = parts.as_slice()
            {
                if op == "return" {
                    ret = closure.clone();
                } else {
                    clauses.insert(op.clone(), closure.clone());
                }
            }
        }
        kont.push(Frame::Prompt { id: fresh_prompt_id(), clauses: Rc::new(clauses), ret });
        return Ok(State::Apply { callee: body_thunk, arg: Value::Unit });
    }
    if name == "__enter_poly" {
        // The body of a row-polymorphic function. Its handlers sit above
        // this barrier and catch only code born inside the call. A closure
        // the caller passed in was born earlier, so what it performs passes
        // through to the caller's handler.
        let mut args = args.into_iter();
        let thunk = args.next().unwrap_or(Value::Unit);
        let Value::Closure { body, env, .. } = thunk else {
            return Err(EvalError::TypeMismatch(
                "a row-polymorphic function body must be a thunk".into(),
            ));
        };
        let id = fresh_barrier_id();
        let mut aware = std::collections::HashSet::new();
        if let Some(Value::Str(names)) = args.next() {
            for name in names.split(',').filter(|name| !name.is_empty()) {
                aware.insert(name.to_string());
            }
        }
        kont.push(Frame::Barrier { id, aware: Rc::new(aware) });
        kont.push(Frame::Origin { birth: id });
        let mut call_env = env;
        call_env.define_local(Value::Unit);
        return Ok(State::Term(body, call_env));
    }
    if name == "__match_dispatch" {
        let mut it = args.into_iter();
        let scrutinee = it.next().unwrap_or(Value::Unit);
        let mut arms = it.collect::<Vec<_>>();
        // The lowered arm spine ends in unit; that terminator is not an arm.
        if arms.last() == Some(&Value::Unit) {
            arms.pop();
        }
        return next_match_arm(scrutinee, arms);
    }
    // A builtin that offers its outcome activates one of its continuations;
    // the rest return a value.
    match crate::eval::run_offering_builtin(name, &args)? {
        Some((consumer, outcome)) => Ok(State::Apply { callee: consumer, arg: outcome }),
        None => Ok(State::Return(run_builtin_function(name, args)?)),
    }
}

/// Try the arms in order, and run the thunk of the first whose pattern
/// matches.
fn next_match_arm(scrutinee: Value, mut arms: Vec<Value>) -> Result<State, EvalError> {
    while !arms.is_empty() {
        let arm = unwrap_match_arm(&arms.remove(0));
        let (descriptor, thunk) = match &arm {
            Value::Tuple(parts) => match parts.as_slice() {
                [Value::Str(descriptor), thunk] => (descriptor.clone(), thunk.clone()),
                _ => {
                    return Err(EvalError::TypeMismatch(format!(
                        "malformed match arm payload: {}",
                        arm.display()
                    )));
                }
            },
            _ => {
                return Err(EvalError::TypeMismatch(format!(
                    "malformed match arm: {}",
                    arm.display()
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
        return run_match_thunk(&thunk, &bindings);
    }
    Err(EvalError::TypeMismatch("non-exhaustive of".into()))
}

/// The chosen arm's body, with the pattern's bindings in scope. The bindings
/// go in the named overlay (the compiler left pattern variables as
/// `Dynamic`); the thunk's own `__match_arg` parameter is the positional
/// slot the body's de Bruijn indices are relative to.
fn run_match_thunk(thunk: &Value, bindings: &[(String, Value)]) -> Result<State, EvalError> {
    let Value::Closure { body, env, .. } = thunk else {
        return Err(EvalError::TypeMismatch("of arm body must be a thunk".into()));
    };
    let mut call_env = env.clone();
    for (name, value) in bindings {
        call_env.define_named(name.clone(), value.clone());
    }
    call_env.define_local(Value::Unit);
    Ok(State::Term(*body, call_env))
}

#[cfg(test)]
mod tunnel {
    use super::*;
    use std::collections::HashSet;

    fn prompt_over(tail: Kont) -> Kont {
        let mut kont = tail;
        kont.push(Frame::Prompt {
            id: 1,
            clauses: Rc::new(std::collections::HashMap::from([("throw".into(), Value::Unit)])),
            ret: Value::Unit,
        });
        kont
    }

    #[test]
    fn an_older_closure_tunnels_through_an_unaware_handler() {
        let mut under = Kont::empty();
        under.push(Frame::Barrier { id: 2, aware: Rc::new(HashSet::new()) });
        let prompt = prompt_over(under);
        let tail = prompt.0.unwrap().tail.clone();
        assert!(Kont::tunneled(&tail, 1, "throw"));
    }

    #[test]
    fn an_older_closure_is_caught_for_an_effect_the_function_names() {
        let mut aware = HashSet::new();
        aware.insert("throw".into());
        let mut under = Kont::empty();
        under.push(Frame::Barrier { id: 2, aware: Rc::new(aware) });
        let prompt = prompt_over(under);
        let tail = prompt.0.unwrap().tail.clone();
        assert!(!Kont::tunneled(&tail, 1, "throw"));
    }

    #[test]
    fn code_born_inside_the_call_is_caught() {
        let mut under = Kont::empty();
        under.push(Frame::Barrier { id: 2, aware: Rc::new(HashSet::new()) });
        let prompt = prompt_over(under);
        let tail = prompt.0.unwrap().tail.clone();
        assert!(!Kont::tunneled(&tail, 2, "throw"));
    }

    #[test]
    fn a_handler_outside_every_polymorphic_call_is_caught() {
        let prompt = prompt_over(Kont::empty());
        let tail = prompt.0.unwrap().tail.clone();
        assert!(!Kont::tunneled(&tail, 1, "throw"));
    }
}
