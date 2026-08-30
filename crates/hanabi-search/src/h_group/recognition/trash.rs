use super::{
    Card, CardId, CardSet, Clue, ClueFacts, ConnectionManager, ConnectionObligation,
    ConnectionTransitionReason, ConventionJournal, HGroupClueInterpretation, HGroupConnectionKind,
    HGroupMoveKind, HGroupRuleEffects, HGroupTurnContext, IdentitySet, ObservedEvent,
    ObservedHistoryEntry, PlayerId, PlayerView, PromiseId, Rank, chop, finesse_position_id,
    identity_of, is_playable_at, is_playable_now, is_trash_at, next_player, pending_is_active,
    protected_cards, push_signal, same_turn_signal, was_clued_before,
};
use crate::h_group::{EffectSource, ProvenancedCardSet};

fn retract_connections(
    entry: &ObservedHistoryEntry,
    actor: PlayerId,
    connections: Vec<ConnectionObligation>,
    pending: &mut ConnectionManager,
    invisibly_clued: &mut ProvenancedCardSet,
    signals: &mut ConventionJournal,
) {
    let promises = connections
        .iter()
        .map(|connection| connection.promise)
        .collect::<Vec<_>>();
    pending.cancel_where(
        entry.turn,
        ConnectionTransitionReason::Superseded,
        |connection| promises.contains(&connection.promise),
    );
    for connection in connections {
        invisibly_clued.retract_source(
            EffectSource::Promise(connection.promise),
            ConnectionTransitionReason::Superseded,
        );
        push_signal(
            signals,
            entry,
            actor,
            Some(connection.actor),
            HGroupMoveKind::Retraction,
            connection.cards,
            Some(connection.expected),
        );
    }
}

pub(in crate::h_group) fn apply_trash_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) {
    // Sources: https://hanabi.github.io/level-14/#the-trash-push
    // https://hanabi.github.io/level-14/#the-trash-finesse
    // https://hanabi.github.io/level-14/#the-reverse-trash-finesse
    // https://hanabi.github.io/level-14/#the-trash-bluff
    let entry = context.entry;
    let hands = context.after.hands;
    let stack_heights = context.after.stack_heights;
    match &entry.event {
        ObservedEvent::Clued {
            giver,
            target,
            clue,
            touched,
            ..
        } if !touched.is_empty()
            && touched.iter().all(|card| {
                identity_of(view, *card)
                    .is_some_and(|identity| is_trash_at(stack_heights, identity))
            }) =>
        {
            let hand = &hands[target.index()];
            // Post-event rule dispatch runs after the physical clue has added
            // every touched card to `explicitly_clued`. Trash Push semantics,
            // however, depend on whether the trash was chop immediately
            // before the clue. Reconstruct that prior protected set without
            // erasing cards that were already explicitly or invisibly gotten.
            let mut explicitly_clued_before = effects.explicitly_clued.clone();
            for card in touched {
                if !was_clued_before(view, entry.turn, *card) {
                    explicitly_clued_before.remove(card);
                }
            }
            let gotten_before = protected_cards(
                &explicitly_clued_before,
                effects.invisibly_clued,
                effects.chop_moved,
            );
            let focus = touched
                .iter()
                .filter_map(|card| {
                    hand.iter()
                        .position(|candidate| candidate == card)
                        .map(|p| (p, *card))
                })
                .max_by_key(|(position, _)| *position)
                .map(|(_, card)| card);
            if let Some(focus) = focus.filter(|focus| chop(hand, &gotten_before) == Some(*focus)) {
                // A known-trash clue is a Trash Push only when the trash
                // itself is on chop. Off-chop trash retains the lower-level
                // Trash Chop Move meaning and moves every intervening card.
                // Sources:
                // - https://hanabi.github.io/level-4/#the-trash-chop-move-tcm
                // - https://hanabi.github.io/level-14/#the-trash-push
                push_signal(
                    effects.signals,
                    entry,
                    *giver,
                    Some(*target),
                    HGroupMoveKind::TrashPush,
                    vec![focus],
                    identity_of(view, focus),
                );
                if let Some(position) = hand.iter().position(|card| *card == focus) {
                    if let Some(pushed) = hand.get(position + 1).copied() {
                        effects.chop_moved.insert(pushed);
                    }
                }
            }
        }
        ObservedEvent::Discarded {
            player,
            card,
            identity,
        } if is_trash_at(stack_heights, *identity)
            && !was_clued_before(view, entry.turn, *card) =>
        {
            let target = next_player(*player, hands.len());
            let playable_finesse = hands[target.index()].last().copied().and_then(|finesse| {
                identity_of(view, finesse)
                    .filter(|expected| is_playable_now(view, *expected))
                    .map(|expected| (finesse, expected))
            });
            if let Some((finesse, expected)) = playable_finesse {
                effects.pending.start(
                    entry.turn,
                    ConnectionObligation {
                        promise: PromiseId::UNASSIGNED,
                        actor: target,
                        cards: vec![finesse],
                        expected,
                        focus_identity: expected,
                        kind: HGroupConnectionKind::Finesse,
                        focus: finesse,
                        step: 0,
                    },
                );
                push_signal(
                    effects.signals,
                    entry,
                    *player,
                    Some(target),
                    HGroupMoveKind::TrashPush,
                    vec![finesse],
                    Some(expected),
                );
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_lines)]
pub(in crate::h_group) fn apply_trash_connection_refinements(
    context: &HGroupTurnContext<'_>,
    effects: &mut HGroupRuleEffects<'_>,
) {
    // Sources:
    // - https://hanabi.github.io/level-14/#the-trash-push-prompt--the-trash-push-finesse
    // - https://hanabi.github.io/level-14/#the-trash-finesse
    // - https://hanabi.github.io/level-14/#the-reverse-trash-finesse
    // - https://hanabi.github.io/level-14/#the-forced-gentlemans-discard-chop-move
    // - https://hanabi.github.io/level-14/#the-trash-bluff
    let entry = context.entry;
    if let ObservedEvent::Discarded {
        player,
        card,
        identity,
    } = entry.event
    {
        let forced_gentleman = is_playable_at(context.before.stack_heights, identity)
            && effects
                .signals
                .of_kind(HGroupMoveKind::ReverseTrashFinesse)
                .rev()
                .any(|signal| {
                    signal.turn < entry.turn
                        && signal.target == Some(player)
                        && signal.cards.contains(&card)
                });
        if forced_gentleman {
            if let Some(position) = context.before.hands[player.index()]
                .iter()
                .position(|candidate| *candidate == card)
            {
                effects.chop_moved.extend(
                    context.before.hands[player.index()][..position]
                        .iter()
                        .copied(),
                );
            }
            push_signal(
                effects.signals,
                entry,
                player,
                Some(player),
                HGroupMoveKind::ForcedGentlemansDiscardChopMove,
                vec![card],
                None,
            );
        }
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
    let Some(trash_focus) = effects
        .signals
        .iter()
        .rev()
        .find(|signal| signal.turn == entry.turn && signal.kind == HGroupMoveKind::TrashPush)
        .and_then(|signal| signal.cards.first())
        .copied()
    else {
        return;
    };
    let connection = effects
        .pending
        .iter()
        .find(|connection| connection.focus == trash_focus || touched.contains(&connection.focus));
    let Some(connection) = connection else {
        return;
    };
    let player_count = context.after.hands.len();
    let distance =
        |player: PlayerId| (player.index() + player_count - giver.index()) % player_count;
    let reverse = distance(connection.actor) > distance(*target);
    let bluff = same_turn_signal(effects.signals, entry.turn, HGroupMoveKind::Bluff);
    let exact_connection = match connection.kind {
        HGroupConnectionKind::Prompt => HGroupMoveKind::TrashPushPrompt,
        HGroupConnectionKind::Finesse => HGroupMoveKind::TrashPushFinesse,
    };
    for kind in [
        Some(HGroupMoveKind::TrashFinesse),
        Some(exact_connection),
        reverse.then_some(HGroupMoveKind::ReverseTrashFinesse),
        bluff.then_some(HGroupMoveKind::TrashBluff),
    ]
    .into_iter()
    .flatten()
    {
        push_signal(
            effects.signals,
            entry,
            *giver,
            Some(connection.actor),
            kind,
            connection
                .cards
                .iter()
                .copied()
                .chain(core::iter::once(trash_focus))
                .collect(),
            None,
        );
    }
    if reverse && !effects.discard_now.contains(&trash_focus) {
        effects.discard_now.push(trash_focus);
    }
    if let Some(position) = context.before.hands[target.index()]
        .iter()
        .position(|card| *card == trash_focus)
    {
        effects.chop_moved.extend(
            context.before.hands[target.index()][..position]
                .iter()
                .copied(),
        );
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(in crate::h_group) fn apply_ejection_discharge_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands_before: &[Vec<CardId>],
    hands: &[Vec<CardId>],
    facts: &[ClueFacts],
    clues: &[HGroupClueInterpretation],
    stack_heights: [u8; 5],
    explicitly_clued: &CardSet,
    invisibly_clued: &mut ProvenancedCardSet,
    chop_moved: &CardSet,
    pending: &mut ConnectionManager,
    forced_playable: &mut CardSet,
    discard_now: &mut Vec<CardId>,
    signals: &mut ConventionJournal,
    extras_enabled: bool,
) {
    // Sources:
    // - https://hanabi.github.io/level-16/#the-5-color-ejection-5ce
    // - https://hanabi.github.io/level-16/#the-unknown-trash-discharge-1-for-1-form-utd
    // - https://hanabi.github.io/level-16/#the-unknown-trash-discharge-2-for-1-form-utd
    // - https://hanabi.github.io/level-16/#the-unknown-dupe-discharge-udd
    // - https://hanabi.github.io/extras/ejection-extensions/#the-out-of-position-ejection
    // - https://hanabi.github.io/extras/ejection-extensions/#the-stacked-ejection
    if let ObservedEvent::Played {
        player,
        card,
        identity,
        successful: true,
    } = &entry.event
    {
        let prior = view
            .history
            .iter()
            .find(|prior| prior.turn.saturating_add(1) == entry.turn);
        let prior_clue = prior.and_then(|prior| match &prior.event {
            ObservedEvent::Clued {
                giver,
                target,
                clue: Clue::Suit(suit),
                ..
            } if next_player(*giver, hands_before.len()) == *player => {
                Some((prior, *giver, *target, *suit))
            }
            _ => None,
        });
        if let Some((prior, giver, target, suit)) = prior_clue {
            let prior_interpretation = clues.iter().rev().find(|clue| clue.turn == prior.turn);
            let mut gotten = explicitly_clued.clone();
            gotten.extend(chop_moved.iter().copied());
            let played_from_ejection_position =
                finesse_position_id(&hands_before[player.index()], &gotten, 1) == Some(*card);
            if played_from_ejection_position
                && identity.suit != suit
                && prior_interpretation.is_some_and(|interpretation| {
                    !was_clued_before(view, prior.turn, interpretation.focus)
                        && stack_heights[suit.index()] <= 3
                })
            {
                let focus = prior_interpretation
                    .expect("the Ejection position check requires a clue interpretation")
                    .focus;
                let connections = pending
                    .iter()
                    .filter(|connection| connection.focus == focus)
                    .cloned()
                    .collect::<Vec<_>>();
                retract_connections(entry, giver, connections, pending, invisibly_clued, signals);
                forced_playable.remove(&focus);
                push_signal(
                    signals,
                    entry,
                    giver,
                    Some(target),
                    HGroupMoveKind::FiveColorEjection,
                    vec![focus],
                    Some(Card::new(suit, Rank::Five)),
                );
            }
        }
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
    let interpretation = clues.iter().rev().find(|clue| clue.turn == entry.turn);
    let focus_identity =
        interpretation.and_then(|interpretation| identity_of(view, interpretation.focus));
    let ejection_actor = next_player(*giver, hands.len());
    let blind_plays = interpretation.map_or(0, |interpretation| {
        let Some(identity) = focus_identity else {
            return 0;
        };
        let previously_gotten = interpretation
            .previously_gotten
            .iter()
            .copied()
            .collect::<CardSet>();
        ((stack_heights[identity.suit.index()] + 1)..identity.rank.number())
            .filter(|rank| {
                let needed = Card::new(identity.suit, Rank::ALL[usize::from(*rank - 1)]);
                !hands.iter().flatten().copied().any(|card| {
                    previously_gotten.contains(&card) && identity_of(view, card) == Some(needed)
                })
            })
            .count()
    });
    let five_ejection = matches!(clue, Clue::Suit(_))
        && interpretation.is_some_and(|interpretation| {
            !was_clued_before(view, entry.turn, interpretation.focus)
        })
        && focus_identity.is_some_and(|identity| {
            identity.rank == Rank::Five
                && 5_u8.saturating_sub(stack_heights[identity.suit.index()]) >= 2
        })
        && blind_plays >= 2;
    // An Unknown Trash Discharge communicates that the focused card is trash.
    // Merely touching an already-played duplicate as a useful non-focus card is
    // an ordinary multi-card clue and must not eject the next player's slot 3.
    let unknown_discharge = touched.len() >= 2
        && !same_turn_signal(signals, entry.turn, HGroupMoveKind::PlayClue)
        && interpretation.is_none_or(|interpretation| interpretation.save_identities.is_empty())
        && interpretation.is_some_and(|interpretation| {
            let possibilities =
                IdentitySet::from_mask(facts[interpretation.focus.index()].identity_mask());
            !possibilities.is_empty()
                && possibilities
                    .iter()
                    .all(|identity| is_trash_at(stack_heights, identity))
        });
    let unknown_dupe_discharge = touched.len() >= 2
        && !same_turn_signal(signals, entry.turn, HGroupMoveKind::PlayClue)
        && !signals.iter().any(|signal| {
            signal.turn == entry.turn
                && matches!(signal.kind, HGroupMoveKind::FixClue | HGroupMoveKind::Stall)
        })
        && interpretation.is_some_and(|interpretation| {
            let possibilities =
                IdentitySet::from_mask(facts[interpretation.focus.index()].identity_mask());
            !possibilities.is_empty()
                && !possibilities
                    .iter()
                    .any(|identity| is_trash_at(stack_heights, identity))
                && possibilities
                    .iter()
                    .all(|identity| !is_playable_at(stack_heights, identity))
                && touched.iter().any(|first| {
                    touched.iter().any(|second| {
                        first != second
                            && identity_of(view, *first).is_some()
                            && identity_of(view, *first) == identity_of(view, *second)
                    })
                })
        });
    let (kind, position) = if five_ejection {
        (Some(HGroupMoveKind::FiveColorEjection), 1)
    } else if unknown_discharge {
        (Some(HGroupMoveKind::UnknownTrashDischarge), 2)
    } else if unknown_dupe_discharge {
        (Some(HGroupMoveKind::UnknownDupeDischarge), 2)
    } else {
        (None, 0)
    };
    if let Some(kind) = kind {
        let mut gotten = protected_cards(explicitly_clued, invisibly_clued, chop_moved);
        // The ordinary connection scheduler runs before advanced precedence
        // rules. Its same-turn apparent Finesse on the 5 must not consume
        // Finesse Positions while deciding whether this clue is an Ejection.
        for connection in pending
            .iter()
            .filter(|connection| pending.was_created_on(connection, entry.turn))
        {
            for card in &connection.cards {
                let protected_by_older_connection = pending.iter().any(|older| {
                    !pending.was_created_on(older, entry.turn) && older.cards.contains(card)
                });
                if !explicitly_clued.contains(card)
                    && !chop_moved.contains(card)
                    && !protected_by_older_connection
                {
                    gotten.remove(card);
                }
            }
        }
        let loaded_connections = pending
            .iter()
            .filter(|connection| {
                connection.actor == ejection_actor
                    && !pending.was_created_on(connection, entry.turn)
                    && pending_is_active(connection, pending)
            })
            .filter_map(|connection| connection.cards.first().copied())
            .collect::<CardSet>()
            .len();
        let available = hands[ejection_actor.index()]
            .iter()
            .filter(|card| !gotten.contains(card))
            .count();
        let stacked = extras_enabled
            && loaded_connections == 1
            && blind_plays + loaded_connections > available;
        let requested_position = if stacked { 0 } else { position };
        let mut actor = ejection_actor;
        let loaded_connection = stacked.then(|| {
            pending.iter().find(|connection| {
                connection.actor == ejection_actor
                    && !pending.was_created_on(connection, entry.turn)
                    && pending_is_active(connection, pending)
            })
        });
        let mut card = loaded_connection
            .flatten()
            .and_then(|connection| connection.cards.get(1).copied())
            .or_else(|| {
                loaded_connection.flatten().and_then(|connection| {
                    let anchor = *connection.cards.first()?;
                    let anchor_position = hands[actor.index()]
                        .iter()
                        .position(|candidate| *candidate == anchor)?;
                    hands[actor.index()][..anchor_position]
                        .iter()
                        .rev()
                        .copied()
                        .find(|candidate| !gotten.contains(candidate))
                })
            })
            .or_else(|| finesse_position_id(&hands[actor.index()], &gotten, requested_position));
        let mut out_of_position = false;
        if extras_enabled && card.is_none() {
            for distance in 2..hands.len() {
                let candidate = PlayerId::new(
                    u8::try_from((giver.index() + distance) % hands.len())
                        .expect("standard Hanabi has at most five players"),
                );
                if let Some(candidate_card) =
                    finesse_position_id(&hands[candidate.index()], &gotten, requested_position)
                {
                    actor = candidate;
                    card = Some(candidate_card);
                    out_of_position = true;
                    break;
                }
            }
        }
        let Some(card) = card else {
            // An Ejection or Discharge cannot supersede an existing connection
            // when the requested ungotten position does not exist.
            return;
        };
        let apparent_focus = interpretation.map(|interpretation| interpretation.focus);
        let same_turn_connections = pending
            .iter()
            .filter(|connection| {
                pending.was_created_on(connection, entry.turn)
                    && apparent_focus == Some(connection.focus)
            })
            .cloned()
            .collect::<Vec<_>>();
        retract_connections(
            entry,
            *giver,
            same_turn_connections,
            pending,
            invisibly_clued,
            signals,
        );
        forced_playable.insert(card);
        if matches!(
            kind,
            HGroupMoveKind::UnknownTrashDischarge | HGroupMoveKind::UnknownDupeDischarge
        ) {
            if let Some(focus) = interpretation.map(|interpretation| interpretation.focus) {
                if !discard_now.contains(&focus) {
                    discard_now.push(focus);
                }
            }
        }
        if kind == HGroupMoveKind::FiveColorEjection && focus_identity.is_some() {
            if let (Some(focus), Clue::Suit(suit)) = (
                interpretation.map(|interpretation| interpretation.focus),
                clue,
            ) {
                // The Ejection meaning proves that the color-clued focus is
                // the 5, replacing its apparent ordinary Play interpretation.
                // Keep this identity claim focus-only; the ejected card is an
                // unrelated successful blind play, not another copy of the 5.
                push_signal(
                    signals,
                    entry,
                    *giver,
                    Some(*target),
                    HGroupMoveKind::FiveColorEjection,
                    vec![focus],
                    Some(Card::new(*suit, Rank::Five)),
                );
            }
        }
        let mut affected = vec![card];
        affected.extend(touched.iter().copied());
        push_signal(
            signals,
            entry,
            *giver,
            Some(actor),
            if stacked {
                if matches!(
                    kind,
                    HGroupMoveKind::UnknownTrashDischarge | HGroupMoveKind::UnknownDupeDischarge
                ) {
                    HGroupMoveKind::StackedDischarge
                } else {
                    HGroupMoveKind::StackedEjection
                }
            } else if out_of_position {
                if matches!(
                    kind,
                    HGroupMoveKind::UnknownTrashDischarge | HGroupMoveKind::UnknownDupeDischarge
                ) {
                    HGroupMoveKind::OutOfPositionDischarge
                } else {
                    HGroupMoveKind::OutOfPositionEjection
                }
            } else {
                kind
            },
            affected,
            None,
        );
    }
}
