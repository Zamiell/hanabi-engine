use core::fmt;
use std::time::{Duration, Instant};

use hanabi_core::{Action, FullState, PlayerId, RuleError};

use crate::{InformationSetError, LogicalDeductions, PolicyDeductions, PolicyError, RolloutPolicy};

/// Defensive ceiling for a rollout. Standard games normally finish far below
/// this even when policies spend clue tokens between card actions.
pub const MAX_ROLLOUT_TURNS: u32 = 512;

/// Makes one official score point dominate the complete `0..=25` raw-score
/// tie-break range.
pub const OFFICIAL_SCORE_UTILITY_WEIGHT: u16 = 26;

/// Maximum terminal utility: a perfect official and raw score of 25.
pub const MAX_TERMINAL_UTILITY: u16 = 25 * OFFICIAL_SCORE_UTILITY_WEIGHT + 25;

/// Convention-free terminal utility used as the search learning signal.
///
/// Official Hanabi score is the primary term. Raw stack score distinguishes
/// outcomes with the same official score, most importantly three-strike
/// endings that all have official score zero.
#[must_use]
pub const fn terminal_utility(official_score: u8, raw_score: u8) -> u16 {
    official_score as u16 * OFFICIAL_SCORE_UTILITY_WEIGHT + raw_score as u16
}

/// A completed simulation and the actions selected along the way.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RolloutOutcome {
    final_state: FullState,
    actions: Vec<Action>,
    turns: usize,
    score: u8,
}

/// Timing breakdown for one completed terminal rollout.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RolloutDiagnostics {
    pub total_time: Duration,
    pub observation_time: Duration,
    pub deduction_time: Duration,
    pub policy_time: Duration,
    pub apply_time: Duration,
    pub other_time: Duration,
}

/// Terminal rollout outcome plus measured timing diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RolloutReport {
    pub outcome: RolloutOutcome,
    pub diagnostics: RolloutDiagnostics,
}

impl RolloutOutcome {
    /// Official Hanabi score, including a zero for a three-strike loss.
    #[must_use]
    pub const fn score(&self) -> u8 {
        self.score
    }

    /// Score currently present on the stacks, even after a three-strike loss.
    #[must_use]
    pub fn raw_score(&self) -> u8 {
        self.final_state.score()
    }

    /// Search utility combining official score with raw score as a tie-break.
    #[must_use]
    pub fn utility(&self) -> u16 {
        terminal_utility(self.score, self.raw_score())
    }

    /// Number of actions performed by this rollout.
    #[must_use]
    pub fn turns(&self) -> usize {
        self.turns
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
/// the acting player's [`LogicalDeductions`] or equivalent compact deductions.
/// This keeps hidden identities out of action selection while still allowing
/// the environment to resolve actions and draws.
///
/// # Errors
///
/// Returns [`RolloutError`] if a legal observation cannot be converted into an
/// information set, the policy cannot act, an action is rejected by the rules,
/// or the defensive turn limit is exceeded.
pub fn rollout_to_terminal<P: RolloutPolicy>(
    state: FullState,
    policy: &P,
) -> Result<RolloutOutcome, RolloutError> {
    Ok(run_rollout::<_, true, false>(state, policy)?.outcome)
}

/// Plays a sampled state to completion and records per-stage rollout timing.
///
/// # Errors
///
/// Returns the same [`RolloutError`] conditions as [`rollout_to_terminal`].
pub fn rollout_to_terminal_with_diagnostics<P: RolloutPolicy>(
    state: FullState,
    policy: &P,
) -> Result<RolloutReport, RolloutError> {
    run_rollout::<_, true, true>(state, policy)
}

pub(crate) fn rollout_for_search<P: RolloutPolicy>(
    state: FullState,
    policy: &P,
    measure_timing: bool,
) -> Result<RolloutReport, RolloutError> {
    if measure_timing {
        run_rollout::<_, false, true>(state, policy)
    } else {
        run_rollout::<_, false, false>(state, policy)
    }
}

fn run_rollout<P: RolloutPolicy, const RECORD_ACTIONS: bool, const MEASURE_TIMING: bool>(
    mut state: FullState,
    policy: &P,
) -> Result<RolloutReport, RolloutError> {
    let rollout_started = MEASURE_TIMING.then(Instant::now);
    let mut actions = Vec::new();
    let mut turns = 0;
    let mut diagnostics = RolloutDiagnostics::default();

    while !state.is_terminal() {
        if turns >= MAX_ROLLOUT_TURNS as usize {
            return Err(RolloutError::TurnLimitExceeded);
        }

        let action = if MEASURE_TIMING {
            select_rollout_action_with_diagnostics(&state, policy, &mut diagnostics)?
        } else {
            select_rollout_action(&state, policy)?
        };
        if MEASURE_TIMING {
            let apply_started = Instant::now();
            let applied = state.apply(action);
            diagnostics.apply_time += apply_started.elapsed();
            applied.map_err(RolloutError::Rule)?;
        } else {
            state.apply(action).map_err(RolloutError::Rule)?;
        }
        if RECORD_ACTIONS {
            actions.push(action);
        }
        turns += 1;
    }

    let outcome = finish_outcome(state, actions, turns)?;
    if let Some(started) = rollout_started {
        diagnostics.total_time = started.elapsed();
        let stages = diagnostics
            .observation_time
            .saturating_add(diagnostics.deduction_time)
            .saturating_add(diagnostics.policy_time)
            .saturating_add(diagnostics.apply_time);
        diagnostics.other_time = diagnostics.total_time.saturating_sub(stages);
    }
    Ok(RolloutReport {
        outcome,
        diagnostics,
    })
}

fn select_rollout_action<P: RolloutPolicy>(
    state: &FullState,
    policy: &P,
) -> Result<Action, RolloutError> {
    let actor = state.current_player();
    if policy.uses_history() {
        let view = state
            .view_for(actor)
            .ok_or(RolloutError::InvalidCurrentPlayer(actor))?;
        let deductions = LogicalDeductions::new(view).map_err(RolloutError::InformationSet)?;
        policy
            .select_action(&deductions)
            .map_err(RolloutError::Policy)
    } else {
        let observation = state
            .policy_observation_for(actor)
            .ok_or(RolloutError::InvalidCurrentPlayer(actor))?;
        let deductions =
            PolicyDeductions::new(&observation).map_err(RolloutError::InformationSet)?;
        policy
            .select_policy_action(&deductions)
            .map_err(RolloutError::Policy)
    }
}

fn select_rollout_action_with_diagnostics<P: RolloutPolicy>(
    state: &FullState,
    policy: &P,
    diagnostics: &mut RolloutDiagnostics,
) -> Result<Action, RolloutError> {
    let actor = state.current_player();
    let observation_started = Instant::now();
    if policy.uses_history() {
        let view = state
            .view_for(actor)
            .ok_or(RolloutError::InvalidCurrentPlayer(actor))?;
        diagnostics.observation_time += observation_started.elapsed();
        let deduction_started = Instant::now();
        let deductions = LogicalDeductions::new(view);
        diagnostics.deduction_time += deduction_started.elapsed();
        let deductions = deductions.map_err(RolloutError::InformationSet)?;
        let policy_started = Instant::now();
        let selected = policy.select_action(&deductions);
        diagnostics.policy_time += policy_started.elapsed();
        selected.map_err(RolloutError::Policy)
    } else {
        let observation = state
            .policy_observation_for(actor)
            .ok_or(RolloutError::InvalidCurrentPlayer(actor))?;
        diagnostics.observation_time += observation_started.elapsed();
        let deduction_started = Instant::now();
        let deductions = PolicyDeductions::new(&observation);
        diagnostics.deduction_time += deduction_started.elapsed();
        let deductions = deductions.map_err(RolloutError::InformationSet)?;
        let policy_started = Instant::now();
        let selected = policy.select_policy_action(&deductions);
        diagnostics.policy_time += policy_started.elapsed();
        selected.map_err(RolloutError::Policy)
    }
}

fn finish_outcome(
    state: FullState,
    actions: Vec<Action>,
    turns: usize,
) -> Result<RolloutOutcome, RolloutError> {
    let score = state
        .final_score()
        .ok_or(RolloutError::NonTerminalOutcome)?;
    Ok(RolloutOutcome {
        final_state: state,
        actions,
        turns,
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
