use hanabi_core::{Card, CardId};

use super::{CardSet, HGroupMoveKind, HGroupSignal};

/// Current convention facts derived from the append-only explanation log.
///
/// `HGroupSignal` answers "why did we infer this?"; this type answers "what is
/// true now?" Rules should query this index rather than searching old signals
/// and accidentally treating a cancelled interpretation as live state.
#[derive(Clone, Debug)]
pub(super) struct ConventionFacts {
    fixed_cards: CardSet,
    known_identities: [Option<Card>; 50],
    demonstrated_layers: [Option<Card>; 50],
    active_priority: CardSet,
}

impl Default for ConventionFacts {
    fn default() -> Self {
        Self {
            fixed_cards: CardSet::default(),
            known_identities: [None; 50],
            demonstrated_layers: [None; 50],
            active_priority: CardSet::default(),
        }
    }
}

impl ConventionFacts {
    pub(super) fn from_signals(signals: &[HGroupSignal]) -> Self {
        let mut facts = Self::default();
        let mut known_conflicts = [false; 50];
        let mut layer_conflicts = [false; 50];
        for signal in signals {
            match signal.kind {
                HGroupMoveKind::FixClue => {
                    facts.fixed_cards.extend(signal.cards.iter().copied());
                }
                HGroupMoveKind::PlayClue => {
                    for card in &signal.cards {
                        facts.fixed_cards.remove(card);
                    }
                }
                HGroupMoveKind::Priority => {
                    facts.active_priority.extend(signal.cards.iter().copied());
                }
                HGroupMoveKind::Retraction => {
                    for card in &signal.cards {
                        facts.active_priority.remove(card);
                    }
                }
                _ => {}
            }
            if let Some(identity) = signal.identity {
                for card in &signal.cards {
                    merge_identity(
                        &mut facts.known_identities[card.index()],
                        &mut known_conflicts[card.index()],
                        identity,
                    );
                }
                if signal.kind == HGroupMoveKind::LayeredFinesse {
                    for card in signal.cards.iter().skip(1) {
                        merge_identity(
                            &mut facts.demonstrated_layers[card.index()],
                            &mut layer_conflicts[card.index()],
                            identity,
                        );
                    }
                }
            }
        }
        facts
    }

    pub(super) const fn fixed_cards(&self) -> &CardSet {
        &self.fixed_cards
    }

    pub(super) fn known_identity(&self, card: CardId) -> Option<Card> {
        self.known_identities[card.index()]
    }

    pub(super) fn demonstrated_layer(&self, card: CardId) -> Option<Card> {
        self.demonstrated_layers[card.index()]
    }

    pub(super) const fn active_priority(&self) -> &CardSet {
        &self.active_priority
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
}
