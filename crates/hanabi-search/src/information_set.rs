use core::fmt;
use std::collections::HashMap;
use std::sync::OnceLock;

use hanabi_core::{
    Card, CardId, ClueFacts, DeterminizationError, FullState, PlayerView, Rank, Suit,
};
use rand::{Rng, RngExt, SeedableRng, rngs::StdRng, seq::SliceRandom};

const CARD_IDENTITY_COUNT: usize = 25;
const STANDARD_CARD_COUNT: usize = 50;
type Counts = [u8; CARD_IDENTITY_COUNT];

/// The hidden worlds logically consistent with one [`PlayerView`].
///
/// This first model uses public card counts and direct positive/negative clues.
/// Convention-dependent weighting will be layered on top rather than changing
/// which worlds are logically possible.
#[derive(Debug)]
pub struct InformationSet {
    view: PlayerView,
    known_identities: Vec<Option<Card>>,
    unknown_hand_cards: Vec<CardId>,
    constraints: Vec<ClueFacts>,
    deck_cards: Vec<CardId>,
    remaining_counts: Counts,
    possibilities: Vec<(CardId, Vec<Card>)>,
    completion_cache: OnceLock<CompletionCache>,
}

#[derive(Clone, Debug)]
struct CompletionCache {
    hand_assignment_count: u64,
    completion_memo: HashMap<(usize, Counts), u64>,
}

impl Clone for InformationSet {
    fn clone(&self) -> Self {
        Self {
            view: self.view.clone(),
            known_identities: self.known_identities.clone(),
            unknown_hand_cards: self.unknown_hand_cards.clone(),
            constraints: self.constraints.clone(),
            deck_cards: self.deck_cards.clone(),
            remaining_counts: self.remaining_counts,
            possibilities: self.possibilities.clone(),
            completion_cache: OnceLock::new(),
        }
    }
}

impl PartialEq for InformationSet {
    fn eq(&self, other: &Self) -> bool {
        self.view == other.view
            && self.known_identities == other.known_identities
            && self.unknown_hand_cards == other.unknown_hand_cards
            && self.constraints == other.constraints
            && self.deck_cards == other.deck_cards
            && self.remaining_counts == other.remaining_counts
            && self.possibilities == other.possibilities
    }
}

impl Eq for InformationSet {}

impl InformationSet {
    /// Builds card-count and direct-clue constraints from a legal observation.
    ///
    /// # Errors
    ///
    /// Returns [`InformationSetError`] when card locations or visible
    /// identities are inconsistent, or when no hidden world satisfies all
    /// direct clues.
    pub fn new(view: PlayerView) -> Result<Self, InformationSetError> {
        if view.observer.index() >= view.hands.len() {
            return Err(InformationSetError::InvalidObserver);
        }

        let mut occupied = [false; STANDARD_CARD_COUNT];
        let mut known_identities = vec![None; STANDARD_CARD_COUNT];
        let mut unknown_hand_cards = Vec::new();
        let mut constraints = Vec::new();

        for hand in &view.hands {
            for observed in hand {
                mark_location(&mut occupied, observed.id)?;
                if let Some(identity) = observed.identity {
                    set_known_identity(&mut known_identities, observed.id, identity)?;
                } else {
                    unknown_hand_cards.push(observed.id);
                    constraints.push(observed.clues.clone());
                }
            }
        }
        for stack in &view.play_stacks {
            for (card, identity) in stack {
                mark_location(&mut occupied, *card)?;
                set_known_identity(&mut known_identities, *card, *identity)?;
            }
        }
        for (card, identity) in &view.discard_pile {
            mark_location(&mut occupied, *card)?;
            set_known_identity(&mut known_identities, *card, *identity)?;
        }

        let deck_cards = occupied
            .iter()
            .enumerate()
            .filter_map(|(index, is_occupied)| (!is_occupied).then_some(CardId::new(index)))
            .collect::<Vec<_>>();
        if deck_cards.len() != view.deck_size {
            return Err(InformationSetError::DeckSizeMismatch {
                observed: view.deck_size,
                inferred: deck_cards.len(),
            });
        }

        let mut remaining_counts = standard_counts();
        for identity in known_identities.iter().flatten().copied() {
            let count = &mut remaining_counts[identity_index(identity)];
            if *count == 0 {
                return Err(InformationSetError::TooManyVisibleCopies(identity));
            }
            *count -= 1;
        }

        let remaining_total: usize = remaining_counts
            .iter()
            .map(|count| usize::from(*count))
            .sum();
        let hidden_total = unknown_hand_cards.len() + deck_cards.len();
        if remaining_total != hidden_total {
            return Err(InformationSetError::HiddenCardCountMismatch {
                identities: remaining_total,
                locations: hidden_total,
            });
        }

        let mut feasibility_memo = HashMap::new();
        if !has_completion(&constraints, 0, remaining_counts, &mut feasibility_memo) {
            return Err(InformationSetError::NoConsistentWorld);
        }

        let feasible_by_slot = feasible_identities(&constraints, remaining_counts);
        let possibilities = unknown_hand_cards
            .iter()
            .copied()
            .zip(feasible_by_slot)
            .collect();

        Ok(Self {
            view,
            known_identities,
            unknown_hand_cards,
            constraints,
            deck_cards,
            remaining_counts,
            possibilities,
            completion_cache: OnceLock::new(),
        })
    }

    #[must_use]
    pub const fn view(&self) -> &PlayerView {
        &self.view
    }

    #[must_use]
    pub fn unknown_hand_cards(&self) -> &[CardId] {
        &self.unknown_hand_cards
    }

    /// Exact number of labeled card assignments satisfying all hidden-hand
    /// clue constraints, before the remaining deck is permuted. The exact
    /// counting table is built lazily because rollout policies need logical
    /// possibilities but do not need sampling weights.
    #[must_use]
    pub fn hand_assignment_count(&self) -> u64 {
        self.completion_cache().hand_assignment_count
    }

    #[must_use]
    pub fn possible_identities(&self, card: CardId) -> Option<&[Card]> {
        self.possibilities
            .iter()
            .find_map(|(candidate, identities)| {
                (*candidate == card).then_some(identities.as_slice())
            })
    }

    /// Samples an exact card-copy-weighted hidden hand, then uniformly shuffles
    /// the remaining identities into the deck's stable draw-order slots.
    ///
    /// # Errors
    ///
    /// Returns [`SampleError`] if reconstruction unexpectedly violates the
    /// source observation. A successfully constructed information set always
    /// has at least one assignment.
    pub fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Result<FullState, SampleError> {
        let completion_cache = self.completion_cache();
        let mut counts = self.remaining_counts;
        let mut identities = self.known_identities.clone();

        for (slot, card_id) in self.unknown_hand_cards.iter().copied().enumerate() {
            let mut candidates = Vec::new();
            let mut total_weight = 0_u64;
            for identity in all_identities() {
                let index = identity_index(identity);
                let copies = counts[index];
                if copies == 0 || !self.constraints[slot].allows(identity) {
                    continue;
                }
                counts[index] -= 1;
                let completions = cached_completions(
                    slot + 1,
                    self.constraints.len(),
                    counts,
                    &completion_cache.completion_memo,
                );
                counts[index] += 1;
                let weight = u64::from(copies) * completions;
                if weight > 0 {
                    candidates.push((identity, weight));
                    total_weight += weight;
                }
            }

            if total_weight == 0 {
                return Err(SampleError::NoConsistentWorld);
            }
            let mut draw = rng.random_range(0..total_weight);
            let Some(selected) = candidates.into_iter().find_map(|(identity, weight)| {
                if draw < weight {
                    Some(identity)
                } else {
                    draw -= weight;
                    None
                }
            }) else {
                return Err(SampleError::NoConsistentWorld);
            };
            counts[identity_index(selected)] -= 1;
            identities[card_id.index()] = Some(selected);
        }

        let mut deck_identities = Vec::with_capacity(self.deck_cards.len());
        for identity in all_identities() {
            deck_identities.extend(core::iter::repeat_n(
                identity,
                counts[identity_index(identity)].into(),
            ));
        }
        deck_identities.shuffle(rng);
        for (card_id, identity) in self.deck_cards.iter().zip(deck_identities) {
            identities[card_id.index()] = Some(identity);
        }

        let cards = identities
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or(SampleError::NoConsistentWorld)?;
        FullState::from_determinization(&self.view, cards).map_err(SampleError::Determinization)
    }

    /// Convenience wrapper for a reproducible sample.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::sample`].
    pub fn sample_seeded(&self, seed: u64) -> Result<FullState, SampleError> {
        self.sample(&mut StdRng::seed_from_u64(seed))
    }

    fn completion_cache(&self) -> &CompletionCache {
        self.completion_cache.get_or_init(|| {
            let mut counts = self.remaining_counts;
            let mut completion_memo = HashMap::new();
            let hand_assignment_count =
                count_completions(&self.constraints, 0, &mut counts, &mut completion_memo);
            CompletionCache {
                hand_assignment_count,
                completion_memo,
            }
        })
    }
}

fn mark_location(
    occupied: &mut [bool; STANDARD_CARD_COUNT],
    card: CardId,
) -> Result<(), InformationSetError> {
    let Some(slot) = occupied.get_mut(card.index()) else {
        return Err(InformationSetError::InvalidCardId(card));
    };
    if *slot {
        return Err(InformationSetError::DuplicateLocation(card));
    }
    *slot = true;
    Ok(())
}

fn set_known_identity(
    identities: &mut [Option<Card>],
    card: CardId,
    identity: Card,
) -> Result<(), InformationSetError> {
    let Some(slot) = identities.get_mut(card.index()) else {
        return Err(InformationSetError::InvalidCardId(card));
    };
    if let Some(existing) = slot {
        if *existing != identity {
            return Err(InformationSetError::ConflictingIdentity {
                card,
                first: *existing,
                second: identity,
            });
        }
    } else {
        *slot = Some(identity);
    }
    Ok(())
}

fn standard_counts() -> Counts {
    let mut counts = [0; CARD_IDENTITY_COUNT];
    for identity in all_identities() {
        counts[identity_index(identity)] = identity.rank.copies();
    }
    counts
}

fn all_identities() -> impl Iterator<Item = Card> {
    Suit::ALL
        .into_iter()
        .flat_map(|suit| Rank::ALL.into_iter().map(move |rank| Card::new(suit, rank)))
}

fn identity_index(card: Card) -> usize {
    card.suit.index() * 5 + card.rank.index()
}

fn count_completions(
    constraints: &[ClueFacts],
    slot: usize,
    counts: &mut Counts,
    memo: &mut HashMap<(usize, Counts), u64>,
) -> u64 {
    if slot == constraints.len() {
        return 1;
    }
    let key = (slot, *counts);
    if let Some(value) = memo.get(&key) {
        return *value;
    }

    let mut total = 0_u64;
    for identity in all_identities() {
        let index = identity_index(identity);
        let copies = counts[index];
        if copies == 0 || !constraints[slot].allows(identity) {
            continue;
        }
        counts[index] -= 1;
        total += u64::from(copies) * count_completions(constraints, slot + 1, counts, memo);
        counts[index] += 1;
    }
    memo.insert(key, total);
    total
}

fn cached_completions(
    slot: usize,
    constraint_count: usize,
    counts: Counts,
    memo: &HashMap<(usize, Counts), u64>,
) -> u64 {
    if slot == constraint_count {
        1
    } else {
        *memo
            .get(&(slot, counts))
            .expect("the exact completion pass memoizes every reachable state")
    }
}

fn has_completion(
    constraints: &[ClueFacts],
    slot: usize,
    counts: Counts,
    memo: &mut HashMap<(usize, Counts), bool>,
) -> bool {
    if slot == constraints.len() {
        return true;
    }
    let key = (slot, counts);
    if let Some(value) = memo.get(&key) {
        return *value;
    }

    let found = all_identities().any(|identity| {
        let index = identity_index(identity);
        if counts[index] == 0 || !constraints[slot].allows(identity) {
            return false;
        }
        let mut remaining = counts;
        remaining[index] -= 1;
        has_completion(constraints, slot + 1, remaining, memo)
    });
    memo.insert(key, found);
    found
}

fn feasible_identities(constraints: &[ClueFacts], counts: Counts) -> Vec<Vec<Card>> {
    constraints
        .iter()
        .enumerate()
        .map(|(slot, constraint)| {
            let other_constraints = constraints
                .iter()
                .enumerate()
                .filter_map(|(index, other)| (index != slot).then_some(other.clone()))
                .collect::<Vec<_>>();
            let mut memo = HashMap::new();

            all_identities()
                .filter(|identity| {
                    let index = identity_index(*identity);
                    if counts[index] == 0 || !constraint.allows(*identity) {
                        return false;
                    }
                    let mut remaining = counts;
                    remaining[index] -= 1;
                    has_completion(&other_constraints, 0, remaining, &mut memo)
                })
                .collect()
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InformationSetError {
    InvalidObserver,
    InvalidCardId(CardId),
    DuplicateLocation(CardId),
    ConflictingIdentity {
        card: CardId,
        first: Card,
        second: Card,
    },
    TooManyVisibleCopies(Card),
    DeckSizeMismatch {
        observed: usize,
        inferred: usize,
    },
    HiddenCardCountMismatch {
        identities: usize,
        locations: usize,
    },
    NoConsistentWorld,
}

impl fmt::Display for InformationSetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidObserver => formatter.write_str("observer does not have a hand"),
            Self::InvalidCardId(card) => write!(formatter, "invalid card identifier {card}"),
            Self::DuplicateLocation(card) => {
                write!(formatter, "card {card} occurs in multiple public locations")
            }
            Self::ConflictingIdentity {
                card,
                first,
                second,
            } => write!(
                formatter,
                "card {card} has conflicting visible identities {first} and {second}"
            ),
            Self::TooManyVisibleCopies(card) => {
                write!(formatter, "too many visible copies of {card}")
            }
            Self::DeckSizeMismatch { observed, inferred } => write!(
                formatter,
                "observed deck has {observed} cards but card locations imply {inferred}"
            ),
            Self::HiddenCardCountMismatch {
                identities,
                locations,
            } => write!(
                formatter,
                "{identities} hidden identities remain for {locations} hidden locations"
            ),
            Self::NoConsistentWorld => {
                formatter.write_str("no hidden world satisfies all direct clues")
            }
        }
    }
}

impl std::error::Error for InformationSetError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SampleError {
    NoConsistentWorld,
    Determinization(DeterminizationError),
}

impl fmt::Display for SampleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoConsistentWorld => formatter.write_str("no consistent world can be sampled"),
            Self::Determinization(error) => write!(formatter, "invalid sampled world: {error}"),
        }
    }
}

impl std::error::Error for SampleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoConsistentWorld => None,
            Self::Determinization(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hanabi_core::{FullState, PlayerId, standard_deck};

    #[test]
    fn exact_counting_is_lazy_and_does_not_affect_equality() {
        let state = FullState::new_standard(2, standard_deck()).unwrap();
        let view = state.view_for(PlayerId::new(0)).unwrap();
        let information_set = InformationSet::new(view).unwrap();

        assert!(information_set.completion_cache.get().is_none());
        assert_eq!(
            information_set
                .possible_identities(information_set.unknown_hand_cards()[0])
                .unwrap()
                .len(),
            22
        );
        assert!(information_set.completion_cache.get().is_none());

        assert_eq!(information_set.hand_assignment_count(), 146_611_080);
        assert!(information_set.completion_cache.get().is_some());

        let cloned = information_set.clone();
        assert!(cloned.completion_cache.get().is_none());
        assert_eq!(cloned, information_set);
    }

    #[test]
    fn short_circuit_feasibility_matches_exact_completion_counts() {
        let constraints = vec![
            ClueFacts {
                positive_ranks: vec![Rank::One],
                ..ClueFacts::default()
            },
            ClueFacts {
                positive_suits: vec![Suit::Red],
                ..ClueFacts::default()
            },
            ClueFacts {
                negative_suits: vec![Suit::Red, Suit::White],
                positive_ranks: vec![Rank::Two],
                ..ClueFacts::default()
            },
            ClueFacts {
                positive_suits: vec![Suit::Blue],
                negative_ranks: vec![Rank::One],
                ..ClueFacts::default()
            },
        ];
        let mut counts = standard_counts();
        counts[identity_index(Card::new(Suit::Red, Rank::One))] = 1;
        counts[identity_index(Card::new(Suit::Blue, Rank::Two))] = 1;

        assert_eq!(
            feasible_identities(&constraints, counts),
            exact_feasible_identities(&constraints, counts)
        );
    }

    fn exact_feasible_identities(constraints: &[ClueFacts], counts: Counts) -> Vec<Vec<Card>> {
        constraints
            .iter()
            .enumerate()
            .map(|(slot, constraint)| {
                let other_constraints = constraints
                    .iter()
                    .enumerate()
                    .filter_map(|(index, other)| (index != slot).then_some(other.clone()))
                    .collect::<Vec<_>>();

                all_identities()
                    .filter(|identity| {
                        let index = identity_index(*identity);
                        if counts[index] == 0 || !constraint.allows(*identity) {
                            return false;
                        }
                        let mut remaining = counts;
                        remaining[index] -= 1;
                        count_completions(
                            &other_constraints,
                            0,
                            &mut remaining,
                            &mut HashMap::new(),
                        ) > 0
                    })
                    .collect()
            })
            .collect()
    }
}
