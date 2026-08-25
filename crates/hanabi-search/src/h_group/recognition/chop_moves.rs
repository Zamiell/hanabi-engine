use super::{
    Clue, HGroupMoveKind, HGroupRuleEffects, HGroupTurnContext, IdentitySet, ObservedEvent,
    PlayerView, Rank, identity_of, push_signal,
};

/// Recognizes Level-4 Trash and 5's Chop Moves from observer-relative clue
/// facts. The recipient must not need the physical identity of their own
/// touched card to understand the move.
pub(crate) fn apply_chop_move_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) {
    // Sources: https://hanabi.github.io/level-4/#the-trash-chop-move-tcm
    // https://hanabi.github.io/level-4/#the-5s-chop-move-5cm
    let ObservedEvent::Clued {
        giver,
        target,
        clue,
        touched,
        ..
    } = &context.entry.event
    else {
        return;
    };
    if effects.signals.iter().any(|signal| {
        signal.turn == context.entry.turn
            && signal.target == Some(*target)
            && matches!(
                signal.kind,
                HGroupMoveKind::FiveStall
                    | HGroupMoveKind::PlayClue
                    | HGroupMoveKind::SaveClue
                    | HGroupMoveKind::FixClue
                    | HGroupMoveKind::TempoClue
                    | HGroupMoveKind::Stall
                    | HGroupMoveKind::TrashPush
                    | HGroupMoveKind::Bluff
            )
    }) {
        return;
    }
    let hand = &context.after.hands[target.index()];
    let all_trash = !touched.is_empty()
        && touched.iter().all(|card| {
            let possibilities = identity_of(view, *card)
                .map(IdentitySet::singleton)
                .or_else(|| {
                    effects
                        .signals
                        .facts()
                        .known_identity(*card)
                        .map(IdentitySet::singleton)
                })
                .unwrap_or_else(|| {
                    context
                        .after
                        .facts
                        .get(card.index())
                        .map_or_else(IdentitySet::all, |facts| {
                            IdentitySet::from_mask(facts.identity_mask())
                        })
                });
            !possibilities.is_empty()
                && possibilities.iter().all(|identity| {
                    identity.rank.number() <= context.after.stack_heights[identity.suit.index()]
                })
        });
    let five_chop_move = *clue == Clue::Rank(Rank::Five) && !touched.is_empty();
    if !all_trash && !five_chop_move {
        return;
    }
    let boundary = touched
        .iter()
        .filter_map(|card| hand.iter().position(|candidate| candidate == card))
        .min();
    let Some(boundary) = boundary else {
        return;
    };
    let count = if five_chop_move { 1 } else { boundary };
    let moved = hand[..boundary]
        .iter()
        .rev()
        .filter(|card| {
            !effects.explicitly_clued.contains(card) && !effects.chop_moved.contains(card)
        })
        .take(count.max(1))
        .copied()
        .collect::<Vec<_>>();
    if moved.is_empty() {
        return;
    }
    effects.chop_moved.extend(moved.iter().copied());
    push_signal(
        effects.signals,
        context.entry,
        *giver,
        Some(*target),
        HGroupMoveKind::ChopMove,
        moved.clone(),
        None,
    );
    push_signal(
        effects.signals,
        context.entry,
        *giver,
        Some(*target),
        if five_chop_move {
            HGroupMoveKind::FiveChopMove
        } else {
            HGroupMoveKind::TrashChopMove
        },
        moved,
        None,
    );
}
