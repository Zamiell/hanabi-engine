use super::{
    CardId, Clue, ConventionJournal, HGroupClueInterpretation, HGroupMoveKind, HGroupRuleEffects,
    HGroupTurnContext, IdentitySet, ObservedEvent, PlayerId, PlayerView, Rank, chop, is_trash_at,
    next_player, protected_cards, push_signal,
};

/// Recognizes the Level-4 Order Chop Move from a deliberately out-of-order 1.
///
/// The originating rank-1 clue defines the comparable cards. Its chop focus
/// plays first, then newly drawn 1s from newest to oldest, then starting-hand
/// 1s from oldest to newest (the UI's right-to-left order). Skipping `n` cards
/// Chop Moves the player `n` seats after the actor.
pub(in crate::h_group) fn apply_order_chop_move_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
    extras_enabled: bool,
) {
    // Sources:
    // - https://hanabi.github.io/level-4/#the-order-chop-move-ocm
    // - https://hanabi.github.io/extras/chop-moves/#double-order-chop-move-for-3-player-games
    // - https://hanabi.github.io/extras/chop-moves/#spillover-chop-move
    let ObservedEvent::Played {
        player,
        card,
        identity,
        successful: true,
    } = context.entry.event
    else {
        return;
    };
    if identity.rank != Rank::One {
        return;
    }

    let actor_hand = &context.before.hands[player.index()];
    let Some(origin) = effects.clues.iter().rev().find(|clue| {
        clue.target == player
            && clue.clue == Clue::Rank(Rank::One)
            && (clue.focus == card || clue.new_non_focus.contains(&card))
            && actor_hand.contains(&clue.focus)
    }) else {
        return;
    };

    // Exact identity knowledge explains the choice and suppresses an OCM.
    let played_possibilities =
        IdentitySet::from_mask(context.before.facts[card.index()].identity_mask());
    if played_possibilities.len() == 1
        || effects.signals.iter().any(|signal| {
            signal.turn < origin.turn
                && matches!(
                    signal.kind,
                    HGroupMoveKind::Finesse
                        | HGroupMoveKind::ReverseFinesse
                        | HGroupMoveKind::SelfFinesse
                        | HGroupMoveKind::LayeredFinesse
                )
                && signal.cards.contains(&card)
        })
    {
        return;
    }

    let ordered = order_chop_move_order(context, view, effects.signals, origin, actor_hand, card);
    let Some(skipped) = ordered.iter().position(|candidate| *candidate == card) else {
        return;
    };
    if skipped == 0 {
        return;
    }

    let double = extras_enabled && context.after.hands.len() == 3 && skipped >= 3;
    let target_distance = if double { skipped - 2 } else { skipped };
    let target_index = (player.index() + target_distance) % context.after.hands.len();
    let mut target = PlayerId::new(
        u8::try_from(target_index).expect("standard Hanabi has at most five players"),
    );
    let gotten = protected_cards(
        effects.explicitly_clued,
        effects.invisibly_clued,
        effects.chop_moved,
    );
    let mut spillover = false;
    if extras_enabled && chop(&context.after.hands[target.index()], &gotten).is_none() {
        target = next_player(target, context.after.hands.len());
        spillover = true;
    }
    let moved = context.after.hands[target.index()]
        .iter()
        .copied()
        .filter(|card| !gotten.contains(card))
        .take(if double { 2 } else { 1 })
        .collect::<Vec<_>>();
    if moved.len() < if double { 2 } else { 1 } {
        return;
    }
    effects.chop_moved.extend(moved.iter().copied());
    push_signal(
        effects.signals,
        context.entry,
        player,
        Some(target),
        if spillover {
            HGroupMoveKind::SpilloverChopMove
        } else if double {
            HGroupMoveKind::DoubleOrderChopMove
        } else {
            HGroupMoveKind::OrderChopMove
        },
        moved,
        None,
    );
}

fn order_chop_move_order(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    signals: &ConventionJournal,
    origin: &HGroupClueInterpretation,
    actor_hand: &[CardId],
    played: CardId,
) -> Vec<CardId> {
    let positive_clue_count = |candidate: CardId| {
        view.history
            .iter()
            .take_while(|entry| entry.turn < context.entry.turn)
            .filter(|entry| {
                matches!(
                    &entry.event,
                    ObservedEvent::Clued { touched, .. } if touched.contains(&candidate)
                )
            })
            .count()
    };
    let played_clue_count = positive_clue_count(played);
    let initial_hand_size = if context.before.hands.len() <= 3 {
        5
    } else {
        4
    };
    let initial_cards = initial_hand_size * context.before.hands.len();
    let convention_facts = signals.facts();
    let contextual_identity = |candidate: CardId| {
        convention_facts.known_identity(candidate).or_else(|| {
            if candidate == origin.focus {
                return (origin.focus_identities.len() == 1)
                    .then(|| origin.focus_identities.iter().next())
                    .flatten();
            }
            origin
                .non_focus_identities
                .iter()
                .find(|(card, _)| *card == candidate)
                .and_then(|(_, identities)| {
                    (identities.len() == 1)
                        .then(|| identities.iter().next())
                        .flatten()
                })
        })
    };
    let mut ordered = core::iter::once(origin.focus)
        .chain(origin.new_non_focus.iter().copied())
        .filter(|candidate| actor_hand.contains(candidate))
        .filter(|candidate| positive_clue_count(*candidate) == played_clue_count)
        .filter(|candidate| {
            if let Some(identity) = contextual_identity(*candidate) {
                return !is_trash_at(context.before.stack_heights, identity);
            }
            let possibilities =
                IdentitySet::from_mask(context.before.facts[candidate.index()].identity_mask());
            !possibilities.is_empty()
                && possibilities
                    .iter()
                    .any(|identity| !is_trash_at(context.before.stack_heights, identity))
        })
        .collect::<Vec<_>>();
    ordered.sort_by_key(|candidate| {
        let position = actor_hand
            .iter()
            .position(|in_hand| in_hand == candidate)
            .unwrap_or(0);
        if *candidate == origin.focus && origin.focus_was_chop {
            (0_u8, 0_usize)
        } else if candidate.index() >= initial_cards {
            (1, usize::MAX - position)
        } else {
            (2, position)
        }
    });
    ordered.dedup();
    ordered
}
