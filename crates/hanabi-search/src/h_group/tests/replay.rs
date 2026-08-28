use super::*;

fn assert_expert_replay_matches_engine(replay: &HanabiLiveReplay) {
    for turn in 0..u32::try_from(replay.actions.len()).expect("replay fits in u32") {
        let state = replay.state_at_turn(turn).expect("fixture prefix is legal");
        let actor = state.current_player();
        let view = state.view_for(actor).expect("current player has a view");
        let analysis = crate::analyze_position(
            &view,
            crate::SupportedConvention::HGroup(HGroupProfile::Max),
            crate::PlannerConfig {
                objective: crate::PlanningObjective::PerfectScore,
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
        let deductions = LogicalDeductions::new(view).expect("fixture position is logical");
        let clue_candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
        let inferences = infer_h_group(&deductions, HGroupProfile::Max);
        assert_eq!(
            analysis.planner.best_action,
            expected,
            "engine disagrees at move {}; planner candidates: {:#?}; convention candidates: {clue_candidates:#?}; inferences: {inferences:#?}",
            turn + 1,
            analysis.planner.root_actions,
        );
    }
}

/// Golden compatibility oracle: every position in the curated expert replay
/// must select its corresponding optimal action.
#[test]
fn optimized_expert_replay_matches_engine() {
    assert_expert_replay_matches_engine(&expert_replay_p4v0s415());
}

#[test]
fn second_expert_replay_matches_engine() {
    assert_expert_replay_matches_engine(&expert_replay_p4v0s9());
}

#[test]
fn third_expert_replay_matches_engine() {
    assert_expert_replay_matches_engine(&expert_replay_p4v0s2());
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
            candidate(red).convention_connection_steps,
            candidate(two).convention_connection_steps,
        ),
        (Some(1), Some(2)),
        "the Bluff must supply one connector and the Clandestine Finesse two; red signals: {red_signals:?}; rank-2 signals: {two_signals:?}; candidates: {candidates:#?}",
    );
    assert_eq!(
        (
            candidate(red).convention_action_count,
            candidate(two).convention_action_count,
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

    assert_eq!(candidate.purpose, CluePurpose::Advanced);
    assert_eq!(candidate.connection_steps, 0);
    assert_eq!(candidate.convention_connection_steps, Some(1));
    assert_eq!(candidate.convention_action_count, Some(1));
    assert!(
        candidate.score() < 400,
        "a 3 Bluff gets Cathy's immediate blind play but does not promise purple 2 or make Alice's purple 3 playable: {candidate:#?}",
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
