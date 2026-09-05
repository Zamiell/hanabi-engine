#[test]
fn learning_path_metadata_covers_every_cumulative_level() {
    assert_eq!(H_GROUP_LEVELS.len(), 26);
    for (index, descriptor) in H_GROUP_LEVELS.iter().enumerate() {
        assert_eq!(usize::from(descriptor.profile.effective_level()), index + 1);
        assert!(!descriptor.title.is_empty());
        assert!(!descriptor.effects.is_empty());
    }
    assert!(
        H_GROUP_LEVELS[5]
            .effects
            .contains(&HGroupMoveKind::Clarity)
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
    let admitted = next_candidates
        .iter()
        .map(|candidate| candidate.action)
        .collect::<Vec<_>>();
    let rejected = h_group_rejected_clues_from_replay(
        &donald_deductions,
        HGroupProfile::Max,
        &donald_replay,
        &admitted,
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
        "Donald sees the blue-3, green-3, green-4 Layered Finesse: candidates={next_candidates:#?}; rejected={rejected:#?}; pending={:#?}; signals={:#?}",
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
        "rank 4 uniquely secures Cathy's purple 4 and must not receive a Clarity penalty: {candidates:#?}"
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
