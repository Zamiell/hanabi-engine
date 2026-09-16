use super::{
    Card, CardId, CardSet, Clue, ConnectionManager, ConnectionObligation,
    ConnectionTransitionReason, ConventionJournal, HGroupClueKind, HGroupConnectionKind,
    HGroupMoveKind, HGroupRuleEffects, HGroupTurnSnapshot, IdentitySet, ObservedEvent,
    ObservedHistoryEntry, PlayerView, PromiseId, Rank, bluff_play_connects, bluff_target_kind_at,
    finesse_position_id, identity_of, is_playable_at, next_player, push_signal, was_clued_before,
};

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(in crate::h_group) fn apply_bluff_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    stack_heights: [u8; 5],
    explicitly_clued: &CardSet,
    already_playing: &CardSet,
    pending: &mut ConnectionManager,
    forced_playable: &mut CardSet,
    signals: &mut ConventionJournal,
) {
    // Sources:
    // - https://hanabi.github.io/level-11/#the-bluff
    // - https://hanabi.github.io/level-11/#the-self-bluff
    let ObservedEvent::Clued {
        giver,
        target,
        clue,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    let Some(focus) = touched.last().copied() else {
        return;
    };
    let Some(focus_identity) = identity_of(view, focus) else {
        return;
    };
    let height = stack_heights[focus_identity.suit.index()];
    let focus_is_one_away = super::super::bluff::bluff_through_clued_cards(
        usize::from(height),
        focus_identity,
        |needed| {
            hands.iter().flatten().any(|card| {
                explicitly_clued.contains(card)
                    && identity_of(view, *card)
                        .or_else(|| signals.facts().known_identity_before(*card, entry.turn))
                        == Some(needed)
            })
        },
    );
    let actor = next_player(*giver, hands.len());
    let hard_three_self = actor == *target
        && *clue == Clue::Rank(Rank::Three)
        && focus_identity.rank == Rank::Three
        && height == 0;
    if is_playable_at(stack_heights, focus_identity) || !(focus_is_one_away || hard_three_self) {
        return;
    }
    let expected_connector = Card::new(focus_identity.suit, Rank::ALL[usize::from(height)]);
    if super::super::bluff::bluff_connector_is_promised(
        view,
        hands,
        already_playing,
        pending,
        expected_connector,
        Some(focus),
    ) {
        return;
    }
    if actor == *target {
        if !matches!(clue, Clue::Rank(_)) {
            return;
        }
        let Some(bluff_card) = finesse_position_id(&hands[actor.index()], explicitly_clued, 0)
        else {
            return;
        };
        if hands.iter().enumerate().any(|(player, hand)| {
            player != actor.index()
                && hand
                    .iter()
                    .any(|card| identity_of(view, *card) == Some(expected_connector))
        }) {
            return;
        }
        let bluff_identity = identity_of(view, bluff_card);
        if bluff_identity.is_some_and(|identity| {
            !is_playable_at(stack_heights, identity) || bluff_play_connects(*clue, identity)
        }) {
            return;
        }
        forced_playable.insert(bluff_card);
        push_signal(
            signals,
            entry,
            *giver,
            Some(actor),
            HGroupMoveKind::Bluff,
            vec![bluff_card, focus],
            bluff_identity,
        );
        push_signal(
            signals,
            entry,
            *giver,
            Some(actor),
            HGroupMoveKind::SelfBluff,
            vec![bluff_card, focus],
            bluff_identity,
        );
        return;
    }
    if super::super::bluff::bluff_is_queued(pending, actor, Some(focus)) {
        return;
    }
    let Some((bluff_card, bluff_identity)) = hands[actor.index()]
        .iter()
        .rev()
        .copied()
        .filter(|card| Some(*card) != Some(focus))
        .find_map(|card| {
            identity_of(view, card)
                .filter(|identity| is_playable_at(stack_heights, *identity))
                .map(|identity| (card, identity))
        })
    else {
        return;
    };
    let stack_height = usize::from(height);
    if stack_height == Rank::ALL.len() {
        return;
    }
    let expected_connector = Card::new(focus_identity.suit, Rank::ALL[stack_height]);
    if bluff_identity == expected_connector || bluff_play_connects(*clue, bluff_identity) {
        // Cathy's Connecting Principle applies to rank clues as well as suit
        // clues. Any 1 connects to a rank-2 clue, so an off-suit 1 proves a
        // (possibly Layered) Finesse rather than a Bluff. The old check only
        // rejected the same-suit connector and incorrectly collapsed the
        // rank-2 superposition into a one-play Bluff.
        // Source: https://hanabi.github.io/level-11/#cathys-connecting-principle-part-2
        return;
    }
    pending.start(
        entry.turn,
        ConnectionObligation {
            promise: PromiseId::UNASSIGNED,
            actor,
            cards: vec![bluff_card],
            expected: bluff_identity,
            focus_identity,
            kind: HGroupConnectionKind::Finesse,
            focus,
            step: 0,
        },
    );
    push_signal(
        signals,
        entry,
        *giver,
        Some(actor),
        HGroupMoveKind::Bluff,
        vec![bluff_card, focus],
        Some(bluff_identity),
    );
}

pub(in crate::h_group) fn apply_resolved_bluff_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    before: &HGroupTurnSnapshot,
    effects: &mut HGroupRuleEffects<'_>,
) {
    let clues = effects.clues;
    let facts = &before.facts;
    let stack_heights = before.stack_heights;
    let already_playing = &mut *effects.already_playing;
    let pending = &mut *effects.pending;
    let signals = &mut *effects.signals;
    let ObservedEvent::Played {
        player,
        card,
        identity,
        successful: true,
    } = entry.event
    else {
        return;
    };
    let had_preexisting_play_obligation = before.older_play_obligations.contains(&card);
    if had_preexisting_play_obligation {
        // A successful play that was already convention-bound before the
        // immediately preceding clue resolves that older promise; it is not
        // evidence that the new clue was a Bluff. In game p4v0s2, Donald's
        // promised red 3 follows Cathy's blue clue to Alice. Reclassifying it
        // as a Bluff would falsely rewrite Alice's playable blue 1 as blue 2.
        // Source: https://hanabi.github.io/level-11/#the-bluff
        return;
    }
    if was_clued_before(view, entry.turn, card) {
        return;
    }
    let Some(clue) = clues.iter().rev().find(|clue| {
        clue.turn + 1 == entry.turn
            && player == next_player(clue.giver, view.hands.len())
            && matches!(clue.kind, HGroupClueKind::Play | HGroupClueKind::PlayOrSave)
    }) else {
        return;
    };
    let connects = bluff_play_connects(clue.clue, identity);
    let legal_bluff_target = IdentitySet::all().iter().any(|candidate| {
        clue.clue.matches(candidate)
            && facts[clue.focus.index()].allows(candidate)
            && (bluff_target_kind_at(stack_heights, clue.clue, candidate).is_some()
                || super::super::bluff::bluff_through_clued_cards(
                    usize::from(clue.stack_heights[candidate.suit.index()]),
                    candidate,
                    |needed| {
                        before.hands.iter().flatten().any(|prior| {
                            was_clued_before(view, clue.turn, *prior)
                                && identity_of(view, *prior).or_else(|| {
                                    signals.facts().known_identity_before(*prior, clue.turn)
                                }) == Some(needed)
                        })
                    },
                ))
    });
    if connects || !legal_bluff_target {
        return;
    }

    pending.cancel_where(
        entry.turn,
        ConnectionTransitionReason::FocusInvalidated,
        |connection| connection.focus == clue.focus,
    );
    already_playing.remove(&clue.focus);
    push_signal(
        signals,
        entry,
        clue.giver,
        Some(player),
        HGroupMoveKind::Bluff,
        vec![card, clue.focus],
        Some(identity),
    );
}
