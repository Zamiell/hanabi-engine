use core::ops::Range;

use hanabi_core::CardId;

use super::{CardKnowledgeEffect, HGroupRuleId, RulePhase};

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
    /// Exact materialized card facts changed by this rule. Counts are useful
    /// for profiling, but cannot detect replacing one card with another.
    pub(super) card_changes: Vec<CardFactChange>,
}

impl RuleProposal {
    pub(super) fn is_empty(&self) -> bool {
        self.signal_range.is_empty()
            && self.promise_transition_range.is_empty()
            && self.mutations.is_empty()
            && self.card_changes.is_empty()
    }
}

/// Atomic result of applying all enabled convention rules to one public
/// event. Tests and decision traces can inspect causality without
/// reverse-engineering the explanation journal.
#[derive(Clone, Debug)]
pub(super) struct ConventionTransitionResult {
    pub(super) turn: u32,
    pub(super) proposals: Vec<RuleProposal>,
    /// Net causal delta of the entire public event, including inline clue and
    /// connection handling as well as post-event recognizers.
    pub(super) delta: ConventionTransitionDelta,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MaterializedCardFact {
    ExplicitlyClued,
    InvisiblyClued,
    AlreadyPlaying,
    ChopMoved,
    ForcedPlayable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FactChangeKind {
    Added,
    Removed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CardFactChange {
    pub(super) fact: MaterializedCardFact,
    pub(super) card: CardId,
    pub(super) kind: FactChangeKind,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct ConventionTransitionDelta {
    pub(super) card_changes: Vec<CardFactChange>,
    pub(super) knowledge_changes: Vec<CardKnowledgeEffect>,
}

impl ConventionTransitionDelta {
    pub(super) fn is_empty(&self) -> bool {
        self.card_changes.is_empty() && self.knowledge_changes.is_empty()
    }

    pub(super) fn added_cards(&self) -> impl Iterator<Item = CardId> + '_ {
        self.card_changes
            .iter()
            .filter_map(|change| (change.kind == FactChangeKind::Added).then_some(change.card))
    }
}
