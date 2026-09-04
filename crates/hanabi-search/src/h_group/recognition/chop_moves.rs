use super::{
    Clue, ConventionJournal, HGroupMoveKind, HGroupRuleEffects, HGroupTurnContext, IdentitySet,
    ObservedEvent, PlayerId, PlayerView, Rank, five_chop_moved_card, identity_of,
    is_card_identity_accounted_trash, protected_cards, push_signal,
};

fn same_turn_signal_blocks_chop_move(
    signals: &ConventionJournal,
    turn: u32,
    target: PlayerId,
    clue_domain_is_accounted: bool,
) -> bool {
    signals.iter().any(|signal| {
        signal.turn == turn
            && signal.target == Some(target)
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
            && (!matches!(
                signal.kind,
                HGroupMoveKind::PlayClue | HGroupMoveKind::Bluff
            ) || !clue_domain_is_accounted)
    })
}

fn card_is_accounted_trash(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &HGroupRuleEffects<'_>,
    gotten: &super::CardSet,
    card: super::CardId,
) -> bool {
    let possibilities = identity_of(view, card)
        .map(IdentitySet::singleton)
        .or_else(|| {
            effects
                .signals
                .facts()
                .known_identity(card)
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
            is_card_identity_accounted_trash(
                view,
                card,
                identity,
                context.after.stack_heights,
                gotten,
                effects.signals.facts(),
            )
        })
}

fn clue_domain_is_accounted(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &HGroupRuleEffects<'_>,
    clue: Clue,
    touched: &[super::CardId],
    gotten: &super::CardSet,
) -> bool {
    IdentitySet::all()
        .iter()
        .filter(|identity| clue.matches(*identity))
        .all(|identity| {
            touched.iter().copied().any(|card| {
                is_card_identity_accounted_trash(
                    view,
                    card,
                    identity,
                    context.after.stack_heights,
                    gotten,
                    effects.signals.facts(),
                )
            })
        })
}

fn supersede_current_connections(
    context: &HGroupTurnContext<'_>,
    effects: &mut HGroupRuleEffects<'_>,
    touched: &[super::CardId],
) {
    let superseded_cards = effects
        .pending
        .iter()
        .filter(|connection| touched.contains(&connection.focus))
        .flat_map(|connection| connection.cards.iter().copied())
        .collect::<Vec<_>>();
    effects.pending.cancel_where(
        context.entry.turn,
        super::ConnectionTransitionReason::Superseded,
        |connection| touched.contains(&connection.focus),
    );
    for card in superseded_cards {
        effects.forced_playable.remove(&card);
    }
}

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
    let hand = &context.after.hands[target.index()];
    let mut gotten = protected_cards(
        effects.explicitly_clued,
        effects.invisibly_clued,
        effects.chop_moved,
    );
    // Current touches cannot account for themselves; TCM identities must
    // already have been accounted for before positive clue facts are applied.
    for card in touched {
        gotten.remove(card);
    }
    let clue_domain_is_accounted =
        clue_domain_is_accounted(context, view, effects, *clue, touched, &gotten);
    if same_turn_signal_blocks_chop_move(
        effects.signals,
        context.entry.turn,
        *target,
        clue_domain_is_accounted,
    ) {
        return;
    }
    let five_chop_moved = (*clue == Clue::Rank(Rank::Five))
        .then(|| five_chop_moved_card(hand, touched, &gotten))
        .flatten();
    let all_trash = !touched.is_empty()
        && touched
            .iter()
            .all(|card| card_is_accounted_trash(context, view, effects, &gotten, *card));
    let five_chop_move = five_chop_moved.is_some();
    if !all_trash && !five_chop_move {
        return;
    }
    if all_trash {
        // A Bluff is only provisional until the clue's ordinary trash
        // interpretation has been checked. If every touched identity is
        // already accounted for, Minimum Clue Value makes this a Trash Chop
        // Move instead; retract any same-focus connection synthesized by the
        // earlier Bluff pass.
        // Source: https://hanabi.github.io/level-4/#the-trash-chop-move-tcm
        supersede_current_connections(context, effects, touched);
    }
    let boundary = touched
        .iter()
        .filter_map(|card| hand.iter().position(|candidate| candidate == card))
        .min()
        .unwrap_or(0);
    let moved = five_chop_moved.map_or_else(
        || {
            hand[..boundary]
                .iter()
                .rev()
                .filter(|card| {
                    !effects.explicitly_clued.contains(card) && !effects.chop_moved.contains(card)
                })
                .take(boundary.max(1))
                .copied()
                .collect::<Vec<_>>()
        },
        |card| vec![card],
    );
    if moved.is_empty()
        || (all_trash
            && moved
                .iter()
                .all(|card| card_is_accounted_trash(context, view, effects, &gotten, *card)))
    {
        // A Trash Chop Move must actually protect something. Moving only
        // cards that the recipient already knows are safe discards changes
        // neither their chop nor their future obligations, so it fails
        // Minimum Clue Value. In the endgame this distinction is especially
        // important: the same useless clue can instead be a Trash Double
        // Ignition.
        // Sources:
        // - https://hanabi.github.io/level-1/#minimum-clue-value-principle
        // - https://hanabi.github.io/level-4/#the-trash-chop-move-tcm
        // - https://hanabi.github.io/level-21/#the-trash-double-ignition-tdi
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
