use super::{
    CardId, CardSet, ConnectionObligation, ConventionJournal, HGroupClueKind, HGroupConnectionKind,
    HGroupMoveKind, HGroupRuleEffects, HGroupTurnContext, IdentitySet, MAX_CLUE_TOKENS,
    ObservedEvent, ObservedHistoryEntry, PlayerId, PlayerView, PromiseId, Rank, chop, focus,
    identity_of, is_playable_at, is_playable_now, is_trash_at, next_player, pending_is_active,
    protected_cards, push_signal, same_turn_signal, was_clued_before, was_clued_before_with,
};

pub(in crate::h_group) fn apply_tempo_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    explicitly_clued: &CardSet,
    chop_moved: &mut CardSet,
    signals: &mut ConventionJournal,
) {
    // Sources: https://hanabi.github.io/level-6/#the-tempo-clue
    // https://hanabi.github.io/level-6/#the-tempo-clue-chop-move-tccm
    if [
        HGroupMoveKind::FiveStall,
        HGroupMoveKind::FixClue,
        HGroupMoveKind::Elimination,
    ]
    .into_iter()
    .any(|kind| signals.has_at_turn(entry.turn, kind))
    {
        return;
    }
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
    if touched.is_empty()
        || touched
            .iter()
            .any(|card| !was_clued_before_with(view, entry.turn, *card, *clue))
        || !touched
            .iter()
            .all(|card| was_clued_before(view, entry.turn, *card) || chop_moved.contains(card))
    {
        return;
    }
    push_signal(
        signals,
        entry,
        *giver,
        Some(*target),
        HGroupMoveKind::TempoClue,
        touched.clone(),
        None,
    );
    let playable_count = touched
        .iter()
        .filter(|card| {
            identity_of(view, **card).is_some_and(|identity| is_playable_now(view, identity))
        })
        .count();
    if playable_count < 2 {
        if let Some(card) = chop(&hands[target.index()], explicitly_clued) {
            chop_moved.insert(card);
            push_signal(
                signals,
                entry,
                *giver,
                Some(*target),
                HGroupMoveKind::ChopMove,
                vec![card],
                None,
            );
            push_signal(
                signals,
                entry,
                *giver,
                Some(*target),
                HGroupMoveKind::TempoClueChopMove,
                vec![card],
                None,
            );
        }
    }
}
#[allow(clippy::too_many_lines)]
pub(in crate::h_group) fn apply_emergency_discard_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
    trash_order_enabled: bool,
) {
    // Sources:
    // - https://hanabi.github.io/level-7/#the-scream-discard-chop-move-sdcm
    // - https://hanabi.github.io/level-7/#the-scream-discard-chop-move-with-known-trash
    // - https://hanabi.github.io/level-7/#the-shout-discard-chop-move
    // - https://hanabi.github.io/level-7/#the-generation-discard
    // - https://hanabi.github.io/level-14/#the-trash-order-chop-move-tocm
    // - https://hanabi.github.io/level-14/#the-shout-discard-order-chop-move
    let entry = context.entry;
    let ObservedEvent::Discarded {
        player,
        card,
        identity: discarded_identity,
    } = &entry.event
    else {
        return;
    };
    let hands_before = &context.before.hands;
    let hands_after = context.after.hands;
    let facts = &context.before.facts;
    let stack_heights = context.before.stack_heights;
    let explicitly_clued = effects.explicitly_clued;
    let pending_connections = &*effects.pending;
    // Emergency-discard interpretation must be reproducible from public
    // knowledge available to the discarder. Looking at convention identities
    // recovered from visible simulator truth made other players see a Scream
    // Discard that the discarder themselves could not know they performed.
    let actor_hand = &hands_before[player.index()];
    let known_playable = actor_hand.iter().any(|candidate| {
        pending_connections.iter().any(|connection| {
            connection.actor == *player
                && connection.cards.contains(candidate)
                && pending_is_active(connection, pending_connections)
        }) || {
            let identities = IdentitySet::from_mask(facts[candidate.index()].identity_mask());
            let live_identities = identities
                .iter()
                .filter(|identity| !is_trash_at(stack_heights, *identity))
                .collect::<Vec<_>>();
            let has_useful_touch = explicitly_clued.contains(candidate)
                && !effects.signals.iter().any(|signal| {
                    signal.turn < entry.turn
                        && matches!(
                            signal.kind,
                            HGroupMoveKind::TrashPush
                                | HGroupMoveKind::Discharge
                                | HGroupMoveKind::UnknownTrashDischarge
                                | HGroupMoveKind::UnknownDupeDischarge
                        )
                        && signal.cards.contains(candidate)
                });
            (identities.len() == 1
                && identities
                    .iter()
                    .next()
                    .is_some_and(|identity| is_playable_at(stack_heights, identity)))
                || (has_useful_touch
                    && !live_identities.is_empty()
                    && live_identities
                        .iter()
                        .all(|identity| is_playable_at(stack_heights, *identity)))
        }
    });
    let known_trash_order = actor_hand
        .iter()
        .copied()
        .filter(|candidate| {
            let possibilities = IdentitySet::from_mask(facts[candidate.index()].identity_mask());
            explicitly_clued.contains(candidate)
                && !possibilities.is_empty()
                && possibilities
                    .iter()
                    .all(|identity| is_trash_at(stack_heights, identity))
        })
        .collect::<Vec<_>>();
    let known_trash_cards = known_trash_order.iter().copied().collect::<CardSet>();
    let discarded_known_trash = known_trash_cards.contains(card);
    let discarded_was_chop = context.actor_saw_normal_discard;
    let trash_skip = known_trash_order
        .iter()
        .position(|candidate| candidate == card)
        .filter(|skip| {
            *skip > 0
                && known_trash_order[..*skip]
                    .iter()
                    .all(|skipped| facts[skipped.index()] == facts[card.index()])
        });

    let gotten = protected_cards(
        effects.explicitly_clued,
        effects.invisibly_clued,
        effects.chop_moved,
    );
    let player_after = |distance: usize| {
        PlayerId::new(
            u8::try_from((player.index() + distance) % hands_after.len())
                .expect("standard Hanabi has at most five players"),
        )
    };
    if trash_order_enabled && discarded_known_trash && !known_playable {
        if let Some(skip) = trash_skip {
            let target = player_after(skip);
            if let Some(target_chop) = chop(&hands_after[target.index()], &gotten) {
                effects.chop_moved.insert(target_chop);
                push_signal(
                    effects.signals,
                    entry,
                    *player,
                    Some(target),
                    HGroupMoveKind::TrashOrderChopMove,
                    vec![target_chop],
                    context.historical.identity(target_chop),
                );
            }
            return;
        }
    }

    let shout = discarded_known_trash && known_playable;
    let scream = discarded_was_chop
        && ((known_playable && known_trash_cards.is_empty())
            || (!known_playable && !known_trash_cards.is_empty()));
    if !shout && !scream {
        return;
    }

    let next = next_player(*player, hands_after.len());
    let after_next = next_player(next, hands_after.len());
    let target_chop = |target: PlayerId| chop(&hands_after[target.index()], &gotten);
    let historically_important = |candidate: CardId| {
        context
            .historical
            .identity(candidate)
            .is_none_or(|identity| {
                if is_playable_at(stack_heights, identity) {
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
                                } if removed == identity
                        )
                    })
                    .count();
                let critical = removed + 1 == usize::from(identity.rank.copies());
                let needed_two = identity.rank == Rank::Two
                    && !hands_after.iter().flatten().copied().any(|other| {
                        other != candidate && context.historical.identity(other) == Some(identity)
                    });
                critical || needed_two
            })
    };

    // A Generation Discard gains the only clue Bob can use to save Cathy's
    // endangered chop. It does not Chop Move Bob. This takes precedence over
    // the superficially similar Scream interpretation.
    let has_safe_action = |actor: PlayerId| {
        hands_after[actor.index()].iter().any(|candidate| {
            let possibilities = IdentitySet::from_mask(facts[candidate.index()].identity_mask());
            !possibilities.is_empty()
                && (possibilities
                    .iter()
                    .all(|identity| is_playable_at(stack_heights, identity))
                    || (explicitly_clued.contains(candidate)
                        && possibilities
                            .iter()
                            .all(|identity| is_trash_at(stack_heights, identity))))
        }) || pending_connections.iter().any(|connection| {
            connection.actor == actor && pending_is_active(connection, pending_connections)
        })
    };
    let next_has_safe_action = has_safe_action(next);
    let after_next_has_safe_action = has_safe_action(after_next);
    let after_next_chop = target_chop(after_next);
    let after_next_is_important = after_next_chop.is_some_and(&historically_important);

    if known_playable
        && context.before.clue_tokens == 0
        && !next_has_safe_action
        && !after_next_has_safe_action
        && after_next_is_important
    {
        effects.must_clue.insert(next);
        push_signal(
            effects.signals,
            entry,
            *player,
            Some(after_next),
            HGroupMoveKind::GenerationDiscard,
            after_next_chop.into_iter().collect(),
            Some(*discarded_identity),
        );
        let cautious = after_next_chop
            .and_then(|endangered| context.historical.identity(endangered))
            .is_some_and(|endangered_identity| {
                hands_after[player.index()].iter().any(|candidate| {
                    let possibilities =
                        IdentitySet::from_mask(facts[candidate.index()].identity_mask());
                    explicitly_clued.contains(candidate)
                        && possibilities.len() > 1
                        && possibilities.contains(endangered_identity)
                })
            });
        if cautious {
            // https://hanabi.github.io/extras/discards-misplays/#the-cautious-generation-discard
            push_signal(
                effects.signals,
                entry,
                *player,
                Some(after_next),
                HGroupMoveKind::CautiousGenerationDiscard,
                after_next_chop.into_iter().collect(),
                None,
            );
        }
        return;
    }

    // An ordinary Scream is a zero-clue last resort. At one clue it is only
    // legal when the next player is locked and therefore could not safely
    // discard after receiving the warning.
    let next_locked = !hands_after[next.index()].is_empty()
        && hands_after[next.index()]
            .iter()
            .all(|candidate| gotten.contains(candidate));
    if scream && context.before.clue_tokens > 0 && !(context.before.clue_tokens == 1 && next_locked)
    {
        return;
    }

    let emergency_target = if shout && trash_order_enabled {
        trash_skip.map_or(next, |skip| player_after(skip + 1))
    } else {
        next
    };
    if let Some(target_chop) = target_chop(emergency_target)
        .filter(|candidate| shout || historically_important(*candidate))
    {
        effects.chop_moved.insert(target_chop);
        effects.must_clue.insert(emergency_target);
        push_signal(
            effects.signals,
            entry,
            *player,
            Some(emergency_target),
            if shout && trash_skip.is_some() && trash_order_enabled {
                HGroupMoveKind::TrashOrderChopMove
            } else if shout {
                HGroupMoveKind::ShoutDiscard
            } else {
                HGroupMoveKind::ScreamDiscard
            },
            vec![target_chop],
            context.historical.identity(target_chop),
        );
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(in crate::h_group) fn apply_positional_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) {
    // Sources: https://hanabi.github.io/level-8/#the-positional-discard-indicating-a-play-with-a-discard
    // https://hanabi.github.io/level-8/#the-positional-misplay-indicating-a-play-with-a-misplay
    // https://hanabi.github.io/level-8/#the-double-positional-misplay-indicating-two-plays-with-a-misplay
    let entry = context.entry;
    let hands = context.after.hands;
    let historical_deck_size = context.after.deck_size;
    let stack_heights = context.after.stack_heights;
    let explicitly_clued = effects.explicitly_clued;
    let invisibly_clued = &*effects.invisibly_clued;
    let chop_moved = &*effects.chop_moved;
    let actor_saw_normal_discard = context.actor_saw_normal_discard;
    let pending = &mut *effects.pending;
    let forced_playable = &mut *effects.forced_playable;
    let signals = &mut *effects.signals;
    if let ObservedEvent::Clued {
        giver,
        target,
        touched,
        ..
    } = &entry.event
    {
        // Source: https://hanabi.github.io/level-8/#the-distribution-clue
        if historical_deck_size <= hands.len() {
            let distributed = touched.iter().copied().find(|card| {
                context.historical.identity(*card).is_some_and(|identity| {
                    is_playable_at(stack_heights, identity)
                        && hands.iter().enumerate().any(|(player, hand)| {
                            player != target.index()
                                && hand.iter().copied().any(|other| {
                                    effects.already_playing.contains(&other)
                                        && context.historical.identity(other) == Some(identity)
                                })
                                && hand
                                    .iter()
                                    .filter(|other| {
                                        effects.already_playing.contains(other)
                                            && context.historical.identity(**other).is_some_and(
                                                |known| is_playable_at(stack_heights, known),
                                            )
                                    })
                                    .count()
                                    >= 2
                        })
                })
            });
            if let Some(card) = distributed {
                effects.already_playing.insert(card);
                push_signal(
                    signals,
                    entry,
                    *giver,
                    Some(*target),
                    HGroupMoveKind::DistributionClue,
                    vec![card],
                    context.historical.identity(card),
                );
            }
        }
        return;
    }
    let (player, card, is_misplay) = match &entry.event {
        ObservedEvent::Discarded { player, card, .. } => (*player, *card, false),
        ObservedEvent::Played {
            player,
            card,
            successful: false,
            ..
        } => (*player, *card, true),
        _ => return,
    };
    if historical_deck_size > view.hands.len()
        || was_clued_before(view, entry.turn, card)
        || invisibly_clued.contains(&card)
        || chop_moved.contains(&card)
        || actor_saw_normal_discard
    {
        // A Positional Discard has to be an otherwise-unexplained discard
        // from an ordinary unknown slot. Discarding a directly clued,
        // conventionally clued, or formerly chop-moved card already has a
        // natural interpretation (most commonly, disposing of known trash).
        // It therefore cannot also promise a matching-slot blind play.
        return;
    }
    let gotten = protected_cards(explicitly_clued, invisibly_clued, chop_moved);
    let ordinary_chop = chop(&context.before.hands[player.index()], &gotten) == Some(card);
    if !is_misplay && ordinary_chop {
        // An expected chop discard is ordinary; it cannot simultaneously
        // communicate a Positional Discard to the matching slot.
        return;
    }
    let indicated_slot = hands[player.index()]
        .iter()
        .filter(|candidate| candidate.index() < card.index())
        .count();
    let visible_targets = (1..hands.len())
        .filter_map(|distance| {
            let index = (player.index() + distance) % hands.len();
            let target = PlayerId::new(u8::try_from(index).ok()?);
            let card = hands[index].get(indicated_slot).copied()?;
            let playable = identity_of(view, card)
                .is_some_and(|identity| is_playable_at(stack_heights, identity));
            playable.then_some((target, card))
        })
        .collect::<Vec<_>>();
    // If another player's matching card is visibly playable, that public
    // recipient resolves the positional message. An observer cannot promote
    // their own hidden matching slot past that known target merely because it
    // might also be playable. Only infer the hidden observer as the target
    // when no visible matching play exists.
    let hidden_observer = hands[view.observer.index()]
        .get(indicated_slot)
        .copied()
        .map(|card| (view.observer, card));
    let double = is_misplay && !ordinary_chop;
    let mut targets = if double {
        visible_targets
            .into_iter()
            .rev()
            .take(2)
            .collect::<Vec<_>>()
    } else {
        visible_targets
            .into_iter()
            .next_back()
            .into_iter()
            .collect::<Vec<_>>()
    };
    if targets.is_empty() {
        targets.extend(hidden_observer);
    }
    if targets.is_empty() {
        return;
    }
    let indicated_cards = targets.iter().map(|(_, card)| *card).collect::<Vec<_>>();
    for (target, indicated) in &targets {
        forced_playable.insert(*indicated);
        if let Some(identity) = identity_of(view, *indicated) {
            pending.start(
                entry.turn,
                ConnectionObligation {
                    promise: PromiseId::UNASSIGNED,
                    actor: *target,
                    cards: vec![*indicated],
                    expected: identity,
                    focus_identity: identity,
                    kind: HGroupConnectionKind::Finesse,
                    focus: *indicated,
                    step: 0,
                },
            );
        }
    }
    push_signal(
        signals,
        entry,
        player,
        targets.first().map(|(target, _)| *target),
        if double {
            HGroupMoveKind::DoublePositionalMisplay
        } else if is_misplay {
            HGroupMoveKind::PositionalMisplay
        } else {
            HGroupMoveKind::PositionalDiscard
        },
        indicated_cards,
        targets
            .first()
            .and_then(|(_, indicated)| identity_of(view, *indicated)),
    );
}
#[allow(clippy::too_many_lines)]
pub(in crate::h_group) fn apply_stall_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) {
    // Sources:
    // - https://hanabi.github.io/level-9/#allowable-stall-clues-stall-table
    // - https://hanabi.github.io/level-9/#the-early-game-severity-1-stalling
    // - https://hanabi.github.io/level-9/#double-discard-situations--double-discard-avoidance-dda-severity-2-stalling
    // - https://hanabi.github.io/level-9/#the-fill-in-clue
    // - https://hanabi.github.io/level-9/#the-locked-hand-save-lhs
    // - https://hanabi.github.io/level-9/#the-anxiety-play-forcing-a-locked-player-to-play
    // - https://hanabi.github.io/level-9/#the-8-clue-save-8cs
    // - https://hanabi.github.io/level-8/#burning-end-game-stalling
    let entry = context.entry;
    let prior_gotten = context
        .before
        .hands
        .iter()
        .flatten()
        .copied()
        .filter(|card| {
            was_clued_before(view, entry.turn, *card)
                || effects.invisibly_clued.contains(card)
                || effects.chop_moved.contains(card)
        })
        .collect::<CardSet>();
    let actor = match entry.event {
        ObservedEvent::Clued { giver, .. } => giver,
        ObservedEvent::Played { player, .. } | ObservedEvent::Discarded { player, .. } => player,
        ObservedEvent::Drew { .. } => return,
    };
    let actor_locked = context.before.hands[actor.index()]
        .iter()
        .all(|card| prior_gotten.contains(card));

    if let ObservedEvent::Played { card, .. } = entry.event
        && context.before.clue_tokens == 0
        && actor_locked
    {
        push_signal(
            effects.signals,
            entry,
            actor,
            Some(actor),
            HGroupMoveKind::AnxietyPlay,
            vec![card],
            None,
        );
        return;
    }

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
    if touched.is_empty() {
        return;
    }

    let target_hand = &context.before.hands[target.index()];
    let target_chop = chop(target_hand, &prior_gotten);
    let target_locked_after = target_hand
        .iter()
        .all(|card| prior_gotten.contains(card) || touched.contains(card));
    let at_eight = context.before.clue_tokens == MAX_CLUE_TOKENS && entry.turn > 0;
    let focus = focus(target_hand, touched, target_chop, &prior_gotten);
    let eight_clue_save = at_eight
        && focus.is_some_and(|card| target_hand.first() != Some(&card))
        && effects.clues.iter().rev().any(|interpretation| {
            interpretation.turn == entry.turn
                && matches!(
                    interpretation.kind,
                    HGroupClueKind::Save(super::HGroupSaveKind::EightClue)
                )
        });
    if eight_clue_save {
        push_signal(
            effects.signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::EightClueSave,
            focus.into_iter().collect(),
            None,
        );
        return;
    }

    let locked_hand_save = actor_locked
        && focus == target_chop
        && !target_locked_after
        && !same_turn_signal(effects.signals, entry.turn, HGroupMoveKind::PlayClue)
        && !same_turn_signal(effects.signals, entry.turn, HGroupMoveKind::SaveClue);
    if locked_hand_save {
        push_signal(
            effects.signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::LockedHandSave,
            focus.into_iter().collect(),
            None,
        );
        push_signal(
            effects.signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::Stall,
            touched.clone(),
            None,
        );
        return;
    }

    let prior_action = view.history.iter().rev().find(|prior| {
        prior.turn < entry.turn && !matches!(prior.event, ObservedEvent::Drew { .. })
    });
    let endangered = prior_action.and_then(|prior| match prior.event {
        ObservedEvent::Discarded { identity, .. }
        | ObservedEvent::Played {
            identity,
            successful: false,
            ..
        } => Some(identity),
        _ => None,
    });
    let actor_chop = chop(&context.before.hands[actor.index()], &prior_gotten);
    let double_discard = endangered.is_some_and(|identity| {
        let removed = view
            .history
            .iter()
            .filter(|prior| prior.turn < entry.turn)
            .filter(|prior| match prior.event {
                ObservedEvent::Discarded {
                    identity: removed, ..
                }
                | ObservedEvent::Played {
                    identity: removed,
                    successful: false,
                    ..
                } => removed == identity,
                _ => false,
            })
            .count();
        actor_chop.is_some_and(|card| context.before.facts[card.index()].allows(identity))
            && removed + 1 == usize::from(identity.rank.copies())
            && !is_trash_at(context.before.stack_heights, identity)
    });

    let all_previously_gotten = touched.iter().all(|card| prior_gotten.contains(card));
    let adds_information = touched
        .iter()
        .any(|card| !context.before.facts[card.index()].has_positive_clue(*clue));
    let fill_in = all_previously_gotten && adds_information;
    let burn = all_previously_gotten && !adds_information;
    let ordinary = [
        HGroupMoveKind::PlayClue,
        HGroupMoveKind::SaveClue,
        HGroupMoveKind::Prompt,
        HGroupMoveKind::Finesse,
        HGroupMoveKind::ReverseFinesse,
        HGroupMoveKind::SelfFinesse,
        HGroupMoveKind::LayeredFinesse,
    ]
    .into_iter()
    .any(|kind| same_turn_signal(effects.signals, entry.turn, kind));
    let severity_allows_fill_or_burn = double_discard
        || actor_locked
        || at_eight
        || context.before.deck_size <= context.before.hands.len();

    let exact = if context.before.early_game
        && same_turn_signal(effects.signals, entry.turn, HGroupMoveKind::FiveStall)
    {
        Some(HGroupMoveKind::EarlyGameStall)
    } else if double_discard && !ordinary {
        Some(HGroupMoveKind::DoubleDiscardAvoidance)
    } else if fill_in && severity_allows_fill_or_burn {
        Some(HGroupMoveKind::FillInClue)
    } else if burn && severity_allows_fill_or_burn {
        Some(HGroupMoveKind::Burn)
    } else {
        None
    };
    if all_previously_gotten {
        // Preserve the generic stall effect used by downstream precedence
        // logic. The exact signal below records which Level-9 permission made
        // that otherwise-low-value clue legal.
        push_signal(
            effects.signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::Stall,
            touched.clone(),
            None,
        );
    }
    if let Some(exact) = exact {
        push_signal(
            effects.signals,
            entry,
            *giver,
            Some(*target),
            exact,
            touched.clone(),
            None,
        );
    }
}
