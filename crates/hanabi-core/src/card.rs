use core::fmt;

/// A suit in standard five-color Hanabi.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum Suit {
    Red,
    Yellow,
    Green,
    Blue,
    Purple,
}

impl Suit {
    pub const ALL: [Self; 5] = [
        Self::Red,
        Self::Yellow,
        Self::Green,
        Self::Blue,
        Self::Purple,
    ];

    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

impl fmt::Display for Suit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Red => "red",
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Blue => "blue",
            Self::Purple => "purple",
        };
        formatter.write_str(name)
    }
}

/// A rank in standard Hanabi.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum Rank {
    One = 1,
    Two = 2,
    Three = 3,
    Four = 4,
    Five = 5,
}

impl Rank {
    pub const ALL: [Self; 5] = [Self::One, Self::Two, Self::Three, Self::Four, Self::Five];

    #[must_use]
    pub const fn number(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub const fn index(self) -> usize {
        self.number() as usize - 1
    }

    #[must_use]
    pub const fn copies(self) -> u8 {
        match self {
            Self::One => 3,
            Self::Two | Self::Three | Self::Four => 2,
            Self::Five => 1,
        }
    }
}

impl fmt::Display for Rank {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.number())
    }
}

/// The identity of a physical card.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Card {
    pub suit: Suit,
    pub rank: Rank,
}

impl Card {
    #[must_use]
    pub const fn new(suit: Suit, rank: Rank) -> Self {
        Self { suit, rank }
    }

    /// Stable index in the 25 standard suit-rank identities.
    #[must_use]
    pub const fn index(self) -> usize {
        self.suit.index() * 5 + self.rank.index()
    }
}

impl fmt::Display for Card {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} {}", self.suit, self.rank)
    }
}

/// Returns the 50 cards in a standard five-color deck.
///
/// The order is stable but intentionally unshuffled. A caller controls the
/// order supplied to [`crate::FullState::new_standard`], which makes tests and
/// exact world enumeration reproducible.
#[must_use]
pub fn standard_deck() -> Vec<Card> {
    let mut deck = Vec::with_capacity(50);
    for suit in Suit::ALL {
        for rank in Rank::ALL {
            deck.extend(core::iter::repeat_n(
                Card::new(suit, rank),
                rank.copies().into(),
            ));
        }
    }
    deck
}
