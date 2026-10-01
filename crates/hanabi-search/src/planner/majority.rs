//! Observer-weighted choice of a teammate's conditional response. Probabilities
//! change a forecast choice only; they never supply hidden card identities.
use super::{
    CandidateComparison, ComparisonAuthority, ComparisonBasis, ComparisonReason,
    EndpointComparison, PlannerActionEvaluation,
};
use crate::{
    AnalysisControl, AnalysisStopped, BeliefConstraints, HGroupProfile, LogicalDeductions,
};
use hanabi_core::{Action, Card, CardId, PlayerView, Rank};

const ASSIGNMENT_LIMIT: usize = 100_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Coverage {
    pub covered: u64,
    pub total: u64,
}
impl Coverage {
    fn majority(self) -> bool {
        self.covered > self.total / 2
    }
}

/// Uniform physical-card dealing conditioned on logical/convention domains,
/// disjunctive beliefs and explicitly supplied Good Touch inequalities.
/// Incomplete enumeration is not a probability estimate.
pub(super) fn probability(
    deductions: &LogicalDeductions,
    belief: &BeliefConstraints,
    distinct: &[(CardId, CardId)],
    identity: Card,
    control: &AnalysisControl,
) -> Result<Option<Coverage>, AnalysisStopped> {
    let view = deductions.view();
    if view.hands[view.observer.index()]
        .iter()
        .any(|card| card.identity.is_some())
    {
        return Ok(None);
    }
    // Blank future draws in other hands have their own constraints. Do not
    // silently integrate them as an unconstrained deck.
    if view.hands.iter().enumerate().any(|(player, hand)| {
        player != view.observer.index() && hand.iter().any(|card| card.identity.is_none())
    }) {
        return Ok(None);
    }
    let mut counts = [0_u64; 25];
    for suit in hanabi_core::Suit::ALL {
        for rank in Rank::ALL {
            counts[Card::new(suit, rank).index()] = u64::from(rank.copies());
        }
    }
    for card in view
        .hands
        .iter()
        .flatten()
        .filter_map(|c| c.identity)
        .chain(view.play_stacks.iter().flatten().map(|(_, c)| *c))
        .chain(view.discard_pile.iter().map(|(_, c)| *c))
    {
        counts[card.index()] -= 1;
    }
    let mut mass = Coverage {
        covered: 0,
        total: 0,
    };
    let mut stopped = None;
    let result = deductions.visit_hand_assignments(ASSIGNMENT_LIMIT, |assignment| {
        if let Err(error) = control.checkpoint() {
            stopped = Some(error);
            return true;
        }
        let assigned = |id| {
            assignment
                .iter()
                .find(|(card, _)| *card == id)
                .map(|(_, c)| *c)
                .or_else(|| {
                    view.hands
                        .iter()
                        .flatten()
                        .find(|c| c.id == id)
                        .and_then(|c| c.identity)
                })
        };
        let admits = |constraints: &[(CardId, crate::IdentitySet)]| {
            constraints
                .iter()
                .all(|(id, domain)| assigned(*id).is_none_or(|c| domain.contains(c)))
        };
        if !admits(&belief.constraints)
            || !belief
                .factors
                .iter()
                .all(|factor| factor.iter().any(|branch| admits(branch)))
            || distinct
                .iter()
                .any(|(a, b)| assigned(*a).zip(assigned(*b)).is_some_and(|(a, b)| a == b))
        {
            return false;
        }
        let mut remaining = counts;
        let mut weight = 1;
        for (_, card) in assignment {
            weight *= remaining[card.index()];
            remaining[card.index()] -= 1;
        }
        mass.total += weight;
        if assignment.iter().any(|(_, c)| *c == identity) {
            mass.covered += weight;
        }
        false
    });
    if let Some(error) = stopped {
        return Err(error);
    }
    Ok(
        (result.end == crate::information_set::HandAssignmentVisitEnd::Exhausted && mass.total > 0)
            .then_some(mass),
    )
}

/// The certain counterpart of the reviewed conditional response. Use the
/// same scheduling tests at the root and in forecasts, without inventing an
/// unknown replacement or turning a majority into guaranteed protection.
pub(super) fn known_coverage(
    source: &PlayerView,
    left: &PlannerActionEvaluation,
    right: &PlannerActionEvaluation,
    basis: &mut Option<ComparisonBasis>,
) -> Option<EndpointComparison> {
    if super::has_unconditional_perfect_finish(left)
        || super::has_unconditional_perfect_finish(right)
        || left.preference.advances_terminal_plan() != right.preference.advances_terminal_plan()
        || left.preference.compare_play_order(right.preference) != core::cmp::Ordering::Equal
    {
        return None;
    }
    let prefers = |play: &PlannerActionEvaluation, clue: &PlannerActionEvaluation| {
        let Action::Clue { target, .. } = clue.action else {
            return false;
        };
        if play.projection.maximum_save_violations() != 0
            || play.projection.maximum_bottom_deck_risks() != 0
            || play.projection.critical_losses_at(usize::MAX) != 0
            || !eligible(source, play, clue, target)
        {
            return false;
        }
        let Some((identity, loss_turn)) = replacement_at(source, play, &[]) else {
            return false;
        };
        source.hands.iter().flatten().any(|card| {
            card.identity == Some(identity)
                && (card.clues.has_positive_clue(hanabi_core::Clue::Suit(identity.suit))
                    || card.clues.has_positive_clue(hanabi_core::Clue::Rank(identity.rank)))
                && !play.projection.steps.iter().any(|step| {
                    step.turn <= loss_turn
                        && matches!(step.projected.action, Action::Play(id) | Action::Discard(id) if id == card.id)
                })
        })
    };
    let result = if prefers(left, right) {
        EndpointComparison::PreferLeft(ComparisonReason::KnownCoverageScheduling)
    } else if prefers(right, left) {
        EndpointComparison::PreferRight(ComparisonReason::KnownCoverageScheduling)
    } else {
        return None;
    };
    let horizon = left
        .projection
        .common_horizon()
        .min(right.projection.common_horizon());
    super::retain_comparison_basis(
        basis,
        "knownCoverageScheduling",
        usize::from(horizon),
        || left.projection.checkpoints_at(horizon),
        || right.projection.checkpoints_at(horizon),
    );
    Some(result)
}

pub(super) fn choose(
    source: &PlayerView,
    actor: &PlayerView,
    profile: HGroupProfile,
    evaluations: &[PlannerActionEvaluation],
    best: usize,
    comparisons: &mut Vec<CandidateComparison>,
    control: &AnalysisControl,
) -> Result<Option<usize>, AnalysisStopped> {
    if source.observer == actor.observer {
        return Ok(None);
    }
    let clue = &evaluations[best];
    let Action::Clue { target, .. } = clue.action else {
        return Ok(None);
    };
    let cases: Vec<_> = evaluations
        .iter()
        .enumerate()
        .filter_map(|(index, play)| {
            if eligible(source, play, clue, target) {
                Some((index, play.clone()))
            } else {
                covered_policy_case(source, play, clue, target).map(|case| (index, case))
            }
        })
        .collect();
    if cases.is_empty() {
        return Ok(None);
    }
    let Ok(deductions) = LogicalDeductions::new(source.clone()) else {
        return Ok(None);
    };
    let analysis = crate::SupportedConvention::HGroup(profile).analyze(&deductions);
    let crate::ConventionInferences::HGroup(inferred) = &analysis.inferences else {
        return Ok(None);
    };
    // Ordinary rank Play Clues promise useful, distinct identities. Only use
    // first-touch collateral; Saves, re-clues and trash collateral do not
    // justify this relation. This conditions a probability, not literal notes.
    let distinct: Vec<_> = inferred
        .clues
        .iter()
        .filter(|c| {
            c.target == source.observer
                && c.kind == crate::HGroupClueKind::Play
                && matches!(c.clue, hanabi_core::Clue::Rank(_))
                && c.non_focus_trash_identities.is_empty()
                && !c.previously_gotten.contains(&c.focus)
        })
        .flat_map(|c| {
            let cards: Vec<_> = core::iter::once(c.focus)
                .chain(c.new_non_focus.iter().copied())
                .collect();
            let mut pairs = Vec::new();
            for (index, a) in cards.iter().enumerate() {
                for b in &cards[index + 1..] {
                    pairs.push((*a, *b));
                }
            }
            pairs
        })
        .collect();
    for (index, play) in cases {
        let losses: Vec<_> = play
            .projection
            .steps
            .iter()
            .filter(|s| {
                s.consequences.save_principle_violation.is_some()
                    || s.consequences.bottom_deck_risk.is_some()
            })
            .collect();
        let Some((identity, loss_turn)) = replacement_at(source, &play, &losses) else {
            continue;
        };
        // A single probabilistic duplicate cannot excuse unrelated losses or
        // critical-card sacrifice, and cannot replace an urgent root action.
        if losses.iter().any(|s| {
            s.consequences.bottom_deck_risk != Some(identity)
                || s.consequences.save_principle_violation
                    == Some(crate::SavePrincipleViolation::CriticalCard)
        }) {
            continue;
        }
        // Coverage must still be in the observer's hand when the discard
        // occurs. Do not reuse a possible replacement already played/discarded.
        if play.projection.steps.iter().any(|s| {
            s.turn < loss_turn
                && s.projected.actor == source.observer
                && matches!(s.projected.action, Action::Play(_) | Action::Discard(_))
        }) {
            continue;
        }
        let Some(chance) = probability(
            &deductions,
            &analysis.belief_constraints,
            &distinct,
            identity,
            control,
        )?
        else {
            continue;
        };
        if !chance.majority() {
            continue;
        }
        record_choice(comparisons, &play, clue, identity, chance);
        return Ok(Some(index));
    }
    Ok(None)
}

/// A covered private world can choose the play even when another world must
/// spend a clue to protect the chop. Keep that response separate from the
/// aggregate's common prefix; it may contain no discard at all.
fn covered_policy_case(
    source: &PlayerView,
    play: &PlannerActionEvaluation,
    clue: &PlannerActionEvaluation,
    target: hanabi_core::PlayerId,
) -> Option<PlannerActionEvaluation> {
    if !matches!(play.action, Action::Play(_))
        || play.projection.maximum_strikes() != 0
        || play.projection.critical_losses_at(usize::MAX) != 0
    {
        return None;
    }
    let owns = |id| {
        source.hands[source.observer.index()]
            .iter()
            .any(|c| c.id == id)
    };
    for branch in &play.projection.private_branches {
        let mut case = play.clone();
        case.projection = branch.continuation.clone();
        let Some((identity, _)) = replacement_at(source, &case, &[]) else {
            continue;
        };
        let covered = |b: &&crate::PrivateHandBranch| {
            b.assignments
                .iter()
                .any(|(id, card)| owns(*id) && *card == identity)
        };
        if !covered(&branch) {
            continue;
        }
        let all_covered = play
            .projection
            .private_branches
            .iter()
            .filter(covered)
            .all(|b| {
                let mut conditional = play.clone();
                conditional.projection = b.continuation.clone();
                let mut visible = source.clone();
                for (id, identity) in &b.assignments {
                    for held in visible.hands.iter_mut().flatten().filter(|c| c.id == *id) {
                        held.identity = Some(*identity);
                    }
                }
                conditional.projection.maximum_save_violations() == 0
                    && conditional.projection.maximum_bottom_deck_risks() == 0
                    && eligible(&visible, &conditional, clue, target)
            });
        if all_covered {
            return Some(case);
        }
    }
    None
}

fn replacement_at(
    source: &PlayerView,
    play: &PlannerActionEvaluation,
    losses: &[&crate::PlanStep],
) -> Option<(Card, u32)> {
    if let Some(step) = losses.first() {
        return step.consequences.bottom_deck_risk.map(|id| (id, step.turn));
    }
    // At probability one, the perspective projector may already export the
    // inferred replacement. Its safe discard has no loss annotation; it still
    // belongs to the same coverage/scheduling comparison.
    play.projection.steps.iter().find_map(|step| {
        let Action::Discard(id) = step.projected.action else {
            return None;
        };
        let card = source
            .hands
            .iter()
            .flatten()
            .find(|c| c.id == id)?
            .identity?;
        (source.play_stacks[card.suit.index()].len() < usize::from(card.rank.number()))
            .then_some((card, step.turn))
    })
}

fn record_choice(
    comparisons: &mut Vec<CandidateComparison>,
    play: &PlannerActionEvaluation,
    clue: &PlannerActionEvaluation,
    identity: Card,
    chance: Coverage,
) {
    comparisons.retain(|c| {
        !((c.left == play.action && c.right == clue.action)
            || (c.right == play.action && c.left == clue.action))
    });
    comparisons.push(CandidateComparison {
        authority: ComparisonAuthority::Heuristic,
        basis: Some(ComparisonBasis {
            stage: "majorityCoverage",
            horizon: usize::from(
                play.projection
                    .common_horizon()
                    .min(clue.projection.common_horizon()),
            ),
            left: Vec::new(),
            right: Vec::new(),
            clue_cost_bounds: None,
            scheduled_refunds: None,
            coverage_probability: Some((identity, chance.covered, chance.total)),
        }),
        left: play.action,
        right: clue.action,
        endpoint: EndpointComparison::Incomparable,
        preferred: play.action,
        reason: ComparisonReason::MajorityCoverage,
        in_cycle: false,
    });
}

fn eligible(
    source: &PlayerView,
    play: &PlannerActionEvaluation,
    clue: &PlannerActionEvaluation,
    target: hanabi_core::PlayerId,
) -> bool {
    if !matches!(play.action, Action::Play(_))
        || play.preference.policy_tier() != clue.preference.policy_tier()
    {
        return false;
    }
    let Some(first) = play.projection.steps.first() else {
        return false;
    };
    if first.consequences.score_gain != 1
        || !(play.certainly_playable
            || first
                .interpreted_identities
                .is_some_and(|ids| ids.len() == 1))
        || play.projection.maximum_strikes() != 0
        || clue.projection.maximum_strikes() != 0
        || !play
            .projection
            .steps
            .iter()
            .skip(1)
            .take_while(|s| s.projected.actor != first.projected.actor)
            .any(|s| s.projected.actor == target && s.consequences.score_gain == 1)
    {
        return false;
    }
    let horizon = play
        .projection
        .common_horizon()
        .min(clue.projection.common_horizon());
    let accelerated: Vec<_> = clue
        .projection
        .steps
        .iter()
        .take(usize::from(horizon))
        .filter(|s| {
            s.consequences.score_gain == 1
                && !play
                    .projection
                    .steps
                    .iter()
                    .take(usize::from(horizon))
                    .any(|p| p.projected.action == s.projected.action)
        })
        .collect();
    if accelerated.is_empty()
        || accelerated.iter().any(|s| {
            let Action::Play(id) = s.projected.action else {
                return true;
            };
            let Some(card) = source
                .hands
                .iter()
                .flatten()
                .find(|c| c.id == id)
                .and_then(|c| c.identity)
            else {
                return true;
            };
            card.rank == Rank::Five
                || source.hands.iter().flatten().any(|c| {
                    c.identity.is_some_and(|next| {
                        next.suit == card.suit && next.rank.number() == card.rank.number() + 1
                    })
                })
        })
    {
        return false;
    }
    let a = play.projection.checkpoints_at(horizon);
    let b = clue.projection.checkpoints_at(horizon);
    !a.is_empty()
        && !b.is_empty()
        && a.iter().all(|a| {
            b.iter().all(|b| {
                a.value.score + a.value.committed_future_plays
                    == b.value.score + b.value.committed_future_plays
                    && a.value.score + a.value.secured_future_plays
                        >= b.value.score + b.value.secured_future_plays
                    && a.value.clues > b.value.clues
                    && a.value.clues >= a.value.clue_demand
                    && a.value.clue_demand <= b.value.clue_demand
                    && a.value.exposed_critical_chops <= b.value.exposed_critical_chops
                    && a.value.save_pressure <= b.value.save_pressure
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hanabi_core::{Clue, PlayerId, Suit};

    fn reviewed_source() -> PlayerView {
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s9.json"
        ))
        .unwrap();
        replay
            .state_at_turn(6)
            .unwrap()
            .view_for(PlayerId::new(1))
            .unwrap()
    }

    #[test]
    fn reviewed_coverage_probability_weights_physical_copies() {
        let source = reviewed_source();
        let d = LogicalDeductions::new(source).unwrap();
        let id = Card::new(Suit::Red, Rank::Three);
        let control = AnalysisControl::default();
        let literal = probability(&d, &BeliefConstraints::default(), &[], id, &control)
            .unwrap()
            .unwrap();
        assert_eq!(literal.covered * 11, literal.total * 3);
        let good_touch = probability(
            &d,
            &BeliefConstraints::default(),
            &[(CardId::new(4), CardId::new(5))],
            id,
            &control,
        )
        .unwrap()
        .unwrap();
        assert_eq!(good_touch.covered * 275, good_touch.total * 83);
        assert!(!good_touch.majority());
        assert!(
            !Coverage {
                covered: 1,
                total: 2
            }
            .majority()
        );
        assert!(
            Coverage {
                covered: 2,
                total: 3
            }
            .majority()
        );
    }

    #[test]
    fn incomplete_coverage_is_not_a_majority_estimate() {
        let mut source = reviewed_source();
        for held in &mut source.hands[1] {
            held.clues = hanabi_core::ClueFacts::default();
        }
        let id = Card::new(Suit::Red, Rank::Three);
        let d = LogicalDeductions::new(source.clone()).unwrap();
        assert_eq!(
            probability(
                &d,
                &BeliefConstraints::default(),
                &[],
                id,
                &AnalysisControl::default()
            )
            .unwrap(),
            None
        );
        assert_eq!(
            probability(
                &d,
                &BeliefConstraints::default(),
                &[],
                id,
                &AnalysisControl::new(crate::CancellationToken::default(), None, 0)
            ),
            Err(AnalysisStopped::WorkLimit)
        );
        source.hands[0][0].identity = None;
        let d = LogicalDeductions::new(source).unwrap();
        assert_eq!(
            probability(
                &d,
                &BeliefConstraints::default(),
                &[],
                id,
                &AnalysisControl::default()
            )
            .unwrap(),
            None
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn reviewed_visible_coverage_applies_to_root_and_forecast() {
        // p4v0s9 turn7: Cathy sees Bob's touched r3. The reviewed conditional
        // policy says to play p1 in this covered world, as well as when Bob's
        // forecast gives that world a strict majority.
        let fixture = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s9.json"
        ))
        .unwrap();
        let source = fixture
            .state_at_turn(6)
            .unwrap()
            .view_for(PlayerId::new(2))
            .unwrap();
        let result = crate::analyze_position(
            &source,
            crate::SupportedConvention::HGroup(HGroupProfile::Max),
            crate::PlannerConfig {
                objective: crate::PlanningObjective::PerfectScore,
                ..crate::PlannerConfig::default()
            },
        )
        .unwrap();
        let play_action = Action::Play(CardId::new(10));
        assert_eq!(result.planner.best_action, play_action);
        let play = result
            .planner
            .root_actions
            .iter()
            .find(|c| c.action == play_action)
            .unwrap();
        let clue = result
            .planner
            .root_actions
            .iter()
            .find(|c| {
                c.action
                    == Action::Clue {
                        target: PlayerId::new(0),
                        clue: Clue::Suit(Suit::Blue),
                    }
            })
            .unwrap();
        assert!(
            result
                .planner
                .comparisons
                .iter()
                .any(|c| c.reason == ComparisonReason::KnownCoverageScheduling
                    && c.preferred == play_action)
        );
        assert_eq!(play.projection.maximum_save_violations(), 0);
        assert_eq!(play.projection.maximum_bottom_deck_risks(), 0);
        let mut basis = None;
        let (known, _) =
            crate::capture_decisions(|| known_coverage(&source, play, clue, &mut basis));
        assert_eq!(
            known,
            Some(EndpointComparison::PreferLeft(
                ComparisonReason::KnownCoverageScheduling
            ))
        );
        assert_eq!(basis.unwrap().horizon, 8);
        assert_eq!(
            known_coverage(&source, clue, play, &mut None),
            Some(EndpointComparison::PreferRight(
                ComparisonReason::KnownCoverageScheduling
            ))
        );
        let d = LogicalDeductions::new(source.clone()).unwrap();
        assert_eq!(
            super::super::choose_projected_follow_up_from(
                &source,
                &d,
                HGroupProfile::Max,
                &AnalysisControl::default()
            )
            .unwrap(),
            Some(play_action)
        );
        for control in 0..4 {
            let mut masked = source.clone();
            match control {
                0 => masked.hands[1][1].identity = None,
                1 => masked.hands[1][1].clues = hanabi_core::ClueFacts::default(),
                2 => masked.hands[2][0].identity = Some(Card::new(Suit::Blue, Rank::Three)),
                _ => {
                    masked.hands[3]
                        .iter_mut()
                        .find(|c| c.id == CardId::new(15))
                        .unwrap()
                        .identity = Some(Card::new(Suit::Blue, Rank::Five));
                }
            }
            assert_eq!(
                known_coverage(&masked, play, clue, &mut None),
                None,
                "control {control}"
            );
        }
        let mut unsafe_play = play.clone();
        unsafe_play.projection.steps[5]
            .consequences
            .bottom_deck_risk = Some(Card::new(Suit::Red, Rank::Three));
        assert_eq!(known_coverage(&source, &unsafe_play, clue, &mut None), None);
        unsafe_play = play.clone();
        unsafe_play.projection.steps[0].consequences.strikes = 1;
        assert_eq!(known_coverage(&source, &unsafe_play, clue, &mut None), None);
        unsafe_play = play.clone();
        unsafe_play.projection.checkpoints.clear();
        assert_eq!(known_coverage(&source, &unsafe_play, clue, &mut None), None);
    }

    fn refined_source(restricted: bool, exclude_draw: bool, known: bool) -> PlayerView {
        let mut source = reviewed_source();
        if restricted {
            for held in &mut source.hands[1][..2] {
                held.clues.add_negative_clue(Clue::Suit(Suit::Blue));
                held.clues.add_negative_clue(Clue::Suit(Suit::Purple));
            }
        }
        if exclude_draw {
            source.hands[1][3]
                .clues
                .add_negative_clue(Clue::Rank(Rank::Three));
        }
        if known {
            source.hands[1][1]
                .clues
                .add_positive_clue(Clue::Suit(Suit::Red));
        }
        source
    }

    #[test]
    fn reviewed_majority_changes_forecast_without_revealing_cards() {
        // Hypothetical information refinements of the reviewed turn-6 position,
        // not invented game histories or new convention authorities.
        for (restricted, exclude_draw, known, wants_play) in [
            (false, false, false, false),
            (true, false, false, true),
            (true, true, false, false),
            (true, false, true, true),
        ] {
            let source = refined_source(restricted, exclude_draw, known);
            let mass = probability(
                &LogicalDeductions::new(source.clone()).unwrap(),
                &BeliefConstraints::default(),
                &[(CardId::new(4), CardId::new(5))],
                Card::new(Suit::Red, Rank::Three),
                &AnalysisControl::default(),
            )
            .unwrap()
            .unwrap();
            let (numerator, denominator) = if known {
                (1, 1)
            } else if !restricted {
                (83, 275)
            } else if exclude_draw {
                (1, 2)
            } else {
                (17, 33)
            };
            assert_eq!(mass.covered * denominator, mass.total * numerator);
            let before = source.clone();
            let mut actor = source.clone();
            actor.observer = PlayerId::new(2);
            if known {
                actor.hands[1][1].identity = Some(Card::new(Suit::Red, Rank::Three));
            }
            for held in &mut actor.hands[2] {
                held.identity = None;
            }
            let d = LogicalDeductions::new(actor).unwrap();
            let (action, decisions) = crate::capture_decisions(|| {
                super::super::choose_projected_follow_up_from(
                    &source,
                    &d,
                    HGroupProfile::Max,
                    &AnalysisControl::default(),
                )
                .unwrap()
            });
            assert_eq!(
                action,
                Some(if wants_play {
                    Action::Play(CardId::new(10))
                } else {
                    Action::Clue {
                        target: PlayerId::new(0),
                        clue: Clue::Suit(Suit::Blue),
                    }
                }),
                "restricted={restricted}, exclude_draw={exclude_draw}"
            );
            assert_eq!(source, before);
            assert!(source.hands[1].iter().all(|c| c.identity.is_none()));
            if wants_play {
                let decision = decisions
                    .iter()
                    .find(|d| {
                        d.comparisons
                            .iter()
                            .any(|c| c.reason == ComparisonReason::MajorityCoverage)
                    })
                    .unwrap();
                let comparison = decision
                    .comparisons
                    .iter()
                    .find(|c| c.reason == ComparisonReason::MajorityCoverage)
                    .unwrap();
                let (_, covered, total) = comparison
                    .basis
                    .as_ref()
                    .unwrap()
                    .coverage_probability
                    .unwrap();
                assert_eq!(covered * denominator, total * numerator);
                if !known {
                    assert_safety_controls(&source, &d, decision);
                }
            }
        }
    }
    fn mark_critical(evidence: &mut crate::ProjectionEvidence) -> bool {
        if let Some(step) = evidence
            .steps
            .iter_mut()
            .find(|s| s.consequences.save_principle_violation.is_some())
        {
            step.consequences.save_principle_violation =
                Some(crate::SavePrincipleViolation::CriticalCard);
            return true;
        }
        evidence
            .private_branches
            .iter_mut()
            .any(|b| mark_critical(&mut b.continuation))
    }

    fn assert_safety_controls(
        source: &PlayerView,
        d: &LogicalDeductions,
        decision: &crate::ProjectedDecision,
    ) {
        let play = decision
            .candidates
            .iter()
            .find(|c| c.action == Action::Play(CardId::new(10)))
            .unwrap();
        // The majority choice must not relabel a conditional discard safe.
        // The same loss assertion now traverses conditional continuations:
        // their losses must not be fabricated in the shared public prefix.
        assert!(play.projection.maximum_save_violations() > 0);
        let clue_index = decision.candidates.iter().position(|c| matches!(c.action, Action::Clue { target, clue: Clue::Suit(Suit::Blue) } if target == PlayerId::new(0))).unwrap();
        let clue = &decision.candidates[clue_index];
        let mut connected = source.clone();
        connected.hands[2][1].identity = Some(Card::new(Suit::Blue, Rank::Three));
        assert!(!eligible(&connected, play, clue, PlayerId::new(0)));
        assert!(covered_policy_case(&connected, play, clue, PlayerId::new(0)).is_none());
        let mut refund = source.clone();
        refund.hands[3]
            .iter_mut()
            .find(|card| card.id == CardId::new(15))
            .unwrap()
            .identity = Some(Card::new(Suit::Blue, Rank::Five));
        assert!(!eligible(&refund, play, clue, PlayerId::new(0)));
        assert!(covered_policy_case(&refund, play, clue, PlayerId::new(0)).is_none());
        let mut unsafe_play = play.clone();
        unsafe_play.projection.steps[0].consequences.strikes = 1;
        assert!(!eligible(source, &unsafe_play, clue, PlayerId::new(0)));
        assert!(covered_policy_case(source, &unsafe_play, clue, PlayerId::new(0)).is_none());
        let mut critical = decision.candidates.clone();
        let modified = critical
            .iter_mut()
            .find(|c| c.action == play.action)
            .unwrap();
        assert!(mark_critical(&mut modified.projection));
        assert!(
            choose(
                source,
                d.view(),
                HGroupProfile::Max,
                &critical,
                clue_index,
                &mut Vec::new(),
                &AnalysisControl::default()
            )
            .unwrap()
            .is_none()
        );
    }
}
