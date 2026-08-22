use core::fmt;

use hanabi_core::{Action, FullState, PlayerId, RuleError};

use crate::{InformationSet, InformationSetError, PolicyError, RolloutPolicy};

/// Defensive ceiling for a rollout. Standard games normally finish far below
/// this even when policies spend clue tokens between card actions.
pub const MAX_ROLLOUT_TURNS: u32 = 512;

/// A completed simulation and the actions selected along the way.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RolloutOutcome {
    final_state: FullState,
    actions: Vec<Action>,
    score: u8,
}

impl RolloutOutcome {
    /// Official Hanabi score, including a zero for a three-strike loss.
    #[must_use]
    pub const fn score(&self) -> u8 {
        self.score
    }

    /// Number of actions performed by this rollout.
    #[must_use]
    pub fn turns(&self) -> usize {
        self.actions.len()
    }

    #[must_use]
    pub const fn final_state(&self) -> &FullState {
        &self.final_state
    }

    #[must_use]
    pub fn actions(&self) -> &[Action] {
        &self.actions
    }
}

/// Plays a sampled authoritative state to completion.
///
/// The driver owns simulator truth, but at every turn the policy receives only
/// the acting player's [`InformationSet`]. This keeps hidden identities out of
/// action selection while still allowing the environment to resolve actions
/// and draws.
///
/// # Errors
///
/// Returns [`RolloutError`] if a legal observation cannot be converted into an
/// information set, the policy cannot act, an action is rejected by the rules,
/// or the defensive turn limit is exceeded.
pub fn rollout_to_terminal<P: RolloutPolicy>(
    mut state: FullState,
    policy: &P,
) -> Result<RolloutOutcome, RolloutError> {
    let mut actions = Vec::new();

    while !state.is_terminal() {
        if actions.len() >= MAX_ROLLOUT_TURNS as usize {
            return Err(RolloutError::TurnLimitExceeded);
        }

        let actor = state.current_player();
        let view = state
            .view_for(actor)
            .ok_or(RolloutError::InvalidCurrentPlayer(actor))?;
        let information_set = InformationSet::new(view).map_err(RolloutError::InformationSet)?;
        let action = policy
            .select_action(&information_set)
            .map_err(RolloutError::Policy)?;
        state.apply(action).map_err(RolloutError::Rule)?;
        actions.push(action);
    }

    let score = state
        .final_score()
        .ok_or(RolloutError::NonTerminalOutcome)?;
    Ok(RolloutOutcome {
        final_state: state,
        actions,
        score,
    })
}

/// Why a rollout could not reach a terminal state.
#[derive(Debug, Eq, PartialEq)]
pub enum RolloutError {
    InvalidCurrentPlayer(PlayerId),
    InformationSet(InformationSetError),
    Policy(PolicyError),
    Rule(RuleError),
    TurnLimitExceeded,
    NonTerminalOutcome,
}

impl fmt::Display for RolloutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCurrentPlayer(player) => {
                write!(
                    formatter,
                    "cannot construct a view for current player {player}"
                )
            }
            Self::InformationSet(error) => write!(formatter, "invalid information set: {error}"),
            Self::Policy(error) => write!(formatter, "rollout policy failed: {error}"),
            Self::Rule(error) => write!(formatter, "rollout action was illegal: {error}"),
            Self::TurnLimitExceeded => write!(
                formatter,
                "rollout exceeded the defensive limit of {MAX_ROLLOUT_TURNS} turns"
            ),
            Self::NonTerminalOutcome => {
                formatter.write_str("rollout stopped without reaching a terminal state")
            }
        }
    }
}

impl std::error::Error for RolloutError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InformationSet(error) => Some(error),
            Self::Policy(error) => Some(error),
            Self::Rule(error) => Some(error),
            Self::InvalidCurrentPlayer(_) | Self::TurnLimitExceeded | Self::NonTerminalOutcome => {
                None
            }
        }
    }
}
