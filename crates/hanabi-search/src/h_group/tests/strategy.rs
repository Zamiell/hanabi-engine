#[test]
fn delayed_play_clue_finesses_newest_unclued_card() {
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::Two),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    let inferred = infer_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert_eq!(
        inferred.connection,
        Some(HGroupConnection {
            card: CardId::new(9),
            identity: Card::new(Suit::Red, Rank::One),
            kind: HGroupConnectionKind::Finesse,
            focus: CardId::new(10),
        })
    );

    // A reconstructed or otherwise arbitrary view can invalidate a
    // convention promise while retaining its public clue history. Such a
    // stale promise must never escape as an illegal planning candidate.
    let mut stale_view = state.view_for(PlayerId::new(1)).unwrap();
    stale_view.hands[1]
        .iter_mut()
        .find(|card| card.id == CardId::new(9))
        .unwrap()
        .id = CardId::new(15);
    let stale_deductions = LogicalDeductions::new(stale_view).unwrap();
    let stale_legal = stale_deductions.view().legal_actions();
    let stale_candidates = ordered_h_group_actions(
        &stale_deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert!(!stale_candidates.contains(&Action::Play(CardId::new(9))));
    assert!(
        stale_candidates
            .iter()
            .all(|action| stale_legal.contains(action))
    );

    let finesse = convention.analyze(&deductions).preferred_action.unwrap();
    assert_eq!(finesse, Action::Play(CardId::new(9)));
    state.apply(finesse).unwrap();

    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert_eq!(
        convention.analyze(&deductions).preferred_action.unwrap(),
        Action::Play(CardId::new(10)),
        "{inferred:#?}"
    );
}

#[test]
fn prompt_takes_precedence_over_finesse_and_policy_plays_it() {
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Red, Rank::Four),
            Card::new(Suit::Red, Rank::Four),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Five),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Five),
        })
        .unwrap();
    state.apply(Action::Discard(CardId::new(5))).unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();

    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    let inferred = infer_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert_eq!(
        inferred.connection,
        Some(HGroupConnection {
            card: CardId::new(15),
            identity: Card::new(Suit::Red, Rank::One),
            kind: HGroupConnectionKind::Prompt,
            focus: CardId::new(10),
        }),
        "{inferred:#?}"
    );
    assert_eq!(
        convention.analyze(&deductions).preferred_action.unwrap(),
        Action::Play(CardId::new(15))
    );
}

#[test]
fn policy_gives_and_recipient_plays_a_direct_level_one_play_clue() {
    let mut state = state_with_prefix(
        2,
        &[
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::Five),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Purple, Rank::Three),
        ],
    );
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    let giver_view = state.view_for(PlayerId::new(0)).unwrap();
    let deductions = LogicalDeductions::new(giver_view.clone()).unwrap();
    let clue = convention.analyze(&deductions).preferred_action.unwrap();
    assert_eq!(
        clue,
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        }
    );
    let line = project_h_group_line(
        &giver_view,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
        clue,
        32,
    );
    assert_eq!(line.actions, 2, "the clue and forced play are projected");
    assert_eq!(line.score_gain, 1);
    state.apply(clue).unwrap();

    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    assert_eq!(
        convention.analyze(&deductions).preferred_action.unwrap(),
        Action::Play(CardId::new(5))
    );
}

#[test]
fn save_principle_prefers_a_five_save_over_a_play_clue() {
    let state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Red, Rank::Five),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Purple, Rank::Three),
        ],
    );
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    assert_eq!(
        convention.analyze(&deductions).preferred_action.unwrap(),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        }
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn chop_clue_note_keeps_both_play_and_critical_save_possibilities() {
    let blue = Clue::Suit(Suit::Blue);
    let blue_one = Card::new(Suit::Blue, Rank::One);
    let blue_two = Card::new(Suit::Blue, Rank::Two);
    let blue_four = Card::new(Suit::Blue, Rank::Four);
    let view = PlayerView {
        observer: PlayerId::new(1),
        current_player: PlayerId::new(1),
        turn: 7,
        hands: vec![
            vec![
                observed(2, Some(Card::new(Suit::Green, Rank::One)), &[]),
                observed(3, Some(Card::new(Suit::Yellow, Rank::One)), &[]),
                observed(4, Some(Card::new(Suit::Purple, Rank::One)), &[]),
                observed(15, Some(Card::new(Suit::Red, Rank::Two)), &[]),
                observed(17, Some(Card::new(Suit::Red, Rank::One)), &[]),
            ],
            vec![
                observed(5, None, &[blue]),
                observed(6, None, &[]),
                observed(7, None, &[]),
                observed(8, None, &[]),
                observed(9, None, &[]),
            ],
            vec![
                observed(11, Some(Card::new(Suit::Green, Rank::Three)), &[]),
                observed(12, Some(Card::new(Suit::Yellow, Rank::Three)), &[]),
                observed(13, Some(Card::new(Suit::Purple, Rank::Three)), &[]),
                observed(14, Some(Card::new(Suit::Red, Rank::Four)), &[]),
                observed(16, Some(Card::new(Suit::Blue, Rank::Five)), &[]),
            ],
        ],
        deck_size: 32,
        play_stacks: [
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![(CardId::new(0), blue_one), (CardId::new(10), blue_two)],
            Vec::new(),
        ],
        discard_pile: vec![(CardId::new(1), blue_four)],
        clue_tokens: 7,
        strikes: 0,
        final_turns_remaining: None,
        status: GameStatus::InProgress,
        history: vec![
            ObservedHistoryEntry {
                turn: 0,
                event: ObservedEvent::Played {
                    player: PlayerId::new(0),
                    card: CardId::new(0),
                    identity: blue_one,
                    successful: true,
                },
            },
            ObservedHistoryEntry {
                turn: 0,
                event: ObservedEvent::Drew {
                    player: PlayerId::new(0),
                    card: CardId::new(15),
                    identity: Some(Card::new(Suit::Red, Rank::Two)),
                },
            },
            ObservedHistoryEntry {
                turn: 2,
                event: ObservedEvent::Played {
                    player: PlayerId::new(2),
                    card: CardId::new(10),
                    identity: blue_two,
                    successful: true,
                },
            },
            ObservedHistoryEntry {
                turn: 2,
                event: ObservedEvent::Drew {
                    player: PlayerId::new(2),
                    card: CardId::new(16),
                    identity: Some(Card::new(Suit::Blue, Rank::Five)),
                },
            },
            ObservedHistoryEntry {
                turn: 3,
                event: ObservedEvent::Discarded {
                    player: PlayerId::new(0),
                    card: CardId::new(1),
                    identity: blue_four,
                },
            },
            ObservedHistoryEntry {
                turn: 3,
                event: ObservedEvent::Drew {
                    player: PlayerId::new(0),
                    card: CardId::new(17),
                    identity: Some(Card::new(Suit::Red, Rank::One)),
                },
            },
            ObservedHistoryEntry {
                turn: 6,
                event: ObservedEvent::Clued {
                    giver: PlayerId::new(0),
                    target: PlayerId::new(1),
                    clue: blue,
                    touched: vec![CardId::new(5)],
                    untouched: vec![
                        CardId::new(6),
                        CardId::new(7),
                        CardId::new(8),
                        CardId::new(9),
                    ],
                },
            },
        ],
    };
    let deductions = LogicalDeductions::new(view).unwrap();
    let inferred = infer_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    let focus = inferred
        .cards
        .iter()
        .find(|card| card.card == CardId::new(5))
        .unwrap();
    assert_eq!(
        focus.identities,
        IdentitySet::singleton(Card::new(Suit::Blue, Rank::Three))
            .union(IdentitySet::singleton(blue_four))
    );
    assert!(focus.saved);
    assert!(!inferred.playable_now.contains(&CardId::new(5)));
}

#[test]
fn double_chop_twos_are_an_exception_to_the_visible_rule() {
    let red_two = Card::new(Suit::Red, Rank::Two);
    let state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Blue, Rank::Four),
            red_two,
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Blue, Rank::Five),
            red_two,
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Two),
        ],
    );
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let candidates =
        h_group_clue_candidates(&deductions, HGroupProfile::Level(HGroupLevel::Level1));
    for target in [PlayerId::new(1), PlayerId::new(2)] {
        assert!(candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target,
                    clue: Clue::Rank(Rank::Two),
                }
        }));
    }
}

#[test]
fn finesse_card_is_invisibly_clued_for_later_focus() {
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Green, Rank::Five),
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Red, Rank::Two),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Five),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        })
        .unwrap();

    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert!(inferred.invisibly_clued.contains(&CardId::new(9)));
    assert_eq!(inferred.clues.last().unwrap().focus, CardId::new(8));
}

#[test]
fn early_game_ends_only_when_a_chop_is_discarded() {
    let mut state = state_with_prefix(
        2,
        &[
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::Five),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Purple, Rank::Three),
        ],
    );
    let initial = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    assert!(infer_h_group(&initial, HGroupProfile::Level(crate::HGroupLevel::Level1)).early_game);
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    state.apply(Action::Discard(CardId::new(6))).unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    assert!(
        !infer_h_group(
            &deductions,
            HGroupProfile::Level(crate::HGroupLevel::Level1)
        )
        .early_game
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn prompt_promise_continues_left_to_right_after_a_wrong_card_plays() {
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Blue, Rank::Four),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Green),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();

    let information_set =
        crate::InformationSet::new(&state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(
        information_set.deductions(),
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert_eq!(inferred.connection.unwrap().card, CardId::new(8));
    assert_eq!(
        inferred.connection_promises,
        vec![HGroupConnectionPromise {
            cards: vec![CardId::new(8), CardId::new(7)],
            identity: Card::new(Suit::Red, Rank::One),
        }]
    );
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    let expected = Card::new(Suit::Red, Rank::One);
    let belief = convention
        .analyze(information_set.deductions())
        .belief_constraints;
    assert_eq!(belief.branches.len(), 2);
    assert!(
        belief
            .branches
            .iter()
            .any(|branch| { branch.contains(&(CardId::new(8), IdentitySet::singleton(expected))) })
    );
    assert!(belief.branches.iter().any(|branch| {
        branch.contains(&(CardId::new(7), IdentitySet::singleton(expected)))
            && branch.iter().any(|(card, identities)| {
                *card == CardId::new(8)
                    && identities.iter().all(|identity| identity.rank == Rank::One)
                    && !identities.contains(expected)
            })
    }));
    state.apply(Action::Play(CardId::new(8))).unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        })
        .unwrap();

    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert_eq!(
        inferred.connection,
        Some(HGroupConnection {
            card: CardId::new(7),
            identity: Card::new(Suit::Red, Rank::One),
            kind: HGroupConnectionKind::Prompt,
            focus: CardId::new(10),
        })
    );
}

#[test]
fn delayed_play_can_use_one_finesse_then_an_accounted_chain() {
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Red, Rank::Four),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Purple, Rank::Two),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Two),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Three),
        })
        .unwrap();
    // Advance to player 0 without giving a clue that itself creates a
    // same-hand Self-Prompt and masks the delayed red chain under test.
    state.apply(Action::Discard(CardId::new(14))).unwrap();

    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    let candidates = convention
        .analyze(&deductions)
        .actions
        .into_iter()
        .map(|candidate| candidate.action)
        .collect::<Vec<_>>();
    assert!(
        candidates.contains(&Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        }),
        "candidates: {candidates:?}; clues: {:?}",
        h_group_clue_candidates(&deductions, HGroupProfile::Level(HGroupLevel::Level1))
    );
}

#[test]
fn good_touch_notes_and_root_belief_exclude_a_duplicate_focus() {
    let mut state = state_with_prefix(
        2,
        &[
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Red, Rank::One),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    let information_set =
        crate::InformationSet::new(&state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(
        information_set.deductions(),
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    let focus = inferred
        .cards
        .iter()
        .find(|card| card.card == CardId::new(9))
        .unwrap();
    let non_focus = inferred
        .cards
        .iter()
        .find(|card| card.card == CardId::new(8))
        .unwrap();
    assert_eq!(
        focus.identities,
        IdentitySet::singleton(Card::new(Suit::Red, Rank::One))
    );
    assert!(
        !non_focus
            .identities
            .contains(Card::new(Suit::Red, Rank::One))
    );

    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    let belief = convention
        .analyze(information_set.deductions())
        .belief_constraints;
    let focus_constraint = belief
        .constraints
        .iter()
        .find(|(card, _)| *card == CardId::new(9))
        .unwrap();
    let non_focus_constraint = belief
        .constraints
        .iter()
        .find(|(card, _)| *card == CardId::new(8))
        .unwrap();
    assert_eq!(
        focus_constraint.1,
        IdentitySet::singleton(Card::new(Suit::Red, Rank::One))
    );
    assert!(
        !non_focus_constraint
            .1
            .contains(Card::new(Suit::Red, Rank::One))
    );
}

#[test]
fn visible_two_off_chop_prevents_a_two_save() {
    let red_two = Card::new(Suit::Red, Rank::Two);
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Four),
            red_two,
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Green, Rank::Five),
            red_two,
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Blue, Rank::Two),
        ],
    );
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    assert!(
        !convention
            .analyze(&deductions)
            .actions
            .into_iter()
            .map(|candidate| candidate.action)
            .collect::<Vec<_>>()
            .contains(&Action::Clue {
                target: PlayerId::new(1),
                clue: Clue::Rank(Rank::Two),
            })
    );

    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        })
        .unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert_eq!(
        inferred.clues.last().unwrap().kind,
        HGroupClueKind::Unrecognized
    );
    assert!(inferred.clues.last().unwrap().save_identities.is_empty());
}

#[test]
fn playable_two_on_chop_is_still_a_two_save() {
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Green, Rank::Five),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Blue, Rank::Two),
        ],
    );
    state.apply(Action::Play(CardId::new(0))).unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Two),
        })
        .unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert_eq!(
        inferred.clues.last().unwrap().kind,
        HGroupClueKind::Save(HGroupSaveKind::Two)
    );
    assert!(inferred.clues.last().unwrap().play_identities.is_empty());
}

#[test]
fn convention_known_trash_is_discarded_before_the_chop() {
    let mut state = state_with_prefix(
        2,
        &[
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Blue, Rank::Four),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    state.apply(Action::Play(CardId::new(0))).unwrap();

    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    let inferred = infer_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert_eq!(inferred.chops[1], Some(CardId::new(6)));
    assert_eq!(
        inferred
            .cards
            .iter()
            .find(|card| card.card == CardId::new(5))
            .unwrap()
            .identities,
        IdentitySet::singleton(Card::new(Suit::Red, Rank::One))
    );
    assert_eq!(
        convention.analyze(&deductions).preferred_action.unwrap(),
        Action::Discard(CardId::new(5))
    );
}

#[test]
fn no_chop_forces_a_tempo_clue_instead_of_a_card_action() {
    let mut state = state_with_prefix(
        2,
        &[
            Card::new(Suit::Red, Rank::Five),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Green, Rank::Five),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Purple, Rank::One),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        })
        .unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    let inferred = infer_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert_eq!(inferred.chops[0], None);
    let candidates = convention
        .analyze(&deductions)
        .actions
        .into_iter()
        .map(|candidate| candidate.action)
        .collect::<Vec<_>>();
    assert!(!candidates.is_empty());
    assert!(
        candidates
            .iter()
            .all(|action| matches!(action, Action::Clue { .. }))
    );
}

#[test]
fn rank_clue_beats_color_only_when_it_gets_more_cards() {
    let state = state_with_prefix(
        2,
        &[
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Three),
        ],
    );
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    assert_eq!(
        convention.analyze(&deductions).preferred_action.unwrap(),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        }
    );
}

#[test]
fn play_clue_to_the_same_player_preoccupies_before_a_save() {
    let state = state_with_prefix(
        2,
        &[
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Red, Rank::Five),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Blue, Rank::One),
        ],
    );
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    assert_eq!(
        convention.analyze(&deductions).preferred_action.unwrap(),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        }
    );
}

#[test]
fn next_players_unique_playable_chop_preempts_own_play() {
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Blue, Rank::Four),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Five),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();

    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    let candidates = convention
        .analyze(&deductions)
        .actions
        .into_iter()
        .map(|candidate| candidate.action)
        .collect::<Vec<_>>();
    assert_eq!(
        candidates.first(),
        Some(&Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        })
    );
    assert!(candidates.contains(&Action::Play(CardId::new(0))));
}

#[test]
fn level_one_policy_can_roll_a_game_to_completion() {
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    for players in 2..=5 {
        let mut deck = standard_deck();
        deck.rotate_left(usize::from(players) * 3);
        let state = FullState::new_standard(players, deck).unwrap();
        let outcome = continuation_to_terminal(state, convention).unwrap();
        assert!(outcome.turns() > 0);
        assert!(outcome.turns() < MAX_TEST_CONTINUATION_TURNS);
    }
}

fn profile_rolls_to_completion(profile: HGroupProfile) {
    let mut deck = standard_deck();
    deck.rotate_left(11);
    let state = FullState::new_standard(3, deck).unwrap();
    let convention = crate::SupportedConvention::HGroup(profile);
    let outcome = continuation_to_terminal(state, convention)
        .unwrap_or_else(|error| panic!("{profile} continuation failed: {error}"));
    assert!(outcome.turns() > 0, "{profile}");
    assert!(outcome.turns() < MAX_TEST_CONTINUATION_TURNS, "{profile}");
}

macro_rules! profile_rollout_test {
    ($name:ident, $profile:expr) => {
        #[test]
        #[ignore = "exhaustive H-Group profile matrix; run scripts/check-exhaustive.sh"]
        fn $name() {
            profile_rolls_to_completion($profile);
        }
    };
}

macro_rules! representative_profile_rollout_test {
    ($name:ident, $profile:expr) => {
        #[test]
        fn $name() {
            profile_rolls_to_completion($profile);
        }
    };
}

representative_profile_rollout_test!(
    representative_profile_rollout_level_01,
    HGroupProfile::Level(HGroupLevel::Level1)
);
representative_profile_rollout_test!(
    representative_profile_rollout_level_10,
    HGroupProfile::Level(HGroupLevel::Level10)
);
representative_profile_rollout_test!(representative_profile_rollout_max, HGroupProfile::Max);

profile_rollout_test!(profile_rollout_level_01, HGroupProfile::Level(HGroupLevel::Level1));
profile_rollout_test!(profile_rollout_level_02, HGroupProfile::Level(HGroupLevel::Level2));
profile_rollout_test!(profile_rollout_level_03, HGroupProfile::Level(HGroupLevel::Level3));
profile_rollout_test!(profile_rollout_level_04, HGroupProfile::Level(HGroupLevel::Level4));
profile_rollout_test!(profile_rollout_level_05, HGroupProfile::Level(HGroupLevel::Level5));
profile_rollout_test!(profile_rollout_level_06, HGroupProfile::Level(HGroupLevel::Level6));
profile_rollout_test!(profile_rollout_level_07, HGroupProfile::Level(HGroupLevel::Level7));
profile_rollout_test!(profile_rollout_level_08, HGroupProfile::Level(HGroupLevel::Level8));
profile_rollout_test!(profile_rollout_level_09, HGroupProfile::Level(HGroupLevel::Level9));
profile_rollout_test!(profile_rollout_level_10, HGroupProfile::Level(HGroupLevel::Level10));
profile_rollout_test!(profile_rollout_level_11, HGroupProfile::Level(HGroupLevel::Level11));
profile_rollout_test!(profile_rollout_level_12, HGroupProfile::Level(HGroupLevel::Level12));
profile_rollout_test!(profile_rollout_level_13, HGroupProfile::Level(HGroupLevel::Level13));
profile_rollout_test!(profile_rollout_level_14, HGroupProfile::Level(HGroupLevel::Level14));
profile_rollout_test!(profile_rollout_level_15, HGroupProfile::Level(HGroupLevel::Level15));
profile_rollout_test!(profile_rollout_level_16, HGroupProfile::Level(HGroupLevel::Level16));
profile_rollout_test!(profile_rollout_level_17, HGroupProfile::Level(HGroupLevel::Level17));
profile_rollout_test!(profile_rollout_level_18, HGroupProfile::Level(HGroupLevel::Level18));
profile_rollout_test!(profile_rollout_level_19, HGroupProfile::Level(HGroupLevel::Level19));
profile_rollout_test!(profile_rollout_level_20, HGroupProfile::Level(HGroupLevel::Level20));
profile_rollout_test!(profile_rollout_level_21, HGroupProfile::Level(HGroupLevel::Level21));
profile_rollout_test!(profile_rollout_level_22, HGroupProfile::Level(HGroupLevel::Level22));
profile_rollout_test!(profile_rollout_level_23, HGroupProfile::Level(HGroupLevel::Level23));
profile_rollout_test!(profile_rollout_level_24, HGroupProfile::Level(HGroupLevel::Level24));
profile_rollout_test!(profile_rollout_level_25, HGroupProfile::Level(HGroupLevel::Level25));
profile_rollout_test!(profile_rollout_max, HGroupProfile::Max);

#[test]
#[allow(clippy::too_many_lines)]
fn positional_five_save_does_not_create_a_false_layered_finesse() {
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
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
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
        Action::Play(CardId::new(15)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(5)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(0)),
        Action::Play(CardId::new(9)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(1)),
        Action::Discard(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(20)),
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(3)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(26)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(21)),
        Action::Discard(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Discard(CardId::new(17)),
        Action::Play(CardId::new(18)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(19)),
        Action::Discard(CardId::new(16)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(23)),
        Action::Discard(CardId::new(29)),
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(37)),
        Action::Discard(CardId::new(22)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(38)),
        Action::Discard(CardId::new(30)),
        Action::Discard(CardId::new(27)),
        Action::Discard(CardId::new(28)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Discard(CardId::new(33)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        },
    ] {
        state.apply(action).unwrap();
    }
    let bad_clue = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Suit(Suit::Green),
    };
    // This synthetic prefix contains an earlier off-chop trash clue. Under
    // the correct TCM interpretation the policy is free to discard here, so
    // exact action selection is not part of this regression. Its purpose is
    // the recipient-side Positional 5's Save interpretation below.
    state.apply(bad_clue).unwrap();
    let recipient = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&recipient, HGroupProfile::Max);
    assert_ne!(
        inferred.connection.map(|connection| connection.card),
        Some(CardId::new(44)),
        "the prior rank-5 stall created a false layered finesse: {inferred:#?}"
    );
    assert_ne!(
        select_h_group_action(&recipient, HGroupProfile::Max),
        Some(Action::Play(CardId::new(44)))
    );
}


#[test]
fn out_of_order_fix_uses_the_historical_stack_height() {
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
    ] {
        state.apply(action).unwrap();
    }
    // This synthetic prefix sets up a historical interpretation, not a
    // best-move oracle. Which unrelated trash card ranks first here is not
    // part of the historical-stack invariant exercised below.
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Play(CardId::new(9)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Green),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    assert_eq!(
        inferred
            .clues
            .iter()
            .find(|clue| clue.turn == 15)
            .map(|clue| clue.stack_heights),
        Some([2, 1, 0, 1, 1]),
        "the rank-4 clue must retain its pre-fix stack, not today's stack: {inferred:#?}"
    );
    assert_ne!(
        inferred.connection.map(|connection| connection.card),
        Some(CardId::new(4)),
        "the fix clue was replayed as a new Prompt: {inferred:#?}"
    );
    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(4)))
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn critical_save_is_not_treated_as_an_out_of_order_play() {
    let mut state = paired_sample_three_state();
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
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    assert!(
        inferred
            .clues
            .iter()
            .any(|clue| { clue.turn == 17 && matches!(clue.kind, HGroupClueKind::Save(_)) })
    );
    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        }),
        "a Save clue incorrectly created a mandatory out-of-order fix: {inferred:#?}"
    );
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(3)),
        Action::Play(CardId::new(15)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(2)),
        Action::Discard(CardId::new(26)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(25)),
        Action::Discard(CardId::new(24)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(27)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let diagnostics = format!(
        "tokens {}; clues {:#?}; candidates {:?}; chops {:?}",
        deductions.view().clue_tokens,
        h_group_clue_candidates(&deductions, HGroupProfile::Max),
        ordered_h_group_actions(&deductions, HGroupProfile::Max),
        infer_h_group(&deductions, HGroupProfile::Max).chops
    );
    let fallback = select_h_group_action(&deductions, HGroupProfile::Max);
    assert_ne!(
        fallback,
        Some(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        }),
        "the no-candidate fallback selected a trash clue that looks like a Play clue: {diagnostics}"
    );
    assert_ne!(
        fallback,
        Some(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Three),
        }),
        "the fallback treated an off-chop critical card as a Save: {diagnostics}"
    );
}
