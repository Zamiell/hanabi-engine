use core::ops::{Deref, DerefMut};
use std::collections::HashMap;
use std::hash::BuildHasherDefault;

use hanabi_core::CardId;

use super::HGroupConnectionKind;
use super::connection::{
    ConnectionManager, ConnectionStatus, ConnectionTransitionReason, PromiseId,
};
use super::model::{CardSet, CompactIdHasher};
use super::rules::HGroupRuleId;
use super::transition::{CardFactChange, FactChangeKind, MaterializedCardFact};

/// Stable provenance for a materialized convention fact.
///
/// Facts may have more than one source. Retracting one interpretation only
/// removes the materialized fact when no other live source still establishes
/// it. This is the convention-layer equivalent of truth maintenance.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum EffectSource {
    /// Objective clue information established by the public event itself.
    Event(u32),
    /// A post-event convention rule and the turn on which it fired.
    Rule { turn: u32, rule: HGroupRuleId },
    /// A delayed Prompt/Finesse promise with an independently retractable
    /// lifecycle.
    Promise(PromiseId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct FactRetraction {
    pub(super) card: CardId,
    pub(super) source: EffectSource,
    pub(super) reason: ConnectionTransitionReason,
}

/// A source-aware set with the same read surface as `CardSet`.
///
/// Existing recognizers may continue borrowing the materialized set while
/// reducer boundaries reconcile their changes with a typed source. New code
/// should use `insert_from` and `retract_source` directly.
#[derive(Clone, Debug, Default)]
pub(super) struct ProvenancedCardSet {
    materialized: CardSet,
    sources: HashMap<CardId, Vec<EffectSource>, BuildHasherDefault<CompactIdHasher>>,
    retractions: Vec<FactRetraction>,
}

impl ProvenancedCardSet {
    pub(super) fn insert_from(&mut self, source: EffectSource, card: CardId) -> bool {
        let sources = self.sources.entry(card).or_default();
        if !sources.contains(&source) {
            sources.push(source);
        }
        self.materialized.insert(card)
    }

    pub(super) fn extend_from(
        &mut self,
        source: EffectSource,
        cards: impl IntoIterator<Item = CardId>,
    ) {
        for card in cards {
            self.insert_from(source, card);
        }
    }

    pub(super) fn sources(&self, card: CardId) -> &[EffectSource] {
        self.sources.get(&card).map_or(&[], Vec::as_slice)
    }

    pub(super) fn retract_source(
        &mut self,
        source: EffectSource,
        reason: ConnectionTransitionReason,
    ) -> Vec<CardId> {
        let cards = self
            .sources
            .iter()
            .filter_map(|(card, sources)| sources.contains(&source).then_some(*card))
            .collect::<Vec<_>>();
        let mut removed = Vec::new();
        for card in cards {
            let Some(sources) = self.sources.get_mut(&card) else {
                continue;
            };
            sources.retain(|candidate| *candidate != source);
            self.retractions.push(FactRetraction {
                card,
                source,
                reason,
            });
            if sources.is_empty() {
                self.sources.remove(&card);
                self.materialized.remove(&card);
                removed.push(card);
            }
        }
        removed
    }

    pub(super) fn mask(&self) -> u64 {
        self.materialized
            .iter()
            .fold(0_u64, |mask, card| mask | (1_u64 << card.index()))
    }

    /// Bitset counterpart of `reconcile`, used by the hot replay loop. A
    /// standard game has at most 50 card IDs, so one machine word captures an
    /// exact snapshot without cloning hash tables for every rule.
    pub(super) fn reconcile_mask(&mut self, before: u64, source: EffectSource) {
        let after = self.mask();
        let mut added = after & !before;
        while added != 0 {
            let index = added.trailing_zeros() as usize;
            let card = CardId::new(index);
            if self.sources(card).is_empty() {
                self.sources.entry(card).or_default().push(source);
            }
            added &= added - 1;
        }
        let mut removed = before & !after;
        while removed != 0 {
            let index = removed.trailing_zeros() as usize;
            self.sources.remove(&CardId::new(index));
            removed &= removed - 1;
        }
    }

    pub(super) fn materialized(&self) -> &CardSet {
        &self.materialized
    }

    pub(super) fn retractions(&self) -> &[FactRetraction] {
        &self.retractions
    }

    pub(super) fn validate(&self) -> Result<(), String> {
        if let Some(card) = self
            .materialized
            .iter()
            .find(|card| self.sources(**card).is_empty())
        {
            return Err(format!(
                "materialized convention fact {card:?} has no source"
            ));
        }
        if let Some(card) = self
            .sources
            .keys()
            .find(|card| !self.materialized.contains(card))
        {
            return Err(format!(
                "provenance exists for non-materialized card {card:?}"
            ));
        }
        Ok(())
    }
}

/// Exact compact snapshot of every provenance-backed materialized card set.
/// Standard Hanabi has only 50 card IDs, so each domain fits in one `u64`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ConventionCardSetSnapshot {
    pub(super) explicitly_clued: u64,
    pub(super) invisibly_clued: u64,
    pub(super) already_playing: u64,
    pub(super) chop_moved: u64,
    pub(super) forced_playable: u64,
}

impl ConventionCardSetSnapshot {
    pub(super) fn capture(
        explicitly_clued: &ProvenancedCardSet,
        invisibly_clued: &ProvenancedCardSet,
        already_playing: &ProvenancedCardSet,
        chop_moved: &ProvenancedCardSet,
        forced_playable: &ProvenancedCardSet,
    ) -> Self {
        Self {
            explicitly_clued: explicitly_clued.mask(),
            invisibly_clued: invisibly_clued.mask(),
            already_playing: already_playing.mask(),
            chop_moved: chop_moved.mask(),
            forced_playable: forced_playable.mask(),
        }
    }

    pub(super) fn changes_to(&self, after: &Self) -> Vec<CardFactChange> {
        let mut changes = Vec::new();
        append_card_set_changes(
            &mut changes,
            MaterializedCardFact::ExplicitlyClued,
            self.explicitly_clued,
            after.explicitly_clued,
        );
        append_card_set_changes(
            &mut changes,
            MaterializedCardFact::InvisiblyClued,
            self.invisibly_clued,
            after.invisibly_clued,
        );
        append_card_set_changes(
            &mut changes,
            MaterializedCardFact::AlreadyPlaying,
            self.already_playing,
            after.already_playing,
        );
        append_card_set_changes(
            &mut changes,
            MaterializedCardFact::ChopMoved,
            self.chop_moved,
            after.chop_moved,
        );
        append_card_set_changes(
            &mut changes,
            MaterializedCardFact::ForcedPlayable,
            self.forced_playable,
            after.forced_playable,
        );
        changes.sort_unstable_by_key(|change| {
            (change.fact as u8, change.card.index(), change.kind as u8)
        });
        changes
    }
}

fn append_card_set_changes(
    changes: &mut Vec<CardFactChange>,
    fact: MaterializedCardFact,
    before: u64,
    after: u64,
) {
    let append = |changes: &mut Vec<CardFactChange>, mut bits: u64, kind| {
        while bits != 0 {
            let index = bits.trailing_zeros() as usize;
            changes.push(CardFactChange {
                fact,
                card: CardId::new(index),
                kind,
            });
            bits &= bits - 1;
        }
    };
    append(changes, before & !after, FactChangeKind::Removed);
    append(changes, after & !before, FactChangeKind::Added);
}

/// Commits all delayed-connection consequences at the same lifecycle
/// boundary as the `ConnectionManager` transition that owns them.
pub(super) fn reconcile_connection_fact_lifecycles(
    pending: &ConnectionManager,
    transition_start: usize,
    invisibly_clued: &mut ProvenancedCardSet,
    already_playing: &mut ProvenancedCardSet,
    forced_playable: &mut ProvenancedCardSet,
) {
    for transition in &pending.transitions()[transition_start..] {
        let source = EffectSource::Promise(transition.promise);
        if transition.reason == ConnectionTransitionReason::Scheduled {
            if let Some(connection) = pending
                .iter()
                .find(|connection| connection.promise == transition.promise)
            {
                if connection.kind == HGroupConnectionKind::Finesse {
                    for card in &connection.cards {
                        if invisibly_clued.contains(card) {
                            invisibly_clued.insert_from(source, *card);
                        }
                    }
                }
                if already_playing.contains(&connection.focus) {
                    already_playing.insert_from(source, connection.focus);
                }
                for card in &connection.cards {
                    if forced_playable.contains(card) {
                        forced_playable.insert_from(source, *card);
                    }
                }
            }
        }
        if transition.to != ConnectionStatus::Pending {
            invisibly_clued.retract_source(source, transition.reason);
            already_playing.retract_source(source, transition.reason);
            forced_playable.retract_source(source, transition.reason);
        }
    }
}

impl Deref for ProvenancedCardSet {
    type Target = CardSet;

    fn deref(&self) -> &Self::Target {
        &self.materialized
    }
}

impl DerefMut for ProvenancedCardSet {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.materialized
    }
}

impl<'a> IntoIterator for &'a ProvenancedCardSet {
    type Item = &'a CardId;
    type IntoIter = std::collections::hash_set::Iter<'a, CardId>;

    fn into_iter(self) -> Self::IntoIter {
        self.materialized.iter()
    }
}

impl Extend<CardId> for ProvenancedCardSet {
    fn extend<T: IntoIterator<Item = CardId>>(&mut self, iter: T) {
        // Recognition mutates only a transaction's materialized working set.
        // The rule/event reducer attaches provenance when the transaction is
        // committed; assigning a fake source here would hide missed commits.
        self.materialized.extend(iter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retracting_one_source_preserves_an_independently_supported_fact() {
        let card = CardId::new(3);
        let mut facts = ProvenancedCardSet::default();
        facts.insert_from(EffectSource::Event(1), card);
        facts.insert_from(EffectSource::Promise(PromiseId::from_raw(7)), card);

        assert!(
            facts
                .retract_source(
                    EffectSource::Promise(PromiseId::from_raw(7)),
                    ConnectionTransitionReason::Superseded,
                )
                .is_empty()
        );
        assert!(facts.contains(&card));
        assert_eq!(facts.sources(card), [EffectSource::Event(1)]);
    }

    #[test]
    fn unsourced_materialized_fact_is_rejected_until_reducer_commit() {
        let card = CardId::new(4);
        let mut facts = ProvenancedCardSet::default();
        facts.materialized.insert(card);
        assert!(facts.validate().is_err());

        facts.reconcile_mask(0, EffectSource::Event(2));
        assert_eq!(facts.validate(), Ok(()));
        assert_eq!(facts.sources(card), [EffectSource::Event(2)]);
    }
}
