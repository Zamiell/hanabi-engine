use super::*;

/// p4v0s415 move 35: reviewed rank 4 connects Bob's Elimination p2,
/// then his p3, to Alice's p4. Donald must retain that same public line.
#[test]
fn elimination_sequence_is_recognized_by_the_next_observer() {
    let fixture = HanabiLiveReplay::from_json(include_str!(
        "../../../../hanabi-protocol/tests/fixtures/game-p4v0s415.json"
    ))
    .unwrap();
    let state = fixture.state_at_turn(35).unwrap();
    let view = state.view_for(PlayerId::new(3)).unwrap();
    let deductions = LogicalDeductions::new(view).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    let clue = inferred.clues.iter().find(|clue| clue.turn == 34).unwrap();
    assert_eq!(clue.kind, HGroupClueKind::Play);
    let line = clue
        .hypotheses
        .iter()
        .find(|hypothesis| hypothesis.focus_identity == Card::new(Suit::Purple, Rank::Four))
        .expect("rank 4 retains the complete purple connection");
    for (card, rank) in [(6, Rank::Two), (38, Rank::Three)] {
        assert!(
            line.connection_steps.iter().any(|step| {
                step.actor == PlayerId::new(1)
                    && step.cards == [CardId::new(card)]
                    && step.expected == Card::new(Suit::Purple, rank)
            }),
            "{line:#?}"
        );
    }
    assert!(
        !h_group_clue_candidates(&deductions, HGroupProfile::Max)
            .iter()
            .any(|candidate| candidate.action
                == Action::Clue {
                    target: PlayerId::new(1),
                    clue: Clue::Suit(Suit::Purple),
                }),
        "repeating both promised purple cards creates no new play"
    );
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

fn assert_expert_replay_matches_engine(seed: &str, replay: &HanabiLiveReplay) {
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
            "{review}\nengine disagrees at move {}; planner candidates: {:#?}; convention candidates: {clue_candidates:#?}; rejected clues: {rejected:#?}; inferences: {inferences:#?}",
            turn + 1,
            analysis.planner.root_actions,
        );
    }
}

/// The user approved moves through 35. The generated suffix awaits review;
/// it is validated for legality, not frozen as optimal strategy.
#[test]
fn optimized_expert_replay_matches_engine() {
    let mut replay = HanabiLiveReplay::from_json(include_str!(
        "../../../../hanabi-protocol/tests/fixtures/game-p4v0s415.json"
    ))
    .expect("active replay is valid");
    replay.replay().expect("generated continuation is legal");
    replay.actions.truncate(35);
    assert_expert_replay_matches_engine("p4v0s415", &replay);
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
fn fifth_replay_distributes_a_queued_green_four() {
    let fixture = expert_replay_p4v0s1();
    let state = fixture.state_at_turn(46).expect("legal clue");
    for player in [1, 3] {
        let d = LogicalDeductions::new(state.view_for(PlayerId::new(player)).expect("view"))
            .expect("logical");
        let replay = replay_h_group(&d, HGroupProfile::Max);
        assert!(
            replay
                .signals
                .has_at_turn(45, HGroupMoveKind::DistributionClue)
        );
    }
    let state = fixture.state_at_turn(47).expect("green 3 has played");
    let deductions =
        LogicalDeductions::new(state.view_for(PlayerId::new(3)).expect("Donald's view"))
            .expect("valid deductions");
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(41)))
    );
}

#[test]
fn fifth_replay_unnecessary_trash_push_announces_both_plays_at_clue_time() {
    let fixture = expert_replay_p4v0s1();
    let before = fixture.state_at_turn(43).expect("before");
    let d = LogicalDeductions::new(
        before
            .view_for(before.current_player())
            .expect("giver view"),
    )
    .expect("logical view");
    let candidates = h_group_clue_candidates(&d, HGroupProfile::Max);
    let action = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Rank(Rank::One),
    };
    let candidate = candidates
        .iter()
        .find(|candidate| candidate.action == action)
        .expect("admitted unnecessary Trash Push");
    assert_eq!(
        candidate.move_kind(),
        Some(HGroupMoveKind::UnnecessaryIgnition)
    );
    assert!(
        candidate.action_coverage() >= 3,
        "counts the unlocked continuation too"
    );
    assert_eq!(select_h_group_action(&d, HGroupProfile::Max), Some(action));
    let state = fixture.state_at_turn(44).expect("legal clue");
    for (player, card) in [(0, 42), (2, 44)] {
        let d = LogicalDeductions::new(state.view_for(PlayerId::new(player)).expect("view"))
            .expect("logical");
        let inferred = infer_h_group(&d, HGroupProfile::Max);
        assert!(
            inferred.discard_now.is_empty(),
            "known-trash push must not create an Unknown Trash Discharge discard: {inferred:#?}"
        );
        assert!(
            inferred.playable_now.contains(&CardId::new(card)),
            "promised play #{card}: {inferred:#?}"
        );
        if player == 0 {
            assert!(
                !inferred.playable_now.contains(&CardId::new(35)),
                "superseded discharge must not force the trash card: {inferred:#?}"
            );
        }
    }
}

#[test]
fn fifth_replay_draw_distribution_preserves_ambiguous_green_card() {
    let state = expert_replay_p4v0s1()
        .state_at_turn(41)
        .expect("legal prefix");
    let d = LogicalDeductions::new(state.view_for(state.current_player()).expect("Bob view"))
        .expect("logical position");
    let inferred = infer_h_group(&d, HGroupProfile::Max);
    let green = inferred
        .cards
        .iter()
        .find(|card| card.card == CardId::new(19))
        .expect("green note");
    assert_eq!(green.identities.len(), 2);
    assert!(
        green
            .identities
            .contains(Card::new(Suit::Green, Rank::Three))
    );
    assert!(
        green
            .identities
            .contains(Card::new(Suit::Green, Rank::Five))
    );
    assert_eq!(
        select_h_group_action(&d, HGroupProfile::Max),
        Some(Action::Discard(CardId::new(31)))
    );

    let candidates = h_group_clue_candidates(&d, HGroupProfile::Max);
    let no_bonus = |deductions: &LogicalDeductions, notes: &HGroupInferences, profile| {
        assert_eq!(
            super::super::draw_distribution::discard_priority(
                deductions,
                notes,
                profile,
                &candidates,
                CardId::new(31),
            ),
            None
        );
    };
    no_bonus(&d, &inferred, HGroupProfile::Level(HGroupLevel::Level7));
    let mut no_draws = d.view().clone();
    no_draws.deck_size = 0;
    no_bonus(
        &LogicalDeductions::new(no_draws).expect("no-draw view"),
        &inferred,
        HGroupProfile::Max,
    );
    let mut due_play = inferred.clone();
    due_play.playable_now.push(CardId::new(19));
    no_bonus(&d, &due_play, HGroupProfile::Max);
}

#[test]
fn fifth_replay_cathy_draws_for_faster_green_completion() {
    let state = expert_replay_p4v0s1()
        .state_at_turn(42)
        .expect("legal prefix");
    let d = LogicalDeductions::new(state.view_for(state.current_player()).expect("Cathy view"))
        .expect("logical position");
    let notes = infer_h_group(&d, HGroupProfile::Max);
    let times = super::super::draw_distribution::completion_times(
        &d,
        &notes,
        HGroupProfile::Max,
        Card::new(Suit::Green, Rank::Three),
    );
    assert_eq!(
        times,
        Some((7, 11)),
        "conditional green completion respects seating order"
    );
    assert_eq!(
        select_h_group_action(&d, HGroupProfile::Max),
        Some(Action::Discard(CardId::new(26)))
    );
}

#[test]
fn fifth_expert_replay_matches_engine() {
    assert_expert_replay_matches_engine("p4v0s1", &expert_replay_p4v0s1());
}

/// Reviewed p4v0s1 turn 17: purple to Bob Reverse Finesses Donald's purple 2.
/// <https://hanabi.github.io/level-2/#the-reverse-finesse>
#[test]
fn fifth_replay_move_seventeen_admits_purple_reverse_finesse() {
    let fixture = expert_replay_p4v0s1();
    let state = fixture.state_at_turn(16).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let deductions = LogicalDeductions::new(view.clone()).unwrap();
    let replay = replay_h_group(&deductions, HGroupProfile::Max);
    let target = PlayerId::new(1);
    let clue = Clue::Suit(Suit::Purple);
    let focus = CardId::new(5);
    let touched = vec![focus];
    let promptable = replay.promptable();
    let score = crate::h_group::interpretation::delayed_connection_score(
        &view,
        HGroupProfile::Max,
        target,
        focus,
        Card::new(Suit::Purple, Rank::Four),
        false,
        replay.cards.facts.fixed_cards(),
        &promptable,
        &replay.cards.already_playing,
        &replay.pending_connections,
        &replay.cards.facts,
        &replay.cards.chop_moved,
    );
    let primary =
        prospective_clue_primary_interpretation(&view, HGroupProfile::Max, target, clue, &touched);
    let hazard = prospective_clue_hazard(
        &view,
        HGroupProfile::Max,
        target,
        focus,
        clue,
        &touched,
        false,
    );
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    assert!(
        score.is_some(),
        "the ordered Prompt must not be blocked by the older red 3"
    );
    assert_eq!(hazard, None);
    let interpretation = primary
        .as_ref()
        .expect("purple has a recipient interpretation");
    let line = interpretation
        .hypotheses
        .iter()
        .find(|hypothesis| hypothesis.focus_identity == Card::new(Suit::Purple, Rank::Four))
        .expect("the Reverse Finesse reaches Bob's purple 4");
    assert!(line.connection_steps.iter().any(|step| {
        step.actor == PlayerId::new(3)
            && step.cards.contains(&CardId::new(24))
            && step.expected == Card::new(Suit::Purple, Rank::Two)
            && step.kind == HGroupConnectionKind::Finesse
    }));
    assert!(line.connection_steps.iter().any(|step| {
        step.actor == PlayerId::new(3)
            && step.cards.contains(&CardId::new(14))
            && step.expected == Card::new(Suit::Purple, Rank::Three)
            && step.kind == HGroupConnectionKind::Prompt
    }));
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.action == Action::Clue { target, clue }),
        "score={score:?}; primary={primary:#?}; hazard={hazard:?}; candidates={candidates:#?}"
    );
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
fn fifth_replay_move_eleven_uses_permission_to_discard() {
    let fixture = expert_replay_p4v0s1();
    let state = fixture.state_at_turn(10).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Cathy has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Discard(CardId::new(9))),
        "Cathy may discard instead of giving the otherwise-mandatory 5 Stall to Bob",
    );
}

#[test]
fn fifth_replay_move_eighteen_plays_the_five_instead_of_duplicating_purple_four() {
    let fixture = expert_replay_p4v0s1();
    let state = fixture.state_at_turn(17).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Bob has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let purple = Action::Clue {
        target: PlayerId::new(3),
        clue: Clue::Suit(Suit::Purple),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates
            .iter()
            .any(|candidate| candidate.action == purple),
        "Bob already knows his own promised purple 4; a purple clue cannot promise Donald's duplicate purple 4: {candidates:#?}",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(17))),
        "the known blue 5 plays and refunds a clue",
    );
}

#[test]
fn fifth_replay_move_twenty_one_does_not_reclue_an_exact_playable_red_three() {
    let fixture = expert_replay_p4v0s1();
    let state = fixture.state_at_turn(20).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Alice has a view");
    let deductions = LogicalDeductions::new(view).expect("logical view");
    let red = Action::Clue {
        target: PlayerId::new(3),
        clue: Clue::Suit(Suit::Red),
    };
    let green = Clue::Suit(Suit::Green);
    let green_touched = deductions.view().hands[2]
        .iter()
        .filter(|card| {
            card.identity
                .is_some_and(|identity| green.matches(identity))
        })
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let green_primary = prospective_clue_primary_interpretation(
        deductions.view(),
        HGroupProfile::Max,
        PlayerId::new(2),
        green,
        &green_touched,
    );
    let green_hazard = green_primary.as_ref().and_then(|primary| {
        prospective_clue_hazard(
            deductions.view(),
            HGroupProfile::Max,
            PlayerId::new(2),
            primary.focus,
            green,
            &green_touched,
            true,
        )
    });
    let donald_cards =
        subjective_convention_cards(deductions.view(), HGroupProfile::Max, PlayerId::new(3))
            .expect("Donald has a projection");
    assert!(
        donald_cards.iter().any(|card| {
            card.card == CardId::new(13)
                && card.identities == IdentitySet::singleton(Card::new(Suit::Red, Rank::Three))
        }),
        "Donald knows #13 is red 3: {donald_cards:#?}",
    );
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates.iter().any(|candidate| candidate.action == red),
        "Donald already knows his rank-clued card is red 3 and red 3 is playable: {candidates:#?}",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Green),
        }),
        "green_touched={green_touched:?}; green_primary={green_primary:#?}; green_hazard={green_hazard:?}; candidates={candidates:#?}",
    );
}

#[test]
fn fourth_replay_move_thirty_three_keeps_the_blue_one_play_due() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(32).expect("fixture prefix is legal");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("Alice has a view"),
    )
    .expect("valid deductions");
    let replay = replay_h_group(&deductions, HGroupProfile::Max);
    let inferred = infer_h_group_from_replay(&deductions, replay.clone(), HGroupProfile::Max);
    let note = inferred
        .cards
        .iter()
        .find(|note| note.card == CardId::new(32));

    assert!(
        !replay
            .signals
            .at_turn(30, HGroupMoveKind::Bluff)
            .any(|signal| {
                signal.cards.contains(&CardId::new(29)) && signal.cards.contains(&CardId::new(32))
            }),
        "Cathy's promised red-1 play cannot retroactively turn Bob's blue clue into a Bluff",
    );
    assert!(
        !replay
            .signals
            .at_turn(30, HGroupMoveKind::FiveColorEjection)
            .any(|signal| signal.cards.contains(&CardId::new(32))),
        "the same pre-existing red-1 obligation cannot reinterpret Alice's blue 1 as blue 5",
    );

    assert!(
        inferred.playable_now.contains(&CardId::new(32)),
        "Bob's direct blue-1 Play Clue remains due after the promised red 1 plays and Donald discards; note={note:#?}; already_playing={:#?}; forced={:#?}; pending={:#?}; signals={:#?}",
        replay.cards.already_playing,
        replay.cards.forced_playable,
        replay.pending_connections,
        replay.signals,
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(32))),
    );
}

#[test]
fn fourth_replay_move_thirty_four_rejects_redundant_red_clue() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(33).expect("fixture prefix is legal");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("Bob has a view"),
    )
    .expect("valid deductions");
    let redundant_red = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Suit(Suit::Red),
    };
    let false_lie_component = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Suit(Suit::Green),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.action != redundant_red),
        "Alice's red 3 is already scheduled and Good Touch identifies the red 4 after it plays, so repeating red creates no action and fails Minimum Clue Value: {candidates:#?}",
    );
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.action != false_lie_component),
        "a recipient-side hypothetical cannot turn Bob's visibly trash green focus into a Finesse with a Lie Component: {candidates:#?}",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Discard(CardId::new(5))),
        "the discard must beat all remaining clues after enforcing Minimum Clue Value: {candidates:#?}",
    );
}

#[test]
fn fourth_replay_move_thirty_five_prompts_the_directly_clued_red_two_candidate() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(34).expect("fixture prefix is legal");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("Cathy has a view"),
    )
    .expect("valid deductions");
    let replay = replay_h_group(&deductions, HGroupProfile::Max);
    let red_two = Card::new(Suit::Red, Rank::Two);
    let obligations = replay
        .pending_connections
        .iter()
        .filter(|connection| connection.actor == PlayerId::new(2) && connection.expected == red_two)
        .collect::<Vec<_>>();

    assert_eq!(
        obligations.len(),
        1,
        "Cathy should have one live red-2 obligation: {obligations:#?}",
    );
    assert_eq!(
        obligations[0].cards,
        [CardId::new(9), CardId::new(26)],
        "the directly rank-clued #9 must Prompt before the untouched Finesse candidate #26 once the red-2 layer becomes active: {obligations:#?}",
    );
    assert_eq!(
        h_group_predictable_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(9))),
        "the selected red 2 must play before any released alternative",
    );
}

#[test]
fn fourth_replay_ordinary_trash_discard_does_not_push_bobs_blue_two() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(35).expect("fixture prefix is legal");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("Donald has a view"),
    )
    .expect("valid deductions");
    let replay = replay_h_group(&deductions, HGroupProfile::Max);
    let blue_to_bob = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Suit(Suit::Blue),
    };

    assert!(
        !replay
            .signals
            .at_turn(28, HGroupMoveKind::TrashPush)
            .any(|signal| signal.cards.contains(&CardId::new(30))),
        "Alice's ordinary trash discard cannot push Bob's blue 2: {:#?}",
        replay.signals,
    );
    assert!(
        !replay
            .signals
            .at_turn(32, HGroupMoveKind::PatchFinesse)
            .any(|signal| signal.cards.contains(&CardId::new(30))),
        "the later blue-1 play cannot patch a connection that never existed: {:#?}",
        replay.signals,
    );
    assert!(
        replay
            .pending_connections
            .iter()
            .all(|connection| !connection.cards.contains(&CardId::new(30))),
        "Bob's blue 2 must not retain a promise fabricated from the discard: {:#?}",
        replay.pending_connections,
    );
    assert!(
        h_group_clue_candidates(&deductions, HGroupProfile::Max)
            .iter()
            .any(|candidate| candidate.action == blue_to_bob),
        "blue to Bob is an ordinary direct Play Clue once blue 1 is down",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(blue_to_bob),
        "the direct blue-2 Play Clue must beat unrelated alternatives once the false promise is removed",
    );
}

#[test]
fn fourth_replay_accounted_rank_fours_are_a_valid_play_clue() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(43).expect("fixture prefix is legal");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("Donald has a view"),
    )
    .expect("valid deductions");
    let rank_four = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Rank(Rank::Four),
    };

    assert!(
        h_group_clue_candidates(&deductions, HGroupProfile::Max)
            .iter()
            .any(|candidate| candidate.action == rank_four),
        "touching every copy accounts for the duplicate blue 4 rather than creating a later Good Touch play",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(rank_four),
    );
}

#[test]
fn fourth_replay_accounted_rank_clue_plays_its_focus_first() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(44).expect("fixture clue is legal");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("Alice has a view"),
    )
    .expect("valid deductions");
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert!(inferred.playable_now.contains(&CardId::new(22)));
    assert!(inferred.playable_now.contains(&CardId::new(35)));
    assert!(
        inferred
            .cards
            .iter()
            .any(|card| card.card == CardId::new(35) && card.focused),
        "the newly rank-clued card remains the transient focus",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(35))),
        "the newly focused 4 must resolve before the older card made playable by elimination",
    );
}

#[test]
fn fourth_replay_accounted_duplicate_leaves_the_other_suit_playable() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(48).expect("fixture prefix is legal");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("Alice has a view"),
    )
    .expect("valid deductions");
    let replay = replay_h_group(&deductions, HGroupProfile::Max);
    let inferred = infer_h_group_from_replay(&deductions, replay.clone(), HGroupProfile::Max);

    assert_eq!(
        deductions.possible_identities(CardId::new(3)),
        Some(IdentitySet::singleton(Card::new(Suit::Red, Rank::Four))),
    );
    assert!(
        inferred.playable_now.contains(&CardId::new(3)),
        "playing the focused blue 4 makes the older duplicate blue 4 trash, but cannot erase the directly known red 4: card={:#?}; playable={:#?}; invalidated={:#?}; declined={:#?}",
        inferred
            .cards
            .iter()
            .find(|card| card.card == CardId::new(3)),
        inferred.playable_now,
        replay.cards.invalidated_focuses,
        replay.cards.declined_direct_plays,
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(3))),
    );
}

#[test]
fn fourth_replay_final_rank_one_is_a_trash_double_ignition() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(49).expect("fixture prefix is legal");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("Bob has a view"),
    )
    .expect("valid deductions");
    let replay = replay_h_group(&deductions, HGroupProfile::Max);
    let gotten = replay.gotten_from(&replay.promptable());
    let rank_one = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Rank(Rank::One),
    };
    let touched = deductions.view().hands[0]
        .iter()
        .filter(|card| {
            card.identity
                .is_some_and(|identity| Clue::Rank(Rank::One).matches(identity))
        })
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let signals = prospective_team_clue_signal_kinds(
        deductions.view(),
        HGroupProfile::Max,
        PlayerId::new(0),
        Clue::Rank(Rank::One),
        &touched,
    );

    assert!(
        candidates.iter().any(|candidate| {
            candidate.action == rank_one
                && candidate.move_kind() == Some(HGroupMoveKind::TrashDoubleIgnition)
                && candidate.action_coverage() == 2
        }),
        "rank 1 on Alice's accounted trash must ignite Cathy's red 5 and Donald's green 5: touched={touched:?}; gotten={:?}; chop_moved={:?}; signals={signals:?}; candidates={candidates:#?}",
        gotten,
        replay.cards.chop_moved,
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(rank_one),
        "Trash Double Ignition should outrank either direct one-for-one 5 clue: {candidates:#?}",
    );
}

#[test]
fn fourth_replay_first_ignition_play_has_consistent_beliefs() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(50).expect("fixture prefix is legal");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("Cathy has a view"),
    )
    .expect("valid deductions");
    let decision = analyze_h_group_convention(&deductions, HGroupProfile::Max);
    let result = crate::analyze_position(
        deductions.view(),
        crate::SupportedConvention::HGroup(HGroupProfile::Max),
        crate::PlannerConfig::default(),
    );

    let analysis = result.unwrap_or_else(|error| {
        panic!(
            "the TDI must not leave contradictory ordinary-clue constraints: error={error:?}; constraints={:#?}; inferences={:#?}",
            decision.belief_constraints, decision.inferences,
        )
    });
    assert_eq!(analysis.planner.best_action, Action::Play(CardId::new(45)));
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
fn fourth_replay_useful_yellow_two_prevents_time_travel_chop_move() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(12).expect("fixture prefix is legal");
    for observer in (0..4).map(PlayerId::new) {
        let deductions =
            LogicalDeductions::new(state.view_for(observer).expect("observer has a legal view"))
                .expect("valid deductions");
        let replay = replay_h_group(&deductions, HGroupProfile::Max);
        assert!(
            !replay.signals.iter().any(|signal| {
                signal.kind == HGroupMoveKind::TimeTravelChopMove
                    && signal.cards.contains(&CardId::new(15))
            }),
            "observer {observer:?} incorrectly treated Donald's useful yellow clue as a Time Travel Chop Move: signals={:#?}",
            replay.signals,
        );
        assert!(
            !replay.cards.chop_moved.contains(&CardId::new(12))
                && !replay.cards.chop_moved.contains(&CardId::new(14)),
            "observer {observer:?} incorrectly chop-moved cards behind Donald's yellow 4: {:#?}",
            replay.cards.chop_moved,
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
fn fourth_replay_move_twenty_rejects_the_redundant_rank_five_fill_in() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(19).expect("fixture prefix is legal");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("Donald has a view"),
    )
    .expect("valid deductions");
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let redundant = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Rank(Rank::Five),
    };
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.action != redundant),
        "Cathy already knows her yellow 5 is a delayed Play promise: {candidates:#?}",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        }),
    );
}

#[test]
fn fourth_replay_move_twenty_three_plays_the_five_for_clue_recovery() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(22).expect("fixture prefix is legal");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("Cathy has a view"),
    )
    .expect("valid deductions");
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    let ordered = ordered_playable_cards(deductions.view(), &inferred, HGroupProfile::Max);

    assert_eq!(
        ordered.first().copied(),
        Some(CardId::new(8)),
        "Level 25 gives the known playable yellow 5 Priority over an ordinary purple 4 because the 5 recovers a clue; ordered={ordered:?}; inferred={inferred:#?}",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(8))),
    );
}

#[test]
fn fourth_replay_move_twenty_four_admits_the_red_double_reverse_finesse() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(23).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Donald has a view");
    let deductions = LogicalDeductions::new(view.clone()).expect("valid deductions");
    let expected = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Rank(Rank::Three),
    };
    let touched = vec![CardId::new(1)];
    let after = prospective_clue_view(
        deductions.view(),
        PlayerId::new(0),
        Clue::Rank(Rank::Three),
        &touched,
    );
    let (_, recipient_replay) =
        projected_h_group_replay(&after, HGroupProfile::Max, PlayerId::new(0))
            .expect("recipient projection succeeds");
    let red_one = Card::new(Suit::Red, Rank::One);
    let red_two = Card::new(Suit::Red, Rank::Two);
    assert!(
        recipient_replay
            .pending_connections
            .iter()
            .any(|connection| {
                connection.actor == PlayerId::new(2)
                    && connection.focus == CardId::new(1)
                    && connection.expected == red_one
            })
    );
    assert!(
        recipient_replay
            .pending_connections
            .iter()
            .any(|connection| {
                connection.actor == PlayerId::new(2)
                    && connection.focus == CardId::new(1)
                    && connection.expected == red_two
            })
    );
    assert!(!recipient_replay.signals.iter().any(|signal| {
        signal.turn == 23
            && matches!(
                signal.kind,
                HGroupMoveKind::Bluff | HGroupMoveKind::ThreeBluff
            )
    }));
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.action == expected),
        "rank 3 to Alice should Reverse-Finesse Cathy's red 1 and red 2 for Alice's red 3: candidates={candidates:#?}",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(expected),
    );
}

#[test]
fn fourth_replay_move_twenty_five_does_not_reclue_the_promised_red_chain() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(24).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Alice has a view");
    let deductions = LogicalDeductions::new(view).expect("valid deductions");
    let redundant_red = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Suit(Suit::Red),
    };
    let expected = Action::Clue {
        target: PlayerId::new(3),
        clue: Clue::Rank(Rank::Five),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.action != redundant_red),
        "Cathy's active red 1 and red 2 are already promised by the Double Reverse Finesse: candidates={candidates:#?}",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(expected),
        "move 25 candidates={candidates:#?}",
    );
}

#[test]
fn fourth_replay_move_twenty_eight_continues_the_red_chain_with_rank_two() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(27).expect("fixture prefix is legal");
    let view = state
        .view_for(state.current_player())
        .expect("Donald has a view");
    let deductions = LogicalDeductions::new(view).expect("valid deductions");
    let expected = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Rank(Rank::Two),
    };
    let touched = vec![CardId::new(9)];
    let signals = prospective_team_clue_signal_kinds(
        deductions.view(),
        HGroupProfile::Max,
        PlayerId::new(2),
        Clue::Rank(Rank::Two),
        &touched,
    );
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(signals.contains(&HGroupMoveKind::ContinuationClue));
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.action == expected),
        "rank 2 should continue Cathy's red-1/red-2 chain: signals={signals:?}; candidates={candidates:#?}",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(expected),
    );
}

#[test]
fn fourth_replay_unrelated_blue_clue_preserves_cathys_red_chain() {
    let fixture = expert_replay_p4v0s3();
    let state = fixture.state_at_turn(30).expect("fixture prefix is legal");
    let view = state.view_for(PlayerId::new(2)).expect("Cathy has a view");
    let deductions = LogicalDeductions::new(view).expect("Cathy's view is logical");
    let replay = replay_h_group(&deductions, HGroupProfile::Max);
    let inferred = infer_h_group_from_replay(&deductions, replay.clone(), HGroupProfile::Max);
    let red_one = inferred
        .cards
        .iter()
        .find(|card| card.card == CardId::new(29))
        .expect("Cathy still holds the promised red 1");

    assert_eq!(
        red_one.promised_identity,
        Some(Card::new(Suit::Red, Rank::One)),
        "an unrelated clue to Alice cannot rewrite Cathy's established red-1 connection: transitions={:#?}; replay={replay:#?}",
        replay.pending_connections.transitions(),
    );
    assert!(
        inferred.playable_now.contains(&CardId::new(29)),
        "Cathy must continue the red chain by playing red 1: {inferred:#?}",
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

    assert_eq!(candidate.purpose(), CluePurpose::Advanced);
    assert_eq!(candidate.connection_steps(), 0);
    assert_eq!(candidate.convention_connection_steps(), Some(1));
    assert_eq!(candidate.convention_action_count(), Some(1));
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
