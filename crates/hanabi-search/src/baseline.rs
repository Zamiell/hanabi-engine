use core::fmt;

use hanabi_core::{
    Action, CardId, GameStatus, MAX_CLUE_TOKENS, ObservedCard, PlayerId, PolicyObservation, Rank,
};

use crate::{IdentitySet, LogicalDeductions, PolicyDeductions};

/// What direct clues, public cards, and card-count elimination prove about one
/// card. No meaning is assigned to why any clue was given.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CardAssessment {
    pub certainly_playable: bool,
    pub certainly_useless: bool,
}

/// Assesses every identity that remains logically possible for a hidden card.
///
/// A card is certainly playable only when every possible identity can be
/// played now. It is certainly useless only when every possible identity has
/// already played or can never play because a required lower rank is exhausted.
/// Returns `None` when `card` is not an unknown card in this information set.
#[must_use]
pub fn assess_card(deductions: &LogicalDeductions, card: CardId) -> Option<CardAssessment> {
    let possibilities = deductions.possible_identities(card)?;
    let context = AssessmentContext::new(deductions.view());

    Some(context.assess(possibilities))
}

struct AssessmentContext {
    stack_heights: [u8; 5],
    maximum_reachable_ranks: [u8; 5],
}

impl AssessmentContext {
    fn new(view: &hanabi_core::PlayerView) -> Self {
        let stack_heights = std::array::from_fn(|suit| {
            u8::try_from(view.play_stacks[suit].len())
                .expect("a standard stack has at most five cards")
        });
        let mut discarded_counts = [0_u8; 25];
        for (_, card) in &view.discard_pile {
            discarded_counts[card.suit.index() * 5 + card.rank.index()] += 1;
        }
        Self::from_counts(stack_heights, discarded_counts)
    }

    fn from_observation(observation: &PolicyObservation) -> Self {
        Self::from_counts(observation.stack_heights, observation.discarded_counts)
    }

    fn from_counts(stack_heights: [u8; 5], discarded_counts: [u8; 25]) -> Self {
        let maximum_reachable_ranks = std::array::from_fn(|suit| {
            let stack_height = stack_heights[suit];
            Rank::ALL
                .into_iter()
                .find(|rank| {
                    rank.number() > stack_height
                        && discarded_counts[suit * 5 + rank.index()] == rank.copies()
                })
                .map_or(5, |rank| rank.number() - 1)
        });
        Self {
            stack_heights,
            maximum_reachable_ranks,
        }
    }

    fn assess(&self, possibilities: IdentitySet) -> CardAssessment {
        CardAssessment {
            certainly_playable: !possibilities.is_empty()
                && possibilities
                    .iter()
                    .all(|card| card.rank.number() == self.stack_heights[card.suit.index()] + 1),
            certainly_useless: !possibilities.is_empty()
                && possibilities.iter().all(|card| {
                    let rank = card.rank.number();
                    rank <= self.stack_heights[card.suit.index()]
                        || rank > self.maximum_reachable_ranks[card.suit.index()]
                }),
        }
    }
}

/// Selects an action using only a player's legal observation and its logical
/// information set.
pub trait RolloutPolicy {
    /// Whether rollout observations must include a copy of public event history.
    /// Convention-aware policies will normally retain the default.
    #[must_use]
    fn uses_history(&self) -> bool {
        true
    }

    /// Chooses one legal action for the current player.
    ///
    /// # Errors
    ///
    /// Returns [`PolicyError`] when the information set is not an actionable
    /// current-player position or does not contain its own hidden hand.
    fn select_action(&self, deductions: &LogicalDeductions) -> Result<Action, PolicyError>;

    /// Chooses an action for a leaf rollout inside search.
    ///
    /// Frameworks may use a more conservative value-estimation policy than
    /// their explicit tree policy. The default preserves ordinary rollout
    /// behavior.
    ///
    /// # Errors
    ///
    /// Returns the same policy-specific errors as [`Self::select_action`].
    fn select_search_action(&self, deductions: &LogicalDeductions) -> Result<Action, PolicyError> {
        self.select_action(deductions)
    }

    /// Returns a convention-forced continuation, when the current position
    /// has one. Search uses this to distinguish a predictable line from a
    /// heuristic rollout without exposing simulator truth to the policy.
    #[must_use]
    fn predictable_action(&self, _deductions: &LogicalDeductions) -> Option<Action> {
        None
    }

    /// Chooses from the compact convention-free rollout representation.
    ///
    /// Policies returning `false` from [`Self::uses_history`] must implement
    /// this method.
    ///
    /// # Errors
    ///
    /// Returns [`PolicyError::CompactObservationUnsupported`] unless the
    /// policy implements compact observation support.
    fn select_policy_action(
        &self,
        _deductions: &PolicyDeductions<'_>,
    ) -> Result<Action, PolicyError> {
        Err(PolicyError::CompactObservationUnsupported)
    }

    /// Compact-observation counterpart to [`Self::select_search_action`].
    ///
    /// # Errors
    ///
    /// Returns the same policy-specific errors as
    /// [`Self::select_policy_action`].
    fn select_search_policy_action(
        &self,
        deductions: &PolicyDeductions<'_>,
    ) -> Result<Action, PolicyError> {
        self.select_policy_action(deductions)
    }
}

/// A deliberately convention-agnostic rollout policy.
///
/// It never gives a clue or interprets why a clue was given. In order, it:
///
/// 1. plays the oldest certainly playable card;
/// 2. discards the oldest certainly useless card;
/// 3. otherwise discards the oldest card; or
/// 4. when discarding is illegal at full clues, blind-plays the newest card.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConventionAgnosticPolicy;

impl RolloutPolicy for ConventionAgnosticPolicy {
    fn uses_history(&self) -> bool {
        false
    }

    fn select_action(&self, deductions: &LogicalDeductions) -> Result<Action, PolicyError> {
        let view = deductions.view();
        let hand = view
            .hands
            .get(view.observer.index())
            .ok_or(PolicyError::MissingOwnHand)?;
        let context = AssessmentContext::new(view);
        select_action_from_knowledge(
            view.observer,
            view.current_player,
            view.status,
            view.clue_tokens,
            hand,
            &context,
            |card| deductions.possible_identities(card),
        )
    }

    fn select_policy_action(
        &self,
        deductions: &PolicyDeductions<'_>,
    ) -> Result<Action, PolicyError> {
        let observation = deductions.observation();
        let context = AssessmentContext::from_observation(observation);
        select_action_from_knowledge(
            observation.observer,
            observation.current_player,
            observation.status,
            observation.clue_tokens,
            &observation.own_hand,
            &context,
            |card| deductions.possible_identities(card),
        )
    }
}

fn select_action_from_knowledge(
    observer: PlayerId,
    current_player: PlayerId,
    status: GameStatus,
    clue_tokens: u8,
    hand: &[ObservedCard],
    context: &AssessmentContext,
    possible_identities: impl Fn(CardId) -> Option<IdentitySet>,
) -> Result<Action, PolicyError> {
    if status != GameStatus::InProgress || observer != current_player {
        return Err(PolicyError::NotCurrentPlayer);
    }
    if hand.is_empty() {
        return Err(PolicyError::EmptyOwnHand);
    }

    let mut first_useless = None;
    for observed in hand {
        let possibilities = possible_identities(observed.id)
            .ok_or(PolicyError::MissingPossibilities(observed.id))?;
        let assessment = context.assess(possibilities);
        if assessment.certainly_playable {
            return Ok(Action::Play(observed.id));
        }
        if first_useless.is_none() && assessment.certainly_useless {
            first_useless = Some(observed.id);
        }
    }

    // Hands are stored oldest-to-newest; a drawn card is appended.
    if clue_tokens < MAX_CLUE_TOKENS {
        return Ok(Action::Discard(first_useless.unwrap_or(hand[0].id)));
    }
    Ok(Action::Play(
        hand.last().expect("the hand was checked as nonempty").id,
    ))
}

/// Why a rollout policy could not choose an action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyError {
    NotCurrentPlayer,
    MissingOwnHand,
    EmptyOwnHand,
    MissingPossibilities(CardId),
    CompactObservationUnsupported,
    NoConventionAction,
}

impl fmt::Display for PolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotCurrentPlayer => formatter.write_str(
                "the information set is not an in-progress position for the current player",
            ),
            Self::MissingOwnHand => formatter.write_str("the observer's hand is missing"),
            Self::EmptyOwnHand => formatter.write_str("the observer's hand is empty"),
            Self::MissingPossibilities(card) => {
                write!(
                    formatter,
                    "the information set has no possibilities for {card}"
                )
            }
            Self::CompactObservationUnsupported => {
                formatter.write_str("policy does not support compact rollout observations")
            }
            Self::NoConventionAction => {
                formatter.write_str("the convention admits no action in this position")
            }
        }
    }
}

impl std::error::Error for PolicyError {}
