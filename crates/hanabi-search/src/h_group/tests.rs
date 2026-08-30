use super::model::FixObligation;
use super::*;
use hanabi_core::{
    Action, FullState, GameEvent, GameStatus, ObservedCard, ObservedHistoryEntry, PlayerView,
    standard_deck,
};
use hanabi_protocol::HanabiLiveReplay;

const MAX_TEST_CONTINUATION_TURNS: usize = 512;

mod architecture;
mod replay;
mod snapshots;

// Keep the large convention corpus physically grouped by responsibility while
// sharing one set of deterministic scenario builders.
include!("tests/support.rs");
include!("tests/learning_rules.rs");
include!("tests/regressions.rs");
include!("tests/strategy.rs");

fn expert_replay_p4v0s415() -> HanabiLiveReplay {
    HanabiLiveReplay::from_json(include_str!(
        "../../../hanabi-protocol/tests/fixtures/game-p4v0s415.json"
    ))
    .expect("expert replay fixture is valid")
}

fn expert_replay_p4v0s9() -> HanabiLiveReplay {
    HanabiLiveReplay::from_json(include_str!(
        "../../../hanabi-protocol/tests/fixtures/game-p4v0s9.json"
    ))
    .expect("second expert replay fixture is valid")
}

fn expert_replay_p4v0s2() -> HanabiLiveReplay {
    HanabiLiveReplay::from_json(include_str!(
        "../../../hanabi-protocol/tests/fixtures/game-p4v0s2.json"
    ))
    .expect("third expert replay fixture is valid")
}

fn expert_replay_p4v0s3() -> HanabiLiveReplay {
    HanabiLiveReplay::from_json(include_str!(
        "../../../hanabi-protocol/tests/fixtures/game-p4v0s3.json"
    ))
    .expect("fourth expert replay fixture is valid")
}

fn replay_action_at_turn(replay: &HanabiLiveReplay, turn: u32) -> Action {
    replay
        .state_at_turn(turn + 1)
        .expect("fixture action is legal")
        .history()
        .iter()
        .find_map(|entry| {
            (entry.turn == turn).then_some(match entry.event {
                GameEvent::Clued { target, clue, .. } => Some(Action::Clue { target, clue }),
                GameEvent::Played { card, .. } => Some(Action::Play(card)),
                GameEvent::Discarded { card, .. } => Some(Action::Discard(card)),
                GameEvent::Drew { .. } => None,
            })?
        })
        .expect("turn contains one player action")
}

#[test]
fn opening_delayed_play_clue_stays_in_superposition_until_the_finesse_is_demonstrated() {
    let yellow_one = Card::new(Suit::Yellow, Rank::One);
    let yellow_two = Card::new(Suit::Yellow, Rank::Two);
    let unresolved = IdentitySet::singleton(yellow_one).union(IdentitySet::singleton(yellow_two));

    for turn in [2, 3] {
        let state = expert_replay_p4v0s415()
            .state_at_turn(turn)
            .expect("fixture prefix is legal");
        let view = state.view_for(PlayerId::new(0)).expect("Alice has a view");
        let deductions = LogicalDeductions::new(view).expect("valid deductions");
        let inferred = infer_h_group(&deductions, HGroupProfile::Max);
        let focus = inferred
            .cards
            .iter()
            .find(|note| note.card == CardId::new(2))
            .expect("Alice's yellow-clued focus has a convention note");

        assert_eq!(
            focus.identities, unresolved,
            "before Donald blind-plays, the clue can still mean direct yellow 1 or delayed yellow 2 at turn {turn}"
        );
    }

    let demonstrated = expert_replay_p4v0s415()
        .state_at_turn(4)
        .expect("fixture prefix is legal");
    let view = demonstrated
        .view_for(PlayerId::new(0))
        .expect("Alice has a view");
    let deductions = LogicalDeductions::new(view).expect("valid deductions");
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    let focus = inferred
        .cards
        .iter()
        .find(|note| note.card == CardId::new(2))
        .expect("Alice's yellow-clued focus has a convention note");

    assert_eq!(focus.identities, IdentitySet::singleton(yellow_two));
}

#[test]
fn proven_layered_yellow_line_eliminates_yellow_one_from_the_new_focus() {
    let expected = IdentitySet::singleton(Card::new(Suit::Yellow, Rank::Three));

    for turn in [5, 6, 7, 8] {
        let state = expert_replay_p4v0s415()
            .state_at_turn(turn)
            .expect("fixture prefix is legal");
        let view = state.view_for(PlayerId::new(2)).expect("Cathy has a view");
        let deductions = LogicalDeductions::new(view).expect("valid deductions");
        let replay = replay_h_group(&deductions, HGroupProfile::Max);
        let inferred = infer_h_group_from_replay(&deductions, replay.clone(), HGroupProfile::Max);
        let focus = inferred
            .cards
            .iter()
            .find(|note| note.card == CardId::new(16))
            .expect("Cathy's yellow-clued focus has a convention note");

        assert_eq!(
            focus.identities, expected,
            "Donald already owns the demonstrated yellow-1 promise and Alice owns yellow 2, so Good Touch makes Cathy's new yellow focus exactly yellow 3 at turn {turn}; replay={replay:#?}"
        );
        assert_eq!(focus.identity_status, HGroupIdentityStatus::Settled);
        assert!(
            !inferred.playable_now.contains(&CardId::new(16)),
            "yellow 3 remains delayed until its promised predecessors play at turn {turn}; clues={:#?}",
            replay.clues
        );
    }
}

#[test]
fn move_33_does_not_treat_an_ungotten_card_as_a_green_five() {
    let state = expert_replay_p4v0s415()
        .state_at_turn(32)
        .expect("fixture prefix is legal");
    let actor = state.current_player();
    let view = state.view_for(actor).expect("current player has a view");
    let deductions = LogicalDeductions::new(view.clone()).expect("valid deductions");
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_eq!(
        ordered_playable_cards(&view, &inferred, HGroupProfile::Max),
        vec![CardId::new(27), CardId::new(3)]
    );
}

#[test]
fn move_35_uses_the_oldest_matching_elimination_note() {
    let replay_fixture = expert_replay_p4v0s415();
    let state = replay_fixture
        .state_at_turn(34)
        .expect("fixture prefix is legal");
    let actor = state.current_player();
    let view = state.view_for(actor).expect("current player has a view");
    let deductions = LogicalDeductions::new(view.clone()).expect("valid deductions");
    let replay = replay_h_group(&deductions, HGroupProfile::Max);
    let purple_two = Card::new(Suit::Purple, Rank::Two);
    let notes = replay
        .signals
        .iter()
        .rev()
        .find(|signal| {
            signal.kind == HGroupMoveKind::Elimination
                && signal.target == Some(PlayerId::new(1))
                && signal.identity == Some(purple_two)
        })
        .expect("discarded purple 2 creates elimination notes");
    assert_eq!(
        notes.cards,
        [CardId::new(6), CardId::new(25), CardId::new(28)]
    );
    assert!(!notes.cards.contains(&CardId::new(38)));

    let clue = Action::Clue {
        target: PlayerId::new(3),
        clue: Clue::Rank(Rank::Three),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    assert!(
        candidates.iter().any(|candidate| candidate.action == clue),
        "rank 3 must use James's oldest purple-2 elimination note: {candidates:#?}"
    );

    let analysis = crate::analyze_position(
        &view,
        crate::SupportedConvention::HGroup(HGroupProfile::Max),
        crate::PlannerConfig {
            objective: crate::PlanningObjective::PerfectScore,
            ..crate::PlannerConfig::default()
        },
    )
    .expect("fixture position is analyzable");
    assert_eq!(
        analysis.planner.best_action, clue,
        "candidates={candidates:#?}; roots={:#?}",
        analysis.planner.root_actions
    );

    let after_clue = replay_fixture
        .state_at_turn(35)
        .expect("fixture clue is legal");
    let james_view = after_clue
        .view_for(PlayerId::new(1))
        .expect("James has a view");
    let james_deductions = LogicalDeductions::new(james_view).expect("valid deductions");
    let james_inferences = infer_h_group(&james_deductions, HGroupProfile::Max);
    assert_eq!(
        james_inferences.connection,
        Some(HGroupConnection {
            card: CardId::new(6),
            identity: purple_two,
            kind: HGroupConnectionKind::Finesse,
            focus: CardId::new(33),
        })
    );
    assert!(james_inferences.signals.iter().any(|signal| {
        signal.kind == HGroupMoveKind::EliminationFinesse
            && signal.cards == [CardId::new(6)]
            && signal.identity == Some(purple_two)
    }));

    let after_connector = replay_fixture
        .state_at_turn(38)
        .expect("fixture connector play is legal");
    let libster_view = after_connector
        .view_for(PlayerId::new(3))
        .expect("Libster has a view");
    let libster_deductions = LogicalDeductions::new(libster_view).expect("valid deductions");
    let libster_inferences = infer_h_group(&libster_deductions, HGroupProfile::Max);
    assert!(libster_inferences.playable_now.contains(&CardId::new(33)));
}

#[test]
fn move_36_compares_purple_and_four_after_good_touch_closure() {
    let state = expert_replay_p4v0s415()
        .state_at_turn(35)
        .expect("fixture prefix is legal");
    let actor = state.current_player();
    let view = state.view_for(actor).expect("current player has a view");
    let deductions = LogicalDeductions::new(view.clone()).expect("valid deductions");
    let purple = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Suit(Suit::Purple),
    };
    let four = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Rank(Rank::Four),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let score = |action| {
        candidates
            .iter()
            .find(|candidate| candidate.action == action)
            .unwrap_or_else(|| panic!("missing candidate {action:?}: {candidates:#?}"))
            .score()
    };

    assert_eq!(
        score(purple),
        score(four) + 1,
        "both clues secure purple 4 and the automatic purple 5; only the color tie-break remains: {candidates:#?}"
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(purple),
        "purple must beat rank 4 once redundant Good Touch deductions are normalized: {candidates:#?}"
    );
}

#[test]
fn move_41_plays_the_promised_purple_four_after_the_connection() {
    let state = expert_replay_p4v0s415()
        .state_at_turn(40)
        .expect("fixture prefix is legal");
    let actor = state.current_player();
    let view = state.view_for(actor).expect("current player has a view");
    let deductions = LogicalDeductions::new(view).expect("valid deductions");
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    assert!(
        inferred.playable_now.contains(&CardId::new(34)),
        "purple 4 should become playable after its connector: {inferred:#?}"
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(34)))
    );
}

#[test]
fn move_45_plays_the_automatic_good_touch_purple_five() {
    let state = expert_replay_p4v0s415()
        .state_at_turn(44)
        .expect("fixture prefix is legal");
    let actor = state.current_player();
    let view = state.view_for(actor).expect("current player has a view");
    let deductions = LogicalDeductions::new(view).expect("valid deductions");
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    let purple_five = inferred
        .cards
        .iter()
        .find(|note| note.card == CardId::new(24))
        .expect("the older purple card retains its Good Touch note");

    assert_eq!(
        purple_five.identities,
        IdentitySet::singleton(Card::new(Suit::Purple, Rank::Five))
    );
    assert!(inferred.playable_now.contains(&CardId::new(24)));
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(24)))
    );
}

#[test]
fn final_move_exhausts_all_worlds_before_the_last_draw() {
    let replay = expert_replay_p4v0s415();
    let state = replay
        .state_at_turn(45)
        .expect("the final decision position is legal");
    let actor = state.current_player();
    let view = state.view_for(actor).expect("the actor has a legal view");
    assert!(
        view.deck_size > 1,
        "the old exact-search gate would reject this position"
    );

    let analysis = crate::analyze_position(
        &view,
        crate::SupportedConvention::HGroup(HGroupProfile::Max),
        crate::PlannerConfig {
            objective: crate::PlanningObjective::PerfectScore,
            ..crate::PlannerConfig::default()
        },
    )
    .expect("the final position is analyzable");
    assert_eq!(analysis.planner.phase, crate::PlannerPhase::Exact);
    assert!(analysis.planner.world_count.is_exact());
    assert_eq!(
        analysis.planner.best_action,
        replay_action_at_turn(&replay, 45)
    );
    let exact = analysis
        .planner
        .root_actions
        .iter()
        .find(|evaluation| evaluation.action == analysis.planner.best_action)
        .and_then(|evaluation| evaluation.exact)
        .expect("the selected play has an exact terminal proof");
    assert_eq!(exact.worlds, analysis.planner.world_count.worlds());
    assert_eq!(exact.perfect_worlds, exact.worlds);
    assert_eq!(exact.score_sum, 25 * exact.worlds);
    assert_eq!(exact.strikeout_worlds, 0);
}

#[test]
fn move_43_prefers_the_final_play_clue_with_known_trash_collateral() {
    let replay_fixture = expert_replay_p4v0s415();
    let state = replay_fixture
        .state_at_turn(42)
        .expect("fixture prefix is legal");
    let actor = state.current_player();
    let view = state.view_for(actor).expect("current player has a view");
    let deductions = LogicalDeductions::new(view).expect("valid deductions");
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    let own_note = inferred
        .cards
        .iter()
        .find(|note| note.card == CardId::new(29))
        .expect("NoMercy's oldest card has a logical domain");
    assert!(own_note.identities.len() > 1, "#29's identity is unknown");
    assert!(
        own_note
            .identities
            .iter()
            .all(|identity| is_convention_trash(
                deductions.view(),
                identity,
                &inferred.gotten(),
                &inferred.cards,
            )),
        "every remaining identity in NoMercy's hand is nevertheless trash"
    );

    let green = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Suit(Suit::Green),
    };
    let five = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Rank(Rank::Five),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let score = |action| {
        candidates
            .iter()
            .find(|candidate| candidate.action == action)
            .unwrap_or_else(|| panic!("missing candidate {action:?}: {candidates:#?}"))
            .score()
    };
    assert!(
        score(green) > score(five),
        "green adds a known-trash card while rank 5 only gives the play: {candidates:#?}"
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(green)
    );

    let after_green = replay_fixture
        .state_at_turn(43)
        .expect("the green clue is legal");
    let james = PlayerId::new(1);
    let james_deductions = LogicalDeductions::new(
        after_green
            .view_for(james)
            .expect("James has a recipient view"),
    )
    .expect("valid deductions");
    let james_inferred = infer_h_group(&james_deductions, HGroupProfile::Max);
    assert!(james_inferred.playable_now.contains(&CardId::new(44)));
    let green_one = james_inferred
        .cards
        .iter()
        .find(|note| note.card == CardId::new(40))
        .expect("the other green card has a convention note");
    assert!(green_one.identities.iter().all(|identity| {
        is_convention_trash(
            james_deductions.view(),
            identity,
            &james_inferred.gotten(),
            &james_inferred.cards,
        )
    }));
}

#[test]
fn final_play_clues_advance_the_plan_before_surplus_known_trash_discards() {
    let cases = [
        (
            expert_replay_p4v0s415(),
            42,
            Action::Clue {
                target: PlayerId::new(1),
                clue: Clue::Suit(Suit::Green),
            },
        ),
        (
            expert_replay_p4v0s9(),
            45,
            Action::Clue {
                target: PlayerId::new(0),
                clue: Clue::Rank(Rank::Five),
            },
        ),
    ];

    for (replay, turn, expected) in cases {
        let state = replay.state_at_turn(turn).expect("fixture prefix is legal");
        let actor = state.current_player();
        let deductions = LogicalDeductions::new(
            state
                .view_for(actor)
                .expect("current player has a legal view"),
        )
        .expect("fixture deductions are valid");
        let decision = analyze_h_group_convention(&deductions, HGroupProfile::Max);
        let expected_priority = decision
            .actions
            .iter()
            .find_map(|(action, _, priority, _)| (*action == expected).then_some(*priority))
            .expect("the final Play Clue is admitted");
        let trash_priority = decision
            .actions
            .iter()
            .filter_map(|(action, _, priority, _)| {
                matches!(action, Action::Discard(_)).then_some(*priority)
            })
            .max()
            .expect("the actor has a known-trash discard");

        assert!(
            expected_priority > trash_priority,
            "move {} should advance an outstanding final play ({expected_priority}) instead of creating a surplus clue token ({trash_priority})",
            turn + 1,
        );
        assert_eq!(decision.preferred, Some(expected));
    }
}

#[test]
fn third_replay_final_play_clues_advance_before_surplus_known_trash() {
    let state = expert_replay_p4v0s2()
        .state_at_turn(43)
        .expect("fixture prefix is legal");
    let actor = state.current_player();
    let deductions = LogicalDeductions::new(
        state
            .view_for(actor)
            .expect("current player has a legal view"),
    )
    .expect("fixture deductions are valid");
    let decision = analyze_h_group_convention(&deductions, HGroupProfile::Max);
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let trash_priority = decision
        .actions
        .iter()
        .filter_map(|(action, _, priority, _)| {
            matches!(action, Action::Discard(_)).then_some(*priority)
        })
        .max()
        .expect("Donald has known trash");

    let actions = [
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Five),
        },
    ];
    let mut clue_priorities = Vec::new();
    for action in actions {
        let clue_priority = decision
            .actions
            .iter()
            .find_map(|(candidate, _, priority, _)| (*candidate == action).then_some(*priority))
            .expect("both direct yellow-5 clues are admitted");
        assert!(
            clue_priority > trash_priority,
            "the final play clue must advance the committed cleanup line"
        );
        clue_priorities.push(clue_priority);
    }
    assert_eq!(clue_priorities[0], clue_priorities[1]);
    let candidate = |action| {
        candidates
            .iter()
            .find(|candidate| candidate.action == action)
            .expect("both direct yellow-5 clues are candidates")
    };
    assert_eq!(candidate(actions[0]).purpose, candidate(actions[1]).purpose);
    assert_eq!(
        candidate(actions[0]).action_coverage,
        candidate(actions[1]).action_coverage
    );
    assert_eq!(
        candidate(actions[0]).score(),
        candidate(actions[1]).score() + 1
    );
}

#[test]
fn rejects_no_value_five_fill_in_before_move_32() {
    let state = expert_replay_p4v0s415()
        .state_at_turn(31)
        .expect("fixture prefix is legal");
    let actor = state.current_player();
    let view = state.view_for(actor).expect("current player has a view");
    let deductions = LogicalDeductions::new(view.clone()).expect("valid deductions");
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    assert!(
        candidates.iter().all(|candidate| {
            candidate.action
                != (Action::Clue {
                    target: PlayerId::new(0),
                    clue: Clue::Rank(Rank::Five),
                })
        }),
        "an already-playing blue 5 plus a non-playable purple 5 has no clue value: {candidates:#?}"
    );

    let analysis = crate::analyze_position(
        &view,
        crate::SupportedConvention::HGroup(HGroupProfile::Max),
        crate::PlannerConfig {
            objective: crate::PlanningObjective::PerfectScore,
            ..crate::PlannerConfig::default()
        },
    )
    .expect("fixture position is analyzable");
    assert_eq!(
        analysis.planner.best_action,
        Action::Discard(CardId::new(21))
    );
}

#[test]
fn recognizes_expert_replay_opening_playable_chop() {
    let state = expert_replay_p4v0s415()
        .state_at_turn(0)
        .expect("turn exists");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("current player exists"),
    )
    .expect("valid deductions");
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Green),
        })
    );
}

#[test]
fn recognizes_expert_replay_layered_reverse_finesse() {
    let state = expert_replay_p4v0s415()
        .state_at_turn(1)
        .expect("turn exists");
    let view = state
        .view_for(state.current_player())
        .expect("current player exists");
    let hazard = prospective_clue_hazard(
        &view,
        HGroupProfile::Max,
        PlayerId::new(0),
        CardId::new(2),
        Clue::Suit(Suit::Yellow),
        &[CardId::new(2)],
        true,
    );
    assert_eq!(hazard, None);

    let deductions = LogicalDeductions::new(view).expect("valid deductions");
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        })
    );
}

#[test]
fn recognizes_expert_replay_queued_yellow_three() {
    let state = expert_replay_p4v0s415()
        .state_at_turn(4)
        .expect("turn exists");
    let view = state
        .view_for(state.current_player())
        .expect("current player exists");
    let hazard = prospective_clue_hazard(
        &view,
        HGroupProfile::Max,
        PlayerId::new(2),
        CardId::new(16),
        Clue::Suit(Suit::Yellow),
        &[CardId::new(10), CardId::new(16)],
        true,
    );
    let after = prospective_clue_view(
        &view,
        PlayerId::new(2),
        Clue::Suit(Suit::Yellow),
        &[CardId::new(10), CardId::new(16)],
    );
    let (recipient, recipient_replay) =
        projected_h_group_replay(&after, HGroupProfile::Max, PlayerId::new(2))
            .expect("recipient projection succeeds");
    let recipient_inferred =
        infer_h_group_from_replay(&recipient, recipient_replay.clone(), HGroupProfile::Max);
    assert_eq!(
        hazard,
        None,
        "signals={:?}; inferred={recipient_inferred:#?}; replay={recipient_replay:#?}",
        prospective_clue_signal_kinds(
            &view,
            HGroupProfile::Max,
            PlayerId::new(2),
            Clue::Suit(Suit::Yellow),
            &[CardId::new(10), CardId::new(16)],
        )
    );

    let deductions = LogicalDeductions::new(view).expect("valid deductions");
    let convention = crate::SupportedConvention::HGroup(HGroupProfile::Max).analyze(&deductions);
    let expected = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Suit(Suit::Yellow),
    };
    assert!(
        convention
            .actions
            .iter()
            .any(|candidate| candidate.action == expected),
        "the queued yellow-3 clue must be admitted; rejected={:#?}",
        convention.rejected_actions,
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(expected)
    );
}

#[test]
fn out_of_order_fix_accepts_both_clues_and_prefers_color() {
    let state = expert_replay_p4v0s415()
        .state_at_turn(6)
        .expect("turn exists");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("current player exists"),
    )
    .expect("valid deductions");
    let green = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Suit(Suit::Green),
    };
    let four = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Rank(Rank::Four),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    assert!(
        candidates.iter().any(|candidate| candidate.action == green),
        "green must be a valid Fix: {candidates:#?}"
    );
    assert!(
        candidates.iter().any(|candidate| candidate.action == four),
        "rank 4 must remain a valid Fix: {candidates:#?}"
    );
    let replay = replay_h_group_inner(
        &deductions,
        HGroupProfile::Max,
        PerspectiveDepth::ObserverOnly,
        false,
    );
    let green_information = convention_information_value(
        deductions.view(),
        HGroupProfile::Max,
        &replay,
        PlayerId::new(0),
        Clue::Suit(Suit::Green),
        &[CardId::new(3)],
    );
    let four_information = convention_information_value(
        deductions.view(),
        HGroupProfile::Max,
        &replay,
        PlayerId::new(0),
        Clue::Rank(Rank::Four),
        &[CardId::new(3)],
    );
    assert_eq!(green_information.promised_action_certainty, 1);
    assert_eq!(
        green_information.promised_action_certainty,
        four_information.promised_action_certainty
    );
    assert!(green_information.future_clue_savings > 0);
    assert_eq!(
        green_information.future_clue_savings,
        four_information.future_clue_savings
    );
    assert!(
        green_information.weighted_eliminations > four_information.weighted_eliminations,
        "green's negative information must be more relevant to likely future play: green={green_information:?}, four={four_information:?}"
    );
    assert!(
        green_information > four_information,
        "green must win on convention-aware negative information, not only the color tie-break: green={green_information:?}, four={four_information:?}"
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(green)
    );
}

#[test]
fn recognizes_expert_replay_play_after_out_of_order_fix() {
    let before_fix = expert_replay_p4v0s415()
        .state_at_turn(6)
        .expect("turn exists");
    let before_fix = LogicalDeductions::new(
        before_fix
            .view_for(PlayerId::new(0))
            .expect("connection actor exists"),
    )
    .expect("valid deductions");
    let before_fix = replay_h_group_inner(
        &before_fix,
        HGroupProfile::Max,
        PerspectiveDepth::ObserverOnly,
        false,
    );
    assert!(
        before_fix.pending_connections.iter().any(|connection| {
            connection.actor == PlayerId::new(0)
                && connection.cards == [CardId::new(3)]
                && connection.expected == Card::new(Suit::Red, Rank::One)
        }),
        "{before_fix:#?}"
    );

    let state = expert_replay_p4v0s415()
        .state_at_turn(8)
        .expect("turn exists");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("current player exists"),
    )
    .expect("valid deductions");
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(1)))
    );
}

#[test]
fn recognizes_expert_replay_priority_blue_three_line() {
    let replay = expert_replay_p4v0s415();
    let turn_nine = replay.state_at_turn(9).expect("turn exists");
    let deductions = LogicalDeductions::new(
        turn_nine
            .view_for(turn_nine.current_player())
            .expect("current player exists"),
    )
    .expect("valid deductions");
    let after = prospective_clue_view(
        deductions.view(),
        PlayerId::new(3),
        Clue::Rank(Rank::Three),
        &[CardId::new(18)],
    );
    let (projected, replay_state) =
        projected_h_group_replay(&after, HGroupProfile::Max, PlayerId::new(3)).unwrap();
    let recipient = infer_h_group_from_replay(&projected, replay_state, HGroupProfile::Max);
    assert!(
        recipient.playable_now.contains(&CardId::new(18)),
        "{recipient:#?}"
    );
    assert_eq!(
        prospective_clue_hazard(
            deductions.view(),
            HGroupProfile::Max,
            PlayerId::new(3),
            CardId::new(18),
            Clue::Rank(Rank::Three),
            &[CardId::new(18)],
            true,
        ),
        None,
        "{recipient:#?}"
    );
    assert!(
        h_group_clue_candidates(&deductions, HGroupProfile::Max)
            .iter()
            .any(|candidate| candidate.action
                == Action::Clue {
                    target: PlayerId::new(3),
                    clue: Clue::Rank(Rank::Three),
                }),
        "the expert Priority clue must remain a valid candidate: {:#?}",
        h_group_clue_candidates(&deductions, HGroupProfile::Max),
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(3),
            clue: Clue::Rank(Rank::Three),
        }),
        "the nearer unoccupied teammate should be loaded before a later play clue: {:#?}",
        h_group_clue_candidates(&deductions, HGroupProfile::Max),
    );

    let turn_eleven = replay.state_at_turn(11).expect("turn exists");
    let deductions = LogicalDeductions::new(
        turn_eleven
            .view_for(turn_eleven.current_player())
            .expect("current player exists"),
    )
    .expect("valid deductions");
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(18)))
    );

    let turn_thirteen = replay.state_at_turn(13).expect("turn exists");
    let deductions = LogicalDeductions::new(
        turn_thirteen
            .view_for(turn_thirteen.current_player())
            .expect("current player exists"),
    )
    .expect("valid deductions");
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(4))),
        "a Load Clue must cancel the provisional Priority Finesse: {:#?}",
        infer_h_group(&deductions, HGroupProfile::Max),
    );
}

#[test]
fn recognizes_expert_replay_red_three_continuation() {
    let state = expert_replay_p4v0s415()
        .state_at_turn(14)
        .expect("turn exists");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("current player exists"),
    )
    .expect("valid deductions");
    let replay = replay_h_group_inner(
        &deductions,
        HGroupProfile::Max,
        PerspectiveDepth::ObserverOnly,
        false,
    );
    assert!(
        replay.pending_connections.iter().any(|connection| {
            connection.actor == PlayerId::new(2)
                && connection.cards.contains(&CardId::new(9))
                && connection.expected == Card::new(Suit::Red, Rank::Three)
        }),
        "{replay:#?}"
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(9)))
    );
}

#[test]
fn optimal_replay_move_26_compares_directness_and_team_tempo() {
    let state = expert_replay_p4v0s415()
        .state_at_turn(25)
        .expect("turn exists");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("current player exists"),
    )
    .expect("valid deductions");
    let yellow = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Suit(Suit::Yellow),
    };
    let blue = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Suit(Suit::Blue),
    };
    let five = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Rank(Rank::Five),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    for action in [yellow, blue, five] {
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.action == action),
            "expected move-26 candidate {action:?}: {candidates:#?}"
        );
    }
    let score = |action| {
        candidates
            .iter()
            .find(|candidate| candidate.action == action)
            .expect("candidate exists")
            .score()
    };
    assert!(
        score(yellow) > score(five),
        "Directness must prefer the direct route to the same Y4/Y5 outcome: {candidates:#?}"
    );
    assert!(
        score(yellow) > score(blue),
        "team action coverage must beat an unneeded clue refund: {candidates:#?}"
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(yellow),
        "the direct yellow line should cover the team's next useful actions: {candidates:#?}"
    );
}

#[test]
fn optimal_replay_move_29_plays_into_the_visible_yellow_five() {
    let state = expert_replay_p4v0s415()
        .state_at_turn(28)
        .expect("turn exists");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("current player exists"),
    )
    .expect("valid deductions");

    assert_eq!(
        ordered_playable_cards(
            deductions.view(),
            &infer_h_group(&deductions, HGroupProfile::Max),
            HGroupProfile::Max,
        )
        .first()
        .copied(),
        Some(CardId::new(31)),
        "yellow 4 has Priority because it leads into the visible yellow 5",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(31))),
    );
}

#[test]
fn recognizes_expert_replay_rank_four_fill_in_after_green_fix() {
    let state = expert_replay_p4v0s415()
        .state_at_turn(15)
        .expect("turn exists");
    let recipient_baseline = LogicalDeductions::new(
        state
            .view_for(PlayerId::new(0))
            .expect("clue recipient exists"),
    )
    .expect("valid recipient deductions");
    let recipient_baseline = replay_h_group_inner(
        &recipient_baseline,
        HGroupProfile::Max,
        PerspectiveDepth::ObserverOnly,
        false,
    );
    assert!(
        recipient_baseline
            .pending_connections
            .iter()
            .any(|connection| {
                connection.actor == PlayerId::new(3)
                    && connection.expected == Card::new(Suit::Yellow, Rank::One)
                    && connection.focus == CardId::new(2)
            }),
        "{recipient_baseline:#?}"
    );
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("current player exists"),
    )
    .expect("valid deductions");
    let expected = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Rank(Rank::Four),
    };
    let after = prospective_clue_view(
        deductions.view(),
        PlayerId::new(0),
        Clue::Rank(Rank::Four),
        &[CardId::new(3)],
    );
    let (projected, projected_replay) =
        projected_h_group_replay(&after, HGroupProfile::Max, PlayerId::new(0)).unwrap();
    let recipient =
        infer_h_group_from_replay(&projected, projected_replay.clone(), HGroupProfile::Max);
    assert!(recipient.cards.iter().any(|card| {
        card.card == CardId::new(3)
            && card.identities == IdentitySet::singleton(Card::new(Suit::Green, Rank::Four))
    }));
    assert_eq!(
        prospective_clue_hazard(
            deductions.view(),
            HGroupProfile::Max,
            PlayerId::new(0),
            CardId::new(3),
            Clue::Rank(Rank::Four),
            &[CardId::new(3)],
            false,
        ),
        None,
        "recipient={recipient:#?}; replay={projected_replay:#?}"
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(expected),
        "candidates={:#?}; inferred={:#?}",
        h_group_clue_candidates(&deductions, HGroupProfile::Max),
        infer_h_group(&deductions, HGroupProfile::Max),
    );
}

#[test]
fn recognizes_expert_replay_releases_unused_layer_candidate() {
    let state = expert_replay_p4v0s415()
        .state_at_turn(16)
        .expect("turn exists");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("current player exists"),
    )
    .expect("valid deductions");
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    assert!(
        !inferred.gotten().contains(&CardId::new(0)),
        "{inferred:#?}"
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Discard(CardId::new(0)))
    );
}

#[test]
fn recognizes_expert_replay_green_layer_receiver() {
    let state = expert_replay_p4v0s415()
        .state_at_turn(17)
        .expect("turn exists");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("current player exists"),
    )
    .expect("valid deductions");
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(22))),
        "ordered={:#?}; inferred={:#?}",
        ordered_h_group_actions(&deductions, HGroupProfile::Max),
        infer_h_group(&deductions, HGroupProfile::Max),
    );
}

#[test]
fn recognizes_expert_replay_delayed_focus_after_connectors() {
    let state = expert_replay_p4v0s415()
        .state_at_turn(19)
        .expect("turn exists");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("current player exists"),
    )
    .expect("valid deductions");
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(17))),
        "{:#?}",
        infer_h_group(&deductions, HGroupProfile::Max),
    );
}

#[test]
fn recognizes_expert_replay_second_green_layer_card() {
    let state = expert_replay_p4v0s415()
        .state_at_turn(21)
        .expect("turn exists");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("current player exists"),
    )
    .expect("valid deductions");
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(7))),
        "{:#?}",
        infer_h_group(&deductions, HGroupProfile::Max),
    );
}

#[test]
fn recognizes_expert_replay_priority_finesse_on_later_player() {
    let state = expert_replay_p4v0s415()
        .state_at_turn(22)
        .expect("turn exists");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("current player exists"),
    )
    .expect("valid deductions");
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(23))),
        "{:#?}",
        infer_h_group(&deductions, HGroupProfile::Max),
    );
}

#[test]
fn recognizes_expert_replay_yellow_two_priority() {
    let state = expert_replay_p4v0s415()
        .state_at_turn(24)
        .expect("turn exists");
    let deductions = LogicalDeductions::new(
        state
            .view_for(state.current_player())
            .expect("current player exists"),
    )
    .expect("valid deductions");
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    assert_eq!(
        ordered_playable_cards(deductions.view(), &inferred, HGroupProfile::Max),
        vec![CardId::new(2), CardId::new(3)],
        "yellow 2 leads into the clued yellow 3: {inferred:#?}",
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(2))),
    );
}

#[test]
fn fixer_understands_expert_red_four_lie() {
    let state = expert_replay_p4v0s415()
        .state_at_turn(6)
        .expect("turn exists");
    let deductions =
        LogicalDeductions::new(state.view_for(PlayerId::new(2)).expect("fixer exists"))
            .expect("valid deductions");
    let replay = replay_h_group_inner(
        &deductions,
        HGroupProfile::Max,
        PerspectiveDepth::ObserverOnly,
        false,
    );
    let clue = replay
        .clues
        .iter()
        .find(|clue| clue.turn == 5)
        .expect("red clue interpreted");
    assert!(
        clue.play_identities
            .contains(Card::new(Suit::Red, Rank::Four)),
        "{clue:#?}"
    );
    assert_eq!(
        replay.required_fixes.iter().collect::<Vec<_>>(),
        vec![FixObligation {
            condition: FixCondition::Unconditional,
            required: RequiredFix {
                actor: PlayerId::new(2),
                target: PlayerId::new(0),
                focus: CardId::new(3),
                identity: Card::new(Suit::Green, Rank::Four),
            },
        }],
        "{replay:#?}",
    );
}

#[test]
fn recipient_keeps_the_red_four_repair_branch_conditional() {
    let state = expert_replay_p4v0s415()
        .state_at_turn(6)
        .expect("turn exists");
    let deductions =
        LogicalDeductions::new(state.view_for(PlayerId::new(3)).expect("recipient exists"))
            .expect("valid deductions");
    let replay = replay_h_group_inner(
        &deductions,
        HGroupProfile::Max,
        PerspectiveDepth::ObserverOnly,
        false,
    );

    assert!(
        replay.required_fixes.iter().any(|obligation| {
            obligation.required
                == RequiredFix {
                    actor: PlayerId::new(2),
                    target: PlayerId::new(0),
                    focus: CardId::new(3),
                    identity: Card::new(Suit::Green, Rank::Four),
                }
                && obligation.condition
                    == FixCondition::FocusIdentity {
                        clue_turn: 5,
                        focus: CardId::new(17),
                        identity: Card::new(Suit::Red, Rank::Four),
                    }
        }),
        "{replay:#?}"
    );
}
