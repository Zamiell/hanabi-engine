use hanabi_core::{Action, Card, CardId, Clue, PlayerId, PlayerView};

use crate::{LogicalDeductions, SymbolicLineOutcome};

use super::{
    ConditionalPlan, HGroupProfile, PerspectiveDepth, PerspectiveProjector, PlanFrontier,
    ProjectedAction, ProjectedConsequences, ProspectiveTransition, identity_of,
    infer_h_group_from_replay, is_playable_now, select_h_group_action,
};

#[test]
fn reviewed_turn_thirty_compares_equal_elapsed_time() {
    let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
        "../../../hanabi-protocol/tests/fixtures/game-p4v0s1.json"
    ))
    .unwrap();
    let state = replay.state_at_turn(29).unwrap();
    let source = state.view_for(state.current_player()).unwrap();
    let roots = [
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(hanabi_core::Rank::Four),
        },
    ];
    let mut values = Vec::new();
    for root in roots {
        let mut public = source.clone();
        for step in 0..4 {
            let (d, r) = PerspectiveProjector::new(&public, HGroupProfile::Max)
                .project(public.current_player, PerspectiveDepth::NestedRecipients)
                .unwrap();
            let inferred = infer_h_group_from_replay(&d, r, HGroupProfile::Max);
            let action = if step == 0 {
                root
            } else if matches!(root, Action::Play(_)) && step >= 2 {
                Action::Discard(CardId::new(if step == 2 { 12 } else { 30 }))
            } else {
                select_h_group_action(&d, HGroupProfile::Max).unwrap()
            };
            public = apply_symbolic_action(&public, &d, &inferred, public.current_player, action)
                .unwrap()
                .0;
        }
        values.push(
            super::frontier_value::evaluate(&source, &public, HGroupProfile::Max, root).unwrap(),
        );
    }
    // User-reviewed counterfactuals, p4v0s1 turn 30: b5 / Save / discard /
    // discard versus 4s / r4 / Save / discard. No hidden draws are supplied.
    assert_eq!(values[0].score, values[1].score);
    assert!(values[1].playable_finesse_opportunities > values[0].playable_finesse_opportunities);
    assert!(values[1].development_preference(values[0], 1, 2));
    assert!(!values[0].development_preference(values[1], 2, 1));
    let analysis = crate::analyze_position(
        &source,
        crate::SupportedConvention::HGroup(HGroupProfile::Max),
        crate::PlannerConfig::default(),
    )
    .unwrap();
    assert_eq!(
        analysis.planner.best_action, roots[1],
        "{:#?}",
        analysis.planner
    );
    for root in roots {
        let result = analysis
            .planner
            .root_actions
            .iter()
            .find(|candidate| candidate.action == root)
            .unwrap();
        assert_eq!(result.symbolic_line.first_rotation.unwrap().actions, 4);
    }
}

/// Projects the convention policy's actions while leaving unknown draws blank.
/// Strategic choices continue under that policy; unresolved identities stop
/// the line rather than being filled using the actual hidden hand or deck.
#[cfg(test)]
pub(crate) fn project_h_group_line(
    source: &PlayerView,
    profile: HGroupProfile,
    root: Action,
    limit: u8,
) -> SymbolicLineOutcome {
    project_h_group_plan(source, profile, root, limit).summarize()
}

pub(crate) fn project_h_group_projection(
    source: &PlayerView,
    profile: HGroupProfile,
    root: Action,
    limit: u8,
    control: &crate::AnalysisControl,
) -> Result<(SymbolicLineOutcome, super::ProjectionEvidence), crate::AnalysisStopped> {
    let _trace_root = crate::diagnostics::root(root);
    let plan = project_h_group_plan_with_control::<true>(source, profile, root, limit, control)?;
    Ok((plan.summarize(), plan.into_evidence()))
}

#[cfg(test)]
fn project_h_group_plan(
    source: &PlayerView,
    profile: HGroupProfile,
    root: Action,
    limit: u8,
) -> ConditionalPlan {
    project_h_group_plan_with_control::<true>(
        source,
        profile,
        root,
        limit,
        &crate::AnalysisControl::default(),
    )
    .expect("unlimited analysis completes")
}

fn project_h_group_plan_with_control<const REUSE_SELECTED: bool>(
    source: &PlayerView,
    profile: HGroupProfile,
    root: Action,
    limit: u8,
    control: &crate::AnalysisControl,
) -> Result<ConditionalPlan, crate::AnalysisStopped> {
    continue_plan::<REUSE_SELECTED>(
        source,
        source.clone(),
        profile,
        Some(root),
        root,
        limit,
        control,
        ConditionalPlan::new(source.clue_tokens),
        // Inverse-planning certificates deliberately use a lower-order
        // convention policy. Asking that proof to run strategic forecasts
        // again would multiply historical proof work and change its model.
        !super::inverse_planning::is_active(),
    )
}

pub(crate) fn project_leaf_projection(
    source: &PlayerView,
    profile: HGroupProfile,
    root: Action,
    control: &crate::AnalysisControl,
) -> Result<(SymbolicLineOutcome, super::ProjectionEvidence), crate::AnalysisStopped> {
    let plan = continue_plan::<true>(
        source,
        source.clone(),
        profile,
        Some(root),
        root,
        32,
        control,
        ConditionalPlan::new(source.clue_tokens),
        false,
    )?;
    Ok((plan.summarize(), plan.into_evidence()))
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn continue_plan<const REUSE_SELECTED: bool>(
    source: &PlayerView,
    mut public: PlayerView,
    profile: HGroupProfile,
    mut action: Option<Action>,
    root: Action,
    limit: u8,
    control: &crate::AnalysisControl,
    mut plan: ConditionalPlan,
    strategic: bool,
) -> Result<ConditionalPlan, crate::AnalysisStopped> {
    #[cfg(test)]
    let _profile = crate::test_profile::span("symbolic_projection");
    // The final part of each iteration already compiles the next actor's
    // perspective to select their action. Keep that exact immutable result for
    // execution; the public state does not change between selection and use.
    let mut selected_perspective = None;

    while let Some(current) = action {
        control.checkpoint()?;
        if public.status != hanabi_core::GameStatus::InProgress {
            plan.stop_at(PlanFrontier::Terminal);
            break;
        }
        if plan.len() >= usize::from(limit) {
            plan.stop_at(PlanFrontier::Limit);
            break;
        }
        let actor = public.current_player;
        let Some(projected) = selected_perspective.take().or_else(|| {
            PerspectiveProjector::new(&public, profile)
                .project_with_evidence(actor, PerspectiveDepth::NestedRecipients)
        }) else {
            plan.stop_at(PlanFrontier::ProjectionUnavailable);
            break;
        };
        plan.record_assumptions(&projected.assumptions);
        let actor_deductions = projected.deductions;
        let actor_replay = projected.replay;
        let actor_inferences = infer_h_group_from_replay(&actor_deductions, actor_replay, profile);
        plan.record_window(super::ActionWindow::from_inferences(
            actor_deductions.view(),
            &actor_inferences,
        ));
        let dependencies = actor_inferences
            .projection_requirements
            .iter()
            .filter(|requirement| requirement.action == current)
            .map(|requirement| super::projection_requirements::assess(&public, requirement))
            .collect();
        if !plan.assess_dependencies(dependencies) {
            plan.stop_at(PlanFrontier::InterpretationBranch);
            break;
        }
        // A convention may tell a future player to try a newly drawn card,
        // but cannot reveal that draw's physical identity to the root observer.
        // Stop before crediting a successful play, discard token, or new draw.
        let unresolved_draw = matches!(current, Action::Play(card) | Action::Discard(card)
            if identity_of(&public, card).is_none()
                && !source.hands.iter().flatten().any(|known| known.id == card));
        let transition = (!unresolved_draw)
            .then(|| {
                apply_symbolic_action(
                    &public,
                    &actor_deductions,
                    &actor_inferences,
                    actor,
                    current,
                )
            })
            .flatten();
        let Some((after, mut consequences)) = transition else {
            let frontier = PlanFrontier::IdentityBranch;
            if let Action::Discard(card) = current {
                if let Some(domain) = safe_discard_domain(&public, profile, card) {
                    // The face is unknown, but the discard's resource effect
                    // and lack of card loss are established. Record that
                    // action, not an unexecuted hazardous frontier. Revealing
                    // the face may change card counting/transfer inferences,
                    // so do not invent a concrete public event or endpoint.
                    plan.push(
                        public.turn,
                        ProjectedAction {
                            actor,
                            action: current,
                        },
                        ProjectedConsequences {
                            discards: 1,
                            clues_gained: u8::from(
                                public.clue_tokens < hanabi_core::MAX_CLUE_TOKENS,
                            ),
                            ..ProjectedConsequences::default()
                        },
                    );
                    plan.record_interpretation(Some(domain));
                    plan.stop_at(PlanFrontier::SafeDiscardReveal);
                    plan.assess(None);
                    control.checkpoint()?;
                    return Ok(plan);
                }
                // An unexecuted unknown discard is not evidence of zero loss.
                // Keep it as a possible hazard without inventing an identity,
                // spending the turn, or crediting a token/draw.
                let risk = LogicalDeductions::new(public.clone()).ok().is_none_or(|deductions| {
                    let source_inferred = super::infer_h_group(&deductions, profile);
                    super::chop_safety::discard_domain(&deductions, &source_inferred, profile, card).is_none_or(|identities| {
                        identities.iter().any(|identity| {
                            super::is_eventually_useful(&public, identity)
                                // Losing a last copy is a distinct critical-card
                                // risk, not a risk that its other copy is bottom
                                // deck. Do not count unknown 5s as BDR evidence.
                                && !is_last_copy(&public, identity)
                                && !public.hands.iter().flatten().any(|other| {
                                    other.id != card && (other.identity == Some(identity)
                                        || source_inferred.cards.iter().any(|note| {
                                            note.card == other.id
                                                && note.identities == crate::IdentitySet::singleton(identity)
                                        }))
                                })
                        })
                    })
                });
                let required_protection = super::projected_h_group_replay(&public, profile, actor)
                    .is_some_and(|(deductions, replay)| {
                        super::decision::emergency_discard_is_required(
                            deductions.view(),
                            &actor_inferences,
                            &replay,
                            profile,
                        )
                    });
                plan.record_unresolved_discard(
                    card,
                    risk,
                    required_protection,
                    strategic || plan.len() == 0,
                );
            }
            plan.stop_at(frontier);
            break;
        };
        if let Action::Discard(card) = current {
            (
                consequences.save_principle_violation,
                consequences.bottom_deck_risk,
            ) = important_discard(&public, profile, card);
        }
        if consequences.score_gain > 0 {
            if let Action::Play(card) = current {
                if let Some(played) =
                    symbolic_identity(&public, &actor_deductions, &actor_inferences, card)
                {
                    if let Some(alternative) = super::frontier_value::conditional_successor(
                        source, &after, profile, played,
                    ) {
                        plan.add_alternative(alternative);
                    }
                }
            }
        }
        plan.push(
            public.turn,
            ProjectedAction {
                actor,
                action: current,
            },
            consequences,
        );
        plan.record_interpretation(match current {
            Action::Play(card) | Action::Discard(card) => actor_inferences
                .cards
                .iter()
                .find(|note| note.card == card)
                .map(|note| note.identities)
                .or_else(|| actor_deductions.possible_identities(card)),
            Action::Clue { .. } => None,
        });
        public = after;
        plan.record_checkpoint(super::frontier_value::evaluate(
            source, &public, profile, root,
        ));
        if plan.len() == source.hands.len() {
            plan.record_rotation(super::frontier_value::evaluate(
                source, &public, profile, root,
            ));
        }
        if public.status != hanabi_core::GameStatus::InProgress {
            plan.stop_at(PlanFrontier::Terminal);
            break;
        }
        if plan.len() >= usize::from(limit) {
            plan.stop_at(PlanFrontier::Limit);
            break;
        }
        let next = public.current_player;
        let Some(projected) = PerspectiveProjector::new(&public, profile)
            .project_with_evidence(next, PerspectiveDepth::NestedRecipients)
        else {
            plan.stop_at(PlanFrontier::ProjectionUnavailable);
            break;
        };
        plan.record_assumptions(&projected.assumptions);
        action = if strategic {
            crate::planner::choose_projected_follow_up(&projected.deductions, profile, control)?
        } else {
            select_h_group_action(&projected.deductions, profile)
        };
        if REUSE_SELECTED {
            selected_perspective = Some(projected);
        }
    }
    let value = final_assessment(source, &public, profile, root, &plan);
    control.checkpoint()?;
    plan.assess(value);
    Ok(plan)
}

/// Assess losses from the deciding observer's partial world, not the
/// discarder who cannot see their own card, and never the simulator's deck.
/// Sources:
/// <https://hanabi.github.io/beginner/save-principle/>
/// <https://hanabi.github.io/level-22/#phantom-playable-cards>
pub(super) fn important_discard(
    source: &PlayerView,
    profile: HGroupProfile,
    card: CardId,
) -> (Option<super::SavePrincipleViolation>, Option<Card>) {
    use super::SavePrincipleViolation as Loss;
    let observed = identity_of(source, card);
    if observed.is_some_and(|identity| !super::is_eventually_useful(source, identity)) {
        return (None, None);
    }
    if observed.is_some_and(|identity| is_last_copy(source, identity)) {
        return (Some(Loss::CriticalCard), None);
    }
    let Ok(deductions) = LogicalDeductions::new(source.clone()) else {
        return (None, None);
    };
    let inferred = super::infer_h_group(&deductions, profile);
    // A known replacement can support an intentional transfer. Merely having
    // an unknown slot which COULD contain another copy cannot justify a loss.
    let known = |id| {
        identity_of(source, id).or_else(|| {
            inferred
                .cards
                .iter()
                .find(|note| note.card == id)
                .filter(|note| note.identities.len() == 1)
                .and_then(|note| note.identities.iter().next())
        })
    };
    let Some(identity) = known(card) else {
        return (None, None);
    };
    if !super::is_eventually_useful(source, identity) {
        return (None, None);
    }
    if is_last_copy(source, identity) {
        return (Some(Loss::CriticalCard), None);
    }
    if source
        .hands
        .iter()
        .flatten()
        .any(|other| other.id != card && known(other.id) == Some(identity))
    {
        return (None, None);
    }
    if identity.rank == hanabi_core::Rank::Two {
        return (Some(Loss::UniqueTwo), Some(identity));
    }
    if is_playable_now(source, identity) {
        return (Some(Loss::UniquePlayable), Some(identity));
    }
    let gotten = inferred.gotten();
    let gotten = &gotten;
    let connectors = source
        .hands
        .iter()
        .flat_map(|hand| {
            let position = super::finesse_position(hand, gotten, 0).map(|card| card.id);
            hand.iter()
                .filter(move |candidate| {
                    super::was_clued_before(source, source.turn, candidate.id)
                        || Some(candidate.id) == position
                })
                .filter_map(|candidate| known(candidate.id))
        })
        .collect::<Vec<_>>();
    // All intervening ranks must be clued/promised or on Finesse Position.
    // Mere visibility is only Phantom Playability and is not this invariant.
    let save_violation = ((source.play_stacks[identity.suit.index()].len() + 1)
        ..usize::from(identity.rank.number()))
        .all(|rank| {
            connectors.contains(&Card::new(identity.suit, hanabi_core::Rank::ALL[rank - 1]))
        })
        .then_some(Loss::UniqueDelayedPlayable);
    // Even a distant connector can be stranded at the bottom of the deck.
    // Do not confuse absence of immediate Save urgency with a harmless loss.
    // https://hanabi.github.io/level-25/#the-load-clue
    (save_violation, Some(identity))
}

/// Physical last-copy loss includes 5s, unlike the non-5 Critical Save
/// convention. BDR requires another copy that could be buried in the deck.
fn is_last_copy(view: &PlayerView, identity: Card) -> bool {
    view.discard_pile
        .iter()
        .filter(|(_, discarded)| *discarded == identity)
        .count()
        + 1
        >= usize::from(identity.rank.copies())
}

fn final_assessment(
    source: &PlayerView,
    public: &PlayerView,
    profile: HGroupProfile,
    root: Action,
    plan: &ConditionalPlan,
) -> Option<crate::ProjectedPositionValue> {
    if plan.len() == source.hands.len() {
        plan.summarize()
            .first_rotation
            .map(|checkpoint| checkpoint.value)
    } else {
        super::frontier_value::evaluate(source, public, profile, root)
    }
}

#[cfg(test)]
fn has_unresolved_requirement(
    source: &PlayerView,
    inferred: &super::HGroupInferences,
    action: Action,
) -> bool {
    inferred
        .projection_requirements
        .iter()
        .filter(|requirement| requirement.action == action)
        .any(|requirement| {
            super::projection_requirements::assess(source, requirement).status
                != super::DependencyStatus::Supported
        })
}

pub(super) fn apply_symbolic_action(
    source: &PlayerView,
    actor_deductions: &LogicalDeductions,
    actor_inferences: &super::HGroupInferences,
    actor: PlayerId,
    action: Action,
) -> Option<(PlayerView, ProjectedConsequences)> {
    let mut consequences = ProjectedConsequences::default();
    match action {
        Action::Clue { target, clue } => {
            let touched = touched_cards(source, target, clue)?;
            consequences.clues_spent = 1;
            Some((
                ProspectiveTransition::symbolic_clue_by(source, actor, target, clue, &touched),
                consequences,
            ))
        }
        Action::Play(card) => {
            let identity = symbolic_identity(source, actor_deductions, actor_inferences, card)?;
            let successful = is_playable_now(source, identity);
            if successful {
                consequences.score_gain = 1;
                if identity.rank.number() == 5 {
                    consequences.clues_gained = 1;
                }
            } else {
                consequences.strikes = 1;
            }
            Some((
                ProspectiveTransition::play(source, actor, card, identity, successful),
                consequences,
            ))
        }
        Action::Discard(card) => {
            let identity = symbolic_identity(source, actor_deductions, actor_inferences, card)?;
            consequences.discards = 1;
            if source.clue_tokens < hanabi_core::MAX_CLUE_TOKENS {
                consequences.clues_gained = 1;
            }
            Some((
                ProspectiveTransition::discard(source, actor, card, identity),
                consequences,
            ))
        }
    }
}

fn symbolic_identity(
    source: &PlayerView,
    deductions: &LogicalDeductions,
    inferences: &super::HGroupInferences,
    card: CardId,
) -> Option<Card> {
    identity_of(source, card).or_else(|| {
        inferences
            .cards
            .iter()
            .find(|inference| inference.card == card)
            .and_then(|inference| {
                (inference.identities.len() == 1)
                    .then(|| inference.identities.iter().next())
                    .flatten()
            })
            .or_else(|| {
                deductions
                    .possible_identities(card)
                    .filter(|identities| identities.len() == 1)
                    .and_then(|identities| identities.iter().next())
            })
    })
}

/// Root-observer evidence only. "No BDR" alone is insufficient: a last-copy
/// loss is not BDR either. Every remaining face must be obsolete or have a
/// visible replacement. Save Principle exclusions remain conditional discard
/// knowledge, not invented exact identities or Good Touch notes on unclued cards.
fn safe_discard_domain(
    source: &PlayerView,
    profile: HGroupProfile,
    card: CardId,
) -> Option<crate::IdentitySet> {
    let deductions = LogicalDeductions::new(source.clone()).ok()?;
    let inferred = super::infer_h_group(&deductions, profile);
    let domain = super::chop_safety::discard_domain(&deductions, &inferred, profile, card)?;
    (!domain.is_empty()
        && domain.iter().all(|identity| {
            !super::is_eventually_useful(source, identity)
                || source
                    .hands
                    .iter()
                    .flatten()
                    .any(|held| held.id != card && held.identity == Some(identity))
        }))
    .then_some(domain)
}

fn touched_cards(source: &PlayerView, target: PlayerId, clue: Clue) -> Option<Vec<CardId>> {
    let hand = source.hands.get(target.index())?;
    let mut touched = Vec::new();
    for card in hand {
        let matches = if let Some(identity) = card.identity {
            clue.matches(identity)
        } else {
            let identities = crate::IdentitySet::from_mask(card.clues.identity_mask());
            if identities.is_empty() {
                return None;
            }
            identities.iter().all(|identity| clue.matches(identity))
        };
        if matches {
            touched.push(card.id);
        }
    }
    (!touched.is_empty()).then_some(touched)
}

#[cfg(test)]
mod tests {
    use crate::SymbolicStopReason;
    use hanabi_core::{FullState, PlayerId, standard_deck};

    use super::*;

    #[test]
    fn first_seed_turn_35_eliminates_discard_risk_without_an_exact_face() {
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s1.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(34).unwrap();
        let view = state.view_for(PlayerId::new(2)).unwrap();
        let deductions = LogicalDeductions::new(view.clone()).unwrap();
        let inferred = super::super::infer_h_group(&deductions, HGroupProfile::Max);
        let domain = super::super::chop_safety::discard_domain(
            &deductions,
            &inferred,
            HGroupProfile::Max,
            CardId::new(18),
        )
        .unwrap();
        assert!(domain.len() > 1);
        assert!(
            domain.iter().all(|identity| {
                !super::super::is_eventually_useful(&view, identity)
                    || view
                        .hands
                        .iter()
                        .flatten()
                        .any(|held| held.identity == Some(identity))
            }),
            "{domain:?}"
        );
        let plan = project_h_group_plan(
            &view,
            HGroupProfile::Max,
            Action::Discard(CardId::new(18)),
            1,
        );
        assert_eq!(plan.summarize().discards, 1);
        assert_eq!(plan.summarize().clues_gained, 1);
        assert!(plan.summarize().position_value.is_none());
        let evidence = plan.into_evidence();
        assert_eq!(evidence.frontier, PlanFrontier::SafeDiscardReveal);
        assert_eq!(evidence.resources.tokens, 2);
        assert_eq!(evidence.forecast_discard_risk(), Some(0));
        assert_eq!(evidence.steps[0].interpreted_identities, Some(domain));
        assert!(evidence.unresolved_discard.is_none());
        // An ordinary unknown card with genuinely useful possibilities is
        // not certified trash by the absence of an observed misplay/loss.
        assert!(safe_discard_domain(&view, HGroupProfile::Max, CardId::new(31)).is_none());
        // A hidden last-copy 5 has no BDR in the narrow sense, but is not
        // safe to discard. Never certify safety from the BDR boolean alone.
        let bob = state.view_for(PlayerId::new(1)).unwrap();
        assert!(safe_discard_domain(&bob, HGroupProfile::Max, CardId::new(19)).is_none());
        assert!(
            view.hands[2]
                .iter()
                .find(|held| held.id == CardId::new(18))
                .unwrap()
                .identity
                .is_none()
        );
    }

    #[test]
    fn reviewed_turn_eleven_distinguishes_blue_three_risk_from_yellow_trash() {
        // Human-reviewed p4v0s3 turn 11: 4s to Bob loses Donald's only
        // visible b3; the 3s Self-Bluff instead leaves Bob to discard y1.
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s3.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(10).unwrap();
        let source = state.view_for(state.current_player()).unwrap();
        assert_eq!(
            important_discard(&source, HGroupProfile::Max, CardId::new(17)),
            (
                None,
                Some(Card::new(hanabi_core::Suit::Blue, hanabi_core::Rank::Three))
            )
        );
        assert_eq!(
            important_discard(&source, HGroupProfile::Max, CardId::new(5)),
            (None, None)
        );
        // Bob's p4 has a replacement in Cathy's hidden hand, but Cathy cannot
        // use that simulator truth. Donald, who sees both copies, can.
        let donald = state.view_for(PlayerId::new(3)).unwrap();
        assert_eq!(
            important_discard(&donald, HGroupProfile::Max, CardId::new(16)),
            (None, None)
        );
    }

    #[test]
    fn reviewed_turn_fourteen_rejects_losing_cathys_delayed_purple_four() {
        // p4v0s3 turn 14, human-reviewed: saving the 2s exposes Cathy's p4
        // to a discard. Bob can see p2/p3 already touched in Donald's hand.
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s3.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(13).unwrap();
        let source = state.view_for(state.current_player()).unwrap();
        assert_eq!(
            important_discard(&source, HGroupProfile::Max, CardId::new(10)).0,
            Some(super::super::SavePrincipleViolation::UniqueDelayedPlayable)
        );
        // The other p4 is in Bob's hidden hand, not evidence that the visible
        // p4 is disposable. Conversely, no loss may be invented for a blank.
        assert_eq!(
            important_discard(&source, HGroupProfile::Max, CardId::new(16)),
            (None, None)
        );
        let rank_two = Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(hanabi_core::Rank::Two),
        };
        let plan = project_h_group_plan(&source, HGroupProfile::Max, rank_two, 2).into_evidence();
        assert_eq!(
            plan.steps[1].projected.action,
            Action::Discard(CardId::new(10))
        );
        assert_eq!(plan.maximum_save_violations(), 1);
        assert_eq!(
            plan.steps[1].consequences.save_principle_violation,
            Some(super::super::SavePrincipleViolation::UniqueDelayedPlayable)
        );
    }

    #[test]
    fn reviewed_fourth_replay_purple_allows_rank_three_self_bluff() {
        // Human-reviewed p4v0s3 turn 11: after Bob's purple clue, Cathy's
        // rank-3 clue to Donald is a Self-Bluff on Donald's newest card.
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s3.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(9).unwrap();
        let mut public = state.view_for(state.current_player()).unwrap();
        for clue in [
            Clue::Suit(hanabi_core::Suit::Purple),
            Clue::Rank(hanabi_core::Rank::Three),
        ] {
            let action = Action::Clue {
                target: PlayerId::new(3),
                clue,
            };
            let (d, r) = PerspectiveProjector::new(&public, HGroupProfile::Max)
                .project(public.current_player, PerspectiveDepth::NestedRecipients)
                .unwrap();
            let inferred = infer_h_group_from_replay(&d, r, HGroupProfile::Max);
            if matches!(clue, Clue::Rank(_)) {
                let analysis = crate::SupportedConvention::HGroup(HGroupProfile::Max).analyze(&d);
                assert!(
                    analysis
                        .actions
                        .iter()
                        .any(|candidate| candidate.action == action)
                );
            }
            public = apply_symbolic_action(&public, &d, &inferred, public.current_player, action)
                .unwrap()
                .0;
        }
        let (d, _) = PerspectiveProjector::new(&public, HGroupProfile::Max)
            .project(public.current_player, PerspectiveDepth::NestedRecipients)
            .unwrap();
        assert_eq!(
            select_h_group_action(&d, HGroupProfile::Max),
            Some(Action::Play(CardId::new(19)))
        );
    }

    #[test]
    fn reviewed_fourth_replay_turn_ten_projects_strategic_followups() {
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s3.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(9).unwrap();
        let source = state.view_for(state.current_player()).unwrap();
        for (target, suit) in [
            (3, hanabi_core::Suit::Purple),
            (2, hanabi_core::Suit::Green),
        ] {
            let plan = project_h_group_plan(
                &source,
                HGroupProfile::Max,
                Action::Clue {
                    target: PlayerId::new(target),
                    clue: Clue::Suit(suit),
                },
                4,
            );
            if suit == hanabi_core::Suit::Green {
                let mut counterfactual = replay.state_at_turn(9).unwrap();
                counterfactual
                    .apply(Action::Clue {
                        target: PlayerId::new(2),
                        clue: Clue::Suit(hanabi_core::Suit::Green),
                    })
                    .unwrap();
                counterfactual
                    .apply(Action::Clue {
                        target: PlayerId::new(3),
                        clue: Clue::Suit(hanabi_core::Suit::Purple),
                    })
                    .unwrap();
                let d = LogicalDeductions::new(counterfactual.view_for(PlayerId::new(3)).unwrap())
                    .unwrap();
                assert_eq!(
                    super::super::infer_h_group(&d, HGroupProfile::Max)
                        .connection
                        .map(|connection| connection.card),
                    Some(CardId::new(19))
                );
                assert_eq!(
                    plan.into_evidence().steps[1].projected.action,
                    Action::Clue {
                        target: PlayerId::new(3),
                        clue: Clue::Suit(hanabi_core::Suit::Purple)
                    }
                );
            }
        }
    }

    #[test]
    fn reviewed_purple_projection_does_not_invent_a_finesse_from_an_unseen_focus() {
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s3.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(9).unwrap();
        let source = state.view_for(state.current_player()).unwrap();
        let plan = project_h_group_plan(
            &source,
            HGroupProfile::Max,
            Action::Clue {
                target: PlayerId::new(3),
                clue: Clue::Suit(hanabi_core::Suit::Purple),
            },
            12,
        );
        assert_eq!(plan.into_evidence().maximum_strikes(), 0);
    }

    #[test]
    fn reviewed_purple_line_keeps_the_draw_blank() {
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s3.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(9).unwrap();
        let root = Action::Clue {
            target: PlayerId::new(3),
            clue: Clue::Suit(hanabi_core::Suit::Purple),
        };
        // Branch from the reviewed position, not the fixture's later history.
        let source = state.view_for(state.current_player()).unwrap();
        let mut public = source.clone();
        for action in [root, Action::Play(CardId::new(8))] {
            let (d, r) = PerspectiveProjector::new(&public, HGroupProfile::Max)
                .project(public.current_player, PerspectiveDepth::NestedRecipients)
                .unwrap();
            let notes = infer_h_group_from_replay(&d, r, HGroupProfile::Max);
            public = apply_symbolic_action(&public, &d, &notes, public.current_player, action)
                .unwrap()
                .0;
        }
        let clue = Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(hanabi_core::Rank::Two),
        };
        let touched = touched_cards(
            &public,
            PlayerId::new(2),
            Clue::Rank(hanabi_core::Rank::Two),
        )
        .unwrap();
        let after = ProspectiveTransition::symbolic_clue_by(
            &public,
            public.current_player,
            PlayerId::new(2),
            Clue::Rank(hanabi_core::Rank::Two),
            &touched,
        );
        for blank in public.hands[2]
            .iter()
            .filter(|card| card.identity.is_none())
        {
            assert!(!touched.contains(&blank.id));
            assert_eq!(
                after.hands[2]
                    .iter()
                    .find(|card| card.id == blank.id)
                    .unwrap()
                    .clues,
                blank.clues
            );
            if let hanabi_core::ObservedEvent::Clued {
                touched, untouched, ..
            } = &after.history.last().unwrap().event
            {
                assert!(!touched.contains(&blank.id));
                assert!(!untouched.contains(&blank.id));
            } else {
                panic!("expected clue event");
            }
        }
        let plan = continue_plan::<true>(
            &public,
            public.clone(),
            HGroupProfile::Max,
            Some(clue),
            clue,
            2,
            &crate::AnalysisControl::default(),
            ConditionalPlan::new(public.clue_tokens),
            false,
        )
        .unwrap();
        let evidence = plan.into_evidence();
        assert!(evidence.clue_branches.is_empty());
        assert_eq!(evidence.steps[0].projected.action, clue);
        assert_eq!(
            evidence.steps[1].projected.action,
            Action::Play(CardId::new(20))
        );
    }

    #[test]
    fn priority_projection_does_not_prove_absence_from_a_blank_hand() {
        // Reviewed p4v0s415 turn 19, projected from Cathy: her hidden r5
        // cannot be used as either present or absent in Bob's visible hands.
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s415.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(18).unwrap();
        let view = state.view_for(state.current_player()).unwrap();
        let (outcome, evidence) = project_h_group_projection(
            &view,
            HGroupProfile::Max,
            Action::Clue {
                target: PlayerId::new(0),
                clue: Clue::Suit(hanabi_core::Suit::Purple),
            },
            32,
            &crate::AnalysisControl::default(),
        )
        .unwrap();
        assert_eq!(outcome.strikes, 0, "{outcome:?}");
        assert!(matches!(
            outcome.stop_reason,
            SymbolicStopReason::UnknownInterpretation | SymbolicStopReason::UnknownIdentity
        ));
        // Priority may select a different preceding play. Test the forbidden
        // inference, not the old policy's exact three-action prefix.
        assert!(
            !evidence.steps.iter().any(|step| {
                step.turn == 21 && step.projected.action == Action::Play(CardId::new(25))
            }),
            "do not execute Bob's unsupported blind play: {evidence:#?}"
        );
        let mut public = ProspectiveTransition::clue_by(
            &view,
            view.current_player,
            PlayerId::new(0),
            Clue::Suit(hanabi_core::Suit::Purple),
            &[CardId::new(19), CardId::new(24)],
        );
        for (actor, card, identity) in [
            (
                3,
                17,
                Card::new(hanabi_core::Suit::Red, hanabi_core::Rank::Four),
            ),
            (
                0,
                19,
                Card::new(hanabi_core::Suit::Purple, hanabi_core::Rank::One),
            ),
        ] {
            public = ProspectiveTransition::successful_play(
                &public,
                PlayerId::new(actor),
                CardId::new(card),
                identity,
            );
        }
        let (deductions, state) = PerspectiveProjector::new(&public, HGroupProfile::Max)
            .project(public.current_player, PerspectiveDepth::NestedRecipients)
            .unwrap();
        let inferred = infer_h_group_from_replay(&deductions, state, HGroupProfile::Max);
        let play = Action::Play(CardId::new(25));
        assert!(has_unresolved_requirement(&public, &inferred, play));
        // Algorithmic domain boundary: if every blank excludes red, none can
        // redirect this r5 Priority interpretation. This is not a fixture edit.
        for card in public
            .hands
            .iter_mut()
            .flatten()
            .filter(|card| card.identity.is_none())
        {
            card.clues
                .add_negative_clue(Clue::Suit(hanabi_core::Suit::Red));
        }
        assert!(!has_unresolved_requirement(&public, &inferred, play));
    }

    #[test]
    fn clue_givers_blank_hand_does_not_create_a_charm_branch() {
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s1.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(0).unwrap();
        let view = state.view_for(state.current_player()).unwrap();
        let target = PlayerId::new(2);
        let clue = Clue::Rank(hanabi_core::Rank::Four);
        let public = ProspectiveTransition::clue_by(
            &view,
            view.current_player,
            target,
            clue,
            &touched_cards(&view, target, clue).unwrap(),
        );
        let (d, r) = PerspectiveProjector::new(&public, HGroupProfile::Max)
            .project(public.current_player, PerspectiveDepth::NestedRecipients)
            .unwrap();
        let inferred = infer_h_group_from_replay(&d, r, HGroupProfile::Max);
        let action = Action::Play(CardId::new(4));
        // September 10 clarification: the giver cannot supply an unknown
        // connector. No invented negative clue facts are needed to rule it out.
        assert!(!has_unresolved_requirement(&public, &inferred, action));
    }

    #[test]
    fn reviewed_save_continues_through_the_next_policy_choice() {
        // p4v0s2 turn 8: test projection mechanics, not optimality of the Save.
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s2.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(7).unwrap();
        let view = state.view_for(state.current_player()).unwrap();
        let root = Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(hanabi_core::Rank::Two),
        };
        let plan = project_h_group_plan(&view, HGroupProfile::Max, root, 32);
        assert!(plan.len() > 1, "must advance beyond the root clue");
        assert!(plan.len() <= 32);
    }

    #[test]
    fn reusing_selected_perspectives_preserves_complete_projection_evidence() {
        // A reviewed replay branch, used only to compare implementations, not
        // to assert that the Save is strategically preferred.
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s2.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(7).unwrap();
        let view = state.view_for(state.current_player()).unwrap();
        let root = Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(hanabi_core::Rank::Two),
        };
        for limit in [0, 1, 2, 16] {
            let control = crate::AnalysisControl::default();
            let original = project_h_group_plan_with_control::<false>(
                &view,
                HGroupProfile::Max,
                root,
                limit,
                &control,
            )
            .unwrap();
            let reused = project_h_group_plan_with_control::<true>(
                &view,
                HGroupProfile::Max,
                root,
                limit,
                &control,
            )
            .unwrap();
            assert_eq!(original, reused, "projection horizon {limit}");
        }
    }

    #[test]
    fn unknown_root_identity_ends_the_line_at_a_symbolic_branch() {
        let state = FullState::new_standard(2, standard_deck()).unwrap();
        let view = state.view_for(PlayerId::new(0)).unwrap();
        let outcome = project_h_group_line(
            &view,
            HGroupProfile::Max,
            Action::Play(view.hands[0][0].id),
            32,
        );

        assert_eq!(outcome.actions, 0);
        assert_eq!(outcome.stop_reason, SymbolicStopReason::UnknownIdentity);
    }

    #[test]
    fn symbolic_action_limit_has_a_distinct_stop_reason() {
        let state = FullState::new_standard(2, standard_deck()).unwrap();
        let view = state.view_for(PlayerId::new(0)).unwrap();
        let outcome = project_h_group_line(
            &view,
            HGroupProfile::Max,
            Action::Play(view.hands[0][0].id),
            0,
        );

        assert_eq!(outcome.actions, 0);
        assert_eq!(outcome.stop_reason, SymbolicStopReason::Limit);
    }
}
