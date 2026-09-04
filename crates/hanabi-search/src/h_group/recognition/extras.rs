use super::{
    Card, CardId, CardSet, Clue, ConnectionObligation, ConnectionTransitionReason, HGroupClueKind,
    HGroupConnectionKind, HGroupMoveKind, HGroupRuleEffects, HGroupTurnContext, IdentitySet,
    ObservedEvent, PlayerId, PlayerView, PromiseId, Rank, RequiredFix, chop, finesse_position_id,
    five_pulled_card, focus, is_critical, is_playable_at, is_trash_at, next_player,
    protected_cards, push_signal, same_turn_signal, was_clued_before,
};
use crate::h_group::EffectSource;
use crate::h_group::interpretation_resolution::is_ignition;

#[derive(Clone, Debug)]
struct LieComponentPlan {
    focus: CardId,
    focus_identity: Card,
    connections: Vec<ConnectionObligation>,
    required_fix: Option<RequiredFix>,
}

fn cyclic_distance(from: PlayerId, to: PlayerId, player_count: usize) -> usize {
    (to.index() + player_count - from.index()) % player_count
}

fn fix_clue_for_blockers(
    context: &HGroupTurnContext<'_>,
    hand: &[CardId],
    blockers: &[CardId],
    expected: Card,
) -> Option<Clue> {
    let first = context.historical.identity(*blockers.first()?)?;
    [Clue::Suit(first.suit), Clue::Rank(first.rank)]
        .into_iter()
        .find(|clue| {
            !clue.matches(expected)
                && blockers.iter().all(|card| {
                    context
                        .historical
                        .identity(*card)
                        .is_some_and(|identity| clue.matches(identity))
                })
                && hand.iter().copied().all(|card| {
                    context
                        .historical
                        .identity(card)
                        .is_none_or(|identity| !clue.matches(identity) || blockers.contains(&card))
                })
        })
}

/// Finds the lowest-precedence Max line in which one future Fix removes a
/// false layer while the original Finesse remains live.
///
/// Source: <https://hanabi.github.io/extras/special-finesses/#finesses-with-a-lie-component>
#[allow(clippy::too_many_lines)]
fn lie_component_plan(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &HGroupRuleEffects<'_>,
    giver: PlayerId,
    target: PlayerId,
    clue: Clue,
    touched: &[CardId],
) -> Option<LieComponentPlan> {
    let has_higher_precedence_meaning = effects.signals.iter().any(|signal| {
        signal.turn == context.entry.turn
            && !matches!(signal.kind, HGroupMoveKind::Context | HGroupMoveKind::Extra)
    });
    if touched.len() < 2 || has_higher_precedence_meaning {
        return None;
    }
    let hands = context.after.hands;
    let mut gotten = protected_cards(
        effects.explicitly_clued,
        effects.invisibly_clued,
        effects.chop_moved,
    );
    for card in touched {
        gotten.remove(card);
    }
    gotten.extend(effects.already_playing.iter().copied());
    let layout = &hands[target.index()];
    let focus = focus(layout, touched, chop(layout, &gotten), &gotten)?;
    let focus_identities = context.historical.identity(focus).map_or_else(
        || IdentitySet::from_mask(context.after.facts[focus.index()].identity_mask()),
        IdentitySet::singleton,
    );
    let player_count = hands.len();

    for focus_identity in focus_identities
        .iter()
        .filter(|identity| clue.matches(*identity))
    {
        let height = context.after.stack_heights[focus_identity.suit.index()];
        if focus_identity.rank.number() <= height + 2 {
            continue;
        }
        let mut simulated = context.after.stack_heights;
        let mut scheduled = CardSet::default();
        let mut connections = Vec::new();
        let mut previous_actor = giver;
        let mut used_fix = false;
        let mut required_fix = None;
        let mut failed = false;

        for (step, rank) in ((height + 1)..focus_identity.rank.number()).enumerate() {
            let expected = Card::new(focus_identity.suit, Rank::ALL[usize::from(rank - 1)]);
            if effects
                .pending
                .iter()
                .any(|connection| connection.expected == expected)
                || effects
                    .already_playing
                    .iter()
                    .any(|card| context.historical.identity(*card) == Some(expected))
            {
                simulated[expected.suit.index()] = expected.rank.number();
                continue;
            }

            let first_actor = next_player(previous_actor, player_count);
            let mut choices = Vec::new();
            for distance in 0..player_count {
                let actor = PlayerId::new(
                    u8::try_from((first_actor.index() + distance) % player_count)
                        .expect("standard Hanabi has at most five players"),
                );
                if actor == giver || (actor == target && connections.is_empty()) {
                    continue;
                }
                let candidates = hands[actor.index()]
                    .iter()
                    .rev()
                    .copied()
                    .filter(|card| {
                        *card != focus
                            && !scheduled.contains(card)
                            && (!gotten.contains(card)
                                || (effects
                                    .signals
                                    .of_kind(HGroupMoveKind::TrashChopMove)
                                    .any(|signal| signal.cards.contains(card))
                                    && !effects.explicitly_clued.contains(card)
                                    && !effects.invisibly_clued.contains(card)))
                    })
                    .collect::<Vec<_>>();
                if candidates.is_empty() {
                    continue;
                }
                let expected_position = if actor == view.observer {
                    let compatible = candidates
                        .iter()
                        .enumerate()
                        .filter_map(|(position, card)| {
                            context.after.facts[card.index()]
                                .allows(expected)
                                .then_some(position)
                        })
                        .collect::<Vec<_>>();
                    if rank + 1 == focus_identity.rank.number() && !used_fix {
                        compatible.last().copied()
                    } else {
                        compatible.first().copied()
                    }
                } else {
                    candidates
                        .iter()
                        .position(|card| context.historical.identity(*card) == Some(expected))
                };
                let Some(expected_position) = expected_position else {
                    continue;
                };
                let cards = candidates[..=expected_position].to_vec();
                let mut local = simulated;
                let mut blockers = Vec::new();
                for card in &cards[..expected_position] {
                    let Some(identity) = context.historical.identity(*card) else {
                        blockers.push(*card);
                        continue;
                    };
                    if is_playable_at(local, identity) {
                        local[identity.suit.index()] = identity.rank.number();
                    } else {
                        blockers.push(*card);
                    }
                }
                let fix_is_available = if blockers.is_empty() {
                    true
                } else {
                    !used_fix
                        && cyclic_distance(previous_actor, giver, player_count) > 0
                        && cyclic_distance(previous_actor, giver, player_count)
                            < cyclic_distance(previous_actor, actor, player_count)
                        && ((actor == view.observer
                            && cards.iter().any(|card| {
                                effects
                                    .signals
                                    .of_kind(HGroupMoveKind::TrashChopMove)
                                    .any(|signal| signal.cards.contains(card))
                            }))
                            || fix_clue_for_blockers(
                                context,
                                &hands[actor.index()],
                                &blockers,
                                expected,
                            )
                            .is_some())
                };
                if fix_is_available {
                    let fix = blockers.first().and_then(|card| {
                        context
                            .historical
                            .identity(*card)
                            .map(|identity| RequiredFix {
                                actor: giver,
                                target: actor,
                                focus: *card,
                                identity,
                            })
                    });
                    choices.push((
                        !blockers.is_empty(),
                        std::cmp::Reverse(blockers.len()),
                        distance,
                        actor,
                        cards,
                        local,
                        fix,
                    ));
                }
            }
            // Prefer a clean line. If every line needs a Fix, prefer the Fix
            // that resolves the most intervening cards before using turn
            // order as the tie-breaker. One red Fix covering two layers is
            // stronger than a one-card Fix on an earlier player.
            choices.sort_by_key(|(needs_fix, fixed_layers, distance, ..)| {
                (*needs_fix, *fixed_layers, *distance)
            });
            let Some((needs_fix, _, _, actor, cards, local, fix)) = choices.into_iter().next()
            else {
                failed = true;
                break;
            };
            used_fix |= needs_fix;
            if needs_fix {
                required_fix = fix;
            }
            scheduled.extend(cards.iter().copied());
            connections.push(ConnectionObligation {
                promise: PromiseId::UNASSIGNED,
                actor,
                cards,
                expected,
                focus_identity,
                kind: HGroupConnectionKind::Finesse,
                focus,
                step: u8::try_from(step).expect("a standard connection has at most four steps"),
            });
            simulated = local;
            simulated[expected.suit.index()] = expected.rank.number();
            previous_actor = actor;
        }
        if !failed && used_fix && !connections.is_empty() {
            return Some(LieComponentPlan {
                focus,
                focus_identity,
                connections,
                required_fix,
            });
        }
    }
    None
}

#[allow(clippy::too_many_lines)]
pub(in crate::h_group) fn lie_component_fix_connection(
    context: &HGroupTurnContext<'_>,
    effects: &HGroupRuleEffects<'_>,
    giver: PlayerId,
    target: PlayerId,
    clue: Clue,
    touched: &[CardId],
) -> Option<ConnectionObligation> {
    if effects.clues.last().is_some_and(|current| {
        current.turn == context.entry.turn && current.kind != HGroupClueKind::Unrecognized
    }) {
        // Lie-component interpretations have the lowest possible precedence.
        // A clue with an ordinary Play/Save meaning cannot be repurposed as a
        // Fix merely because an older multi-touch clue resembles a lie line.
        // Source: <https://hanabi.github.io/extras/special-finesses/#finesses-with-a-lie-component>
        return None;
    }
    let connection = effects.pending.iter().find(|connection| {
        connection.actor == target
            && connection.kind == HGroupConnectionKind::Finesse
            && touched.iter().all(|card| connection.cards.contains(card))
            && connection.cards.iter().any(|card| !touched.contains(card))
            && !clue.matches(connection.expected)
            && effects.signals.iter().any(|signal| {
                signal.turn < context.entry.turn
                    && signal.kind == HGroupMoveKind::LieComponentFinesse
                    && signal.cards.contains(&connection.focus)
            })
    });
    connection.cloned().or_else(|| {
        effects.clues.iter().rev().find_map(|prior| {
            let next_giver_turn = prior.turn
                + u32::try_from(context.after.hands.len())
                    .expect("standard Hanabi has at most five players");
            if context.entry.turn != next_giver_turn
                || prior.giver != giver
                || prior.touched.len() < 2
            {
                return None;
            }
            let focus_identity = context.historical.identity(prior.focus)?;
            if prior.clue != Clue::Rank(focus_identity.rank) {
                return None;
            }
            let height = context.after.stack_heights[focus_identity.suit.index()];
            if height + 1 >= focus_identity.rank.number() {
                return None;
            }
            let expected = Card::new(focus_identity.suit, Rank::ALL[usize::from(height)]);
            if clue.matches(expected) {
                return None;
            }
            let card = context.after.hands[target.index()]
                .iter()
                .copied()
                .find(|card| {
                    !touched.contains(card) && context.historical.identity(*card) == Some(expected)
                })?;
            Some(ConnectionObligation {
                promise: PromiseId::UNASSIGNED,
                actor: target,
                cards: vec![card],
                expected,
                focus_identity,
                kind: HGroupConnectionKind::Finesse,
                focus: prior.focus,
                step: height,
            })
        })
    })
}

fn apply_lie_component_fix(
    context: &HGroupTurnContext<'_>,
    effects: &mut HGroupRuleEffects<'_>,
    giver: PlayerId,
    target: PlayerId,
    clue: Clue,
    touched: &[CardId],
) -> bool {
    let Some(connection) =
        lie_component_fix_connection(context, effects, giver, target, clue, touched)
    else {
        return false;
    };
    if connection.promise == PromiseId::UNASSIGNED {
        let cards = connection.cards.clone();
        let promise = effects
            .pending
            .start(context.entry.turn, connection.clone());
        effects
            .invisibly_clued
            .extend_from(EffectSource::Promise(promise), cards);
    }
    effects.pending.repair_actor(
        context.entry.turn,
        target,
        |card| touched.contains(&card),
        |_| None,
    );
    for card in touched {
        if !effects
            .pending
            .iter()
            .any(|pending| pending.cards.contains(card))
        {
            effects.invisibly_clued.remove(card);
        }
    }
    effects.required_fixes.retain(|obligation| {
        let required = obligation.required;
        !(required.target == target && touched.contains(&required.focus))
    });
    push_signal(
        effects.signals,
        context.entry,
        giver,
        Some(target),
        HGroupMoveKind::FixClue,
        touched.to_vec(),
        None,
    );
    push_signal(
        effects.signals,
        context.entry,
        giver,
        context
            .after
            .hands
            .iter()
            .position(|hand| hand.contains(&connection.focus))
            .and_then(|index| u8::try_from(index).ok())
            .map(PlayerId::new),
        HGroupMoveKind::LieComponentFinesse,
        vec![connection.focus],
        Some(connection.focus_identity),
    );
    true
}

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
            let repaired_lie =
                apply_lie_component_fix(context, effects, *giver, *target, *clue, touched);
            if !repaired_lie {
                if let Some(plan) =
                    lie_component_plan(context, view, effects, *giver, *target, *clue, touched)
                {
                    if let Some(required) = plan.required_fix {
                        effects.required_fixes.insert_unconditional(required);
                    }
                    for connection in plan.connections {
                        let cards = connection.cards.clone();
                        let promise = effects.pending.start(entry.turn, connection);
                        if promise != PromiseId::UNASSIGNED {
                            effects
                                .invisibly_clued
                                .extend_from(EffectSource::Promise(promise), cards.iter().copied());
                        }
                    }
                    push_signal(
                        effects.signals,
                        entry,
                        *giver,
                        Some(*target),
                        HGroupMoveKind::LieComponentFinesse,
                        vec![plan.focus],
                        Some(plan.focus_identity),
                    );
                }
            }
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
            let just_in_time_targets = effects
                .pending
                .iter()
                .filter_map(|connection| {
                    let finesse_target = connection
                        .cards
                        .first()
                        .copied()
                        .filter(|card| touched.contains(card))?;
                    (connection.actor == *target
                        && connection.kind == HGroupConnectionKind::Finesse
                        && next_player(*giver, hands.len()) == *target
                        && touched
                            .iter()
                            .all(|card| was_clued_before(view, entry.turn, *card))
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
                        }))
                    .then_some(finesse_target)
                })
                .collect::<Vec<_>>();
            for finesse_target in just_in_time_targets {
                // https://hanabi.github.io/extras/fix-clues/#the-just-in-time-fix-clue-jit
                // JIT fixes the final card that would otherwise be
                // blind-played, not the separately clued focus at the end of
                // the Finesse. Treating a fill-in on the focus as JIT gives a
                // redundant clue false multi-action value.
                push_signal(
                    effects.signals,
                    entry,
                    *giver,
                    Some(*target),
                    HGroupMoveKind::JustInTimeFix,
                    vec![finesse_target],
                    context.historical.identity(finesse_target),
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
            let ignition_already_recognized = effects
                .signals
                .iter()
                .any(|signal| signal.turn == entry.turn && is_ignition(signal.kind));
            let unknown_trash_charm = !ignition_already_recognized
                && non_focus_all_trash
                && effects.signals.iter().any(|signal| {
                    signal.turn == entry.turn
                        && signal.kind == HGroupMoveKind::UnknownTrashDischarge
                });
            let junk_charm = !ignition_already_recognized
                && non_focus_all_trash
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
                            | HGroupMoveKind::UnnecessaryIgnition
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
                let bad_chop_move =
                    (!same_turn_signal(effects.signals, entry.turn, HGroupMoveKind::PlayClue))
                        .then(|| {
                            effects.signals.iter().find(|signal| {
                                signal.turn == entry.turn
                                    && signal.kind == HGroupMoveKind::ChopMove
                                    && signal.cards.iter().all(|card| {
                                        context.historical.identity(*card).is_some_and(|known| {
                                            is_trash_at(context.before.stack_heights, known)
                                        })
                                    })
                            })
                        })
                        .flatten();
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
