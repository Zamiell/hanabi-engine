//! Cooperative, request-local control. Cancellation never returns a partially
//! compiled convention state or labels a partially visited belief exhaustive.

use std::{
    cell::Cell,
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

pub struct AnalysisControl {
    cancellation: CancellationToken,
    deadline: Option<Instant>,
    work_limit: u64,
    used: Cell<u64>,
}

impl Default for AnalysisControl {
    fn default() -> Self {
        Self::new(CancellationToken::default(), None, u64::MAX)
    }
}

impl AnalysisControl {
    #[must_use]
    pub const fn new(
        cancellation: CancellationToken,
        deadline: Option<Instant>,
        work_limit: u64,
    ) -> Self {
        Self {
            cancellation,
            deadline,
            work_limit,
            used: Cell::new(0),
        }
    }

    #[must_use]
    pub fn used(&self) -> u64 {
        self.used.get()
    }

    pub(crate) fn interrupt_handle(&self) -> RequestInterrupt {
        RequestInterrupt {
            cancellation: self.cancellation.clone(),
            deadline: self.deadline,
        }
    }

    /// A work unit is a compiler invocation, projection step, or world/search
    /// node, not elapsed time. Recursive inference also checks cancellation and
    /// deadlines, without consuming additional search-node work units.
    pub(crate) fn checkpoint(&self) -> Result<(), AnalysisStopped> {
        crate::budget::checkpoint();
        if self.cancellation.0.load(Ordering::Relaxed) {
            return Err(AnalysisStopped::Cancelled);
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(AnalysisStopped::Deadline);
        }
        if self.used.get() >= self.work_limit {
            return Err(AnalysisStopped::WorkLimit);
        }
        self.used.set(self.used.get().saturating_add(1));
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalysisStopped {
    Cancelled,
    Deadline,
    WorkLimit,
}

impl fmt::Display for AnalysisStopped {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Cancelled => "analysis cancelled",
            Self::Deadline => "analysis deadline reached",
            Self::WorkLimit => "analysis work limit reached",
        })
    }
}
impl std::error::Error for AnalysisStopped {}

// A non-counting handle used at fine-grained inference checkpoints.
pub(crate) struct RequestInterrupt {
    cancellation: CancellationToken,
    deadline: Option<Instant>,
}
impl RequestInterrupt {
    pub(crate) fn reason(&self) -> Option<AnalysisStopped> {
        if self.cancellation.0.load(Ordering::Relaxed) {
            Some(AnalysisStopped::Cancelled)
        } else if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            Some(AnalysisStopped::Deadline)
        } else {
            None
        }
    }
}
