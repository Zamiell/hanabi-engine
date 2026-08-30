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
        let replay = replay_h_group(&deductions, HGroupProfile::Max);
        let admitted = clue_candidates
            .iter()
            .map(|candidate| candidate.action)
            .collect::<Vec<_>>();
        let rejected =
            h_group_rejected_clues_from_replay(&deductions, HGroupProfile::Max, &replay, &admitted);
        let inferences = infer_h_group(&deductions, HGroupProfile::Max);
        assert_eq!(
            analysis.planner.best_action,
            expected,
            "engine disagrees at move {}; planner candidates: {:#?}; convention candidates: {clue_candidates:#?}; rejected clues: {rejected:#?}; inferences: {inferences:#?}",
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
fn first_replay_move_eight_keeps_a_loaded_clue_in_superposition() {
    let fixture = expert_replay_p4v0s415();
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
    );
}

#[test]
fn first_replay_move_ten_admits_the_direct_rank_three_play_clue() {
    let fixture = expert_replay_p4v0s415();
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
    let fixture = expert_replay_p4v0s415();
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
    assert_expert_replay_matches_engine(&expert_replay_p4v0s9());
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
    assert_expert_replay_matches_engine(&expert_replay_p4v0s2());
}

#[test]
#[ignore = "pending expert review: first unresolved disagreement is move 20"]
fn fourth_expert_replay_matches_engine() {
    assert_expert_replay_matches_engine(&expert_replay_p4v0s3());
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
fn fourth_replay_move_eleven_schedules_the_lie_component_finesse() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(10).expect("fixture prefix is legal");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("Cathy has a view"),
    )
    .expect("valid deductions");
    let view = deductions.view();
    let target = PlayerId::new(1);
    let touched = vec![CardId::new(7), CardId::new(16)];
    let action = Action::Clue {
        target,
        clue: Clue::Rank(Rank::Four),
    };
    let signals = prospective_team_clue_signal_kinds(
        view,
        HGroupProfile::Max,
        target,
        Clue::Rank(Rank::Four),
        &touched,
    );
    assert!(
        signals.contains(&HGroupMoveKind::LieComponentFinesse),
        "rank 4 schedules purple 1, purple 2, a red Fix, purple 3, and purple 4: {signals:?}",
    );
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let lie_candidate = candidates
        .iter()
        .find(|candidate| candidate.action == action)
        .expect("lie candidate exists");
    assert!(
        lie_candidate.can_preempt_ordinary_play(),
        "the multi-step lie line must be allowed to park an ordinary play: {lie_candidate:#?}",
    );
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.action == action),
        "the initiating Lie Component Finesse must be admitted: {candidates:#?}",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(action),
        "the full candidate set was {candidates:#?}",
    );
}

#[test]
fn fourth_replay_move_fifteen_fixes_the_false_red_layer_without_cancelling_the_finesse() {
    let fixture = expert_replay_p4v0s3();
    let before = fixture.state_at_turn(14).expect("fixture prefix is legal");
    let before_deductions = LogicalDeductions::new(
        before
            .view_for(before.current_player())
            .expect("Cathy has a view"),
    )
    .expect("valid deductions");
    let before_replay = replay_h_group(&before_deductions, HGroupProfile::Max);
    assert!(
        before_replay.pending_connections.iter().any(|connection| {
            connection.actor == PlayerId::new(0)
                && connection.expected == Card::new(Suit::Purple, Rank::Three)
                && connection.cards == vec![CardId::new(3), CardId::new(1), CardId::new(0)]
        }),
        "the live connections were {:#?}",
        before_replay.pending_connections,
    );

    let after = fixture.state_at_turn(15).expect("fixture prefix is legal");
    let after_deductions = LogicalDeductions::new(
        after
            .view_for(after.current_player())
            .expect("Donald has a view"),
    )
    .expect("valid deductions");
    let after_replay = replay_h_group(&after_deductions, HGroupProfile::Max);
    assert!(
        after_replay.required_fixes.iter().next().is_none(),
        "the completed lie Fix left residual obligations: {:#?}",
        after_replay.required_fixes,
    );
    assert!(
        after_replay.pending_connections.iter().any(|connection| {
            connection.actor == PlayerId::new(0)
                && connection.expected == Card::new(Suit::Purple, Rank::Three)
                && connection.cards == vec![CardId::new(0)]
        }),
        "the repaired connections were {:#?}",
        after_replay.pending_connections,
    );
}

#[test]
fn fourth_replay_time_travel_chop_move_is_observer_invariant() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(12).expect("fixture prefix is legal");
    for observer in (0..4).map(PlayerId::new) {
        let deductions =
            LogicalDeductions::new(state.view_for(observer).expect("observer has a legal view"))
                .expect("valid deductions");
        let replay = replay_h_group(&deductions, HGroupProfile::Max);
        assert!(
            replay.signals.iter().any(|signal| {
                signal.kind == HGroupMoveKind::TimeTravelChopMove
                    && signal.cards.contains(&CardId::new(15))
                    && signal.cards.contains(&CardId::new(14))
            }),
            "observer {observer:?} did not preserve Donald's Time Travel Chop Move: signals={:#?}",
            replay.signals,
        );
        if observer != PlayerId::new(3) {
            assert!(
                replay.signals.iter().any(|signal| {
                    signal.kind == HGroupMoveKind::LieComponentFinesse
                        && signal.cards.contains(&CardId::new(16))
                }),
                "observer {observer:?} did not recognize the Lie Component Finesse: signals={:#?}",
                replay.signals,
            );
        }
    }
}

#[test]
fn fourth_replay_move_sixteen_continues_with_green_after_the_lie_fix() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(15).expect("fixture prefix is legal");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("Donald has a view"),
    )
    .expect("valid deductions");
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let expected = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Suit(Suit::Green),
    };
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(expected),
        "the post-Fix candidates were {candidates:#?}",
    );
}

#[test]
fn fourth_replay_move_eighteen_transfers_the_promised_purple_four() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(17).expect("fixture prefix is legal");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("Bob has a view"),
    )
    .expect("valid deductions");
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Discard(CardId::new(16))),
        "Bob should transfer his promised purple 4 to Cathy's visible copy: {inferred:#?}",
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

    assert!(candidate(rank_four).action_coverage > candidate(rank_two).action_coverage);
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
    let replay = replay_h_group(&deductions, HGroupProfile::Max);
    let inferred = infer_h_group_from_replay(&deductions, replay.clone(), HGroupProfile::Max);
    let yellow_four = Card::new(Suit::Yellow, Rank::Four);
    let card = inferred
        .cards
        .iter()
        .find(|card| card.card == CardId::new(2))
        .expect("Alice still holds card #2");

    assert_eq!(card.identities, IdentitySet::singleton(yellow_four));
    assert!(inferred.playable_now.contains(&CardId::new(2)));
    assert!(
        replay
            .cards
            .facts
            .declined_alternatives()
            .iter()
            .any(|inference| {
                inference.turn == 30
                    && inference.actor == PlayerId::new(2)
                    && inference.card == CardId::new(2)
                    && inference.identity == yellow_four
                    && inference.chosen
                        == Action::Clue {
                            target: PlayerId::new(0),
                            clue: Clue::Rank(Rank::Two),
                        }
                    && inference.superior
                        == Action::Clue {
                            target: PlayerId::new(1),
                            clue: Clue::Rank(Rank::Four),
                        }
            })
    );
    assert!(
        replay
            .knowledge
            .effects_for(CardId::new(2))
            .any(|effect| matches!(
                effect.source(),
                KnowledgeSource::DeclinedAlternative { turn: 30, .. }
            )),
        "the exact identity retains the declined-alternative provenance",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(2))),
    );
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
                    && pending_is_active(connection, &replay.pending_connections)
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

    assert!(candidate(five).save && !candidate(five).immediate_play());
    assert!(candidate(red).immediate_play() && !candidate(red).save);
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
    assert_eq!(
        (
            candidate(blue).convention_action_count,
            candidate(blue).convention_connection_steps,
        ),
        (Some(2), Some(1)),
        "the Stacked Ejection promises its blind play and the blue 5, not the apparent Finesse chain",
    );
    assert!(
        !candidate(blue).is_urgent_save(),
        "Donald's existing green-1 obligation prevents him from discarding the blue 5",
    );
    assert!(
        candidate(two).score() > candidate(blue).score(),
        "the 3-for-1 Clandestine Finesse must beat the premature 2-for-1 Stacked Ejection: blue={:#?}; two={:#?}",
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
