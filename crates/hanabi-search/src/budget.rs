//! Request-scoped deadline checks inside recursive convention compilation.
//!
//! The private unwind signal crosses the existing infallible inference APIs.
//! Catch it only at transaction boundaries: incomplete candidates are discarded,
//! RAII restores inference scopes, and ordinary panics are always propagated.
use crate::{AnalysisControl, AnalysisStopped};
use std::{
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    time::{Duration, Instant},
};

pub(crate) const MAX_MOVE_TIME: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Stop {
    MoveDeadline,
    Request(AnalysisStopped),
}
struct Signal(Stop);
struct Active {
    deadline: Instant,
    request: crate::control::RequestInterrupt,
}
thread_local! {
    static ACTIVE: RefCell<Vec<Active>> = const { RefCell::new(Vec::new()) };
}

pub(crate) struct Scope;
impl Scope {
    pub(crate) fn enter(limit: Duration, control: &AnalysisControl) -> Self {
        ACTIVE.with_borrow_mut(|active| {
            active.push(Active {
                deadline: Instant::now() + limit.min(MAX_MOVE_TIME),
                request: control.interrupt_handle(),
            });
        });
        Self
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        ACTIVE.with_borrow_mut(|active| {
            active.pop();
        });
    }
}

pub(crate) fn checkpoint() {
    let stopped = ACTIVE.with_borrow(|active| {
        // Explicit cancellation/deadlines take precedence over a best-so-far result.
        for scope in active {
            if let Some(reason) = scope.request.reason() {
                return Some(Stop::Request(reason));
            }
        }
        (!active.is_empty() && active.iter().any(|scope| Instant::now() >= scope.deadline))
            .then_some(Stop::MoveDeadline)
    });
    if let Some(reason) = stopped {
        // Unlike panic!, this does not invoke the application's panic hook.
        resume_unwind(Box::new(Signal(reason)));
    }
}

pub(crate) fn run<T>(operation: impl FnOnce() -> T) -> Result<T, Stop> {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(value) => Ok(value),
        Err(payload) => match payload.downcast::<Signal>() {
            Ok(signal) => Err(signal.0),
            Err(payload) => resume_unwind(payload),
        },
    }
}

impl Stop {
    pub(crate) const fn reason(self) -> AnalysisStopped {
        match self {
            Self::MoveDeadline => AnalysisStopped::Deadline,
            Self::Request(reason) => reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadline_unwinds_scopes_and_does_not_swallow_other_panics() {
        let control = AnalysisControl::default();
        let stopped = run(|| {
            let _scope = Scope::enter(Duration::ZERO, &control);
            checkpoint();
        });
        assert_eq!(stopped, Err(Stop::MoveDeadline));
        checkpoint(); // The expired scope was removed on unwind.
        let ordinary = catch_unwind(|| run(|| std::panic::resume_unwind(Box::new(42_u8))));
        assert_eq!(*ordinary.unwrap_err().downcast::<u8>().unwrap(), 42);
    }

    #[test]
    fn nested_scopes_cannot_extend_budget_and_requests_take_precedence() {
        let control = AnalysisControl::default();
        let _outer = Scope::enter(Duration::ZERO, &control);
        let _inner = Scope::enter(Duration::from_secs(600), &control);
        ACTIVE.with_borrow(|active| {
            assert!(active.last().unwrap().deadline <= Instant::now() + MAX_MOVE_TIME);
        });
        assert_eq!(run(checkpoint), Err(Stop::MoveDeadline));
        let token = crate::CancellationToken::default();
        let cancelled = AnalysisControl::new(token.clone(), None, u64::MAX);
        let _request = Scope::enter(MAX_MOVE_TIME, &cancelled);
        token.cancel();
        assert_eq!(
            run(checkpoint),
            Err(Stop::Request(AnalysisStopped::Cancelled))
        );
    }
}
