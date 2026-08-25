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
    pub(super) protected_cards: Vec<CardId>,
    pub(super) known_trash: Vec<CardId>,
    pub(super) new_connections: usize,
}

impl LineOutcome {
    pub(super) fn normalize(&mut self) {
        let key =
            |commitment: &ActionCommitment| (commitment.card.index(), commitment.owner.index());
        self.public_actions.sort_unstable_by_key(key);
        self.public_actions.dedup();
        self.owner_actions.sort_unstable_by_key(key);
        self.owner_actions.dedup();
        self.protected_cards
            .sort_unstable_by_key(|card| card.index());
        self.protected_cards.dedup();
        self.known_trash.sort_unstable_by_key(|card| card.index());
        self.known_trash.dedup();
    }

    pub(super) fn covered_players(&self) -> usize {
        let mut players = self
            .public_actions
            .iter()
            .map(|commitment| commitment.owner)
            .collect::<Vec<_>>();
        players.sort_unstable_by_key(|player| player.index());
        players.dedup();
        players.len()
    }

    pub(super) fn protected_card_count(&self) -> usize {
        self.protected_cards.len()
    }

    pub(super) fn first_action_distance(&self, current: PlayerId, player_count: usize) -> usize {
        self.public_actions
            .iter()
            .map(|commitment| {
                let distance =
                    (commitment.owner.index() + player_count - current.index()) % player_count;
                if distance == 0 {
                    player_count
                } else {
                    distance
                }
            })
            .min()
            .unwrap_or(player_count)
    }

    pub(super) fn directness_key(&self) -> Vec<(CardId, IdentitySet, PlayerId)> {
        self.owner_actions
            .iter()
            .map(|commitment| (commitment.card, commitment.identities, commitment.owner))
            .collect()
    }
}
