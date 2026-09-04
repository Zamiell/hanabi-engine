use core::{cmp::Ordering, fmt, str::FromStr};
use std::{borrow::Cow, collections::HashMap};

use hanabi_core::{Action, Clue, FullState, GameStatus, PlayerView, Rank, RuleError, Suit};

use crate::{
    ConventionAction, ConventionAnalysis, ConventionPolicyTier, EnumerateWorldsError,
    InformationSet, InformationSetError, LogicalDeductions, SupportedConvention, WorldCount,
    assess_card,
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
    /// Every convention-consistent identity world was used for an exhaustive
    /// solve or a mathematically conclusive terminal-action proof.
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
    pub policy_tier: ConventionPolicyTier,
    /// Convention ordering, derived solely from the legal observation.
    pub convention_priority: i32,
    pub certainly_playable: bool,
    pub certainly_useless: bool,
    pub newly_touched: u8,
    pub immediately_playable_touched: u8,
    pub critical_touched: u8,
    pub oldest_card_touched: bool,
    /// Convention-forced public continuation with unresolved draws kept blank.
    pub symbolic_line: SymbolicLineOutcome,
    pub exact: Option<ExactActionValue>,
}

/// Deterministic consequences reachable before a genuine choice or unknown
/// identity branch interrupts the projected line.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SymbolicLineOutcome {
    pub actions: u8,
    pub score_gain: u8,
    pub discards: u8,
    pub clues_spent: u8,
    pub clues_gained: u8,
    pub strikes: u8,
    pub stop_reason: SymbolicStopReason,
}

/// Why a deterministic symbolic continuation stopped.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SymbolicStopReason {
    /// The game ended; no subsequent action exists.
    Terminal,
    /// No convention-forced action remained.
    #[default]
    Choice,
    /// The next action depended on an unresolved card identity.
    UnknownIdentity,
    /// The configured symbolic-action bound was reached.
    Limit,
    /// A nested player perspective could not be reconstructed.
    ProjectionUnavailable,
}

impl SymbolicLineOutcome {
    fn compare(self, other: Self) -> Ordering {
        other
            .strikes
            .cmp(&self.strikes)
            .then_with(|| self.score_gain.cmp(&other.score_gain))
            .then_with(|| self.net_clues().cmp(&other.net_clues()))
            .then_with(|| other.discards.cmp(&self.discards))
            .then_with(|| self.actions.cmp(&other.actions))
    }

    fn net_clues(self) -> i16 {
        i16::from(self.clues_gained) - i16::from(self.clues_spent)
    }
}

/// Result of deterministic symbolic planning or an exhaustive endgame solve.
#[derive(Clone, Debug, PartialEq)]
pub struct PlannerResult {
    pub best_action: Action,
    pub phase: PlannerPhase,
    /// Exact belief size when known, otherwise the first count beyond the
    /// configured exact-world limit.
    pub world_count: WorldCount,
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
    let mut evaluations = symbolic_root_evaluations(deductions, convention, &candidates);

    let belief = &analysis.belief_constraints;
    let count = information_set.world_count_up_to(belief, config.exact_world_limit);
    let counted_worlds = count.worlds();
    if count == WorldCount::Exact(0) {
        return Err(PlannerError::ConventionBeliefConflict);
    }

    if count.is_exact()
        && counted_worlds > 0
        && objective == PlanningObjective::PerfectScore
        && information_set
            .view()
            .play_stacks
            .iter()
            .map(Vec::len)
            .sum::<usize>()
            == 24
    {
        let proof = try_terminal_perfect_proof(
            information_set,
            analysis,
            counted_worlds,
            &mut evaluations,
            preferred,
        )?;
        if let Some((best_index, tested_actions)) = proof {
            return Ok(PlannerResult {
                best_action: evaluations[best_index].action,
                phase: PlannerPhase::Exact,
                world_count: count,
                exact_nodes: tested_actions,
                root_actions: evaluations,
            });
        }
    }
    // With one admissible action, searching its continuations cannot change
    // the choice. Still validate current beliefs and retain terminal proofs
    // above; only omit the otherwise unnecessary exhaustive continuation.
    let full_exact_search = candidates.len() > 1
        && count.is_exact()
        && counted_worlds > 0
        && exact_preflight(
            information_set.view(),
            counted_worlds,
            candidates.len(),
            config.exact_node_limit,
        );
    if full_exact_search {
        let worlds = information_set
            .collect_worlds_after_count(
                belief,
                usize::try_from(counted_worlds).unwrap_or(usize::MAX),
            )
            .map_err(PlannerError::EnumerateWorlds)?;
        let mut budget = ExactBudget {
            used: 0,
            limit: config.exact_node_limit,
        };
        match evaluate_exact_root(
            &worlds,
            convention,
            objective,
            &evaluations,
            preferred,
            &mut budget,
        ) {
            Ok((values, proven)) => {
                for (evaluation, value) in evaluations.iter_mut().zip(values) {
                    evaluation.exact = value;
                }
                let best_index = proven
                    .or_else(|| best_exact_index(&evaluations, objective, preferred))
                    .ok_or(PlannerError::NoCandidateActions)?;
                return Ok(PlannerResult {
                    best_action: evaluations[best_index].action,
                    phase: PlannerPhase::Exact,
                    world_count: count,
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

    symbolic_result(evaluations, preferred, count)
}

fn symbolic_root_evaluations(
    deductions: &LogicalDeductions,
    convention: SupportedConvention,
    candidates: &[ConventionAction],
) -> Vec<PlannerActionEvaluation> {
    let mut evaluations = candidates
        .iter()
        .copied()
        .map(|action| symbolic_evaluation(deductions, action))
        .collect::<Vec<_>>();
    // Forced-line projection is comparatively expensive and is meaningful
    // only among candidates the convention already considers equivalent.
    // Semantic obligations and the convention's explicit preferred action
    // therefore remain authoritative.
    let best_priority = evaluations
        .iter()
        .map(|evaluation| evaluation.convention_priority)
        .max()
        .unwrap_or(i32::MIN);
    let best_priority_count = evaluations
        .iter()
        .filter(|evaluation| evaluation.convention_priority == best_priority)
        .count();
    if best_priority_count > 1 {
        for evaluation in &mut evaluations {
            if evaluation.convention_priority == best_priority {
                evaluation.symbolic_line =
                    convention.project_symbolic_line(deductions.view(), evaluation.action, 32);
            }
        }
    }
    evaluations
}

fn try_terminal_perfect_proof(
    information_set: &InformationSet,
    analysis: &ConventionAnalysis,
    world_count: u64,
    evaluations: &mut [PlannerActionEvaluation],
    preferred: Option<Action>,
) -> Result<Option<(usize, u64)>, PlannerError> {
    let worlds = information_set
        .collect_worlds_after_count(
            &analysis.belief_constraints,
            usize::try_from(world_count).unwrap_or(usize::MAX),
        )
        .map_err(PlannerError::EnumerateWorlds)?;
    prove_unanimous_terminal_perfect(&worlds, evaluations, preferred).map_err(PlannerError::Rule)
}

fn symbolic_result(
    evaluations: Vec<PlannerActionEvaluation>,
    preferred: Option<Action>,
    world_count: WorldCount,
) -> Result<PlannerResult, PlannerError> {
    let best_index =
        best_symbolic_index(&evaluations, preferred).ok_or(PlannerError::NoCandidateActions)?;
    Ok(PlannerResult {
        best_action: evaluations[best_index].action,
        phase: PlannerPhase::Symbolic,
        world_count,
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
                        policy_tier: ConventionPolicyTier::Required,
                        priority: 0,
                        reason: crate::ConventionActionReason::Fallback,
                    }),
            ])
        },
    )
}

fn exact_preflight(view: &PlayerView, worlds: u64, root_actions: usize, node_limit: u64) -> bool {
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

/// Finds a root play that ends every convention-consistent world at 25.
///
/// A unanimous perfect terminal result is globally maximal under the
/// perfect-score objective. Non-play actions cannot immediately increase a
/// score of 24, so only admitted plays need to be checked. Among multiple
/// proofs, ordinary convention ordering remains the deterministic tie-break.
fn prove_unanimous_terminal_perfect(
    worlds: &[FullState],
    evaluations: &mut [PlannerActionEvaluation],
    preferred: Option<Action>,
) -> Result<Option<(usize, u64)>, RuleError> {
    let mut tested_actions = 0_u64;
    let mut best: Option<(usize, i32, bool)> = None;
    for (index, evaluation) in evaluations.iter_mut().enumerate() {
        if !matches!(evaluation.action, Action::Play(_)) {
            continue;
        }
        tested_actions += 1;
        let mut value = ExactActionValue {
            worlds: 0,
            perfect_worlds: 0,
            score_sum: 0,
            strikeout_worlds: 0,
            score_ceiling_sum: 0,
        };
        let mut unanimous = true;
        for world in worlds {
            let mut advanced = world.clone();
            advanced.apply(evaluation.action)?;
            if advanced.final_score() != Some(25) {
                unanimous = false;
                break;
            }
            value.add(terminal_value(&advanced));
        }
        if !unanimous {
            continue;
        }
        evaluation.exact = Some(value);
        let priority = evaluation.convention_priority;
        let is_preferred = preferred == Some(evaluation.action);
        let replace =
            best.as_ref()
                .is_none_or(|(current_index, current_priority, current_preferred)| {
                    priority
                        .cmp(current_priority)
                        .then_with(|| is_preferred.cmp(current_preferred))
                        .then_with(|| current_index.cmp(&index))
                        == Ordering::Greater
                });
        if replace {
            best = Some((index, priority, is_preferred));
        }
    }
    Ok(best.map(|(index, _, _)| (index, tested_actions)))
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
        policy_tier: convention_action.policy_tier,
        convention_priority: convention_action.priority,
        certainly_playable: assessment.is_some_and(|value| value.certainly_playable),
        certainly_useless: assessment.is_some_and(|value| value.certainly_useless),
        newly_touched,
        immediately_playable_touched,
        critical_touched,
        oldest_card_touched,
        symbolic_line: SymbolicLineOutcome::default(),
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
            left.policy_tier
                .cmp(&right.policy_tier)
                .then_with(|| left.convention_priority.cmp(&right.convention_priority))
                .then_with(|| {
                    (preferred == Some(left.action)).cmp(&(preferred == Some(right.action)))
                })
                .then_with(|| left.symbolic_line.compare(right.symbolic_line))
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
                .then_with(|| left.policy_tier.cmp(&right.policy_tier))
                .then_with(|| left.convention_priority.cmp(&right.convention_priority))
                .then_with(|| {
                    (preferred == Some(left.action)).cmp(&(preferred == Some(right.action)))
                })
                .then_with(|| right_index.cmp(left_index))
        })
        .map(|(index, _)| index)
}

type ExactRootValues = (Vec<Option<ExactActionValue>>, Option<usize>);

fn evaluate_exact_root(
    worlds: &[FullState],
    convention: SupportedConvention,
    objective: PlanningObjective,
    candidates: &[PlannerActionEvaluation],
    preferred: Option<Action>,
    budget: &mut ExactBudget,
) -> Result<ExactRootValues, ExactAbort> {
    let mut values = vec![None; candidates.len()];
    let upper = exact_value_upper_bound(worlds);
    let mut ordered = candidates.iter().enumerate().collect::<Vec<_>>();
    ordered.sort_by_key(|(index, candidate)| {
        (
            core::cmp::Reverse(candidate.policy_tier),
            core::cmp::Reverse(candidate.convention_priority),
            core::cmp::Reverse(preferred == Some(candidate.action)),
            *index,
        )
    });
    // Exact branches repeatedly converge on the same public observation.
    // Convention interpretation is a pure function of that observation, so
    // compile it once for the whole solve instead of replaying H-Group history
    // independently in every identity world and branch.
    let mut analysis_cache = ConventionAnalysisCache::default();
    for (index, candidate) in ordered {
        budget.consume()?;
        let mut advanced = Vec::with_capacity(worlds.len());
        for world in worlds {
            let mut state = world.clone();
            state.apply(candidate.action).map_err(ExactAbort::Rule)?;
            advanced.push(state);
        }
        let value = solve_partitioned(
            advanced,
            convention,
            objective,
            budget,
            &mut analysis_cache,
            1,
        )?;
        values[index] = Some(value);
        if value.compare(upper, objective) == Ordering::Equal {
            return Ok((values, Some(index)));
        }
    }
    Ok((values, None))
}

fn solve_partitioned(
    worlds: Vec<FullState>,
    convention: SupportedConvention,
    objective: PlanningObjective,
    budget: &mut ExactBudget,
    analysis_cache: &mut ConventionAnalysisCache,
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
            view,
            &group,
            convention,
            objective,
            budget,
            analysis_cache,
            depth,
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
    analysis_cache: &mut ConventionAnalysisCache,
    depth: u16,
) -> Result<ExactActionValue, ExactAbort> {
    if let Some(value) = equivalent_terminal_actions(&view, worlds, objective)? {
        return Ok(value);
    }
    let analysis = analysis_cache.compile(view, convention)?;
    let preferred = analysis.preferred_action;
    let candidates = planning_candidates(&analysis);
    if candidates.is_empty() {
        return Err(ExactAbort::NoCandidateActions);
    }

    // No continuation can recover a lost stack or score above its current
    // ceiling. Visit tie-break-preferred actions first, so reaching this bound
    // proves that remaining candidates cannot improve either value or ties.
    let upper_bound = exact_value_upper_bound(worlds);
    let mut ordered = candidates.iter().copied().enumerate().collect::<Vec<_>>();
    ordered.sort_by_key(|(index, candidate)| {
        (
            core::cmp::Reverse(candidate.priority),
            core::cmp::Reverse(preferred == Some(candidate.action)),
            *index,
        )
    });
    let mut best: Option<(ExactActionValue, i32, bool, usize)> = None;
    for (index, candidate) in ordered {
        let action = candidate.action;
        budget.consume()?;
        let mut advanced = Vec::with_capacity(worlds.len());
        for world in worlds {
            let mut state = world.clone();
            state.apply(action).map_err(ExactAbort::Rule)?;
            advanced.push(state);
        }
        let value = solve_partitioned(
            advanced,
            convention,
            objective,
            budget,
            analysis_cache,
            depth + 1,
        )?;
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
        if value.compare(upper_bound, objective) == Ordering::Equal {
            break;
        }
    }
    best.map(|(value, _, _, _)| value)
        .ok_or(ExactAbort::NoCandidateActions)
}

/// Avoid convention compilation only when every rules-legal action ends every
/// world with the same utility. Any convention-admissible subset must then have
/// that value too; this does not relax the root player's convention constraints.
fn equivalent_terminal_actions(
    view: &PlayerView,
    worlds: &[FullState],
    objective: PlanningObjective,
) -> Result<Option<ExactActionValue>, ExactAbort> {
    if view.final_turns_remaining != Some(1) {
        return Ok(None);
    }
    let mut common: Option<ExactActionValue> = None;
    for action in view.legal_actions() {
        let mut value = ExactActionValue {
            worlds: 0,
            perfect_worlds: 0,
            score_sum: 0,
            strikeout_worlds: 0,
            score_ceiling_sum: 0,
        };
        for world in worlds {
            let mut advanced = world.clone();
            advanced.apply(action).map_err(ExactAbort::Rule)?;
            if !advanced.is_terminal() {
                return Ok(None);
            }
            value.add(terminal_value(&advanced));
        }
        if common.is_some_and(|previous| value.compare(previous, objective) != Ordering::Equal) {
            return Ok(None);
        }
        common = Some(value);
    }
    Ok(common)
}

/// Optimistic final-round score with omniscient players and free passes.
/// Each remaining player acts at most once and there are no further draws.
/// This is only an upper bound: actual decisions still share observations
/// across worlds and must obey their convention constraints.
fn exact_value_upper_bound(worlds: &[FullState]) -> ExactActionValue {
    let mut bound = ExactActionValue {
        worlds: 0,
        perfect_worlds: 0,
        score_sum: 0,
        strikeout_worlds: 0,
        score_ceiling_sum: 0,
    };
    for world in worlds {
        let ceiling = u64::from(score_ceiling(world));
        let reachable = final_round_score_bound(world).map_or(ceiling, |value| ceiling.min(value));
        bound.worlds += 1;
        bound.perfect_worlds += u64::from(reachable == 25);
        bound.score_sum += reachable;
        bound.score_ceiling_sum += ceiling;
    }
    bound
}

fn final_round_score_bound(world: &FullState) -> Option<u64> {
    let remaining = world.final_turns_remaining()?;
    let heights = world
        .play_stacks()
        .each_ref()
        .map(|stack| u8::try_from(stack.len()).expect("standard stack"));
    Some(
        u64::from(world.score())
            + u64::from(final_round_max_plays(
                world,
                heights,
                world.current_player().index(),
                remaining,
            )),
    )
}

fn final_round_max_plays(world: &FullState, heights: [u8; 5], actor: usize, remaining: u8) -> u8 {
    if remaining == 0 {
        return 0;
    }
    let next = (actor + 1) % world.hands().len();
    let mut best = final_round_max_plays(world, heights, next, remaining - 1);
    for card in &world.hands()[actor] {
        let identity = world.card(*card).expect("world hand card");
        if identity.rank.number() == heights[identity.suit.index()] + 1 {
            let mut advanced = heights;
            advanced[identity.suit.index()] += 1;
            best = best.max(1 + final_round_max_plays(world, advanced, next, remaining - 1));
        }
    }
    best
}

/// Per-solve cache for the pure observer-relative convention compiler.
/// Keeping this local to one exact search avoids global mutable state while
/// allowing identity-world branches with the same observation to share the
/// expensive history reduction.
#[derive(Default)]
struct ConventionAnalysisCache {
    entries: HashMap<PlayerView, ConventionAnalysis>,
    #[cfg(test)]
    compilations: usize,
}

impl ConventionAnalysisCache {
    fn compile(
        &mut self,
        view: PlayerView,
        convention: SupportedConvention,
    ) -> Result<ConventionAnalysis, ExactAbort> {
        if let Some(cached) = self.entries.get(&view) {
            return Ok(cached.clone());
        }
        let deductions =
            LogicalDeductions::new(view.clone()).map_err(ExactAbort::InformationSet)?;
        let compiled = convention.analyze(&deductions);
        self.entries.insert(view, compiled.clone());
        #[cfg(test)]
        {
            self.compilations += 1;
        }
        Ok(compiled)
    }
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
        assert!(!first.world_count.is_exact());
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
                    policy_tier: ConventionPolicyTier::Admitted,
                    priority: 900,
                    reason: crate::ConventionActionReason::PromisedPlay,
                },
                ConventionAction {
                    action: forced,
                    policy_tier: ConventionPolicyTier::Required,
                    priority: 400,
                    reason: crate::ConventionActionReason::PromisedPlay,
                },
            ],
            forced_action: Some(forced),
            ..ConventionAnalysis::default()
        };

        assert_eq!(
            planning_candidates(&analysis).as_ref(),
            &[ConventionAction {
                action: forced,
                policy_tier: ConventionPolicyTier::Required,
                priority: 400,
                reason: crate::ConventionActionReason::PromisedPlay,
            }]
        );
    }

    #[test]
    fn required_policy_tier_outweighs_a_larger_heuristic_number() {
        let state = FullState::new_standard(2, standard_deck()).unwrap();
        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
        let legal = deductions.view().legal_actions();
        let low_required = ConventionAction {
            action: legal[0],
            policy_tier: ConventionPolicyTier::Required,
            priority: 1,
            reason: crate::ConventionActionReason::PromisedPlay,
        };
        let high_admitted = ConventionAction {
            action: legal[1],
            policy_tier: ConventionPolicyTier::Admitted,
            priority: 10_000,
            reason: crate::ConventionActionReason::OtherClue,
        };
        let evaluations = [
            symbolic_evaluation(&deductions, low_required),
            symbolic_evaluation(&deductions, high_admitted),
        ];

        assert_eq!(best_symbolic_index(&evaluations, None), Some(0));
    }

    #[test]
    fn exact_solver_compiles_each_public_observation_once() {
        let state = FullState::new_standard(2, standard_deck()).unwrap();
        let view = state.view_for(PlayerId::new(0)).unwrap();
        let mut cache = ConventionAnalysisCache::default();

        cache
            .compile(view.clone(), SupportedConvention::None)
            .unwrap();
        cache.compile(view, SupportedConvention::None).unwrap();

        assert_eq!(cache.compilations, 1);
        assert_eq!(cache.entries.len(), 1);
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
        assert!(result.world_count.is_exact());
        let best = result
            .root_actions
            .iter()
            .find(|evaluation| evaluation.action == result.best_action)
            .unwrap()
            .exact
            .unwrap();
        if result
            .root_actions
            .iter()
            .any(|evaluation| evaluation.exact.is_none())
        {
            let worlds = information
                .collect_worlds_after_count(
                    &crate::BeliefConstraints::default(),
                    usize::try_from(result.world_count.worlds()).unwrap(),
                )
                .unwrap();
            assert_eq!(
                best,
                exact_value_upper_bound(&worlds),
                "unsearched roots require a proven global bound"
            );
        }
        let mut forced = SupportedConvention::None.analyze(information.deductions());
        forced.actions.truncate(1);
        let only_action = forced.actions[0].action;
        forced.preferred_action = Some(only_action);
        let single = plan_move_with_analysis(
            &information,
            SupportedConvention::None,
            &forced,
            PlannerConfig {
                exact_world_limit: 100_000,
                exact_node_limit: 1_000_000,
                ..PlannerConfig::default()
            },
        )
        .unwrap();
        assert_eq!(single.best_action, only_action);
        assert!(single.world_count.is_exact());
        assert_eq!(
            single.exact_nodes, 0,
            "a forced action needs no continuation search"
        );
        assert_eq!(single.phase, PlannerPhase::Symbolic);
    }
}
