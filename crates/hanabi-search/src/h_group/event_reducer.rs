use hanabi_core::CardId;

use super::{
    ConnectionManager, ConventionCardSetSnapshot, ConventionJournal, EffectSource,
    HGroupClueInterpretation, IdentitySet, PlayerSet, ProvenancedCardSet, RequiredFix,
    reconcile_connection_fact_lifecycles,
};

/// Mutable event effects shared by every convention rule.
///
/// This is the only capability handed to recognizers. It keeps mutation
/// separate from `HGroupTurnContext`, whose before/after views are immutable,
/// and centralizes provenance reconciliation at the rule boundary.
pub(super) struct HGroupRuleEffects<'a> {
    pub(super) explicitly_clued: &'a ProvenancedCardSet,
    pub(super) invisibly_clued: &'a mut ProvenancedCardSet,
    pub(super) clues: &'a [HGroupClueInterpretation],
    pub(super) already_playing: &'a mut ProvenancedCardSet,
    pub(super) pending: &'a mut ConnectionManager,
    pub(super) chop_moved: &'a mut ProvenancedCardSet,
    pub(super) must_clue: &'a mut PlayerSet,
    pub(super) forced_playable: &'a mut ProvenancedCardSet,
    pub(super) discard_now: &'a mut Vec<CardId>,
    pub(super) implicit_saves: &'a mut Vec<(CardId, IdentitySet)>,
    pub(super) required_fix: &'a mut Option<RequiredFix>,
    pub(super) signals: &'a mut ConventionJournal,
}

impl HGroupRuleEffects<'_> {
    pub(super) fn card_snapshot(&self) -> ConventionCardSetSnapshot {
        ConventionCardSetSnapshot::capture(
            self.explicitly_clued,
            self.invisibly_clued,
            self.already_playing,
            self.chop_moved,
            self.forced_playable,
        )
    }

    pub(super) fn reconcile_card_sources(
        &mut self,
        before: &ConventionCardSetSnapshot,
        source: EffectSource,
    ) {
        self.invisibly_clued
            .reconcile_mask(before.invisibly_clued, source);
        self.already_playing
            .reconcile_mask(before.already_playing, source);
        self.chop_moved.reconcile_mask(before.chop_moved, source);
        self.forced_playable
            .reconcile_mask(before.forced_playable, source);
    }

    pub(super) fn reconcile_connection_lifecycles(&mut self, transition_start: usize) {
        reconcile_connection_fact_lifecycles(
            self.pending,
            transition_start,
            self.invisibly_clued,
            self.already_playing,
            self.forced_playable,
        );
    }
}
