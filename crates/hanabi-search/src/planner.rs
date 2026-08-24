use core::{cmp::Ordering, fmt, str::FromStr};
use std::{borrow::Cow, collections::HashMap};

use hanabi_core::{Action, Clue, FullState, GameStatus, PlayerView, Rank, RuleError, Suit};

use crate::{
    ConventionAction, ConventionAnalysis, EnumerateWorldsError, InformationSet,
    InformationSetError, LogicalDeductions, SupportedConvention, assess_card,
};

/// The result the planner should optimize during exact endgame analysis.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum PlanningObjective {
    /// Maximize expected official score.
    #[default]
    ExpectedScore,
    /// Maximize the chance of scoring 25 before preferring lesser outcomes.
    PerfectScore,
}

impl fmt::Display for PlanningObjective {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ExpectedScore => "expected-score",
            Self::PerfectScore => "perfect-score",
        })
    }
}

impl FromStr for PlanningObjective {
    type Err = ParsePlanningObjectiveError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "expected-score" => Ok(Self::ExpectedScore),
            "perfect-score" => Ok(Self::PerfectScore),
            _ => Err(ParsePlanningObjectiveError(value.to_owned())),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsePlanningObjectiveError(String);

impl fmt::Display for ParsePlanningObjectiveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "unknown planning objective {:?}; expected expected-score or perfect-score",
            self.0
        )
    }
}

impl std::error::Error for ParsePlanningObjectiveError {}

/// Highest score still reachable after accounting for exhausted identities.
#[must_use]
fn score_ceiling(state: &FullState) -> u8 {
    let mut discarded = [[0_u8; 5]; 5];
    for id in state.discard_pile() {
        if let Some(card) = state.card(*id) {
            discarded[card.suit.index()][card.rank.index()] += 1;
        }
    }
    Suit::ALL
        .iter()
        .map(|suit| {
            let played = state.play_stacks()[suit.index()].len();
            let blocked = Rank::ALL.iter().position(|rank| {
                rank.index() >= played && discarded[suit.index()][rank.index()] >= rank.copies()
            });
            u8::try_from(blocked.unwrap_or(5)).unwrap_or(5)
        })
        .sum()
}

/// Deterministic belief-state planner configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlannerConfig {
    pub objective: PlanningObjective,
    /// Maximum complete identity worlds admitted to the exact endgame.
    pub exact_world_limit: u64,
    /// Maximum observation-group/action nodes admitted to an exact solve.
    pub exact_node_limit: u64,
}

impl Default for PlannerConfig {
    fn default() -> Self {
        Self {
            objective: PlanningObjective::ExpectedScore,
            exact_world_limit: 4_096,
            exact_node_limit: 50_000,
        }
    }
}

/// Which representation produced a planner decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlannerPhase {
    /// Unknown identities stayed as constrained domains and public counts.
    Symbolic,
    /// Every convention-consistent identity world was solved exhaustively.
    Exact,
}

/// Exact outcome distribution for one root action.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExactActionValue {
    pub worlds: u64,
    pub perfect_worlds: u64,
    pub score_sum: u64,
    pub strikeout_worlds: u64,
    pub score_ceiling_sum: u64,
}

impl ExactActionValue {
    #[must_use]
    pub fn perfect_rate(self) -> f64 {
        ratio(self.perfect_worlds, self.worlds)
    }

    #[must_use]
    pub fn expected_score(self) -> f64 {
        ratio(self.score_sum, self.worlds)
    }

    #[must_use]
    pub fn strikeout_rate(self) -> f64 {
        ratio(self.strikeout_worlds, self.worlds)
    }

    #[must_use]
    pub fn expected_score_ceiling(self) -> f64 {
        ratio(self.score_ceiling_sum, self.worlds)
    }

    fn add(&mut self, other: Self) {
        self.worlds += other.worlds;
        self.perfect_worlds += other.perfect_worlds;
        self.score_sum += other.score_sum;
        self.strikeout_worlds += other.strikeout_worlds;
        self.score_ceiling_sum += other.score_ceiling_sum;
    }

    fn compare(self, other: Self, objective: PlanningObjective) -> Ordering {
        let primary = match objective {
            PlanningObjective::ExpectedScore => self
                .score_sum
                .cmp(&other.score_sum)
                .then_with(|| self.perfect_worlds.cmp(&other.perfect_worlds)),
            PlanningObjective::PerfectScore => self
                .perfect_worlds
                .cmp(&other.perfect_worlds)
                .then_with(|| self.score_sum.cmp(&other.score_sum)),
        };
        primary
            .then_with(|| other.strikeout_worlds.cmp(&self.strikeout_worlds))
            .then_with(|| self.score_ceiling_sum.cmp(&other.score_ceiling_sum))
    }
}

/// Deterministic evidence for one root candidate.
#[derive(Clone, Debug, PartialEq)]
pub struct PlannerActionEvaluation {
    pub action: Action,
    /// Convention ordering, derived solely from the legal observation.
    pub convention_priority: i32,
    pub certainly_playable: bool,
    pub certainly_useless: bool,
    pub newly_touched: u8,
    pub immediately_playable_touched: u8,
    pub critical_touched: u8,
    pub oldest_card_touched: bool,
    pub exact: Option<ExactActionValue>,
}

/// Result of deterministic symbolic planning or an exhaustive endgame solve.
#[derive(Clone, Debug, PartialEq)]
pub struct PlannerResult {
    pub best_action: Action,
    pub phase: PlannerPhase,
    /// Exact belief size when known, otherwise the first count beyond the
    /// configured exact-world limit.
    pub considered_worlds: u64,
    pub world_count_exact: bool,
    pub exact_nodes: u64,
    pub root_actions: Vec<PlannerActionEvaluation>,
}

/// Plans from known information without random world construction.
///
/// Early and midgame positions remain symbolic: hidden cards are identity
/// domains constrained by clues, convention inferences, and remaining copy
/// counts. Once the complete belief fits inside `exact_world_limit`, every
/// identity permutation is enumerated and solved. Exact recursion groups
/// worlds by the acting player's observation before choosing an action, so it
/// never conditions a decision on simulator truth that the player cannot see.
///
/// # Errors
///
/// Returns [`PlannerError`] for invalid observations, convention failures,
/// illegal transitions, or an unactionable position.
pub fn plan_move(
    information_set: &InformationSet,
    convention: SupportedConvention,
    config: PlannerConfig,
) -> Result<PlannerResult, PlannerError> {
    let deductions = information_set.deductions();
    let analysis = convention.analyze(deductions);
    plan_move_with_analysis(information_set, convention, &analysis, config)
}

pub(crate) fn plan_move_with_analysis(
    information_set: &InformationSet,
    convention: SupportedConvention,
    analysis: &ConventionAnalysis,
    config: PlannerConfig,
) -> Result<PlannerResult, PlannerError> {
    let objective = config.objective;
    let deductions = information_set.deductions();
    let candidates = planning_candidates(analysis);
    if candidates.is_empty() {
        return Err(PlannerError::NoCandidateActions);
    }
    let preferred = analysis.preferred_action;
    let mut evaluations = candidates
        .iter()
        .copied()
        .map(|action| symbolic_evaluation(deductions, action))
        .collect::<Vec<_>>();

    let belief = &analysis.belief_constraints;
    let count = information_set.world_count_up_to(belief, config.exact_world_limit);
    if count.exact && count.worlds == 0 {
        return Err(PlannerError::ConventionBeliefConflict);
    }

    if count.exact
        && count.worlds > 0
        && exact_preflight(
            information_set.view(),
            count.worlds,
            candidates.len(),
            config.exact_node_limit,
        )
    {
        let worlds = information_set
            .collect_worlds_after_count(belief, usize::try_from(count.worlds).unwrap_or(usize::MAX))
            .map_err(PlannerError::EnumerateWorlds)?;
        let mut budget = ExactBudget {
            used: 0,
            limit: config.exact_node_limit,
        };
        let candidate_actions = candidates
            .iter()
            .map(|candidate| candidate.action)
            .collect::<Vec<_>>();
        match evaluate_exact_root(
            &worlds,
            convention,
            objective,
            &candidate_actions,
            &mut budget,
        ) {
            Ok(values) => {
                for (evaluation, value) in evaluations.iter_mut().zip(values) {
                    evaluation.exact = Some(value);
                }
                let best_index = best_exact_index(&evaluations, objective, preferred)
                    .ok_or(PlannerError::NoCandidateActions)?;
                return Ok(PlannerResult {
                    best_action: evaluations[best_index].action,
                    phase: PlannerPhase::Exact,
                    considered_worlds: count.worlds,
                    world_count_exact: true,
                    exact_nodes: budget.used,
                    root_actions: evaluations,
                });
            }
            Err(ExactAbort::BudgetExceeded | ExactAbort::DepthExceeded) => {}
            Err(ExactAbort::InvalidCurrentPlayer | ExactAbort::NoCandidateActions) => {
                return Err(PlannerError::NoCandidateActions);
            }
            Err(ExactAbort::InformationSet(error)) => {
                return Err(PlannerError::InformationSet(error));
            }
            Err(ExactAbort::Rule(error)) => return Err(PlannerError::Rule(error)),
        }
    }

    let best_index =
        best_symbolic_index(&evaluations, preferred).ok_or(PlannerError::NoCandidateActions)?;
    Ok(PlannerResult {
        best_action: evaluations[best_index].action,
        phase: PlannerPhase::Symbolic,
        considered_worlds: count.worlds,
        world_count_exact: count.exact,
        exact_nodes: 0,
        root_actions: evaluations,
    })
}

/// Applies convention-forced continuations identically at the root and at
/// every exact observation group. Borrowing the normal action list avoids an
/// allocation on the common path.
fn planning_candidates(analysis: &ConventionAnalysis) -> Cow<'_, [ConventionAction]> {
    analysis.forced_action.map_or_else(
        || Cow::Borrowed(analysis.actions.as_slice()),
        |forced| {
            Cow::Owned(vec![
                analysis
                    .actions
                    .iter()
                    .find(|candidate| candidate.action == forced)
                    .copied()
                    .unwrap_or(ConventionAction {
                        action: forced,
                        priority: 0,
                    }),
            ])
        },
    )
}

fn exact_preflight(view: &PlayerView, worlds: u64, root_actions: usize, node_limit: u64) -> bool {
    // Exact search is intentionally an endgame algorithm. Before the last
    // draw, clue/discard cycles make even a small identity set produce a very
    // deep action tree.
    if view.deck_size > 1 {
        return false;
    }
    let remaining_turns = view.final_turns_remaining.map_or_else(
        || view.hands.len().saturating_add(view.deck_size),
        usize::from,
    );
    // A forced root can reveal several legal continuations. Eight is a
    // conservative floor without forbidding every tractable final-round solve.
    let branching = u64::try_from(root_actions.max(8)).unwrap_or(u64::MAX);
    let mut estimate = worlds;
    let mut frontier = worlds;
    for _ in 0..remaining_turns {
        frontier = frontier.saturating_mul(branching);
        estimate = estimate.saturating_add(frontier);
        if estimate > node_limit {
            return false;
        }
    }
    true
}

#[allow(clippy::cast_precision_loss)]
fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn symbolic_evaluation(
    deductions: &LogicalDeductions,
    convention_action: ConventionAction,
) -> PlannerActionEvaluation {
    let action = convention_action.action;
    let view = deductions.view();
    let assessment = match action {
        Action::Play(card) | Action::Discard(card) => assess_card(deductions, card),
        Action::Clue { .. } => None,
    };
    let (newly_touched, immediately_playable_touched, critical_touched, oldest_card_touched) =
        match action {
            Action::Clue { target, clue } => clue_effects(view, target.index(), clue),
            Action::Play(_) | Action::Discard(_) => (0, 0, 0, false),
        };
    PlannerActionEvaluation {
        action,
        convention_priority: convention_action.priority,
        certainly_playable: assessment.is_some_and(|value| value.certainly_playable),
        certainly_useless: assessment.is_some_and(|value| value.certainly_useless),
        newly_touched,
        immediately_playable_touched,
        critical_touched,
        oldest_card_touched,
        exact: None,
    }
}

fn clue_effects(view: &PlayerView, target: usize, clue: Clue) -> (u8, u8, u8, bool) {
    let Some(hand) = view.hands.get(target) else {
        return (0, 0, 0, false);
    };
    let oldest = hand.first().map(|card| card.id);
    hand.iter()
        .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
        .fold((0_u8, 0_u8, 0_u8, false), |value, card| {
            let (new, playable, critical, touched_oldest) = value;
            let identity = card.identity.expect("another player's hand is visible");
            (
                new + u8::from(!card.clues.has_positive_clue(clue)),
                playable
                    + u8::from(
                        identity.rank.number()
                            == u8::try_from(view.play_stacks[identity.suit.index()].len())
                                .expect("a standard stack has at most five cards")
                                + 1,
                    ),
                critical + u8::from(is_publicly_critical(view, identity)),
                touched_oldest || oldest == Some(card.id),
            )
        })
}

fn is_publicly_critical(view: &PlayerView, identity: hanabi_core::Card) -> bool {
    if view.play_stacks[identity.suit.index()].len() >= usize::from(identity.rank.number()) {
        return false;
    }
    let discarded = view
        .discard_pile
        .iter()
        .filter(|(_, card)| *card == identity)
        .count();
    discarded + 1 >= usize::from(identity.rank.copies())
}

fn best_symbolic_index(
    evaluations: &[PlannerActionEvaluation],
    preferred: Option<Action>,
) -> Option<usize> {
    evaluations
        .iter()
        .enumerate()
        .max_by(|(left_index, left), (right_index, right)| {
            left.convention_priority
                .cmp(&right.convention_priority)
                .then_with(|| {
                    (preferred == Some(left.action)).cmp(&(preferred == Some(right.action)))
                })
                .then_with(|| left.certainly_playable.cmp(&right.certainly_playable))
                .then_with(|| left.certainly_useless.cmp(&right.certainly_useless))
                .then_with(|| left.critical_touched.cmp(&right.critical_touched))
                .then_with(|| left.oldest_card_touched.cmp(&right.oldest_card_touched))
                .then_with(|| {
                    left.immediately_playable_touched
                        .cmp(&right.immediately_playable_touched)
                })
                .then_with(|| left.newly_touched.cmp(&right.newly_touched))
                // Stable candidate order wins exact ties.
                .then_with(|| right_index.cmp(left_index))
        })
        .map(|(index, _)| index)
}

fn best_exact_index(
    evaluations: &[PlannerActionEvaluation],
    objective: PlanningObjective,
    preferred: Option<Action>,
) -> Option<usize> {
    evaluations
        .iter()
        .enumerate()
        .max_by(|(left_index, left), (right_index, right)| {
            let exact = left
                .exact
                .expect("exact selection follows a complete root solve")
                .compare(
                    right
                        .exact
                        .expect("exact selection follows a complete root solve"),
                    objective,
                );
            exact
                .then_with(|| left.convention_priority.cmp(&right.convention_priority))
                .then_with(|| {
                    (preferred == Some(left.action)).cmp(&(preferred == Some(right.action)))
                })
                .then_with(|| right_index.cmp(left_index))
        })
        .map(|(index, _)| index)
}

fn evaluate_exact_root(
    worlds: &[FullState],
    convention: SupportedConvention,
    objective: PlanningObjective,
    candidates: &[Action],
    budget: &mut ExactBudget,
) -> Result<Vec<ExactActionValue>, ExactAbort> {
    let mut values = Vec::with_capacity(candidates.len());
    for action in candidates {
        budget.consume()?;
        let mut advanced = Vec::with_capacity(worlds.len());
        for world in worlds {
            let mut state = world.clone();
            state.apply(*action).map_err(ExactAbort::Rule)?;
            advanced.push(state);
        }
        values.push(solve_partitioned(
            advanced, convention, objective, budget, 1,
        )?);
    }
    Ok(values)
}

fn solve_partitioned(
    worlds: Vec<FullState>,
    convention: SupportedConvention,
    objective: PlanningObjective,
    budget: &mut ExactBudget,
    depth: u16,
) -> Result<ExactActionValue, ExactAbort> {
    if depth > 512 {
        return Err(ExactAbort::DepthExceeded);
    }
    let mut terminal = ExactActionValue {
        worlds: 0,
        perfect_worlds: 0,
        score_sum: 0,
        strikeout_worlds: 0,
        score_ceiling_sum: 0,
    };
    let mut groups: Vec<(PlayerView, Vec<FullState>)> = Vec::new();
    let mut group_indices: HashMap<PlayerView, usize> = HashMap::new();
    for world in worlds {
        if world.is_terminal() {
            terminal.add(terminal_value(&world));
            continue;
        }
        let view = world
            .view_for(world.current_player())
            .ok_or(ExactAbort::InvalidCurrentPlayer)?;
        if let Some(index) = group_indices.get(&view).copied() {
            groups[index].1.push(world);
        } else {
            group_indices.insert(view.clone(), groups.len());
            groups.push((view, vec![world]));
        }
    }
    for (view, group) in groups {
        terminal.add(solve_observation_group(
            view, &group, convention, objective, budget, depth,
        )?);
    }
    Ok(terminal)
}

fn solve_observation_group(
    view: PlayerView,
    worlds: &[FullState],
    convention: SupportedConvention,
    objective: PlanningObjective,
    budget: &mut ExactBudget,
    depth: u16,
) -> Result<ExactActionValue, ExactAbort> {
    let deductions = LogicalDeductions::new(view).map_err(ExactAbort::InformationSet)?;
    let analysis = convention.analyze(&deductions);
    let preferred = analysis.preferred_action;
    let candidates = planning_candidates(&analysis);
    if candidates.is_empty() {
        return Err(ExactAbort::NoCandidateActions);
    }

    let mut best: Option<(ExactActionValue, i32, bool, usize)> = None;
    for (index, candidate) in candidates.iter().copied().enumerate() {
        let action = candidate.action;
        budget.consume()?;
        let mut advanced = Vec::with_capacity(worlds.len());
        for world in worlds {
            let mut state = world.clone();
            state.apply(action).map_err(ExactAbort::Rule)?;
            advanced.push(state);
        }
        let value = solve_partitioned(advanced, convention, objective, budget, depth + 1)?;
        let priority = candidate.priority;
        let is_preferred = preferred == Some(action);
        let replace = best.as_ref().is_none_or(
            |(current, current_priority, current_preferred, current_index)| {
                value
                    .compare(*current, objective)
                    .then_with(|| priority.cmp(current_priority))
                    .then_with(|| is_preferred.cmp(current_preferred))
                    .then_with(|| current_index.cmp(&index))
                    == Ordering::Greater
            },
        );
        if replace {
            best = Some((value, priority, is_preferred, index));
        }
    }
    best.map(|(value, _, _, _)| value)
        .ok_or(ExactAbort::NoCandidateActions)
}

fn terminal_value(state: &FullState) -> ExactActionValue {
    let score = state
        .final_score()
        .expect("terminal states have an official score");
    ExactActionValue {
        worlds: 1,
        perfect_worlds: u64::from(score == 25),
        score_sum: u64::from(score),
        strikeout_worlds: u64::from(matches!(
            state.status(),
            GameStatus::Finished(hanabi_core::EndReason::TooManyStrikes)
        )),
        score_ceiling_sum: u64::from(score_ceiling(state)),
    }
}

struct ExactBudget {
    used: u64,
    limit: u64,
}

impl ExactBudget {
    fn consume(&mut self) -> Result<(), ExactAbort> {
        if self.used >= self.limit {
            return Err(ExactAbort::BudgetExceeded);
        }
        self.used += 1;
        Ok(())
    }
}

#[derive(Debug)]
enum ExactAbort {
    BudgetExceeded,
    DepthExceeded,
    InvalidCurrentPlayer,
    NoCandidateActions,
    InformationSet(InformationSetError),
    Rule(RuleError),
}

/// Failure returned by deterministic planning.
#[derive(Debug, PartialEq)]
pub enum PlannerError {
    ConventionBeliefConflict,
    NoCandidateActions,
    EnumerateWorlds(EnumerateWorldsError),
    InformationSet(InformationSetError),
    Rule(RuleError),
}

impl fmt::Display for PlannerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConventionBeliefConflict => formatter.write_str(
                "convention identity constraints contradict the logical information set",
            ),
            Self::NoCandidateActions => formatter.write_str("position has no candidate actions"),
            Self::EnumerateWorlds(error) => write!(formatter, "cannot enumerate belief: {error}"),
            Self::InformationSet(error) => write!(formatter, "invalid observation: {error}"),
            Self::Rule(error) => write!(formatter, "planned action was illegal: {error}"),
        }
    }
}

impl std::error::Error for PlannerError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SupportedConvention;
    use hanabi_core::{PlayerId, standard_deck};

    #[test]
    fn opening_planning_is_deterministic_and_symbolic() {
        let state = FullState::new_standard(2, standard_deck()).unwrap();
        let information = InformationSet::new(&state.view_for(PlayerId::new(0)).unwrap()).unwrap();
        let first = plan_move(
            &information,
            SupportedConvention::None,
            PlannerConfig::default(),
        )
        .unwrap();
        let second = plan_move(
            &information,
            SupportedConvention::None,
            PlannerConfig::default(),
        )
        .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.phase, PlannerPhase::Symbolic);
        assert!(!first.world_count_exact);
        assert_eq!(first.best_action, Action::Play(hanabi_core::CardId::new(4)));
    }

    #[test]
    fn forced_continuation_is_the_only_planning_candidate() {
        let first = Action::Play(hanabi_core::CardId::new(0));
        let forced = Action::Play(hanabi_core::CardId::new(1));
        let analysis = ConventionAnalysis {
            actions: vec![
                ConventionAction {
                    action: first,
                    priority: 900,
                },
                ConventionAction {
                    action: forced,
                    priority: 400,
                },
            ],
            forced_action: Some(forced),
            ..ConventionAnalysis::default()
        };

        assert_eq!(
            planning_candidates(&analysis).as_ref(),
            &[ConventionAction {
                action: forced,
                priority: 400,
            }]
        );
    }

    #[test]
    fn exact_solver_respects_observation_groups() {
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
                        .find(|action| matches!(action, Action::Discard(_)))
                        .unwrap_or_else(|| {
                            state
                                .legal_actions()
                                .into_iter()
                                .find(|action| matches!(action, Action::Clue { .. }))
                                .unwrap()
                        })
                },
                |card| Action::Play(*card),
            );
            state.apply(action).unwrap();
        }
        assert!(!state.is_terminal());
        let information =
            InformationSet::new(&state.view_for(state.current_player()).unwrap()).unwrap();
        let result = plan_move(
            &information,
            SupportedConvention::None,
            PlannerConfig {
                objective: PlanningObjective::ExpectedScore,
                exact_world_limit: 100_000,
                exact_node_limit: 1_000_000,
            },
        )
        .unwrap();
        assert_eq!(result.phase, PlannerPhase::Exact);
        assert!(result.world_count_exact);
        assert!(
            result
                .root_actions
                .iter()
                .all(|evaluation| evaluation.exact.is_some())
        );
    }
}
