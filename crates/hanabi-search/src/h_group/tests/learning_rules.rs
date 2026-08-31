#[test]
fn learning_path_metadata_covers_every_cumulative_level() {
    assert_eq!(H_GROUP_LEVELS.len(), 26);
    for (index, descriptor) in H_GROUP_LEVELS.iter().enumerate() {
        assert_eq!(usize::from(descriptor.profile.effective_level()), index + 1);
        assert!(!descriptor.title.is_empty());
        assert!(!descriptor.effects.is_empty());
    }
    assert!(
        H_GROUP_LEVELS[9]
            .effects
            .contains(&HGroupMoveKind::Directness)
    );
    assert_eq!(HGroupProfile::Max.effective_level(), 26);
}

#[test]
fn level_thirteen_admits_the_opening_hard_three_self_bluff() {
    let fixture = expert_replay_p4v0s9();
    let state = fixture.state_at_turn(0).expect("opening position exists");
    let actor = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(actor).expect("actor has a view"))
        .expect("valid deductions");
    let bluff = replay_action_at_turn(&fixture, 0);
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    assert!(
        candidates.iter().any(|candidate| candidate.action == bluff),
        "Hard 3 Self-Bluff must be a convention-valid candidate: {candidates:#?}"
    );
    let after = fixture.state_at_turn(1).expect("Bluff clue is legal");
    let recipient = after.current_player();
    let recipient_deductions =
        LogicalDeductions::new(after.view_for(recipient).expect("recipient has a view"))
            .expect("valid recipient deductions");
    let inferred = infer_h_group(&recipient_deductions, HGroupProfile::Max);
    assert!(
        inferred.playable_now.contains(&CardId::new(7)),
        "Self-Bluff must tell Bob to blind-play his Finesse Position: {inferred:#?}"
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(bluff),
        "the three-card Bluff must beat the ordinary rank-1 clue: {candidates:#?}"
    );

    let after_blind_play = fixture
        .state_at_turn(2)
        .expect("Bluff continuation is legal");
    let cathy = after_blind_play.current_player();
    let cathy_deductions =
        LogicalDeductions::new(after_blind_play.view_for(cathy).expect("Cathy has a view"))
            .expect("valid Cathy deductions");
    let cathy_replay = replay_h_group(&cathy_deductions, HGroupProfile::Max);
    assert!(
        cathy_replay.required_fixes.iter().next().is_none(),
        "a resolved Bluff must not leave an ordinary-connection Fix obligation"
    );
    let cathy_candidates = h_group_clue_candidates(&cathy_deductions, HGroupProfile::Max);
    assert_eq!(
        select_h_group_action(&cathy_deductions, HGroupProfile::Max),
        Some(replay_action_at_turn(&fixture, 2)),
        "Cathy should give the efficient rank-1 clue after the Bluff resolves: {cathy_candidates:#?}"
    );

    let after_multi_one = fixture
        .state_at_turn(3)
        .expect("multi-one clue is legal");
    let donald = after_multi_one.current_player();
    let donald_deductions =
        LogicalDeductions::new(after_multi_one.view_for(donald).expect("Donald has a view"))
            .expect("valid Donald deductions");
    let donald_replay = replay_h_group(&donald_deductions, HGroupProfile::Max);
    let donald_inferences =
        infer_h_group_from_replay(&donald_deductions, donald_replay.clone(), HGroupProfile::Max);
    assert!(donald_inferences.playable_now.contains(&CardId::new(12)));
    assert!(donald_inferences.playable_now.contains(&CardId::new(13)));
    assert_eq!(
        select_h_group_action(&donald_deductions, HGroupProfile::Max),
        Some(replay_action_at_turn(&fixture, 3)),
        "Donald should prefer the playable 1 that advances the opening line"
    );
}

#[test]
fn second_replay_order_chop_move_protects_alices_red_two() {
    let fixture = expert_replay_p4v0s9();
    let after_skip = fixture
        .state_at_turn(4)
        .expect("the out-of-order red-1 play is legal");
    let alice = after_skip.current_player();
    let deductions =
        LogicalDeductions::new(after_skip.view_for(alice).expect("Alice has a view"))
            .expect("valid Alice deductions");
    let replay = replay_h_group(&deductions, HGroupProfile::Max);

    assert!(
        replay.cards.chop_moved.contains(&CardId::new(0)),
        "signals: {:#?}",
        replay.signals
    );
    assert!(replay.signals.iter().any(|signal| {
        signal.turn == 3
            && signal.actor == PlayerId::new(3)
            && signal.target == Some(PlayerId::new(0))
            && signal.kind == HGroupMoveKind::OrderChopMove
            && signal.cards == [CardId::new(0)]
    }));
    let order_chop_proposal = replay
        .transitions
        .iter()
        .find(|transition| transition.turn == 3)
        .and_then(|transition| {
            transition.proposals.iter().find(|proposal| {
                replay
                    .signals
                    .get(proposal.signal_range.clone())
                    .is_some_and(|signals| {
                        signals
                            .iter()
                            .any(|signal| signal.kind == HGroupMoveKind::OrderChopMove)
                    })
            })
        })
        .expect("the OCM has a causal rule proposal");
    assert_eq!(order_chop_proposal.rule, HGroupRuleId::ChopMoves);
    assert!(
        order_chop_proposal
            .mutations
            .contains(MutationDomain::ChopMovement)
    );
    let alice_candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(replay_action_at_turn(&fixture, 4)),
        "Alice must extinguish the available Play Clue before ending the Early Game: {alice_candidates:#?}",
    );

    let bob_turn = fixture
        .state_at_turn(5)
        .expect("Alice's purple clue is legal");
    let bob = bob_turn.current_player();
    let bob_deductions =
        LogicalDeductions::new(bob_turn.view_for(bob).expect("Bob has a view"))
            .expect("valid Bob deductions");
    let bob_candidates = h_group_clue_candidates(&bob_deductions, HGroupProfile::Max);
    assert_eq!(
        select_h_group_action(&bob_deductions, HGroupProfile::Max),
        Some(replay_action_at_turn(&fixture, 5)),
        "the rank-2 clue must load Alice's green 2 behind Donald's promised green 1; candidates: {bob_candidates:#?}",
    );
}

#[test]
fn second_replay_move_eight_keeps_the_promised_green_one() {
    let fixture = expert_replay_p4v0s9();
    let state = fixture.state_at_turn(7).expect("move-8 position exists");
    let donald = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(donald).expect("Donald has a view"))
        .expect("valid Donald deductions");
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(12))),
        "an unrelated clue must not preempt Donald's promised green 1: {candidates:#?}"
    );
}

#[test]
fn clue_giver_does_not_prompt_their_own_ambiguous_rank_three() {
    let fixture = expert_replay_p4v0s9();
    let state = fixture
        .state_at_turn(13)
        .expect("the second replay prefix is legal");
    let bob = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(bob).expect("Bob has a view"))
        .expect("valid Bob deductions");
    let replay = replay_h_group(&deductions, HGroupProfile::Max);

    assert!(
        replay.pending_connections.iter().all(|connection| {
            connection.actor != bob || !connection.cards.contains(&CardId::new(5))
        }),
        "Bob cannot knowingly use his merely-compatible rank 3 as his own Prompt: {:#?}",
        replay.pending_connections
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(replay_action_at_turn(&fixture, 13))
    );
}

#[test]
fn second_replay_direct_blue_two_play_clue_persists_until_played() {
    let fixture = expert_replay_p4v0s9();
    let donald = PlayerId::new(3);
    let blue_two = Card::new(Suit::Blue, Rank::Two);
    let exact_blue_two = IdentitySet::singleton(blue_two);

    let after_clue = fixture
        .state_at_turn(14)
        .expect("the direct blue Play Clue is legal");
    let clue_deductions = LogicalDeductions::new(
        after_clue
            .view_for(donald)
            .expect("Donald has a post-clue view"),
    )
    .expect("valid post-clue deductions");
    let clue_replay = replay_h_group(&clue_deductions, HGroupProfile::Max);
    assert!(
        !clue_replay
            .signals
            .has_at_turn(13, HGroupMoveKind::FixClue),
        "a fresh blue clue on Donald's blue 2 is a direct Play Clue, not a Fix: {:#?}",
        clue_replay.signals
    );
    let clue_inferences = infer_h_group(&clue_deductions, HGroupProfile::Max);
    let direct_clue = clue_inferences
        .clues
        .iter()
        .find(|clue| clue.turn == 13 && clue.target == donald)
        .expect("move 14 has a recipient-visible clue interpretation");
    assert_eq!(
        direct_clue.kind,
        HGroupClueKind::Play,
        "{direct_clue:#?}"
    );
    assert_eq!(direct_clue.focus, CardId::new(15));
    assert_eq!(direct_clue.focus_identities, exact_blue_two);
    assert_eq!(
        clue_inferences
            .cards
            .iter()
            .find(|card| card.card == CardId::new(15))
            .map(|card| card.identities),
        Some(exact_blue_two),
        "the recipient must immediately record the direct Play Clue as blue 2: {clue_inferences:#?}"
    );

    let play_turn = fixture
        .state_at_turn(27)
        .expect("the position before move 28 exists");
    assert_eq!(play_turn.current_player(), donald);
    let play_deductions = LogicalDeductions::new(
        play_turn
            .view_for(donald)
            .expect("Donald has a move-28 view"),
    )
    .expect("valid move-28 deductions");
    let play_inferences = infer_h_group(&play_deductions, HGroupProfile::Max);
    assert_eq!(
        play_inferences
            .cards
            .iter()
            .find(|card| card.card == CardId::new(15))
            .map(|card| card.identities),
        Some(exact_blue_two),
        "later clues must not widen the exact blue-2 Play promise: {play_inferences:#?}"
    );
    assert!(play_inferences.playable_now.contains(&CardId::new(15)));
    assert_eq!(
        select_h_group_action(&play_deductions, HGroupProfile::Max),
        Some(replay_action_at_turn(&fixture, 27))
    );
}

#[test]
fn second_replay_move_thirty_one_can_defer_to_a_more_efficient_clue() {
    let fixture = expert_replay_p4v0s9();
    let state = fixture
        .state_at_turn(30)
        .expect("the position before move 31 exists");
    let cathy = state.current_player();
    let view = state.view_for(cathy).expect("Cathy has a move-31 view");
    let deductions = LogicalDeductions::new(view.clone()).expect("valid Cathy deductions");
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let green = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Suit(Suit::Green),
    };
    assert_eq!(
        candidates
            .iter()
            .find(|candidate| candidate.action == green)
            .map(|candidate| candidate.action_coverage()),
        Some(2),
        "green promises green 3 and green 4: {candidates:#?}"
    );
    let current_rank_four = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Rank(Rank::Four),
    };
    assert_eq!(
        candidates
            .iter()
            .find(|candidate| candidate.action == current_rank_four)
            .map(|candidate| candidate.action_coverage()),
        Some(2),
        "Cathy sees Donald's nearer green 3, so her rank-4 clue is only a two-action line: {candidates:#?}"
    );

    let after_discard = ProspectiveTransition::discard(
        &view,
        cathy,
        CardId::new(8),
        Card::new(Suit::Red, Rank::One),
    );
    let donald = after_discard.current_player;
    let (donald_deductions, donald_replay) = PerspectiveProjector::new(
        &after_discard,
        HGroupProfile::Max,
    )
    .project(donald, PerspectiveDepth::NestedRecipients)
    .expect("Donald can see the revealed discard");
    let next_candidates = h_group_clue_candidates_from_replay(
        &donald_deductions,
        HGroupProfile::Max,
        &donald_replay,
    );
    let rank_four = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Rank(Rank::Four),
    };
    let touched = after_discard.hands[0]
        .iter()
        .filter(|card| {
            card.identity
                .is_some_and(|identity| Clue::Rank(Rank::Four).matches(identity))
        })
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let after_rank_four = prospective_clue_view(
        donald_deductions.view(),
        PlayerId::new(0),
        Clue::Rank(Rank::Four),
        &touched,
    );
    let (_, alice_replay) = projected_h_group_replay(
        &after_rank_four,
        HGroupProfile::Max,
        PlayerId::new(0),
    )
    .expect("Alice can interpret rank 4");
    assert_eq!(
        next_candidates
            .iter()
            .find(|candidate| candidate.action == rank_four)
            .map(|candidate| candidate.action_coverage()),
        Some(3),
        "Donald sees the blue-3, green-3, green-4 Layered Finesse: candidates={next_candidates:#?}; pending={:#?}; signals={:#?}",
        alice_replay.pending_connections,
        alice_replay.signals
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Discard(CardId::new(8))),
        "Cathy should recover the last clue so Donald can give the three-action Layered Finesse"
    );
}

#[test]
fn second_replay_move_thirty_three_admits_the_rank_five_clue() {
    let fixture = expert_replay_p4v0s9();
    let state = fixture
        .state_at_turn(32)
        .expect("the position before move 33 exists");
    let alice = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(alice).expect("Alice has a view"))
        .expect("valid Alice deductions");
    let replay = replay_h_group(&deductions, HGroupProfile::Max);
    let candidates = h_group_clue_candidates_from_replay(
        &deductions,
        HGroupProfile::Max,
        &replay,
    );
    let rank_five = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Rank(Rank::Five),
    };
    let admitted = candidates
        .iter()
        .map(|candidate| candidate.action)
        .collect::<Vec<_>>();
    let rejected = h_group_rejected_clues_from_replay(
        &deductions,
        HGroupProfile::Max,
        &replay,
        &admitted,
    );
    assert!(
        admitted.contains(&rank_five),
        "rank 5 must be admitted: rejected={rejected:#?}; state={state:#?}; replay={replay:#?}; candidates={candidates:#?}"
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(rank_five),
        "rank 5 should preempt Alice's parked yellow 4: replay={replay:#?}; candidates={candidates:#?}"
    );

    let after_save = fixture
        .state_at_turn(33)
        .expect("the rank-5 Save is legal");
    let bob = after_save.current_player();
    let bob_deductions = LogicalDeductions::new(after_save.view_for(bob).expect("Bob has a view"))
        .expect("valid Bob deductions");
    let bob_replay = replay_h_group(&bob_deductions, HGroupProfile::Max);
    let bob_inferences = infer_h_group_from_replay(
        &bob_deductions,
        bob_replay.clone(),
        HGroupProfile::Max,
    );
    assert!(
        bob_replay.pending_connections.iter().any(|connection| {
            connection.actor == bob
                && connection.cards.starts_with(&[CardId::new(34), CardId::new(30)])
                && connection.expected == Card::new(Suit::Green, Rank::Three)
        }),
        "Alice's unrelated Save must preserve Bob's layered obligation: {bob_replay:#?}"
    );
    assert!(
        bob_inferences.playable_now.contains(&CardId::new(34)),
        "Bob must play the first layer, blue 3: {bob_inferences:#?}"
    );
}

#[test]
fn second_replay_move_thirty_eight_keeps_the_layered_green_three_consistent() {
    let fixture = expert_replay_p4v0s9();
    let state = fixture
        .state_at_turn(37)
        .expect("the position before move 38 exists");
    let bob = state.current_player();
    let view = state.view_for(bob).expect("Bob has a view");
    let information_set = crate::InformationSet::new(&view).expect("valid information set");
    let deductions = information_set.deductions();
    let analysis = crate::SupportedConvention::HGroup(HGroupProfile::Max).analyze(deductions);
    let logical = view.hands[bob.index()]
        .iter()
        .map(|card| (card.id, deductions.possible_identities(card.id)))
        .collect::<Vec<_>>();
    let count = information_set.world_count_up_to(&analysis.belief_constraints, 4_096);
    assert!(
        count.worlds() > 0,
        "the demonstrated blue-3/green-3 layer must have a consistent world: logical={logical:#?}; constraints={:#?}",
        analysis.belief_constraints,
    );
}

#[test]
fn second_replay_move_forty_one_keeps_the_rank_four_focus_due() {
    let fixture = expert_replay_p4v0s9();
    let state = fixture
        .state_at_turn(40)
        .expect("the position before move 41 exists");
    let alice = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(alice).expect("Alice has a view"))
        .expect("valid Alice deductions");
    let replay = replay_h_group(&deductions, HGroupProfile::Max);
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    let green_four = inferred
        .cards
        .iter()
        .find(|card| card.card == CardId::new(20));

    assert!(
        inferred.playable_now.contains(&CardId::new(20)),
        "Donald's rank-4 clue promised the focused green 4 after yellow 4; note={green_four:#?}; clue={:#?}; pending={:#?}; signals={:#?}",
        replay.clues.iter().find(|clue| clue.turn == 31),
        replay.pending_connections,
        replay.signals.iter().filter(|signal| signal.turn == 31).collect::<Vec<_>>(),
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(20))),
    );
}

#[test]
fn second_replay_move_forty_two_prefers_both_playable_fives() {
    let fixture = expert_replay_p4v0s9();
    let state = fixture
        .state_at_turn(41)
        .expect("the position before move 42 exists");
    let bob = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(bob).expect("Bob has a view"))
        .expect("valid Bob deductions");
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let rank_five = Action::Clue {
        target: PlayerId::new(3),
        clue: Clue::Rank(Rank::Five),
    };
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(rank_five),
        "rank 5 directly gives Donald both playable 5s: {candidates:#?}"
    );
}

#[test]
fn second_replay_move_forty_four_plays_the_chop_focused_five_first() {
    let fixture = expert_replay_p4v0s9();
    let state = fixture
        .state_at_turn(43)
        .expect("the position before move 44 exists");
    let donald = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(donald).expect("Donald has a view"))
        .expect("valid Donald deductions");
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(21))),
        "the chop-focused green 5 must precede the collateral red 5: {inferred:#?}"
    );
}

#[test]
fn purple_to_donald_does_not_promise_cathys_ambiguous_purple_four() {
    let fixture = expert_replay_p4v0s9();
    let state = fixture
        .state_at_turn(9)
        .expect("the second replay prefix is legal");
    let bob = state.current_player();
    let view = state.view_for(bob).expect("Bob has a view");
    let donald = PlayerId::new(3);
    let purple = Clue::Suit(Suit::Purple);
    let touched = view.hands[donald.index()]
        .iter()
        .filter(|card| card.identity.is_some_and(|identity| purple.matches(identity)))
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let after = prospective_clue_view(&view, donald, purple, &touched);
    let cathy = PlayerId::new(2);
    let (cathy_deductions, cathy_replay) =
        projected_h_group_replay(&after, HGroupProfile::Max, cathy)
            .expect("Cathy can interpret the prospective clue");
    let inferred = infer_h_group_from_replay(
        &cathy_deductions,
        cathy_replay,
        HGroupProfile::Max,
    );
    let purple_card = inferred
        .cards
        .iter()
        .find(|note| note.card == CardId::new(9))
        .expect("Cathy retains a note for her purple card");

    assert!(
        purple_card
            .identities
            .contains(Card::new(Suit::Purple, Rank::Four))
    );
    assert!(
        purple_card
            .identities
            .contains(Card::new(Suit::Purple, Rank::Five))
    );
    assert!(!inferred.playable_now.contains(&CardId::new(9)));
}

#[test]
fn second_replay_rank_four_secures_more_than_purple_to_donald() {
    let fixture = expert_replay_p4v0s9();
    let state = fixture
        .state_at_turn(9)
        .expect("the second replay prefix is legal");
    let bob = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(bob).expect("Bob has a view"))
        .expect("valid Bob deductions");
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(replay_action_at_turn(&fixture, 9)),
        "rank 4 uniquely secures Cathy's purple 4 and must not receive a Directness penalty: {candidates:#?}"
    );
}

#[test]
fn second_replay_rank_one_trash_chop_move_is_admitted() {
    let fixture = expert_replay_p4v0s9();
    let state = fixture
        .state_at_turn(14)
        .expect("the second replay prefix is legal");
    let cathy = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(cathy).expect("Cathy has a view"))
        .expect("valid Cathy deductions");
    let alice_playable = crate::h_group::prospective::subjective_playable_cards(
        deductions.view(),
        HGroupProfile::Max,
        PlayerId::new(0),
    )
    .expect("Alice has a subjective convention projection");
    let alice_cards = crate::h_group::prospective::subjective_convention_cards(
        deductions.view(),
        HGroupProfile::Max,
        PlayerId::new(0),
    )
    .expect("Alice has subjective card notes");
    let alice_two = alice_cards
        .iter()
        .find(|card| card.card == CardId::new(0))
        .expect("Alice retains her rank-2 note");
    assert!(
        !alice_two
            .identities
            .contains(Card::new(Suit::Purple, Rank::Two)),
        "Donald's played purple 2 disproves Alice's provisional purple-2 Prompt: {alice_two:#?}"
    );
    assert!(
        alice_playable.contains(&CardId::new(0)),
        "Alice's rank-2 card is already playing before Cathy acts: playable={alice_playable:?}, cards={alice_cards:#?}"
    );
    let bob_playable = crate::h_group::prospective::subjective_playable_cards(
        deductions.view(),
        HGroupProfile::Max,
        PlayerId::new(1),
    )
    .expect("Bob has a subjective convention projection");
    let bob_cards = crate::h_group::prospective::subjective_convention_cards(
        deductions.view(),
        HGroupProfile::Max,
        PlayerId::new(1),
    )
    .expect("Bob has subjective card notes");
    let bob_three = bob_cards
        .iter()
        .find(|card| card.card == CardId::new(5))
        .expect("Bob retains his red-3 note");
    assert!(
        bob_three
            .identities
            .contains(Card::new(Suit::Red, Rank::Three)),
        "Bob's rank-3 possibilities still include its visible red-3 identity: {bob_three:#?}"
    );
    assert!(
        !bob_playable.contains(&CardId::new(5)),
        "Bob's red 3 is not already playing: {bob_playable:?}"
    );
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let redundant_red_fill_in = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Suit(Suit::Red),
    };
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.action != redundant_red_fill_in),
        "red to Bob cannot manufacture a Prompt from Alice's already-playing red 2: {candidates:#?}"
    );
    assert!(
        candidates.iter().any(|candidate| {
            candidate.action == replay_action_at_turn(&fixture, 14)
                && candidate.recognition() == ClueRecognition::RecipientReplay
        }),
        "rank 1 on Alice's trash purple 1 must be admitted as a Trash Chop Move: {candidates:#?}"
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(replay_action_at_turn(&fixture, 14)),
        "the Trash Chop Move is the best remaining legal clue: {candidates:#?}"
    );
}

#[test]
fn second_replay_off_chop_trash_moves_both_fours_before_move_twenty() {
    let fixture = expert_replay_p4v0s9();
    let state = fixture
        .state_at_turn(19)
        .expect("the second replay prefix is legal");
    let donald = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(donald).expect("Donald has a view"))
        .expect("valid deductions");
    let replay = replay_h_group(&deductions, HGroupProfile::Max);

    assert!(replay.cards.chop_moved.contains(&CardId::new(2)));
    assert!(replay.cards.chop_moved.contains(&CardId::new(20)));
    assert!(replay.signals.iter().any(|signal| {
        signal.turn == 14
            && signal.kind == HGroupMoveKind::TrashChopMove
            && signal.cards == [CardId::new(20), CardId::new(2)]
    }));
    let gotten = replay.gotten_from(&replay.promptable());
    assert_eq!(
        chop(&replay.hands[0], &gotten),
        Some(CardId::new(24)),
        "Alice's green 1 is chop after both 4s are Chop Moved"
    );
    let rank_four_touched = [CardId::new(2), CardId::new(20)];
    assert_eq!(
        focus(
            &replay.hands[0],
            &rank_four_touched,
            chop(&replay.hands[0], &gotten),
            &gotten,
        ),
        Some(CardId::new(20)),
        "a rank-4 reclue is leftmost-focused on Alice's green 4"
    );
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let illegal_rank_four = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Rank(Rank::Four),
    };
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.action != illegal_rank_four),
        "rank 4 cannot borrow the Yellow-4 Reverse Finesse when it focuses green 4: {candidates:#?}"
    );
}

#[test]
fn second_replay_move_twenty_admits_the_visible_reverse_finesse() {
    let fixture = expert_replay_p4v0s9();
    let state = fixture
        .state_at_turn(19)
        .expect("the second replay prefix is legal");
    let donald = state.current_player();
    let view = state.view_for(donald).expect("Donald has a view");
    let deductions = LogicalDeductions::new(view.clone()).expect("valid deductions");
    let expected = replay_action_at_turn(&fixture, 19);
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let touched = view.hands[0]
        .iter()
        .filter(|card| {
            card.identity
                .is_some_and(|identity| Clue::Suit(Suit::Yellow).matches(identity))
        })
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let after = prospective_clue_view(
        &view,
        PlayerId::new(0),
        Clue::Suit(Suit::Yellow),
        &touched,
    );
    let (_recipient_deductions, recipient_replay) = projected_h_group_replay(
        &after,
        HGroupProfile::Max,
        PlayerId::new(0),
    )
    .expect("recipient replay succeeds");
    let interpretation = recipient_replay
        .clues
        .last()
        .expect("the prospective clue has an interpretation");
    let yellow_four = Card::new(Suit::Yellow, Rank::Four);
    assert_eq!(interpretation.kind, HGroupClueKind::Play);
    assert_eq!(interpretation.focus, CardId::new(2));
    assert_eq!(
        interpretation.focus_identities,
        IdentitySet::singleton(yellow_four)
    );
    let new_connections = recipient_replay
        .pending_connections
        .iter()
        .filter(|connection| connection.focus == CardId::new(2))
        .map(|connection| {
            (
                connection.actor,
                connection.cards.clone(),
                connection.expected,
                connection.kind,
                connection.focus_identity,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        new_connections,
        vec![
            (
                PlayerId::new(2),
                vec![CardId::new(26)],
                Card::new(Suit::Yellow, Rank::Two),
                HGroupConnectionKind::Finesse,
                yellow_four,
            ),
            (
                PlayerId::new(1),
                vec![CardId::new(4)],
                Card::new(Suit::Yellow, Rank::Three),
                HGroupConnectionKind::Prompt,
                yellow_four,
            ),
        ]
    );
    assert!(recipient_replay.signals.iter().any(|signal| {
        signal.turn == 19
            && signal.kind == HGroupMoveKind::ReverseFinesse
            && signal.cards == [CardId::new(26)]
    }));
    assert_eq!(
        expected,
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        }
    );
    assert!(
        candidates.iter().any(|candidate| {
            candidate.action == expected
                && candidate.recognition() == ClueRecognition::RecipientReplay
                && candidate.score() == 373
        }),
        "the Yellow-2 Reverse Finesse must retain its full strategic value: {candidates:#?}"
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(expected),
        "the multi-player setup must be allowed to park Donald's blue 2: {candidates:#?}"
    );
}

#[test]
fn second_replay_move_twenty_two_rejects_a_good_touch_duplicate() {
    let fixture = expert_replay_p4v0s9();
    let state = fixture
        .state_at_turn(21)
        .expect("the second replay prefix is legal");
    let bob = state.current_player();
    let deductions =
        LogicalDeductions::new(state.view_for(bob).expect("Bob has a view"))
            .expect("valid deductions");
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    let existing_red_three = inferred
        .cards
        .iter()
        .find(|card| card.card == CardId::new(5))
        .expect("Bob retains the opening rank-3 card");
    assert!(
        existing_red_three
            .identities
            .contains(Card::new(Suit::Red, Rank::Three)),
        "Bob's own promised 3 can still be red 3, so he may not duplicate it"
    );

    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let duplicate_red = Action::Clue {
        target: PlayerId::new(3),
        clue: Clue::Suit(Suit::Red),
    };
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.action != duplicate_red),
        "red to Donald illegally duplicates Bob's promised red 3 ({existing_red_three:?}): {candidates:#?}"
    );

    let redundant_yellow = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Suit(Suit::Yellow),
    };
    let redundant_two = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Rank(Rank::Two),
    };
    assert!(
        candidates.iter().all(|candidate| {
            candidate.action != redundant_yellow && candidate.action != redundant_two
        }),
        "Cathy's yellow 2 is already playing from the move-20 Reverse Finesse, so a direct fill-in has no Minimum Clue Value: {candidates:#?}"
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(replay_action_at_turn(&fixture, 21)),
        "purple to Donald is the remaining valuable play clue: {candidates:#?}"
    );

    let after = fixture.state_at_turn(22).expect("move 22 is legal");
    let cathy = after.current_player();
    let cathy_deductions = LogicalDeductions::new(after.view_for(cathy).unwrap()).unwrap();
    let cathy_replay = replay_h_group(&cathy_deductions, HGroupProfile::Max);
    assert!(
        cathy_replay.pending_connections.iter().any(|connection| {
            connection.actor == cathy
                && connection.cards == [CardId::new(26)]
                && connection.expected == Card::new(Suit::Yellow, Rank::Two)
                && connection.focus == CardId::new(2)
                && connection.kind == HGroupConnectionKind::Finesse
        }),
        "the unrelated purple clue must preserve Cathy's move-20 Reverse Finesse: {:#?}",
        cathy_replay.pending_connections,
    );
    assert_eq!(
        select_h_group_action(&cathy_deductions, HGroupProfile::Max),
        Some(replay_action_at_turn(&fixture, 22)),
        "Cathy must blind-play yellow 2 before taking an unrelated discard",
    );
}

/// <https://hanabi.github.io/level-4/#the-5s-chop-move-5cm>
#[test]
fn second_replay_move_twenty_five_treats_rank_five_as_a_five_chop_move() {
    let fixture = expert_replay_p4v0s9();
    let state = fixture
        .state_at_turn(24)
        .expect("the second replay prefix is legal");
    let alice = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(alice).expect("Alice has a view"))
        .expect("valid Alice deductions");
    let rank_five = Action::Clue {
        target: PlayerId::new(3),
        clue: Clue::Rank(Rank::Five),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let kinds = prospective_clue_signal_kinds(
        deductions.view(),
        HGroupProfile::Max,
        PlayerId::new(3),
        Clue::Rank(Rank::Five),
        &[CardId::new(21), CardId::new(29)],
    );
    let after_five = prospective_clue_view(
        deductions.view(),
        PlayerId::new(3),
        Clue::Rank(Rank::Five),
        &[CardId::new(21), CardId::new(29)],
    );
    let (_, donald_replay) = projected_h_group_replay(
        &after_five,
        HGroupProfile::Max,
        PlayerId::new(3),
    )
    .expect("Donald has a prospective replay");
    let five_chop_move = donald_replay
        .signals
        .iter()
        .find(|signal| signal.turn == 24 && signal.kind == HGroupMoveKind::FiveChopMove)
        .expect("Donald must recognize the 5CM");
    assert_eq!(five_chop_move.cards, vec![CardId::new(14)]);
    assert!(
        kinds.contains(&HGroupMoveKind::FiveChopMove),
        "rank 5 to Donald must be interpreted as a 5CM on his red 3: {kinds:?}; candidates={candidates:#?}"
    );
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.action != rank_five),
        "the 5CM protects a duplicate red 3 that is already promised elsewhere, so it has no Minimum Clue Value: {candidates:#?}"
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(replay_action_at_turn(&fixture, 24)),
        "rank 4 to Cathy is the first convention-valid line after rejecting the valueless 5CM: {candidates:#?}",
    );
}

#[test]
fn second_replay_move_twenty_seven_keeps_the_rank_four_focus_due() {
    let fixture = expert_replay_p4v0s9();
    let state = fixture.state_at_turn(26).expect("fixture prefix is legal");
    let cathy = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(cathy).expect("Cathy has a view"))
        .expect("valid Cathy deductions");
    let replay = replay_h_group(&deductions, HGroupProfile::Max);
    let inferred = infer_h_group_from_replay(&deductions, replay.clone(), HGroupProfile::Max);

    assert!(
        inferred.playable_now.contains(&CardId::new(28)),
        "Bob's red-3 Prompt demonstrates that the rank-4 focus is red 4 and due: clues={:#?}; pending={:#?}; transitions={:#?}; cards={:#?}",
        replay.clues,
        replay.pending_connections,
        replay.transitions,
        inferred.cards,
    );
}

#[test]
fn second_replay_move_thirty_keeps_the_parked_yellow_three_prompt() {
    let fixture = expert_replay_p4v0s9();
    let state = fixture.state_at_turn(29).expect("fixture prefix is legal");
    let bob = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(bob).expect("Bob has a view"))
        .expect("valid Bob deductions");
    let replay = replay_h_group(&deductions, HGroupProfile::Max);
    let inferred = infer_h_group_from_replay(&deductions, replay.clone(), HGroupProfile::Max);

    assert!(
        inferred.playable_now.contains(&CardId::new(4)),
        "Bob's original yellow-3 Prompt remains parked while he demonstrates red 3: pending={:#?}; transitions={:#?}; cards={:#?}",
        replay.pending_connections,
        replay.transitions,
        inferred.cards,
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(replay_action_at_turn(&fixture, 29)),
        "the parked, physically clued yellow 3 has Priority over an invisible duplicate; excluded={:?}; claims={:#?}; pending={:#?}",
        replay.cards.facts.excluded_identities(CardId::new(4)),
        replay.cards.facts.identity_claims(),
        replay.pending_connections,
    );
}

/// <https://hanabi.github.io/level-14/#the-trash-order-chop-move-tocm>
#[test]
fn trash_order_chop_move_uses_the_number_of_skipped_known_trash_cards() {
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Green, Rank::Five),
            Card::new(Suit::Purple, Rank::Five),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    state.apply(Action::Play(CardId::new(5))).unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        })
        .unwrap();
    let player_two_chop = state.hands()[2][0];
    state.apply(Action::Discard(player_two_chop)).unwrap();
    state.apply(Action::Discard(CardId::new(1))).unwrap();

    let observer = state.current_player();
    assert_eq!(observer, PlayerId::new(1));
    let deductions = LogicalDeductions::new(state.view_for(observer).unwrap()).unwrap();
    let max = replay_h_group(&deductions, HGroupProfile::Max);
    assert!(
        max.cards.chop_moved.contains(&CardId::new(6)),
        "hands={:?}; signals={:#?}",
        max.hands,
        max.signals
    );
    assert!(max.signals.iter().any(|signal| {
        signal.turn == 6
            && signal.actor == PlayerId::new(0)
            && signal.target == Some(PlayerId::new(1))
            && signal.kind == HGroupMoveKind::TrashOrderChopMove
            && signal.cards == [CardId::new(6)]
    }));

    let level_13 = replay_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level13),
    );
    assert!(!level_13.cards.chop_moved.contains(&CardId::new(6)));
}

#[test]
fn level_two_enables_an_off_chop_five_stall() {
    let state = state_with_prefix(
        2,
        &[
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Red, Rank::Five),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Four),
        ],
    );
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let five_stall = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Rank(Rank::Five),
    };
    let level_one = h_group_clue_candidates(&deductions, HGroupProfile::Level(HGroupLevel::Level1));
    let level_two = h_group_clue_candidates(&deductions, HGroupProfile::Level(HGroupLevel::Level2));
    let level_one_score = level_one
        .iter()
        .find(|candidate| candidate.action == five_stall)
        .map(|candidate| candidate.score());
    let level_two_score = level_two
        .iter()
        .find(|candidate| candidate.action == five_stall)
        .map(|candidate| candidate.score());
    assert!(level_two_score > level_one_score);
}

#[test]
fn level_five_keeps_a_layered_finesse_as_an_exact_disjunction() {
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Purple, Rank::Four),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let level_four = infer_h_group(&deductions, HGroupProfile::Level(HGroupLevel::Level4));
    let level_five = infer_h_group(&deductions, HGroupProfile::Level(HGroupLevel::Level5));
    assert_eq!(
        level_four.connection_promises[0].cards,
        vec![CardId::new(9)]
    );
    assert_eq!(
        level_five.connection_promises[0].cards[..2],
        [CardId::new(9), CardId::new(8)]
    );
    assert!(
        level_five
            .signals
            .iter()
            .any(|signal| signal.kind == HGroupMoveKind::LayeredFinesse)
    );
}

#[test]
fn level_sixteen_ejects_the_second_finesse_position() {
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Red, Rank::Five),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Purple, Rank::Four),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let level_fifteen = infer_h_group(&deductions, HGroupProfile::Level(HGroupLevel::Level15));
    let level_sixteen = infer_h_group(&deductions, HGroupProfile::Level(HGroupLevel::Level16));
    assert!(!level_fifteen.playable_now.contains(&CardId::new(8)));
    assert!(level_sixteen.connection.is_none());
    assert!(
        level_sixteen.playable_now.contains(&CardId::new(8)),
        "inference: {level_sixteen:#?}"
    );
    assert!(
        level_sixteen
            .signals
            .iter()
            .any(|signal| signal.kind == HGroupMoveKind::FiveColorEjection)
    );
}

#[test]
fn focus_prefers_chop_when_a_clue_newly_touches_multiple_cards() {
    let red_one = Card::new(Suit::Red, Rank::One);
    let mut state = state_with_prefix(
        2,
        &[
            red_one,
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Four),
            red_one,
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::One),
            red_one,
        ],
    );
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
    assert_eq!(inferred.clues[0].focus, CardId::new(5));
    assert!(inferred.clues[0].focus_was_chop);
}

#[test]
fn focus_rules_cover_retouched_single_and_leftmost_new_cards() {
    let hand = (0..5).map(CardId::new).collect::<Vec<_>>();
    let gotten = [CardId::new(0), CardId::new(1)]
        .into_iter()
        .collect::<CardSet>();
    let current_chop = chop(&hand, &gotten);
    assert_eq!(current_chop, Some(CardId::new(2)));
    assert_eq!(
        focus(
            &hand,
            &[CardId::new(0), CardId::new(1)],
            current_chop,
            &gotten,
        ),
        Some(CardId::new(1))
    );
    assert_eq!(
        focus(
            &hand,
            &[CardId::new(0), CardId::new(3)],
            current_chop,
            &gotten,
        ),
        Some(CardId::new(3))
    );
    assert_eq!(
        focus(
            &hand,
            &[CardId::new(3), CardId::new(4)],
            current_chop,
            &gotten,
        ),
        Some(CardId::new(4))
    );
    assert_eq!(
        focus(
            &hand,
            &[CardId::new(2), CardId::new(3)],
            current_chop,
            &gotten,
        ),
        Some(CardId::new(2))
    );
}

#[test]
fn two_save_is_followed_by_the_more_efficient_rank_one_play_clue() {
    let mut state = paired_sample_five_state();
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        })
        .unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();

    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        }),
        "candidates: {:#?}",
        h_group_clue_candidates(&deductions, HGroupProfile::Max),
    );
}

#[test]
fn out_of_order_clue_is_not_given_to_the_immediate_next_player() {
    let mut state = paired_sample_five_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Yellow),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(2),
                    clue: Clue::Suit(Suit::Blue),
                }
        }),
        "the recipient would act before anyone could give the required Fix Clue: {candidates:#?}"
    );
}

#[test]
fn multi_card_purple_clue_prompts_purple_one_before_purple_two() {
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
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    assert_eq!(inferred.connection, None);
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(0))),
        "inference: {inferred:#?}; clues: {:#?}",
        h_group_clue_candidates(&deductions, HGroupProfile::Max)
    );
}

#[test]
fn repeated_two_clue_does_not_play_red_two_before_its_prompt() {
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
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(14)),
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(5))),
        "inference: {inferred:#?}"
    );
}

#[test]
fn delayed_green_three_without_connectors_is_not_a_fix_clue() {
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
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(14)),
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(0),
                    clue: Clue::Suit(Suit::Green),
                }
        }),
        "candidates: {candidates:#?}"
    );
    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(2),
                    clue: Clue::Suit(Suit::Blue),
                }
        }),
        "a blue-4 out-of-order clue has no accounted blue 2: {candidates:#?}"
    );
}

#[test]
fn self_prompt_survives_an_intervening_fix_clue() {
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
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(3)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        h_group_predictable_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(26))),
        "yellow 4 cannot be a predictable play before yellow 3: {inferred:#?}"
    );
    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(9))),
        "inference: {inferred:#?}"
    );
}

#[test]
fn immediate_rank_three_clue_is_rejected_when_it_creates_a_false_self_prompt() {
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
            clue: Clue::Suit(Suit::Blue),
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
    ] {
        state.apply(action).unwrap();
    }
    let action = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Rank(Rank::Three),
    };
    let giver = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    assert!(
        !h_group_clue_candidates(&giver, HGroupProfile::Max)
            .iter()
            .any(|candidate| candidate.action == action),
        "candidates: {:#?}",
        h_group_clue_candidates(&giver, HGroupProfile::Max),
    );
}

#[test]
fn five_color_ejection_is_not_given_when_second_finesse_position_would_misplay() {
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
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Green),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let candidates = ordered_h_group_actions(&deductions, HGroupProfile::Max);

    assert!(!candidates.contains(&Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Suit(Suit::Purple),
    }));
}

#[test]
fn duplicate_rank_ones_use_a_focused_play_clue_instead_of_ignition() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(14)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(4)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();

    let rank_one = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Rank(Rank::One),
    };
    assert!(ordered_h_group_actions(&deductions, HGroupProfile::Max).contains(&rank_one));

    state.apply(rank_one).unwrap();
    let receiver = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&receiver, HGroupProfile::Max);
    assert_eq!(
        select_h_group_action(&receiver, HGroupProfile::Max),
        Some(Action::Play(CardId::new(13)))
    );
    assert!(!inferred.playable_now.contains(&CardId::new(11)));
    assert!(!inferred.playable_now.contains(&CardId::new(12)));
}

#[test]
fn delayed_green_four_is_rejected_before_charms_and_admitted_as_a_charm_at_level_twenty_three() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
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
        Action::Play(CardId::new(0)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let before_charms =
        h_group_clue_candidates(&deductions, HGroupProfile::Level(HGroupLevel::Level22));
    let with_charms =
        h_group_clue_candidates(&deductions, HGroupProfile::Level(HGroupLevel::Level23));
    let green_four = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Suit(Suit::Green),
    };

    assert!(
        !before_charms
            .iter()
            .any(|candidate| candidate.action == green_four),
        "the ordinary Finesse is invalid before Level 23: {before_charms:#?}"
    );
    assert!(
        with_charms
            .iter()
            .any(|candidate| candidate.action == green_four),
        "Level 23 reinterprets the same physical clue as a 4 Charm: {with_charms:#?}; inference: {:#?}",
        infer_h_group(&deductions, HGroupProfile::Max)
    );
}

#[test]
fn low_score_five_stall_does_not_also_finesse_the_next_player() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
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
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_eq!(inferred.connection, None);
    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(16)))
    );
}

#[test]
fn completed_blue_prompt_plays_the_focus_before_same_clue_ancillary_cards() {
    let profile = HGroupProfile::Level(HGroupLevel::Level25);
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
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, profile);
    let candidates = h_group_clue_candidates(&deductions, profile);

    assert_eq!(
        select_h_group_action(&deductions, profile),
        Some(Action::Play(CardId::new(0))),
        "inference: {inferred:#?}; candidates: {candidates:#?}"
    );
}

#[test]
fn tempo_clue_does_not_reclue_an_active_play_promise() {
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
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(1),
                    clue: Clue::Suit(Suit::Blue),
                }
        }),
        "candidates: {candidates:#?}"
    );
}

/// <https://hanabi.github.io/level-6/#the-tempo-clue-chop-move-tccm>
#[test]
fn rank_fill_in_is_a_tccm_outside_a_stalling_situation() {
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::Four),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    state.apply(Action::Play(CardId::new(9))).unwrap();

    let cathy = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(cathy).unwrap()).unwrap();
    let fill_in = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Rank(Rank::Two),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    assert!(
        candidates.iter().any(|candidate| candidate.action == fill_in),
        "the rank fill-in must remain admissible as a TCCM: {candidates:#?}"
    );

    state.apply(fill_in).unwrap();
    let bob = PlayerId::new(1);
    let deductions = LogicalDeductions::new(state.view_for(bob).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    assert!(inferred.signals.iter().any(|signal| {
        signal.turn == 2
            && signal.kind == HGroupMoveKind::TempoClue
            && signal.cards == [CardId::new(8)]
    }));
    assert!(inferred.signals.iter().any(|signal| {
        signal.turn == 2
            && signal.kind == HGroupMoveKind::TempoClueChopMove
            && signal.cards == [CardId::new(5)]
    }));
    assert!(!inferred.signals.iter().any(|signal| {
        signal.turn == 2
            && matches!(signal.kind, HGroupMoveKind::Stall | HGroupMoveKind::FillInClue)
    }));
}

#[test]
fn out_of_order_fix_obligation_selects_the_repairing_rank_clue() {
    let profile = HGroupProfile::Level(HGroupLevel::Level25);
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
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, profile);

    assert_eq!(
        select_h_group_action(&deductions, profile),
        Some(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        }),
        "inference: {inferred:#?}"
    );
}

#[test]
fn collateral_fix_cancels_the_false_finesse_it_would_otherwise_create() {
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
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        inferred.connection.map(|connection| connection.card),
        Some(CardId::new(15)),
        "inference: {inferred:#?}"
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(13))),
        "inference: {inferred:#?}"
    );
}

#[test]
fn rank_two_clue_preserves_play_delayed_and_save_superposition_without_a_self_prompt() {
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
        Action::Play(CardId::new(2)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let clue = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Rank(Rank::Two),
    };
    assert!(
        candidates.iter().any(|candidate| candidate.action == clue),
        "the rank-2 clue is a valid ambiguous play/delayed/save clue: {candidates:#?}"
    );

    state.apply(clue).unwrap();
    let recipient = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&recipient, HGroupProfile::Max);
    let expected = IdentitySet::singleton(Card::new(Suit::Red, Rank::Two))
        .union(IdentitySet::singleton(Card::new(Suit::Blue, Rank::Two)))
        .union(IdentitySet::singleton(Card::new(Suit::Purple, Rank::Two)));
    let focus = inferred
        .cards
        .iter()
        .find(|card| card.card == CardId::new(21))
        .expect("the newly drawn focused 2 has a convention note");
    assert_eq!(focus.identities, expected, "inference: {inferred:#?}");
    assert!(
        !inferred.playable_now.contains(&CardId::new(21)),
        "not every identity in the focus superposition is playable: {inferred:#?}"
    );
    assert!(
        inferred.connection.is_none_or(|connection| {
            connection.focus != CardId::new(21) || connection.card != CardId::new(12)
        }),
        "a direct-play possibility makes the red-1 Self-Prompt invalid: {inferred:#?}"
    );
}

#[test]
fn blue_three_clue_is_rejected_when_its_blue_two_finesse_would_misplay() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(8)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(5)),
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
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
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(1),
                    clue: Clue::Suit(Suit::Blue),
                }
        }),
        "candidates: {candidates:#?}"
    );
}

#[test]
fn two_save_does_not_finesse_the_next_players_green_four() {
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
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(16))),
        "inference: {inferred:#?}"
    );
}

#[test]
fn multi_card_red_clue_rejects_a_chop_moved_false_prompt() {
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
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(14)),
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(13)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(19)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(12)),
    ] {
        state.apply(action).unwrap();
    }
    let giver = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let replay = replay_h_group(&giver, HGroupProfile::Max);
    assert!(
        !replay.cards.explicitly_clued.contains(&CardId::new(8))
            && !replay.cards.invisibly_clued.contains(&CardId::new(8)),
        "the false Prompt card must not be promptable: {replay:#?}"
    );
    let candidates = h_group_clue_candidates(&giver, HGroupProfile::Max);
    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(1),
                    clue: Clue::Suit(Suit::Red),
                }
        }),
        "the giver must reject a clue its recipient would misplay: {candidates:#?}"
    );
    assert_ne!(
        select_h_group_action(&giver, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        }),
        "candidate fallback selected the rejected clue: {candidates:#?}"
    );
}

#[test]
fn unknown_trash_discharge_is_rejected_when_third_finesse_would_misplay() {
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
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let invalid = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Rank(Rank::One),
    };

    assert!(
        !candidates
            .iter()
            .any(|candidate| candidate.action == invalid),
        "candidates: {candidates:#?}"
    );
    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(invalid),
        "candidates: {candidates:#?}"
    );
}

#[test]
fn green_clue_is_rejected_when_the_next_player_would_misplay_yellow_four() {
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
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Play(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(2)),
        Action::Discard(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(8)),
        Action::Play(CardId::new(22)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Discard(CardId::new(18)),
        Action::Play(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(23)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let clue = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Suit(Suit::Green),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        candidates.iter().any(|candidate| candidate.action == clue),
        "expected the direct green-1 clue to be valid: {candidates:#?}"
    );
    state.apply(clue).unwrap();
    let receiver = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&receiver, HGroupProfile::Max);
    assert_ne!(
        select_h_group_action(&receiver, HGroupProfile::Max),
        Some(Action::Play(CardId::new(17))),
        "inference: {inferred:#?}"
    );
}

#[test]
fn delayed_green_four_without_connectors_is_not_a_play_clue() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
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
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(7)),
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
                    clue: Clue::Suit(Suit::Green),
                }
        }),
        "candidates: {candidates:#?}, inference: {:#?}",
        infer_h_group(&deductions, HGroupProfile::Max)
    );
}

#[test]
fn existing_blue_one_satisfies_a_later_blue_connection() {
    let profile = HGroupProfile::Level(HGroupLevel::Level25);
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
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    assert_eq!(
        select_h_group_action(&deductions, profile),
        Some(Action::Play(CardId::new(13))),
        "inference: {:#?}; clues: {:#?}; actions: {:#?}",
        infer_h_group(&deductions, profile),
        h_group_clue_candidates(&deductions, profile),
        ordered_h_group_actions(&deductions, profile),
    );
}

/// <https://hanabi.github.io/extras/chop-moves/#double-order-chop-move-for-3-player-games>
#[test]
fn max_turns_a_three_player_three_skip_into_a_double_order_chop_move() {
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
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let numbered = infer_h_group(
        &deductions,
        HGroupProfile::Level(HGroupLevel::Level25),
    );
    let maximum = infer_h_group(&deductions, HGroupProfile::Max);

    assert!(numbered.signals.iter().any(|signal| {
        signal.turn == 2
            && signal.kind == HGroupMoveKind::OrderChopMove
            && signal.cards == [CardId::new(10)]
    }));
    assert!(maximum.signals.iter().any(|signal| {
        signal.turn == 2
            && signal.kind == HGroupMoveKind::DoubleOrderChopMove
            && signal.cards == [CardId::new(0), CardId::new(1)]
    }));
}

#[test]
fn red_five_connection_uses_existing_red_one_prompt() {
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
            clue: Clue::Suit(Suit::Blue),
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
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(3)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(16))),
        "inference: {inferred:#?}"
    );
}

#[test]
fn layered_red_four_connection_is_not_discarded() {
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
            clue: Clue::Suit(Suit::Blue),
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
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(3)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(15)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(18)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Discard(CardId::new(16))),
        "inference: {inferred:#?}"
    );
}

#[test]
fn multi_yellow_clue_does_not_play_non_focus_yellow_four() {
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
            clue: Clue::Suit(Suit::Blue),
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
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(2)),
        Action::Discard(CardId::new(18)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(15)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(17))),
        "inference: {inferred:#?}"
    );
}

#[test]
fn extra_clue_does_not_create_an_invalid_purple_two_finesse() {
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
            clue: Clue::Suit(Suit::Blue),
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
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(12)),
        Action::Discard(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Discard(CardId::new(15)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(2),
                    clue: Clue::Rank(Rank::Two),
                }
        }),
        "candidates: {candidates:#?}"
    );
}

#[test]
fn color_clue_to_an_unplayable_five_is_not_a_play_clue() {
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
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(14)),
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(0),
                    clue: Clue::Suit(Suit::Red),
                }
        }),
        "candidates: {candidates:#?}"
    );
}

#[test]
fn bluff_is_not_given_to_a_player_with_a_queued_play() {
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
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(14)),
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(17)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(0),
                    clue: Clue::Suit(Suit::Yellow),
                }
        }),
        "James already has a queued red 1, so a Bluff cannot resolve immediately: {candidates:#?}"
    );
}

#[test]
fn trash_chop_move_requires_publicly_known_trash() {
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
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(14)),
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(17)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(0),
                    clue: Clue::Suit(Suit::Purple),
                }
        }),
        "the recipient could interpret the fresh purple 1 as purple 4: {candidates:#?}"
    );
}

#[test]
fn ordinary_discard_does_not_finesse_the_matching_card() {
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
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(14)),
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Discard(CardId::new(2)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert!(
        !inferred.playable_now.contains(&CardId::new(6)),
        "inference: {inferred:#?}"
    );
    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(6))),
        "inference: {inferred:#?}"
    );
}

#[test]
fn no_information_reclue_does_not_fix_an_unqueued_card() {
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
            clue: Clue::Suit(Suit::Blue),
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
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(2)),
        Action::Discard(CardId::new(18)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(15)),
        Action::Play(CardId::new(21)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(0),
                    clue: Clue::Suit(Suit::Yellow),
                }
        }),
        "the yellow 4 is not queued and the clue adds no information: {candidates:#?}"
    );
}

#[test]
fn out_of_order_red_clue_is_not_given_without_time_for_a_fix() {
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
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(1),
                    clue: Clue::Suit(Suit::Red),
                }
        }),
        "the recipient would act before the out-of-order Fix Clue: {candidates:#?}"
    );
}

#[test]
fn duplicate_rank_ones_are_fixed_before_optional_play_clues() {
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
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();

    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        })
    );
}

#[test]
fn five_save_does_not_also_pull_the_adjacent_purple_four() {
    let state = paired_sample_five_after_second_five_save();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert!(
        inferred.is_saved(CardId::new(19)),
        "inference: {inferred:#?}"
    );
    assert!(!inferred.playable_now.contains(&CardId::new(4)));
    assert!(
        !inferred
            .signals
            .iter()
            .any(|signal| { signal.turn == 14 && signal.kind == HGroupMoveKind::FivePull })
    );
}

#[test]
fn level_twenty_three_blaze_discard_uses_matching_finesse_positions() {
    // Directly models https://hanabi.github.io/level-23/#the-blaze-discard:
    // Cathy's clued red 2 is transferred to Bob's Second Finesse Position,
    // so Alice must blind-play her own Second Finesse Position.
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Green, Rank::Five),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Red, Rank::Two),
        ],
    );
    state.apply(Action::Play(CardId::new(0))).unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    state.apply(Action::Discard(CardId::new(14))).unwrap();

    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let replay = replay_h_group(
        &deductions,
        HGroupProfile::Level(HGroupLevel::Level23),
    );
    assert!(
        replay.signals.iter().any(|signal| {
                signal.kind == HGroupMoveKind::BlazeDiscard
                && signal.target == Some(PlayerId::new(0))
                && signal.cards == [CardId::new(4), CardId::new(8)]
        }),
        "the Blaze must communicate the matching Finesse Position: {replay:#?}"
    );
    assert!(replay.cards.forced_playable.contains(&CardId::new(4)));
}

#[test]
fn level_twenty_three_hesitation_uses_the_unique_safe_connector() {
    // Directly models
    // https://hanabi.github.io/level-23/#the-hesitation-blind-play:
    // blue 3 is directly playable, but red 3 remains a Reverse-Finesse
    // possibility. Bob's discard identifies Cathy's red 2 as the sole safe
    // connector.
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Red, Rank::Four),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Red, Rank::Five),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Green, Rank::Five),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Red, Rank::Two),
        ],
    );
    for action in [
        Action::Play(CardId::new(0)),
        Action::Play(CardId::new(5)),
        Action::Play(CardId::new(10)),
        Action::Play(CardId::new(1)),
        Action::Play(CardId::new(6)),
        Action::Play(CardId::new(11)),
        Action::Play(CardId::new(2)),
        Action::Play(CardId::new(7)),
        Action::Play(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Discard(CardId::new(8)),
    ] {
        state.apply(action).unwrap();
    }

    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let replay = replay_h_group(
        &deductions,
        HGroupProfile::Level(HGroupLevel::Level23),
    );
    assert!(
        replay.signals.iter().any(|signal| {
            signal.kind == HGroupMoveKind::HesitationBlindPlay
                && signal.target == Some(PlayerId::new(2))
                && signal.cards == [CardId::new(23), CardId::new(22)]
                && signal.identity == Some(Card::new(Suit::Red, Rank::Two))
        }),
        "the hesitation must identify the sole missing red-2 connector: {replay:#?}"
    );
    assert!(replay.cards.forced_playable.contains(&CardId::new(23)));
}
