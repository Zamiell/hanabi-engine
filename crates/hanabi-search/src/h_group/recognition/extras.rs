use super::{
    Card, CardSet, Clue, ConnectionTransitionReason, HGroupClueKind, HGroupMoveKind,
    HGroupRuleEffects, HGroupTurnContext, IdentitySet, ObservedEvent, PlayerId, PlayerView, Rank,
    chop, finesse_position_id, five_pulled_card, focus, is_critical, is_playable_at, is_trash_at,
    next_player, protected_cards, push_signal, same_turn_signal, was_clued_before,
};

#[allow(clippy::too_many_lines)]
pub(in crate::h_group) fn apply_extra_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) {
    let entry = context.entry;
    let hands = context.after.hands;
    match &entry.event {
        ObservedEvent::Clued {
            giver,
            target,
            clue,
            touched,
            ..
        } => {
            let pending_cards = effects
                .pending
                .iter()
                .filter(|connection| connection.actor == *target)
                .flat_map(|connection| connection.cards.iter().copied())
                .collect::<CardSet>();
            let continuation = touched.iter().any(|card| pending_cards.contains(card))
                && touched
                    .iter()
                    .any(|card| !was_clued_before(view, entry.turn, *card));
            if continuation {
                // https://hanabi.github.io/extras/play-clues/#the-continuation-clue-touching-both-inside-and-outside-a-layer
                push_signal(
                    effects.signals,
                    entry,
                    *giver,
                    Some(*target),
                    HGroupMoveKind::ContinuationClue,
                    touched.clone(),
                    None,
                );
            }
            let just_in_time_focuses = effects
                .pending
                .iter()
                .filter(|connection| {
                    connection.actor == *target
                        && touched.contains(&connection.focus)
                        && touched
                            .iter()
                            .all(|card| was_clued_before(view, entry.turn, *card))
                        && connection
                            .cards
                            .iter()
                            .all(|candidate| !touched.contains(candidate))
                        && effects.signals.iter().any(|signal| {
                            signal.turn < entry.turn
                                && matches!(
                                    signal.kind,
                                    HGroupMoveKind::Finesse
                                        | HGroupMoveKind::ReverseFinesse
                                        | HGroupMoveKind::SelfFinesse
                                        | HGroupMoveKind::LayeredFinesse
                                        | HGroupMoveKind::ClandestineFinesse
                                        | HGroupMoveKind::QueuedFinesse
                                )
                                && (signal.cards.contains(&connection.focus)
                                    || connection
                                        .cards
                                        .iter()
                                        .any(|card| signal.cards.contains(card)))
                        })
                })
                .map(|connection| connection.focus)
                .collect::<Vec<_>>();
            for focus in just_in_time_focuses {
                // https://hanabi.github.io/extras/fix-clues/#the-just-in-time-fix-clue-jit
                push_signal(
                    effects.signals,
                    entry,
                    *giver,
                    Some(*target),
                    HGroupMoveKind::JustInTimeFix,
                    vec![focus],
                    context.historical.identity(focus),
                );
            }
            if !continuation {
                let invalidated = effects
                    .pending
                    .iter()
                    .filter(|connection| connection.actor == *target)
                    .filter(|connection| {
                        connection.cards.iter().any(|pending_card| {
                            view.hands[target.index()]
                                .iter()
                                .find(|card| card.id == *pending_card)
                                .is_some_and(|card| !card.clues.allows(connection.expected))
                        })
                    })
                    .map(|connection| connection.focus)
                    .collect::<CardSet>();
                let released = effects
                    .pending
                    .iter()
                    .filter(|connection| invalidated.contains(&connection.focus))
                    .flat_map(|connection| connection.cards.iter().copied())
                    .collect::<CardSet>();
                effects.pending.cancel_where(
                    entry.turn,
                    ConnectionTransitionReason::FocusInvalidated,
                    |connection| invalidated.contains(&connection.focus),
                );
                // A disproved layered connection must release every candidate
                // that was protected solely by that connection. Keeping its
                // trailing candidates invisibly clued after cancellation
                // changes chop and can make a later 5CM look like a 5 Save.
                for card in released {
                    if !effects.explicitly_clued.contains(&card)
                        && !effects
                            .pending
                            .iter()
                            .any(|connection| connection.cards.contains(&card))
                    {
                        effects.invisibly_clued.remove(&card);
                    }
                }
            }

            // Source: https://hanabi.github.io/extras/chop-moves/#the-transfer-chop-move
            let clue_focus = effects
                .clues
                .iter()
                .rev()
                .find(|clue| clue.turn == entry.turn)
                .map(|clue| clue.focus);
            let transferred = touched.iter().copied().find(|card| {
                clue_focus == Some(*card)
                    && effects.chop_moved.contains(card)
                    && context.historical.identity(*card).is_some_and(|identity| {
                        is_trash_at(context.before.stack_heights, identity)
                            || hands.iter().flatten().copied().any(|other| {
                                other != *card
                                    && effects.explicitly_clued.contains(&other)
                                    && context.historical.identity(other) == Some(identity)
                            })
                    })
            });
            if let Some(card) = transferred {
                effects.chop_moved.remove(&card);
                if !effects.discard_now.contains(&card) {
                    effects.discard_now.push(card);
                }
                let gotten = protected_cards(
                    effects.explicitly_clued,
                    effects.invisibly_clued,
                    effects.chop_moved,
                );
                if let Some(new_chop) = chop(&hands[target.index()], &gotten) {
                    effects.chop_moved.insert(new_chop);
                    push_signal(
                        effects.signals,
                        entry,
                        *giver,
                        Some(*target),
                        HGroupMoveKind::TransferChopMove,
                        vec![card, new_chop],
                        context.historical.identity(card),
                    );
                }
            }

            // Source: https://hanabi.github.io/extras/chop-moves/#the-negative-self-chop-move
            for (player_index, hand) in hands.iter().enumerate() {
                for card in hand {
                    if effects.explicitly_clued.contains(card) || effects.chop_moved.contains(card)
                    {
                        continue;
                    }
                    let identities =
                        IdentitySet::from_mask(context.after.facts[card.index()].identity_mask());
                    if !identities.is_empty()
                        && identities
                            .iter()
                            .all(|identity| identity.rank == Rank::Five)
                    {
                        effects.chop_moved.insert(*card);
                        push_signal(
                            effects.signals,
                            entry,
                            *giver,
                            Some(PlayerId::new(
                                u8::try_from(player_index)
                                    .expect("standard Hanabi has at most five players"),
                            )),
                            HGroupMoveKind::NegativeSelfChopMove,
                            vec![*card],
                            None,
                        );
                    }
                }
            }

            // Source: https://hanabi.github.io/extras/discards-misplays/#the-promise-clue--the-promise-discard
            if touched.len() >= 2 {
                let promised = effects.pending.iter().find_map(|connection| {
                    let duplicate_touched = touched
                        .iter()
                        .copied()
                        .any(|card| context.historical.identity(card) == Some(connection.expected));
                    (connection.actor != *target && duplicate_touched)
                        .then(|| connection.cards.first().copied())
                        .flatten()
                });
                if let Some(promised) = promised {
                    if !effects.discard_now.contains(&promised) {
                        effects.discard_now.push(promised);
                    }
                    push_signal(
                        effects.signals,
                        entry,
                        *giver,
                        Some(*target),
                        HGroupMoveKind::PromiseClue,
                        vec![promised],
                        context.historical.identity(promised),
                    );
                }
            }

            // Sources:
            // - https://hanabi.github.io/extras/charms/#the-unknown-trash-charm-utc
            // - https://hanabi.github.io/extras/charms/#the-junk-charm-for-1s
            let current_interpretation = effects
                .clues
                .iter()
                .rev()
                .find(|candidate| candidate.turn == entry.turn);
            let fake_save = context.before.clue_tokens == 1
                && matches!(clue, Clue::Suit(_))
                && current_interpretation.is_some_and(|meaning| {
                    meaning.focus_was_chop
                        && !meaning.save_identities.is_empty()
                        && touched
                            .iter()
                            .filter(|card| {
                                context.historical.identity(**card).is_some_and(|identity| {
                                    identity.rank == Rank::Five || {
                                        let removed = view
                                            .history
                                            .iter()
                                            .filter(|prior| {
                                                prior.turn < entry.turn
                                                    && matches!(
                                                        prior.event,
                                                        ObservedEvent::Discarded {
                                                            identity: removed,
                                                            ..
                                                        } | ObservedEvent::Played {
                                                            identity: removed,
                                                            successful: false,
                                                            ..
                                                        } if removed == identity
                                                    )
                                            })
                                            .count();
                                        removed + 1 == usize::from(identity.rank.copies())
                                    }
                                })
                            })
                            .count()
                            >= 2
                });
            if fake_save {
                // https://hanabi.github.io/extras/save-clues/#the-fake-save
                push_signal(
                    effects.signals,
                    entry,
                    *giver,
                    Some(*target),
                    HGroupMoveKind::FakeSave,
                    touched.clone(),
                    None,
                );
            }
            let gotten_for_self_bluff = protected_cards(
                effects.explicitly_clued,
                effects.invisibly_clued,
                effects.chop_moved,
            );
            let self_color_bluff = if matches!(clue, Clue::Suit(_))
                && *target == next_player(*giver, hands.len())
                && !context.after.early_game
                && context.before.clue_tokens < 8
            {
                current_interpretation
                    .filter(|meaning| meaning.kind == HGroupClueKind::Play)
                    .and_then(|meaning| {
                        context
                            .historical
                            .identity(meaning.focus)
                            .map(|identity| (meaning, identity))
                    })
                    .and_then(|(meaning, identity)| {
                        let distance = usize::from(identity.rank.number()).saturating_sub(
                            usize::from(context.before.stack_heights[identity.suit.index()]) + 1,
                        );
                        let blind =
                            finesse_position_id(&hands[target.index()], &gotten_for_self_bluff, 0)?;
                        let blind_identity = context.historical.identity(blind)?;
                        (distance >= 1
                            && is_playable_at(context.before.stack_heights, blind_identity)
                            && !clue.matches(blind_identity)
                            && !effects.pending.iter().any(|connection| {
                                connection.focus == meaning.focus && connection.actor != *target
                            }))
                        .then_some((blind, distance))
                    })
            } else {
                None
            };
            if let Some((blind, distance)) = self_color_bluff {
                effects.pending.cancel_where(
                    entry.turn,
                    ConnectionTransitionReason::Superseded,
                    |connection| connection.focus == current_interpretation.expect("checked").focus,
                );
                effects.forced_playable.insert(blind);
                let mut cards = vec![blind];
                let kind = if distance >= 2 {
                    let second = next_player(*target, hands.len());
                    if let Some(second_blind) =
                        finesse_position_id(&hands[second.index()], &gotten_for_self_bluff, 0)
                            .filter(|card| {
                                context.historical.identity(*card).is_some_and(|identity| {
                                    is_playable_at(context.before.stack_heights, identity)
                                })
                            })
                    {
                        effects.forced_playable.insert(second_blind);
                        cards.push(second_blind);
                    }
                    HGroupMoveKind::SelfColorDoubleBluff
                } else {
                    HGroupMoveKind::SelfColorBluff
                };
                push_signal(
                    effects.signals,
                    entry,
                    *giver,
                    Some(*target),
                    kind,
                    cards,
                    context.historical.identity(blind),
                );
            }
            let non_focus = current_interpretation
                .map(|meaning| {
                    touched
                        .iter()
                        .copied()
                        .filter(|card| *card != meaning.focus)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let non_focus_all_trash = !non_focus.is_empty()
                && non_focus.iter().all(|card| {
                    context
                        .historical
                        .identity(*card)
                        .is_some_and(|identity| is_trash_at(context.before.stack_heights, identity))
                });
            let unknown_trash_charm = non_focus_all_trash
                && effects.signals.iter().any(|signal| {
                    signal.turn == entry.turn
                        && signal.kind == HGroupMoveKind::UnknownTrashDischarge
                });
            let junk_charm = non_focus_all_trash
                && *clue == Clue::Rank(Rank::One)
                && current_interpretation.is_some_and(|meaning| {
                    context
                        .historical
                        .identity(meaning.focus)
                        .is_some_and(|identity| {
                            is_playable_at(context.before.stack_heights, identity)
                                && touched
                                    .iter()
                                    .filter(|card| {
                                        context
                                            .historical
                                            .identity(**card)
                                            .is_some_and(|other| other.suit == identity.suit)
                                    })
                                    .count()
                                    == 1
                        })
                });
            if unknown_trash_charm || junk_charm {
                for prior in effects.signals.iter().filter(|signal| {
                    signal.turn == entry.turn
                        && matches!(
                            signal.kind,
                            HGroupMoveKind::UnknownTrashDischarge
                                | HGroupMoveKind::UnknownDupeDischarge
                        )
                }) {
                    if let Some(previous_forced) = prior.cards.first() {
                        effects.forced_playable.remove(previous_forced);
                    }
                }
                let gotten = protected_cards(
                    effects.explicitly_clued,
                    effects.invisibly_clued,
                    effects.chop_moved,
                );
                let actor = next_player(*giver, hands.len());
                if let Some(charmed) = finesse_position_id(&hands[actor.index()], &gotten, 3)
                    .filter(|card| {
                        context.historical.identity(*card).is_none_or(|identity| {
                            is_playable_at(context.before.stack_heights, identity)
                        })
                    })
                {
                    effects.forced_playable.insert(charmed);
                    push_signal(
                        effects.signals,
                        entry,
                        *giver,
                        Some(actor),
                        if unknown_trash_charm {
                            HGroupMoveKind::UnknownTrashCharm
                        } else {
                            HGroupMoveKind::JunkCharm
                        },
                        vec![charmed],
                        context.historical.identity(charmed),
                    );
                }
            }

            // https://hanabi.github.io/extras/pushes-pulls/#the-trash-pull
            // It is the last remaining interpretation of a known-trash clue:
            // ordinary TCM and Trash Double Ignition both take precedence.
            let all_touched_trash = !touched.is_empty()
                && touched.iter().all(|card| {
                    context
                        .historical
                        .identity(*card)
                        .is_some_and(|identity| is_trash_at(context.before.stack_heights, identity))
                });
            if all_touched_trash
                && !same_turn_signal(effects.signals, entry.turn, HGroupMoveKind::TrashChopMove)
                && !same_turn_signal(
                    effects.signals,
                    entry.turn,
                    HGroupMoveKind::TrashDoubleIgnition,
                )
                && !same_turn_signal(effects.signals, entry.turn, HGroupMoveKind::Stall)
            {
                let gotten = protected_cards(
                    effects.explicitly_clued,
                    effects.invisibly_clued,
                    effects.chop_moved,
                );
                if let Some(pulled) = five_pulled_card(&hands[target.index()], touched, &gotten)
                    .filter(|card| {
                        context.historical.identity(*card).is_some_and(|identity| {
                            is_playable_at(context.before.stack_heights, identity)
                        })
                    })
                {
                    effects.forced_playable.insert(pulled);
                    push_signal(
                        effects.signals,
                        entry,
                        *giver,
                        Some(*target),
                        HGroupMoveKind::TrashPull,
                        vec![pulled],
                        context.historical.identity(pulled),
                    );
                }
            }
            let has_ignition_or_ejection = effects.signals.iter().any(|signal| {
                signal.turn == entry.turn
                    && matches!(
                        signal.kind,
                        HGroupMoveKind::ReplayDoubleIgnition
                            | HGroupMoveKind::TrashDoubleIgnition
                            | HGroupMoveKind::PokeDoubleIgnition
                            | HGroupMoveKind::ChopMoveIgnition
                            | HGroupMoveKind::TempoClue
                            | HGroupMoveKind::TempoClueChopMove
                            | HGroupMoveKind::FiveColorEjection
                            | HGroupMoveKind::UnknownTrashDischarge
                            | HGroupMoveKind::UnknownDupeDischarge
                            | HGroupMoveKind::OutOfPositionEjection
                            | HGroupMoveKind::StackedEjection
                            | HGroupMoveKind::TrashPull
                    )
            });
            if !has_ignition_or_ejection {
                let interpretation = effects
                    .clues
                    .iter()
                    .rev()
                    .find(|candidate| candidate.turn == entry.turn);
                let all_previous = touched
                    .iter()
                    .all(|card| was_clued_before(view, entry.turn, *card));
                let all_playable = touched.iter().all(|card| {
                    context
                        .historical
                        .identity(*card)
                        .is_some_and(|known| is_playable_at(context.before.stack_heights, known))
                });
                let all_trash = touched.iter().all(|card| {
                    context
                        .historical
                        .identity(*card)
                        .is_some_and(|known| is_trash_at(context.before.stack_heights, known))
                });
                let rank_choice = matches!(clue, Clue::Rank(Rank::Two | Rank::Five))
                    && interpretation.is_some_and(|meaning| {
                        if !meaning.focus_was_chop {
                            return false;
                        }
                        let Some(known) = context.historical.identity(meaning.focus) else {
                            return false;
                        };
                        if !is_playable_at(context.before.stack_heights, known) {
                            return false;
                        }
                        let gotten = protected_cards(
                            effects.explicitly_clued,
                            effects.invisibly_clued,
                            effects.chop_moved,
                        );
                        let color_touched = hands[target.index()]
                            .iter()
                            .copied()
                            .filter(|card| {
                                context
                                    .historical
                                    .identity(*card)
                                    .is_some_and(|identity| identity.suit == known.suit)
                            })
                            .collect::<Vec<_>>();
                        color_touched.len() == 1
                            && focus(
                                &hands[target.index()],
                                &color_touched,
                                chop(&hands[target.index()], &gotten),
                                &gotten,
                            ) == Some(meaning.focus)
                    });
                let bad_chop_move = effects.signals.iter().find(|signal| {
                    signal.turn == entry.turn
                        && signal.kind == HGroupMoveKind::ChopMove
                        && signal.cards.iter().all(|card| {
                            context.historical.identity(*card).is_some_and(|known| {
                                is_trash_at(context.before.stack_heights, known)
                            })
                        })
                });
                let trash_push = effects.signals.iter().find(|signal| {
                    signal.turn == entry.turn && signal.kind == HGroupMoveKind::TrashPush
                });
                let pushed_identity = trash_push.and_then(|signal| {
                    let focus = *signal.cards.first()?;
                    let hand = &hands[target.index()];
                    let position = hand.iter().position(|candidate| *candidate == focus)?;
                    hand.get(position + 1)
                        .and_then(|pushed| context.historical.identity(*pushed))
                });
                let trash_finesse = effects.signals.iter().any(|signal| {
                    signal.turn == entry.turn
                        && matches!(
                            signal.kind,
                            HGroupMoveKind::Finesse
                                | HGroupMoveKind::ReverseFinesse
                                | HGroupMoveKind::LayeredFinesse
                                | HGroupMoveKind::Bluff
                        )
                }) && !same_turn_signal(
                    effects.signals,
                    entry.turn,
                    HGroupMoveKind::TrashChopMove,
                );
                let special = if rank_choice {
                    Some((HGroupMoveKind::RankChoiceEjection, 1))
                } else if bad_chop_move.is_some() {
                    Some((HGroupMoveKind::BadChopMoveEjection, 1))
                } else if trash_finesse
                    && pushed_identity
                        .is_some_and(|known| is_trash_at(context.before.stack_heights, known))
                {
                    // https://hanabi.github.io/extras/ejections/#the-bad-trash-finesse-ejection--the-bad-trash-bluff-ejection
                    Some((HGroupMoveKind::BadTrashFinesseEjection, 1))
                } else if trash_finesse
                    && pushed_identity.is_some_and(|known| {
                        usize::from(known.rank.number())
                            > usize::from(context.before.stack_heights[known.suit.index()]) + 2
                    })
                {
                    // https://hanabi.github.io/extras/ejections/#the-trash-finesse-push-ejection--the-trash-bluff-push-ejection
                    Some((HGroupMoveKind::TrashFinessePushEjection, 1))
                } else if pushed_identity
                    .is_some_and(|known| is_trash_at(context.before.stack_heights, known))
                {
                    Some((HGroupMoveKind::TrashPushDischarge, 2))
                } else if pushed_identity.is_some_and(|known| {
                    usize::from(known.rank.number())
                        > usize::from(context.before.stack_heights[known.suit.index()]) + 2
                }) {
                    Some((HGroupMoveKind::TrashPushEjection, 1))
                } else if all_previous
                    && all_playable
                    && !same_turn_signal(effects.signals, entry.turn, HGroupMoveKind::Stall)
                {
                    Some((HGroupMoveKind::ReplayEjection, 1))
                } else if all_previous
                    && all_trash
                    && !same_turn_signal(effects.signals, entry.turn, HGroupMoveKind::Stall)
                {
                    Some((HGroupMoveKind::PokeEjection, 1))
                } else if !all_previous
                    && all_trash
                    && !same_turn_signal(effects.signals, entry.turn, HGroupMoveKind::ChopMove)
                    && !same_turn_signal(effects.signals, entry.turn, HGroupMoveKind::TrashPush)
                {
                    Some((HGroupMoveKind::TrashEjection, 1))
                } else {
                    None
                };
                if let Some((kind, position)) = special {
                    let gotten = protected_cards(
                        effects.explicitly_clued,
                        effects.invisibly_clued,
                        effects.chop_moved,
                    );
                    let actor = next_player(*giver, hands.len());
                    if let Some(ejected) =
                        finesse_position_id(&hands[actor.index()], &gotten, position)
                    {
                        if context
                            .historical
                            .identity(ejected)
                            .is_none_or(|known| is_playable_at(context.before.stack_heights, known))
                        {
                            effects.forced_playable.insert(ejected);
                            let mut affected = vec![ejected];
                            affected.extend(touched.iter().copied());
                            push_signal(
                                effects.signals,
                                entry,
                                *giver,
                                Some(actor),
                                kind,
                                affected,
                                context.historical.identity(ejected),
                            );
                        }
                    }
                }
            }
        }
        ObservedEvent::Played {
            player,
            card,
            identity,
            successful,
        } => {
            if *successful
                && same_turn_signal(effects.signals, entry.turn, HGroupMoveKind::Priority)
            {
                let gotten = protected_cards(
                    effects.explicitly_clued,
                    effects.invisibly_clued,
                    effects.chop_moved,
                );
                let next = next_player(*player, hands.len());
                let next_blind =
                    finesse_position_id(&hands[next.index()], &gotten, 0).filter(|candidate| {
                        context
                            .historical
                            .identity(*candidate)
                            .is_some_and(|known| is_playable_at(context.after.stack_heights, known))
                    });
                let held_connector = (identity.rank != Rank::Five)
                    .then(|| Card::new(identity.suit, Rank::ALL[identity.rank.index() + 1]))
                    .is_some_and(|connector| {
                        context.before.hands[player.index()]
                            .iter()
                            .any(|candidate| {
                                *candidate != *card
                                    && effects.explicitly_clued.contains(candidate)
                                    && context.historical.identity(*candidate) == Some(connector)
                            })
                    });
                if held_connector {
                    if let Some(blind) = next_blind {
                        // https://hanabi.github.io/extras/special-bluffs/#the-known-priority-bluff
                        effects.forced_playable.insert(blind);
                        push_signal(
                            effects.signals,
                            entry,
                            *player,
                            Some(next),
                            HGroupMoveKind::KnownPriorityBluff,
                            vec![blind],
                            context.historical.identity(blind),
                        );
                    }
                } else {
                    let declined_free_finesse = context.before.hands[player.index()]
                        .iter()
                        .copied()
                        .filter(|candidate| *candidate != *card)
                        .filter_map(|candidate| {
                            let candidate_identity = context.historical.identity(candidate)?;
                            is_playable_at(context.before.stack_heights, candidate_identity)
                                .then_some(candidate_identity)
                        })
                        .any(|candidate_identity| {
                            if candidate_identity.rank == Rank::Five {
                                return false;
                            }
                            let connector = Card::new(
                                candidate_identity.suit,
                                Rank::ALL[candidate_identity.rank.index() + 1],
                            );
                            (1..hands.len()).any(|distance| {
                                let target = PlayerId::new(
                                    u8::try_from((player.index() + distance) % hands.len())
                                        .unwrap_or(0),
                                );
                                finesse_position_id(&hands[target.index()], &gotten, 0).is_some_and(
                                    |candidate| {
                                        context.historical.identity(candidate) == Some(connector)
                                    },
                                )
                            })
                        });
                    if declined_free_finesse {
                        if let Some(blind) = next_blind {
                            // https://hanabi.github.io/extras/special-finesses/#inverted-priority-finesse
                            effects.forced_playable.insert(blind);
                            push_signal(
                                effects.signals,
                                entry,
                                *player,
                                Some(next),
                                HGroupMoveKind::InvertedPriorityFinesse,
                                vec![blind],
                                context.historical.identity(blind),
                            );
                        }
                    }
                }
            }
            if *successful && !was_clued_before(view, entry.turn, *card) {
                effects.forced_playable.remove(card);
                // Source: https://hanabi.github.io/extras/ejection-extensions/#the-double-ejection
                let ejection = effects.signals.iter().rev().any(|signal| {
                    signal.turn < entry.turn
                        && signal.cards.first() == Some(card)
                        && matches!(
                            signal.kind,
                            HGroupMoveKind::Ejection
                                | HGroupMoveKind::FiveColorEjection
                                | HGroupMoveKind::OutOfPositionEjection
                                | HGroupMoveKind::StackedEjection
                        )
                });
                if ejection {
                    let target = next_player(*player, hands.len());
                    let gotten = protected_cards(
                        effects.explicitly_clued,
                        effects.invisibly_clued,
                        effects.chop_moved,
                    );
                    let direct_was_available =
                        [Clue::Suit(identity.suit), Clue::Rank(identity.rank)]
                            .into_iter()
                            .any(|direct| {
                                let touched = context.before.hands[player.index()]
                                    .iter()
                                    .copied()
                                    .filter(|candidate| {
                                        context
                                            .historical
                                            .identity(*candidate)
                                            .is_some_and(|known| direct.matches(known))
                                    })
                                    .collect::<Vec<_>>();
                                focus(
                                    &context.before.hands[player.index()],
                                    &touched,
                                    chop(&context.before.hands[player.index()], &gotten),
                                    &gotten,
                                ) == Some(*card)
                            });
                    if direct_was_available {
                        if let Some(second) =
                            finesse_position_id(&hands[target.index()], &gotten, 1)
                        {
                            effects.forced_playable.insert(second);
                            push_signal(
                                effects.signals,
                                entry,
                                *player,
                                Some(target),
                                HGroupMoveKind::DoubleEjection,
                                vec![second],
                                context.historical.identity(second),
                            );
                        }
                    }
                }
            } else if !successful {
                // Source: https://hanabi.github.io/extras/chop-moves/#the-misplay-chop-move
                let origin = effects
                    .clues
                    .iter()
                    .rev()
                    .find(|clue| clue.target == *player && clue.focus == *card);
                if let Some(origin) = origin {
                    let hand = &context.before.hands[player.index()];
                    if let Some(position) = hand.iter().position(|candidate| *candidate == *card) {
                        let kept = origin
                            .new_non_focus
                            .iter()
                            .copied()
                            .chain(core::iter::once(origin.focus))
                            .collect::<CardSet>();
                        let moved = hand[..position]
                            .iter()
                            .rev()
                            .copied()
                            .filter(|candidate| !kept.contains(candidate))
                            .collect::<Vec<_>>();
                        effects.chop_moved.extend(moved.iter().copied());
                        if !moved.is_empty() {
                            push_signal(
                                effects.signals,
                                entry,
                                *player,
                                Some(*player),
                                HGroupMoveKind::MisplayChopMove,
                                moved,
                                None,
                            );
                        }
                    }
                }
            }
        }
        ObservedEvent::Discarded {
            player,
            card,
            identity,
        } => {
            let prior_same_one = identity.rank == Rank::One
                && view.history.iter().any(|prior| {
                    prior.turn < entry.turn
                        && matches!(
                            prior.event,
                            ObservedEvent::Discarded {
                                player: prior_player,
                                identity: prior_identity,
                                ..
                            } if prior_player == *player && prior_identity == *identity
                        )
                });
            if prior_same_one {
                // https://hanabi.github.io/extras/miscellaneous/#the-elimination-rewrite-for-1s
                push_signal(
                    effects.signals,
                    entry,
                    *player,
                    Some(*player),
                    HGroupMoveKind::EliminationRewrite,
                    context.after.hands[player.index()].clone(),
                    Some(*identity),
                );
            }
            if effects.signals.iter().any(|signal| {
                signal.turn < entry.turn
                    && signal.kind == HGroupMoveKind::PromiseClue
                    && signal.cards.contains(card)
            }) {
                push_signal(
                    effects.signals,
                    entry,
                    *player,
                    Some(*player),
                    HGroupMoveKind::PromiseDiscard,
                    vec![*card],
                    context.historical.identity(*card),
                );
            }
        }
        ObservedEvent::Drew { .. } => {}
    }

    // https://hanabi.github.io/extras/miscellaneous/#the-negative-blind-play
    // A card whose remaining public clue domain contains only currently
    // playable, non-duplicated identities is as safe to play as an explicit
    // Play Clue. Keep this rule in the observer reducer so it cannot inspect
    // the observer's hidden identity.
    let observer = view.observer;
    for card in &context.after.hands[observer.index()] {
        if effects.explicitly_clued.contains(card)
            || effects.invisibly_clued.contains(card)
            || effects.forced_playable.contains(card)
        {
            continue;
        }
        let possibilities =
            IdentitySet::from_mask(context.after.facts[card.index()].identity_mask());
        let negatively_playable = !possibilities.is_empty()
            && possibilities.iter().all(|identity| {
                is_playable_at(context.after.stack_heights, identity)
                    && !context.after.hands.iter().flatten().copied().any(|other| {
                        other != *card
                            && (effects.explicitly_clued.contains(&other)
                                || effects.invisibly_clued.contains(&other))
                            && context.historical.identity(other) == Some(identity)
                    })
            });
        if negatively_playable {
            effects.forced_playable.insert(*card);
            if !effects.signals.iter().any(|signal| {
                signal.kind == HGroupMoveKind::NegativeBlindPlay && signal.cards.contains(card)
            }) {
                push_signal(
                    effects.signals,
                    entry,
                    observer,
                    Some(observer),
                    HGroupMoveKind::NegativeBlindPlay,
                    vec![*card],
                    None,
                );
            }
        }
    }
    // Extras refine or compose the numbered primitives. Preserve an explicit
    // marker when a single turn already matched two or more primitive effects;
    // consumers can inspect the preceding same-turn signals to recover the
    // exact composition without a parallel hierarchy of bespoke state types.
    let same_turn = effects
        .signals
        .iter()
        .filter(|signal| {
            signal.turn == entry.turn
                && !matches!(
                    signal.kind,
                    HGroupMoveKind::Extra | HGroupMoveKind::Retraction
                )
        })
        .count();
    if same_turn >= 2 {
        let actor = match &entry.event {
            ObservedEvent::Clued { giver, .. } => *giver,
            ObservedEvent::Played { player, .. }
            | ObservedEvent::Discarded { player, .. }
            | ObservedEvent::Drew { player, .. } => *player,
        };
        let cards = match &entry.event {
            ObservedEvent::Clued { touched, .. } => touched.clone(),
            ObservedEvent::Played { card, .. }
            | ObservedEvent::Discarded { card, .. }
            | ObservedEvent::Drew { card, .. } => vec![*card],
        };
        push_signal(
            effects.signals,
            entry,
            actor,
            None,
            HGroupMoveKind::Extra,
            cards,
            None,
        );
    }
}

#[allow(clippy::too_many_lines)]
pub(in crate::h_group) fn apply_max_special_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) {
    let entry = context.entry;
    let hands = context.after.hands;
    let gotten = protected_cards(
        effects.explicitly_clued,
        effects.invisibly_clued,
        effects.chop_moved,
    );
    match &entry.event {
        ObservedEvent::Clued {
            giver,
            target,
            clue,
            touched,
            ..
        } => {
            if same_turn_signal(
                effects.signals,
                entry.turn,
                HGroupMoveKind::EliminationFinesse,
            ) && same_turn_signal(effects.signals, entry.turn, HGroupMoveKind::Bluff)
            {
                // https://hanabi.github.io/extras/special-bluffs/#the-elimination-bluff--the-elimination-layered-finesse
                push_signal(
                    effects.signals,
                    entry,
                    *giver,
                    Some(*target),
                    HGroupMoveKind::EliminationBluff,
                    touched.clone(),
                    None,
                );
            }

            if hands.len() == 5
                && same_turn_signal(
                    effects.signals,
                    entry.turn,
                    HGroupMoveKind::PestilentDoubleBluff,
                )
            {
                let third = next_player(
                    next_player(next_player(*giver, hands.len()), hands.len()),
                    hands.len(),
                );
                if let Some(card) =
                    finesse_position_id(&hands[third.index()], &gotten, 0).filter(|card| {
                        context.historical.identity(*card).is_none_or(|identity| {
                            is_playable_at(context.before.stack_heights, identity)
                        })
                    })
                {
                    // https://hanabi.github.io/extras/special-bluffs/#the-pestilent-triple-bluff
                    effects.forced_playable.insert(card);
                    push_signal(
                        effects.signals,
                        entry,
                        *giver,
                        Some(third),
                        HGroupMoveKind::PestilentTripleBluff,
                        vec![card],
                        None,
                    );
                }
            }

            if matches!(clue, Clue::Rank(_)) {
                let current_connections = effects
                    .pending
                    .iter()
                    .filter(|connection| touched.contains(&connection.focus))
                    .cloned()
                    .collect::<Vec<_>>();
                for connection in current_connections {
                    let Some(first) = connection.cards.first() else {
                        continue;
                    };
                    let Some(position) = hands[connection.actor.index()]
                        .iter()
                        .position(|card| card == first)
                    else {
                        continue;
                    };
                    let extras = hands[connection.actor.index()][position + 1..]
                        .iter()
                        .copied()
                        .filter(|card| !gotten.contains(card))
                        .filter(|card| {
                            context.historical.identity(*card).is_some_and(|identity| {
                                identity.rank.number()
                                    < match clue {
                                        Clue::Rank(rank) => rank.number(),
                                        Clue::Suit(_) => unreachable!("rank clue checked above"),
                                    }
                                    && is_playable_at(context.before.stack_heights, identity)
                            })
                        })
                        .collect::<Vec<_>>();
                    if !extras.is_empty() {
                        // https://hanabi.github.io/extras/special-finesses/#the-surreptitious-finesse
                        effects.forced_playable.extend(extras.iter().copied());
                        push_signal(
                            effects.signals,
                            entry,
                            *giver,
                            Some(connection.actor),
                            HGroupMoveKind::SurreptitiousFinesse,
                            extras,
                            None,
                        );
                    }
                }
            }

            let current_meaning = effects
                .clues
                .iter()
                .rev()
                .find(|meaning| meaning.turn == entry.turn);
            let declined_five = touched.len() == 1
                && current_meaning.is_some_and(|meaning| {
                    meaning.focus_was_chop
                        && matches!(meaning.kind, HGroupClueKind::Save(_))
                        && context
                            .historical
                            .identity(meaning.focus)
                            .is_some_and(|identity| {
                                identity.rank != Rank::Five && is_critical(view, identity)
                            })
                        && hands[target.index()].iter().copied().any(|card| {
                            card != meaning.focus
                                && context
                                    .historical
                                    .identity(card)
                                    .is_some_and(|identity| identity.rank == Rank::Five)
                        })
                });
            if declined_five {
                let actor = next_player(*giver, hands.len());
                if let Some(card) =
                    finesse_position_id(&hands[actor.index()], &gotten, 0).filter(|card| {
                        context.historical.identity(*card).is_none_or(|identity| {
                            is_playable_at(context.before.stack_heights, identity)
                        })
                    })
                {
                    // https://hanabi.github.io/extras/special-finesses/#the-declined-5s-finesse
                    effects.forced_playable.insert(card);
                    push_signal(
                        effects.signals,
                        entry,
                        *giver,
                        Some(actor),
                        HGroupMoveKind::DeclinedFiveFinesse,
                        vec![card, touched[0]],
                        None,
                    );
                }
            }
            if matches!(clue, Clue::Rank(_))
                && current_meaning
                    .is_some_and(|meaning| matches!(meaning.kind, HGroupClueKind::Save(_)))
                && current_meaning.is_some_and(|meaning| {
                    let Some(identity) = context.historical.identity(meaning.focus) else {
                        return false;
                    };
                    let color_touched = hands[target.index()]
                        .iter()
                        .copied()
                        .filter(|card| {
                            context
                                .historical
                                .identity(*card)
                                .is_some_and(|candidate| candidate.suit == identity.suit)
                        })
                        .collect::<Vec<_>>();
                    color_touched == vec![meaning.focus]
                        && context.after.facts[meaning.focus.index()].identity_mask()
                            == 1 << identity.index()
                })
            {
                // https://hanabi.github.io/extras/special-finesses/#the-rank-choice-save-finesse--the-rank-choice-save-bluff
                let actor = next_player(*giver, hands.len());
                if let Some(card) =
                    finesse_position_id(&hands[actor.index()], &gotten, 0).filter(|card| {
                        context.historical.identity(*card).is_none_or(|identity| {
                            is_playable_at(context.before.stack_heights, identity)
                        })
                    })
                {
                    effects.forced_playable.insert(card);
                    push_signal(
                        effects.signals,
                        entry,
                        *giver,
                        Some(actor),
                        HGroupMoveKind::RankChoiceSaveFinesse,
                        vec![card],
                        None,
                    );
                }
            }

            let previous_tempo =
                effects
                    .signals
                    .latest(HGroupMoveKind::TempoClue)
                    .filter(|signal| {
                        signal.turn < entry.turn
                            && entry.turn.saturating_sub(signal.turn)
                                < u32::try_from(hands.len()).unwrap_or(u32::MAX)
                    });
            if same_turn_signal(effects.signals, entry.turn, HGroupMoveKind::PlayClue)
                && previous_tempo.is_some_and(|tempo| {
                    effects.pending.iter().any(|connection| {
                        Some(connection.actor) == tempo.target
                            && touched.contains(&connection.focus)
                    })
                })
            {
                // https://hanabi.github.io/extras/save-clues/#saving-playable-cards-when-the-preceding-cards-are-not-promptable
                push_signal(
                    effects.signals,
                    entry,
                    *giver,
                    Some(*target),
                    HGroupMoveKind::UnpromptablePredecessorSave,
                    touched.clone(),
                    None,
                );
            }

            if same_turn_signal(effects.signals, entry.turn, HGroupMoveKind::FixClue)
                && effects.signals.iter().any(|signal| {
                    signal.turn < entry.turn
                        && matches!(
                            signal.kind,
                            HGroupMoveKind::OutOfOrderFinesse
                                | HGroupMoveKind::SuboptimalConnection
                        )
                        && signal.target == Some(*target)
                })
            {
                // https://hanabi.github.io/extras/special-finesses/#finesses-with-a-lie-component
                push_signal(
                    effects.signals,
                    entry,
                    *giver,
                    Some(*target),
                    HGroupMoveKind::LieComponentFinesse,
                    touched.clone(),
                    None,
                );
            }
        }
        ObservedEvent::Played {
            player,
            card,
            identity,
            successful: true,
        } => {
            if same_turn_signal(effects.signals, entry.turn, HGroupMoveKind::Priority)
                && context.before.hands[player.index()]
                    .iter()
                    .copied()
                    .any(|candidate| {
                        candidate != *card
                            && effects.explicitly_clued.contains(&candidate)
                            && context.historical.identity(candidate).is_some_and(|known| {
                                known.suit == identity.suit
                                    && known.rank.number() == identity.rank.number() + 1
                            })
                    })
            {
                // https://hanabi.github.io/extras/special-finesses/#potential-priority-duplication--the-certain-priority-finesse-or-priority-certain-finesse
                push_signal(
                    effects.signals,
                    entry,
                    *player,
                    Some(*player),
                    HGroupMoveKind::CertainPriorityFinesse,
                    vec![*card],
                    None,
                );
            }

            let patch = effects.pending.iter().find_map(|connection| {
                let first = connection.cards.first().copied()?;
                let first_identity = context.historical.identity(first)?;
                (first_identity.suit == identity.suit
                    && first_identity.rank.number() == identity.rank.number() + 1
                    && is_playable_at(context.after.stack_heights, first_identity))
                .then_some((connection.actor, first))
            });
            if let Some((actor, patched)) = patch {
                // https://hanabi.github.io/extras/special-finesses/#the-patch-finesse
                effects.forced_playable.insert(patched);
                push_signal(
                    effects.signals,
                    entry,
                    *player,
                    Some(actor),
                    HGroupMoveKind::PatchFinesse,
                    vec![*card, patched],
                    None,
                );
            }
        }
        ObservedEvent::Discarded { player, card, .. } => {
            let passed = effects
                .pending
                .iter()
                .find(|connection| connection.actor == *player && !connection.cards.contains(card));
            if let Some(passed) = passed {
                let next = next_player(*player, hands.len());
                if let Some(blind) =
                    finesse_position_id(&hands[next.index()], &gotten, 0).filter(|candidate| {
                        context
                            .historical
                            .identity(*candidate)
                            .is_some_and(|identity| {
                                is_playable_at(context.before.stack_heights, identity)
                            })
                    })
                {
                    // https://hanabi.github.io/extras/special-bluffs/#the-pass-bluff
                    effects.forced_playable.insert(blind);
                    push_signal(
                        effects.signals,
                        entry,
                        *player,
                        Some(next),
                        HGroupMoveKind::PassBluff,
                        vec![blind],
                        None,
                    );
                    if effects.signals.iter().any(|signal| {
                        signal.turn < entry.turn
                            && signal.kind == HGroupMoveKind::AmbiguousFinesse
                            && signal
                                .cards
                                .iter()
                                .any(|candidate| passed.cards.contains(candidate))
                    }) {
                        // https://hanabi.github.io/extras/special-finesses/#the-ambiguous-finesse-pass-back-afpb
                        push_signal(
                            effects.signals,
                            entry,
                            *player,
                            Some(passed.actor),
                            HGroupMoveKind::AmbiguousFinessePassBack,
                            passed.cards.clone(),
                            None,
                        );
                    }
                }
            }
        }
        ObservedEvent::Drew { .. }
        | ObservedEvent::Played {
            successful: false, ..
        } => {}
    }

    if hands.len() == 3 {
        for connection in effects.pending.iter() {
            let purge = hands[connection.actor.index()]
                .iter()
                .copied()
                .filter(|card| !gotten.contains(card))
                .collect::<Vec<_>>();
            if purge.len() >= 2
                && purge.iter().all(|card| {
                    context.historical.identity(*card).is_some_and(|identity| {
                        is_playable_at(context.after.stack_heights, identity)
                    })
                })
            {
                // https://hanabi.github.io/extras/special-bluffs/#the-purge-bluff-layered-bluff
                effects.forced_playable.extend(purge.iter().copied());
                push_signal(
                    effects.signals,
                    entry,
                    connection.actor,
                    Some(connection.actor),
                    HGroupMoveKind::PurgeBluff,
                    purge,
                    None,
                );
                break;
            }
        }
    }
}
