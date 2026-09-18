use super::{
    BluffTargetKind, Card, CardSet, Clue, ConnectionTransitionReason, HGroupClueKind,
    HGroupMoveKind, HGroupRuleEffects, HGroupTurnContext, IdentitySet, ObservedEvent, PlayerView,
    Rank, bluff_play_connects, bluff_target_kind_at, bluff_target_order_is_legal, chop,
    finesse_position_id, focus, identity_of, is_critical_save_identity, is_playable_at,
    is_trash_at, next_player, protected_cards, push_signal, same_turn_signal, was_clued_before,
};

#[allow(clippy::too_many_lines)]
pub(in crate::h_group) fn apply_context_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) {
    // Sources: https://hanabi.github.io/level-12/#the-selfish-clue
    // https://hanabi.github.io/level-12/#the-stale-1s-clue
    // https://hanabi.github.io/level-12/#focus-inversion
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
    if effects
        .signals
        .has_at_turn(entry.turn, HGroupMoveKind::FixClue)
    {
        // https://hanabi.github.io/level-3/#the-fix-clue
        // A Fix retracts an earlier identity promise; it does not establish
        // the touched card as the delayed focus of a new Context clue.
        return;
    }
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
    let old_focus = focus(
        &context.before.hands[target.index()],
        touched,
        chop(&context.before.hands[target.index()], &prior_gotten),
        &prior_gotten,
    );
    let newest_touched = context.before.hands[target.index()]
        .iter()
        .rev()
        .copied()
        .find(|card| touched.contains(card));
    let previous_action = view.history.iter().rev().find(|prior| {
        prior.turn < entry.turn && !matches!(prior.event, ObservedEvent::Drew { .. })
    });
    let stale_ones = *clue == Clue::Rank(Rank::One)
        && touched.len() >= 2
        && old_focus != newest_touched
        && previous_action.is_some_and(|prior| {
            matches!(prior.event, ObservedEvent::Discarded { .. }) && prior.turn + 1 == entry.turn
        })
        && old_focus.is_some_and(|card| {
            context
                .historical
                .identity(card)
                .is_some_and(|identity| is_trash_at(context.before.stack_heights, identity))
        })
        && newest_touched.is_some_and(|card| {
            context
                .historical
                .identity(card)
                .is_some_and(|identity| is_playable_at(context.before.stack_heights, identity))
        });
    let directly_impossible_focus = old_focus.is_some_and(|old_focus| {
        !IdentitySet::all().iter().any(|identity| {
            clue.matches(identity)
                && context.before.facts[old_focus.index()].allows(identity)
                && is_playable_at(context.before.stack_heights, identity)
        })
    });
    // https://hanabi.github.io/level-12/#focus-inversion
    // A physically playable newly touched card does not invert focus when the
    // ordinary focus already has a valid Prompt/Finesse chain. For example,
    // a clue can touch green 3 while focusing green 5 through green 4: green
    // 3 starts the line, but green 5 remains its focus.
    let old_focus_has_executable_connection = old_focus.is_some_and(|old_focus| {
        effects.pending.iter().any(|connection| {
            connection.focus == old_focus && effects.pending.was_created_on(connection, entry.turn)
        })
    });
    let primary_is_play = effects
        .clues
        .iter()
        .rev()
        .find(|interpretation| interpretation.turn == entry.turn)
        .is_some_and(|interpretation| {
            matches!(
                interpretation.kind,
                HGroupClueKind::Play | HGroupClueKind::PlayOrSave
            )
        });
    let focus_inversion = primary_is_play
        && touched.len() >= 2
        && old_focus.is_some_and(|card| {
            chop(&context.before.hands[target.index()], &prior_gotten) == Some(card)
        })
        && old_focus != newest_touched
        && directly_impossible_focus
        && !old_focus_has_executable_connection
        && newest_touched.is_some_and(|card| {
            IdentitySet::all().iter().any(|identity| {
                clue.matches(identity)
                    && context.before.facts[card.index()].allows(identity)
                    && is_playable_at(context.before.stack_heights, identity)
            })
        });
    if let (true, Some(old_focus), Some(new_focus)) =
        (stale_ones || focus_inversion, old_focus, newest_touched)
    {
        effects.already_playing.remove(&old_focus);
        effects.forced_playable.remove(&old_focus);
        effects.pending.cancel_where(
            entry.turn,
            ConnectionTransitionReason::FocusInvalidated,
            |connection| connection.focus == old_focus,
        );
        effects.forced_playable.insert(new_focus);
        if stale_ones && !effects.discard_now.contains(&old_focus) {
            effects.discard_now.push(old_focus);
        }
        push_signal(
            effects.signals,
            entry,
            *giver,
            Some(*target),
            if stale_ones {
                HGroupMoveKind::StaleOnesClue
            } else {
                HGroupMoveKind::FocusInversion
            },
            vec![old_focus, new_focus],
            None,
        );
        return;
    }

    let Some(focus) = touched.last().copied() else {
        return;
    };
    let Some(identity) = identity_of(view, focus) else {
        return;
    };
    let height = view.play_stacks[identity.suit.index()].len();
    if usize::from(identity.rank.number()) <= height + 1 {
        return;
    }
    let connector = Card::new(identity.suit, Rank::ALL[height]);
    let selfish = hands[giver.index()].iter().any(|card| {
        effects.explicitly_clued.contains(card) && identity_of(view, *card) == Some(connector)
    });
    push_signal(
        effects.signals,
        entry,
        *giver,
        Some(*target),
        if selfish {
            HGroupMoveKind::SelfishClue
        } else {
            HGroupMoveKind::Context
        },
        vec![focus],
        Some(identity),
    );
    if selfish
        && [
            HGroupMoveKind::Finesse,
            HGroupMoveKind::ReverseFinesse,
            HGroupMoveKind::SelfFinesse,
            HGroupMoveKind::LayeredFinesse,
        ]
        .into_iter()
        .any(|kind| same_turn_signal(effects.signals, entry.turn, kind))
    {
        push_signal(
            effects.signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::SelfishFinesse,
            vec![focus],
            None,
        );
    }
}

#[allow(clippy::too_many_lines)]
pub(in crate::h_group) fn apply_intermediate_bluff_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) {
    // Sources: https://hanabi.github.io/level-13/#the-3-bluff
    // https://hanabi.github.io/level-13/#the-critical-color-bluff-ccb
    // https://hanabi.github.io/level-13/#the-hard-bluff
    // https://hanabi.github.io/level-13/#the-good-touch-bluff
    let entry = context.entry;
    let hands = context.after.hands;
    let stack_heights = context.after.stack_heights;
    let ObservedEvent::Clued {
        giver,
        target,
        clue,
        ..
    } = &entry.event
    else {
        return;
    };
    let actor = next_player(*giver, hands.len());
    if !bluff_target_order_is_legal(*clue, actor, *target) {
        return;
    }
    let Some(interpretation) = effects
        .clues
        .iter()
        .rev()
        .find(|candidate| candidate.turn == entry.turn)
    else {
        return;
    };
    // Intermediate Bluff types refine a Play Clue; Save precedence rules
    // them out. Without this shared-meaning guard, a critical color Save was
    // independently reclassified as a CCB and forced a trash blind play.
    if !same_turn_signal(effects.signals, entry.turn, HGroupMoveKind::PlayClue) {
        return;
    }
    let focus = interpretation.focus;
    // A complete two-step connection is a Double Finesse, not a 3 Bluff.
    // The 3 Bluff remains the fallback when the recipient cannot see that
    // full chain.
    // Sources:
    // - https://hanabi.github.io/level-2/#the-reverse-finesse
    // - https://hanabi.github.io/level-13/#the-3-bluff
    let complete_double_finesse = interpretation
        .hypotheses
        .iter()
        .any(|hypothesis| hypothesis.connection_steps.len() > 1);
    let special_three = !complete_double_finesse
        && !interpretation.focus_identities.is_empty()
        && interpretation.focus_identities.iter().all(|identity| {
            bluff_target_kind_at(stack_heights, *clue, identity) == Some(BluffTargetKind::Three)
        });
    let impossible_new_connections = effects
        .pending
        .iter()
        .filter(|connection| {
            connection.actor == actor
                && actor == view.observer
                && connection.focus == focus
                && effects
                    .pending
                    .provenance(connection.promise)
                    .is_some_and(|origin| origin.created_turn == entry.turn)
                && !context
                    .historical
                    .has_unseen_copy(connection.expected, hands)
        })
        .map(|connection| connection.promise)
        .collect::<Vec<_>>();
    let ruled_out_finesse_bluff = !complete_double_finesse
        && !impossible_new_connections.is_empty()
        && !interpretation.focus_identities.is_empty()
        && interpretation
            .focus_identities
            .iter()
            .all(|identity| bluff_target_kind_at(stack_heights, *clue, identity).is_some());
    if effects.pending.iter().any(|connection| {
        connection.actor == actor
            && effects.pending.is_active(connection)
            && !impossible_new_connections.contains(&connection.promise)
    }) {
        // A loaded player must perform the play already promised to them.
        // The same guard is used by the ordinary Bluff rule; omitting it here
        // made a direct clue to a later player look like an Intermediate Bluff.
        return;
    }
    let Some(bluff_card) = finesse_position_id(&hands[actor.index()], effects.explicitly_clued, 0)
    else {
        return;
    };
    let bluff_identity = identity_of(view, bluff_card);
    if bluff_identity.is_some_and(|identity| {
        !is_playable_at(stack_heights, identity) || bluff_play_connects(*clue, identity)
    }) {
        return;
    }
    let focus_identity = identity_of(view, focus);
    if focus_identity.is_some_and(|identity| is_playable_at(stack_heights, identity)) {
        // A clue on a currently playable focus is a direct Play interpretation
        // (or, when that identity is already promised elsewhere, a Trash Chop
        // Move). It cannot simultaneously manufacture a Critical Color Bluff.
        return;
    }
    let critical_color = matches!(clue, Clue::Suit(_))
        && !was_clued_before(view, entry.turn, focus)
        && focus_identity.is_some_and(|identity| {
            identity.rank != Rank::Five && is_critical_save_identity(view, identity)
        });
    let connecting_identity = bluff_identity.and_then(|identity| {
        (identity.rank != Rank::Five)
            .then(|| Card::new(identity.suit, Rank::ALL[identity.rank.index() + 1]))
    });
    let hard = connecting_identity.is_some_and(|connecting| {
        connecting != focus_identity.unwrap_or(connecting)
            && clue.matches(connecting)
            && context.before.facts[focus.index()].allows(connecting)
    });
    let good_touch = connecting_identity.is_some_and(|connecting| {
        clue.matches(connecting)
            && hands.iter().flatten().copied().any(|card| {
                card != focus
                    && effects.explicitly_clued.contains(&card)
                    && identity_of(view, card) == Some(connecting)
            })
    });
    if !(special_three || critical_color || hard || good_touch || ruled_out_finesse_bluff) {
        return;
    }
    // An impossible same-clue Finesse is not an older play obligation.
    // With every connecting copy visible, the reactor can establish the
    // available Bluff without pretending to hold that connecting identity.
    // https://hanabi.github.io/level-13/#the-3-bluff
    effects.pending.cancel_where(
        entry.turn,
        ConnectionTransitionReason::Superseded,
        |connection| impossible_new_connections.contains(&connection.promise),
    );
    effects.forced_playable.insert(bluff_card);
    push_signal(
        effects.signals,
        entry,
        *giver,
        Some(actor),
        HGroupMoveKind::Bluff,
        vec![bluff_card, focus],
        bluff_identity,
    );
    for exact in [
        special_three.then_some(HGroupMoveKind::ThreeBluff),
        critical_color.then_some(HGroupMoveKind::CriticalColorBluff),
        hard.then_some(HGroupMoveKind::HardBluff),
        good_touch.then_some(HGroupMoveKind::GoodTouchBluff),
    ]
    .into_iter()
    .flatten()
    {
        push_signal(
            effects.signals,
            entry,
            *giver,
            Some(actor),
            exact,
            vec![bluff_card, focus],
            bluff_identity,
        );
    }
}

pub(in crate::h_group) fn apply_double_bluff_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) {
    // Sources: https://hanabi.github.io/level-15/#the-double-bluff
    // https://hanabi.github.io/level-15/#the-hard-double-bluff
    let entry = context.entry;
    let hands = context.after.hands;
    if matches!(entry.event, ObservedEvent::Played { .. }) {
        apply_demonstrated_double_bluff(context, view, effects);
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
    let Some(identity) = touched.last().and_then(|card| identity_of(view, *card)) else {
        return;
    };
    let distance = usize::from(identity.rank.number())
        .saturating_sub(usize::from(context.after.stack_heights[identity.suit.index()]) + 1);
    if distance < 2 {
        return;
    }
    let first = next_player(*giver, hands.len());
    let second = next_player(first, hands.len());
    if next_player(second, hands.len()) != *target {
        return;
    }
    let interpretation = effects
        .clues
        .iter()
        .rev()
        .find(|clue| clue.turn == entry.turn);
    let first_blind_plays = interpretation.and_then(|clue| clue.blind_plays_for(first, identity));
    // The first reactor must actually read a Play/Finesse, not a Save or a
    // 5 Color Ejection. A Double Bluff need not already be an ordinary Bluff:
    // its defining case is precisely a non-ordinary Bluff target.
    if bluff_target_kind_at(context.after.stack_heights, *clue, identity).is_some()
        || super::super::bluff::bluff_through_clued_cards(
            usize::from(context.after.stack_heights[identity.suit.index()]),
            identity,
            |needed| {
                hands.iter().flatten().any(|card| {
                    effects.explicitly_clued.contains(card)
                        && identity_of(view, *card).or_else(|| {
                            effects
                                .signals
                                .facts()
                                .known_identity_before(*card, entry.turn)
                        }) == Some(needed)
                })
            },
        )
        || first_blind_plays.is_none_or(|count| count == 0)
        || (matches!(clue, Clue::Suit(_))
            && identity.rank == Rank::Five
            && first_blind_plays.is_some_and(|count| count >= 2))
    {
        return;
    }
    let first_play = finesse_position_id(&hands[first.index()], effects.explicitly_clued, 0)
        .filter(|card| {
            identity_of(view, *card)
                .is_none_or(|identity| is_playable_at(context.after.stack_heights, identity))
        });
    let second_play = finesse_position_id(&hands[second.index()], effects.explicitly_clued, 0)
        .filter(|card| {
            let mut after_first = context.after.stack_heights;
            if let Some(played) = first_play.and_then(|first| identity_of(view, first)) {
                after_first[played.suit.index()] = played.rank.number();
            }
            identity_of(view, *card).is_none_or(|identity| is_playable_at(after_first, identity))
        });
    if let (Some(first_play), Some(second_play)) = (first_play, second_play) {
        effects.forced_playable.insert(first_play);
        // The second player learns that their blind play is required only
        // when the first play demonstrates this clue on the following turn.
        let first_identity = identity_of(view, first_play);
        let second_identity = identity_of(view, second_play);
        let hard = first_identity
            .is_some_and(|first_identity| first_identity.suit == identity.suit)
            && second_identity.is_some_and(|second_identity| second_identity.suit == identity.suit);
        let cards = vec![first_play, second_play, touched[touched.len() - 1]];
        push_signal(
            effects.signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::DoubleBluff,
            cards.clone(),
            None,
        );
        if hard {
            push_signal(
                effects.signals,
                entry,
                *giver,
                Some(*target),
                HGroupMoveKind::HardDoubleBluff,
                cards.clone(),
                None,
            );
        }
    }
}

/// Two immediate first-position plays resolve the Bluff; no third blind play
/// remains owed. Reconstruct the public evidence, not the simulator's future
/// identities. The second reactor can see the non-ordinary target after the
/// first play even when the recipient cannot yet distinguish it from a Bluff.
/// <https://hanabi.github.io/level-15/#the-double-bluff>
fn apply_demonstrated_double_bluff(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) {
    let ObservedEvent::Played {
        player,
        card,
        identity,
        successful: true,
    } = context.entry.event
    else {
        return;
    };
    if context.before.older_play_obligations.contains(&card)
        || was_clued_before(view, context.entry.turn, card)
    {
        return;
    }
    let Some(clue) = effects.clues.iter().rev().find(|clue| {
        let offset = context.entry.turn.saturating_sub(clue.turn);
        (offset == 1 && player == next_player(clue.giver, view.hands.len()))
            || (offset == 2
                && player
                    == next_player(next_player(clue.giver, view.hands.len()), view.hands.len()))
    }) else {
        return;
    };
    let first = next_player(clue.giver, view.hands.len());
    let second = next_player(first, view.hands.len());
    if next_player(second, view.hands.len()) != clue.target
        || !clue.save_identities.is_empty()
        || finesse_position_id(
            &context.before.hands[player.index()],
            effects.explicitly_clued,
            0,
        ) != Some(card)
    {
        return;
    }
    let offset = context.entry.turn - clue.turn;
    let cards = if offset == 1 {
        let facts = effects.signals.facts();
        let Some(focus_identity) = identity_of(view, clue.focus) else {
            return;
        };
        if is_trash_at(clue.stack_heights, focus_identity)
            || bluff_target_kind_at(clue.stack_heights, clue.clue, focus_identity).is_some()
            || super::super::bluff::bluff_through_clued_cards(
                usize::from(clue.stack_heights[focus_identity.suit.index()]),
                focus_identity,
                |needed| {
                    context.before.hands.iter().flatten().any(|candidate| {
                        was_clued_before(view, clue.turn, *candidate)
                            && identity_of(view, *candidate)
                                .or_else(|| facts.known_identity_before(*candidate, clue.turn))
                                == Some(needed)
                    })
                },
            )
            || bluff_play_connects(clue.clue, identity)
        {
            return;
        }
        let Some(next_card) = finesse_position_id(
            &context.after.hands[second.index()],
            effects.explicitly_clued,
            0,
        ) else {
            return;
        };
        effects.forced_playable.insert(next_card);
        vec![card, next_card, clue.focus]
    } else {
        let Some(first_card) = effects.signals.iter().find_map(|signal| {
            // Reuse the first play's demonstrated causal link. Two unrelated
            // plays are not evidence of a Double Bluff.
            (signal.turn == clue.turn + 1
                && matches!(
                    signal.kind,
                    HGroupMoveKind::Bluff | HGroupMoveKind::DoubleBluff
                )
                && signal.cards.len() >= 2
                && signal.cards.last() == Some(&clue.focus))
            .then(|| signal.cards[0])
        }) else {
            return;
        };
        vec![first_card, card, clue.focus]
    };
    effects.pending.cancel_where(
        context.entry.turn,
        ConnectionTransitionReason::FocusInvalidated,
        |connection| connection.focus == clue.focus,
    );
    effects.already_playing.remove(&clue.focus);
    effects.forced_playable.remove(&clue.focus);
    push_signal(
        effects.signals,
        context.entry,
        clue.giver,
        Some(clue.target),
        HGroupMoveKind::DoubleBluff,
        cards,
        None,
    );
}

#[allow(clippy::too_many_lines)]
pub(in crate::h_group) fn apply_duplication_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) {
    // Sources:
    // - https://hanabi.github.io/level-17/#the-duplicitous-value-clue
    // - https://hanabi.github.io/level-17/#the-duplicitous-blind-play
    // - https://hanabi.github.io/level-17/#the-duplicitous-tempo-clue
    // - https://hanabi.github.io/level-17/#the-assisted-trash-chop-move
    // - https://hanabi.github.io/level-17/#the-time-travel-chop-move-direct-form
    // - https://hanabi.github.io/level-17/#the-time-travel-chop-move-blind-play-form
    let entry = context.entry;
    match &entry.event {
        ObservedEvent::Clued {
            giver,
            target,
            touched,
            ..
        } => {
            let duplicated = touched.iter().find_map(|card| {
                let identity = context.historical.identity(*card)?;
                context
                    .after
                    .hands
                    .iter()
                    .flatten()
                    .copied()
                    .any(|other| {
                        other != *card
                            && effects.explicitly_clued.contains(&other)
                            && context.historical.identity(other) == Some(identity)
                    })
                    .then_some((*card, identity))
            });
            if let Some((card, identity)) = duplicated {
                push_signal(
                    effects.signals,
                    entry,
                    *giver,
                    Some(*target),
                    HGroupMoveKind::Duplication,
                    vec![card],
                    Some(identity),
                );
            }
        }
        ObservedEvent::Played {
            player,
            card,
            identity,
            successful: true,
        } => {
            let gotten = protected_cards(
                effects.explicitly_clued,
                effects.invisibly_clued,
                effects.chop_moved,
            );
            let duplicate_is_gotten = context.after.hands.iter().flatten().copied().any(|other| {
                let is_trash_chop_move_focus = effects.clues.iter().any(|clue| {
                    clue.focus == other
                        && effects.signals.iter().any(|signal| {
                            signal.turn == clue.turn && signal.kind == HGroupMoveKind::TrashChopMove
                        })
                });
                other != *card
                    && (effects.explicitly_clued.contains(&other)
                        || effects.invisibly_clued.contains(&other))
                    && (context.historical.identity(other) == Some(*identity)
                        || IdentitySet::from_mask(
                            context.after.facts[other.index()].identity_mask(),
                        ) == IdentitySet::singleton(*identity)
                        || (is_trash_chop_move_focus
                            && context.after.facts[other.index()].allows(*identity)
                            && IdentitySet::from_mask(
                                context.after.facts[other.index()].identity_mask(),
                            )
                            .iter()
                            .filter(|candidate| *candidate != *identity)
                            .all(|candidate| {
                                is_trash_at(context.before.stack_heights, candidate)
                                    || context.after.hands.iter().flatten().copied().any(
                                        |duplicate| {
                                            duplicate != other
                                                && gotten.contains(&duplicate)
                                                && context.historical.identity(duplicate)
                                                    == Some(candidate)
                                        },
                                    )
                            })))
            });
            if !duplicate_is_gotten {
                return;
            }

            let direct = effects.clues.iter().rev().find(|clue| clue.focus == *card);
            let blind_signal = effects.signals.iter().rev().find(|signal| {
                signal.turn < entry.turn
                    && matches!(
                        signal.kind,
                        HGroupMoveKind::Finesse
                            | HGroupMoveKind::ReverseFinesse
                            | HGroupMoveKind::SelfFinesse
                            | HGroupMoveKind::LayeredFinesse
                    )
                    && signal.cards.contains(card)
            });
            let origin = direct.or_else(|| {
                blind_signal.and_then(|signal| {
                    effects
                        .clues
                        .iter()
                        .rev()
                        .find(|clue| clue.turn == signal.turn)
                })
            });
            let Some(origin) = origin.cloned() else {
                return;
            };

            // For a blind play the clued focus is also an extra card: it is
            // distinct from the duplicated card that just played. Level 17's
            // Duplicitous Blind-Play explicitly values saving that focus.
            // Only direct plays consume the focus itself. Reviewed p4v0s415
            // turn 31: duplicated g4 still secures Donald's newly clued p3.
            let newly_secured_focus = (direct.is_none()
                && !origin.previously_gotten.contains(&origin.focus))
            .then_some(origin.focus);
            let valuable_extra = origin
                .new_non_focus
                .iter()
                .copied()
                .chain(newly_secured_focus)
                .any(|extra| {
                    context.historical.identity(extra).map_or_else(
                        || {
                            extra == origin.focus
                                && origin
                                    .play_identities
                                    .union(origin.save_identities)
                                    .iter()
                                    .any(|candidate| !is_trash_at(origin.stack_heights, candidate))
                        },
                        |candidate| !is_trash_at(origin.stack_heights, candidate),
                    )
                });
            let filled_in = view
                .history
                .iter()
                .find_map(|history| {
                    (history.turn == origin.turn).then(|| match &history.event {
                        ObservedEvent::Clued { touched, .. } => touched.iter().any(|touched| {
                            *touched != origin.focus
                                && !origin.new_non_focus.contains(touched)
                                && was_clued_before(view, origin.turn, *touched)
                        }),
                        _ => false,
                    })
                })
                .unwrap_or(false);

            let assisted_trash = direct.is_none()
                && context
                    .historical
                    .identity(origin.focus)
                    .is_some_and(|focus_identity| {
                        is_trash_at(origin.stack_heights, focus_identity)
                    })
                && origin.new_non_focus.is_empty();
            let kind = if assisted_trash {
                HGroupMoveKind::AssistedTrashChopMove
            } else if valuable_extra {
                if direct.is_some() {
                    HGroupMoveKind::DuplicitousValue
                } else {
                    HGroupMoveKind::DuplicitousBlindPlay
                }
            } else if filled_in {
                HGroupMoveKind::DuplicitousTempo
            } else {
                HGroupMoveKind::TimeTravelChopMove
            };

            let mut affected = vec![*card];
            if matches!(
                kind,
                HGroupMoveKind::TimeTravelChopMove | HGroupMoveKind::AssistedTrashChopMove
            ) {
                let (owner, anchor) = if kind == HGroupMoveKind::AssistedTrashChopMove {
                    (origin.target, origin.focus)
                } else {
                    (*player, *card)
                };
                let hand = &context.before.hands[owner.index()];
                if let Some(position) = hand.iter().position(|candidate| *candidate == anchor) {
                    let gotten = protected_cards(
                        effects.explicitly_clued,
                        effects.invisibly_clued,
                        effects.chop_moved,
                    );
                    let moved = hand[..position]
                        .iter()
                        .rev()
                        .copied()
                        .filter(|candidate| !gotten.contains(candidate))
                        .collect::<Vec<_>>();
                    effects.chop_moved.extend(moved.iter().copied());
                    affected.extend(moved);
                }
            }

            effects.pending.cancel_where(
                entry.turn,
                ConnectionTransitionReason::IdentitySatisfiedElsewhere,
                |connection| connection.focus == origin.focus,
            );
            effects.already_playing.remove(&origin.focus);
            push_signal(
                effects.signals,
                entry,
                *player,
                Some(origin.target),
                kind,
                affected,
                Some(*identity),
            );
        }
        ObservedEvent::Played { .. }
        | ObservedEvent::Discarded { .. }
        | ObservedEvent::Drew { .. } => {}
    }
}
