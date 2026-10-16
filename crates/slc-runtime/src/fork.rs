//! Host threads for `fork::parallel`.
//!
//! `__fork_both` forces two pure thunks at the same time and returns the
//! positional pair. Each force has its own stack. They share the fuel counter
//! and the installed chunk. A fatal in either arm stops the other, so a loop
//! in one arm does not hide the other's failure. The value of the pair does
//! not depend on which arm finishes first.

use crate::chunk::{Node, current_chunk, node, with_chunk};
use crate::eval::EvalError;
use crate::machine::{self, Kont, menu_dispatch};
use crate::value::Value;
use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Shared fuel and the stop flag for one parallel region, including a nested
/// `parallel` inside a force. The outermost `__fork_both` owns it.
pub(crate) struct Clock {
    fuel: AtomicUsize,
    stop: AtomicBool,
}

/// How far one force may run ahead of the other before it yields. The
/// native safepoint uses the same bound (`slc_rt_fork_fair`).
const PACE_AHEAD: usize = 64;
/// Host stack for one force. The main thread is 8 MiB; a spawned thread
/// would otherwise be 2 MiB and refuse a recursion `sequential` can run.
const WORKER_STACK: usize = 8 << 20;
/// After both forces have taken this many steps, stop yielding.
const PACE_WARMUP: usize = 256;

struct Pace {
    steps: [AtomicUsize; 2],
}

thread_local! {
    static PACE: Cell<*const Pace> = const { Cell::new(std::ptr::null()) };
    static PACE_SLOT: Cell<usize> = const { Cell::new(0) };
    static PACE_HOT: Cell<bool> = const { Cell::new(false) };
}

/// Yield when this force is well ahead of its sibling, until both have
/// started or one has finished. A tight loop then cannot spend the shared
/// fuel before the other force has run.
fn pace() {
    if PACE_HOT.with(|cell| cell.get()) {
        return;
    }
    let ptr = PACE.with(|cell| cell.get());
    if ptr.is_null() {
        return;
    }
    let pace = unsafe { &*ptr };
    let slot = PACE_SLOT.with(|cell| cell.get());
    let mine = pace.steps[slot].fetch_add(1, Ordering::Relaxed);
    let other = pace.steps[1 - slot].load(Ordering::Relaxed);
    if other == usize::MAX || (mine > PACE_WARMUP && other > PACE_WARMUP) {
        PACE_HOT.with(|cell| cell.set(true));
        return;
    }
    if mine > other.saturating_add(PACE_AHEAD) {
        std::thread::yield_now();
    }
}

struct PaceGuard {
    pace: *const Pace,
    slot: usize,
    hot: bool,
}

impl Drop for PaceGuard {
    fn drop(&mut self) {
        PACE.with(|cell| cell.set(self.pace));
        PACE_SLOT.with(|cell| cell.set(self.slot));
        PACE_HOT.with(|cell| cell.set(self.hot));
    }
}

fn install_pace(pace: &Pace, slot: usize) -> PaceGuard {
    let pace_ptr = PACE.with(|cell| cell.replace(std::ptr::from_ref(pace)));
    let prev_slot = PACE_SLOT.with(|cell| cell.replace(slot));
    let hot = PACE_HOT.with(|cell| cell.replace(false));
    PaceGuard { pace: pace_ptr, slot: prev_slot, hot }
}

impl Clock {
    pub(crate) fn charge(&self) -> Result<(), EvalError> {
        if self.stop.load(Ordering::Relaxed) {
            return Err(EvalError::Stopped);
        }
        pace();
        loop {
            let left = self.fuel.load(Ordering::Relaxed);
            if left == 0 {
                return Err(EvalError::Diverged);
            }
            if self
                .fuel
                .compare_exchange(left, left - 1, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return Ok(());
            }
        }
    }

    fn halt(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

thread_local! {
    static CLOCK: Cell<*const Clock> = const { Cell::new(std::ptr::null()) };
}

pub(crate) fn clock() -> Option<&'static Clock> {
    let ptr = CLOCK.with(|cell| cell.get());
    if ptr.is_null() { None } else { Some(unsafe { &*ptr }) }
}

struct ClockGuard(*const Clock);

/// The parent joins both forces before the clock is dropped.
#[derive(Copy, Clone)]
struct ClockPtr(*const Clock);
// SAFETY: the pointer is read only while `overlap` is still on the stack,
// and both forces are joined before that frame returns. `Clock` is shared
// through atomics.
unsafe impl Send for ClockPtr {}

impl ClockPtr {
    /// A method, so the closure captures this wrapper rather than the raw pointer.
    fn get(self) -> *const Clock {
        self.0
    }
}

impl Drop for ClockGuard {
    fn drop(&mut self) {
        CLOCK.with(|cell| cell.set(self.0));
    }
}

fn install_clock(clock: &Clock) -> ClockGuard {
    let previous = CLOCK.with(|cell| cell.replace(std::ptr::from_ref(clock)));
    ClockGuard(previous)
}

/// Force both thunks and return `(left, right)`.
pub(crate) fn both(left: Value, right: Value) -> Result<Value, EvalError> {
    if let Some(existing) = clock() {
        return overlap(existing, left, right);
    }
    let owned =
        Clock { fuel: AtomicUsize::new(machine::read_fuel()), stop: AtomicBool::new(false) };
    let result = overlap(&owned, left, right);
    machine::write_fuel(owned.fuel.load(Ordering::Relaxed));
    result
}

/// Both arms pass this before either force charges fuel, so a loop in the
/// arm that the scheduler runs first cannot spend the shared budget during
/// the other arm's startup.
fn arrive(entered: &AtomicUsize) {
    entered.fetch_add(1, Ordering::Release);
    while entered.load(Ordering::Acquire) < 2 {
        std::thread::yield_now();
    }
    std::thread::yield_now();
}

fn overlap(clock: &Clock, left: Value, right: Value) -> Result<Value, EvalError> {
    let chunk = current_chunk();
    let entered = Arc::new(AtomicUsize::new(0));
    let pace = Arc::new(Pace { steps: [AtomicUsize::new(0), AtomicUsize::new(0)] });
    // Joined before this function returns, so the clock outlives both forces.
    let clock_ptr = ClockPtr(std::ptr::from_ref(clock));
    let mut handles = Vec::with_capacity(2);
    for (index, thunk) in [left, right].into_iter().enumerate() {
        let entered = Arc::clone(&entered);
        let pace = Arc::clone(&pace);
        let chunk = chunk.clone();
        let handle = std::thread::Builder::new().stack_size(WORKER_STACK).spawn(move || {
            let clock = unsafe { &*clock_ptr.get() };
            let _clock = install_clock(clock);
            let _pace = install_pace(&pace, index);
            arrive(&entered);
            let result = with_chunk(chunk, || force_thunk(thunk));
            if result.is_ok() {
                pace.steps[index].store(usize::MAX, Ordering::Relaxed);
            } else {
                clock.halt();
            }
            result
        });
        match handle {
            Ok(handle) => handles.push(handle),
            Err(error) => {
                clock.halt();
                for handle in handles {
                    let _ = handle.join();
                }
                return Err(EvalError::TypeMismatch(format!(
                    "a parallel force did not start: {error}"
                )));
            }
        }
    }
    let mut done = [None, None];
    let mut panicked = None;
    for (index, handle) in handles.into_iter().enumerate() {
        match handle.join() {
            Ok(result) => {
                if let Err(error) = &result
                    && !matches!(error, EvalError::Stopped)
                {
                    clock.halt();
                }
                done[index] = Some(result);
            }
            Err(payload) => {
                clock.halt();
                if panicked.is_none() {
                    panicked = Some(payload);
                }
            }
        }
    }
    if let Some(payload) = panicked {
        std::panic::resume_unwind(payload);
    }
    let left = done[0].take().expect("left force");
    let right = done[1].take().expect("right force");
    match (left, right) {
        (Ok(left), Ok(right)) => Ok(Value::Tuple(vec![left, right])),
        (Err(EvalError::Stopped), Err(EvalError::Stopped)) => Err(EvalError::Diverged),
        (Err(EvalError::Stopped), Err(error)) | (Err(error), Err(EvalError::Stopped)) => Err(error),
        (Err(EvalError::Stopped), Ok(_)) | (Ok(_), Err(EvalError::Stopped)) => {
            Err(EvalError::Diverged)
        }
        (Err(left), Err(_)) => Err(left),
        (Err(error), Ok(_)) | (Ok(_), Err(error)) => Err(error),
    }
}

/// Peel delays, then demand the menu's `force`. The answer continuation is an
/// empty stack, so the produced value returns from this run.
fn force_thunk(thunk: Value) -> Result<Value, EvalError> {
    let mut fuel = usize::MAX;
    let mut kont = Kont::empty();
    let start = machine::force_value(thunk, &mut kont);
    let peeled = machine::run(start, kont, &mut fuel)?;
    let Value::Menu { node: menu, env } = peeled else {
        return Err(EvalError::TypeMismatch(format!(
            "parallel forces a menu, got {}",
            peeled.display()
        )));
    };
    let Node::CoMatch(branches) = node(menu) else {
        return Err(EvalError::TypeMismatch("parallel forces a menu".into()));
    };
    let Some(label) = branches
        .iter()
        .find(|branch| branch.label.as_ref() == "force" || branch.label.ends_with("::force"))
        .map(|branch| branch.label.clone())
    else {
        return Err(EvalError::TypeMismatch("a parallel thunk has no force".into()));
    };
    let state = menu_dispatch(menu, env, &label, Value::Kont(Kont::empty()))?;
    machine::run(state, Kont::empty(), &mut fuel)
}
