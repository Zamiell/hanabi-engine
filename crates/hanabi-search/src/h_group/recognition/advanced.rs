use super::{
    Card, CardId, CardSet, Clue, ConnectionManager, ConnectionObligation,
    ConnectionTransitionReason, ConventionJournal, HGroupClueInterpretation, HGroupClueKind,
    HGroupConnectionKind, HGroupMoveKind, HGroupRuleEffects, HGroupTurnContext, IdentitySet,
    ObservedEvent, ObservedHistoryEntry, PlayerView, PromiseId, Rank, RequiredFix, chop,
    finesse_position_id, five_pulled_card, four_charm_blind_plays, identity_of, is_playable_at,
    is_trash_at, next_player, protected_cards, push_signal, same_turn_signal, was_clued_before,
};
use crate::h_group::model::FixObligations;

pub(in crate::h_group) fn apply_elimination_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    stack_heights: [u8; 5],
    signals: &mut ConventionJournal,
) {
    // Sources: https://hanabi.github.io/level-18/#elimination--elimination-notes
    // https://hanabi.github.io/level-18/#the-elimination-blind-play
    // https://hanabi.github.io/level-18/#the-elimination-play-clue
    // https://hanabi.github.io/level-18/#the-elimination-finesse
    match &entry.event {
        ObservedEvent::Discarded {
            player,
            card,
            identity,
        } if !is_trash_at(stack_heights, *identity)
            && (identity.rank == Rank::Two || is_playable_at(stack_heights, *identity))
            && view
                .history
                .iter()
                .filter(|prior| prior.turn < entry.turn)
                .filter(|prior| {
                    matches!(
                        prior.event,
                        ObservedEvent::Discarded {
                            identity: removed,
                            ..
                        } | ObservedEvent::Played {
                            identity: removed,
                            successful: false,
                            ..
                        } if removed == *identity
                    )
                })
                .count()
                + 1
                < usize::from(identity.rank.copies())
            && !hands.iter().enumerate().any(|(owner, hand)| {
                owner != player.index()
                    && hand
                        .iter()
                        .any(|candidate| identity_of(view, *candidate) == Some(*identity))
            }) =>
        {
            push_signal(
                signals,
                entry,
                *player,
                Some(*player),
                HGroupMoveKind::Elimination,
                hands[player.index()].clone(),
                Some(*identity),
            );
            let _ = card;
        }
        ObservedEvent::Clued {
            giver,
            target,
            touched,
            untouched,
            ..
        } => {
            let has_notes = signals.iter().any(|signal| {
                signal.kind == HGroupMoveKind::Elimination
                    && signal.target == Some(*target)
                    && signal.identity.is_some()
            });
            let singled_out = touched.len() == 1 || untouched.len() == 1;
            if has_notes
                && singled_out
                && touched
                    .iter()
                    .all(|card| was_clued_before(view, entry.turn, *card))
            {
                let cards = if touched.len() == 1 {
                    touched.clone()
                } else {
                    untouched.clone()
                };
                push_signal(
                    signals,
                    entry,
                    *giver,
                    Some(*target),
                    HGroupMoveKind::Elimination,
                    cards.clone(),
                    None,
                );
                push_signal(
                    signals,
                    entry,
                    *giver,
                    Some(*target),
                    if touched.iter().all(|card| {
                        identity_of(view, *card)
                            .is_some_and(|identity| is_trash_at(stack_heights, identity))
                    }) {
                        HGroupMoveKind::TrashTouchElimination
                    } else {
                        HGroupMoveKind::EliminationPlayClue
                    },
                    cards,
                    None,
                );
            }
        }
        ObservedEvent::Played { .. }
        | ObservedEvent::Drew { .. }
        | ObservedEvent::Discarded { .. } => {}
    }
}

pub(in crate::h_group) fn apply_elimination_resolution_effects(
    context: &HGroupTurnContext<'_>,
    effects: &mut HGroupRuleEffects<'_>,
) {
    // Sources:
    // - https://hanabi.github.io/level-18/#the-elimination-blind-play
    // - https://hanabi.github.io/level-18/#the-elimination-riding-deduction
    // - https://hanabi.github.io/level-18/#the-elimination-self-chop-move
    let entry = context.entry;
    let claims = effects
        .signals
        .facts()
        .identity_claims()
        .iter()
        .filter(|claim| {
            matches!(
                claim.source,
                HGroupMoveKind::Elimination | HGroupMoveKind::EliminationRewrite
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    for claim in claims {
        let Some(target) = claim.target else {
            continue;
        };
        let candidates_after = context.after.hands[target.index()]
            .iter()
            .copied()
            .filter(|card| {
                claim.cards.contains(card)
                    && context.after.facts[card.index()].allows(claim.identity)
            })
            .collect::<Vec<_>>();
        if is_trash_at(context.after.stack_heights, claim.identity) {
            for card in &claim.cards {
                effects.forced_playable.remove(card);
                effects.chop_moved.remove(card);
            }
            continue;
        }
        if candidates_after.len() == 1
            && !effects.explicitly_clued.contains(&candidates_after[0])
            && !effects.signals.iter().any(|signal| {
                signal.turn < entry.turn
                    && signal.kind == HGroupMoveKind::EliminationSelfChopMove
                    && signal.cards == candidates_after
            })
        {
            let card = candidates_after[0];
            effects.chop_moved.insert(card);
            if is_playable_at(context.after.stack_heights, claim.identity) {
                effects.forced_playable.insert(card);
            }
            push_signal(
                effects.signals,
                entry,
                claim.actor,
                Some(target),
                HGroupMoveKind::EliminationSelfChopMove,
                vec![card],
                None,
            );
        }

        let ObservedEvent::Played {
            player,
            card,
            successful: true,
            ..
        } = entry.event
        else {
            continue;
        };
        if player != target || !claim.cards.contains(&card) {
            continue;
        }
        let candidates_before = context.before.hands[target.index()]
            .iter()
            .copied()
            .filter(|candidate| {
                claim.cards.contains(candidate)
                    && context.before.facts[candidate.index()].allows(claim.identity)
            })
            .collect::<Vec<_>>();
        let kind = if candidates_before.len() == 1 {
            Some(HGroupMoveKind::EliminationBlindPlay)
        } else if candidates_before.len() == 2
            && chop(&context.before.hands[target.index()], effects.chop_moved) != Some(card)
        {
            Some(HGroupMoveKind::EliminationRiding)
        } else {
            None
        };
        if let Some(kind) = kind {
            push_signal(
                effects.signals,
                entry,
                player,
                Some(target),
                kind,
                vec![card],
                None,
            );
        }
    }
}
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(in crate::h_group) fn apply_five_tech_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    clues: &[HGroupClueInterpretation],
    stack_heights: [u8; 5],
    explicitly_clued: &CardSet,
    invisibly_clued: &CardSet,
    chop_moved: &mut CardSet,
    pending: &mut ConnectionManager,
    forced_playable: &mut CardSet,
    implicit_saves: &mut Vec<(CardId, IdentitySet)>,
    signals: &mut ConventionJournal,
) {
    // Sources: https://hanabi.github.io/level-19/#the-early-5s-chop-move
    // https://hanabi.github.io/level-19/#the-5-pull
    // https://hanabi.github.io/level-19/#the-5-number-ejection-5ne
    // https://hanabi.github.io/level-19/#the-5-number-discharge-5nd
    let ObservedEvent::Clued {
        giver,
        target,
        clue: Clue::Rank(Rank::Five),
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    if touched.is_empty() {
        return;
    }
    let repeated_five = touched
        .iter()
        .all(|card| was_clued_before(view, entry.turn, *card));
    let gotten = protected_cards(explicitly_clued, invisibly_clued, chop_moved);
    let target_chop = chop(&hands[target.index()], &gotten);
    let visible_saved_two = clues
        .iter()
        .rev()
        .filter(|interpretation| interpretation.turn < entry.turn)
        .filter(|interpretation| interpretation.target != *target)
        .filter(|interpretation| !interpretation.save_identities.is_empty())
        .find_map(|interpretation| {
            let twos = IdentitySet::from_mask(
                interpretation
                    .save_identities
                    .iter()
                    .filter(|identity| identity.rank == Rank::Two)
                    .fold(0, |mask, identity| mask | (1 << identity.index())),
            );
            (!twos.is_empty()).then_some(twos)
        });
    if repeated_five
        && same_turn_signal(signals, entry.turn, HGroupMoveKind::FiveStall)
        && same_turn_signal(signals, entry.turn, HGroupMoveKind::Stall)
    {
        if let (Some(chop), Some(identities)) = (target_chop, visible_saved_two) {
            // Interaction Between 2 Saves & 5 Stalls: the receiver excludes
            // their chop (the giver could have saved it directly) and writes
            // elimination notes on every otherwise-unclued off-chop card.
            // Source: https://hanabi.github.io/level-19/#interaction-between-2-saves--5-stalls
            let candidates = view.hands[target.index()]
                .iter()
                .filter(|card| {
                    card.id != chop
                        && !touched.contains(&card.id)
                        && !explicitly_clued.contains(&card.id)
                        && identities
                            .iter()
                            .any(|identity| card.clues.allows(identity))
                })
                .map(|card| card.id)
                .collect::<Vec<_>>();
            if !candidates.is_empty() {
                implicit_saves.extend(candidates.iter().copied().map(|card| (card, identities)));
                push_signal(
                    signals,
                    entry,
                    *giver,
                    Some(*target),
                    HGroupMoveKind::FiveStall,
                    touched.clone(),
                    None,
                );
                return;
            }
        }
    }
    if signals
        .iter()
        .any(|signal| signal.turn == entry.turn && signal.kind == HGroupMoveKind::FiveStall)
    {
        return;
    }
    // Save clues take precedence over 5 tech. A rank-5 clue that can be a
    // 5 Save cannot simultaneously pull the adjacent card.
    if clues.last().is_some_and(|interpretation| {
        interpretation.turn == entry.turn && !interpretation.save_identities.is_empty()
    }) {
        return;
    }
    let Some(pulled) = five_pulled_card(&hands[target.index()], touched, &gotten) else {
        return;
    };
    let Some(identity) = identity_of(view, pulled) else {
        return;
    };
    let height = stack_heights[identity.suit.index()];
    let actor = next_player(*giver, hands.len());
    let (kind, forced) = if identity.rank.number() <= height {
        let Some(card) = finesse_position_id(&hands[actor.index()], &gotten, 2).filter(|card| {
            identity_of(view, *card)
                .is_some_and(|candidate| is_playable_at(stack_heights, candidate))
        }) else {
            return;
        };
        chop_moved.insert(pulled);
        (HGroupMoveKind::FiveNumberDischarge, Some(card))
    } else if identity.rank.number() == height + 1 {
        (HGroupMoveKind::FivePull, Some(pulled))
    } else if identity.rank.number() == height + 2 {
        if actor == *target {
            return;
        }
        let connector = Card::new(identity.suit, Rank::ALL[usize::from(height)]);
        let Some(card) = finesse_position_id(&hands[actor.index()], &gotten, 0)
            .filter(|card| identity_of(view, *card) == Some(connector))
        else {
            return;
        };
        pending.cancel_where(
            entry.turn,
            ConnectionTransitionReason::Superseded,
            |connection| connection.actor == actor || connection.actor == *target,
        );
        pending.start(
            entry.turn,
            ConnectionObligation {
                promise: PromiseId::UNASSIGNED,
                actor,
                cards: vec![card],
                expected: connector,
                focus_identity: identity,
                kind: HGroupConnectionKind::Finesse,
                focus: pulled,
                step: 0,
            },
        );
        pending.start(
            entry.turn,
            ConnectionObligation {
                promise: PromiseId::UNASSIGNED,
                actor: *target,
                cards: vec![pulled],
                expected: identity,
                focus_identity: identity,
                kind: HGroupConnectionKind::Finesse,
                focus: pulled,
                step: 1,
            },
        );
        (HGroupMoveKind::FivePull, None)
    } else {
        let Some(card) = finesse_position_id(&hands[actor.index()], &gotten, 1).filter(|card| {
            identity_of(view, *card)
                .is_some_and(|candidate| is_playable_at(stack_heights, candidate))
        }) else {
            return;
        };
        chop_moved.insert(pulled);
        (HGroupMoveKind::FiveNumberEjection, Some(card))
    };
    if let Some(forced) = forced {
        pending.cancel_where(
            entry.turn,
            ConnectionTransitionReason::Superseded,
            |connection| connection.actor == actor,
        );
        forced_playable.insert(forced);
    }
    push_signal(
        signals,
        entry,
        *giver,
        Some(*target),
        kind,
        touched.clone(),
        touched.iter().find_map(|card| identity_of(view, *card)),
    );
    if matches!(
        kind,
        HGroupMoveKind::FiveNumberEjection | HGroupMoveKind::FiveNumberDischarge
    ) {
        push_signal(
            signals,
            entry,
            *giver,
            Some(*target),
            if kind == HGroupMoveKind::FiveNumberEjection {
                HGroupMoveKind::Ejection
            } else {
                HGroupMoveKind::Discharge
            },
            touched.clone(),
            touched.iter().find_map(|card| identity_of(view, *card)),
        );
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(in crate::h_group) fn apply_out_of_order_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    clues: &[HGroupClueInterpretation],
    stack_heights: [u8; 5],
    explicitly_clued: &CardSet,
    invisibly_clued: &CardSet,
    chop_moved: &CardSet,
    pending: &mut ConnectionManager,
    _forced_playable: &mut CardSet,
    required_fixes: &mut FixObligations,
    signals: &mut ConventionJournal,
) {
    // Sources: https://hanabi.github.io/level-20/#the-occupied-play-clue--the-occupied-finesse-opc
    // https://hanabi.github.io/level-20/#the-out-of-order-play-clue-triple-o--ooo
    // https://hanabi.github.io/level-20/#the-out-of-order-finesse
    // https://hanabi.github.io/level-20/#the-suboptimal-prompt--the-suboptimal-finesse--the-suboptimal-bluff
    // https://hanabi.github.io/level-20/#the-no-information-finesse
    // https://hanabi.github.io/level-20/#the-no-information-double-bluff-nidb
    let ObservedEvent::Clued {
        giver,
        target,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    let interpretation = clues
        .iter()
        .rev()
        .find(|clue| clue.turn == entry.turn)
        .filter(|clue| {
            matches!(clue.kind, HGroupClueKind::Play)
                || (clue.kind == HGroupClueKind::Unrecognized && clue.save_identities.is_empty())
        });
    let four_charm = interpretation.is_some_and(|meaning| {
        if *target == next_player(*giver, hands.len())
            || was_clued_before(view, entry.turn, meaning.focus)
        {
            return false;
        }
        let Some(focus_identity) = identity_of(view, meaning.focus) else {
            return false;
        };
        if focus_identity.rank != Rank::Four || stack_heights[focus_identity.suit.index()] != 0 {
            return false;
        }
        if same_turn_signal(signals, entry.turn, HGroupMoveKind::CriticalColorBluff)
            || same_turn_signal(signals, entry.turn, HGroupMoveKind::DoubleBluff)
        {
            return false;
        }
        let actor = next_player(*giver, hands.len());
        if four_charm_blind_plays(view, actor, focus_identity, stack_heights, explicitly_clued) < 3
        {
            return false;
        }
        let gotten = protected_cards(explicitly_clued, invisibly_clued, chop_moved);
        let charmed = finesse_position_id(&hands[actor.index()], &gotten, 3).is_some_and(|card| {
            identity_of(view, card).map_or(actor == view.observer, |identity| {
                is_playable_at(stack_heights, identity)
            })
        });
        let double_bluff_actor = next_player(actor, hands.len());
        let double_bluff_available =
            finesse_position_id(&hands[double_bluff_actor.index()], &gotten, 0)
                .and_then(|card| identity_of(view, card))
                .is_some_and(|identity| is_playable_at(stack_heights, identity));
        charmed && !double_bluff_available
    });
    if four_charm {
        // A 4 Charm replaces the apparent multi-card Out-of-Order Play. It
        // must not manufacture a Fix obligation for the focused 4.
        // Source: https://hanabi.github.io/level-23/#the-4-charm
        return;
    }
    if let Some(card) = interpretation.map(|clue| clue.focus).filter(|card| {
        identity_of(view, *card).is_some_and(|identity| {
            identity.rank.number() > stack_heights[identity.suit.index()] + 1
                && touched.iter().any(|candidate| {
                    candidate != card
                        && interpretation
                            .is_some_and(|clue| !clue.previously_gotten.contains(candidate))
                        && identity_of(view, *candidate).is_some_and(|lower| {
                            lower.suit == identity.suit
                                && lower.rank.number() < identity.rank.number()
                                && lower.rank.number() > stack_heights[identity.suit.index()]
                        })
                })
        })
    }) {
        let focus = card;
        let focus_identity = identity_of(view, focus);
        if let Some(identity) = focus_identity {
            required_fixes.insert_unconditional(RequiredFix {
                actor: next_player(*giver, view.hands.len()),
                target: *target,
                focus,
                identity,
            });
        }
        push_signal(
            signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::OccupiedPlay,
            vec![focus],
            focus_identity,
        );
        push_signal(
            signals,
            entry,
            *giver,
            Some(*target),
            if pending.iter().any(|connection| connection.focus == focus) {
                HGroupMoveKind::OutOfOrderFinesse
            } else {
                HGroupMoveKind::OutOfOrderPlay
            },
            vec![focus],
            focus_identity,
        );
    }
    let current_connections = pending
        .iter()
        .filter(|connection| {
            interpretation.is_some_and(|meaning| connection.focus == meaning.focus)
        })
        .collect::<Vec<_>>();
    let no_information = !touched.is_empty()
        && touched
            .iter()
            .all(|card| was_clued_before(view, entry.turn, *card));
    let suboptimal = current_connections.iter().any(|connection| {
        connection.cards.len() > 1
            || connection
                .cards
                .first()
                .is_some_and(|card| hands[connection.actor.index()].last() != Some(card))
    });
    if suboptimal {
        push_signal(
            signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::SuboptimalConnection,
            current_connections
                .iter()
                .flat_map(|connection| connection.cards.iter().copied())
                .collect(),
            None,
        );
    }
    if no_information && !current_connections.is_empty() {
        push_signal(
            signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::NoInformationFinesse,
            current_connections
                .iter()
                .flat_map(|connection| connection.cards.iter().copied())
                .collect(),
            None,
        );
    }
    if no_information && same_turn_signal(signals, entry.turn, HGroupMoveKind::DoubleBluff) {
        push_signal(
            signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::NoInformationDoubleBluff,
            touched.clone(),
            None,
        );
    }
}
