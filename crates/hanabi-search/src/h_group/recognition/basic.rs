use super::{
    CardSet, Clue, ConnectionTransitionReason, ConventionJournal, HGroupConnectionKind,
    HGroupMoveKind, HGroupRuleEffects, HGroupTurnContext, IdentitySet, ObservedEvent,
    ObservedHistoryEntry, PlayerView, Rank, chop, five_chop_moved_card, focus, identity_of,
    identity_set, is_playable_at, is_playable_now, next_player, protected_cards, push_signal,
    same_turn_signal, was_clued_before,
};

pub(in crate::h_group) fn apply_level_two_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) {
    // Sources: https://hanabi.github.io/level-2/#the-5-stall-cluing-off-chop-5s
    // https://hanabi.github.io/level-2/#the-reverse-finesse
    // https://hanabi.github.io/level-2/#the-self-finesse
    let entry = context.entry;
    let hands = context.after.hands;
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
    let gotten = protected_cards(
        effects.explicitly_clued,
        effects.invisibly_clued,
        effects.chop_moved,
    );
    let five_chop_move = *clue == Clue::Rank(Rank::Five)
        && !context.before.early_game
        && five_chop_moved_card(&hands[target.index()], touched, &gotten).is_some();
    if *clue == Clue::Rank(Rank::Five)
        && !touched.is_empty()
        && !five_chop_move
        && hands[target.index()]
            .first()
            .is_none_or(|chop| !touched.contains(chop))
    {
        push_signal(
            effects.signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::FiveStall,
            touched.clone(),
            None,
        );
    }

    // Reverse/Self-Finesse refine a canonical Play interpretation. They must
    // never be inferred independently from the visible rank of a delayed
    // focus: Save precedence can deliberately clue an unplayable chop card,
    // and re-deriving meaning here used to turn that Save into a false play.
    if !same_turn_signal(effects.signals, entry.turn, HGroupMoveKind::PlayClue) {
        return;
    }

    // A clue to one's own next connecting card is the Self-Finesse form. A
    // connection that has to pass the target before resolving is the Reverse
    // form. Multi-card Prompts/Finesses are represented by repeated primitive
    // connection signals in resolution order.
    let delayed_focus = touched.last().copied().and_then(|focus| {
        identity_of(view, focus)
            .filter(|identity| !is_playable_now(view, *identity))
            .map(|identity| (focus, identity))
    });
    if let Some((focus, identity)) = delayed_focus {
        let actor = next_player(*giver, hands.len());
        let kind = if actor == *target {
            HGroupMoveKind::SelfFinesse
        } else if target.index() < actor.index() {
            HGroupMoveKind::ReverseFinesse
        } else if effects.explicitly_clued.contains(&focus) {
            HGroupMoveKind::Prompt
        } else {
            HGroupMoveKind::Finesse
        };
        push_signal(
            effects.signals,
            entry,
            *giver,
            Some(*target),
            kind,
            vec![focus],
            Some(identity),
        );
    }
}

pub(in crate::h_group) fn apply_level_three_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) {
    // Sources: https://hanabi.github.io/level-3/#playing-multiple-1s
    // https://hanabi.github.io/level-3/#the-fix-clue
    // https://hanabi.github.io/level-3/#the-sarcastic-discard-sd
    if apply_repeated_one_fix(context, view, effects) {
        return;
    }
    if !apply_fill_in_fix(context, view, effects) {
        apply_sarcastic_discard(context.entry, view, effects.signals);
    }
}

pub(super) fn apply_repeated_one_fix(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) -> bool {
    let entry = context.entry;
    let hands = context.after.hands;
    let explicitly_clued = effects.explicitly_clued;
    let ObservedEvent::Clued {
        giver,
        target,
        clue: Clue::Rank(Rank::One),
        touched,
        ..
    } = &entry.event
    else {
        return false;
    };
    if touched.is_empty()
        || !touched.iter().all(|card| {
            view.history
                .iter()
                .take_while(|prior| prior.turn < entry.turn)
                .any(|prior| {
                    matches!(
                        &prior.event,
                        ObservedEvent::Clued {
                            clue: Clue::Rank(Rank::One),
                            touched,
                            ..
                        } if touched.contains(card)
                    )
                })
        })
    {
        return false;
    }
    let Some(fixed) = focus(
        &hands[target.index()],
        touched,
        chop(&hands[target.index()], explicitly_clued),
        explicitly_clued,
    ) else {
        return false;
    };
    let canceled_cards = effects
        .pending
        .iter()
        .filter(|connection| connection.focus == fixed)
        .flat_map(|connection| connection.cards.iter().copied())
        .collect::<CardSet>();
    effects.already_playing.remove(&fixed);
    effects.pending.cancel_where(
        entry.turn,
        ConnectionTransitionReason::Fixed,
        |connection| connection.focus == fixed,
    );
    for card in canceled_cards {
        if !explicitly_clued.contains(&card)
            && !effects
                .pending
                .iter()
                .any(|connection| connection.cards.contains(&card))
        {
            effects.invisibly_clued.remove(&card);
        }
    }
    effects.forced_playable.remove(&fixed);
    push_signal(
        effects.signals,
        entry,
        *giver,
        Some(*target),
        HGroupMoveKind::FixClue,
        vec![fixed],
        None,
    );
    true
}

pub(super) fn apply_fill_in_fix(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) -> bool {
    let entry = context.entry;
    let ObservedEvent::Clued {
        giver,
        target,
        clue,
        touched,
        ..
    } = &entry.event
    else {
        return false;
    };
    if touched.is_empty()
        || !touched
            .iter()
            .all(|card| was_clued_before(view, entry.turn, *card))
    {
        return false;
    }
    let fills_in = touched.iter().any(|card| {
        !view
            .history
            .iter()
            .take_while(|prior| prior.turn < entry.turn)
            .any(
                        |prior| matches!(&prior.event, ObservedEvent::Clued { clue: prior_clue, touched, .. } if prior_clue == clue && touched.contains(card)),
            )
    });
    let facts = context.after.facts;
    let identities = touched
        .iter()
        .filter_map(|card| {
            let identities = IdentitySet::from_mask(facts[card.index()].identity_mask());
            (identities.len() == 1)
                .then(|| identities.iter().next())
                .flatten()
        })
        .collect::<Vec<_>>();
    let duplicate = identities.len() == touched.len()
        && identity_set(identities.iter().copied()).len() < identities.len();
    let stops_existing = touched.iter().any(|card| {
        effects.already_playing.contains(card) && {
            let identities = IdentitySet::from_mask(facts[card.index()].identity_mask());
            !identities.is_empty()
                && identities
                    .iter()
                    .all(|identity| !is_playable_at(context.after.stack_heights, identity))
                && !effects.pending.iter().any(|connection| {
                    connection.focus == *card && effects.pending.is_active(connection)
                })
        }
    });
    if !fills_in || (!duplicate && !stops_existing) {
        return false;
    }
    let canceled_cards = effects
        .pending
        .iter()
        .filter(|connection| touched.contains(&connection.focus))
        .filter(|connection| connection.kind == HGroupConnectionKind::Finesse)
        .flat_map(|connection| connection.cards.iter().copied())
        .collect::<CardSet>();
    effects
        .already_playing
        .retain(|card| !touched.contains(card));
    effects.pending.cancel_where(
        entry.turn,
        ConnectionTransitionReason::Fixed,
        |connection| touched.contains(&connection.focus),
    );
    for card in canceled_cards {
        if !effects.explicitly_clued.contains(&card)
            && !effects
                .pending
                .iter()
                .any(|connection| connection.cards.contains(&card))
        {
            effects.invisibly_clued.remove(&card);
        }
    }
    effects
        .forced_playable
        .retain(|card| !touched.contains(card));
    push_signal(
        effects.signals,
        entry,
        *giver,
        Some(*target),
        HGroupMoveKind::FixClue,
        touched.clone(),
        None,
    );
    true
}

pub(super) fn apply_sarcastic_discard(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    signals: &mut ConventionJournal,
) {
    let ObservedEvent::Discarded {
        player,
        card,
        identity,
    } = entry.event
    else {
        return;
    };
    if was_clued_before(view, entry.turn, card)
        && view.hands.iter().flatten().any(|candidate| {
            candidate.id != card
                && candidate.identity == Some(identity)
                && was_clued_before(view, entry.turn, candidate.id)
        })
    {
        push_signal(
            signals,
            entry,
            player,
            None,
            HGroupMoveKind::SarcasticDiscard,
            vec![card],
            Some(identity),
        );
    }
}
