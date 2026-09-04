use hanabi_core::{Card, CardId, PlayerId};

use super::IdentitySet;

/// One future action established by a clue under a particular observer's
/// knowledge. A singleton identity domain is deterministic; a larger domain
/// records a promise without pretending that the owner knows its identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ActionCommitment {
    pub(super) card: CardId,
    pub(super) owner: PlayerId,
    pub(super) identities: IdentitySet,
}

/// The complete identity superposition that a card's owner retains after a
/// clue line. Directness may compare two lines only when these domains agree
/// for every explicitly or invisibly clued card.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CluedCardSuperposition {
    pub(super) card: CardId,
    pub(super) owner: PlayerId,
    pub(super) identities: IdentitySet,
}

/// What a clue causes one recipient to do with a card. This is deliberately
/// behavioral: convention principles such as Good Touch care whether a player
/// will try to play a duplicate, not merely whether two physical cards share
/// an identity in the giver's view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RecipientCardDisposition {
    PlayNow,
    PlayAfterConnection,
    KnownTrash,
    Protected,
}

/// One causal, owner-relative consequence of a compiled clue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RecipientCardConsequence {
    pub(super) card: CardId,
    pub(super) owner: PlayerId,
    pub(super) identities: IdentitySet,
    pub(super) disposition: RecipientCardDisposition,
}

impl ActionCommitment {
    pub(super) const fn exact(card: CardId, owner: PlayerId, identity: Card) -> Self {
        Self {
            card,
            owner,
            identities: IdentitySet::singleton(identity),
        }
    }
}

/// Structured semantic result of a clue line. Strategic principles compare
/// this object before converting genuine preferences to numeric ordering.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct LineOutcome {
    /// Publicly secured actions, used for team coverage and tempo.
    pub(super) public_actions: Vec<ActionCommitment>,
    /// Actions known by each card's owner, used for Directness equivalence.
    pub(super) owner_actions: Vec<ActionCommitment>,
    /// Owner-visible identity domains for every clued card after the line.
    pub(super) clued_superpositions: Vec<CluedCardSuperposition>,
    pub(super) protected_cards: Vec<CardId>,
    pub(super) known_trash: Vec<CardId>,
    /// Canonical behavioral consequences. Aggregate metrics are derived from
    /// this collection instead of separately reinterpreting card identities.
    pub(super) recipient_consequences: Vec<RecipientCardConsequence>,
    pub(super) new_connections: usize,
    /// Number of actions in the line as interpreted by the clue recipient.
    /// Other observer projections remain useful for owner knowledge, but must
    /// not inflate Teamwork coverage with mutually incompatible readings.
    pub(super) action_coverage: usize,
    /// Total cards secured by the canonical named convention line.
    pub(super) convention_action_count: Option<usize>,
    /// Blind-play steps established by the canonical named interpretation.
    /// This differs from a focus card's raw distance from its stack when a
    /// Layered or Clandestine Finesse crosses suits.
    pub(super) convention_connection_steps: Option<usize>,
}

impl LineOutcome {
    pub(super) fn play_consequences(&self) -> impl Iterator<Item = &RecipientCardConsequence> {
        self.recipient_consequences.iter().filter(|consequence| {
            matches!(
                consequence.disposition,
                RecipientCardDisposition::PlayNow | RecipientCardDisposition::PlayAfterConnection
            )
        })
    }

    pub(super) fn protects(&self, card: CardId) -> bool {
        self.recipient_consequences.iter().any(|consequence| {
            consequence.card == card
                && consequence.disposition == RecipientCardDisposition::Protected
        })
    }

    pub(super) fn normalize(&mut self) {
        let key =
            |commitment: &ActionCommitment| (commitment.card.index(), commitment.owner.index());
        self.public_actions.sort_unstable_by_key(key);
        self.public_actions.dedup();
        self.owner_actions.sort_unstable_by_key(key);
        self.owner_actions.dedup();
        self.clued_superpositions
            .sort_unstable_by_key(|knowledge| (knowledge.card.index(), knowledge.owner.index()));
        self.clued_superpositions.dedup();
        self.protected_cards
            .sort_unstable_by_key(|card| card.index());
        self.protected_cards.dedup();
        self.known_trash.sort_unstable_by_key(|card| card.index());
        self.known_trash.dedup();
        self.recipient_consequences
            .sort_unstable_by_key(|consequence| {
                (
                    consequence.owner.index(),
                    consequence.card.index(),
                    consequence.disposition as u8,
                )
            });
        self.recipient_consequences.dedup();
    }

    pub(super) fn covered_players(&self) -> usize {
        let mut players = self
            .play_consequences()
            .map(|consequence| consequence.owner)
            .collect::<Vec<_>>();
        players.sort_unstable_by_key(|player| player.index());
        players.dedup();
        players.len()
    }

    pub(super) fn first_action_distance(&self, current: PlayerId, player_count: usize) -> usize {
        self.play_consequences()
            .map(|consequence| {
                let distance =
                    (consequence.owner.index() + player_count - current.index()) % player_count;
                if distance == 0 {
                    player_count
                } else {
                    distance
                }
            })
            .min()
            .unwrap_or(player_count)
    }

    pub(super) fn has_same_direct_outcome(&self, other: &Self) -> bool {
        self.owner_actions == other.owner_actions
            && self.clued_superpositions == other.clued_superpositions
    }
}

#[cfg(test)]
mod tests {
    use hanabi_core::{Rank, Suit};

    use super::*;

    #[test]
    fn directness_requires_identical_clued_card_superpositions() {
        let card = CardId::new(3);
        let owner = PlayerId::new(1);
        let action = ActionCommitment::exact(card, owner, Card::new(Suit::Red, Rank::Three));
        let mut direct = LineOutcome {
            owner_actions: vec![action],
            clued_superpositions: vec![CluedCardSuperposition {
                card,
                owner,
                identities: IdentitySet::singleton(Card::new(Suit::Red, Rank::Three)),
            }],
            ..LineOutcome::default()
        };
        direct.normalize();
        let mut ambiguous = direct.clone();
        ambiguous.clued_superpositions[0].identities =
            IdentitySet::singleton(Card::new(Suit::Red, Rank::Three))
                .union(IdentitySet::singleton(Card::new(Suit::Red, Rank::Four)));

        assert!(!direct.has_same_direct_outcome(&ambiguous));
        assert!(direct.has_same_direct_outcome(&direct.clone()));
    }

    #[test]
    fn behavioral_consequences_drive_team_coverage() {
        let playing = PlayerId::new(1);
        let protected = PlayerId::new(2);
        let outcome = LineOutcome {
            recipient_consequences: vec![
                RecipientCardConsequence {
                    card: CardId::new(1),
                    owner: playing,
                    identities: IdentitySet::singleton(Card::new(Suit::Red, Rank::One)),
                    disposition: RecipientCardDisposition::PlayNow,
                },
                RecipientCardConsequence {
                    card: CardId::new(2),
                    owner: protected,
                    identities: IdentitySet::singleton(Card::new(Suit::Blue, Rank::Five)),
                    disposition: RecipientCardDisposition::Protected,
                },
            ],
            ..LineOutcome::default()
        };

        assert_eq!(outcome.covered_players(), 1);
        assert_eq!(outcome.first_action_distance(PlayerId::new(0), 4), 1);
        assert!(outcome.protects(CardId::new(2)));
    }
}
