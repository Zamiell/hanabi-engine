use super::{
    Card, CardId, CardSet, Clue, ConnectionManager, ConnectionTransitionReason, ConventionJournal,
    HGroupClueInterpretation, HGroupClueKind, HGroupMoveKind, HGroupRuleEffects, HGroupTurnContext,
    IdentitySet, ObservedEvent, ObservedHistoryEntry, PlayerId, PlayerView, Rank, card_is_trash,
    chop, finesse_position_id, focus, has_higher_basic_priority, identity_of, is_playable_at,
    is_trash_at, next_player, pending_identity_is_queued, protected_cards, push_signal,
    was_clued_before,
};

#[allow(clippy::too_many_lines)]
pub(in crate::h_group) fn apply_ignition_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) {
    // Sources:
    // - https://hanabi.github.io/level-21/#double-ignition-di
    // - https://hanabi.github.io/level-21/#the-replay-double-ignition-rdi
    // - https://hanabi.github.io/level-21/#the-trash-double-ignition-tdi
    // - https://hanabi.github.io/level-21/#the-poke-double-ignition-pdi
    // - https://hanabi.github.io/level-21/#the-chop-move-ignition-cmi-with-1-card-chop-moved
    // - https://hanabi.github.io/level-21/#the-chop-move-ignition-cmi-with-2-cards-chop-moved
    // - https://hanabi.github.io/level-21/#bomb-double-ignition
    // - https://hanabi.github.io/level-21/#bomb-triple-ignition
    let entry = context.entry;
    if let ObservedEvent::Played {
        player,
        card,
        identity,
        successful: false,
    } = &entry.event
    {
        let globally_known_trash = is_trash_at(context.before.stack_heights, *identity)
            && (effects.explicitly_clued.contains(card) || effects.invisibly_clued.contains(card));
        // Positional Misplays are the ordinary end-game meaning. A known-trash
        // bomb while cards remain in the deck is instead the explicit
        // Double/Triple Ignition message.
        if !globally_known_trash || context.before.deck_size == 0 {
            return;
        }
        let gotten = protected_cards(
            effects.explicitly_clued,
            effects.invisibly_clued,
            effects.chop_moved,
        );
        let clue_form_available = context.before.clue_tokens > 0
            && context
                .before
                .hands
                .iter()
                .flatten()
                .copied()
                .any(|candidate| {
                    candidate != *card
                        && effects.explicitly_clued.contains(&candidate)
                        && context.historical.identity(candidate).is_some_and(|known| {
                            is_playable_at(context.before.stack_heights, known)
                                || is_trash_at(context.before.stack_heights, known)
                        })
                });
        let requested = if clue_form_available { 3 } else { 2 };
        let mut ignited = (1..context.after.hands.len())
            .filter_map(|distance| {
                let target = PlayerId::new(
                    u8::try_from((player.index() + distance) % context.after.hands.len()).ok()?,
                );
                let candidate =
                    finesse_position_id(&context.after.hands[target.index()], &gotten, 0)?;
                context
                    .historical
                    .identity(candidate)
                    .is_some_and(|known| is_playable_at(context.before.stack_heights, known))
                    .then_some(candidate)
            })
            .collect::<Vec<_>>();
        if ignited.len() < requested {
            return;
        }
        ignited.truncate(requested);
        effects.forced_playable.extend(ignited.iter().copied());
        let exact = if requested == 3 {
            HGroupMoveKind::BombTripleIgnition
        } else {
            HGroupMoveKind::BombDoubleIgnition
        };
        push_signal(
            effects.signals,
            entry,
            *player,
            None,
            exact,
            ignited.clone(),
            None,
        );
        push_signal(
            effects.signals,
            entry,
            *player,
            None,
            HGroupMoveKind::Ignition,
            ignited,
            None,
        );
        return;
    }
    let ObservedEvent::Clued {
        giver,
        target,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    if touched.is_empty() {
        return;
    }

    let stack_heights = context.before.stack_heights;
    let gotten = protected_cards(
        effects.explicitly_clued,
        effects.invisibly_clued,
        effects.chop_moved,
    );
    let all_previously_touched = touched
        .iter()
        .all(|card| was_clued_before(view, entry.turn, *card));
    let touched_identities = touched
        .iter()
        .map(|card| context.historical.identity(*card))
        .collect::<Option<Vec<_>>>();
    let globally_playable = touched_identities.as_ref().is_some_and(|identities| {
        identities
            .iter()
            .all(|identity| is_playable_at(stack_heights, *identity))
    });
    let globally_trash = touched_identities.as_ref().is_some_and(|identities| {
        identities
            .iter()
            .all(|identity| is_trash_at(stack_heights, *identity))
    });

    let actor_locked = !context.before.hands[giver.index()].is_empty()
        && context.before.hands[giver.index()]
            .iter()
            .all(|card| gotten.contains(card));
    let end_game = context.before.deck_size <= context.before.hands.len();
    let stalling = actor_locked
        || context.before.clue_tokens == 8
        || end_game
        || effects.signals.iter().any(|signal| {
            signal.turn == entry.turn
                && matches!(
                    signal.kind,
                    HGroupMoveKind::Stall | HGroupMoveKind::FiveStall
                )
        });

    let same_turn_chop_move = effects
        .signals
        .iter()
        .find(|signal| signal.turn == entry.turn && signal.kind == HGroupMoveKind::ChopMove)
        .and_then(|signal| {
            signal.cards.iter().copied().find(|card| {
                context
                    .historical
                    .identity(*card)
                    .is_some_and(|identity| is_playable_at(stack_heights, identity))
            })
        });
    if let Some(chop_moved_playable) = same_turn_chop_move {
        let actor = next_player(*giver, context.after.hands.len());
        let actor_is_occupied = effects
            .pending
            .iter()
            .any(|connection| connection.actor == actor)
            || context.after.hands[actor.index()].iter().any(|card| {
                effects.already_playing.contains(card) || effects.forced_playable.contains(card)
            });
        // A Chop Move Ignition promises an immediate blind play to the next
        // player. It has no Minimum Clue Value when that player already has a
        // pending connection or another promised play.
        if actor_is_occupied {
            return;
        }
        if let Some(ignited) = finesse_position_id(&context.after.hands[actor.index()], &gotten, 0)
        {
            effects.forced_playable.insert(ignited);
            push_signal(
                effects.signals,
                entry,
                *giver,
                Some(actor),
                HGroupMoveKind::ChopMoveIgnition,
                vec![ignited, chop_moved_playable],
                context.historical.identity(ignited),
            );
            push_signal(
                effects.signals,
                entry,
                *giver,
                Some(actor),
                HGroupMoveKind::Ignition,
                vec![ignited, chop_moved_playable],
                None,
            );
        }
        return;
    }

    let primitive_same_turn = |kind| {
        effects
            .signals
            .iter()
            .any(|signal| signal.turn == entry.turn && signal.kind == kind)
    };
    let kind = if all_previously_touched && globally_playable && !stalling {
        Some(HGroupMoveKind::ReplayDoubleIgnition)
    } else if all_previously_touched && globally_trash && !stalling {
        Some(HGroupMoveKind::PokeDoubleIgnition)
    } else if !all_previously_touched
        && globally_trash
        && end_game
        && !primitive_same_turn(HGroupMoveKind::ChopMove)
        && !primitive_same_turn(HGroupMoveKind::TrashPush)
    {
        Some(HGroupMoveKind::TrashDoubleIgnition)
    } else {
        None
    };
    let Some(kind) = kind else {
        return;
    };

    let player_count = context.after.hands.len();
    let ordered_players = (1..player_count)
        .map(|distance| {
            PlayerId::new(
                u8::try_from((giver.index() + distance) % player_count)
                    .expect("standard Hanabi has at most five players"),
            )
        })
        .collect::<Vec<_>>();
    let known_playable = |player: PlayerId| {
        context.after.hands[player.index()].iter().any(|card| {
            let identities =
                IdentitySet::from_mask(context.after.facts[card.index()].identity_mask());
            !identities.is_empty()
                && identities
                    .iter()
                    .all(|identity| is_playable_at(stack_heights, identity))
        })
    };
    let first = ordered_players
        .iter()
        .copied()
        .find(|player| !known_playable(*player));
    let Some(first) = first else {
        return;
    };
    let first_card = finesse_position_id(&context.after.hands[first.index()], &gotten, 0);
    let second = ordered_players.iter().rev().copied().find_map(|player| {
        if player == first {
            return None;
        }
        let card = finesse_position_id(&context.after.hands[player.index()], &gotten, 0)?;
        let is_playable = context
            .historical
            .identity(card)
            .is_some_and(|identity| is_playable_at(stack_heights, identity));
        (is_playable || player == view.observer).then_some((player, card))
    });
    let (Some(first_card), Some((_, second_card))) = (first_card, second) else {
        return;
    };

    effects.forced_playable.insert(first_card);
    effects.forced_playable.insert(second_card);
    push_signal(
        effects.signals,
        entry,
        *giver,
        Some(*target),
        kind,
        vec![first_card, second_card],
        None,
    );
    push_signal(
        effects.signals,
        entry,
        *giver,
        Some(*target),
        HGroupMoveKind::Ignition,
        vec![first_card, second_card],
        None,
    );
}
#[allow(clippy::too_many_lines)]
pub(in crate::h_group) fn apply_phantom_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) {
    // Sources:
    // - https://hanabi.github.io/level-22/#phantom-playable-cards
    // - https://hanabi.github.io/level-22/#the-scream-discard-for-a-phantom-playable-card
    // - https://hanabi.github.io/level-22/#the-sacrifice-discard
    // - https://hanabi.github.io/level-22/#the-echo-scream-discard-chop-move-esdcm
    // - https://hanabi.github.io/level-22/#the-composition-discard
    // - https://hanabi.github.io/level-22/#the-rebellious-discard
    let entry = context.entry;
    let ObservedEvent::Discarded {
        player,
        card,
        identity,
    } = &entry.event
    else {
        return;
    };
    let hands = context.after.hands;
    let stack_heights = context.before.stack_heights;
    let gotten = protected_cards(
        effects.explicitly_clued,
        effects.invisibly_clued,
        effects.chop_moved,
    );
    let safe_action = |actor: PlayerId| {
        hands[actor.index()].iter().any(|candidate| {
            let identities =
                IdentitySet::from_mask(context.after.facts[candidate.index()].identity_mask());
            !identities.is_empty()
                && (identities
                    .iter()
                    .all(|candidate| is_playable_at(stack_heights, candidate))
                    || (effects.explicitly_clued.contains(candidate)
                        && identities
                            .iter()
                            .all(|candidate| is_trash_at(stack_heights, candidate))))
        })
    };
    let important = |candidate: CardId| {
        context
            .historical
            .identity(candidate)
            .is_none_or(|candidate| {
                if is_playable_at(stack_heights, candidate) {
                    return true;
                }
                let removed = view
                    .history
                    .iter()
                    .take_while(|prior| prior.turn < entry.turn)
                    .filter(|prior| {
                        matches!(
                            prior.event,
                            ObservedEvent::Discarded { identity: removed, .. }
                                | ObservedEvent::Played {
                                    identity: removed,
                                    successful: false,
                                    ..
                                } if removed == candidate
                        )
                    })
                    .count();
                removed + 1 == usize::from(candidate.rank.copies()) || candidate.rank == Rank::Two
            })
    };
    let phantom = |candidate: CardId| {
        let Some(candidate_identity) = context.historical.identity(candidate) else {
            return false;
        };
        let height = stack_heights[candidate_identity.suit.index()];
        if candidate_identity.rank.number() <= height + 1 {
            return false;
        }
        let mut has_unaccounted_connector = false;
        for rank_number in (height + 1)..candidate_identity.rank.number() {
            let connector = Card::new(
                candidate_identity.suit,
                Rank::ALL[usize::from(rank_number - 1)],
            );
            let Some((owner, connector_card)) =
                hands.iter().enumerate().find_map(|(owner, hand)| {
                    hand.iter()
                        .copied()
                        .find(|card| context.historical.identity(*card) == Some(connector))
                        .map(|card| (owner, card))
                })
            else {
                return false;
            };
            let finesse = finesse_position_id(&hands[owner], &gotten, 0);
            if !gotten.contains(&connector_card) && finesse != Some(connector_card) {
                has_unaccounted_connector = true;
            }
        }
        has_unaccounted_connector
    };

    let same_turn_scream = effects
        .signals
        .iter()
        .find(|signal| signal.turn == entry.turn && signal.kind == HGroupMoveKind::ScreamDiscard)
        .cloned();
    if let Some(scream) = same_turn_scream {
        if scream.cards.iter().copied().any(&phantom) {
            push_signal(
                effects.signals,
                entry,
                *player,
                scream.target,
                HGroupMoveKind::PhantomPlayable,
                scream.cards.clone(),
                None,
            );
        }

        let next = next_player(*player, hands.len());
        let after_next = next_player(next, hands.len());
        let after_next_chop = chop(&hands[after_next.index()], &gotten);
        if safe_action(next) && !safe_action(after_next) && after_next_chop.is_some_and(&important)
        {
            let bounced = after_next_chop.expect("checked above");
            effects.chop_moved.insert(bounced);
            effects.must_clue.insert(after_next);
            push_signal(
                effects.signals,
                entry,
                *player,
                Some(after_next),
                HGroupMoveKind::EchoScreamDiscard,
                scream
                    .cards
                    .iter()
                    .copied()
                    .chain(core::iter::once(bounced))
                    .collect(),
                None,
            );
        } else if hands.len() >= 4 && safe_action(next) && safe_action(after_next) {
            let fourth = next_player(after_next, hands.len());
            if let Some(fourth_chop) =
                chop(&hands[fourth.index()], &gotten).filter(|card| important(*card))
            {
                effects.must_clue.insert(after_next);
                push_signal(
                    effects.signals,
                    entry,
                    *player,
                    Some(after_next),
                    HGroupMoveKind::CompositionDiscard,
                    scream
                        .cards
                        .iter()
                        .copied()
                        .chain(core::iter::once(fourth_chop))
                        .collect(),
                    None,
                );
            }
        }
    }

    let previously_screamed = effects
        .signals
        .iter()
        .rev()
        .find(|signal| {
            signal.turn < entry.turn
                && signal.target == Some(*player)
                && matches!(
                    signal.kind,
                    HGroupMoveKind::ScreamDiscard | HGroupMoveKind::EchoScreamDiscard
                )
        })
        .cloned();
    if previously_screamed.as_ref().is_some_and(|previous| {
        !view.history.iter().any(|history| {
            history.turn > previous.turn
                && history.turn < entry.turn
                && matches!(
                    history.event,
                    ObservedEvent::Played { player: actor, .. }
                        | ObservedEvent::Discarded { player: actor, .. }
                        | ObservedEvent::Clued { giver: actor, .. } if actor == *player
                )
        })
    }) {
        let target = next_player(*player, hands.len());
        push_signal(
            effects.signals,
            entry,
            *player,
            Some(target),
            HGroupMoveKind::RebelliousDiscard,
            vec![*card],
            Some(*identity),
        );
    }

    let before_hand = &context.before.hands[player.index()];
    let locked = !before_hand.is_empty() && before_hand.iter().all(|held| gotten.contains(held));
    let discarded_was_clued = effects.explicitly_clued.contains(card);
    let removed_before = view
        .history
        .iter()
        .take_while(|prior| prior.turn < entry.turn)
        .filter(|prior| {
            matches!(
                prior.event,
                ObservedEvent::Discarded { identity: removed, .. }
                    | ObservedEvent::Played {
                        identity: removed,
                        successful: false,
                        ..
                    } if removed == *identity
            )
        })
        .count();
    let non_critical = removed_before + 1 < usize::from(identity.rank.copies());
    let not_generation = !effects.signals.iter().any(|signal| {
        signal.turn == entry.turn && signal.kind == HGroupMoveKind::GenerationDiscard
    });
    if locked
        && discarded_was_clued
        && non_critical
        && !is_trash_at(stack_heights, *identity)
        && not_generation
    {
        push_signal(
            effects.signals,
            entry,
            *player,
            None,
            HGroupMoveKind::SacrificeDiscard,
            vec![*card],
            Some(*identity),
        );
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(in crate::h_group) fn apply_charm_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    clues: &[HGroupClueInterpretation],
    stack_heights: [u8; 5],
    explicitly_clued: &CardSet,
    invisibly_clued: &CardSet,
    chop_moved: &CardSet,
    pending: &mut ConnectionManager,
    forced_playable: &mut CardSet,
    signals: &mut ConventionJournal,
) {
    // Sources: https://hanabi.github.io/level-23/#the-4-charm
    // https://hanabi.github.io/level-23/#the-blaze-discard
    // https://hanabi.github.io/level-23/#the-hesitation-blind-play
    match &entry.event {
        ObservedEvent::Clued {
            giver,
            target,
            clue: Clue::Rank(Rank::Four),
            touched,
            ..
        } => {
            let interpretation = clues.iter().rev().find(|clue| clue.turn == entry.turn);
            let charm_focus = interpretation.and_then(|clue| {
                identity_of(view, clue.focus).filter(|identity| {
                    usize::from(identity.rank.number())
                        == usize::from(stack_heights[identity.suit.index()]) + 4
                })
            });
            if let (Some(interpretation), Some(_)) = (interpretation, charm_focus) {
                // Once a rank-4 clue would require three ordinary blind plays,
                // it is either a valid 4 Charm or an invalid clue. Do not leave
                // the generic layered-Finesse interpretation active.
                pending.cancel_where(
                    entry.turn,
                    ConnectionTransitionReason::FocusInvalidated,
                    |connection| connection.focus == interpretation.focus,
                );
                let actor = next_player(*giver, hands.len());
                let gotten = explicitly_clued
                    .union(invisibly_clued)
                    .copied()
                    .chain(chop_moved.iter().copied())
                    .collect::<CardSet>();
                let charmed = (actor != *target)
                    .then(|| finesse_position_id(&hands[actor.index()], &gotten, 3))
                    .flatten()
                    .filter(|card| {
                        identity_of(view, *card)
                            .is_some_and(|identity| is_playable_at(stack_heights, identity))
                    });
                if let Some(charmed) = charmed {
                    pending.cancel_where(
                        entry.turn,
                        ConnectionTransitionReason::Superseded,
                        |connection| connection.actor == actor,
                    );
                    forced_playable.insert(charmed);
                    push_signal(
                        signals,
                        entry,
                        *giver,
                        Some(actor),
                        HGroupMoveKind::Charm,
                        vec![charmed],
                        identity_of(view, charmed),
                    );
                }
            }
            let _ = touched;
        }
        ObservedEvent::Discarded { player, card, .. }
            if !was_clued_before(view, entry.turn, *card)
                && !signals.iter().any(|signal| {
                    signal.turn == entry.turn
                        && matches!(
                            signal.kind,
                            HGroupMoveKind::ScreamDiscard
                                | HGroupMoveKind::ShoutDiscard
                                | HGroupMoveKind::GenerationDiscard
                                | HGroupMoveKind::EchoScreamDiscard
                        )
                }) =>
        {
            let delayed = clues.iter().rev().find(|clue| {
                clue.target == *player
                    && matches!(clue.kind, HGroupClueKind::Play | HGroupClueKind::PlayOrSave)
                    && hands[player.index()].contains(&clue.focus)
                    && !view.history.iter().any(|prior| {
                        prior.turn > clue.turn
                            && prior.turn < entry.turn
                            && match prior.event {
                                ObservedEvent::Played { player: actor, .. }
                                | ObservedEvent::Discarded { player: actor, .. } => {
                                    actor == *player
                                }
                                ObservedEvent::Clued { giver, .. } => giver == *player,
                                ObservedEvent::Drew { .. } => false,
                            }
                    })
            });
            let Some(delayed) = delayed else {
                return;
            };
            let focus_identity = identity_of(view, delayed.focus).or_else(|| {
                (delayed.focus_identities.len() == 1)
                    .then(|| delayed.focus_identities.iter().next())
                    .flatten()
            });
            let Some(focus_identity) = focus_identity else {
                return;
            };
            let height = stack_heights[focus_identity.suit.index()];
            if focus_identity.rank.number() <= height + 1 {
                return;
            }
            let connector = Card::new(focus_identity.suit, Rank::ALL[usize::from(height)]);
            if pending_identity_is_queued(pending, connector) {
                // Hesitation calls for a genuinely missing connector. The
                // focus owner may discard while an earlier teammate's queued
                // connection is still waiting for its next turn; that does
                // not transfer the connection to the following player.
                // Source: https://hanabi.github.io/level-23/#the-hesitation-blind-play
                return;
            }
            let actor = next_player(*player, hands.len());
            let gotten = protected_cards(explicitly_clued, invisibly_clued, chop_moved);
            let hesitation =
                finesse_position_id(&hands[actor.index()], &gotten, 0).filter(|card| {
                    identity_of(view, *card).is_none_or(|identity| identity == connector)
                });
            if let Some(hesitation) = hesitation {
                forced_playable.insert(hesitation);
                push_signal(
                    signals,
                    entry,
                    *player,
                    Some(actor),
                    HGroupMoveKind::HesitationBlindPlay,
                    vec![hesitation, delayed.focus],
                    Some(connector),
                );
            }
        }
        ObservedEvent::Discarded {
            player,
            card,
            identity,
        } if was_clued_before(view, entry.turn, *card) && !card_is_trash(view, *identity) => {
            let gotten = protected_cards(explicitly_clued, invisibly_clued, chop_moved);
            let matching = hands.iter().enumerate().find_map(|(owner, hand)| {
                hand.iter()
                    .copied()
                    .find(|candidate| identity_of(view, *candidate) == Some(*identity))
                    .map(|candidate| (owner, candidate))
            });
            if let Some((owner, matching)) = matching {
                let position = hands[owner]
                    .iter()
                    .rev()
                    .filter(|candidate| !gotten.contains(candidate))
                    .position(|candidate| *candidate == matching);
                if let Some(position) = position.filter(|position| *position > 0) {
                    let actor = next_player(*player, hands.len());
                    if let Some(blaze) =
                        finesse_position_id(&hands[actor.index()], &gotten, position)
                    {
                        forced_playable.insert(blaze);
                        push_signal(
                            signals,
                            entry,
                            *player,
                            Some(actor),
                            HGroupMoveKind::BlazeDiscard,
                            vec![blaze, matching],
                            identity_of(view, blaze),
                        );
                        return;
                    }
                }
            }
            push_signal(
                signals,
                entry,
                *player,
                None,
                HGroupMoveKind::Charm,
                vec![*card],
                Some(*identity),
            );
        }
        _ => {}
    }
}
#[allow(clippy::too_many_lines)]
pub(in crate::h_group) fn apply_unnecessary_move_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) {
    // Sources:
    // - https://hanabi.github.io/level-24/#trash-finesses-and-trash-bluffs-are-always-unnecessary
    // - https://hanabi.github.io/level-24/#unnecessary-moves-with-known-trash--ignition
    // - https://hanabi.github.io/level-24/#unnecessary-moves-with-unknown-trash-off-chop--chop-move
    // - https://hanabi.github.io/level-24/#unnecessary-moves-with-unknown-trash-on-chop--trash-push
    let entry = context.entry;
    let ObservedEvent::Played {
        player,
        card,
        identity,
        successful: true,
    } = &entry.event
    else {
        return;
    };
    if was_clued_before(view, entry.turn, *card) {
        return;
    }
    let Some(connection) = effects.signals.iter().rev().find(|signal| {
        signal.turn < entry.turn
            && signal.cards.contains(card)
            && matches!(
                signal.kind,
                HGroupMoveKind::Finesse
                    | HGroupMoveKind::ReverseFinesse
                    | HGroupMoveKind::SelfFinesse
                    | HGroupMoveKind::LayeredFinesse
                    | HGroupMoveKind::Bluff
                    | HGroupMoveKind::Discharge
                    | HGroupMoveKind::UnknownTrashDischarge
                    | HGroupMoveKind::UnknownDupeDischarge
                    | HGroupMoveKind::Ejection
                    | HGroupMoveKind::FiveColorEjection
            )
    }) else {
        return;
    };
    let Some(origin) = effects
        .clues
        .iter()
        .rev()
        .find(|clue| clue.turn == connection.turn)
        .cloned()
    else {
        return;
    };
    let side_benefit = origin.new_non_focus.iter().copied().any(|extra| {
        context
            .historical
            .identity(extra)
            .is_some_and(|known| !is_trash_at(origin.stack_heights, known))
    }) || effects.signals.iter().any(|signal| {
        signal.turn == origin.turn
            && matches!(
                signal.kind,
                HGroupMoveKind::FixClue
                    | HGroupMoveKind::DuplicitousValue
                    | HGroupMoveKind::DuplicitousTempo
            )
    });
    if side_benefit {
        return;
    }

    let gotten = protected_cards(
        effects.explicitly_clued,
        effects.invisibly_clued,
        effects.chop_moved,
    );
    let direct_play_available = [Clue::Suit(identity.suit), Clue::Rank(identity.rank)]
        .into_iter()
        .any(|direct| {
            let hand = &context.before.hands[player.index()];
            let touched = hand
                .iter()
                .copied()
                .filter(|candidate| {
                    context
                        .historical
                        .identity(*candidate)
                        .is_some_and(|known| direct.matches(known))
                })
                .collect::<Vec<_>>();
            focus(hand, &touched, chop(hand, &gotten), &gotten) == Some(*card)
        });
    let trash_finesse_or_bluff = context
        .historical
        .identity(origin.focus)
        .is_some_and(|known| is_trash_at(origin.stack_heights, known));
    let discharge_or_ejection = matches!(
        connection.kind,
        HGroupMoveKind::Discharge
            | HGroupMoveKind::UnknownTrashDischarge
            | HGroupMoveKind::UnknownDupeDischarge
            | HGroupMoveKind::Ejection
            | HGroupMoveKind::FiveColorEjection
    );
    // An ordinary Finesse is the direct meaning of its Play Clue; the Level
    // 24 transform applies to fancy trash moves and discharge/ejection
    // substitutes, not to every connection whose card could be clued later.
    if !(trash_finesse_or_bluff || discharge_or_ejection && direct_play_available) {
        return;
    }

    let recipient_knew_trash = effects.signals.iter().any(|signal| {
        signal.turn == origin.turn
            && signal.kind == HGroupMoveKind::TrashPush
            && signal.cards.contains(&origin.focus)
    });
    let (kind, affected) = if recipient_knew_trash {
        let actor = next_player(origin.giver, context.after.hands.len());
        let target = if actor == *player {
            (1..context.after.hands.len()).rev().find_map(|distance| {
                let candidate = PlayerId::new(
                    u8::try_from((origin.giver.index() + distance) % context.after.hands.len())
                        .ok()?,
                );
                let card =
                    finesse_position_id(&context.after.hands[candidate.index()], &gotten, 0)?;
                context
                    .historical
                    .identity(card)
                    .is_some_and(|known| is_playable_at(context.before.stack_heights, known))
                    .then_some(card)
            })
        } else {
            finesse_position_id(&context.after.hands[actor.index()], &gotten, 0)
        };
        let Some(ignited) = target else {
            return;
        };
        effects.forced_playable.insert(ignited);
        (HGroupMoveKind::UnnecessaryIgnition, vec![ignited])
    } else {
        let hand = &context.before.hands[origin.target.index()];
        let Some(position) = hand.iter().position(|candidate| *candidate == origin.focus) else {
            return;
        };
        if origin.focus_was_chop {
            let Some(pushed) = hand.get(position + 1).copied() else {
                return;
            };
            effects.forced_playable.insert(pushed);
            (HGroupMoveKind::UnnecessaryTrashPush, vec![pushed])
        } else {
            let moved = hand[..position]
                .iter()
                .rev()
                .copied()
                .filter(|candidate| !gotten.contains(candidate))
                .collect::<Vec<_>>();
            if moved.is_empty() {
                return;
            }
            effects.chop_moved.extend(moved.iter().copied());
            (HGroupMoveKind::UnnecessaryChopMove, moved)
        }
    };
    push_signal(
        effects.signals,
        entry,
        *player,
        Some(origin.target),
        kind,
        affected,
        Some(*identity),
    );
    push_signal(
        effects.signals,
        entry,
        *player,
        Some(origin.target),
        HGroupMoveKind::UnnecessaryMove,
        vec![origin.focus],
        None,
    );
}
#[allow(clippy::too_many_lines)]
pub(in crate::h_group) fn apply_priority_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    explicitly_clued: &CardSet,
    pending: &ConnectionManager,
    forced_playable: &mut CardSet,
    signals: &mut ConventionJournal,
) {
    // Sources: https://hanabi.github.io/level-25/#the-priority-prompt--the-priority-finesse
    // https://hanabi.github.io/level-25/#the-priority-bluff
    // https://hanabi.github.io/level-25/#the-layered-priority-finesse
    // https://hanabi.github.io/level-25/#the-load-clue
    // https://hanabi.github.io/level-25/#the-paused-priority-finesse
    // https://hanabi.github.io/level-25/#the-trust-finesse-a-priority-finesse-from-playing-an-unknown-card
    let entry = context.entry;
    let hands = &context.before.hands;
    let facts = &context.before.facts;
    let stack_heights = context.before.stack_heights;
    if let ObservedEvent::Clued {
        target,
        clue,
        touched,
        ..
    } = &entry.event
    {
        // A clue matching a pending Priority connector but touching a
        // different card is the Load Clue that disproves the provisional
        // Finesse-Position interpretation.
        let convention_facts = signals.facts();
        let canceled = signals
            .iter()
            .filter(|signal| {
                signal.kind == HGroupMoveKind::Priority
                    && signal
                        .cards
                        .iter()
                        .any(|card| convention_facts.active_priority().contains(card))
                    && signal.target == Some(*target)
                    && signal
                        .identity
                        .is_some_and(|identity| clue.matches(identity))
                    && signal.cards.iter().all(|card| !touched.contains(card))
            })
            .flat_map(|signal| signal.cards.iter().copied())
            .collect::<CardSet>();
        for card in &canceled {
            forced_playable.remove(card);
        }
        if !canceled.is_empty() {
            push_signal(
                signals,
                entry,
                *target,
                Some(*target),
                HGroupMoveKind::Retraction,
                canceled.iter().copied().collect(),
                None,
            );
        }
        return;
    }
    let ObservedEvent::Played {
        player,
        card,
        identity,
        successful: true,
        ..
    } = &entry.event
    else {
        return;
    };

    // Ordinary Priority is a deliberate choice between globally-known playable
    // cards. Trust Finesses from unknown cards need substantially stronger
    // evidence and must not be inferred merely from hidden simulator truth.
    let played_possibilities = IdentitySet::from_mask(facts[card.index()].identity_mask());
    let convention_facts = signals.facts();
    let fixed_cards = convention_facts.fixed_cards();
    let played_is_known = played_possibilities == IdentitySet::singleton(*identity)
        || convention_facts.known_identity(*card) == Some(*identity);
    let advances_existing_connection = signals.iter().any(|signal| {
        signal.turn < entry.turn
            && signal.cards.first() == Some(card)
            && signal.target == Some(*player)
            && pending.iter().any(|connection| {
                connection.actor == *player && signal.identity == Some(connection.expected)
            })
            && matches!(
                signal.kind,
                HGroupMoveKind::Prompt
                    | HGroupMoveKind::Finesse
                    | HGroupMoveKind::ReverseFinesse
                    | HGroupMoveKind::LayeredFinesse
                    | HGroupMoveKind::ClandestineFinesse
                    | HGroupMoveKind::QueuedFinesse
                    | HGroupMoveKind::AmbiguousFinesse
            )
    });
    if !played_is_known || fixed_cards.contains(card) || advances_existing_connection {
        // Priority communicates a deliberate choice between otherwise-free
        // plays. Playing the first card of an existing connection is already
        // explained by that connection and cannot create an unrelated
        // Priority Finesse in the next player's hidden hand.
        // Source: https://hanabi.github.io/level-25/#the-priority-prompt--the-priority-finesse
        return;
    }

    let actor_hand = &hands[player.index()];
    let declined_priority = actor_hand.iter().copied().find(|candidate| {
        if *candidate == *card || fixed_cards.contains(candidate) {
            return false;
        }
        let clue_possibilities = IdentitySet::from_mask(facts[candidate.index()].identity_mask());
        let possibilities = convention_facts
            .demonstrated_layer(*candidate)
            .map_or(clue_possibilities, IdentitySet::singleton);
        !possibilities.is_empty()
            && possibilities.iter().all(|candidate_identity| {
                is_playable_at(stack_heights, candidate_identity)
                    && has_higher_basic_priority(
                        view,
                        hands,
                        facts,
                        forced_playable,
                        *player,
                        actor_hand,
                        *candidate,
                        candidate_identity,
                        *card,
                        *identity,
                    )
            })
    });

    if declined_priority.is_some() {
        let mut priority_connection = None;
        if identity.rank != Rank::Five {
            let connector = Card::new(identity.suit, Rank::ALL[identity.rank.index() + 1]);
            let players = (1..hands.len())
                .map(|offset| {
                    PlayerId::new(
                        u8::try_from((player.index() + offset) % hands.len())
                            .expect("standard Hanabi has at most five players"),
                    )
                })
                .collect::<Vec<_>>();

            // A Priority Prompt can land in any later hand, and a Priority
            // Finesse can deliberately skip the next player. First locate a
            // visible connector. If it is unclued, it is only a finesse when
            // it occupies that player's current Finesse Position; otherwise
            // the move commits the team to a Load Clue instead.
            let prompt = players.iter().find_map(|target| {
                hands[target.index()]
                    .iter()
                    .rev()
                    .copied()
                    .find(|candidate| {
                        explicitly_clued.contains(candidate)
                            && context.historical.identity(*candidate) == Some(connector)
                    })
                    .map(|candidate| (*target, candidate))
            });
            let visible_connector = players.iter().find_map(|target| {
                hands[target.index()]
                    .iter()
                    .copied()
                    .find(|candidate| context.historical.identity(*candidate) == Some(connector))
                    .map(|candidate| (*target, candidate))
            });
            let visible_finesse = visible_connector.and_then(|(target, connector_card)| {
                hands[target.index()]
                    .iter()
                    .rev()
                    .copied()
                    .find(|candidate| !explicitly_clued.contains(candidate))
                    .filter(|finesse_position| *finesse_position == connector_card)
                    .map(|candidate| (target, candidate))
            });
            let subjective_finesse = (visible_connector.is_none() && view.observer != *player)
                .then(|| {
                    hands[view.observer.index()]
                        .iter()
                        .rev()
                        .copied()
                        .find(|candidate| !explicitly_clued.contains(candidate))
                        .map(|candidate| (view.observer, candidate))
                })
                .flatten();
            priority_connection = prompt.or(visible_finesse).or(subjective_finesse);
            if let Some((_, connection)) = priority_connection {
                forced_playable.insert(connection);
            }
        }
        if let Some((target, connection)) = priority_connection {
            let connector = Card::new(identity.suit, Rank::ALL[identity.rank.index() + 1]);
            push_signal(
                signals,
                entry,
                *player,
                Some(target),
                HGroupMoveKind::Priority,
                vec![connection],
                Some(connector),
            );
        } else {
            push_signal(
                signals,
                entry,
                *player,
                None,
                HGroupMoveKind::Priority,
                vec![*card],
                context.historical.identity(*card),
            );
        }
    }
}
