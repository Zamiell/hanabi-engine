//! Ordered future-card comparisons, not point bonuses.
//! User-reviewed p4v0s1 opening strategy, September 15, 2026:
//! lower ranks and fewer missing predecessors are preferable; a near 4 with
//! its visible 5 is preferable to a distant 3.

use core::{
    cmp::{Ordering, Reverse},
    fmt,
};
use hanabi_core::Rank;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FutureCardQuality {
    pub rank: Rank,
    pub missing_predecessors: u8,
    pub visible_successor: bool,
}

// Ascending strategic order. The explicit middle band encodes the reviewed
// visible-5 comparison without assigning arbitrary numeric weights.
#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
enum QualityBand {
    Five,
    Four,
    DistantThree,
    NearFourWithVisibleFive,
    NearThree,
    Two,
    One,
}

impl FutureCardQuality {
    fn band(self) -> QualityBand {
        match self.rank {
            Rank::One => QualityBand::One,
            Rank::Two => QualityBand::Two,
            Rank::Three if self.missing_predecessors <= 1 => QualityBand::NearThree,
            Rank::Three => QualityBand::DistantThree,
            Rank::Four if self.missing_predecessors <= 1 && self.visible_successor => {
                QualityBand::NearFourWithVisibleFive
            }
            Rank::Four => QualityBand::Four,
            Rank::Five => QualityBand::Five,
        }
    }
}

impl Ord for FutureCardQuality {
    fn cmp(&self, other: &Self) -> Ordering {
        (
            self.band(),
            Reverse(self.missing_predecessors),
            self.visible_successor,
        )
            .cmp(&(
                other.band(),
                Reverse(other.missing_predecessors),
                other.visible_successor,
            ))
    }
}

impl PartialOrd for FutureCardQuality {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Distinct secured identities, ordered best first. Comparing matching cards
/// prevents many weak promises from being summed into one strong promise.
#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub struct SecuredCardQuality {
    cards: [Option<FutureCardQuality>; 25],
}

impl fmt::Debug for SecuredCardQuality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.cards.iter().flatten()).finish()
    }
}

impl SecuredCardQuality {
    pub(crate) fn from_cards(cards: impl IntoIterator<Item = FutureCardQuality>) -> Self {
        let mut result = Self::default();
        for (index, card) in cards.into_iter().enumerate() {
            *result
                .cards
                .get_mut(index)
                .expect("at most 25 distinct secured identities") = Some(card);
        }
        result.cards.sort_unstable_by(|a, b| b.cmp(a));
        result
    }

    pub(crate) fn no_worse_than(self, other: Self) -> bool {
        self.cards
            .iter()
            .zip(other.cards.iter())
            .all(|(a, b)| a >= b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(rank: Rank, missing_predecessors: u8, visible_successor: bool) -> FutureCardQuality {
        FutureCardQuality {
            rank,
            missing_predecessors,
            visible_successor,
        }
    }

    #[test]
    fn reviewed_rank_distance_and_visible_five_order() {
        let near_three = card(Rank::Three, 1, false);
        let distant_three = card(Rank::Three, 2, false);
        let near_four = card(Rank::Four, 1, false);
        let four_with_five = card(Rank::Four, 1, true);
        assert!(near_three > distant_three);
        assert!(distant_three > near_four);
        assert!(four_with_five > distant_three);
        assert!(near_three > four_with_five);
        assert!(near_four > card(Rank::Four, 2, false));
        assert!(card(Rank::Three, 1, true) > near_three);
        assert!(card(Rank::Four, 0, true) > four_with_five);
    }

    #[test]
    fn reviewed_two_threes_are_stronger_than_two_fours() {
        let threes = SecuredCardQuality::from_cards([
            card(Rank::Three, 1, true),
            card(Rank::Three, 2, false),
        ]);
        let fours = SecuredCardQuality::from_cards([
            card(Rank::Four, 1, false),
            card(Rank::Four, 3, false),
        ]);
        assert!(threes.no_worse_than(fours));
        assert!(!fours.no_worse_than(threes));
    }
}
