use core::fmt;

use hanabi_core::{
    Card, CardId, ClueFacts, FullState, PlayerView, Rank, Suit, WorldConstructionError,
    WorldTemplate,
};

const CARD_IDENTITY_COUNT: usize = 25;
const STANDARD_CARD_COUNT: usize = 50;
type Counts = [u8; CARD_IDENTITY_COUNT];

/// The hidden worlds logically consistent with one [`PlayerView`].
///
/// Direct clues and public card counts define the logical base; optional
/// convention constraints can further restrict the worlds visited by a plan.
#[derive(Debug)]
pub struct InformationSet {
    deductions: LogicalDeductions,
    known_identities: [Option<Card>; STANDARD_CARD_COUNT],
    constraint_masks: Vec<u32>,
    deck_cards: Vec<CardId>,
    world_template: WorldTemplate,
}

/// Certainties derived from current public state, direct clues, and card counts.
///
/// This contains no convention interpretation and no world construction-only deck
/// reconstruction data, making it suitable for the policy hot path.
#[derive(Debug)]
pub struct LogicalDeductions {
    view: PlayerView,
    unknown_hand_cards: Vec<CardId>,
    possibilities: Vec<(CardId, IdentitySet)>,
    remaining_counts: Counts,
}

/// Compact set of the 25 standard card identities.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IdentitySet(u32);

/// Result of deterministically visiting complete, card-count-consistent own
/// hand assignments for nested perspective analysis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HandAssignmentVisit {
    pub(crate) examined: usize,
    pub(crate) complete: bool,
    pub(crate) stopped: bool,
}

/// Result of counting complete identity assignments up to a caller-supplied
/// limit. `exact` is false when traversal stopped after proving that more than
/// `limit` worlds exist.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldCount {
    pub worlds: u64,
    pub exact: bool,
}

/// Convention-derived restrictions on the root belief state.
///
/// `constraints` always apply. Each entry in `branches` is an additional,
/// mutually-exclusive conjunction; an empty branch list means that only the
/// common constraints apply.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BeliefConstraints {
    pub constraints: Vec<(CardId, IdentitySet)>,
    pub branches: Vec<Vec<(CardId, IdentitySet)>>,
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

impl Clone for InformationSet {
    fn clone(&self) -> Self {
        Self {
            deductions: self.deductions.clone(),
            known_identities: self.known_identities,
            constraint_masks: self.constraint_masks.clone(),
            deck_cards: self.deck_cards.clone(),
            world_template: self.world_template.clone(),
        }
    }
}

impl PartialEq for InformationSet {
    fn eq(&self, other: &Self) -> bool {
        self.deductions == other.deductions
            && self.known_identities == other.known_identities
            && self.constraint_masks == other.constraint_masks
            && self.deck_cards == other.deck_cards
    }
}

impl Eq for InformationSet {}

impl LogicalDeductions {
    /// Derives exact possible identities without constructing complete worlds.
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
            remaining_counts,
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

    /// Visits complete own-hand assignments without inventing impossible card
    /// combinations. Returning `true` from `visitor` stops traversal.
    pub(crate) fn visit_hand_assignments(
        &self,
        limit: usize,
        mut visitor: impl FnMut(&[(CardId, Card)]) -> bool,
    ) -> HandAssignmentVisit {
        let mut remaining = self.remaining_counts;
        let mut assignment = Vec::with_capacity(self.unknown_hand_cards.len());
        let mut result = HandAssignmentVisit {
            examined: 0,
            complete: true,
            stopped: false,
        };
        visit_assignments(
            &self.unknown_hand_cards,
            &self.possibilities,
            0,
            &mut remaining,
            &mut assignment,
            limit,
            &mut visitor,
            &mut result,
        );
        result
    }
}

#[allow(clippy::too_many_arguments)]
fn visit_assignments(
    cards: &[CardId],
    possibilities: &[(CardId, IdentitySet)],
    slot: usize,
    remaining: &mut Counts,
    assignment: &mut Vec<(CardId, Card)>,
    limit: usize,
    visitor: &mut impl FnMut(&[(CardId, Card)]) -> bool,
    result: &mut HandAssignmentVisit,
) {
    if result.stopped || !result.complete {
        return;
    }
    if slot == cards.len() {
        if result.examined == limit {
            result.complete = false;
            return;
        }
        result.examined += 1;
        result.stopped = visitor(assignment);
        return;
    }
    let card = cards[slot];
    let identities = possibilities
        .iter()
        .find_map(|(candidate, identities)| (*candidate == card).then_some(*identities))
        .unwrap_or_default();
    for identity in identities.iter() {
        let count = &mut remaining[identity_index(identity)];
        if *count == 0 {
            continue;
        }
        *count -= 1;
        assignment.push((card, identity));
        visit_assignments(
            cards,
            possibilities,
            slot + 1,
            remaining,
            assignment,
            limit,
            visitor,
            result,
        );
        assignment.pop();
        remaining[identity_index(identity)] += 1;
        if result.stopped || !result.complete {
            return;
        }
    }
}

impl Clone for LogicalDeductions {
    fn clone(&self) -> Self {
        Self {
            view: self.view.clone(),
            unknown_hand_cards: self.unknown_hand_cards.clone(),
            possibilities: self.possibilities.clone(),
            remaining_counts: self.remaining_counts,
        }
    }
}

impl PartialEq for LogicalDeductions {
    fn eq(&self, other: &Self) -> bool {
        self.view == other.view
            && self.unknown_hand_cards == other.unknown_hand_cards
            && self.possibilities == other.possibilities
            && self.remaining_counts == other.remaining_counts
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
    pub fn new(view: &PlayerView) -> Result<Self, InformationSetError> {
        let deductions = LogicalDeductions::new(view.clone())?;

        let mut occupied = [false; STANDARD_CARD_COUNT];
        let mut known_identities = [None; STANDARD_CARD_COUNT];
        let mut constraint_masks = Vec::new();

        for hand in &view.hands {
            for observed in hand {
                mark_location(&mut occupied, observed.id)?;
                if let Some(identity) = observed.identity {
                    set_known_identity(&mut known_identities, observed.id, identity)?;
                } else {
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

        let remaining_counts = deductions.remaining_counts;

        let remaining_total: usize = remaining_counts
            .iter()
            .map(|count| usize::from(*count))
            .sum();
        let hidden_total = deductions.unknown_hand_cards.len() + deck_cards.len();
        if remaining_total != hidden_total {
            return Err(InformationSetError::HiddenCardCountMismatch {
                identities: remaining_total,
                locations: hidden_total,
            });
        }

        let world_template =
            WorldTemplate::new(view).map_err(InformationSetError::InvalidWorldTemplate)?;

        Ok(Self {
            deductions,
            known_identities,
            constraint_masks,
            deck_cards,
            world_template,
        })
    }

    #[must_use]
    pub const fn view(&self) -> &PlayerView {
        self.deductions.view()
    }

    /// Convention-agnostic logical analysis shared with symbolic planning.
    #[must_use]
    pub const fn deductions(&self) -> &LogicalDeductions {
        &self.deductions
    }

    #[must_use]
    pub fn possible_identities(&self, card: CardId) -> Option<IdentitySet> {
        self.deductions.possible_identities(card)
    }

    /// Counts complete, identity-distinct hidden worlds admitted by direct
    /// clues and optional convention constraints.
    ///
    /// Counting stops as soon as `limit + 1` worlds have been found. This is
    /// the inexpensive gate used by exact endgame planning; it avoids trying
    /// to represent the astronomical opening belief space.
    #[must_use]
    pub fn world_count_up_to(&self, belief: &BeliefConstraints, limit: u64) -> WorldCount {
        let stop_after = limit.saturating_add(1);
        let mut worlds = 0_u64;
        self.visit_belief_masks(belief, |masks| {
            let mut counts = self.deductions.remaining_counts;
            count_identity_worlds(
                masks,
                self.deck_cards.len(),
                0,
                &mut counts,
                stop_after,
                &mut worlds,
            );
            worlds < stop_after
        });
        WorldCount {
            worlds: worlds.min(stop_after),
            exact: worlds <= limit,
        }
    }

    /// Visits every complete hidden world admitted by direct clues and
    /// convention constraints, provided the set contains at most `limit`
    /// worlds.
    ///
    /// # Errors
    ///
    /// Returns [`EnumerateWorldsError::LimitExceeded`] before invoking the
    /// visitor when the exact-planning gate is too large, or wraps an invalid
    /// world construction if an internal invariant is violated.
    pub fn visit_worlds(
        &self,
        belief: &BeliefConstraints,
        limit: u64,
        visitor: impl FnMut(FullState),
    ) -> Result<u64, EnumerateWorldsError> {
        let count = self.world_count_up_to(belief, limit);
        if !count.exact {
            return Err(EnumerateWorldsError::LimitExceeded {
                limit,
                at_least: count.worlds,
            });
        }

        self.visit_worlds_after_count(belief, visitor)
    }

    /// Materializes worlds after the caller has already completed the bounded
    /// count used by the exact-planning gate.
    pub(crate) fn collect_worlds_after_count(
        &self,
        belief: &BeliefConstraints,
        capacity: usize,
    ) -> Result<Vec<FullState>, EnumerateWorldsError> {
        let mut worlds = Vec::with_capacity(capacity);
        self.visit_worlds_after_count(belief, |world| worlds.push(world))?;
        Ok(worlds)
    }

    fn visit_worlds_after_count(
        &self,
        belief: &BeliefConstraints,
        mut visitor: impl FnMut(FullState),
    ) -> Result<u64, EnumerateWorldsError> {
        let placeholder = Card::new(Suit::Red, Rank::One);
        let base_cards = self
            .known_identities
            .map(|identity| identity.unwrap_or(placeholder))
            .to_vec();
        let mut visited = 0_u64;
        let mut error = None;
        self.visit_belief_masks(belief, |masks| {
            let mut locations = self.deductions.unknown_hand_cards.clone();
            locations.extend(self.deck_cards.iter().copied());
            let mut cards = base_cards.clone();
            let mut counts = self.deductions.remaining_counts;
            visit_identity_worlds(
                &locations,
                masks,
                0,
                &mut counts,
                &mut cards,
                &mut |cards| match self.world_template.instantiate(cards.to_vec()) {
                    Ok(state) => {
                        visited += 1;
                        visitor(state);
                        true
                    }
                    Err(source) => {
                        error = Some(source);
                        false
                    }
                },
            );
            error.is_none()
        });
        if let Some(source) = error {
            return Err(EnumerateWorldsError::WorldConstruction(source));
        }
        Ok(visited)
    }

    fn visit_belief_masks(
        &self,
        belief: &BeliefConstraints,
        mut visitor: impl FnMut(&[u32]) -> bool,
    ) {
        let base_masks = self.constrained_masks(&belief.constraints);
        if base_masks.contains(&0) {
            return;
        }
        if belief.branches.is_empty() {
            let _ = visitor(&base_masks);
            return;
        }
        for branch in &belief.branches {
            let mut masks = base_masks.clone();
            self.intersect_masks(&mut masks, branch);
            if !masks.contains(&0) && !visitor(&masks) {
                return;
            }
        }
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
}

fn count_identity_worlds(
    hand_masks: &[u32],
    deck_len: usize,
    slot: usize,
    counts: &mut Counts,
    stop_after: u64,
    worlds: &mut u64,
) {
    if *worlds >= stop_after {
        return;
    }
    let location_count = hand_masks.len() + deck_len;
    if slot == location_count {
        *worlds += 1;
        return;
    }
    let allowed = hand_masks
        .get(slot)
        .copied()
        .unwrap_or(IdentitySet::ALL_MASK);
    for identity in 0..CARD_IDENTITY_COUNT {
        if counts[identity] == 0 || allowed & (1 << identity) == 0 {
            continue;
        }
        counts[identity] -= 1;
        count_identity_worlds(hand_masks, deck_len, slot + 1, counts, stop_after, worlds);
        counts[identity] += 1;
        if *worlds >= stop_after {
            return;
        }
    }
}

fn visit_identity_worlds(
    locations: &[CardId],
    hand_masks: &[u32],
    slot: usize,
    counts: &mut Counts,
    cards: &mut [Card],
    visitor: &mut impl FnMut(&[Card]) -> bool,
) -> bool {
    if slot == locations.len() {
        return visitor(cards);
    }
    let allowed = hand_masks
        .get(slot)
        .copied()
        .unwrap_or(IdentitySet::ALL_MASK);
    for identity in 0..CARD_IDENTITY_COUNT {
        if counts[identity] == 0 || allowed & (1 << identity) == 0 {
            continue;
        }
        counts[identity] -= 1;
        cards[locations[slot].index()] = identity_from_index(identity);
        if !visit_identity_worlds(locations, hand_masks, slot + 1, counts, cards, visitor) {
            counts[identity] += 1;
            return false;
        }
        counts[identity] += 1;
    }
    true
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
    InvalidWorldTemplate(WorldConstructionError),
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
            Self::InvalidWorldTemplate(error) => {
                write!(
                    formatter,
                    "invalid public state for exact planning: {error}"
                )
            }
            Self::NoConsistentWorld => {
                formatter.write_str("no hidden world satisfies all direct clues")
            }
        }
    }
}

impl std::error::Error for InformationSetError {}

/// Failure while exhaustively materializing a bounded belief state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EnumerateWorldsError {
    LimitExceeded { limit: u64, at_least: u64 },
    WorldConstruction(WorldConstructionError),
}

impl fmt::Display for EnumerateWorldsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LimitExceeded { limit, at_least } => write!(
                formatter,
                "belief contains at least {at_least} worlds, exceeding exact limit {limit}"
            ),
            Self::WorldConstruction(error) => {
                write!(formatter, "invalid enumerated world: {error}")
            }
        }
    }
}

impl std::error::Error for EnumerateWorldsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::LimitExceeded { .. } => None,
            Self::WorldConstruction(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    use hanabi_core::{FullState, PlayerId, standard_deck};

    #[test]
    fn bounded_world_count_short_circuits_openings() {
        let state = FullState::new_standard(2, standard_deck()).unwrap();
        let information = InformationSet::new(&state.view_for(PlayerId::new(0)).unwrap()).unwrap();
        assert_eq!(
            information.world_count_up_to(&BeliefConstraints::default(), 32),
            WorldCount {
                worlds: 33,
                exact: false,
            }
        );
    }

    #[test]
    fn exact_world_enumeration_is_complete_unique_and_observation_safe() {
        let mut state = FullState::new_standard(2, standard_deck()).unwrap();
        while state.deck_size() > 0 && !state.is_terminal() {
            let playable = state
                .hand(state.current_player())
                .unwrap()
                .iter()
                .find(|card| {
                    let identity = state.card(**card).unwrap();
                    identity.rank.number()
                        == u8::try_from(state.play_stacks()[identity.suit.index()].len()).unwrap()
                            + 1
                });
            let action = playable.map_or_else(
                || {
                    state
                        .legal_actions()
                        .into_iter()
                        .find(|action| matches!(action, hanabi_core::Action::Discard(_)))
                        .unwrap_or_else(|| {
                            state
                                .legal_actions()
                                .into_iter()
                                .find(|action| matches!(action, hanabi_core::Action::Clue { .. }))
                                .unwrap()
                        })
                },
                |card| hanabi_core::Action::Play(*card),
            );
            state.apply(action).unwrap();
        }
        assert!(!state.is_terminal());
        let observer = state.current_player();
        let view = state.view_for(observer).unwrap();
        let information = InformationSet::new(&view.clone()).unwrap();
        let belief = BeliefConstraints::default();
        let count = information.world_count_up_to(&belief, 100_000);
        assert!(count.exact);
        assert!(count.worlds > 1);

        let mut unique = HashSet::new();
        let visited = information
            .visit_worlds(&belief, 100_000, |world| {
                assert_eq!(world.view_for(observer), Some(view.clone()));
                assert!(unique.insert(format!("{world:?}")));
            })
            .unwrap();
        assert_eq!(visited, count.worlds);
        assert_eq!(u64::try_from(unique.len()).unwrap(), count.worlds);
    }
}
