//! Minimal tracing spans for the offline build environment.
//!
//! The public API is deliberately shaped like a tiny subset of `tracing`
//! (`span!`, `Span::enter`, and event messages). Setting `SLC_TRACE=1`
//! enables stderr output; otherwise every operation is a no-op.

use std::cell::RefCell;
use std::fmt::Display;

thread_local! {
    static DEPTH: RefCell<usize> = const { RefCell::new(0) };
}

fn enabled() -> bool {
    // Cache the environment lookup once per thread to keep tracing cheap.
    thread_local! {
        static ENABLED: bool = std::env::var("SLC_TRACE").is_ok();
    }
    ENABLED.with(|enabled| *enabled)
}

/// A lexical tracing span.
pub struct Span {
    name: &'static str,
}

impl Span {
    pub fn new(name: &'static str) -> Self {
        if enabled() {
            DEPTH.with(|depth| {
                let depth = &mut *depth.borrow_mut();
                eprintln!("{}-> {name}", "  ".repeat(*depth));
                *depth += 1;
            });
        }
        Self { name }
    }

    /// Enter the span. The returned guard exits on drop.
    pub fn enter(&self) -> SpanGuard<'_> {
        SpanGuard { span: self }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if enabled() {
            DEPTH.with(|depth| {
                let depth = &mut *depth.borrow_mut();
                *depth = depth.saturating_sub(1);
                eprintln!("{}<- {}", "  ".repeat(*depth), self.name);
            });
        }
    }
}

/// RAII guard for a tracing span.
pub struct SpanGuard<'a> {
    span: &'a Span,
}

impl Drop for SpanGuard<'_> {
    fn drop(&mut self) {
        let _ = self.span;
    }
}

/// Create a span with a static name.
#[macro_export]
macro_rules! span {
    ($name:literal) => {
        $crate::tracing::Span::new($name)
    };
}

/// Emit an event if tracing is enabled.
pub fn event(message: impl Display) {
    if enabled() {
        DEPTH.with(|depth| {
            eprintln!("{}{}", "  ".repeat(*depth.borrow()), message);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_has_guard_and_event_is_callable() {
        let span = span!("test-span");
        let _guard = span.enter();
        event("event inside span");
    }
}
