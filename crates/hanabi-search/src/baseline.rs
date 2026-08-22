use core::fmt;

use hanabi_core::{Action, Card, CardId, GameStatus, MAX_CLUE_TOKENS, Rank};

use crate::InformationSet;

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
pub fn assess_card(information_set: &InformationSet, card: CardId) -> Option<CardAssessment> {
    let possibilities = information_set.possible_identities(card)?;
    let view = information_set.view();

    Some(CardAssessment {
        certainly_playable: !possibilities.is_empty()
            && possibilities
                .iter()
                .copied()
                .all(|identity| is_playable(view, identity)),
        certainly_useless: !possibilities.is_empty()
            && possibilities
                .iter()
                .copied()
                .all(|identity| is_useless(view, identity)),
    })
}

fn is_playable(view: &hanabi_core::PlayerView, card: Card) -> bool {
    let stack_height = view.play_stacks[card.suit.index()].len();
    usize::from(card.rank.number()) == stack_height + 1
}

fn is_useless(view: &hanabi_core::PlayerView, card: Card) -> bool {
    let stack_height = view.play_stacks[card.suit.index()].len();
    let rank = usize::from(card.rank.number());
    if rank <= stack_height {
        return true;
    }

    Rank::ALL
        .into_iter()
        .filter(|required| {
            let required = usize::from(required.number());
            required > stack_height && required < rank
        })
        .any(|required| {
            let discarded = view
                .discard_pile
                .iter()
                .filter(|(_, identity)| identity.suit == card.suit && identity.rank == required)
                .count();
            discarded == usize::from(required.copies())
        })
}

/// Selects an action using only a player's legal observation and its logical
/// information set.
pub trait RolloutPolicy {
    /// Chooses one legal action for the current player.
    ///
    /// # Errors
    ///
    /// Returns [`PolicyError`] when the information set is not an actionable
    /// current-player position or does not contain its own hidden hand.
    fn select_action(&self, information_set: &InformationSet) -> Result<Action, PolicyError>;
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
    fn select_action(&self, information_set: &InformationSet) -> Result<Action, PolicyError> {
        let view = information_set.view();
        if view.status != GameStatus::InProgress || view.observer != view.current_player {
            return Err(PolicyError::NotCurrentPlayer);
        }

        let hand = view
            .hands
            .get(view.observer.index())
            .ok_or(PolicyError::MissingOwnHand)?;
        if hand.is_empty() {
            return Err(PolicyError::EmptyOwnHand);
        }

        let mut assessments = Vec::with_capacity(hand.len());
        for observed in hand {
            let assessment = assess_card(information_set, observed.id)
                .ok_or(PolicyError::MissingPossibilities(observed.id))?;
            assessments.push((observed.id, assessment));
        }

        // Hands are stored oldest-to-newest; a drawn card is appended.
        if let Some((card, _)) = assessments
            .iter()
            .find(|(_, assessment)| assessment.certainly_playable)
        {
            return Ok(Action::Play(*card));
        }

        if view.clue_tokens < MAX_CLUE_TOKENS {
            if let Some((card, _)) = assessments
                .iter()
                .find(|(_, assessment)| assessment.certainly_useless)
            {
                return Ok(Action::Discard(*card));
            }
            return Ok(Action::Discard(hand[0].id));
        }

        Ok(Action::Play(
            hand.last().expect("the hand was checked as nonempty").id,
        ))
    }
}

/// Why a rollout policy could not choose an action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyError {
    NotCurrentPlayer,
    MissingOwnHand,
    EmptyOwnHand,
    MissingPossibilities(CardId),
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
        }
    }
}

impl std::error::Error for PolicyError {}
