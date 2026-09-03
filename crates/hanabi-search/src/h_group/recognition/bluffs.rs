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
    let focus_is_one_away = focus_identity.rank.number() == height + 2;
    let actor = next_player(*giver, hands.len());
    let hard_three_self = actor == *target
        && *clue == Clue::Rank(Rank::Three)
        && focus_identity.rank == Rank::Three
        && height == 0;
    if is_playable_at(stack_heights, focus_identity) || !(focus_is_one_away || hard_three_self) {
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
        let expected_connector = Card::new(focus_identity.suit, Rank::ALL[usize::from(height)]);
        let connector_is_already_promised = hands.iter().flatten().any(|card| {
            already_playing.contains(card) && identity_of(view, *card) == Some(expected_connector)
        }) || pending.iter().any(|connection| {
            pending.is_active(connection) && connection.expected == expected_connector
        });
        if connector_is_already_promised {
            // A Self-Bluff supplies a missing connector. If that connector is
            // already convention-bound, the focus simply follows the queued
            // play; blind-playing another card would duplicate the line.
            // Source: https://hanabi.github.io/level-11/#the-self-bluff
            return;
        }
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
    let actor_is_loaded = pending.iter().any(|connection| {
        connection.actor == actor && connection.focus != focus && pending.is_active(connection)
    }) || hands[actor.index()].iter().any(|card| {
        explicitly_clued.contains(card)
            && identity_of(view, *card)
                .is_some_and(|identity| is_playable_at(stack_heights, identity))
    });
    if actor_is_loaded {
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
            && bluff_target_kind_at(stack_heights, clue.clue, candidate).is_some()
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
