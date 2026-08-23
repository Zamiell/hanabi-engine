use core::fmt;
use core::ops::Deref;
use std::collections::HashMap;
use std::sync::OnceLock;

use hanabi_core::{
    Card, CardId, ClueFacts, DeterminizationError, DeterminizationTemplate, FullState, PlayerView,
    PolicyObservation, Rank, Suit,
};
use rand::{Rng, RngExt, SeedableRng, rngs::StdRng, seq::SliceRandom};

const CARD_IDENTITY_COUNT: usize = 25;
const STANDARD_CARD_COUNT: usize = 50;
type Counts = [u8; CARD_IDENTITY_COUNT];
type CompletionMemo = HashMap<u64, u64>;

/// The hidden worlds logically consistent with one [`PlayerView`].
///
/// This first model uses public card counts and direct positive/negative clues.
/// Convention-dependent weighting will be layered on top rather than changing
/// which worlds are logically possible.
#[derive(Debug)]
pub struct InformationSet {
    deductions: LogicalDeductions,
    known_identities: [Option<Card>; STANDARD_CARD_COUNT],
    constraint_masks: Vec<u32>,
    deck_cards: Vec<CardId>,
    remaining_counts: Counts,
    determinization_template: DeterminizationTemplate,
    completion_cache: OnceLock<CompletionCache>,
}

/// Certainties derived from current public state, direct clues, and card counts.
///
/// This contains no convention interpretation and no determinization-only deck
/// reconstruction data, making it suitable for the policy hot path.
#[derive(Debug)]
pub struct LogicalDeductions {
    view: PlayerView,
    unknown_hand_cards: Vec<CardId>,
    possibilities: Vec<(CardId, IdentitySet)>,
    pub(crate) h_group_analysis: OnceLock<crate::h_group::HGroupAnalysis>,
}

/// Compact set of the 25 standard card identities.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IdentitySet(u32);

/// Logical possibilities derived from a compact rollout observation.
#[derive(Debug)]
pub struct PolicyDeductions<'a> {
    observation: &'a PolicyObservation,
    possibilities: [IdentitySet; 5],
}

impl IdentitySet {
    pub(crate) const ALL_MASK: u32 = (1 << CARD_IDENTITY_COUNT) - 1;

    pub(crate) const fn from_mask(mask: u32) -> Self {
        Self(mask & Self::ALL_MASK)
    }

    #[must_use]
    pub const fn all() -> Self {
        Self(Self::ALL_MASK)
    }

    #[must_use]
    pub const fn singleton(card: Card) -> Self {
        Self(1 << card.index())
    }

    #[must_use]
    pub const fn contains(self, card: Card) -> bool {
        self.0 & (1 << card.index()) != 0
    }

    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[must_use]
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub const fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    pub fn iter(self) -> impl Iterator<Item = Card> {
        IdentitySetIter(self.0)
    }
}

impl<'a> PolicyDeductions<'a> {
    /// Derives exact card-count possibilities for a rollout observation.
    ///
    /// # Errors
    ///
    /// Returns [`InformationSetError::NoConsistentWorld`] when the observation's
    /// direct clues admit no complete own-hand assignment.
    pub fn new(observation: &'a PolicyObservation) -> Result<Self, InformationSetError> {
        let mut constraints = [ClueFacts::default(); 5];
        for (slot, card) in observation.own_hand.iter().enumerate() {
            constraints[slot] = card.clues;
        }
        let possibilities = feasible_identity_array(
            &constraints[..observation.own_hand.len()],
            observation.remaining_counts,
        );
        if possibilities[..observation.own_hand.len()]
            .iter()
            .any(|identities| identities.is_empty())
        {
            return Err(InformationSetError::NoConsistentWorld);
        }
        Ok(Self {
            observation,
            possibilities,
        })
    }

    #[must_use]
    pub const fn observation(&self) -> &PolicyObservation {
        self.observation
    }

    #[must_use]
    pub fn possible_identities(&self, card: CardId) -> Option<IdentitySet> {
        self.observation
            .own_hand
            .iter()
            .position(|candidate| candidate.id == card)
            .map(|slot| self.possibilities[slot])
    }
}

struct IdentitySetIter(u32);

impl Iterator for IdentitySetIter {
    type Item = Card;

    fn next(&mut self) -> Option<Self::Item> {
        if self.0 == 0 {
            return None;
        }
        let index = self.0.trailing_zeros() as usize;
        self.0 &= self.0 - 1;
        Some(Card::new(Suit::ALL[index / 5], Rank::ALL[index % 5]))
    }
}

#[derive(Clone, Debug)]
struct CompletionCache {
    hand_assignment_count: u64,
    completion_memo: CompletionMemo,
}

impl Clone for InformationSet {
    fn clone(&self) -> Self {
        Self {
            deductions: self.deductions.clone(),
            known_identities: self.known_identities,
            constraint_masks: self.constraint_masks.clone(),
            deck_cards: self.deck_cards.clone(),
            remaining_counts: self.remaining_counts,
            determinization_template: self.determinization_template.clone(),
            completion_cache: OnceLock::new(),
        }
    }
}

impl PartialEq for InformationSet {
    fn eq(&self, other: &Self) -> bool {
        self.deductions == other.deductions
            && self.known_identities == other.known_identities
            && self.constraint_masks == other.constraint_masks
            && self.deck_cards == other.deck_cards
            && self.remaining_counts == other.remaining_counts
    }
}

impl Eq for InformationSet {}

impl Deref for InformationSet {
    type Target = LogicalDeductions;

    fn deref(&self) -> &Self::Target {
        &self.deductions
    }
}

impl LogicalDeductions {
    /// Derives exact possible identities without constructing sampling state.
    ///
    /// # Errors
    ///
    /// Returns [`InformationSetError`] when visible card counts or direct clues
    /// are inconsistent.
    pub fn new(view: PlayerView) -> Result<Self, InformationSetError> {
        let own_hand = view
            .hands
            .get(view.observer.index())
            .ok_or(InformationSetError::InvalidObserver)?;
        let unknown_hand_cards = own_hand.iter().map(|card| card.id).collect::<Vec<_>>();
        let constraints = own_hand.iter().map(|card| card.clues).collect::<Vec<_>>();
        let mut remaining_counts = standard_counts();
        for identity in view
            .hands
            .iter()
            .flatten()
            .filter_map(|card| card.identity)
            .chain(
                view.play_stacks
                    .iter()
                    .flatten()
                    .map(|(_, identity)| *identity),
            )
            .chain(view.discard_pile.iter().map(|(_, identity)| *identity))
        {
            let count = &mut remaining_counts[identity_index(identity)];
            if *count == 0 {
                return Err(InformationSetError::TooManyVisibleCopies(identity));
            }
            *count -= 1;
        }
        let feasible_by_slot = feasible_identities(&constraints, remaining_counts);
        if feasible_by_slot
            .iter()
            .any(|identities| identities.is_empty())
        {
            return Err(InformationSetError::NoConsistentWorld);
        }
        let possibilities = unknown_hand_cards
            .iter()
            .copied()
            .zip(feasible_by_slot)
            .collect();
        Ok(Self {
            view,
            unknown_hand_cards,
            possibilities,
            h_group_analysis: OnceLock::new(),
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

    #[must_use]
    pub fn possible_identities(&self, card: CardId) -> Option<IdentitySet> {
        self.possibilities
            .iter()
            .find_map(|(candidate, identities)| (*candidate == card).then_some(*identities))
    }
}

impl Clone for LogicalDeductions {
    fn clone(&self) -> Self {
        Self {
            view: self.view.clone(),
            unknown_hand_cards: self.unknown_hand_cards.clone(),
            possibilities: self.possibilities.clone(),
            h_group_analysis: OnceLock::new(),
        }
    }
}

impl PartialEq for LogicalDeductions {
    fn eq(&self, other: &Self) -> bool {
        self.view == other.view
            && self.unknown_hand_cards == other.unknown_hand_cards
            && self.possibilities == other.possibilities
    }
}

impl Eq for LogicalDeductions {}

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
        let mut known_identities = [None; STANDARD_CARD_COUNT];
        let mut unknown_hand_cards = Vec::new();
        let mut constraint_masks = Vec::new();

        for hand in &view.hands {
            for observed in hand {
                mark_location(&mut occupied, observed.id)?;
                if let Some(identity) = observed.identity {
                    set_known_identity(&mut known_identities, observed.id, identity)?;
                } else {
                    unknown_hand_cards.push(observed.id);
                    constraint_masks.push(observed.clues.identity_mask());
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

        let feasible_by_slot = feasible_identity_masks(&constraint_masks, remaining_counts)
            [..constraint_masks.len()]
            .to_vec();
        if feasible_by_slot
            .iter()
            .any(|identities| identities.is_empty())
        {
            return Err(InformationSetError::NoConsistentWorld);
        }
        let possibilities = unknown_hand_cards
            .iter()
            .copied()
            .zip(feasible_by_slot)
            .collect();
        let determinization_template = DeterminizationTemplate::new(&view)
            .map_err(InformationSetError::InvalidDeterminizationTemplate)?;

        Ok(Self {
            deductions: LogicalDeductions {
                view,
                unknown_hand_cards,
                possibilities,
                h_group_analysis: OnceLock::new(),
            },
            known_identities,
            constraint_masks,
            deck_cards,
            remaining_counts,
            determinization_template,
            completion_cache: OnceLock::new(),
        })
    }

    #[must_use]
    pub const fn view(&self) -> &PlayerView {
        self.deductions.view()
    }

    #[must_use]
    pub fn unknown_hand_cards(&self) -> &[CardId] {
        self.deductions.unknown_hand_cards()
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
    pub fn possible_identities(&self, card: CardId) -> Option<IdentitySet> {
        self.deductions.possible_identities(card)
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
        self.sample_with_masks(&self.constraint_masks, self.completion_cache(), rng)
    }

    /// Samples a world satisfying additional convention identity constraints.
    ///
    /// Sampling remains exact and card-copy weighted over the worlds admitted
    /// by the intersection of direct clues and `constraints`.
    ///
    /// # Errors
    ///
    /// Returns [`SampleError::NoConsistentWorld`] when the additional
    /// constraints contradict direct clues or public card counts.
    pub fn sample_constrained<R: Rng + ?Sized>(
        &self,
        constraints: &[(CardId, IdentitySet)],
        rng: &mut R,
    ) -> Result<FullState, SampleError> {
        let masks = self.constrained_masks(constraints);
        if masks.contains(&0) {
            return Err(SampleError::NoConsistentWorld);
        }
        let cache = self.cache_for_masks(&masks);
        if cache.hand_assignment_count == 0 {
            return Err(SampleError::NoConsistentWorld);
        }
        self.sample_with_masks(&masks, &cache, rng)
    }

    /// Samples exactly from a union of mutually-exclusive convention branches.
    /// Every branch is intersected with `constraints` and direct clue facts.
    pub(crate) fn sample_constrained_branches<R: Rng + ?Sized>(
        &self,
        constraints: &[(CardId, IdentitySet)],
        branches: &[Vec<(CardId, IdentitySet)>],
        rng: &mut R,
    ) -> Result<FullState, SampleError> {
        if branches.is_empty() {
            return self.sample_constrained(constraints, rng);
        }
        let base_masks = self.constrained_masks(constraints);
        let mut admitted = Vec::new();
        let mut total = 0_u64;
        for branch in branches {
            let mut masks = base_masks.clone();
            self.intersect_masks(&mut masks, branch);
            if masks.contains(&0) {
                continue;
            }
            let cache = self.cache_for_masks(&masks);
            if cache.hand_assignment_count == 0 {
                continue;
            }
            total += cache.hand_assignment_count;
            admitted.push((masks, cache));
        }
        if total == 0 {
            return Err(SampleError::NoConsistentWorld);
        }
        let mut draw = rng.random_range(0..total);
        for (masks, cache) in &admitted {
            if draw < cache.hand_assignment_count {
                return self.sample_with_masks(masks, cache, rng);
            }
            draw -= cache.hand_assignment_count;
        }
        Err(SampleError::NoConsistentWorld)
    }

    fn constrained_masks(&self, constraints: &[(CardId, IdentitySet)]) -> Vec<u32> {
        let mut masks = self.constraint_masks.clone();
        self.intersect_masks(&mut masks, constraints);
        masks
    }

    fn intersect_masks(&self, masks: &mut [u32], constraints: &[(CardId, IdentitySet)]) {
        for (card, identities) in constraints {
            let Some(slot) = self
                .deductions
                .unknown_hand_cards
                .iter()
                .position(|candidate| candidate == card)
            else {
                continue;
            };
            masks[slot] &= identities.0;
        }
    }

    fn cache_for_masks(&self, masks: &[u32]) -> CompletionCache {
        let mut counts = self.remaining_counts;
        let mut completion_memo = HashMap::new();
        let hand_assignment_count = count_completions(masks, 0, &mut counts, &mut completion_memo);
        CompletionCache {
            hand_assignment_count,
            completion_memo,
        }
    }

    fn sample_with_masks<R: Rng + ?Sized>(
        &self,
        masks: &[u32],
        completion_cache: &CompletionCache,
        rng: &mut R,
    ) -> Result<FullState, SampleError> {
        let mut counts = self.remaining_counts;
        let placeholder = Card::new(Suit::Red, Rank::One);
        let mut cards = self
            .known_identities
            .map(|identity| identity.unwrap_or(placeholder))
            .to_vec();

        for (slot, card_id) in self
            .deductions
            .unknown_hand_cards
            .iter()
            .copied()
            .enumerate()
        {
            let mut candidate_identities = [0_usize; CARD_IDENTITY_COUNT];
            let mut candidate_weights = [0_u64; CARD_IDENTITY_COUNT];
            let mut candidate_count = 0;
            let mut total_weight = 0_u64;
            for index in 0..CARD_IDENTITY_COUNT {
                let copies = counts[index];
                if copies == 0 || masks[slot] & (1 << index) == 0 {
                    continue;
                }
                counts[index] -= 1;
                let completions = cached_completions(
                    slot + 1,
                    masks.len(),
                    counts,
                    &completion_cache.completion_memo,
                );
                counts[index] += 1;
                let weight = u64::from(copies) * completions;
                if weight > 0 {
                    candidate_identities[candidate_count] = index;
                    candidate_weights[candidate_count] = weight;
                    candidate_count += 1;
                    total_weight += weight;
                }
            }

            if total_weight == 0 {
                return Err(SampleError::NoConsistentWorld);
            }
            let mut draw = rng.random_range(0..total_weight);
            let Some(selected_index) = (0..candidate_count).find_map(|candidate| {
                let weight = candidate_weights[candidate];
                if draw < weight {
                    Some(candidate_identities[candidate])
                } else {
                    draw -= weight;
                    None
                }
            }) else {
                return Err(SampleError::NoConsistentWorld);
            };
            counts[selected_index] -= 1;
            cards[card_id.index()] = identity_from_index(selected_index);
        }

        let mut deck_identities = [placeholder; STANDARD_CARD_COUNT];
        let mut deck_identity_count = 0;
        for identity in all_identities() {
            for _ in 0..counts[identity_index(identity)] {
                deck_identities[deck_identity_count] = identity;
                deck_identity_count += 1;
            }
        }
        debug_assert_eq!(deck_identity_count, self.deck_cards.len());
        deck_identities[..deck_identity_count].shuffle(rng);
        for (card_id, identity) in self
            .deck_cards
            .iter()
            .zip(deck_identities[..deck_identity_count].iter().copied())
        {
            cards[card_id.index()] = identity;
        }

        self.determinization_template
            .instantiate(cards)
            .map_err(SampleError::Determinization)
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
                count_completions(&self.constraint_masks, 0, &mut counts, &mut completion_memo);
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
    card.index()
}

fn identity_from_index(index: usize) -> Card {
    Card::new(Suit::ALL[index / 5], Rank::ALL[index % 5])
}

fn count_completions(
    constraint_masks: &[u32],
    slot: usize,
    counts: &mut Counts,
    memo: &mut CompletionMemo,
) -> u64 {
    if slot == constraint_masks.len() {
        return 1;
    }
    let key = completion_key(slot, *counts);
    if let Some(value) = memo.get(&key) {
        return *value;
    }

    let mut total = 0_u64;
    for identity in all_identities() {
        let index = identity_index(identity);
        let copies = counts[index];
        if copies == 0 || constraint_masks[slot] & (1 << index) == 0 {
            continue;
        }
        counts[index] -= 1;
        total += u64::from(copies) * count_completions(constraint_masks, slot + 1, counts, memo);
        counts[index] += 1;
    }
    memo.insert(key, total);
    total
}

fn cached_completions(
    slot: usize,
    constraint_count: usize,
    counts: Counts,
    memo: &CompletionMemo,
) -> u64 {
    if slot == constraint_count {
        1
    } else {
        *memo
            .get(&completion_key(slot, counts))
            .expect("the exact completion pass memoizes every reachable state")
    }
}

fn completion_key(slot: usize, counts: Counts) -> u64 {
    debug_assert!(slot <= 5);
    let packed_counts = counts
        .iter()
        .enumerate()
        .fold(0_u64, |packed, (identity, count)| {
            debug_assert!(*count <= 3);
            packed | (u64::from(*count) << (identity * 2))
        });
    packed_counts | ((slot as u64) << (CARD_IDENTITY_COUNT * 2))
}

fn feasible_identities(constraints: &[ClueFacts], counts: Counts) -> Vec<IdentitySet> {
    feasible_identity_array(constraints, counts)[..constraints.len()].to_vec()
}

fn feasible_identity_array(constraints: &[ClueFacts], counts: Counts) -> [IdentitySet; 5] {
    debug_assert!(constraints.len() <= 5);
    let mut masks = [0; 5];
    for (slot, constraint) in constraints.iter().enumerate() {
        masks[slot] = constraint.identity_mask();
    }
    feasible_identity_masks(&masks[..constraints.len()], counts)
}

fn feasible_identity_masks(constraint_masks: &[u32], counts: Counts) -> [IdentitySet; 5] {
    debug_assert!(constraint_masks.len() <= 5);
    let mut allowed = [0_u32; 5];
    allowed[..constraint_masks.len()].copy_from_slice(constraint_masks);
    let subset_count = 1_usize << constraint_masks.len();
    let mut union_masks = [0_u32; 32];
    let mut union_capacities = [0_u8; 32];
    for subset in 1..subset_count {
        let slot = subset.trailing_zeros() as usize;
        let previous = subset & !(1 << slot);
        let union = union_masks[previous] | allowed[slot];
        union_masks[subset] = union;
        let mut capacity = union_capacities[previous];
        let mut identities = allowed[slot] & !union_masks[previous];
        while identities != 0 {
            let identity = identities.trailing_zeros() as usize;
            identities &= identities - 1;
            capacity += counts[identity];
        }
        union_capacities[subset] = capacity;
    }

    let mut feasible = [IdentitySet::default(); 5];
    let available_identities = counts
        .iter()
        .enumerate()
        .fold(0_u32, |mask, (identity, count)| {
            mask | (u32::from(*count > 0) << identity)
        });
    for fixed_slot in 0..constraint_masks.len() {
        let mut forbidden = 0_u32;
        let mut impossible = false;
        for subset in (1..subset_count).filter(|subset| subset & (1 << fixed_slot) == 0) {
            let required =
                u8::try_from(subset.count_ones()).expect("a standard hand has at most five cards");
            match union_capacities[subset].cmp(&required) {
                core::cmp::Ordering::Less => {
                    impossible = true;
                    break;
                }
                core::cmp::Ordering::Equal => forbidden |= union_masks[subset],
                core::cmp::Ordering::Greater => {}
            }
        }
        if !impossible {
            feasible[fixed_slot] =
                IdentitySet::from_mask(allowed[fixed_slot] & available_identities & !forbidden);
        }
    }
    feasible
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
    InvalidDeterminizationTemplate(DeterminizationError),
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
            Self::InvalidDeterminizationTemplate(error) => {
                write!(formatter, "invalid public state for sampling: {error}")
            }
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
    use hanabi_core::{Clue, FullState, PlayerId, standard_deck};
    use rand::{RngExt, SeedableRng, rngs::StdRng};

    fn clue_facts(positive: &[Clue], negative: &[Clue]) -> ClueFacts {
        let mut facts = ClueFacts::default();
        for clue in positive {
            facts.add_positive_clue(*clue);
        }
        for clue in negative {
            facts.add_negative_clue(*clue);
        }
        facts
    }

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
            clue_facts(&[Clue::Rank(Rank::One)], &[]),
            clue_facts(&[Clue::Suit(Suit::Red)], &[]),
            clue_facts(
                &[Clue::Rank(Rank::Two)],
                &[Clue::Suit(Suit::Red), Clue::Suit(Suit::Purple)],
            ),
            clue_facts(&[Clue::Suit(Suit::Blue)], &[Clue::Rank(Rank::One)]),
        ];
        let mut counts = standard_counts();
        counts[identity_index(Card::new(Suit::Red, Rank::One))] = 1;
        counts[identity_index(Card::new(Suit::Blue, Rank::Two))] = 1;

        assert_eq!(
            feasible_identities(&constraints, counts),
            exact_feasible_identities(&constraints, counts)
        );
    }

    #[test]
    fn hall_feasibility_matches_exact_counting_across_random_constraints() {
        let mut rng = StdRng::seed_from_u64(0x4841_4e41_4249);
        for case in 0..256 {
            let mut counts = [0; CARD_IDENTITY_COUNT];
            for _ in 0..8 {
                counts[rng.random_range(0..CARD_IDENTITY_COUNT)] = rng.random_range(1..=2);
            }
            if counts
                .iter()
                .map(|count| usize::from(*count))
                .sum::<usize>()
                < 4
            {
                counts[0..4].fill(1);
            }

            let constraints = (0..4)
                .map(|_| {
                    let suit = Suit::ALL[rng.random_range(0..Suit::ALL.len())];
                    let rank = Rank::ALL[rng.random_range(0..Rank::ALL.len())];
                    match rng.random_range(0..6) {
                        0 => clue_facts(&[], &[]),
                        1 => clue_facts(&[Clue::Suit(suit)], &[]),
                        2 => clue_facts(&[Clue::Rank(rank)], &[]),
                        3 => clue_facts(&[], &[Clue::Suit(suit)]),
                        4 => clue_facts(&[], &[Clue::Rank(rank)]),
                        _ => clue_facts(&[Clue::Suit(suit)], &[Clue::Rank(rank)]),
                    }
                })
                .collect::<Vec<_>>();

            assert_eq!(
                feasible_identities(&constraints, counts),
                exact_feasible_identities(&constraints, counts),
                "random constraint case {case}"
            );
        }
    }

    fn exact_feasible_identities(constraints: &[ClueFacts], counts: Counts) -> Vec<IdentitySet> {
        constraints
            .iter()
            .enumerate()
            .map(|(slot, constraint)| {
                let other_constraints = constraints
                    .iter()
                    .enumerate()
                    .filter_map(|(index, other)| (index != slot).then_some(other.identity_mask()))
                    .collect::<Vec<_>>();

                all_identities()
                    .enumerate()
                    .filter(|(_, identity)| {
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
                    .fold(IdentitySet::default(), |identities, (index, _)| {
                        IdentitySet::from_mask(identities.0 | (1 << index))
                    })
            })
            .collect()
    }
}
