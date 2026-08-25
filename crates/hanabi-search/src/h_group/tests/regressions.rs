#[test]
fn paired_sample_zero_does_not_blind_play_a_fresh_red_five() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(1)),
        Action::Play(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Play(CardId::new(2)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(19))),
        "fresh red 5 was falsely forced playable: {inferred:#?}"
    );
}

#[test]
fn paired_sample_one_rank_one_continuation_does_not_misplay() {
    let mut state = paired_sample_one_state();
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        })
        .unwrap();
    let convention = crate::SupportedConvention::HGroup(HGroupProfile::Max);
    let report = continuation_for_search(state.clone(), convention).unwrap();

    for (index, action) in report.outcome.actions().iter().enumerate() {
        if let Action::Play(card) = action {
            let identity = state.card(*card).unwrap();
            if identity.rank.number()
                != u8::try_from(state.play_stacks()[identity.suit.index()].len()).unwrap() + 1
            {
                let actor = LogicalDeductions::new(state.view_for(state.current_player()).unwrap())
                    .unwrap();
                panic!(
                    "continuation action {index}, turn {}, actor {:?}: played {identity:?} when its stack was not ready: action {action:?}; prior actions {:?}; inference {:#?}",
                    state.turn(),
                    state.current_player(),
                    &report.outcome.actions()[..index],
                    infer_h_group(&actor, HGroupProfile::Max)
                );
            }
        }
        state.apply(*action).unwrap();
    }
}

#[test]
fn paired_sample_zero_green_four_save_does_not_finesse_a_trash_blue_one() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(1)),
        Action::Play(CardId::new(7)),
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(12)),
        Action::Play(CardId::new(21)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Discard(CardId::new(18)),
        Action::Play(CardId::new(5)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(24)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(27)),
        Action::Discard(CardId::new(26)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(25)),
    ] {
        state.apply(action).unwrap();
    }
    state
        .apply(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        })
        .unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(23))),
        "critical green-4 Save created a false green-2 finesse: {inferred:#?}"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn paired_sample_zero_ordinary_chop_does_not_block_a_purple_three_save() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(1)),
        Action::Play(CardId::new(7)),
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(12)),
        Action::Play(CardId::new(21)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Discard(CardId::new(18)),
        Action::Play(CardId::new(5)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(24)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(27)),
        Action::Discard(CardId::new(26)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(25)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Discard(CardId::new(15)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(9)),
        Action::Play(CardId::new(16)),
        Action::Discard(CardId::new(31)),
    ] {
        state.apply(action).unwrap();
    }
    let save_clue = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Suit(Suit::Purple),
    };
    let giver_deductions =
        LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&giver_deductions, HGroupProfile::Max);
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.action == save_clue && candidate.save),
        "the ordinary chop discard was falsely promoted to an Emergency Discard: {candidates:#?}"
    );
    state.apply(save_clue).unwrap();
    for player in [PlayerId::new(0), PlayerId::new(2)] {
        let deductions = LogicalDeductions::new(state.view_for(player).unwrap()).unwrap();
        let inferred = infer_h_group(&deductions, HGroupProfile::Max);
        assert!(
            inferred.playable_now.is_empty() && inferred.connection.is_none(),
            "purple Save must not create a play for {player:?}: {inferred:#?}"
        );
    }
}

#[test]
fn paired_sample_zero_plays_saved_red_two_before_red_three() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(1)),
        Action::Play(CardId::new(7)),
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(22)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(21)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(12)),
        Action::Play(CardId::new(17)),
        Action::Discard(CardId::new(19)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(27)),
        Action::Discard(CardId::new(24)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(18)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(20)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(5))),
        "Red-4 connection skipped the saved Red 2: {inferred:#?}"
    );
}

#[test]
fn paired_sample_zero_does_not_play_red_four_before_red_three() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(1)),
        Action::Play(CardId::new(7)),
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(22)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(21)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(12)),
        Action::Play(CardId::new(17)),
        Action::Discard(CardId::new(19)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(27)),
        Action::Discard(CardId::new(24)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(18)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(20)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(5)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(16))),
        "Red 4 was played before the missing Red 3: {inferred:#?}"
    );
}

#[test]
fn paired_sample_zero_rejects_a_purple_two_save_that_causes_false_anxiety() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(1)),
        Action::Play(CardId::new(7)),
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(12)),
        Action::Play(CardId::new(21)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Discard(CardId::new(18)),
        Action::Play(CardId::new(5)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(24)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(27)),
        Action::Discard(CardId::new(26)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(25)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Discard(CardId::new(15)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(28)),
        Action::Discard(CardId::new(16)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Purple),
        }),
        "critical purple-2 Save would cause false Anxiety: {candidates:#?}"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn paired_sample_one_clues_the_playable_red_four_before_it_is_discarded() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(7)),
        Action::Play(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(5)),
        Action::Discard(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(9)),
        Action::Discard(CardId::new(21)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let selected = select_h_group_action(&deductions, HGroupProfile::Max);

    assert!(
        matches!(
            selected,
            Some(Action::Clue {
                target,
                clue,
            }) if target == PlayerId::new(2)
                && (clue == Clue::Suit(Suit::Red) || clue == Clue::Rank(Rank::Four))
        ),
        "playable red 4 on chop was not protected: selected={selected:?}; hand={:#?}; inference={:#?}; candidates={:#?}; hazard={:?}",
        deductions.view().hands[2],
        infer_h_group(&deductions, HGroupProfile::Max),
        h_group_clue_candidates(&deductions, HGroupProfile::Max),
        prospective_clue_hazard(
            deductions.view(),
            HGroupProfile::Max,
            PlayerId::new(2),
            CardId::new(23),
            Clue::Suit(Suit::Red),
            &[CardId::new(23)],
            true,
        ),
    );
    state
        .apply(selected.expect("a red-4 clue was selected"))
        .unwrap();
    for action in [
        Action::Discard(CardId::new(8)),
        Action::Play(CardId::new(23)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(22)),
        Action::Discard(CardId::new(27)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Two),
        }),
        "trash yellow 2 was incorrectly saved: {candidates:#?}"
    );
}

#[test]
fn saved_red_two_survives_the_corrected_rank_two_continuation() {
    let convention = crate::SupportedConvention::HGroup(HGroupProfile::Max);
    let mut state = paired_sample_five_state();
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        })
        .unwrap();
    let report = continuation_for_search(state, convention).unwrap();

    assert!(
        !report
            .outcome
            .actions()
            .contains(&Action::Discard(CardId::new(5))),
        "saved red 2 was discarded: {:?}",
        report.outcome.actions()
    );
}

#[test]
fn saved_red_two_is_held_until_another_red_two_plays() {
    let convention = crate::SupportedConvention::HGroup(HGroupProfile::Max);
    let mut state = paired_sample_two_state();
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        })
        .unwrap();
    let report = continuation_for_search(state, convention).unwrap();
    if let Some(index) = report
        .outcome
        .actions()
        .iter()
        .position(|action| *action == Action::Discard(CardId::new(5)))
    {
        let mut before = paired_sample_two_state();
        before
            .apply(Action::Clue {
                target: PlayerId::new(1),
                clue: Clue::Rank(Rank::Two),
            })
            .unwrap();
        for action in &report.outcome.actions()[..index] {
            before.apply(*action).unwrap();
        }
        assert!(
            before.play_stacks()[Suit::Red.index()].len() >= usize::from(Rank::Two.number()),
            "saved red 2 was discarded before the team played a red 2: {:?}",
            report.outcome.actions()
        );
    }
}

#[test]
fn paired_sample_three_rank_four_clue_occupies_green_five_holder() {
    let mut state = paired_sample_three_state();
    for (index, action) in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(17)),
        Action::Play(CardId::new(7)),
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(8)),
        Action::Discard(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Discard(CardId::new(22)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Discard(CardId::new(3)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Discard(CardId::new(15)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(24)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Discard(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Four),
        },
    ]
    .into_iter()
    .enumerate()
    {
        state
            .apply(action)
            .unwrap_or_else(|error| panic!("action {index} {action:?}: {error:?}"));
    }
    let player = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(player).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Discard(CardId::new(21))),
        "player: {player:?}; candidates: {:#?}; clues: {:#?}; inference: {inferred:#?}",
        ordered_h_group_actions(&deductions, HGroupProfile::Max),
        h_group_clue_candidates(&deductions, HGroupProfile::Max)
    );
}

/// <https://hanabi.github.io/level-7/#the-scream-discard-chop-move-sdcm>
#[test]
fn paired_sample_three_does_not_discard_the_unique_yellow_five() {
    let profile = HGroupProfile::Level(HGroupLevel::Level25);
    let mut state = paired_sample_three_state();
    for (index, action) in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(17)),
        Action::Play(CardId::new(7)),
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(8)),
        Action::Discard(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Discard(CardId::new(18)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Discard(CardId::new(3)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Play(CardId::new(23)),
        Action::Discard(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Discard(CardId::new(15)),
        Action::Play(CardId::new(25)),
        Action::Play(CardId::new(9)),
    ]
    .into_iter()
    .enumerate()
    {
        state
            .apply(action)
            .unwrap_or_else(|error| panic!("action {index} {action:?}: {error:?}"));
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, profile);
    assert!(inferred.signals.iter().any(|signal| {
        signal.turn == 19
            && signal.kind == HGroupMoveKind::ScreamDiscard
            && signal.cards == [CardId::new(15)]
    }), "expected Level-7 Scream after applying Max extras: {inferred:#?}");

    assert_eq!(
        select_h_group_action(&deductions, profile),
        Some(Action::Play(CardId::new(28))),
        "the demonstrated Priority play preempts the later save clue while the Level-7 Scream still protects the unique yellow 5: {inferred:#?}; clues: {:#?}",
        h_group_clue_candidates(&deductions, profile),
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn paired_sample_three_does_not_play_yellow_four_before_yellow_three() {
    let mut state = paired_sample_three_state();
    for (turn, action) in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(6)),
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(8)),
        Action::Play(CardId::new(13)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(5)),
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(9)),
        Action::Play(CardId::new(20)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(0)),
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(17)),
        Action::Play(CardId::new(16)),
        Action::Discard(CardId::new(23)),
        Action::Play(CardId::new(15)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(3)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(24)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(30)),
        Action::Play(CardId::new(32)),
        Action::Discard(CardId::new(18)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(35)),
        Action::Play(CardId::new(28)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Discard(CardId::new(36)),
        Action::Play(CardId::new(2)),
    ]
    .into_iter()
    .enumerate()
    {
        if turn == 36 {
            let giver = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
            let receiver =
                LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
            assert_eq!(
                h_group_clue_candidates(&giver, HGroupProfile::Max)
                    .iter()
                    .find(|candidate| candidate.action == action)
                    .map(|candidate| candidate.save),
                Some(true),
                "yellow 4 should be given as a Critical Save"
            );
            assert_eq!(
                infer_h_group(&receiver, HGroupProfile::Max).chops[1],
                Some(CardId::new(26)),
                "public chop must agree before the clue: {:#?}",
                infer_h_group(&receiver, HGroupProfile::Max)
            );
        }
        state.apply(action).unwrap();
        if turn == 36 {
            let receiver =
                LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
            let clue = infer_h_group(&receiver, HGroupProfile::Max)
                .clues
                .into_iter()
                .last()
                .unwrap();
            assert!(clue.focus_was_chop && !clue.save_identities.is_empty());
        }
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    let saved_clue = inferred.clues.iter().find(|clue| clue.turn == 36).unwrap();

    assert!(saved_clue.focus_was_chop && !saved_clue.save_identities.is_empty());

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(26))),
        "yellow 4 cannot play before yellow 3: {inferred:#?}"
    );
}

#[test]
fn paired_sample_eight_clears_the_layered_finesse_after_the_red_three_prompt() {
    let mut state = paired_sample_eight_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(6)),
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(8)),
        Action::Play(CardId::new(13)),
        Action::Discard(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(2)),
        Action::Play(CardId::new(5)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Play(CardId::new(9)),
    ] {
        state.apply(action).unwrap();
    }
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Green),
        })
        .unwrap();
    let actor = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&actor, HGroupProfile::Max);

    assert_ne!(
        h_group_predictable_action(&actor, HGroupProfile::Max),
        Some(Action::Play(CardId::new(21))),
        "the successful red 3 Prompt must clear the false layered alternative: {inferred:#?}"
    );
}

#[test]
fn paired_sample_eight_rejects_rank_three_with_a_false_red_two_prompt() {
    let mut state = paired_sample_eight_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(6)),
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(8)),
        Action::Play(CardId::new(13)),
        Action::Discard(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(19)),
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
    ] {
        state.apply(action).unwrap();
    }
    let giver = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let bad_clue = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Rank(Rank::Three),
    };
    let touched = vec![CardId::new(9)];

    let candidates = h_group_clue_candidates(&giver, HGroupProfile::Max);
    assert!(
        !candidates
            .iter()
            .any(|candidate| candidate.action == bad_clue),
        "rank 3 would Prompt the newer red-clued red 4 before the saved red 2; candidates={candidates:#?}; hazard={:?}",
        prospective_clue_hazard(
            giver.view(),
            HGroupProfile::Max,
            PlayerId::new(1),
            CardId::new(9),
            Clue::Rank(Rank::Three),
            &touched,
            true,
        )
    );
}

#[test]
fn paired_sample_eleven_rejects_rank_two_after_an_ordinary_discard() {
    let mut state = paired_sample_eleven_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(13)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(8)),
        Action::Discard(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(5)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Discard(CardId::new(1)),
        Action::Play(CardId::new(19)),
        Action::Discard(CardId::new(11)),
        Action::Discard(CardId::new(3)),
        Action::Play(CardId::new(9)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
    ] {
        state.apply(action).unwrap();
    }
    let giver = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let clue = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Rank(Rank::Two),
    };
    let candidates = h_group_clue_candidates(&giver, HGroupProfile::Max);
    assert!(
        !candidates.iter().any(|candidate| candidate.action == clue),
        "the giver invented an Emergency Discard that the actor saw as ordinary: {candidates:#?}"
    );
    state.apply(clue).unwrap();
    let recipient = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(&recipient, HGroupProfile::Max);
    let interpretation = inferred.clues.last().unwrap();
    assert!(
        !interpretation.focus_was_chop && interpretation.save_identities.is_empty(),
        "the recipient incorrectly treated the unsafe clue as a 2 Save: {interpretation:#?}"
    );
}

#[test]
fn paired_sample_twelve_rejects_a_rank_one_that_can_duplicate_own_focus() {
    let mut state = paired_sample_twelve_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
    ] {
        state.apply(action).unwrap();
    }
    let actor = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let bad_clue = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Rank(Rank::One),
    };

    assert!(
        !h_group_clue_candidates(&actor, HGroupProfile::Max)
            .iter()
            .any(|candidate| candidate.action == bad_clue),
        "the visible yellow 1 overlaps the giver's unresolved clued 1: {:#?}",
        infer_h_group(&actor, HGroupProfile::Max)
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn paired_sample_one_saves_the_remaining_purple_three() {
    let mut state = paired_sample_one_state();
    let actions = [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(12)),
        Action::Discard(CardId::new(18)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(21)),
        Action::Play(CardId::new(22)),
        Action::Play(CardId::new(5)),
        Action::Play(CardId::new(16)),
        Action::Discard(CardId::new(20)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Discard(CardId::new(15)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(29)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(30)),
        Action::Play(CardId::new(32)),
        Action::Discard(CardId::new(8)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(0)),
        Action::Play(CardId::new(9)),
        Action::Play(CardId::new(23)),
        Action::Discard(CardId::new(34)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(25)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Discard(CardId::new(27)),
        Action::Play(CardId::new(40)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(31)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(36)),
    ];
    for (index, action) in actions.into_iter().enumerate() {
        state
            .apply(action)
            .unwrap_or_else(|error| panic!("action {index} {action:?}: {error:?}"));
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(35))),
        "inference: {inferred:#?}; clues: {:#?}; actions: {:#?}",
        h_group_clue_candidates(&deductions, HGroupProfile::Max),
        ordered_h_group_actions(&deductions, HGroupProfile::Max),
    );
}

#[test]
fn paired_sample_one_rejects_a_rank_two_clue_that_duplicates_saved_red_two() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(2)),
        Action::Play(CardId::new(8)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(0)),
        Action::Play(CardId::new(22)),
        Action::Discard(CardId::new(12)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let bad_clue = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Rank(Rank::Two),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates
            .iter()
            .any(|candidate| candidate.action == bad_clue),
        "Good Touch allowed a duplicate Red 2 clue: {candidates:#?}"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn paired_sample_one_does_not_discard_the_last_blue_four_off_chop() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(0)),
        Action::Play(CardId::new(8)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Discard(CardId::new(12)),
        Action::Play(CardId::new(22)),
        Action::Play(CardId::new(5)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(26)),
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(28)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(16)),
        Action::Discard(CardId::new(30)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(10)),
        Action::Play(CardId::new(18)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(15)),
        Action::Play(CardId::new(20)),
        Action::Discard(CardId::new(9)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Discard(CardId::new(34)),
        Action::Play(CardId::new(19)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(36)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(25)),
        Action::Play(CardId::new(3)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(35)),
        Action::Play(CardId::new(32)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Discard(CardId::new(23)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(21)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Discard(CardId::new(40)),
        Action::Play(CardId::new(46)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Discard(CardId::new(37)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let selected = select_h_group_action(&deductions, HGroupProfile::Max);
    state
        .apply(selected.expect("the clue giver has an action"))
        .unwrap();
    let actor = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&actor, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&actor, HGroupProfile::Max),
        Some(Action::Discard(CardId::new(47))),
        "the last Blue 4 was discarded off chop: chops={:?}; moved={:?}; invisible={:?}; discard_now={:?}; note={:?}; hand={:#?}; tokens={}; candidates={:#?}",
        inferred.chops,
        inferred.chop_moved,
        inferred.invisibly_clued,
        inferred.discard_now,
        inferred
            .cards
            .iter()
            .find(|card| card.card == CardId::new(47)),
        actor.view().hands[0],
        actor.view().clue_tokens,
        ordered_h_group_actions(&actor, HGroupProfile::Max),
    );
}

#[test]
fn paired_sample_two_rejects_a_discharge_whose_focus_is_not_known_trash() {
    let mut state = paired_sample_two_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(8)),
        Action::Play(CardId::new(13)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(5)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(19)),
        Action::Play(CardId::new(21)),
        Action::Discard(CardId::new(3)),
        Action::Play(CardId::new(9)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(26)),
        Action::Discard(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(28)),
        Action::Discard(CardId::new(24)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(20)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(29)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(0),
                    clue: Clue::Suit(Suit::Red),
                }
        }),
        "the giver called Red 2/R5 an Unknown Trash Discharge even though the recipient could not know that Red 2 was the focus: {candidates:#?}"
    );
}

/// <https://hanabi.github.io/level-17/#the-time-travel-chop-move-direct-form>
#[test]
fn paired_sample_four_discards_instead_of_recluing_a_trash_blue_one() {
    let mut state = paired_sample_four_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(14)),
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    assert!(inferred.signals.iter().any(|signal| {
        signal.kind == HGroupMoveKind::TimeTravelChopMove
            && signal.cards.starts_with(&[CardId::new(4), CardId::new(3)])
    }));

    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Discard(CardId::new(15))),
        "the earlier Time Travel Chop Move changes the physical chop; the policy must recover a token from that new chop: {inferred:#?}; candidates={:#?}",
        h_group_clue_candidates(&deductions, HGroupProfile::Max),
    );
}

#[test]
fn paired_sample_five_treats_a_number_two_on_chop_as_a_save() {
    let mut state = paired_sample_five_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(3)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(15)),
        Action::Discard(CardId::new(8)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(19)),
        Action::Play(CardId::new(6)),
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Discard(CardId::new(22))),
        "the number 2 clue on chop was incorrectly treated as a direct play: {inferred:#?}"
    );
    assert!(inferred.saved.contains(&CardId::new(20)));
}

#[test]
fn paired_sample_five_does_not_play_purple_three_before_purple_two() {
    let mut state = paired_sample_five_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(3)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(15)),
        Action::Discard(CardId::new(8)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(19)),
        Action::Play(CardId::new(6)),
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Discard(CardId::new(22)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(18))),
        "Purple 3 was forced before the saved Purple 2: {inferred:#?}"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn paired_sample_one_holds_saved_blue_four_until_blue_three_plays() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(0)),
        Action::Play(CardId::new(8)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Discard(CardId::new(12)),
        Action::Play(CardId::new(22)),
        Action::Play(CardId::new(5)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(26)),
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(28)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(16)),
        Action::Discard(CardId::new(30)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(10)),
        Action::Play(CardId::new(18)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(15)),
        Action::Play(CardId::new(20)),
        Action::Discard(CardId::new(9)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Discard(CardId::new(34)),
        Action::Play(CardId::new(19)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(36)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(25)),
        Action::Play(CardId::new(3)),
        Action::Discard(CardId::new(27)),
        Action::Play(CardId::new(35)),
        Action::Discard(CardId::new(38)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Discard(CardId::new(23)),
        Action::Play(CardId::new(32)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(46)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(47))),
        "the critical Blue 4 Save was played before Blue 3: {inferred:#?}"
    );
}

#[test]
fn paired_sample_four_plays_red_two_before_red_five() {
    let mut state = paired_sample_four_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(14)),
        Action::Play(CardId::new(0)),
        Action::Discard(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(1)),
        Action::Play(CardId::new(8)),
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(16)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(7)),
        Action::Discard(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(5))),
        "the delayed Red 5 focus bypassed its Red 2 connector: {inferred:#?}"
    );
}

#[test]
fn unknown_chop_moved_card_does_not_trigger_another_emergency_discard() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Discard(CardId::new(2)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    assert!(
        !inferred.chop_moved.contains(&CardId::new(8)),
        "a merely chop-moved Green 2 was treated as known playable: {inferred:#?}"
    );
}

#[test]
fn play_that_unlocks_a_saved_card_preempts_an_unrelated_play_clue() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Discard(CardId::new(2)),
        Action::Play(CardId::new(22)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();

    let selected = select_h_group_action(&deductions, HGroupProfile::Max);
    assert!(
        matches!(
            selected,
            Some(Action::Clue {
                target,
                clue: Clue::Rank(Rank::One) | Clue::Suit(Suit::Red),
            }) if target == PlayerId::new(1)
        ),
        "the intervening Green 2 play would expose Red 1 on chop: {selected:?}; hand={:#?}; inference={:#?}; clues={:#?}; rank1_hazard={:?}",
        deductions.view().hands[1],
        infer_h_group(&deductions, HGroupProfile::Max),
        h_group_clue_candidates(&deductions, HGroupProfile::Max),
        prospective_clue_hazard(
            deductions.view(),
            HGroupProfile::Max,
            PlayerId::new(1),
            CardId::new(8),
            Clue::Rank(Rank::One),
            &[CardId::new(8)],
            true,
        )
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn paired_sample_one_giver_rejects_a_rank_three_false_prompt() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(7)),
        Action::Play(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(5)),
        Action::Discard(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(9)),
        Action::Discard(CardId::new(21)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(8)),
        Action::Play(CardId::new(23)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(22)),
        Action::Discard(CardId::new(27)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Discard(CardId::new(19)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Discard(CardId::new(2)),
        Action::Play(CardId::new(32)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(18)),
        Action::Discard(CardId::new(26)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let bad_clue = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Rank(Rank::Three),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates
            .iter()
            .any(|candidate| candidate.action == bad_clue),
        "giver knowingly created a false Purple-2 Prompt through Purple 4: {candidates:#?}"
    );
}

#[test]
fn paired_sample_one_no_information_purple_reclue_does_not_play_purple_four() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(7)),
        Action::Play(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(5)),
        Action::Discard(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(9)),
        Action::Discard(CardId::new(21)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(8)),
        Action::Play(CardId::new(23)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(22)),
        Action::Discard(CardId::new(27)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Discard(CardId::new(19)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(25))),
        "no-information Purple reclue falsely made Purple 4 playable: {inferred:#?}"
    );
}

#[test]
fn paired_sample_one_giver_rejects_green_clue_that_blind_plays_purple_four() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(2)),
        Action::Play(CardId::new(8)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(18)),
        Action::Play(CardId::new(5)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let bad_clue = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Suit(Suit::Green),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates
            .iter()
            .any(|candidate| candidate.action == bad_clue),
        "giver knowingly forced a fresh Purple 4: {candidates:#?}"
    );
}

#[test]
fn paired_sample_one_giver_rejects_green_five_without_a_green_three() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(2)),
        Action::Play(CardId::new(8)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let bad_clue = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Suit(Suit::Green),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates
            .iter()
            .any(|candidate| candidate.action == bad_clue),
        "giver knowingly created a Green-3 layered finesse without Green 3: {candidates:#?}"
    );
}

#[test]
fn charm_does_not_retouch_an_already_chop_moved_four() {
    let mut state = paired_sample_two_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(10)),
    ] {
        state.apply(action).unwrap();
    }
    let player = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(player).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Four),
        }),
        "player: {player:?}; candidates: {:#?}; clues: {:#?}; inference: {inferred:#?}",
        ordered_h_group_actions(&deductions, HGroupProfile::Max),
        h_group_clue_candidates(&deductions, HGroupProfile::Max)
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn paired_sample_six_plays_yellow_two_connection_before_yellow_three() {
    let mut state = paired_sample_six_state();
    for (index, action) in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(15)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(13)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(8)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(7)),
        Action::Discard(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(5)),
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(9)),
        Action::Discard(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Discard(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Discard(CardId::new(2)),
        Action::Play(CardId::new(24)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(27)),
        Action::Discard(CardId::new(26)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(3)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(25)),
        Action::Play(CardId::new(0)),
        Action::Discard(CardId::new(30)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Discard(CardId::new(16)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(21)),
        Action::Discard(CardId::new(29)),
        Action::Play(CardId::new(22)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(37)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(23)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(34)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(33)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(32)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Yellow),
        },
    ]
    .into_iter()
    .enumerate()
    {
        state
            .apply(action)
            .unwrap_or_else(|error| panic!("action {index} {action:?}: {error:?}"));
    }
    let player = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(player).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Discard(CardId::new(38))),
        "player: {player:?}; candidates: {:#?}; inference: {inferred:#?}",
        ordered_h_group_actions(&deductions, HGroupProfile::Max)
    );
}

#[test]
fn delayed_purple_clue_is_rejected_without_a_demonstrable_connector() {
    let mut state = paired_sample_seven_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
    ] {
        state.apply(action).unwrap();
    }
    let player = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(player).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        }),
        "player: {player:?}; candidates: {:#?}; inference: {inferred:#?}",
        ordered_h_group_actions(&deductions, HGroupProfile::Max)
    );
}

#[test]
fn blue_three_clue_is_rejected_when_it_would_prompt_an_older_blue_five() {
    let mut state = paired_sample_five_state();
    for (index, action) in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(3)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(15)),
        Action::Discard(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(1)),
        Action::Discard(CardId::new(7)),
    ]
    .into_iter()
    .enumerate()
    {
        state
            .apply(action)
            .unwrap_or_else(|error| panic!("action {index} {action:?}: {error:?}"));
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    let bad_clue = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Suit(Suit::Blue),
    };
    assert!(
        !ordered_h_group_actions(&deductions, HGroupProfile::Max).contains(&bad_clue),
        "selected: {:?}; inference: {inferred:#?}; clues: {:#?}",
        select_h_group_action(&deductions, HGroupProfile::Max),
        h_group_clue_candidates(&deductions, HGroupProfile::Max),
    );
    assert!(select_h_group_action(&deductions, HGroupProfile::Max).is_some());
}

#[test]
fn five_on_chop_is_a_save_and_is_not_played() {
    let mut state = state_with_prefix(
        2,
        &[
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Red, Rank::Five),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        })
        .unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert_eq!(
        inferred.clues[0].kind,
        HGroupClueKind::Save(HGroupSaveKind::Five)
    );
    assert_eq!(inferred.saved, vec![CardId::new(5)]);
    assert!(inferred.playable_now.is_empty());
}
