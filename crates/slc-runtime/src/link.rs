//! The interpreter's rendezvous for `fork::exchange`.
//!
//! One `talk` is open at a time. Each arm runs above that hand's prompt, so
//! `give` and `take` re-enter this installation. A blocked arm is a stored
//! resumption, not a spinning step. The live stack under the prompt stays
//! put; installing an arm pushes the prompt again, and leaving an arm pops
//! back down through it before the next arm or the final resume goes on.

use crate::chunk::{Node, node};
use crate::eval::EvalError;
use crate::machine::{Frame, Kont, State, menu_dispatch};
use crate::value::Value;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
    static STATE: RefCell<LinkState> =
        const { RefCell::new(LinkState { next_token: 0, open: None }) };
}

struct LinkState {
    next_token: i64,
    open: Option<Session>,
}

struct Session {
    prompt: Frame,
    prompt_id: u64,
    final_resume: Kont,
    arms: Vec<Arm>,
    running: Option<usize>,
    ends: HashMap<i64, End>,
}

struct End {
    taker: Option<usize>,
    /// The next `take` starts at this arm and walks the talk list.
    cursor: usize,
}

enum Arm {
    Fresh(Value),
    Running,
    Runnable { resume: Kont, delivery: Value },
    BlockedGive { token: i64, value: Value, resume: Kont },
    BlockedTake { token: i64, resume: Kont },
    Done(Value),
}

enum Choice {
    Ready(usize),
    Wait(usize),
    Unmatched,
}

enum Step {
    Fatal(&'static str),
    Begin { index: usize, prompt_id: u64 },
    Switch { prompt_id: u64, resume: Kont, value: Value },
    Deliver { prompt_id: u64, resume: Kont, list: Value },
}

enum Started {
    Fresh { menu: Value, prompt: Frame },
    Runnable { resume: Kont, delivery: Value },
}

/// Clears a session left by a fatal when the outermost `run` begins, and
/// again when it ends, so the next program does not see `link already open`.
pub(crate) struct Guard;

impl Guard {
    pub(crate) fn enter() -> Self {
        DEPTH.with(|depth| {
            if depth.get() == 0 {
                clear();
            }
            depth.set(depth.get() + 1);
        });
        Guard
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        DEPTH.with(|depth| {
            let next = depth.get().saturating_sub(1);
            depth.set(next);
            if next == 0 {
                clear();
            }
        });
    }
}

fn clear() {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.next_token = 0;
        state.open = None;
    });
}

fn fatal(message: &'static str) -> EvalError {
    EvalError::TypeMismatch(message.into())
}

pub(crate) fn mint() -> Result<State, EvalError> {
    let token = STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.next_token += 1;
        state.next_token
    });
    Ok(State::Return(Value::Int(token)))
}

pub(crate) fn start(args: Vec<Value>, kont: &mut Kont) -> Result<State, EvalError> {
    let mut args = args.into_iter();
    let arms = list_values(args.next().unwrap_or(Value::Unit))?;
    let resume = as_resume(args.next().unwrap_or(Value::Unit))?;
    let Some(prompt) = resume.top_prompt() else {
        return Err(EvalError::TypeMismatch("link converse has no handler".into()));
    };
    let Frame::Prompt { id: prompt_id, .. } = &prompt else {
        return Err(EvalError::TypeMismatch("link converse has no handler".into()));
    };
    let prompt_id = *prompt_id;
    let step = STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.open.is_some() {
            return Step::Fatal("link already open");
        }
        let count = arms.len();
        state.open = Some(Session {
            prompt,
            prompt_id,
            final_resume: resume,
            arms: arms.into_iter().map(Arm::Fresh).collect(),
            running: None,
            ends: HashMap::new(),
        });
        if count == 0 {
            let session = state.open.take().expect("just opened");
            deliver(session)
        } else {
            Step::Begin { index: 0, prompt_id }
        }
    });
    apply(step, kont)
}

pub(crate) fn give(
    token: Value,
    value: Value,
    resume: Value,
    kont: &mut Kont,
) -> Result<State, EvalError> {
    let token = as_token(token)?;
    let resume = as_resume(resume)?;
    let step = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(session) = state.open.as_mut() else {
            return Step::Fatal("unmatched link");
        };
        let Some(index) = session.running else {
            return Step::Fatal("unmatched link");
        };
        if index >= session.arms.len() {
            return Step::Fatal("unmatched link");
        }
        session.arms[index] = Arm::BlockedGive { token, value, resume };
        session.running = None;
        session.ends.entry(token).or_insert(End { taker: None, cursor: 0 });
        if let Some(taker) = waiting_taker(session, token) {
            match select_giver(session, token, Some(taker)) {
                Choice::Ready(giver) => rendezvous(session, taker, giver),
                Choice::Wait(prefer) => schedule(session, Some(prefer), index),
                Choice::Unmatched => Step::Fatal("unmatched link"),
            }
        } else {
            schedule(session, None, index)
        }
    });
    apply(step, kont)
}

pub(crate) fn take(token: Value, resume: Value, kont: &mut Kont) -> Result<State, EvalError> {
    let token = as_token(token)?;
    let resume = as_resume(resume)?;
    let step = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(session) = state.open.as_mut() else {
            return Step::Fatal("unmatched link");
        };
        let Some(index) = session.running else {
            return Step::Fatal("unmatched link");
        };
        if index >= session.arms.len() {
            return Step::Fatal("unmatched link");
        }
        {
            let end = session.ends.entry(token).or_insert(End { taker: None, cursor: 0 });
            if let Some(existing) = end.taker
                && existing != index
            {
                return Step::Fatal("end already taken");
            }
            end.taker = Some(index);
        }
        session.arms[index] = Arm::BlockedTake { token, resume };
        session.running = None;
        match select_giver(session, token, Some(index)) {
            Choice::Ready(giver) => rendezvous(session, index, giver),
            Choice::Wait(prefer) => schedule(session, Some(prefer), index),
            Choice::Unmatched => Step::Fatal("unmatched link"),
        }
    });
    apply(step, kont)
}

pub(crate) fn arm_finished(
    index: usize,
    value: Value,
    kont: &mut Kont,
) -> Result<State, EvalError> {
    let step = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(session) = state.open.as_mut() else {
            return Step::Fatal("unmatched link");
        };
        if index >= session.arms.len() {
            return Step::Fatal("unmatched link");
        }
        session.arms[index] = Arm::Done(value);
        session.running = None;
        if session.arms.iter().all(|arm| matches!(arm, Arm::Done(_))) {
            let session = state.open.take().expect("open");
            return deliver(session);
        }
        let Some(session) = state.open.as_mut() else {
            return Step::Fatal("unmatched link");
        };
        if let Some(step) = poke(session) {
            return step;
        }
        schedule(session, None, index)
    });
    apply(step, kont)
}

fn deliver(session: Session) -> Step {
    let Session { prompt_id, final_resume, arms, .. } = session;
    let values = arms
        .into_iter()
        .map(|arm| match arm {
            Arm::Done(value) => value,
            _ => unreachable!("deliver runs when every arm is done"),
        })
        .collect();
    Step::Deliver { prompt_id, resume: final_resume, list: list_value(values) }
}

fn schedule(session: &Session, prefer: Option<usize>, from: usize) -> Step {
    if let Some(index) = prefer
        && startable(&session.arms[index])
    {
        return Step::Begin { index, prompt_id: session.prompt_id };
    }
    if let Some(index) = next_startable(session, from) {
        return Step::Begin { index, prompt_id: session.prompt_id };
    }
    Step::Fatal("unmatched link")
}

fn poke(session: &mut Session) -> Option<Step> {
    let count = session.arms.len();
    for index in 0..count {
        let Arm::BlockedTake { token, .. } = &session.arms[index] else {
            continue;
        };
        let token = *token;
        if let Choice::Ready(giver) = select_giver(session, token, Some(index)) {
            return Some(rendezvous(session, index, giver));
        }
    }
    None
}

/// Walk the talk list from this end's cursor. An earlier arm that has not
/// given and has not finished is waited for, even when a later giver is
/// already blocked. The arm that takes is not a giver.
fn select_giver(session: &Session, token: i64, taker: Option<usize>) -> Choice {
    let count = session.arms.len();
    if count == 0 {
        return Choice::Unmatched;
    }
    let start = session.ends.get(&token).map(|end| end.cursor).unwrap_or(0) % count;
    let mut waiting = None;
    for step in 0..count {
        let index = (start + step) % count;
        if taker == Some(index) {
            continue;
        }
        match &session.arms[index] {
            Arm::BlockedGive { token: given, .. } if *given == token => {
                return if let Some(earlier) = waiting {
                    Choice::Wait(earlier)
                } else {
                    Choice::Ready(index)
                };
            }
            Arm::Done(_) => {}
            _ => {
                if waiting.is_none() {
                    waiting = Some(index);
                }
            }
        }
    }
    waiting.map(Choice::Wait).unwrap_or(Choice::Unmatched)
}

fn rendezvous(session: &mut Session, taker: usize, giver: usize) -> Step {
    let prompt_id = session.prompt_id;
    let count = session.arms.len();
    let giver_arm = std::mem::replace(&mut session.arms[giver], Arm::Running);
    let taker_arm = std::mem::replace(&mut session.arms[taker], Arm::Running);
    let Arm::BlockedGive { token, value, resume: giver_resume } = giver_arm else {
        session.arms[giver] = giver_arm;
        session.arms[taker] = taker_arm;
        return Step::Fatal("unmatched link");
    };
    let Arm::BlockedTake { resume: taker_resume, .. } = taker_arm else {
        session.arms[giver] = Arm::BlockedGive { token, value, resume: giver_resume };
        session.arms[taker] = taker_arm;
        return Step::Fatal("unmatched link");
    };
    session.arms[giver] = Arm::Runnable { resume: giver_resume, delivery: Value::Unit };
    session.arms[taker] = Arm::Running;
    session.running = Some(taker);
    if let Some(end) = session.ends.get_mut(&token) {
        end.cursor = (giver + 1) % count;
    }
    Step::Switch { prompt_id, resume: taker_resume, value }
}

fn waiting_taker(session: &Session, token: i64) -> Option<usize> {
    session
        .arms
        .iter()
        .position(|arm| matches!(arm, Arm::BlockedTake { token: waiting, .. } if *waiting == token))
}

fn startable(arm: &Arm) -> bool {
    matches!(arm, Arm::Fresh(_) | Arm::Runnable { .. })
}

fn next_startable(session: &Session, from: usize) -> Option<usize> {
    let count = session.arms.len();
    if count == 0 {
        return None;
    }
    (1..=count).map(|step| (from + step) % count).find(|&index| startable(&session.arms[index]))
}

fn apply(step: Step, kont: &mut Kont) -> Result<State, EvalError> {
    match step {
        Step::Fatal(message) => Err(fatal(message)),
        Step::Deliver { prompt_id, resume, list } => {
            unwind(kont, prompt_id);
            kont.append(&resume);
            Ok(State::Return(list))
        }
        Step::Switch { prompt_id, resume, value } => {
            unwind(kont, prompt_id);
            kont.append(&resume);
            Ok(State::Return(value))
        }
        Step::Begin { index, prompt_id } => {
            unwind(kont, prompt_id);
            begin(index, kont)
        }
    }
}

fn unwind(kont: &mut Kont, prompt_id: u64) {
    if kont.contains_prompt(prompt_id) {
        kont.pop_through_prompt(prompt_id);
    }
}

fn begin(index: usize, kont: &mut Kont) -> Result<State, EvalError> {
    let started = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(session) = state.open.as_mut() else {
            return Err(fatal("unmatched link"));
        };
        if index >= session.arms.len() {
            return Err(fatal("unmatched link"));
        }
        let prompt = session.prompt.clone();
        let arm = std::mem::replace(&mut session.arms[index], Arm::Running);
        session.running = Some(index);
        match arm {
            Arm::Fresh(menu) => Ok(Started::Fresh { menu, prompt }),
            Arm::Runnable { resume, delivery } => Ok(Started::Runnable { resume, delivery }),
            other => {
                session.arms[index] = other;
                session.running = None;
                Err(fatal("unmatched link"))
            }
        }
    })?;
    match started {
        Started::Fresh { menu, prompt } => {
            kont.push(prompt);
            kont.push(Frame::LinkArm { index });
            force_lazy(menu, kont)
        }
        Started::Runnable { resume, delivery } => {
            kont.append(&resume);
            Ok(State::Return(delivery))
        }
    }
}

fn force_lazy(menu: Value, kont: &Kont) -> Result<State, EvalError> {
    let Value::Menu { node: menu, env } = menu else {
        return Err(EvalError::TypeMismatch(format!(
            "a link arm must be a menu, got {}",
            menu.display()
        )));
    };
    let Node::CoMatch(branches) = node(menu) else {
        return Err(EvalError::TypeMismatch("a link arm must be a menu".into()));
    };
    let Some(label) = branches
        .iter()
        .find(|branch| branch.label.as_ref() == "force" || branch.label.ends_with("::force"))
        .map(|branch| branch.label.clone())
    else {
        return Err(EvalError::TypeMismatch("a link arm has no force".into()));
    };
    menu_dispatch(menu, env, &label, Value::Kont(kont.clone()))
}

fn as_resume(value: Value) -> Result<Kont, EvalError> {
    match value {
        Value::Resume(frames) => Ok(frames),
        other => Err(EvalError::TypeMismatch(format!(
            "a link resume must be a continuation, got {}",
            other.display()
        ))),
    }
}

fn as_token(value: Value) -> Result<i64, EvalError> {
    match value {
        Value::Int(token) => Ok(token),
        other => Err(EvalError::TypeMismatch(format!(
            "a link token must be an integer, got {}",
            other.display()
        ))),
    }
}

fn list_values(value: Value) -> Result<Vec<Value>, EvalError> {
    let mut out = Vec::new();
    let mut current = value;
    loop {
        match current {
            Value::Tagged(label, _) if is_end(&label) => {
                return Ok(out);
            }
            Value::Tagged(label, payload) if is_link(&label) => {
                let Value::Tuple(mut items) = *payload else {
                    return Err(EvalError::TypeMismatch(
                        "an arm spine must be Arm of a menu and a tail".into(),
                    ));
                };
                if items.len() != 2 {
                    return Err(EvalError::TypeMismatch(
                        "an arm spine must be Arm of a menu and a tail".into(),
                    ));
                }
                let tail = items.pop().expect("tail");
                let head = items.pop().expect("head");
                out.push(head);
                current = tail;
            }
            other => {
                return Err(EvalError::TypeMismatch(format!(
                    "talk arms must be an Arms spine, got {}",
                    other.display()
                )));
            }
        }
    }
}

fn is_end(label: &str) -> bool {
    label == "Nil" || label == "Stop" || label.ends_with("::Nil") || label.ends_with("::Stop")
}

fn is_link(label: &str) -> bool {
    label == "Cons" || label == "Arm" || label.ends_with("::Cons") || label.ends_with("::Arm")
}

fn list_value(mut values: Vec<Value>) -> Value {
    // The list module qualifies these tags. `Display` matches them by that name.
    let mut tail = Value::Tagged("list::List::Nil".into(), Box::new(Value::Unit));
    while let Some(head) = values.pop() {
        tail = Value::Tagged("list::List::Cons".into(), Box::new(Value::Tuple(vec![head, tail])));
    }
    tail
}
