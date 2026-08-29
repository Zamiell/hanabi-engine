use super::*;
use crate::{InformationSet, SupportedConvention};

fn expert_replays() -> [(&'static str, HanabiLiveReplay); 3] {
    [
        ("game-p4v0s415", expert_replay_p4v0s415()),
        ("game-p4v0s9", expert_replay_p4v0s9()),
        ("game-p4v0s2", expert_replay_p4v0s2()),
    ]
}

#[test]
fn every_semantic_move_links_to_its_documented_rule() {
    let source = include_str!("../../h_group.rs");
    let enum_body = source
        .split_once("pub enum HGroupMoveKind {")
        .expect("move-kind enum exists")
        .1
        .split_once("\n}")
        .expect("move-kind enum closes")
        .0;
    let mut preceding_docs = Vec::new();
    for line in enum_body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("///") {
            preceding_docs.push(trimmed);
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with("#[") {
            continue;
        }
        let variant = trimmed.trim_end_matches(',');
        if variant == "Retraction" {
            preceding_docs.clear();
            continue;
        }
        assert!(
            preceding_docs
                .iter()
                .any(|documentation| documentation.contains("https://hanabi.github.io/")),
            "HGroupMoveKind::{variant} must link to its exact hanabi.github.io section"
        );
        preceding_docs.clear();
    }
}

#[test]
fn every_semantic_move_has_a_production_implementation_reference() {
    let root = include_str!("../../h_group.rs");
    let enum_body = root
        .split_once("pub enum HGroupMoveKind {")
        .expect("move-kind enum exists")
        .1
        .split_once("\n}")
        .expect("move-kind enum closes")
        .0;
    let production = [
        root,
        include_str!("../action_schedule.rs"),
        include_str!("../bluff.rs"),
        include_str!("../candidate.rs"),
        include_str!("../candidate_pipeline.rs"),
        include_str!("../claims.rs"),
        include_str!("../connection.rs"),
        include_str!("../decision.rs"),
        include_str!("../effects.rs"),
        include_str!("../epistemic.rs"),
        include_str!("../facts.rs"),
        include_str!("../hand.rs"),
        include_str!("../identity.rs"),
        include_str!("../information_value.rs"),
        include_str!("../interpretation.rs"),
        include_str!("../interpretation/candidate_validation.rs"),
        include_str!("../interpretation/knowledge.rs"),
        include_str!("../knowledge_effects.rs"),
        include_str!("../model.rs"),
        include_str!("../outcome.rs"),
        include_str!("../perspective.rs"),
        include_str!("../prospective.rs"),
        include_str!("../recognition.rs"),
        include_str!("../recognition/advanced.rs"),
        include_str!("../recognition/advanced_bluffs.rs"),
        include_str!("../recognition/basic.rs"),
        include_str!("../recognition/bluffs.rs"),
        include_str!("../recognition/chop_moves.rs"),
        include_str!("../recognition/extras.rs"),
        include_str!("../recognition/late_game.rs"),
        include_str!("../recognition/order_chop.rs"),
        include_str!("../recognition/special_discards.rs"),
        include_str!("../recognition/tempo.rs"),
        include_str!("../recognition/trash.rs"),
        include_str!("../rule_engine.rs"),
        include_str!("../rules.rs"),
        include_str!("../strategic_value.rs"),
        include_str!("../symbolic_line.rs"),
        include_str!("../transition.rs"),
        include_str!("../turn_context.rs"),
    ]
    .concat();

    for line in enum_body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("///") || trimmed.starts_with("#[") {
            continue;
        }
        let variant = trimmed.trim_end_matches(',');
        if matches!(variant, "Retraction" | "Directness") {
            continue;
        }
        let needle = format!("HGroupMoveKind::{variant}");
        assert!(
            production.match_indices(&needle).count() >= 2,
            "HGroupMoveKind::{variant} appears only in the level registry; add executable production handling"
        );
    }

    // Directness is a scoring principle rather than a journal signal.
    assert!(
        include_str!("../strategic_value.rs")
            .contains("https://hanabi.github.io/level-10/#directness-principle")
    );
}

#[test]
fn every_expert_replay_prefix_satisfies_h_group_state_invariants() {
    for (fixture_name, fixture) in expert_replays() {
        for turn in 0..=u32::try_from(fixture.actions.len()).expect("replay fits in u32") {
            let state = fixture.state_at_turn(turn).expect("turn exists");
            for observer in 0..state.num_players() {
                let observer = PlayerId::new(observer);
                let deductions =
                    LogicalDeductions::new(state.view_for(observer).expect("observer exists"))
                        .expect("valid deductions");
                let replay = replay_h_group_inner(
                    &deductions,
                    HGroupProfile::Max,
                    PerspectiveDepth::ObserverOnly,
                    false,
                );
                assert_eq!(
                    replay.validate(),
                    Ok(()),
                    "invalid {fixture_name} replay at turn {turn} for {observer:?}"
                );
                assert_eq!(
                    replay.cards.facts.signal_reducible_subset(),
                    ConventionFacts::from_signals(&replay.signals),
                    "incremental signal-derived facts differ from a complete signal reduction in {fixture_name} at turn {turn} for {observer:?}"
                );
            }
        }
    }
}

#[test]
fn canonical_knowledge_program_is_stable_for_every_expert_prefix() {
    for (fixture_name, fixture) in expert_replays() {
        for turn in 0..=u32::try_from(fixture.actions.len()).expect("replay fits in u32") {
            let state = fixture.state_at_turn(turn).expect("turn exists");
            for observer in 0..state.num_players() {
                let observer = PlayerId::new(observer);
                let deductions =
                    LogicalDeductions::new(state.view_for(observer).expect("observer exists"))
                        .expect("valid deductions");
                let replay = replay_h_group_inner(
                    &deductions,
                    HGroupProfile::Max,
                    PerspectiveDepth::ObserverOnly,
                    false,
                );
                let rebuilt = build_convention_knowledge(&deductions, &replay);
                assert_eq!(
                    rebuilt.effects(),
                    replay.knowledge.effects(),
                    "incremental knowledge changed after rebuilding {fixture_name} at turn {turn} for {observer:?}"
                );
                assert_eq!(
                    rebuilt.project(&deductions),
                    convention_card_inferences(&deductions, &replay),
                    "pure owner projection diverged in {fixture_name} at turn {turn} for {observer:?}"
                );
                for transition in &replay.transitions {
                    assert!(
                        transition
                            .delta
                            .knowledge_changes
                            .iter()
                            .all(|effect| effect.source().turn() == transition.turn)
                    );
                }
                for card in &deductions.view().hands[observer.index()] {
                    assert_eq!(
                        replay.knowledge.effects_for(card.id).collect::<Vec<_>>(),
                        replay
                            .knowledge
                            .effects()
                            .iter()
                            .filter(|effect| effect.card() == card.id)
                            .collect::<Vec<_>>()
                    );
                }
            }
        }
    }
}

#[test]
fn demonstrated_relational_claims_leave_a_nonempty_exact_belief() {
    // By this position an earlier Purple-2 OneOf claim has been demonstrated
    // by a card that left the hand. The surviving candidates must be excluded
    // from Purple 2, not incorrectly forced to become it.
    let state = expert_replay_p4v0s415()
        .state_at_turn(42)
        .expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("current player has a view");
    let deductions = LogicalDeductions::new(view.clone()).expect("logical position");
    let analysis = SupportedConvention::HGroup(HGroupProfile::Max).analyze(&deductions);
    let information = InformationSet::new(&view).expect("information set is valid");
    assert!(
        information
            .world_count_up_to(&analysis.belief_constraints, 1)
            .worlds()
            > 0,
        "resolved OneOf claims must not contradict the logical information set"
    );
}

#[test]
fn owner_projection_cannot_resurrect_an_identity_rejected_by_canonical_focus() {
    for (fixture_name, fixture) in expert_replays() {
        for turn in 0..=u32::try_from(fixture.actions.len()).expect("replay fits in u32") {
            let state = fixture.state_at_turn(turn).expect("turn exists");
            for observer_index in 0..state.num_players() {
                let observer = PlayerId::new(observer_index);
                let deductions =
                    LogicalDeductions::new(state.view_for(observer).expect("observer exists"))
                        .expect("valid deductions");
                let replay = replay_h_group_inner(
                    &deductions,
                    HGroupProfile::Max,
                    PerspectiveDepth::ObserverOnly,
                    false,
                );
                for effect in replay.knowledge.effects() {
                    let CardKnowledgeEffect::RestrictDomain {
                        card,
                        allowed,
                        source:
                            KnowledgeSource::Clue(clue_turn) | KnowledgeSource::CurrentFocus(clue_turn),
                    } = effect
                    else {
                        continue;
                    };
                    let Some(clue) = replay
                        .clues
                        .iter()
                        .rev()
                        .find(|clue| clue.turn == *clue_turn && clue.focus == *card)
                    else {
                        continue;
                    };
                    assert!(
                        allowed.without(clue.focus_identities).is_empty(),
                        "owner projection resurrected identities rejected by canonical focus in {fixture_name}, turn {turn}, observer {observer:?}: clue={clue:?}, effect={effect:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn generated_legal_histories_preserve_knowledge_and_state_invariants() {
    for seed in 0..4_usize {
        let mut deck = standard_deck();
        let deck_len = deck.len();
        deck.rotate_left((seed * 7) % deck_len);
        let mut state = FullState::new_standard(4, deck).expect("generated deal is valid");
        let mut selector = u64::try_from(seed + 1).expect("small seed");
        for turn in 0..12_u32 {
            for observer_index in 0..state.num_players() {
                let observer = PlayerId::new(observer_index);
                let deductions =
                    LogicalDeductions::new(state.view_for(observer).expect("observer exists"))
                        .expect("generated logical state is valid");
                let replay = replay_h_group_inner(
                    &deductions,
                    HGroupProfile::Max,
                    PerspectiveDepth::ObserverOnly,
                    false,
                );
                assert_eq!(
                    replay.validate(),
                    Ok(()),
                    "generated seed {seed}, turn {turn}, observer {observer:?}"
                );
                let rebuilt = build_convention_knowledge(&deductions, &replay);
                assert_eq!(rebuilt.effects(), replay.knowledge.effects());
            }
            if state.is_terminal() {
                break;
            }
            let view = state
                .view_for(state.current_player())
                .expect("current player exists");
            let legal = view.legal_actions();
            selector = selector
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let index =
                usize::try_from(selector % u64::try_from(legal.len()).expect("legal count"))
                    .expect("selected action index fits");
            state.apply(legal[index]).expect("selected action is legal");
        }
    }
}

#[test]
fn transition_proposals_are_a_unique_causal_partition_of_post_event_signals() {
    let fixture = expert_replay_p4v0s415();
    for turn in 0..=u32::try_from(fixture.actions.len()).expect("replay fits in u32") {
        let state = fixture.state_at_turn(turn).expect("turn exists");
        let observer = state.current_player();
        let deductions = LogicalDeductions::new(state.view_for(observer).expect("observer exists"))
            .expect("valid deductions");
        let replay = replay_h_group_inner(
            &deductions,
            HGroupProfile::Max,
            PerspectiveDepth::ObserverOnly,
            false,
        );
        let mut proposed = Vec::new();
        for transition in &replay.transitions {
            for proposal in &transition.proposals {
                for signal in &replay.signals[proposal.signal_range.clone()] {
                    assert!(replay.signals.contains(signal));
                    assert!(
                        !proposed.contains(signal),
                        "signal proposed twice: {signal:?}"
                    );
                    proposed.push(signal.clone());
                }
            }
        }
    }
}

#[test]
fn every_legal_replay_clue_is_either_admitted_or_explained() {
    let fixture = expert_replay_p4v0s415();
    for turn in 0..u32::try_from(fixture.actions.len()).expect("replay fits in u32") {
        let state = fixture.state_at_turn(turn).expect("turn exists");
        let actor = state.current_player();
        let deductions = LogicalDeductions::new(state.view_for(actor).expect("actor exists"))
            .expect("valid deductions");
        let analysis = SupportedConvention::HGroup(HGroupProfile::Max).analyze(&deductions);
        for action in deductions
            .view()
            .legal_actions()
            .into_iter()
            .filter(|action| matches!(action, Action::Clue { .. }))
        {
            let admitted = analysis
                .actions
                .iter()
                .any(|candidate| candidate.action == action);
            let rejection = analysis
                .rejected_actions
                .iter()
                .find(|rejected| rejected.action == action);
            assert_ne!(
                admitted,
                rejection.is_some(),
                "clue must have exactly one disposition at turn {turn}: {action:?}; analysis={analysis:#?}"
            );
        }
    }
}

#[test]
fn every_replay_clue_uses_the_same_hypothetical_and_recipient_interpretation() {
    let fixture = expert_replay_p4v0s415();
    for turn in 0..u32::try_from(fixture.actions.len()).expect("replay fits in u32") {
        let action = replay_action_at_turn(&fixture, turn);
        let Action::Clue { target, clue } = action else {
            continue;
        };
        let before = fixture.state_at_turn(turn).expect("turn exists");
        let giver = before.current_player();
        let mut source = before.view_for(giver).expect("giver exists");
        // Resolve the giver's physical hand only inside this equivalence test.
        // Production projection instead quantifies over these possible worlds;
        // choosing the fixture's true world lets us compare one identical
        // world on both sides of the transition.
        for card in &mut source.hands[giver.index()] {
            card.identity = before.card(card.id);
        }
        let touched = source.hands[target.index()]
            .iter()
            .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
            .map(|card| card.id)
            .collect::<Vec<_>>();
        let hypothetical = ProspectiveTransition::clue_by(&source, giver, target, clue, &touched);
        let (hypothetical_deductions, hypothetical_replay) =
            PerspectiveProjector::project_resolved_owned(hypothetical, HGroupProfile::Max, target)
                .expect("hypothetical recipient projection succeeds");
        let hypothetical_inferences = infer_h_group_from_replay(
            &hypothetical_deductions,
            hypothetical_replay,
            HGroupProfile::Max,
        );

        let after = fixture.state_at_turn(turn + 1).expect("next turn exists");
        let actual_deductions =
            LogicalDeductions::new(after.view_for(target).expect("recipient exists"))
                .expect("actual recipient deductions are valid");
        let actual_replay = replay_h_group_inner(
            &actual_deductions,
            HGroupProfile::Max,
            PerspectiveDepth::ObserverOnly,
            false,
        );
        let actual_inferences =
            infer_h_group_from_replay(&actual_deductions, actual_replay, HGroupProfile::Max);

        assert_eq!(hypothetical_inferences.clues, actual_inferences.clues);
        assert_eq!(
            hypothetical_inferences.playable_now,
            actual_inferences.playable_now,
            "play obligations diverged after move {}",
            turn + 1
        );
        assert_eq!(
            hypothetical_inferences.saved_cards().collect::<Vec<_>>(),
            actual_inferences.saved_cards().collect::<Vec<_>>()
        );
        assert_eq!(
            hypothetical_inferences.connection,
            actual_inferences.connection
        );
        assert_eq!(hypothetical_inferences.signals, actual_inferences.signals);
        assert_eq!(
            hypothetical_inferences.cards.len(),
            actual_inferences.cards.len()
        );
        for (predicted, actual) in hypothetical_inferences
            .cards
            .iter()
            .zip(&actual_inferences.cards)
        {
            assert_eq!(predicted.card, actual.card);
            assert_eq!(predicted.focused, actual.focused);
            assert_eq!(predicted.saved, actual.saved);
            assert_eq!(predicted.finessed, actual.finessed);
            assert_eq!(predicted.play_obligation, actual.play_obligation);
            assert!(
                actual.identities.without(predicted.identities).is_empty(),
                "the recipient may eliminate identities by seeing the giver's physical hand, but may not gain convention meaning the giver failed to predict after move {}: predicted={predicted:?}, actual={actual:?}",
                turn + 1
            );
        }
    }
}

#[test]
fn observer_epistemic_state_is_invariant_to_leaked_own_hand_truth() {
    let fixture = expert_replay_p4v0s415();
    for turn in 0..=u32::try_from(fixture.actions.len()).expect("replay fits in u32") {
        let state = fixture.state_at_turn(turn).expect("turn exists");
        for player in 0..state.num_players() {
            let observer = PlayerId::new(player);
            let source = state.view_for(observer).expect("observer exists");
            let mut leaked = source.clone();
            for card in &mut leaked.hands[observer.index()] {
                card.identity = state.card(card.id);
            }

            let project = |view: &PlayerView| {
                let (deductions, replay) = PerspectiveProjector::new(view, HGroupProfile::Max)
                    .project(observer, PerspectiveDepth::ObserverOnly)
                    .expect("observer projection succeeds");
                let inferred = infer_h_group_from_replay(&deductions, replay, HGroupProfile::Max);
                EpistemicState::from_analysis(&deductions, &inferred)
            };
            assert_eq!(
                project(&source),
                project(&leaked),
                "own hidden truth changed observer knowledge at turn {turn} for {observer:?}"
            );
        }
    }
}

#[test]
fn cancelling_a_promise_atomically_retracts_its_materialized_effects() {
    let mut pending = ConnectionManager::default();
    let card = CardId::new(5);
    let focus = CardId::new(9);
    let promise = pending.start(
        3,
        ConnectionObligation {
            promise: PromiseId::UNASSIGNED,
            actor: PlayerId::new(1),
            cards: vec![card],
            expected: Card::new(Suit::Red, Rank::Two),
            focus_identity: Card::new(Suit::Red, Rank::Three),
            kind: HGroupConnectionKind::Finesse,
            focus,
            step: 0,
        },
    );
    let source = EffectSource::Promise(promise);
    let mut invisible = ProvenancedCardSet::default();
    let mut playing = ProvenancedCardSet::default();
    let mut forced = ProvenancedCardSet::default();
    invisible.insert_from(source, card);
    playing.insert_from(source, focus);

    let transition_start = pending.transitions().len();
    pending.cancel_where(
        4,
        ConnectionTransitionReason::FocusInvalidated,
        |connection| connection.promise == promise,
    );
    reconcile_connection_fact_lifecycles(
        &pending,
        transition_start,
        &mut invisible,
        &mut playing,
        &mut forced,
    );

    assert!(!invisible.contains(&card));
    assert!(!playing.contains(&focus));
    assert_eq!(invisible.retractions().len(), 1);
    assert_eq!(playing.retractions().len(), 1);
}

#[test]
fn production_transition_delta_records_exact_clue_cards() {
    let fixture = expert_replay_p4v0s415();
    let state = fixture.state_at_turn(1).expect("first move is legal");
    let observer = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(observer).expect("observer exists"))
        .expect("valid deductions");
    let expected = match &deductions.view().history[0].event {
        ObservedEvent::Clued { touched, .. } => touched.iter().copied().collect::<CardSet>(),
        event => panic!("first replay event should be a clue, got {event:?}"),
    };
    let replay = replay_h_group(&deductions, HGroupProfile::Max);
    let transition = replay
        .transitions
        .iter()
        .find(|transition| transition.turn == 0)
        .expect("first clue has a production transition");
    let explicitly_clued = transition
        .delta
        .card_changes
        .iter()
        .filter(|change| {
            change.fact == MaterializedCardFact::ExplicitlyClued
                && change.kind == FactChangeKind::Added
        })
        .map(|change| change.card)
        .collect::<CardSet>();

    assert_eq!(explicitly_clued, expected);
}
