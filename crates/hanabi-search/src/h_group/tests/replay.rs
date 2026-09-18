use super::*;

#[test]
fn first_seed_purple_line_uses_fives_chop_move() {
    // Reviewed p4v0s1 branch, turns 14-22. This does not change the fixture:
    // green to Donald, b4, g1, discard y4, then purple to Donald.
    let mut state = expert_replay_p4v0s1().state_at_turn(14).unwrap();
    for action in [
        Action::Play(CardId::new(11)),
        Action::Play(CardId::new(20)),
        Action::Discard(CardId::new(0)),
    ] {
        state.apply(action).unwrap();
    }
    let purple = Action::Clue {
        target: PlayerId::new(3),
        clue: Clue::Suit(Suit::Purple),
    };
    let five = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Rank(Rank::Five),
    };
    let source = state.view_for(state.current_player()).unwrap();
    let mut public = source.clone();
    for action in [purple, Action::Discard(CardId::new(9))] {
        let (d, r) = PerspectiveProjector::new(&public, HGroupProfile::Max)
            .project(public.current_player, PerspectiveDepth::NestedRecipients)
            .unwrap();
        let notes = infer_h_group_from_replay(&d, r, HGroupProfile::Max);
        public = super::super::symbolic_line::apply_symbolic_action(
            &public,
            &d,
            &notes,
            public.current_player,
            action,
        )
        .unwrap()
        .0;
    }
    let (donald, _) = PerspectiveProjector::new(&public, HGroupProfile::Max)
        .project(PlayerId::new(3), PerspectiveDepth::NestedRecipients)
        .unwrap();
    let candidates = h_group_clue_candidates(&donald, HGroupProfile::Max);
    let purple_to_cathy = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Suit(Suit::Purple),
    };
    let candidate = candidates
        .iter()
        .find(|c| c.action == purple_to_cathy)
        .unwrap();
    assert_eq!(candidate.purpose(), CluePurpose::Play);
    assert_eq!(
        candidates
            .iter()
            .find(|c| c.action == five)
            .unwrap()
            .move_kind(),
        Some(HGroupMoveKind::FiveChopMove)
    );
    let (_, risky) = super::super::symbolic_line::project_h_group_projection(
        &public,
        HGroupProfile::Max,
        purple_to_cathy,
        3,
        &crate::AnalysisControl::default(),
    )
    .unwrap();
    assert_eq!(
        risky
            .steps
            .iter()
            .map(|step| step.projected.action)
            .collect::<Vec<_>>(),
        vec![purple_to_cathy, Action::Play(CardId::new(24))]
    );
    let discard = risky
        .unresolved_discard
        .expect("Bob must Scream Discard; his card stays unknown to Bob");
    assert_eq!(discard.card, CardId::new(5));
    assert!(discard.bottom_deck_risk);
    assert!(discard.required_protection);
    let (_, projection) = super::super::symbolic_line::project_h_group_projection(
        &source,
        HGroupProfile::Max,
        purple,
        5,
        &crate::AnalysisControl::default(),
    )
    .unwrap();
    assert_eq!(
        projection
            .steps
            .iter()
            .map(|step| step.projected.action)
            .collect::<Vec<_>>(),
        vec![
            purple,
            Action::Discard(CardId::new(9)),
            five,
            Action::Play(CardId::new(24)),
            Action::Play(CardId::new(17))
        ]
    );
}

#[test]
fn first_seed_red_line_protects_the_five_before_the_next_play() {
    // User-reviewed p4v0s1 branch: green to Donald at 14, b4, g1,
    // discard y4. Red to Cathy at 18 must not let her p5 be discarded.
    let mut state = expert_replay_p4v0s1().state_at_turn(14).unwrap();
    for action in [
        Action::Play(CardId::new(11)),
        Action::Play(CardId::new(20)),
        Action::Discard(CardId::new(0)),
    ] {
        state.apply(action).unwrap();
    }
    let red = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Suit(Suit::Red),
    };
    let (_, projection) = super::super::symbolic_line::project_h_group_projection(
        &state.view_for(state.current_player()).unwrap(),
        HGroupProfile::Max,
        red,
        5,
        &crate::AnalysisControl::default(),
    )
    .unwrap();
    assert_eq!(
        projection
            .steps
            .iter()
            .map(|step| step.projected.action)
            .collect::<Vec<_>>(),
        vec![
            red,
            Action::Discard(CardId::new(9)),
            Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Rank(Rank::Five)
            },
            Action::Play(CardId::new(24)),
            Action::Play(CardId::new(17)),
        ],
    );
}

#[test]
fn first_seed_play_is_not_penalized_for_a_longer_discard_forecast() {
    // Human-reviewed p4v0s1 turn 12: play the promised y2. Both lines
    // eventually reach Donald's unknown chop; the longer line is not more
    // dangerous merely because its forecast sees more intervening turns.
    let state = expert_replay_p4v0s1().state_at_turn(11).unwrap();
    let analysis = crate::analyze_position(
        &state.view_for(state.current_player()).unwrap(),
        crate::SupportedConvention::HGroup(HGroupProfile::Max),
        crate::PlannerConfig::default(),
    )
    .unwrap();
    assert_eq!(
        analysis.planner.best_action,
        Action::Play(CardId::new(15)),
        "{:#?}",
        analysis.planner.comparisons
    );
}

#[test]
fn first_seed_blank_draw_does_not_enable_an_unsafe_double_bluff() {
    // Reviewed p4v0s1 turn 7 counterfactual. Unknown draws must not add
    // purple touches; Cathy cannot bluff Donald's unplayable finesse card.
    let state = expert_replay_p4v0s1().state_at_turn(6).unwrap();
    let view = state.view_for(PlayerId::new(2)).unwrap();
    let (outcome, evidence) = super::super::symbolic_line::project_h_group_projection(
        &view,
        HGroupProfile::Max,
        Action::Play(CardId::new(8)),
        12,
        &crate::AnalysisControl::default(),
    )
    .unwrap();
    assert!(evidence.clue_branches.is_empty());
    assert_eq!(outcome.strikes, 0);
    assert!(
        evidence
            .steps
            .iter()
            .all(|step| match step.projected.action {
                Action::Play(card) | Action::Discard(card) =>
                    view.hands.iter().flatten().any(|known| known.id == card),
                Action::Clue { .. } => true,
            }),
        "a future draw cannot be credited as a known physical card"
    );
    let mut public = view.clone();
    // Execute the reviewed visible prefix, without drawing actual deck cards.
    for action in [
        Action::Play(CardId::new(8)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(3),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(6)),
    ] {
        let (d, r) =
            super::super::perspective::PerspectiveProjector::new(&public, HGroupProfile::Max)
                .project(public.current_player, PerspectiveDepth::NestedRecipients)
                .unwrap();
        let notes = infer_h_group_from_replay(&d, r, HGroupProfile::Max);
        public = super::super::symbolic_line::apply_symbolic_action(
            &public,
            &d,
            &notes,
            public.current_player,
            action,
        )
        .unwrap()
        .0;
    }
    let d = LogicalDeductions::new(public).unwrap();
    let candidates = h_group_clue_candidates(&d, HGroupProfile::Max);
    assert!(
        !candidates.iter().any(|candidate| candidate.action
            == Action::Clue {
                target: PlayerId::new(1),
                clue: Clue::Suit(Suit::Purple),
            }),
        "{candidates:#?}"
    );
}

#[test]
fn first_seed_four_charm_does_not_secure_visibly_false_connectors() {
    // Reviewed p4v0s1 opening comparison: the 4 Charm saves g4 and b4.
    // Recipient connector assumptions that disagree with Alice's visible
    // cards cannot count as additional saved cards or protected BDR.
    let state = expert_replay_p4v0s1().state_at_turn(0).unwrap();
    let view = state.view_for(PlayerId::new(0)).unwrap();
    let (outcome, _) = super::super::symbolic_line::project_h_group_projection(
        &view,
        HGroupProfile::Max,
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Four),
        },
        2,
        &crate::AnalysisControl::default(),
    )
    .unwrap();
    let value = outcome.position_value.unwrap();
    assert_eq!(value.secured_future_plays, 2, "{value:#?}");
    // The played b1 also counts as protected; fictitious connectors do not.
    assert_eq!(value.protected_bottom_deck_risks, 3, "{value:#?}");
}

#[test]
fn reviewed_rank_two_opening_queues_purple_before_an_early_five_save() {
    fn check(evidence: &super::super::plan::ProjectionEvidence, expected: &[Action]) -> usize {
        let actions = evidence
            .steps
            .iter()
            .map(|step| step.projected.action)
            .collect::<Vec<_>>();
        assert_eq!(actions, expected[..actions.len()], "{evidence:#?}");
        assert!(
            evidence
                .steps
                .iter()
                .all(|step| step.consequences.strikes == 0)
        );
        evidence
            .clue_branches
            .iter()
            .map(|branch| check(&branch.continuation, expected))
            .max()
            .unwrap_or(0)
            .max(actions.len())
    }
    // User-reviewed p4v0s2 turn 2 line, projected from Bob's information.
    // Unknown draws and Bob's hand must not be filled from simulator truth.
    let state = expert_replay_p4v0s2().state_at_turn(1).unwrap();
    let view = state.view_for(PlayerId::new(1)).unwrap();
    let root = Action::Clue {
        target: PlayerId::new(3),
        clue: Clue::Rank(Rank::Two),
    };
    let (_, evidence) = super::super::symbolic_line::project_h_group_projection(
        &view,
        HGroupProfile::Max,
        root,
        14,
        &crate::AnalysisControl::default(),
    )
    .unwrap();
    let expected = [
        root,
        Action::Play(CardId::new(11)), // p1
        Action::Play(CardId::new(15)), // g1
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(7)),  // g2
        Action::Play(CardId::new(10)), // r1
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(1)), // b1
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(9)),  // p2
        Action::Play(CardId::new(13)), // r2
        Action::Play(CardId::new(0)),  // p3
        Action::Clue {
            target: PlayerId::new(3),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(8)), // r4, turn 15 (not turn 11)
    ];
    assert_eq!(
        check(&evidence, &expected),
        expected.len(),
        "must actually reach the reviewed discard"
    );
}

#[test]
fn fourth_replay_anxiety_play_does_not_layer_a_gentlemans_discard() {
    // p4v0s3 turn 34: Alice's forced r3 play does not ask Bob to play g4.
    // Cathy already owns the exact g4 transferred by Bob on turn 26.
    let state = expert_replay_p4v0s3().state_at_turn(33).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let d = LogicalDeductions::new(view).unwrap();
    let inferred = infer_h_group(&d, HGroupProfile::Max);
    assert!(inferred.connection.is_none(), "{inferred:#?}");
    assert_eq!(
        select_h_group_action(&d, HGroupProfile::Max),
        Some(Action::Play(CardId::new(30))),
        "{inferred:#?}"
    );
}

#[test]
fn fourth_replay_rank_four_locks_alice_into_the_leftmost_anxiety_play() {
    // Human-reviewed p4v0s3 turns 30–33: the 4s clue leaves both untouched
    // cards chop-moved. At zero tokens Alice must play leftmost slot 3 (r3).
    // https://hanabi.github.io/level-9/#the-anxiety-play-forcing-a-locked-player-to-play
    let state = expert_replay_p4v0s3().state_at_turn(32).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    assert_eq!(view.clue_tokens, 0);
    let d = LogicalDeductions::new(view).unwrap();
    let inferred = infer_h_group(&d, HGroupProfile::Max);
    assert_eq!(
        select_h_group_action(&d, HGroupProfile::Max),
        Some(Action::Play(CardId::new(1))),
        "{inferred:#?}"
    );
    let state = expert_replay_p4v0s3().state_at_turn(29).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let d = LogicalDeductions::new(view.clone()).unwrap();
    let four = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Rank(Rank::Four),
    };
    let candidates = h_group_clue_candidates(&d, HGroupProfile::Max);
    assert!(
        candidates.iter().any(|candidate| candidate.action == four),
        "{candidates:#?}"
    );
    let analysis = crate::analyze_position(
        &view,
        crate::SupportedConvention::HGroup(HGroupProfile::Max),
        crate::PlannerConfig::default(),
    )
    .unwrap();
    // This regression owns admission and the forced-play prefix. Full replay
    // parity still checks the root choice and the unresolved later 4 identity.
    let projection = &analysis
        .planner
        .root_actions
        .iter()
        .find(|candidate| candidate.action == four)
        .unwrap()
        .projection;
    assert_eq!(
        projection
            .steps
            .iter()
            .take(4)
            .map(|step| step.projected.action)
            .collect::<Vec<_>>(),
        vec![
            four,
            Action::Play(CardId::new(9)),
            Action::Play(CardId::new(32)),
            Action::Play(CardId::new(1))
        ],
        "{projection:#?}"
    );
}

#[test]
fn fourth_replay_turn_twenty_six_does_not_assume_an_unfinished_play_line_is_safe() {
    // Reviewed GD of g4. A longer GD forecast must not lose to the shorter
    // play forecast merely because it reaches a later loss. An unresolved
    // clue in the play line is not evidence that the continuation is safe.
    let state = expert_replay_p4v0s3().state_at_turn(25).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let analysis = crate::analyze_position(
        &view,
        crate::SupportedConvention::HGroup(HGroupProfile::Max),
        crate::PlannerConfig::default(),
    )
    .unwrap();
    assert_eq!(
        analysis.planner.best_action,
        Action::Discard(CardId::new(7)),
        "{:#?}",
        analysis.planner.comparisons
    );
}

#[test]
fn fourth_replay_turn_thirty_visible_replacements_remove_discard_bdr() {
    // Human-reviewed p4v0s3 turn 30: all needed non-critical identities have
    // visible replacements. Bob's unknown chop must not acquire BDR merely
    // because it could contain an unseen critical card. The blue line loses r4.
    let state = expert_replay_p4v0s3().state_at_turn(29).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let analysis = crate::analyze_position(
        &view,
        crate::SupportedConvention::HGroup(HGroupProfile::Max),
        crate::PlannerConfig::default(),
    )
    .unwrap();
    let discard = analysis
        .planner
        .root_actions
        .iter()
        .find(|candidate| candidate.action == Action::Discard(CardId::new(5)))
        .unwrap();
    assert_eq!(discard.projection.forecast_discard_risk(), Some(0));
    let blue = analysis
        .planner
        .root_actions
        .iter()
        .find(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(0),
                    clue: Clue::Suit(Suit::Blue),
                }
        })
        .unwrap();
    assert!(blue.projection.steps.iter().any(|step| {
        step.consequences.bottom_deck_risk == Some(Card::new(Suit::Red, Rank::Four))
    }));
    // The updated human review prefers 4s, not discard. Keep this test about
    // risk assessment; the replay parity test owns the chosen-move expectation.
}

#[test]
fn fourth_replay_red_one_precedes_transferred_green_four() {
    // Human-reviewed p4v0s3 turn 27: the GD establishes g4, but r1 leads
    // into Cathy's clued r2. A globally known transfer is not an urgent finesse.
    let state = expert_replay_p4v0s3().state_at_turn(26).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let deductions = LogicalDeductions::new(view.clone()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    assert_eq!(
        ordered_playable_cards(&view, &inferred, HGroupProfile::Max).first(),
        Some(&CardId::new(29)),
        "{inferred:#?}"
    );
    let analysis = crate::analyze_position(
        &view,
        crate::SupportedConvention::HGroup(HGroupProfile::Max),
        crate::PlannerConfig::default(),
    )
    .unwrap();
    assert_eq!(analysis.planner.best_action, Action::Play(CardId::new(29)));
}

#[test]
fn fourth_replay_red_clue_checks_cathys_actual_decision_turn() {
    // p4v0s3 turn 25: r1/r3 is unresolved immediately after red. Bob acts
    // before Cathy; admission must evaluate his response before rejecting r1.
    let state = expert_replay_p4v0s3().state_at_turn(24).unwrap();
    let d = LogicalDeductions::new(state.view_for(state.current_player()).unwrap()).unwrap();
    let red = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Suit(Suit::Red),
    };
    let after = ProspectiveTransition::clue(
        d.view(),
        PlayerId::new(2),
        Clue::Suit(Suit::Red),
        &[CardId::new(9), CardId::new(29)],
    );
    let arrival = super::super::prospective::project_recipient_arrival(
        &after,
        HGroupProfile::Max,
        PlayerId::new(2),
        &[CardId::new(9), CardId::new(29)],
    )
    .unwrap();
    let recipient = arrival.projection(PlayerId::new(2)).unwrap();
    assert_eq!(recipient.deductions.view().turn, 26);
    assert_eq!(
        recipient
            .inferred
            .cards
            .iter()
            .find(|card| card.card == CardId::new(29))
            .unwrap()
            .identities,
        IdentitySet::singleton(Card::new(Suit::Red, Rank::One))
    );
    let hazard = super::super::prospective::prospective_clue_hazard(
        d.view(),
        HGroupProfile::Max,
        PlayerId::new(2),
        CardId::new(29),
        Clue::Suit(Suit::Red),
        &[CardId::new(9), CardId::new(29)],
        true,
    );
    assert_eq!(hazard, None);
    let candidates = h_group_clue_candidates(&d, HGroupProfile::Max);
    assert!(
        candidates.iter().any(|candidate| candidate.action == red),
        "{candidates:#?}"
    );
}

#[test]
fn fourth_replay_two_save_preserves_positional_red_options() {
    // Human-reviewed p4v0s3 turn 24: Bob/Cathy both have r1 on Finesse
    // Position. Saving r2 leaves a possible r1 -> prompted r2 -> r3 line.
    // Donald may hold r3, but neither its identity nor a future Bluff is known.
    let state = expert_replay_p4v0s3().state_at_turn(23).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let d = LogicalDeductions::new(view.clone()).unwrap();
    let candidates = h_group_clue_candidates(&d, HGroupProfile::Max);
    let candidate = |rank| {
        *candidates
            .iter()
            .find(|candidate| {
                candidate.action
                    == Action::Clue {
                        target: PlayerId::new(2),
                        clue: Clue::Rank(rank),
                    }
            })
            .unwrap()
    };
    let direct =
        super::super::positional_value::evaluate(&d, HGroupProfile::Max, candidate(Rank::One));
    let save =
        super::super::positional_value::evaluate(&d, HGroupProfile::Max, candidate(Rank::Two));
    assert_eq!(direct.foregone_blind_plays, 1);
    assert_eq!(save.foregone_blind_plays, 0);
    assert_eq!(save.conditional_prompt_chains, 1);
    assert!(view.hands[3].iter().all(|card| card.identity.is_none()));
    let analysis = crate::analyze_position(
        &view,
        crate::SupportedConvention::HGroup(HGroupProfile::Max),
        crate::PlannerConfig::default(),
    )
    .unwrap();
    assert_eq!(
        analysis.planner.best_action,
        candidate(Rank::Two).action,
        "{analysis:#?}"
    );
}

#[test]
fn fourth_replay_rank_four_recipient_does_not_stomp_the_red_one() {
    // Reviewed alternative at p4v0s3 turn 24: Donald's 4s promises Alice
    // r4 through Cathy's r1. Alice must not spend another clue naming r1.
    let state = expert_replay_p4v0s3().state_at_turn(23).unwrap();
    let source = state.view_for(PlayerId::new(3)).unwrap();
    let source = ProspectiveTransition::clue_by(
        &source,
        PlayerId::new(3),
        PlayerId::new(0),
        Clue::Rank(Rank::Four),
        &[CardId::new(3), CardId::new(22)],
    );
    let (d, replay) = PerspectiveProjector::new(&source, HGroupProfile::Max)
        .project(PlayerId::new(0), PerspectiveDepth::NestedRecipients)
        .unwrap();
    let candidates = h_group_clue_candidates(&d, HGroupProfile::Max);
    let inferred = infer_h_group(&d, HGroupProfile::Max);
    assert_eq!(
        inferred
            .cards
            .iter()
            .find(|note| note.card == CardId::new(3))
            .unwrap()
            .identities,
        IdentitySet::singleton(Card::new(Suit::Red, Rank::Four)),
        "{inferred:#?}"
    );
    assert!(
        !candidates.iter().any(|candidate| candidate.action
            == Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Rank(Rank::One),
            }),
        "candidates: {candidates:#?}; pending: {:#?}; clues: {:#?}",
        replay.pending_connections,
        replay.clues
    );
}

#[test]
fn fourth_replay_two_save_does_not_invent_a_prompt_on_a_chop_moved_card() {
    let state = expert_replay_p4v0s3().state_at_turn(21).unwrap();
    let view = state.view_for(PlayerId::new(1)).unwrap();
    let d = LogicalDeductions::new(view.clone()).unwrap();
    // Human-reviewed turn 22. The old precheck treated Cathy's Chop-Moved
    // p4 as a potential r1 Prompt using only its literal clue mask. Trust the
    // actual recipient interpretation and its ordinary safety validation.
    let candidates = h_group_clue_candidates(&d, HGroupProfile::Max);
    assert!(
        candidates.iter().any(|candidate| candidate.action
            == Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Rank(Rank::Two)
            }
            && candidate.is_save()),
        "{candidates:#?}"
    );
}

#[test]
fn fourth_replay_cannot_tempo_a_chop_moved_duplicate_purple_three() {
    // Turn 20: Alice's p3 is Chop Moved, not positively clued. Donald's
    // known p3 is already playing, so purple cannot bypass Good Touch by
    // being reclassified as a Tempo Clue Chop Move.
    let state = expert_replay_p4v0s3().state_at_turn(19).unwrap();
    let d = LogicalDeductions::new(state.view_for(PlayerId::new(3)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&d, HGroupProfile::Max);
    assert!(
        !candidates.iter().any(|candidate| candidate.action
            == Action::Clue {
                target: PlayerId::new(0),
                clue: Clue::Suit(Suit::Purple)
            }),
        "{candidates:#?}"
    );
}

#[test]
fn fourth_replay_green_gets_more_new_plays_than_rank_four() {
    let state = expert_replay_p4v0s3().state_at_turn(16).unwrap();
    let d = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&d, HGroupProfile::Max);
    for (clue, count) in [(Clue::Suit(Suit::Green), 2), (Clue::Rank(Rank::Four), 1)] {
        let candidate = candidates
            .iter()
            .find(|candidate| {
                candidate.action
                    == Action::Clue {
                        target: PlayerId::new(1),
                        clue,
                    }
            })
            .unwrap();
        let outcome = super::super::strategic_value::scheduled_clue_outcome(
            d.view(),
            HGroupProfile::Max,
            candidate,
        )
        .unwrap();
        assert_eq!(outcome.action_coverage, count, "{clue:?}: {outcome:#?}");
    }
}

#[test]
fn fourth_replay_green_continuation_plays_instead_of_inventing_an_urgent_ejection() {
    // Reviewed turn 17 continuation, from Alice's perspective: Bob plays
    // the transferred p2, then Cathy plays g2 to advance the green Finesse.
    // In particular Alice's hidden cards and Bob's new draw stay unknown.
    let state = expert_replay_p4v0s3().state_at_turn(16).unwrap();
    let source = state.view_for(PlayerId::new(0)).unwrap();
    let source = ProspectiveTransition::clue_by(
        &source,
        PlayerId::new(0),
        PlayerId::new(1),
        Clue::Suit(Suit::Green),
        &[CardId::new(7)],
    );
    let source = ProspectiveTransition::successful_play(
        &source,
        PlayerId::new(1),
        CardId::new(18),
        Card::new(Suit::Purple, Rank::Two),
    );
    let (d, _) = PerspectiveProjector::new(&source, HGroupProfile::Max)
        .project(PlayerId::new(2), PerspectiveDepth::NestedRecipients)
        .unwrap();
    assert!(!prospective_play_has_unsafe_inference(
        &d,
        HGroupProfile::Max,
        CardId::new(11)
    ));
    assert_eq!(
        select_h_group_action(&d, HGroupProfile::Max),
        Some(Action::Play(CardId::new(11)))
    );
    let candidates = h_group_clue_candidates(&d, HGroupProfile::Max);
    assert!(!candidates.iter().any(|candidate| candidate.action
        == Action::Clue {
            target: PlayerId::new(3),
            clue: Clue::Suit(Suit::Blue),
        }));
    assert!(
        !candidates
            .iter()
            .any(|candidate| candidate.target() == PlayerId::new(3) && candidate.is_urgent_save())
    );
    assert_eq!(
        ordered_h_group_actions(&d, HGroupProfile::Max).first(),
        Some(&Action::Play(CardId::new(11)))
    );
    assert_eq!(
        crate::planner::choose_projected_follow_up(
            &d,
            HGroupProfile::Max,
            &crate::AnalysisControl::default()
        )
        .unwrap(),
        Some(Action::Play(CardId::new(11)))
    );
    let source = ProspectiveTransition::successful_play(
        &source,
        PlayerId::new(2),
        CardId::new(11),
        Card::new(Suit::Green, Rank::Two),
    );
    let (donald, _) = PerspectiveProjector::new(&source, HGroupProfile::Max)
        .project(PlayerId::new(3), PerspectiveDepth::NestedRecipients)
        .unwrap();
    let inferred = infer_h_group(&donald, HGroupProfile::Max);
    assert_eq!(
        select_h_group_action(&donald, HGroupProfile::Max),
        Some(Action::Play(CardId::new(14))),
        "{inferred:#?}"
    );
    let decision = crate::planner::choose_projected_follow_up(
        &donald,
        HGroupProfile::Max,
        &crate::AnalysisControl::default(),
    )
    .unwrap();
    // Alice's partial-world forecast need not choose Donald's actual replay
    // move: he may instead Save Cathy's r2. The regression here is avoiding
    // the critical b5 discard that used to win by hiding behind a zero-length
    // forecast. Actual turn-20 move parity is tested by the complete replay.
    assert!(decision.is_some());
    assert_ne!(decision, Some(Action::Discard(CardId::new(21))));
}

#[test]
fn fourth_replay_leaves_the_shared_green_clue_to_unoccupied_alice() {
    // Reviewed p4v0s3 turn 16: Donald can play/transfer p2; Alice has no
    // play and can give the same green clue to Bob without losing the line.
    let state = expert_replay_p4v0s3().state_at_turn(15).unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(3)).unwrap()).unwrap();
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Discard(CardId::new(12))),
    );
}

#[test]
fn fourth_replay_chop_moved_duplicate_does_not_discard_the_purple_connector() {
    // Human-reviewed alternative at p4v0s3 turn 11: 4s to Bob makes Donald's
    // p3 exact. Alice's visible p3 is only Chop Moved, not arranged to play.
    let mut state = expert_replay_p4v0s3().state_at_turn(10).unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Four),
        })
        .unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(3)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    assert!(inferred.chop_moved.contains(&CardId::new(0)));
    assert!(!inferred.clued_or_promised().contains(&CardId::new(0)));
    assert!(inferred.clued_or_promised().contains(&CardId::new(14)));
    assert!(!is_convention_trash(
        deductions.view(),
        Card::new(Suit::Purple, Rank::Three),
        &inferred.clued_or_promised(),
        &inferred.cards,
    ));
    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Discard(CardId::new(14)))
    );
}

#[test]
fn fourth_replay_turn_seventeen_admits_the_reviewed_green_connection() {
    // Human-reviewed p4v0s3 turn 17: Cathy's promised g2 plays after Bob's
    // next turn, then her g3 connects Bob's g4. Do not require another g2
    // Prompt merely because Bob must wait for this Reverse Finesse.
    // https://hanabi.github.io/level-2/#the-reverse-finesse
    let state = expert_replay_p4v0s3().state_at_turn(16).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let d = LogicalDeductions::new(view.clone()).unwrap();
    let target = PlayerId::new(1);
    let clue = Clue::Suit(Suit::Green);
    let interpretation = prospective_clue_primary_interpretation(
        &view,
        HGroupProfile::Max,
        target,
        clue,
        &[CardId::new(7)],
    );
    let interpretation = interpretation.expect("recipient recognizes the delayed play");
    assert_eq!(
        interpretation.focus_identities,
        IdentitySet::singleton(Card::new(Suit::Green, Rank::Four))
    );
    assert!(interpretation.hypotheses.iter().any(|hypothesis| {
        hypothesis.connection_steps.iter().any(|step| {
            step.actor == PlayerId::new(2)
                && step.cards == [CardId::new(23)]
                && step.expected == Card::new(Suit::Green, Rank::Three)
        })
    }));
    assert!(
        h_group_clue_candidates(&d, HGroupProfile::Max)
            .iter()
            .any(|candidate| candidate.action == Action::Clue { target, clue })
    );
}

#[test]
fn second_replay_projected_ejection_cannot_force_a_green_five_misplay() {
    // p4v0s9 turn 15 after Bob's reviewed blue clue. Bob's projection of
    // Cathy must not authorize green as a Stacked Ejection when Donald
    // instead reads his green 5 as an immediately playable green 3.
    let mut state = expert_replay_p4v0s9().state_at_turn(13).unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(3),
            clue: Clue::Suit(Suit::Blue),
        })
        .unwrap();
    let public = state.view_for(PlayerId::new(1)).unwrap();
    let projected = PerspectiveProjector::new(&public, HGroupProfile::Max)
        .project_with_evidence(PlayerId::new(2), PerspectiveDepth::NestedRecipients)
        .unwrap();
    let a = analyze_h_group_convention(&projected.deductions, HGroupProfile::Max);
    assert!(!a.actions.iter().any(|candidate| candidate.action
        == Action::Clue {
            target: PlayerId::new(3),
            clue: Clue::Suit(Suit::Green)
        }));
}

#[test]
fn second_replay_finesse_establishes_one_more_play_than_rank_two() {
    // Human-reviewed p4v0s9 turn 10: p2 -> p3 -> p4 plus the b1 follow-up
    // yields one more play than p2 + b2 plus that same b1. Protection of an
    // unthreatened b2 must not erase the extra established play.
    let state = expert_replay_p4v0s9().state_at_turn(9).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let analysis = crate::analyze_position(
        &view,
        crate::SupportedConvention::HGroup(HGroupProfile::Max),
        crate::PlannerConfig::default(),
    )
    .unwrap();
    let four = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Rank(Rank::Four),
    };
    let two = Action::Clue {
        target: PlayerId::new(3),
        clue: Clue::Rank(Rank::Two),
    };
    let endpoint = |action| {
        analysis
            .planner
            .root_actions
            .iter()
            .find(|candidate| candidate.action == action)
            .unwrap()
            .symbolic_line
            .position_value
            .unwrap()
    };
    let a = endpoint(four);
    let b = endpoint(two);
    assert_eq!(a.score, b.score);
    assert_eq!(a.clues, b.clues);
    assert_eq!(
        a.committed_future_plays,
        b.committed_future_plays + 1,
        "{a:?} vs {b:?}"
    );
    assert_eq!(analysis.planner.best_action, four);
}

#[test]
fn first_replay_final_clue_does_not_override_an_available_play() {
    // Human-reviewed p4v0s415 turn 45: Alice can advance p4 -> p5 herself.
    // The final-clue-over-idle-discard preference must not override a play.
    let fixture = HanabiLiveReplay::from_json(include_str!(
        "../../../../hanabi-protocol/tests/fixtures/game-p4v0s415.json"
    ))
    .unwrap();
    let state = fixture.state_at_turn(44).unwrap();
    let analysis = crate::analyze_position(
        &state.view_for(PlayerId::new(0)).unwrap(),
        crate::SupportedConvention::HGroup(HGroupProfile::Max),
        crate::PlannerConfig::default(),
    )
    .unwrap();
    let green = analysis
        .planner
        .root_actions
        .iter()
        .find(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(2),
                    clue: Clue::Suit(Suit::Green),
                }
        })
        .unwrap();
    assert!(!green.preference.advances_terminal_plan());
    // Clarity: the extra GD token is surplus; p4 directly then the existing
    // p5 promise completes just as soon without transferring a blind card.
    // https://hanabi.github.io/level-6/#clarity-principle-part-1
    assert_eq!(analysis.planner.best_action, Action::Play(CardId::new(34)));
    let transfer = analysis
        .planner
        .root_actions
        .iter()
        .find(|candidate| candidate.action == Action::Discard(CardId::new(34)))
        .unwrap();
    let play = analysis
        .planner
        .root_actions
        .iter()
        .find(|candidate| candidate.action == Action::Play(CardId::new(34)))
        .unwrap();
    assert!(
        play.preference > transfer.preference,
        "retain the legal transfer, but prefer clarity"
    );
    // Resource-accounting counterfactual, not an assertion about optimal play:
    // without tokens the same completion needs funding, so do not claim that
    // the transfer token is surplus just because all needed cards are held.
    let mut unfunded = state.view_for(PlayerId::new(0)).unwrap();
    unfunded.clue_tokens = 0;
    let d = LogicalDeductions::new(unfunded).unwrap();
    let actions = analyze_h_group_convention(&d, HGroupProfile::Max);
    let play = actions
        .actions
        .iter()
        .find(|candidate| candidate.action == Action::Play(CardId::new(34)))
        .unwrap();
    let transfer = actions
        .actions
        .iter()
        .find(|candidate| candidate.action == Action::Discard(CardId::new(34)))
        .unwrap();
    assert!(transfer.preference > play.preference);
}

#[test]
fn first_replay_playable_five_save_does_not_eject() {
    // Human-reviewed p4v0s415 turn 34: both g5 and b5 play immediately;
    // choosing rank over color therefore does not forgo a play (RCE premise).
    // https://hanabi.github.io/extras/ejections/#the-rank-choice-ejection-with-a-number-2-or-a-number-5-rce
    let fixture = HanabiLiveReplay::from_json(include_str!(
        "../../../../hanabi-protocol/tests/fixtures/game-p4v0s415.json"
    ))
    .unwrap();
    let before = fixture.state_at_turn(33).unwrap();
    let d = LogicalDeductions::new(before.view_for(PlayerId::new(1)).unwrap()).unwrap();
    assert!(h_group_clue_candidates(&d, HGroupProfile::Max).iter().any(
        |candidate| candidate.action
            == Action::Clue {
                target: PlayerId::new(0),
                clue: Clue::Rank(Rank::Five)
            }
    ));
    let state = fixture.state_at_turn(34).unwrap();
    for observer in 0..4 {
        let d = LogicalDeductions::new(state.view_for(PlayerId::new(observer)).unwrap()).unwrap();
        let inferred = infer_h_group(&d, HGroupProfile::Max);
        assert!(
            !inferred
                .signals
                .iter()
                .any(|s| s.turn == 33 && s.kind == HGroupMoveKind::RankChoiceEjection),
            "observer {observer}"
        );
    }
}

#[test]
fn fifth_replay_burn_preserves_the_final_playing_clock() {
    // p4v0s1 turn 48: y5, g4 and g5 are known; all needed cards are held.
    // https://hanabi.github.io/level-8/#burning-end-game-stalling
    let state = expert_replay_p4v0s1().state_at_turn(47).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let analysis = crate::analyze_position(
        &view,
        crate::SupportedConvention::HGroup(HGroupProfile::Max),
        crate::PlannerConfig::default(),
    )
    .unwrap();
    assert!(
        matches!(analysis.planner.best_action, Action::Clue { .. }),
        "{:#?}",
        analysis.planner
    );
    let selected = analysis
        .planner
        .root_actions
        .iter()
        .find(|candidate| candidate.action == analysis.planner.best_action)
        .unwrap();
    assert!(
        selected.immediately_playable_touched > 0,
        "Level 8 prefers re-cluing an already playable card over a delayed promise"
    );
}

#[test]
fn fifth_replay_fully_clued_endgame_clues_are_all_burns() {
    // Human-reviewed p4v0s1 turn 48: even a new collateral touch cannot
    // communicate another needed play after every remaining play is clued.
    let state = expert_replay_p4v0s1().state_at_turn(47).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let clues = view
        .legal_actions()
        .into_iter()
        .filter(|a| matches!(a, Action::Clue { .. }))
        .count();
    let d = LogicalDeductions::new(view).unwrap();
    let candidates = h_group_clue_candidates(&d, HGroupProfile::Max);
    assert_eq!(candidates.len(), clues);
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.move_kind() == Some(HGroupMoveKind::Burn))
    );
    for candidate in candidates {
        let mut after = state.clone();
        after.apply(candidate.action).unwrap();
        for observer in 0..4 {
            let d =
                LogicalDeductions::new(after.view_for(PlayerId::new(observer)).unwrap()).unwrap();
            let inferred = infer_h_group(&d, HGroupProfile::Max);
            assert!(
                inferred
                    .signals
                    .iter()
                    .any(|signal| signal.turn == 47 && signal.kind == HGroupMoveKind::Burn),
                "observer {observer}, {:?}: cards {:?}; current signals {:?}",
                candidate.action,
                inferred.cards,
                inferred
                    .signals
                    .iter()
                    .filter(|signal| signal.turn == 47)
                    .collect::<Vec<_>>()
            );
            assert!(
                !inferred
                    .signals
                    .iter()
                    .any(|signal| signal.turn == 47 && signal.kind != HGroupMoveKind::Burn),
                "{:?}: {:?}",
                candidate.action,
                inferred.signals
            );
        }
    }
}

#[test]
fn fifth_replay_clues_the_last_missing_connector_before_surplus_discard() {
    // Reviewed p4v0s1 turn 46: g3/g4 are visible; Bob knows his own g5.
    // Seven tokens already fund the remaining clues. No draw is required.
    let state = expert_replay_p4v0s1().state_at_turn(45).unwrap();
    let source = state.view_for(state.current_player()).unwrap();
    let analysis = crate::analyze_position(
        &source,
        crate::SupportedConvention::HGroup(HGroupProfile::Max),
        crate::PlannerConfig::default(),
    )
    .unwrap();
    assert_eq!(
        analysis.planner.best_action,
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Three)
        }
    );
}

#[test]
fn fifth_replay_bluffs_through_the_givers_known_green_four() {
    // p4v0s1 turn 35: green to Bob goes through Cathy's already-known g4.
    // https://hanabi.github.io/level-11/#bluffs-through-already-clued-cards
    let state = expert_replay_p4v0s1().state_at_turn(34).unwrap();
    let d = LogicalDeductions::new(state.view_for(state.current_player()).unwrap()).unwrap();
    let green = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Suit(Suit::Green),
    };
    let candidates = h_group_clue_candidates(&d, HGroupProfile::Max);
    let own = infer_h_group(&d, HGroupProfile::Max);
    assert!(super::super::interpretation::bluff_focus_is_one_away(
        d.view(),
        Card::new(Suit::Green, Rank::Five),
        &own.gotten(),
        &own.cards
    ));
    assert!(
        candidates.iter().any(|c| c.action == green),
        "cards={:?} gotten={:?} candidates={candidates:#?}",
        infer_h_group(&d, HGroupProfile::Max).cards,
        infer_h_group(&d, HGroupProfile::Max).gotten()
    );
    let (line, evidence) = super::super::symbolic_line::project_h_group_projection(
        d.view(),
        HGroupProfile::Max,
        green,
        4,
        &crate::AnalysisControl::default(),
    )
    .unwrap();
    let after = expert_replay_p4v0s1().state_at_turn(36).unwrap();
    let bob = LogicalDeductions::new(after.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let bob = infer_h_group(&bob, HGroupProfile::Max);
    assert_eq!(
        bob.cards
            .iter()
            .find(|card| card.card == CardId::new(19))
            .unwrap()
            .identities,
        IdentitySet::singleton(Card::new(Suit::Green, Rank::Five))
    );
    assert!(!bob.playable_now.contains(&CardId::new(19)));
    assert_eq!(
        crate::analyze_position(
            d.view(),
            crate::SupportedConvention::HGroup(HGroupProfile::Max),
            crate::PlannerConfig::default()
        )
        .unwrap()
        .planner
        .best_action,
        green,
        "the same r5 play additionally protects Bob's critical g5"
    );
    assert_eq!(line.strikes, 0, "{evidence:#?}");
    assert_eq!(
        evidence.steps.get(1).map(|step| step.projected.action),
        Some(Action::Play(CardId::new(33))),
        "{evidence:#?}"
    );
}

/// Human-reviewed p4v0s1 turn 22: Cathy's available clued r2 does not
/// prevent a rank-4 Bluff on her newest g2, saving Alice's y4.
/// <https://hanabi.github.io/level-11/#the-bluff>
#[test]
fn fifth_replay_bluff_interrupts_an_ordinary_clued_play() {
    let fixture = expert_replay_p4v0s1();
    let state = fixture.state_at_turn(21).unwrap();
    let d = LogicalDeductions::new(state.view_for(state.current_player()).unwrap()).unwrap();
    let action = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Rank(Rank::Four),
    };
    let candidates = h_group_clue_candidates(&d, HGroupProfile::Max);
    let replay = replay_h_group(&d, HGroupProfile::Max);
    // Donald's already-promised p3 is a truthful connector, not a reason
    // to Bluff a second copy out of Cathy.
    assert!(super::super::bluff::bluff_connector_is_promised(
        d.view(),
        &replay.hands,
        &replay.cards.already_playing,
        &replay.pending_connections,
        Card::new(Suit::Purple, Rank::Three),
        None,
    ));
    let candidate = candidates
        .iter()
        .find(|c| c.action == action)
        .expect("rank 4 is admitted even though Cathy has a clued play");
    assert_eq!(candidate.move_kind(), Some(HGroupMoveKind::Bluff));
    assert!(candidate.expiring_multi_card_opportunity());
    let inferred = infer_h_group(&d, HGroupProfile::Max);
    assert!(super::super::decision::can_park_surplus_five(
        d.view(),
        &inferred
    ));
    let mut unfunded = d.view().clone();
    unfunded.clue_tokens = 0;
    assert!(!super::super::decision::can_park_surplus_five(
        &unfunded, &inferred
    ));
    let convention = analyze_h_group_convention(&d, HGroupProfile::Max);
    assert!(
        convention.forced.is_none(),
        "an ordinary b5 play is not mandatory"
    );
    let analysis = crate::analyze_position(
        d.view(),
        crate::SupportedConvention::HGroup(HGroupProfile::Max),
        crate::PlannerConfig::default(),
    )
    .unwrap();
    assert_eq!(
        analysis.planner.best_action, action,
        "the reviewed 2-for-1 expires after Cathy's draw; b5 and its unneeded refund can wait"
    );
    for expected in [action, Action::Play(CardId::new(17))] {
        let root = analysis
            .planner
            .root_actions
            .iter()
            .find(|root| root.action == expected)
            .expect("both the Bluff and the ordinary play must reach root search");
        assert!(!root.projection.steps.is_empty());
    }
    let (line, evidence) = super::super::symbolic_line::project_h_group_projection(
        d.view(),
        HGroupProfile::Max,
        action,
        32,
        &crate::AnalysisControl::default(),
    )
    .unwrap();
    assert_eq!(line.strikes, 0);
    assert_eq!(
        evidence
            .steps
            .iter()
            .take(2)
            .map(|step| step.projected.action)
            .collect::<Vec<_>>(),
        vec![action, Action::Play(CardId::new(25))]
    );
    let after = fixture.state_at_turn(22).unwrap();
    let cathy = LogicalDeductions::new(after.view_for(PlayerId::new(2)).unwrap()).unwrap();
    assert_eq!(
        select_h_group_action(&cathy, HGroupProfile::Max),
        Some(Action::Play(CardId::new(25)))
    );
    let resolved = fixture.state_at_turn(23).unwrap();
    let alice = LogicalDeductions::new(resolved.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&alice, HGroupProfile::Max);
    assert_eq!(
        inferred
            .cards
            .iter()
            .find(|note| note.card == CardId::new(2))
            .unwrap()
            .identities,
        IdentitySet::singleton(Card::new(Suit::Yellow, Rank::Four))
    );
}

#[test]
fn fifth_replay_demonstrated_bluff_eliminates_older_good_touch_duplicates() {
    // p4v0s1 turn 24, after the reviewed g2 Bluff: Cathy's saved 2
    // cannot duplicate it. This is Good Touch, not physical card counting.
    let resolved = expert_replay_p4v0s1().state_at_turn(23).unwrap();
    let cathy = LogicalDeductions::new(resolved.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&cathy, HGroupProfile::Max);
    assert_eq!(
        inferred
            .cards
            .iter()
            .find(|note| note.card == CardId::new(18))
            .unwrap()
            .identities,
        IdentitySet::singleton(Card::new(Suit::Red, Rank::Two))
    );
    assert!(inferred.playable_now.contains(&CardId::new(18)));
}

#[test]
fn reviewed_pending_finesse_still_blocks_a_queued_bluff() {
    // p4v0s415 turn 3: Donald owes the blind play for Alice's yellow card.
    let fixture = reviewed_rank_three_branch_p4v0s415();
    let state = fixture.state_at_turn(2).unwrap();
    let d = LogicalDeductions::new(state.view_for(PlayerId::new(3)).unwrap()).unwrap();
    let replay = replay_h_group(&d, HGroupProfile::Max);
    assert!(super::super::bluff::bluff_is_queued(
        &replay.pending_connections,
        PlayerId::new(3),
        None,
    ));
}

/// Human-reviewed p4v0s1 turn 14: purple to Alice initiates the two
/// immediate blind plays (Cathy's g1, Donald's r1), not a 5 Color Ejection.
/// <https://hanabi.github.io/level-15/#the-double-bluff>
#[test]
fn fifth_replay_purple_double_bluff_is_admitted() {
    let fixture = expert_replay_p4v0s1();
    let state = fixture.state_at_turn(13).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let deductions = LogicalDeductions::new(view.clone()).unwrap();
    let action = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Suit(Suit::Purple),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    for turn in 14..=16 {
        let after = fixture.state_at_turn(turn).unwrap();
        for player in 0..4 {
            let d = LogicalDeductions::new(after.view_for(PlayerId::new(player)).unwrap()).unwrap();
            let inferred = infer_h_group(&d, HGroupProfile::Max);
            if turn == 14 && player == 2 {
                assert!(
                    inferred.playable_now.contains(&CardId::new(20)),
                    "{inferred:#?}"
                );
                assert!(
                    !inferred
                        .signals
                        .iter()
                        .any(|s| s.turn == 13 && s.kind == HGroupMoveKind::FiveColorEjection)
                );
            }
            if turn == 15 && player == 3 {
                assert!(
                    inferred.playable_now.contains(&CardId::new(21)),
                    "{inferred:#?}"
                );
            }
            if turn == 16 && player == 0 {
                assert_eq!(
                    inferred
                        .cards
                        .iter()
                        .find(|note| note.card == CardId::new(22))
                        .unwrap()
                        .identities,
                    IdentitySet::singleton(Card::new(Suit::Purple, Rank::Five)),
                    "{inferred:#?}"
                );
            }
        }
    }
    let candidate = candidates
        .iter()
        .find(|candidate| candidate.action == action)
        .expect("the Double Bluff is admitted");
    let outcome =
        super::super::strategic_value::scheduled_clue_outcome(&view, HGroupProfile::Max, candidate)
            .unwrap();
    assert_eq!(outcome.convention_action_count, Some(3));
    assert_eq!(outcome.convention_connection_steps, Some(2));
    // Human-reviewed comparison: r2 is not facing a discard deadline;
    // Cathy already has a play and Alice/Bob can arrange red afterwards.
    // The automatic release of Donald's clued r3 is not a third card
    // obtained by the red clue.
    let red = candidates
        .iter()
        .find(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(2),
                    clue: Clue::Suit(Suit::Red),
                }
        })
        .unwrap();
    let red_outcome =
        super::super::strategic_value::scheduled_clue_outcome(&view, HGroupProfile::Max, red)
            .unwrap();
    assert_eq!(red_outcome.clue_efficiency, 2);
    assert_eq!(outcome.clue_efficiency, 3);
    assert!(candidate.value.total() > red.value.total());
    let (line, evidence) = super::super::symbolic_line::project_h_group_projection(
        &view,
        HGroupProfile::Max,
        action,
        32,
        &crate::AnalysisControl::default(),
    )
    .unwrap();
    assert!(line.actions >= 3);
    assert!(line.score_gain >= 2);
    assert_eq!(line.strikes, 0);
    assert_eq!(
        evidence
            .steps
            .iter()
            .take(3)
            .map(|step| step.projected.action)
            .collect::<Vec<_>>(),
        vec![
            action,
            Action::Play(CardId::new(20)),
            Action::Play(CardId::new(21))
        ]
    );
}

/// Same reviewed demonstration, retaining the information hidden from each
/// source observer rather than exporting Alice's perfect-information note.
#[test]
fn fifth_replay_double_bluff_keeps_observer_relative_focus_domains() {
    let state = expert_replay_p4v0s1().state_at_turn(16).unwrap();
    for (player, ranks) in [
        (0, vec![Rank::Five]),
        (1, vec![Rank::Four, Rank::Five]),
        (2, vec![Rank::Five]),
        (3, vec![Rank::Three, Rank::Four, Rank::Five]),
    ] {
        let source = state.view_for(PlayerId::new(player)).unwrap();
        let (d, r) = PerspectiveProjector::new(&source, HGroupProfile::Max)
            .project(PlayerId::new(0), PerspectiveDepth::NestedRecipients)
            .unwrap();
        let inferred = infer_h_group_from_replay(&d, r.clone(), HGroupProfile::Max);
        let expected = ranks.into_iter().fold(IdentitySet::default(), |set, rank| {
            set.union(IdentitySet::singleton(Card::new(Suit::Purple, rank)))
        });
        assert_eq!(
            inferred
                .cards
                .iter()
                .find(|note| note.card == CardId::new(22))
                .unwrap()
                .identities,
            expected,
            "source={player}; {inferred:#?}"
        );
        assert!(
            !r.pending_connections
                .iter()
                .any(|connection| connection.focus == CardId::new(22)),
            "no third blind play is owed"
        );
    }
}

#[test]
fn demonstrated_yellow_layer_is_shared_across_observer_projections() {
    let fixture = HanabiLiveReplay::from_json(include_str!(
        "../../../../hanabi-protocol/tests/fixtures/game-p4v0s415.json"
    ))
    .unwrap();
    for turn in [4, 8, 11, 12] {
        let state = fixture.state_at_turn(turn).unwrap();
        for observer in 0..4 {
            let view = state.view_for(PlayerId::new(observer)).unwrap();
            let (d, r) = PerspectiveProjector::new(&view, HGroupProfile::Max)
                .project(PlayerId::new(0), PerspectiveDepth::NestedRecipients)
                .unwrap();
            let inferred = infer_h_group_from_replay(&d, r.clone(), HGroupProfile::Max);
            let note = inferred
                .cards
                .iter()
                .find(|n| n.card == CardId::new(2))
                .unwrap();
            assert_eq!(
                note.identities,
                IdentitySet::singleton(Card::new(Suit::Yellow, Rank::Two)),
                "turn={turn} source={observer} clues={:?} signals={:?} pending={:?}",
                r.clues,
                r.signals,
                r.pending_connections
            );
            assert!(!inferred.playable_now.contains(&CardId::new(2)));
            assert!(
                r.pending_connections.iter().any(|connection| {
                    connection.actor == PlayerId::new(3)
                        && connection.focus == CardId::new(2)
                        && connection.expected == Card::new(Suit::Yellow, Rank::One)
                }),
                "the public y1 obligation must remain assigned to Donald"
            );
        }
    }
    let state = fixture.state_at_turn(11).unwrap();
    let view = state.view_for(PlayerId::new(3)).unwrap();
    let outcome = super::super::symbolic_line::project_h_group_line(
        &view,
        HGroupProfile::Max,
        Action::Play(CardId::new(18)),
        32,
    );
    assert_eq!(
        outcome.strikes, 0,
        "b3 continuation must not invent a y1 play from Alice's y2"
    );
}

/// Reviewed fixture opening, alternative clue: a blank in Alice's hand is
/// not evidence that Bob lacks a visible external b3 connector. This tests
/// projection uncertainty, not the optimality of an invented continuation.
#[test]
fn fifth_opening_charm_projection_excludes_the_clue_givers_hand() {
    let state = expert_replay_p4v0s1().state_at_turn(0).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let d = LogicalDeductions::new(view.clone()).unwrap();
    let action = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Rank(Rank::Four),
    };
    let candidates = h_group_clue_candidates(&d, HGroupProfile::Max);
    assert_eq!(
        candidates
            .iter()
            .find(|c| c.action == action)
            .unwrap()
            .move_kind(),
        Some(HGroupMoveKind::Charm)
    );
    let outcome =
        super::super::symbolic_line::project_h_group_line(&view, HGroupProfile::Max, action, 32);
    // The giver cannot intend a connection through their own hidden cards.
    // The Charm's b1 is therefore a supported play, not an unknown branch.
    assert!(outcome.actions >= 2);
    assert!(outcome.score_gain >= 1);
    let mut after = state.clone();
    after.apply(action).unwrap();
    let bob = LogicalDeductions::new(after.view_for(PlayerId::new(1)).unwrap()).unwrap();
    assert!(
        infer_h_group(&bob, HGroupProfile::Max)
            .signals
            .iter()
            .any(|s| s.kind == HGroupMoveKind::Charm)
    );
}

#[test]
fn reviewed_yellow_play_is_not_a_redundant_invisible_alternative() {
    let state = expert_replay_p4v0s2().state_at_turn(7).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    // User-reviewed p4v0s2 turn 8: y1 is new information even though Bob
    // already has g2 to play; loading the hand is strategy, not illegality.
    let d = LogicalDeductions::new(view).unwrap();
    let candidates = h_group_clue_candidates(&d, HGroupProfile::Max);
    let yellow = candidates
        .iter()
        .find(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(1),
                    clue: Clue::Suit(Suit::Yellow),
                }
        })
        .unwrap();
    assert!(yellow.immediate_play());
    assert_eq!(yellow.move_kind(), Some(HGroupMoveKind::PlayClue));
    let blue = candidates
        .iter()
        .find(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(0),
                    clue: Clue::Suit(Suit::Blue),
                }
        })
        .unwrap();
    assert!(
        yellow.score() < blue.score(),
        "give idle Alice an action rather than loading Bob"
    );
}

/// User-reviewed p4v0s2 turn 8: Cathy supplies b2 behind a playable r1
/// layer. Alice needs only b1 + b3, so 4s to Bob cannot be a 4 Charm.
/// <https://hanabi.github.io/level-23/#the-4-charm>
#[test]
fn third_replay_four_charm_counts_blind_plays_in_the_reactors_hand() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(7).unwrap();
    for observer in [PlayerId::new(0), PlayerId::new(3)] {
        let view = state.view_for(observer).unwrap();
        let explicit = view
            .hands
            .iter()
            .flatten()
            .filter(|card| was_clued_before(&view, view.turn, card.id))
            .map(|card| card.id)
            .collect();
        assert_eq!(
            super::super::recognition::unassigned_finesse_ranks(
                &view,
                PlayerId::new(3),
                PlayerId::new(0),
                Card::new(Suit::Blue, Rank::Four),
                std::array::from_fn(|suit| u8::try_from(view.play_stacks[suit].len()).unwrap()),
                &explicit,
                view.turn,
            ),
            2
        );
    }
    let deductions =
        LogicalDeductions::new(state.view_for(state.current_player()).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    for clue in [Clue::Rank(Rank::Four), Clue::Suit(Suit::Blue)] {
        let action = Action::Clue {
            target: PlayerId::new(1),
            clue,
        };
        assert!(
            !candidates
                .iter()
                .any(|candidate| candidate.action == action)
        );
        let mut after = state.clone();
        after.apply(action).unwrap();
        let d = LogicalDeductions::new(after.view_for(PlayerId::new(0)).unwrap()).unwrap();
        let replay = replay_h_group(&d, HGroupProfile::Max);
        assert!(
            !replay
                .signals
                .iter()
                .any(|signal| signal.turn == 7 && signal.kind == HGroupMoveKind::Charm)
        );
    }
}

/// Slot-selection invariants using the reviewed p4v0s415 turn-35 notes.
/// <https://hanabi.github.io/level-18/#the-elimination-finesse>
#[test]
fn elimination_finesse_slot_selection_respects_notes_and_chop_moves() {
    let fixture = reviewed_rank_three_branch_p4v0s415();
    let state = fixture.state_at_turn(34).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let deductions = LogicalDeductions::new(view).unwrap();
    let replay = replay_h_group(&deductions, HGroupProfile::Max);
    let actor = PlayerId::new(1);
    let select = |moved: &CardSet, excluded: Option<CardId>| {
        elimination_finesse_card(
            actor,
            &replay.hands[1],
            CardId::new(33),
            Card::new(Suit::Purple, Rank::Two),
            &replay.cards.facts,
            moved,
            |card| Some(card) != excluded,
        )
    };
    assert_eq!(select(&CardSet::default(), None), Some(CardId::new(6)));
    assert_eq!(
        select(&[CardId::new(6)].into_iter().collect(), None),
        Some(CardId::new(25))
    );
    assert_eq!(
        select(
            &[CardId::new(6), CardId::new(25), CardId::new(28)]
                .into_iter()
                .collect(),
            None
        ),
        Some(CardId::new(6))
    );
    assert_eq!(
        select(&CardSet::default(), Some(CardId::new(6))),
        Some(CardId::new(25))
    );
}

/// Same reviewed clue: admission, connection proof, and owner agree without
/// granting any special admission score or overriding clue safety.
#[test]
fn elimination_finesse_is_admitted_and_understood_by_its_owner() {
    let fixture = reviewed_rank_three_branch_p4v0s415();
    let state = fixture.state_at_turn(34).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let deductions = LogicalDeductions::new(view.clone()).unwrap();
    let target = PlayerId::new(3);
    let clue = Clue::Rank(Rank::Three);
    assert!(
        h_group_clue_candidates(&deductions, HGroupProfile::Max)
            .iter()
            .any(|candidate| candidate.action == Action::Clue { target, clue })
    );
    assert_eq!(
        prospective_clue_hazard(
            &view,
            HGroupProfile::Max,
            target,
            CardId::new(33),
            clue,
            &[CardId::new(33)],
            false
        ),
        None
    );
    let interpretation = prospective_clue_primary_interpretation(
        &view,
        HGroupProfile::Max,
        target,
        clue,
        &[CardId::new(33)],
    )
    .unwrap();
    assert!(interpretation.hypotheses.iter().any(|hypothesis| {
        hypothesis.connection_steps.iter().any(|step| {
            step.actor == PlayerId::new(1)
                && step.cards == [CardId::new(6)]
                && step.expected == Card::new(Suit::Purple, Rank::Two)
        })
    }));
    let after = fixture.state_at_turn(35).unwrap();
    let owner = LogicalDeductions::new(after.view_for(PlayerId::new(1)).unwrap()).unwrap();
    assert_eq!(
        infer_h_group(&owner, HGroupProfile::Max)
            .connection
            .map(|connection| connection.card),
        Some(CardId::new(6))
    );
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Review {
    seed: String,
    reviewed_through: Option<usize>,
    continuation: String,
    profile: String,
    objective: String,
    checks: Vec<String>,
}

fn expert_reviews() -> Vec<Review> {
    serde_json::from_str(include_str!(
        "../../../../hanabi-protocol/tests/fixtures/expert-manifest.json"
    ))
    .unwrap()
}

#[test]
fn every_expert_fixture_has_one_explicit_review_contract() {
    let manifest = expert_reviews();
    let mut registered = manifest
        .iter()
        .map(|review| format!("game-{}.json", review.seed))
        .collect::<Vec<_>>();
    registered.sort();
    assert!(
        registered.windows(2).all(|pair| pair[0] != pair[1]),
        "duplicate review contract"
    );
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../hanabi-protocol/tests/fixtures");
    let mut fixtures = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| {
            name.starts_with("game-")
                && std::path::Path::new(name)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        })
        .collect::<Vec<_>>();
    fixtures.sort();
    assert_eq!(
        registered, fixtures,
        "new fixtures must declare their review boundary"
    );
}

fn assert_expert_replay_matches_engine(seed: &str, replay: &HanabiLiveReplay) {
    let manifest = expert_reviews();
    assert_eq!(
        manifest.iter().filter(|review| review.seed == seed).count(),
        1
    );
    let review = manifest.iter().find(|review| review.seed == seed).unwrap();
    let profile = review.profile.parse().unwrap();
    assert_eq!(review.checks, ["legality", "reviewed-action-parity"]);
    assert_eq!(
        review.continuation,
        if review.reviewed_through.is_some() {
            "engine-generated"
        } else {
            "human-reviewed"
        }
    );
    replay.replay().expect("entire fixture remains rules-legal");
    let through = review.reviewed_through.unwrap_or(replay.actions.len());
    assert!(through > 0 && through <= replay.actions.len());
    eprintln!(
        "{seed}: reviewed action parity {through}/{} moves; suffix provenance: {}",
        replay.actions.len(),
        review.continuation
    );
    for turn in 0..u32::try_from(through).expect("replay fits in u32") {
        let profiling = std::env::var_os("HANABI_PROFILE_REPLAY").is_some();
        let started = std::time::Instant::now();
        if profiling {
            crate::test_profile::start();
        }
        let state = replay.state_at_turn(turn).expect("fixture prefix is legal");
        let actor = state.current_player();
        let view = state.view_for(actor).expect("current player has a view");
        let analysis = crate::analyze_position(
            &view,
            crate::SupportedConvention::HGroup(profile),
            crate::PlannerConfig {
                objective: review.objective.parse().unwrap(),
                ..crate::PlannerConfig::default()
            },
        )
        .unwrap_or_else(|error| {
            panic!(
                "fixture position at move {} is analyzable: {error}",
                turn + 1
            )
        });
        let expected = replay_action_at_turn(replay, turn);
        if profiling {
            crate::test_profile::finish(seed, turn + 1, started.elapsed());
        }
        if analysis.planner.best_action == expected {
            continue;
        }
        let link = hanabi_protocol::replay_link(replay, usize::try_from(turn).unwrap() + 1)
            .unwrap_or_else(|error| format!("Replay link generation failed: {error}"));
        let review = format!(
            "{seed}, Hanab Live turn {} ({}):\nFixture: {expected:?}\nEngine: {:?}\nReplay: {link}",
            turn + 1,
            replay.players[actor.index()],
            analysis.planner.best_action,
        );
        // Print before the large diagnostics so the review link is easy to find.
        eprintln!("{review}");
        let deductions = LogicalDeductions::new(view).expect("fixture position is logical");
        let clue_candidates = h_group_clue_candidates(&deductions, profile);
        let replay = replay_h_group(&deductions, profile);
        let admitted = clue_candidates
            .iter()
            .map(|candidate| candidate.action)
            .collect::<Vec<_>>();
        let rejected = h_group_rejected_clues_from_replay(&deductions, profile, &replay, &admitted);
        let inferences = infer_h_group(&deductions, profile);
        assert_eq!(
            analysis.planner.best_action,
            expected,
            "{review}\nengine disagrees at move {}; planner comparisons: {:#?}; planner candidates: {:#?}; convention candidates: {clue_candidates:#?}; rejected clues: {rejected:#?}; inferences: {inferences:#?}",
            turn + 1,
            analysis.planner.comparisons,
            analysis.planner.root_actions,
        );
    }
}

/// The user approved moves through 36. The generated suffix awaits review;
/// it is validated for legality, not frozen as optimal strategy.
#[test]
fn first_expert_replay_reviewed_prefix_matches_engine() {
    let replay = HanabiLiveReplay::from_json(include_str!(
        "../../../../hanabi-protocol/tests/fixtures/game-p4v0s415.json"
    ))
    .expect("active replay is valid");
    replay.replay().expect("generated continuation is legal");
    assert_expert_replay_matches_engine("p4v0s415", &replay);
}

#[test]
fn first_replay_three_bluff_does_not_chop_move_recipient_cards() {
    // Human-reviewed p4v0s415 turn 30 is a 3 Bluff, not a Trash Chop Move.
    // #18's historical rank-3 promise cannot account for p3 after #18 played b3.
    // https://hanabi.github.io/level-13/#the-3-bluff
    let fixture = HanabiLiveReplay::from_json(include_str!(
        "../../../../hanabi-protocol/tests/fixtures/game-p4v0s415.json"
    ))
    .unwrap();
    for turn in [30, 31, 39] {
        let state = fixture.state_at_turn(turn).unwrap();
        for observer in 0..4 {
            let deductions =
                LogicalDeductions::new(state.view_for(PlayerId::new(observer)).unwrap()).unwrap();
            let inferred = infer_h_group(&deductions, HGroupProfile::Max);
            assert!(
                !inferred.signals.iter().any(|signal| {
                    signal.turn == 29
                        && matches!(
                            signal.kind,
                            HGroupMoveKind::ChopMove | HGroupMoveKind::TrashChopMove
                        )
                }),
                "turn {turn}, observer {observer}: {:#?}",
                inferred.signals
            );
            assert!(
                !inferred.signals.iter().any(|signal| {
                    signal.turn == 30 && signal.kind == HGroupMoveKind::TimeTravelChopMove
                }),
                "the blind play secures p3; it is not a zero-value Time Travel Chop Move: turn {turn}, observer {observer}"
            );
        }
    }
}

#[test]
fn first_replay_turn_thirty_one_bluff_keeps_secured_duplicate_playable() {
    // Reviewed p4v0s415 turn 31: Cathy blind-plays g4 although Alice's
    // other g4 is already secured. Both p2s are visible in Bob's hand.
    // https://hanabi.github.io/level-13/#the-3-bluff
    let fixture = HanabiLiveReplay::from_json(include_str!(
        "../../../../hanabi-protocol/tests/fixtures/game-p4v0s415.json"
    ))
    .unwrap();
    let state = fixture.state_at_turn(30).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let deductions = LogicalDeductions::new(view.clone()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    let newest = inferred
        .cards
        .iter()
        .find(|card| card.card == CardId::new(32))
        .unwrap();
    assert!(
        newest
            .identities
            .contains(Card::new(Suit::Green, Rank::Four)),
        "{newest:#?}"
    );
    assert_eq!(newest.promised_identity, None, "no unseen p2 exists");
    assert_eq!(newest.play_obligation, Some(HGroupPlayObligation::Forced));
    let analysis = crate::analyze_position(
        &view,
        crate::SupportedConvention::HGroup(HGroupProfile::Max),
        crate::PlannerConfig::default(),
    )
    .expect("the owner's hand must admit a consistent world");
    assert_eq!(analysis.planner.best_action, Action::Play(CardId::new(32)));
}

#[test]
fn first_replay_resolved_three_bluff_does_not_create_hesitation_play() {
    // Branch from reviewed p4v0s415 turn 30. A disconnected g4 establishes
    // a 3 Bluff; Donald can discard while his saved 3 remains unplayable.
    // This asserts interpretation, not that the alternative is optimal.
    // https://hanabi.github.io/level-13/#the-3-bluff
    let fixture = HanabiLiveReplay::from_json(include_str!(
        "../../../../hanabi-protocol/tests/fixtures/game-p4v0s415.json"
    ))
    .unwrap();
    let mut state = fixture.state_at_turn(29).unwrap();
    for action in [
        Action::Clue {
            target: PlayerId::new(3),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(32)),
        Action::Discard(CardId::new(21)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    assert!(
        inferred
            .signals
            .iter()
            .any(|signal| { signal.turn == 30 && signal.kind == HGroupMoveKind::Bluff })
    );
    assert!(
        !inferred.signals.iter().any(|signal| {
            signal.turn == 31 && signal.kind == HGroupMoveKind::HesitationBlindPlay
        }),
        "a resolved Bluff does not imply a missing p2: {inferred:#?}"
    );
}

#[test]
fn first_replay_move_thirty_bluff_must_not_touch_played_purple_one() {
    // User-reviewed p4v0s415 turn 30: purple would newly touch Donald's
    // p3 #33 AND p1 #26, although p1 #19 already played on turn 21.
    // https://hanabi.github.io/beginner/good-touch-principle/
    let fixture = HanabiLiveReplay::from_json(include_str!(
        "../../../../hanabi-protocol/tests/fixtures/game-p4v0s415.json"
    ))
    .unwrap();
    let state = fixture.state_at_turn(29).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    assert_eq!(view.play_stacks[Suit::Purple.index()].len(), 1);
    assert_eq!(
        identity_of(&view, CardId::new(26)),
        Some(Card::new(Suit::Purple, Rank::One))
    );
    let deductions = LogicalDeductions::new(view).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let clue = |clue| Action::Clue {
        target: PlayerId::new(3),
        clue,
    };
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.action != clue(Clue::Suit(Suit::Purple))),
        "a Bluff cannot waive Good Touch for its collateral p1: {candidates:#?}"
    );
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.action == clue(Clue::Rank(Rank::Three))),
        "the clean rank-3 alternative must remain available"
    );
}

#[test]
fn first_replay_move_eight_keeps_a_loaded_clue_in_superposition() {
    let fixture = reviewed_rank_three_branch_p4v0s415();
    let state = fixture.state_at_turn(7).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Donald has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    let red_focus = inferred
        .cards
        .iter()
        .find(|note| note.card == CardId::new(17))
        .expect("Donald tracks the newly red-clued card");

    assert!(
        red_focus.identities.len() > 1 && !inferred.playable_now.contains(&CardId::new(17)),
        "the newer red clue remains direct/delayed while Donald owes the older yellow-1 connection: {inferred:#?}",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(14))),
        "the older blind-play obligation remains mandatory: inference={inferred:#?}; candidates={:#?}",
        h_group_clue_candidates(&deductions, HGroupProfile::Max),
    );
}

#[test]
fn first_replay_move_ten_admits_the_direct_rank_three_play_clue() {
    let fixture = reviewed_rank_three_branch_p4v0s415();
    let state = fixture.state_at_turn(9).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Bob has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let replay = replay_h_group(&deductions, HGroupProfile::Max);
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let expected = Action::Clue {
        target: PlayerId::new(3),
        clue: Clue::Rank(Rank::Three),
    };
    let hazard = prospective_clue_hazard(
        deductions.view(),
        HGroupProfile::Max,
        PlayerId::new(3),
        CardId::new(18),
        Clue::Rank(Rank::Three),
        &[CardId::new(18)],
        true,
    );
    let mut after = state.clone();
    after
        .apply(expected)
        .expect("rank 3 is a legal game action");
    let recipient_view = after.view_for(PlayerId::new(3)).expect("Donald has a view");
    let recipient_deductions = LogicalDeductions::new(recipient_view).expect("logical view");
    let recipient_replay = replay_h_group(&recipient_deductions, HGroupProfile::Max);
    let recipient_inferred = infer_h_group(&recipient_deductions, HGroupProfile::Max);

    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.action == expected),
        "rank 3 to Donald remains a direct play clue; hazard={hazard:#?}; recipient clue={:#?}; recipient card={:#?}; pending={:#?}; candidates={candidates:#?}",
        recipient_replay.clues.last(),
        recipient_inferred
            .cards
            .iter()
            .find(|card| card.card == CardId::new(18)),
        replay.pending_connections,
    );
}

#[test]
fn first_replay_move_seven_is_a_fix_of_the_promised_red_one() {
    let fixture = reviewed_rank_three_branch_p4v0s415();
    let before = fixture.state_at_turn(6).expect("fixture prefix is legal");
    let before_view = before
        .view_for(before.current_player())
        .expect("Cathy has a view");
    let before_deductions = LogicalDeductions::new(before_view).expect("logical view");
    let before_replay = replay_h_group(&before_deductions, HGroupProfile::Max);
    let after = fixture.state_at_turn(7).expect("fixture clue is legal");
    for observer in [PlayerId::new(0), PlayerId::new(3)] {
        let after_view = after.view_for(observer).expect("observer has a view");
        let after_deductions = LogicalDeductions::new(after_view).expect("logical view");
        let after_replay = replay_h_group(&after_deductions, HGroupProfile::Max);

        assert!(
            after_replay
                .signals
                .at_turn(6, HGroupMoveKind::FixClue)
                .any(|signal| signal.cards.contains(&CardId::new(3))),
            "green to Alice must fix the red-1 promise on #3 for observer {observer:?}; before pending={:#?}; after pending={:#?}; after signals={:#?}",
            before_replay.pending_connections,
            after_replay.pending_connections,
            after_replay.signals,
        );
    }
}

#[test]
fn second_expert_replay_matches_engine() {
    assert_expert_replay_matches_engine("p4v0s9", &expert_replay_p4v0s9());
}

#[test]
fn second_replay_move_fourteen_admits_blue_to_donald() {
    let fixture = expert_replay_p4v0s9();
    let state = fixture.state_at_turn(13).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Bob has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let action = Action::Clue {
        target: PlayerId::new(3),
        clue: Clue::Suit(Suit::Blue),
    };
    let touched = deductions.view().hands[3]
        .iter()
        .filter(|card| {
            card.identity
                .is_some_and(|identity| Clue::Suit(Suit::Blue).matches(identity))
        })
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let focus = *touched.last().expect("blue clue touches Donald");
    let hazard = prospective_clue_hazard(
        deductions.view(),
        HGroupProfile::Max,
        PlayerId::new(3),
        focus,
        Clue::Suit(Suit::Blue),
        &touched,
        false,
    );
    let signals = prospective_team_clue_signal_kinds(
        deductions.view(),
        HGroupProfile::Max,
        PlayerId::new(3),
        Clue::Suit(Suit::Blue),
        &touched,
    );
    let recipient_signals = prospective_clue_signal_kinds(
        deductions.view(),
        HGroupProfile::Max,
        PlayerId::new(3),
        Clue::Suit(Suit::Blue),
        &touched,
    );
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.action == action),
        "blue to Donald remains convention-valid; touched={touched:?}; hazard={hazard:?}; signals={signals:?}; recipient={recipient_signals:?}; candidates={candidates:#?}",
    );
}

#[test]
fn second_replay_move_seventeen_excludes_the_promised_purple_two() {
    let fixture = expert_replay_p4v0s9();
    let state = fixture.state_at_turn(16).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Alice has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let inferences = infer_h_group(&deductions, HGroupProfile::Max);
    let card = inferences
        .cards
        .iter()
        .find(|card| card.card == CardId::new(0))
        .expect("Alice still holds her rank-2 card");

    assert!(
        !card.identities.contains(Card::new(Suit::Purple, Rank::Two)),
        "Donald already demonstrated the promised purple 2, so Good Touch permanently excludes purple 2 from Alice's previously clued card: {card:#?}",
    );
    assert!(
        inferences.playable_now.contains(&CardId::new(0)),
        "every remaining convention identity is playable, so Alice must play card #0: {inferences:#?}",
    );
}

#[test]
fn third_expert_replay_matches_engine() {
    assert_expert_replay_matches_engine("p4v0s2", &expert_replay_p4v0s2());
}

#[test]
fn third_replay_two_new_duplicate_fours_still_violate_good_touch() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(38).expect("fixture prefix is legal");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("Cathy has a view"),
    )
    .expect("valid deductions");
    let rank_four = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Rank(Rank::Four),
    };

    assert!(
        !h_group_clue_candidates(&deductions, HGroupProfile::Max)
            .iter()
            .any(|candidate| candidate.action == rank_four),
        "two newly touched copies would both look like future plays, so accounting for every physical copy does not excuse Good Touch",
    );
}

#[test]
fn fourth_expert_replay_matches_engine() {
    assert_expert_replay_matches_engine("p4v0s3", &expert_replay_p4v0s3());
}

#[test]
fn fifth_expert_replay_matches_engine() {
    assert_expert_replay_matches_engine("p4v0s1", &expert_replay_p4v0s1());
}

#[test]
fn fifth_replay_move_one_prefers_protecting_bottom_deck_risk() {
    let fixture = expert_replay_p4v0s1();
    let state = fixture.state_at_turn(0).expect("initial position is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Alice has a view");
    let deductions = LogicalDeductions::new(view).expect("initial position is logical");
    let rank_three = Action::Clue {
        target: PlayerId::new(3),
        clue: Clue::Rank(Rank::Three),
    };
    let purple = Action::Clue {
        target: PlayerId::new(3),
        clue: Clue::Suit(Suit::Purple),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let score = |action| {
        candidates
            .iter()
            .find(|candidate| candidate.action == action)
            .expect("candidate is convention-valid")
            .score()
    };

    assert!(
        score(rank_three) > score(purple),
        "rank 3 protects the one-visible-copy red 3; both purple 4s are visible and trivially saveable: {candidates:#?}",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(rank_three),
    );
}

#[test]
fn fifth_replay_move_four_recognizes_the_visible_blue_continuation() {
    let fixture = expert_replay_p4v0s1();
    let state = fixture.state_at_turn(3).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Donald has a view");
    let deductions = LogicalDeductions::new(view).expect("position is logical");
    let clue = Clue::Rank(Rank::Four);
    let action = Action::Clue {
        target: PlayerId::new(2),
        clue,
    };
    let touched = deductions.view().hands[2]
        .iter()
        .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let signals = prospective_team_clue_signal_kinds(
        deductions.view(),
        HGroupProfile::Max,
        PlayerId::new(2),
        clue,
        &touched,
    );
    let primary = prospective_clue_primary_interpretation(
        deductions.view(),
        HGroupProfile::Max,
        PlayerId::new(2),
        clue,
        &touched,
    );
    let hazard = prospective_clue_hazard(
        deductions.view(),
        HGroupProfile::Max,
        PlayerId::new(2),
        CardId::new(11),
        clue,
        &touched,
        false,
    );
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.action == action),
        "blue 1 and blue 2 are already scheduled in Bob's hand, so Alice's visible blue 3 connects Cathy's blue 4: signals={signals:?}; primary={primary:#?}; hazard={hazard:#?}; replay={:#?}; inferences={:#?}; candidates={candidates:#?}",
        replay_h_group(&deductions, HGroupProfile::Max),
        infer_h_group(&deductions, HGroupProfile::Max),
    );
}

#[test]
fn fifth_replay_move_five_keeps_the_existing_prompt_ahead_of_a_four_charm() {
    let fixture = expert_replay_p4v0s1();
    let state = fixture.state_at_turn(4).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Alice has a view");
    let deductions = LogicalDeductions::new(view).expect("position is logical");
    let replay = replay_h_group(&deductions, HGroupProfile::Max);
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !replay
            .signals
            .iter()
            .any(|signal| { signal.turn == 3 && signal.kind == HGroupMoveKind::Charm }),
        "the already-clued blue 2 is a Prompt in the layered blue-4 line, so the clue is not a 4 Charm: {replay:#?}",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Yellow),
        }),
        "Alice must continue with the fixture's yellow clue; candidates={candidates:#?}",
    );
}

#[test]
fn fifth_replay_move_eleven_rejects_an_unconnected_yellow_four() {
    let fixture = expert_replay_p4v0s1();
    let state = fixture.state_at_turn(10).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Cathy has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let target = PlayerId::new(0);
    let clue = Clue::Rank(Rank::Four);
    let action = Action::Clue { target, clue };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates
            .iter()
            .any(|candidate| candidate.action == action),
        "connections belonging to hypothetical red/green/purple 4s cannot justify Cathy's visibly yellow-4 focus: {candidates:#?}",
    );
}

#[test]
fn fifth_replay_move_eleven_permits_discarding() {
    let fixture = expert_replay_p4v0s1();
    let state = fixture.state_at_turn(10).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Cathy has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    // PTD is permission, not a requirement to outrank every newly implemented
    // clue in the greedy rollout policy. The full replay checks move selection.
    assert!(
        ordered_h_group_actions(&deductions, HGroupProfile::Max)
            .contains(&Action::Discard(CardId::new(9))),
        "Cathy may discard instead of giving the otherwise-mandatory 5 Stall to Bob",
    );
}

#[test]
fn fourth_replay_opening_color_clue_is_a_four_charm() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(0).expect("fixture prefix is legal");
    let view = state.view_for(PlayerId::new(0)).expect("Alice has a view");
    let deductions = LogicalDeductions::new(view).expect("valid deductions");
    let charm = Action::Clue {
        target: PlayerId::new(3),
        clue: Clue::Suit(Suit::Yellow),
    };
    let rank_charm = Action::Clue {
        target: PlayerId::new(3),
        clue: Clue::Rank(Rank::Four),
    };

    assert!(
        !h_group_clue_candidates(&deductions, HGroupProfile::Level(HGroupLevel::Level22))
            .iter()
            .any(|candidate| candidate.action == charm),
        "the Charm must not be available before Level 23"
    );
    assert!(
        h_group_clue_candidates(&deductions, HGroupProfile::Level(HGroupLevel::Level23))
            .iter()
            .any(|candidate| candidate.action == charm),
        "Level 23 permits a 4 Charm with either a color clue or a rank clue"
    );
    assert!(
        h_group_clue_candidates(&deductions, HGroupProfile::Level(HGroupLevel::Level23))
            .iter()
            .any(|candidate| candidate.action == rank_charm),
        "the ordinary rank form of the 4 Charm remains available"
    );

    let after_clue = fixture
        .state_at_turn(1)
        .expect("opening Charm is a legal fixture prefix");
    let deductions = LogicalDeductions::new(
        after_clue
            .view_for(PlayerId::new(1))
            .expect("Bob has a view"),
    )
    .expect("valid deductions");
    let replay = replay_h_group(&deductions, HGroupProfile::Level(HGroupLevel::Level23));
    assert!(
        replay.signals.iter().any(|signal| {
            signal.kind == HGroupMoveKind::Charm
                && signal.target == Some(PlayerId::new(1))
                && signal.cards == [CardId::new(4)]
        }),
        "the color 4 Charm must force Bob's Fourth Finesse Position: {replay:#?}"
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Level(HGroupLevel::Level23)),
        Some(Action::Play(CardId::new(4))),
        "Bob must immediately prove the Charm by blind-playing his Fourth Finesse Position",
    );
}

#[test]
fn fourth_replay_move_three_uses_the_charm_settled_prompt_without_inverting_focus() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(2).expect("fixture prefix is legal");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("Cathy has a view"),
    )
    .expect("valid deductions");
    let rank_two = Clue::Rank(Rank::Two);
    let rank_two_touched = deductions.view().hands[3]
        .iter()
        .filter(|card| {
            card.identity
                .is_some_and(|identity| rank_two.matches(identity))
        })
        .map(|card| card.id)
        .collect::<Vec<_>>();
    assert!(
        !prospective_clue_signal_kinds(
            deductions.view(),
            HGroupProfile::Max,
            PlayerId::new(3),
            rank_two,
            &rank_two_touched,
        )
        .contains(&HGroupMoveKind::FocusInversion),
        "a 2 Save remains chop-focused; filling in the collateral yellow 2 does not invert focus",
    );

    let expected = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Rank(Rank::Three),
    };
    assert!(
        h_group_clue_candidates(&deductions, HGroupProfile::Max)
            .iter()
            .any(|candidate| candidate.action == expected),
        "the Charm-settled yellow 4 must not block Donald's yellow-2 Prompt into Bob's yellow 3",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(expected),
    );
}

#[test]
fn fourth_replay_move_five_uses_the_visible_reverse_prompt() {
    let fixture = expert_replay_p4v0s3();
    let before_clue = fixture.state_at_turn(4).expect("fixture prefix is legal");
    let deductions = LogicalDeductions::new(
        before_clue
            .view_for(before_clue.current_player())
            .expect("Alice has a view"),
    )
    .expect("valid deductions");
    let yellow = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Suit(Suit::Yellow),
    };
    assert!(
        h_group_clue_candidates(&deductions, HGroupProfile::Max)
            .iter()
            .any(|candidate| candidate.action == yellow),
        "yellow 5 connects through Donald's visible, previously clued yellow 4",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(yellow),
    );

    let after_clue = fixture.state_at_turn(5).expect("fixture clue is legal");
    let deductions = LogicalDeductions::new(
        after_clue
            .view_for(after_clue.current_player())
            .expect("Bob has a view"),
    )
    .expect("valid deductions");
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(6))),
        "the visible reverse Prompt must not create a false yellow-4 Layered Finesse in Bob's hand",
    );
}

#[test]
fn fourth_replay_move_seven_recognizes_the_accounted_yellow_trash_chop_move() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(6).expect("fixture prefix is legal");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("Cathy has a view"),
    )
    .expect("valid deductions");
    let expected = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Suit(Suit::Yellow),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.action == expected),
        "yellow 1-3 are played, Donald has the promised yellow 4, and Cathy has the gotten yellow 5, so Alice's yellow card is known trash: {candidates:#?}",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(expected),
    );
}

#[test]
fn fourth_replay_move_nine_discards_the_trash_chop_move_target_off_chop() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(8).expect("fixture prefix is legal");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("Alice has a view"),
    )
    .expect("valid deductions");
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    assert_ne!(
        inferred.chops[0],
        Some(CardId::new(2)),
        "the Trash Chop Move moved Alice's chop beyond the clued yellow trash",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Discard(CardId::new(2))),
        "discarding off-chop known trash does not end the Early Game",
    );
}

#[test]
fn fourth_replay_move_eight_uses_the_demonstrated_charm_identity_as_a_prompt() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(7).expect("fixture prefix is legal");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("Donald has a view"),
    )
    .expect("valid deductions");
    let replay = replay_h_group(&deductions, HGroupProfile::Max);
    assert_eq!(
        replay.cards.facts.known_identity(CardId::new(15)),
        Some(Card::new(Suit::Yellow, Rank::Four)),
        "Bob's demonstrated Charm proves the hidden clue focus is yellow 4 to Donald",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(15))),
        "the proven yellow 4 is the visible reverse Prompt for Cathy's yellow 5",
    );
}

#[test]
fn third_replay_move_thirty_two_resolves_the_demonstrated_bluff() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(31).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Donald has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert!(
        !inferred.playable_now.contains(&CardId::new(33)),
        "Bob's purple-5 play demonstrates Alice's Bluff and cancels the competing yellow-2 Finesse: {inferred:#?}",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Discard(CardId::new(14))),
    );
}

#[test]
fn third_replay_green_four_counterfactual_prefers_rank_four_to_bob() {
    let mut fixture = expert_replay_p4v0s2();
    // Preserve the deck multiset while making Alice's ambiguous rank-4 card
    // green rather than yellow.
    fixture.deck.swap(2, 39);
    let state = fixture.state_at_turn(30).expect("fixture prefix is legal");
    let view = state.view_for(PlayerId::new(2)).expect("Cathy has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let rank_two = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Rank(Rank::Two),
    };
    let rank_four = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Rank(Rank::Four),
    };
    let candidate = |action| {
        candidates
            .iter()
            .find(|candidate| candidate.action == action)
            .expect("both comparison clues are convention-valid")
    };

    assert!(candidate(rank_four).action_coverage() > candidate(rank_two).action_coverage());
    assert!(candidate(rank_four).score() > candidate(rank_two).score());
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(rank_four),
    );
}

#[test]
fn third_replay_declined_rank_four_resolves_alices_card_as_yellow_four() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(36).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("current player has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let baseline_domain = || {
        super::super::inverse_planning::baseline(|| {
            let ordinary = replay_h_group(&deductions, HGroupProfile::Max);
            assert!(ordinary.strategic_deductions.is_empty());
            convention_card_inferences(&deductions, &ordinary)
                .into_iter()
                .find(|note| note.card == CardId::new(2))
                .unwrap()
                .identities
        })
    };
    let ordinary_domain = baseline_domain();
    assert!(ordinary_domain.contains(Card::new(Suit::Green, Rank::Four)));
    let replay = replay_h_group(&deductions, HGroupProfile::Max);
    assert!(
        replay
            .strategic_deductions
            .iter()
            .any(|proof| proof.card == CardId::new(2)
                && proof.checked_assignments > 1
                && proof.witnesses.len() > 1
                && proof.is_valid()),
        "the exact note must have joint-hand evidence, not a one-world shortcut"
    );
    let inferred = infer_h_group_from_replay(&deductions, replay.clone(), HGroupProfile::Max);
    let yellow_four = Card::new(Suit::Yellow, Rank::Four);
    let card = inferred
        .cards
        .iter()
        .find(|card| card.card == CardId::new(2))
        .expect("Alice still holds card #2");

    assert_eq!(card.identities, IdentitySet::singleton(yellow_four));
    assert!(inferred.playable_now.contains(&CardId::new(2)));
    assert_eq!(
        baseline_domain(),
        ordinary_domain,
        "strategic conclusions must not contaminate lower-order counterfactual queries"
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(2))),
    );
}

#[test]
fn third_replay_counterfactual_yellow_line_is_not_interrupted_by_an_early_save() {
    // Human-reviewed alternative to Donald's red clue on Hanab Live turn 36.
    // This is a counterfactual branch, not a change to the expert replay.
    let mut fixture = expert_replay_p4v0s2();
    fixture.deck.swap(2, 39);
    let state = fixture.state_at_turn(35).expect("legal reviewed prefix");
    let view = state
        .view_for(PlayerId::new(3))
        .expect("Donald's legal view");
    let root = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Suit(Suit::Yellow),
    };
    let (outcome, evidence) = super::super::symbolic_line::project_h_group_projection(
        &view,
        HGroupProfile::Max,
        root,
        4,
        &crate::AnalysisControl::default(),
    )
    .expect("unlimited analysis");
    let actions = evidence
        .steps
        .iter()
        .map(|step| step.projected.action)
        .collect::<Vec<_>>();
    assert_eq!(
        actions,
        vec![
            root,
            Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Suit(Suit::Yellow)
            },
            Action::Play(CardId::new(28)),
            Action::Play(CardId::new(38)),
        ],
        "{evidence:#?}",
    );
    assert_eq!(outcome.score_gain, 2);
    assert_eq!(outcome.strikes, 0);
    assert_eq!(outcome.clues_spent, 2);
    assert_eq!(outcome.clues_gained, 1);
}

#[test]
fn third_replay_counterfactual_finesse_checks_the_rest_of_alices_hand() {
    // p4v0s2, turn 36: changing the focal card alone is insufficient. A
    // different newest card in Alice's hand changes yellow to Bob's reading.
    super::super::inverse_planning::baseline(|| {
        let mut fixture = expert_replay_p4v0s2();
        fixture.deck.swap(2, 39);
        let interpretation = |fixture: &HanabiLiveReplay| {
            let state = fixture.state_at_turn(35).unwrap();
            let view = state.view_for(PlayerId::new(3)).unwrap();
            super::super::prospective_clue_primary_interpretation(
                &view,
                HGroupProfile::Max,
                PlayerId::new(1),
                Clue::Suit(Suit::Yellow),
                &[CardId::new(28)],
            )
            .unwrap()
            .focus_identities
        };
        let ordinary = interpretation(&fixture);
        fixture.deck.swap(36, 39);
        let interfering = interpretation(&fixture);
        assert_eq!(
            ordinary,
            IdentitySet::singleton(Card::new(Suit::Yellow, Rank::Four))
        );
        assert!(interfering.contains(Card::new(Suit::Yellow, Rank::Five)));
        assert_ne!(ordinary, interfering);
    });
}

#[test]
fn third_replay_gentlemans_discard_does_not_delay_an_immediate_five_refund() {
    // Counterfactual p4v0s2 turn 36: Bob can transfer y4 to Alice's newest
    // card, but playing it lets Cathy finish yellow immediately. The token
    // from the transfer is not a benefit over that already-funded sequence.
    super::super::inverse_planning::baseline(|| {
        let mut fixture = expert_replay_p4v0s2();
        fixture.deck.swap(2, 39);
        fixture.deck.swap(36, 39);
        let mut state = fixture.state_at_turn(35).unwrap();
        state
            .apply(Action::Clue {
                target: PlayerId::new(1),
                clue: Clue::Rank(Rank::Four),
            })
            .unwrap();
        state
            .apply(Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Suit(Suit::Yellow),
            })
            .unwrap();
        let d = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
        assert_eq!(
            select_h_group_action(&d, HGroupProfile::Max),
            Some(Action::Play(CardId::new(28)))
        );
    });
}

#[test]
fn third_replay_move_thirty_one_does_not_defer_a_one_for_one_play_clue() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(30).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Cathy has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");

    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        }),
        "an ordinary direct Play Clue must not be treated as a multi-action line merely because later inverse planning can resolve Alice's old rank-4 clue",
    );
}

#[test]
fn third_replay_move_thirty_six_prefers_occupying_play_over_save() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(35).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Donald has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let replay = replay_h_group(&deductions, HGroupProfile::Max);
    assert!(
        !replay.cards.already_playing.contains(&CardId::new(34))
            && !replay.pending_connections.iter().any(|connection| {
                connection.actor == PlayerId::new(1)
                    && replay.pending_connections.is_active(connection)
            }),
        "Bob has no pre-existing play obligation before Donald's clue: {replay:#?}",
    );
    let five = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Rank(Rank::Five),
    };
    let red = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Suit(Suit::Red),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let candidate = |action| {
        candidates
            .iter()
            .find(|candidate| candidate.action == action)
            .expect("both ordinary clues remain convention-valid")
    };

    assert!(candidate(five).is_save() && !candidate(five).immediate_play());
    assert!(candidate(red).immediate_play() && !candidate(red).is_save());
    assert!(
        candidate(red).score() > candidate(five).score(),
        "red occupies Bob with red 5, postponing any discard of his green-5 chop; red={:#?}; five={:#?}",
        candidate(red),
        candidate(five),
    );
}

#[test]
fn third_replay_move_forty_uses_green_to_advance_the_final_stack() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(39).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Donald has a view");
    let target = PlayerId::new(1);
    let clue = Clue::Suit(Suit::Green);
    let touched = view.hands[target.index()]
        .iter()
        .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let interpretation =
        prospective_clue_primary_interpretation(&view, HGroupProfile::Max, target, clue, &touched);
    let interpretation = interpretation.expect("recipient recognizes the green clue");
    assert_eq!(interpretation.kind, HGroupClueKind::Play);
    assert_eq!(interpretation.focus, CardId::new(23));
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue { target, clue }),
        "green advances the remaining green-3/4/5 stack; candidates={candidates:#?}",
    );
}

#[test]
fn third_replay_final_green_line_remains_committed_before_the_last_clue() {
    let fixture = expert_replay_p4v0s2();
    let after_clue = fixture.state_at_turn(40).expect("green clue is legal");
    let after_clue_view = after_clue
        .view_for(PlayerId::new(0))
        .expect("Alice has a view after the clue");
    let after_clue_deductions =
        LogicalDeductions::new(after_clue_view).expect("Alice's view is logical");
    let after_clue_replay = replay_h_group(&after_clue_deductions, HGroupProfile::Max);
    let clue_turn = after_clue_deductions
        .view()
        .history
        .last()
        .expect("green clue is the latest event")
        .turn;
    let green_clue = after_clue_replay
        .clues
        .iter()
        .find(|clue| clue.turn == clue_turn)
        .expect("the green clue has an interpretation");
    assert_eq!(green_clue.focus, CardId::new(23));
    assert!(
        green_clue
            .play_identities
            .contains(Card::new(Suit::Green, Rank::Five)),
        "Bob's green 5 remains the focus: {green_clue:#?}",
    );
    assert!(
        !after_clue_replay.signals.iter().any(|signal| {
            signal.turn == clue_turn && signal.kind == HGroupMoveKind::FocusInversion
        }),
        "green 3 is the first play in the line; it does not invert focus away from green 5: {:#?}",
        after_clue_replay.signals,
    );
    assert!(
        after_clue_replay.pending_connections.iter().any(|pending| {
            pending.actor == PlayerId::new(0)
                && pending.cards.first() == Some(&CardId::new(39))
                && pending.expected == Card::new(Suit::Green, Rank::Four)
        }),
        "the green clue must initially schedule Alice's green 4: {:#?}",
        after_clue_replay.pending_connections,
    );
    let state = fixture.state_at_turn(43).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Donald has a view");
    let team = TeamConventionSnapshot::new(view, HGroupProfile::Max);
    let alice = team
        .projection(PlayerId::new(0))
        .expect("Alice has a projection");
    let bob = team
        .projection(PlayerId::new(1))
        .expect("Bob has a projection");

    assert!(
        alice.replay.pending_connections.iter().any(|pending| {
            pending.actor == PlayerId::new(0)
                && pending.cards.first() == Some(&CardId::new(39))
                && pending.expected == Card::new(Suit::Green, Rank::Four)
        }),
        "Alice's executable green-4 connection must survive the later-layer discard: {:#?}",
        alice.replay.pending_connections,
    );
    assert!(
        alice.inferred.signals.iter().any(|signal| {
            signal.target == Some(PlayerId::new(0))
                && signal.cards.contains(&CardId::new(39))
                && signal.identity == Some(Card::new(Suit::Green, Rank::Four))
                && signal.kind == HGroupMoveKind::LayeredFinesse
        }),
        "Alice still owes green 4: {:#?}",
        alice.inferred,
    );
    assert!(
        bob.inferred.clues.iter().any(|clue| {
            clue.focus == CardId::new(23)
                && matches!(clue.kind, HGroupClueKind::Play | HGroupClueKind::PlayOrSave)
                && clue
                    .play_identities
                    .contains(Card::new(Suit::Green, Rank::Five))
        }),
        "Bob still owns the green-5 focus: {:#?}",
        bob.inferred,
    );
}

#[test]
fn third_replay_move_two_keeps_the_valid_but_stalled_purple_line() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(1).expect("fixture prefix is legal");
    let action = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Suit(Suit::Purple),
    };
    let view = state
        .view_for(state.current_player())
        .expect("current player has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let bob_inferences = infer_h_group(&deductions, HGroupProfile::Max);
    let green_focus = bob_inferences
        .cards
        .iter()
        .find(|card| card.card == CardId::new(7))
        .expect("Bob retains Alice's opening green focus");
    assert!(
        green_focus
            .identities
            .contains(Card::new(Suit::Green, Rank::One))
            && green_focus
                .identities
                .contains(Card::new(Suit::Green, Rank::Two)),
        "Bob retains the correlated green-1/green-2 promise from Alice's opening clue",
    );
    let hazard = prospective_clue_hazard(
        deductions.view(),
        HGroupProfile::Max,
        PlayerId::new(0),
        CardId::new(0),
        Clue::Suit(Suit::Purple),
        &[CardId::new(0)],
        false,
    );
    assert_eq!(
        hazard, None,
        "purple to Alice is a convention-valid Layered Finesse",
    );
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let rank_two_hazard = prospective_clue_hazard(
        deductions.view(),
        HGroupProfile::Max,
        PlayerId::new(3),
        CardId::new(13),
        Clue::Rank(Rank::Two),
        &[CardId::new(13)],
        false,
    );
    let mut rank_two_state = state.clone();
    rank_two_state
        .apply(Action::Clue {
            target: PlayerId::new(3),
            clue: Clue::Rank(Rank::Two),
        })
        .expect("rank 2 is game-legal");
    let donald = LogicalDeductions::new(
        rank_two_state
            .view_for(PlayerId::new(3))
            .expect("Donald has a view"),
    )
    .expect("logical view");
    let donald_inferred = infer_h_group(&donald, HGroupProfile::Max);
    assert_eq!(
        rank_two_hazard, None,
        "rank 2 to Donald remains safe: {donald_inferred:#?}"
    );
    let purple_candidate = candidates
        .iter()
        .find(|candidate| candidate.action == action)
        .expect("purple to Alice remains a candidate");
    let rank_two_to_donald = candidates
        .iter()
        .find(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(3),
                    clue: Clue::Rank(Rank::Two),
                }
        })
        .expect("rank 2 to Donald remains a candidate");
    let mut after = state;
    after.apply(action).expect("purple clue is game-legal");
    assert!(
        purple_candidate.score() < rank_two_to_donald.score(),
        "the valid but stalled purple line must lose to a line that advances: purple={purple_candidate:#?}, rank two={rank_two_to_donald:#?}",
    );

    let cathy_view = after.view_for(PlayerId::new(2)).expect("Cathy has a view");
    let cathy_deductions = LogicalDeductions::new(cathy_view).expect("logical view");
    let cathy_inferences = infer_h_group(&cathy_deductions, HGroupProfile::Max);
    assert!(
        !cathy_inferences.playable_now.contains(&CardId::new(11)),
        "Cathy sees Donald's purple 1 and must pass rather than blind-play her duplicate",
    );
    let donald_view = after.view_for(PlayerId::new(3)).expect("Donald has a view");
    let donald_deductions = LogicalDeductions::new(donald_view).expect("logical view");
    let donald_inferences = infer_h_group(&donald_deductions, HGroupProfile::Max);
    assert!(
        !donald_inferences.playable_now.contains(&CardId::new(14)),
        "Donald must not treat his purple 1 as the immediate continuation",
    );
}

#[test]
fn third_replay_move_two_distinguishes_bluff_from_clandestine_finesse() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(1).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("current player has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let red = Action::Clue {
        target: PlayerId::new(3),
        clue: Clue::Suit(Suit::Red),
    };
    let two = Action::Clue {
        target: PlayerId::new(3),
        clue: Clue::Rank(Rank::Two),
    };
    let blue = Action::Clue {
        target: PlayerId::new(3),
        clue: Clue::Suit(Suit::Blue),
    };
    let signal_kinds = |action| {
        let Action::Clue { target, clue } = action else {
            unreachable!();
        };
        let touched = deductions.view().hands[target.index()]
            .iter()
            .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
            .map(|card| card.id)
            .collect::<Vec<_>>();
        prospective_team_clue_signal_kinds(
            deductions.view(),
            HGroupProfile::Max,
            target,
            clue,
            &touched,
        )
    };
    let red_signals = signal_kinds(red);
    let two_signals = signal_kinds(two);
    assert!(
        red_signals.contains(&HGroupMoveKind::Bluff),
        "red to Donald is a Bluff on Cathy's purple 1: {red_signals:?}",
    );
    assert!(
        two_signals.contains(&HGroupMoveKind::ClandestineFinesse),
        "rank 2 to Donald is a Clandestine Finesse through Cathy's purple 1 and red 1: {two_signals:?}",
    );
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let candidate = |action| {
        candidates
            .iter()
            .find(|candidate| candidate.action == action)
            .expect("candidate remains convention-valid")
    };
    assert_eq!(
        (
            candidate(red).convention_connection_steps(),
            candidate(two).convention_connection_steps(),
        ),
        (Some(1), Some(2)),
        "the Bluff must supply one connector and the Clandestine Finesse two; red signals: {red_signals:?}; rank-2 signals: {two_signals:?}; candidates: {candidates:#?}",
    );
    assert_eq!(
        (
            candidate(red).convention_action_count(),
            candidate(two).convention_action_count(),
        ),
        (Some(2), Some(3)),
        "the engine must compare the Bluff as a 2-for-1 and the Clandestine Finesse as a 3-for-1",
    );
    assert!(
        candidate(two).score() > candidate(red).score(),
        "the 3-for-1 Clandestine Finesse must beat the 2-for-1 Bluff: red={:#?}; two={:#?}",
        candidate(red),
        candidate(two),
    );
    assert_eq!(
        (
            candidate(blue).convention_action_count(),
            candidate(blue).convention_connection_steps(),
        ),
        (Some(1), Some(1)),
        "the Stacked Ejection secures one blind play; protecting blue 5 does not establish blue 1-4",
    );
    assert!(
        !candidate(blue).is_urgent_save(),
        "Donald's existing green-1 obligation prevents him from discarding the blue 5",
    );
    assert!(
        candidate(two).score() > candidate(blue).score(),
        "the 3-for-1 Clandestine Finesse must beat the one-play Stacked Ejection: blue={:#?}; two={:#?}",
        candidate(blue),
        candidate(two),
    );
}

#[test]
fn third_replay_move_two_scores_rank_three_as_a_bluff_not_a_delayed_play() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(1).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("current player has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let rank_three = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Rank(Rank::Three),
    };
    let candidate = h_group_clue_candidates(&deductions, HGroupProfile::Max)
        .into_iter()
        .find(|candidate| candidate.action == rank_three)
        .expect("rank 3 remains a legal 3 Bluff");

    assert_eq!(candidate.purpose(), CluePurpose::Advanced);
    assert_eq!(candidate.connection_steps(), 0);
    assert_eq!(candidate.convention_connection_steps(), Some(1));
    // User-reviewed: one blind play plus protection of the focused 3 is
    // 2-for-1 efficiency, not two immediately executable plays.
    assert_eq!(candidate.convention_action_count(), Some(2));
    let clandestine = h_group_clue_candidates(&deductions, HGroupProfile::Max)
        .into_iter()
        .find(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(3),
                    clue: Clue::Rank(Rank::Two),
                }
        })
        .expect("the reviewed Reverse Clandestine Finesse remains valid");
    assert!(
        clandestine.score() > candidate.score(),
        "the 3-for-1 must beat the 2-for-1 without invented coverage or a false blue-5 deadline"
    );
    assert!(
        candidate.score() < 400,
        "a 3 Bluff gets Cathy's immediate blind play but does not promise purple 2 or make Alice's purple 3 playable: {candidate:#?}",
    );
}

#[test]
fn third_replay_yellow_to_bob_must_not_expose_critical_chop() {
    // User-reviewed p4v0s2 turn 25: y2 through Donald's y1 is still
    // possible, so Alice cannot rely on Bob playing the focused y1.
    let state = expert_replay_p4v0s2().state_at_turn(24).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let after = prospective_clue_view(
        &view,
        PlayerId::new(1),
        Clue::Suit(Suit::Yellow),
        &[CardId::new(5), CardId::new(28)],
    );
    let (d, r) = projected_h_group_replay(&after, HGroupProfile::Max, PlayerId::new(1)).unwrap();
    let inferred = infer_h_group_from_replay(&d, r, HGroupProfile::Max);
    assert_eq!(
        inferred
            .cards
            .iter()
            .find(|card| card.card == CardId::new(5))
            .unwrap()
            .identities,
        IdentitySet::singleton(Card::new(Suit::Yellow, Rank::One))
            .union(IdentitySet::singleton(Card::new(Suit::Yellow, Rank::Two)))
    );
    assert!(!inferred.playable_now.contains(&CardId::new(5)));
    let deductions = LogicalDeductions::new(view).unwrap();
    assert!(
        !h_group_clue_candidates(&deductions, HGroupProfile::Max)
            .iter()
            .any(|c| c.action
                == Action::Clue {
                    target: PlayerId::new(1),
                    clue: Clue::Suit(Suit::Yellow)
                })
    );
}

#[test]
fn third_replay_turn_six_blue_is_a_five_color_ejection() {
    // User-reviewed p4v0s2 turn 6: Cathy ejects p2 #9 and Donald saves b5 #12.
    // https://hanabi.github.io/level-16/#the-5-color-ejection-5ce
    let state = expert_replay_p4v0s2().state_at_turn(5).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let deductions = LogicalDeductions::new(view.clone()).unwrap();
    let after = prospective_clue_view(
        &view,
        PlayerId::new(3),
        Clue::Suit(Suit::Blue),
        &[CardId::new(12)],
    );
    let (cathy, _) =
        projected_h_group_replay(&after, HGroupProfile::Max, PlayerId::new(2)).unwrap();
    assert_eq!(
        select_h_group_action(&cathy, HGroupProfile::Max),
        Some(Action::Play(CardId::new(9)))
    );
    let action = Action::Clue {
        target: PlayerId::new(3),
        clue: Clue::Suit(Suit::Blue),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.action == action),
        "{candidates:#?}"
    );
    let analysis = analyze_h_group_convention(&deductions, HGroupProfile::Max);
    assert!(
        analysis
            .actions
            .iter()
            .any(|candidate| candidate.action == action),
        "{:?}",
        analysis.actions
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(action)
    );
    let after_play = expert_replay_p4v0s2().state_at_turn(7).unwrap();
    let donald = LogicalDeductions::new(after_play.view_for(PlayerId::new(3)).unwrap()).unwrap();
    let knowledge = infer_h_group(&donald, HGroupProfile::Max);
    assert_eq!(
        knowledge
            .cards
            .iter()
            .find(|card| card.card == CardId::new(12))
            .unwrap()
            .identities,
        IdentitySet::singleton(Card::new(Suit::Blue, Rank::Five))
    );
}

#[test]
fn cathy_does_not_stomp_donalds_pending_purple_finesse() {
    // User-reviewed p4v0s2 turn 3 after Bob's hypothetical purple to Alice.
    // Project Cathy from Bob's view, without revealing Bob's own cards.
    let state = expert_replay_p4v0s2().state_at_turn(1).unwrap();
    let source = state.view_for(PlayerId::new(1)).unwrap();
    let after = ProspectiveTransition::clue_by(
        &source,
        PlayerId::new(1),
        PlayerId::new(0),
        Clue::Suit(Suit::Purple),
        &[CardId::new(0)],
    );
    let (deductions, replay) = PerspectiveProjector::new(&after, HGroupProfile::Max)
        .project(PlayerId::new(2), PerspectiveDepth::NestedRecipients)
        .unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let analysis = analyze_h_group_convention(&deductions, HGroupProfile::Max);
    assert!(replay.clues.iter().any(|prior| {
        prior.focus == CardId::new(0)
            && prior.unresolved_visible_prefix.iter().any(|step| {
                step.actor == PlayerId::new(3)
                    && step.cards == [CardId::new(14)]
                    && step.expected == Card::new(Suit::Purple, Rank::One)
            })
    }));
    let stomp = Action::Clue {
        target: PlayerId::new(3),
        clue: Clue::Suit(Suit::Purple),
    };
    assert!(
        !candidates.iter().any(|candidate| candidate.action == stomp),
        "{candidates:#?}"
    );
    assert!(
        analysis
            .rejected_actions
            .iter()
            .any(|rejected| rejected.action == stomp
                && rejected.reason == ConventionRejectionReason::RedundantOutcome)
    );
    assert!(
        !candidates.iter().any(|candidate| candidate.action
            == Action::Clue {
                target: PlayerId::new(3),
                clue: Clue::Rank(Rank::One)
            }),
        "the rank-1 clue only names the same pending g1 and p1"
    );
    assert!(
        !replay
            .pending_connections
            .iter()
            .any(|step| step.actor == PlayerId::new(2)
                && step.expected == Card::new(Suit::Purple, Rank::Two)),
        "the visible prefix must not commit Cathy's unknown follow-up as purple 2"
    );
    assert!(candidates.iter().any(|candidate| candidate.action
        == Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue)
        }));
    let save = candidates
        .iter()
        .find(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(3),
                    clue: Clue::Rank(Rank::Five),
                }
        })
        .unwrap();
    assert!(!save.is_urgent_save(), "{save:#?}");
    assert!(
        analysis.actions.iter().any(|candidate| {
            candidate.action == save.action
                && candidate.preference.policy_tier() == crate::ConventionPolicyTier::Deferred
        }),
        "the Early Save stays admitted, but must not displace the playable-chop clue"
    );
    assert_eq!(
        crate::planner::choose_projected_follow_up(
            &deductions,
            HGroupProfile::Max,
            &crate::AnalysisControl::default()
        )
        .unwrap(),
        Some(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue)
        }),
        "{candidates:#?}"
    );
}

#[test]
fn reviewed_ambiguous_purple_line_passes_back_instead_of_duplicating() {
    // Human-reviewed counterfactual from p4v0s2 turn 2. Cathy defers to
    // Donald, and Donald must not make her duplicate p1 as the next rank.
    // The extra 5 Save only supplies Cathy's unrelated intervening action.
    let mut state = expert_replay_p4v0s2().state_at_turn(1).unwrap();
    for action in [
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(15)),
        Action::Play(CardId::new(1)),
        Action::Play(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(3),
            clue: Clue::Rank(Rank::Five),
        },
    ] {
        state.apply(action).unwrap();
    }
    let d = LogicalDeductions::new(state.view_for(PlayerId::new(3)).unwrap()).unwrap();
    assert!(
        super::super::prospective::assumed_play_has_unsafe_inference(
            d.view(),
            HGroupProfile::Max,
            CardId::new(14),
            Card::new(Suit::Purple, Rank::One)
        )
    );
    assert!(
        !ordered_h_group_actions(&d, HGroupProfile::Max).contains(&Action::Play(CardId::new(14)))
    );
}

#[test]
fn third_replay_save_does_not_preempt_an_arriving_promised_play() {
    // p4v0s2 turn 5: Cathy's promised r1 makes Donald's r2 playable before
    // his next turn. His b5 is not an urgent Save merely because r1 is
    // absent from the stack at Alice's current turn.
    let state = expert_replay_p4v0s2().state_at_turn(4).unwrap();
    let d = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&d, HGroupProfile::Max);
    let save = candidates
        .iter()
        .find(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(3),
                    clue: Clue::Rank(Rank::Five),
                }
        })
        .unwrap();
    assert!(!save.is_urgent_save(), "{save:#?}");
}

#[test]
fn third_replay_move_sixteen_does_not_reinterpret_an_already_promised_play_as_a_bluff() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(16).expect("fixture prefix is legal");
    let view = state.view_for(PlayerId::new(0)).expect("Alice has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let inferences = infer_h_group(&deductions, HGroupProfile::Max);
    let blue_one = Card::new(Suit::Blue, Rank::One);
    let note = inferences
        .cards
        .iter()
        .find(|note| note.card == CardId::new(1))
        .expect("Alice retains her focused blue card");

    assert_eq!(note.identities, IdentitySet::singleton(blue_one));
    assert!(
        !inferences.signals.iter().any(|signal| {
            signal.kind == HGroupMoveKind::Bluff
                && signal.cards == [CardId::new(17), CardId::new(1)]
        }),
        "Donald's already-promised red 3 must not resolve Cathy's later blue clue as a Bluff",
    );
}

#[test]
fn third_replay_move_sixteen_keeps_donalds_promised_red_three_due() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(15).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Donald has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let replay = replay_h_group(&deductions, HGroupProfile::Max);
    let inferences = infer_h_group_from_replay(&deductions, replay.clone(), HGroupProfile::Max);

    assert!(
        inferences.playable_now.contains(&CardId::new(17)),
        "the earlier red connection remains due after red 1 and red 2 play; pending={:#?}; signals={:#?}",
        replay.pending_connections,
        replay.signals
    );
}

#[test]
fn third_replay_move_twenty_three_keeps_cathys_red_four_after_the_connection() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(22).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Cathy has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let inferences = infer_h_group(&deductions, HGroupProfile::Max);

    assert!(
        inferences.playable_now.contains(&CardId::new(8)),
        "completing red 1 through red 3 must leave the original red-4 focus due"
    );
}

#[test]
fn third_replay_move_six_admits_the_blue_stacked_ejection() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(5).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Bob has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let action = Action::Clue {
        target: PlayerId::new(3),
        clue: Clue::Suit(Suit::Blue),
    };
    let touched = vec![CardId::new(12)];
    let signals = prospective_team_clue_signal_kinds(
        deductions.view(),
        HGroupProfile::Max,
        PlayerId::new(3),
        Clue::Suit(Suit::Blue),
        &touched,
    );
    let hazard = prospective_clue_hazard(
        deductions.view(),
        HGroupProfile::Max,
        PlayerId::new(3),
        CardId::new(12),
        Clue::Suit(Suit::Blue),
        &touched,
        false,
    );

    assert!(
        signals.contains(&HGroupMoveKind::StackedEjection),
        "blue to Donald must be recognized as a Stacked Ejection: {signals:?}",
    );
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let candidate = candidates
        .iter()
        .find(|candidate| candidate.action == action)
        .unwrap_or_else(|| {
            panic!(
                "the recognized Stacked Ejection must survive candidate admission: signals={signals:?}; ordinary-play hazard={hazard:?}"
            )
        });
    assert!(
        candidate.is_urgent_save(),
        "the recognized Stacked Ejection must survive candidate admission: signals={signals:?}; ordinary-play hazard={hazard:?}",
    );
}

#[test]
fn third_replay_move_seven_has_a_consistent_stacked_ejection_belief() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(6).expect("fixture prefix is legal");
    let view = state.view_for(PlayerId::new(2)).expect("Cathy has a view");
    let deductions = LogicalDeductions::new(view.clone()).expect("logical view");
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    let analysis = crate::SupportedConvention::HGroup(HGroupProfile::Max).analyze(&deductions);
    let information = crate::InformationSet::new(&view).expect("information set is valid");

    assert!(
        inferred.playable_now.contains(&CardId::new(9)),
        "the Stacked Ejection must instruct Cathy to play her purple 2: {inferred:#?}",
    );
    assert!(
        !inferred.discard_now.contains(&CardId::new(10)),
        "Cathy cannot be required to discard her still-active red-1 connector",
    );
    assert_eq!(
        inferred
            .cards
            .iter()
            .find(|card| card.card == CardId::new(10))
            .map(|card| card.identities),
        Some(IdentitySet::singleton(Card::new(Suit::Red, Rank::One))),
        "the red clue must preserve the identity of Cathy's active red-1 connector",
    );
    assert!(
        information
            .world_count_up_to(&analysis.belief_constraints, 1)
            .worlds()
            > 0,
        "the Stacked Ejection constraints must admit Cathy's actual information set: constraints={:#?}; inferences={:#?}",
        analysis.belief_constraints,
        inferred,
    );
}

#[test]
fn third_replay_move_eight_resolves_the_blue_focus_as_blue_five() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(7).expect("fixture prefix is legal");
    let view = state.view_for(PlayerId::new(3)).expect("Donald has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    let blue_five = Card::new(Suit::Blue, Rank::Five);

    assert_eq!(
        inferred
            .cards
            .iter()
            .find(|card| card.card == CardId::new(12))
            .map(|card| card.identities),
        Some(IdentitySet::singleton(blue_five)),
        "the resolved 5 Color Ejection must replace the apparent blue-1 interpretation",
    );
    assert!(
        !inferred.playable_now.contains(&CardId::new(12)),
        "blue 5 is saved but cannot play on an empty blue stack",
    );
}

#[test]
fn third_replay_move_eight_admits_purple_to_alice() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(7).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Donald has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let action = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Suit(Suit::Purple),
    };
    let convention = crate::SupportedConvention::HGroup(HGroupProfile::Max).analyze(&deductions);
    assert!(
        convention
            .actions
            .iter()
            .any(|candidate| candidate.action == action),
        "purple directly plays Alice's purple 3; rejected={:#?}",
        convention.rejected_actions
    );
}

#[test]
fn third_replay_move_ten_preserves_the_visible_red_continuation() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(9).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Bob has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let rank_four = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Rank(Rank::Four),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let candidate = candidates
        .iter()
        .find(|candidate| candidate.action == rank_four)
        .expect("rank 4 to Alice is convention-valid");
    assert!(
        candidate.preserves_visible_continuation(),
        "cluing now preserves Donald's red 2, which unlocks the visible red 3"
    );
    let analysis = crate::analyze_position(
        deductions.view(),
        crate::SupportedConvention::HGroup(HGroupProfile::Max),
        crate::PlannerConfig {
            objective: crate::PlanningObjective::PerfectScore,
            ..crate::PlannerConfig::default()
        },
    )
    .expect("move-10 position is analyzable");
    assert_eq!(analysis.planner.best_action, rank_four);
}

#[test]
fn third_replay_move_thirteen_uses_the_actors_gentlemans_discard_perspective() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(12).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Alice has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let analysis = crate::analyze_position(
        deductions.view(),
        crate::SupportedConvention::HGroup(HGroupProfile::Max),
        crate::PlannerConfig {
            objective: crate::PlanningObjective::PerfectScore,
            ..crate::PlannerConfig::default()
        },
    )
    .expect("move-13 position is analyzable");

    assert_eq!(
        analysis.planner.best_action,
        Action::Discard(CardId::new(19)),
        "Alice sees Bob's matching purple 4 on Finesse Position and transfers her promised copy"
    );
}

#[test]
fn third_replay_gentlemans_discard_transfers_from_the_recipients_perspective() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture
        .state_at_turn(13)
        .expect("fixture through Alice's Gentleman's Discard is legal");
    let view = state.view_for(PlayerId::new(1)).expect("Bob has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    let purple_four = Card::new(Suit::Purple, Rank::Four);
    let note = inferred
        .cards
        .iter()
        .find(|note| note.card == CardId::new(6))
        .expect("Bob tracks his Finesse-position card");

    assert_eq!(note.identities, IdentitySet::singleton(purple_four));
    assert!(
        inferred.playable_now.contains(&CardId::new(6))
            && inferred.connection.is_none()
            && !note.finessed
            && note.play_obligation.is_none(),
        "the recipient must project the transferred purple-4 play without seeing his own card: {inferred:#?}",
    );

    let later = fixture
        .state_at_turn(24)
        .expect("fixture through the turn before the yellow clue is legal");
    let later_view = later.view_for(PlayerId::new(1)).expect("Bob has a view");
    let later_deductions = LogicalDeductions::new(later_view).expect("logical view");
    let later_inferred = infer_h_group(&later_deductions, HGroupProfile::Max);
    assert!(
        later_inferred.cards.iter().any(|note| {
            note.card == CardId::new(6) && note.identities == IdentitySet::singleton(purple_four)
        }) && (later_inferred.playable_now.contains(&CardId::new(6))
            || later_inferred
                .connection
                .is_some_and(|connection| connection.card == CardId::new(6))),
        "Bob's projected purple-4 action must remain live until he performs it: {later_inferred:#?}",
    );
}

#[test]
fn fourth_replay_gentlemans_discard_is_not_a_self_created_sacrifice() {
    // Reviewed p4v0s3 turns 16–18: Donald transfers p2 to Bob's #18.
    let fixture = expert_replay_p4v0s3();
    for (turn, observer) in [(16, 0), (16, 1), (17, 1)] {
        let state = fixture
            .state_at_turn(turn)
            .expect("reviewed prefix is legal");
        let view = state
            .view_for(PlayerId::new(observer))
            .expect("player view");
        let deductions = LogicalDeductions::new(view).expect("logical view");
        let inferred = infer_h_group(&deductions, HGroupProfile::Max);
        if observer == 1 {
            assert!(
                inferred.cards.iter().any(|note| {
                    note.card == CardId::new(18)
                        && note.identities
                            == IdentitySet::singleton(Card::new(Suit::Purple, Rank::Two))
                }),
                "Bob must receive the transfer: {inferred:#?}"
            );
            assert!(inferred.playable_now.contains(&CardId::new(18)));
        }
        assert!(inferred.signals.iter().any(|signal| {
            signal.turn == 15
                && signal.kind == HGroupMoveKind::GentlemansDiscard
                && signal.target == Some(PlayerId::new(1))
                && signal.cards.contains(&CardId::new(18))
        }));
        assert!(!inferred.signals.iter().any(|signal| {
            signal.turn == 15 && signal.kind == HGroupMoveKind::SacrificeDiscard
        }));
    }
}

#[test]
fn fourth_replay_projects_the_received_purple_two() {
    let state = expert_replay_p4v0s3().state_at_turn(16).unwrap();
    let source = state.view_for(PlayerId::new(0)).unwrap();
    let source = ProspectiveTransition::clue_by(
        &source,
        PlayerId::new(0),
        PlayerId::new(1),
        Clue::Suit(Suit::Green),
        &[CardId::new(7)],
    );
    let (deductions, replay) = PerspectiveProjector::new(&source, HGroupProfile::Max)
        .project(PlayerId::new(1), PerspectiveDepth::NestedRecipients)
        .unwrap();
    let inferred = infer_h_group_from_replay(&deductions, replay, HGroupProfile::Max);
    assert!(
        inferred.playable_now.contains(&CardId::new(18)),
        "{inferred:#?}"
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(18)))
    );
}

#[test]
fn fourth_replay_possible_prompt_waits_for_visible_finesse() {
    // User-reviewed turn 17: Donald must let Cathy demonstrate g3; g2 is
    // its prerequisite, not a decline of the visible Finesse.
    for turn in [17, 19, 23] {
        let state = expert_replay_p4v0s3().state_at_turn(turn).unwrap();
        let d = LogicalDeductions::new(state.view_for(PlayerId::new(3)).unwrap()).unwrap();
        let inferred = infer_h_group(&d, HGroupProfile::Max);
        assert!(
            !inferred.playable_now.contains(&CardId::new(17)),
            "turn {turn}: {inferred:#?}"
        );
        if turn == 19 {
            assert_eq!(
                select_h_group_action(&d, HGroupProfile::Max),
                Some(Action::Play(CardId::new(14)))
            );
        }
    }
}

#[test]
fn third_replay_move_fourteen_applies_normal_priority_to_the_transferred_card() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(13).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Bob has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");

    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(7))),
        "the exact Gentleman's-Discard note counts as clued for Priority, so green 2 precedes purple 4",
    );
}

#[test]
fn third_replay_move_eighteen_can_park_the_transferred_play_for_a_multi_action_clue() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(17).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Bob has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");

    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        }),
        "the blue clue schedules two team actions while the exact purple 4 remains safely parked",
    );
}

#[test]
fn third_replay_move_fifteen_admits_the_blue_layered_play_clue() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(14).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Cathy has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let blue_to_alice = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Suit(Suit::Blue),
    };
    let convention = crate::SupportedConvention::HGroup(HGroupProfile::Max).analyze(&deductions);

    assert!(
        convention
            .actions
            .iter()
            .any(|candidate| candidate.action == blue_to_alice),
        "the clue must establish Alice's blue 1, Cathy's blue 2, and Alice's blue 3; rejected={:#?}",
        convention.rejected_actions
    );
}

#[test]
fn third_replay_move_twenty_six_does_not_reinterpret_a_transferred_play_as_a_bluff() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(26).expect("fixture prefix is legal");
    let view = state.view_for(PlayerId::new(3)).expect("Donald has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let inferences = infer_h_group(&deductions, HGroupProfile::Max);
    let yellow_one = Card::new(Suit::Yellow, Rank::One);
    let note = inferences
        .cards
        .iter()
        .find(|note| note.card == CardId::new(30))
        .expect("Donald retains his focused yellow card");

    assert_eq!(note.identities, IdentitySet::singleton(yellow_one));
    assert!(
        !inferences.signals.iter().any(|signal| {
            signal.kind == HGroupMoveKind::Bluff
                && signal.cards == [CardId::new(6), CardId::new(30)]
        }),
        "Bob's transferred purple 4 must not resolve Alice's later yellow clue as a Bluff",
    );
}

#[test]
fn third_replay_move_two_keeps_but_penalizes_rank_two_to_cathy() {
    let fixture = expert_replay_p4v0s2();
    let mut state = fixture.state_at_turn(1).expect("fixture prefix is legal");
    let action = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Rank(Rank::Two),
    };
    let giver_view = state
        .view_for(state.current_player())
        .expect("current player has a view");
    let giver = LogicalDeductions::new(giver_view).expect("logical view");
    state.apply(action).expect("rank 2 is game-legal");
    let recipient_view = state.view_for(PlayerId::new(2)).expect("Cathy has a view");
    let recipient = LogicalDeductions::new(recipient_view).expect("logical view");
    let inferred = infer_h_group(&recipient, HGroupProfile::Max);
    let focus = inferred
        .cards
        .iter()
        .find(|note| note.card == CardId::new(9))
        .expect("Cathy's focused 2 has a convention note");
    assert!(
        focus.identities.contains(Card::new(Suit::Green, Rank::Two)),
        "the clue retains green 2 through the visible green-1 Reverse Finesse routes: {inferred:#?}",
    );
    assert!(
        focus
            .identities
            .contains(Card::new(Suit::Purple, Rank::Two)),
        "the clue also retains purple 2 through Donald's purple 1: {inferred:#?}",
    );
    let candidates = h_group_clue_candidates(&giver, HGroupProfile::Max);
    let cathy = candidates
        .iter()
        .find(|candidate| candidate.action == action)
        .expect("rank 2 to Cathy remains convention-readable");
    let donald = candidates
        .iter()
        .find(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(3),
                    clue: Clue::Rank(Rank::Two),
                }
        })
        .expect("rank 2 to Donald remains a candidate");
    assert!(
        cathy.score() < donald.score(),
        "Cathy retains several delayed branches but acquires no executable action: cathy={cathy:#?}; donald={donald:#?}",
    );
}

#[test]
fn third_replay_opening_rank_two_makes_a_later_green_clue_duplicate() {
    let fixture = expert_replay_p4v0s2();
    let mut state = fixture.state_at_turn(0).expect("fixture prefix is legal");
    for action in [
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(3),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
    ] {
        state.apply(action).expect("hypothetical prefix is legal");
    }
    assert_eq!(state.current_player(), PlayerId::new(0));
    let alice = LogicalDeductions::new(state.view_for(PlayerId::new(0)).expect("Alice has a view"))
        .expect("logical view");
    let duplicate_green = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Suit(Suit::Green),
    };
    let candidates = h_group_clue_candidates(&alice, HGroupProfile::Max);
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.action != duplicate_green),
        "Cathy's opening rank-2 superposition still reserves green 2, so Alice may not independently promise Bob's green 2: {candidates:#?}",
    );
}

#[test]
fn third_replay_rank_one_play_connects_to_rank_two_instead_of_becoming_a_bluff() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(3).expect("fixture prefix is legal");
    let alice = PlayerId::new(0);
    let deductions = LogicalDeductions::new(state.view_for(alice).expect("Alice has a view"))
        .expect("logical view");
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    assert!(
        !inferred
            .signals
            .iter()
            .any(|signal| signal.turn == 1 && signal.kind == HGroupMoveKind::Bluff),
        "any rank 1 connects to a rank-2 clue under Cathy's Connecting Principle: {inferred:#?}",
    );
}

#[test]
fn third_replay_donald_resolves_the_ambiguous_layer_by_playing_green_one() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(3).expect("fixture prefix is legal");
    let donald = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(donald).expect("Donald has a view"))
        .expect("logical view");
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(15))),
        "Cathy's off-suit layer transfers the ambiguous green-1 obligation to Donald",
    );
}
