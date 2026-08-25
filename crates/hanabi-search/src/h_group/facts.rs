use hanabi_core::{Card, CardId, PlayerId};

use super::{CardSet, HGroupMoveKind, HGroupSignal, IdentitySet};

/// How a convention identity claim applies to its referenced cards.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum IdentityClaimRelation {
    /// Every referenced card independently has this identity.
    Each,
    /// Exactly one referenced card has this identity, but the observer cannot
    /// yet determine which one.
    OneOf,
}

/// A normalized identity claim retained by current convention state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ConventionIdentityClaim {
    pub(super) turn: u32,
    pub(super) actor: PlayerId,
    pub(super) target: Option<PlayerId>,
    pub(super) source: HGroupMoveKind,
    pub(super) cards: Vec<CardId>,
    pub(super) identity: Card,
    pub(super) relation: IdentityClaimRelation,
}

/// Current convention facts derived from the append-only explanation log.
///
/// `HGroupSignal` answers "why did we infer this?"; this type answers "what is
/// true now?" Rules should query this index rather than searching old signals
/// and accidentally treating a cancelled interpretation as live state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ConventionFacts {
    fixed_cards: CardSet,
    known_identities: [Option<Card>; 50],
    known_conflicts: [bool; 50],
    excluded_identities: [IdentitySet; 50],
    demonstrated_layers: [Option<Card>; 50],
    layer_conflicts: [bool; 50],
    active_priority: CardSet,
    identity_claims: Vec<ConventionIdentityClaim>,
}

impl Default for ConventionFacts {
    fn default() -> Self {
        Self {
            fixed_cards: CardSet::default(),
            known_identities: [None; 50],
            known_conflicts: [false; 50],
            excluded_identities: [IdentitySet::default(); 50],
            demonstrated_layers: [None; 50],
            layer_conflicts: [false; 50],
            active_priority: CardSet::default(),
            identity_claims: Vec::new(),
        }
    }
}

impl ConventionFacts {
    #[cfg(test)]
    pub(super) fn from_signals(signals: &[HGroupSignal]) -> Self {
        let mut facts = Self::default();
        for signal in signals {
            facts.apply_signal(signal);
        }
        facts
    }

    /// Incrementally materializes one explanation signal into current facts.
    #[cfg(test)]
    pub(super) fn apply_signal(&mut self, signal: &HGroupSignal) {
        match signal.kind {
            HGroupMoveKind::FixClue => {
                self.set_fixed(&signal.cards, true);
            }
            HGroupMoveKind::PlayClue => {
                self.set_fixed(&signal.cards, false);
            }
            HGroupMoveKind::Priority => {
                self.set_priority(&signal.cards, true);
            }
            HGroupMoveKind::Retraction => {
                self.set_priority(&signal.cards, false);
            }
            _ => {}
        }

        self.apply_identity_effect(signal);
    }

    pub(super) fn set_fixed(&mut self, cards: &[CardId], fixed: bool) {
        for card in cards {
            if fixed {
                self.fixed_cards.insert(*card);
            } else {
                self.fixed_cards.remove(card);
            }
        }
    }

    pub(super) fn set_priority(&mut self, cards: &[CardId], active: bool) {
        for card in cards {
            if active {
                self.active_priority.insert(*card);
            } else {
                self.active_priority.remove(card);
            }
        }
    }

    pub(super) fn apply_identity_effect(&mut self, signal: &HGroupSignal) {
        let Some(identity) = signal.identity else {
            return;
        };
        if signal.kind == HGroupMoveKind::Retraction {
            let retracted = IdentitySet::singleton(identity);
            for card in &signal.cards {
                self.excluded_identities[card.index()] =
                    self.excluded_identities[card.index()].union(retracted);
            }
            self.identity_claims.retain(|claim| {
                claim.identity != identity
                    || !claim.cards.iter().any(|card| signal.cards.contains(card))
            });
            for card in &signal.cards {
                self.rebuild_known_identity(*card);
            }
            return;
        }
        if signal.kind == HGroupMoveKind::EliminationRewrite {
            // https://hanabi.github.io/extras/miscellaneous/#the-elimination-rewrite-for-1s
            // The second discarded copy of a playable 1 invalidates the old
            // positional OneOf claim before establishing a fresh claim over
            // the cards that remain after the second discard.
            self.identity_claims.retain(|claim| {
                !(claim.target == signal.target
                    && claim.identity == identity
                    && matches!(
                        claim.source,
                        HGroupMoveKind::Elimination | HGroupMoveKind::EliminationRewrite
                    ))
            });
        }
        let relation = if matches!(
            signal.kind,
            HGroupMoveKind::Elimination
                | HGroupMoveKind::EliminationFinesse
                | HGroupMoveKind::EliminationRewrite
        ) {
            IdentityClaimRelation::OneOf
        } else {
            IdentityClaimRelation::Each
        };
        let claim = ConventionIdentityClaim {
            turn: signal.turn,
            actor: signal.actor,
            target: signal.target,
            source: signal.kind,
            cards: signal.cards.clone(),
            identity,
            relation,
        };
        if !self.identity_claims.contains(&claim) {
            self.identity_claims.push(claim);
        }
        if relation == IdentityClaimRelation::OneOf {
            return;
        }
        for card in &signal.cards {
            self.excluded_identities[card.index()] =
                self.excluded_identities[card.index()].without(IdentitySet::singleton(identity));
            merge_identity(
                &mut self.known_identities[card.index()],
                &mut self.known_conflicts[card.index()],
                identity,
            );
        }
        if signal.kind == HGroupMoveKind::LayeredFinesse {
            for card in signal.cards.iter().skip(1) {
                merge_identity(
                    &mut self.demonstrated_layers[card.index()],
                    &mut self.layer_conflicts[card.index()],
                    identity,
                );
            }
        }
    }

    pub(super) const fn fixed_cards(&self) -> &CardSet {
        &self.fixed_cards
    }

    pub(super) fn known_identity(&self, card: CardId) -> Option<Card> {
        self.known_identities[card.index()]
    }

    pub(super) const fn excluded_identities(&self, card: CardId) -> IdentitySet {
        self.excluded_identities[card.index()]
    }

    pub(super) fn demonstrated_layer(&self, card: CardId) -> Option<Card> {
        self.demonstrated_layers[card.index()]
    }

    pub(super) const fn active_priority(&self) -> &CardSet {
        &self.active_priority
    }

    pub(super) fn identity_claims(&self) -> &[ConventionIdentityClaim] {
        &self.identity_claims
    }

    pub(super) fn validate(&self) -> Result<(), String> {
        if self
            .identity_claims()
            .iter()
            .any(|claim| claim.cards.is_empty())
        {
            return Err("convention identity claim has no candidate cards".to_owned());
        }
        Ok(())
    }

    fn rebuild_known_identity(&mut self, card: CardId) {
        self.known_identities[card.index()] = None;
        self.known_conflicts[card.index()] = false;
        for claim in self.identity_claims.iter().filter(|claim| {
            claim.relation == IdentityClaimRelation::Each && claim.cards.contains(&card)
        }) {
            merge_identity(
                &mut self.known_identities[card.index()],
                &mut self.known_conflicts[card.index()],
                claim.identity,
            );
        }
    }
}

fn merge_identity(slot: &mut Option<Card>, conflict: &mut bool, identity: Card) {
    if *conflict {
        return;
    }
    match *slot {
        None => *slot = Some(identity),
        Some(existing) if existing == identity => {}
        Some(_) => {
            *slot = None;
            *conflict = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use hanabi_core::{PlayerId, Rank, Suit};

    use super::*;

    fn signal(kind: HGroupMoveKind, card: CardId, identity: Option<Card>) -> HGroupSignal {
        HGroupSignal {
            turn: 1,
            actor: PlayerId::new(0),
            target: None,
            kind,
            cards: vec![card],
            identity,
        }
    }

    #[test]
    fn current_facts_are_distinct_from_signal_history() {
        let card = CardId::new(3);
        let red_one = Card::new(Suit::Red, Rank::One);
        let signals = [
            signal(HGroupMoveKind::FixClue, card, Some(red_one)),
            signal(HGroupMoveKind::PlayClue, card, Some(red_one)),
        ];
        let facts = ConventionFacts::from_signals(&signals);
        assert!(!facts.fixed_cards().contains(&card));
        assert_eq!(facts.known_identity(card), Some(red_one));
    }

    #[test]
    fn elimination_identity_is_retained_as_a_disjunction() {
        let red_one = Card::new(Suit::Red, Rank::One);
        let mut elimination = signal(
            HGroupMoveKind::EliminationFinesse,
            CardId::new(3),
            Some(red_one),
        );
        elimination.cards.push(CardId::new(4));
        let facts = ConventionFacts::from_signals(&[elimination]);

        assert_eq!(facts.known_identity(CardId::new(3)), None);
        assert_eq!(facts.known_identity(CardId::new(4)), None);
        assert_eq!(
            facts.identity_claims(),
            &[ConventionIdentityClaim {
                turn: 1,
                actor: PlayerId::new(0),
                target: None,
                source: HGroupMoveKind::EliminationFinesse,
                cards: vec![CardId::new(3), CardId::new(4)],
                identity: red_one,
                relation: IdentityClaimRelation::OneOf,
            }]
        );
    }

    #[test]
    fn retraction_turns_a_disproved_identity_into_negative_knowledge() {
        let card = CardId::new(3);
        let purple_two = Card::new(Suit::Purple, Rank::Two);
        let facts = ConventionFacts::from_signals(&[
            signal(HGroupMoveKind::Prompt, card, Some(purple_two)),
            signal(HGroupMoveKind::Retraction, card, Some(purple_two)),
        ]);

        assert_eq!(facts.known_identity(card), None);
        assert!(facts.excluded_identities(card).contains(purple_two));
        assert!(facts.identity_claims().is_empty());
    }
}
