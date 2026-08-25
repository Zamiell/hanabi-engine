use core::ops::Range;

#[cfg(test)]
use std::cell::Cell;

use super::{HGroupRuleId, RulePhase};

/// Convention-state domains changed by one rule proposal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MutationDomain {
    InvisibleClues,
    PlayingPromises,
    Connections,
    ChopMovement,
    MustClue,
    ForcedPlays,
    RequiredDiscards,
    ImplicitSaves,
    RequiredFix,
    CurrentFacts,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct MutationSet(u16);

impl MutationSet {
    pub(super) fn insert(&mut self, domain: MutationDomain) {
        self.0 |= 1 << domain as u8;
    }

    pub(super) const fn contains(self, domain: MutationDomain) -> bool {
        self.0 & (1 << domain as u8) != 0
    }

    pub(super) const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// Declarative record of what one recognizer contributed to a transition.
/// The audit signal is separate from the materialized domains it changed.
#[derive(Clone, Debug)]
pub(super) struct RuleProposal {
    pub(super) rule: HGroupRuleId,
    pub(super) phase: RulePhase,
    pub(super) signal_range: Range<usize>,
    pub(super) promise_transition_range: Range<usize>,
    pub(super) mutations: MutationSet,
}

impl RuleProposal {
    pub(super) fn is_empty(&self) -> bool {
        self.signal_range.is_empty()
            && self.promise_transition_range.is_empty()
            && self.mutations.is_empty()
    }
}

/// Atomic result of applying all enabled convention rules to one public
/// event. Tests and decision traces can inspect causality without
/// reverse-engineering the explanation journal.
#[derive(Clone, Debug)]
pub(super) struct ConventionTransitionResult {
    pub(super) turn: u32,
    pub(super) proposals: Vec<RuleProposal>,
}

#[cfg(test)]
thread_local! {
    static RECORD_TRANSITIONS: Cell<bool> = const { Cell::new(false) };
}

#[cfg(test)]
pub(super) fn transition_recording_enabled() -> bool {
    RECORD_TRANSITIONS.get()
}

#[cfg(not(test))]
pub(super) const fn transition_recording_enabled() -> bool {
    false
}

#[cfg(test)]
pub(super) fn with_transition_recording<T>(run: impl FnOnce() -> T) -> T {
    struct RecordingGuard<'a> {
        enabled: &'a Cell<bool>,
        previous: bool,
    }

    impl Drop for RecordingGuard<'_> {
        fn drop(&mut self) {
            self.enabled.set(self.previous);
        }
    }

    RECORD_TRANSITIONS.with(|enabled| {
        let previous = enabled.replace(true);
        let _guard = RecordingGuard { enabled, previous };
        run()
    })
}
