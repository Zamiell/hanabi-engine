use crate::{Card, CardId, PlayerId, Rank, Suit};

/// A standard color or rank clue.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Clue {
    Suit(Suit),
    Rank(Rank),
}

impl Clue {
    #[must_use]
    pub fn matches(self, card: Card) -> bool {
        match self {
            Self::Suit(suit) => card.suit == suit,
            Self::Rank(rank) => card.rank == rank,
        }
    }
}

/// An action selected by the current player.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Action {
    Play(CardId),
    Discard(CardId),
    Clue { target: PlayerId, clue: Clue },
}
