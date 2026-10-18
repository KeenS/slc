//! `spawn` runs a proc with `main`'s signature on another host thread.
//!
//! The proc is the same two binders as `main`: the empty value group, then
//! the exit. The thread applies the unit, then the exit, under its own `IO`
//! prompt. The status that reaches the exit stays on that thread. The parent
//! receives `(,)` as soon as the thread has started, and the process waits
//! for every such thread before it reports `main`'s status. A fatal is that
//! report; the proc's `i32` is not.

use crate::chunk::{current_chunk, with_chunk};
use crate::eval::{EvalError, apply_under_io, apply_value};
use crate::machine;
use crate::value::Value;
use std::sync::Mutex;
use std::thread::JoinHandle;

/// Host stack for one proc. The main thread is 8 MiB; a spawned thread would
/// otherwise be 2 MiB and refuse a recursion `main` can run.
const WORKER_STACK: usize = 8 << 20;

fn jobs() -> &'static Mutex<Vec<JoinHandle<Result<(), String>>>> {
    static JOBS: Mutex<Vec<JoinHandle<Result<(), String>>>> = Mutex::new(Vec::new());
    &JOBS
}

/// Start `job` and return `(,)`. The proc's status is discarded.
pub(crate) fn launch(job: Value) -> Result<Value, EvalError> {
    let chunk = current_chunk();
    // A private budget. The parent's counter keeps moving on this thread, and
    // the child must not charge it: the two runs are not one parallel region.
    let fuel = machine::read_fuel();
    let handle = std::thread::Builder::new()
        .name("slc-spawn".into())
        .stack_size(WORKER_STACK)
        .spawn(move || with_chunk(chunk, || run_job(job, fuel)))
        .map_err(|error| EvalError::TypeMismatch(format!("spawn did not start: {error}")))?;
    jobs().lock().unwrap_or_else(|err| err.into_inner()).push(handle);
    Ok(Value::Unit)
}

fn run_job(job: Value, mut fuel: usize) -> Result<(), String> {
    let entry = apply_value(job, Value::Unit, &mut fuel).map_err(|error| error.to_string())?;
    match apply_under_io(entry, Value::Builtin("EXIT".into()), &mut fuel) {
        Err(EvalError::Exit(_)) => Ok(()),
        Ok(value) => Err(format!(
            "spawned proc finished without leaving through its exit, with {}",
            value.display()
        )),
        Err(error) => Err(error.to_string()),
    }
}

/// Wait for every proc `spawn` started. A later `spawn` from one of them is
/// waited for too. The `i32` each proc exits with is already gone.
pub fn finish() -> Result<(), String> {
    let mut errors = Vec::new();
    loop {
        let batch = {
            let mut guard = jobs().lock().unwrap_or_else(|err| err.into_inner());
            std::mem::take(&mut *guard)
        };
        if batch.is_empty() {
            break;
        }
        for handle in batch {
            match handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(message)) => errors.push(message),
                Err(_) => errors.push("spawned proc panicked".into()),
            }
        }
    }
    if errors.is_empty() { Ok(()) } else { Err(errors.join("\n")) }
}
