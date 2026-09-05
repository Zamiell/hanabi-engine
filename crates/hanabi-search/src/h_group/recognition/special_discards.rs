use super::{
    CardSet, ConnectionManager, ConnectionObligation, ConventionJournal, HGroupConnectionKind,
    HGroupMoveKind, HGroupRuleEffects, HGroupTurnContext, ObservedEvent, PlayerId, PlayerView,
    PromiseId, identity_of, is_playable_at, is_trash_at, push_signal, same_turn_signal,
    was_clued_before,
};

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(in crate::h_group) fn apply_transfer_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    explicitly_clued: &CardSet,
    invisibly_clued: &mut CardSet,
    already_playing: &mut CardSet,
    pending: &mut ConnectionManager,
    signals: &mut ConventionJournal,
) {
    let entry = context.entry;
    let stack_heights = context.before.stack_heights;
    let hands = context.after.hands;
    // Sources: https://hanabi.github.io/level-10/#the-gentlemans-discard-gd
    // https://hanabi.github.io/level-10/#the-layered-gentlemans-discard
    // https://hanabi.github.io/level-10/#the-baton-discard-bd
    // https://hanabi.github.io/level-10/#the-sarcastic-finesse
    // https://hanabi.github.io/level-10/#the-certain-finesse--the-certain-discard
    // https://hanabi.github.io/level-10/#the-composition-finesse
    let ObservedEvent::Discarded {
        player,
        card,
        identity,
    } = &entry.event
    else {
        return;
    };
    if is_trash_at(stack_heights, *identity)
        || context.actor_before.discarded_identity != Some(*identity)
    {
        // A revealed identity is not proof that the discarder knew it. Both
        // Gentleman's and Baton Discards require exact owner knowledge; an
        // observer may not manufacture a transfer from visible simulator
        // truth that was hidden from the actor.
        // https://hanabi.github.io/level-10/#the-gentlemans-discard-gd
        // https://hanabi.github.io/level-10/#the-baton-discard-bd
        return;
    }
    if signals
        .iter()
        .any(|signal| signal.turn == entry.turn && signal.kind == HGroupMoveKind::SacrificeDiscard)
    {
        return;
    }
    if !was_clued_before(view, entry.turn, *card) {
        return;
    }
    let mut transfer = None;
    let mut kind = HGroupMoveKind::TransferDiscard;
    for distance in 1..hands.len() {
        let index = (player.index() + distance) % hands.len();
        if let Some(target_card) = hands[index].iter().rev().copied().find(|candidate| {
            (explicitly_clued.contains(candidate) || invisibly_clued.contains(candidate))
                && identity_of(view, *candidate)
                    .or_else(|| signals.facts().known_identity(*candidate))
                    .or_else(|| {
                        let literal = context.before.facts[candidate.index()].identity_mask();
                        literal
                            .is_power_of_two()
                            .then(|| crate::IdentitySet::from_mask(literal).iter().next())
                            .flatten()
                    })
                    == Some(*identity)
        }) {
            transfer = Some((PlayerId::new(u8::try_from(index).unwrap_or(0)), target_card));
            kind = if explicitly_clued.contains(&target_card) {
                HGroupMoveKind::SarcasticDiscard
            } else {
                HGroupMoveKind::TransferDiscard
            };
            break;
        }
    }
    if transfer.is_none() && *player != view.observer {
        // Sarcastic Discards take precedence over GDs/Batons, just as
        // Prompts take precedence over Finesses. The recipient cannot see
        // their own matching card; use its pre-discard information instead.
        // Multiple matching clued slots remain a disjunction, not a promise
        // on the newest unclued card.
        // https://hanabi.github.io/level-3/#the-sarcastic-discard-sd
        let candidates = hands[view.observer.index()]
            .iter()
            .copied()
            .filter(|candidate| {
                explicitly_clued.contains(candidate)
                    && context.before.facts[candidate.index()].allows(*identity)
                    && !transfer_was_clued_after_visible_touch(view, entry.turn, *card, *candidate)
                    && signals
                        .facts()
                        .known_identity(*candidate)
                        .is_none_or(|known| known == *identity)
            })
            .collect::<Vec<_>>();
        if candidates.len() == 1 {
            transfer = Some((view.observer, candidates[0]));
            kind = HGroupMoveKind::SarcasticDiscard;
        } else if !candidates.is_empty() {
            push_signal(
                signals,
                entry,
                *player,
                Some(view.observer),
                HGroupMoveKind::SarcasticDiscard,
                candidates,
                Some(*identity),
            );
            return;
        }
    }
    if transfer.is_none() {
        let mut observer_fallback = None;
        for distance in 1..hands.len() {
            let index = (player.index() + distance) % hands.len();
            let ungotten = hands[index]
                .iter()
                .rev()
                .copied()
                .filter(|candidate| {
                    !explicitly_clued.contains(candidate) && !invisibly_clued.contains(candidate)
                })
                .collect::<Vec<_>>();
            if index == view.observer.index() {
                observer_fallback = observer_fallback.or_else(|| {
                    ungotten
                        .first()
                        .copied()
                        .map(|card| (PlayerId::new(u8::try_from(index).unwrap_or(0)), card))
                });
                continue;
            }
            let Some((layer, target_card)) = ungotten
                .iter()
                .copied()
                .enumerate()
                .find(|(_, candidate)| identity_of(view, *candidate) == Some(*identity))
            else {
                continue;
            };
            let playable = is_playable_at(stack_heights, *identity);
            let layer_is_safe = if layer == 0 {
                true
            } else {
                let mut heights = stack_heights;
                ungotten[..layer].iter().all(|layer_card| {
                    let Some(layer_identity) = identity_of(view, *layer_card) else {
                        return false;
                    };
                    let suit = layer_identity.suit.index();
                    if layer_identity.rank.number() != heights[suit] + 1 {
                        return false;
                    }
                    heights[suit] += 1;
                    true
                })
            };
            if !layer_is_safe {
                continue;
            }
            kind = match (playable, layer) {
                (true, 0) => HGroupMoveKind::GentlemansDiscard,
                (true, _) => HGroupMoveKind::LayeredGentlemansDiscard,
                (false, 0) => HGroupMoveKind::BatonDiscard,
                // Layered Baton Discards are explicitly illegal: an
                // unplayable transfer cannot safely unwrap a layer.
                (false, _) => continue,
            };
            transfer = Some((PlayerId::new(u8::try_from(index).unwrap_or(0)), target_card));
            break;
        }
        if transfer.is_none() {
            transfer = observer_fallback;
            if transfer.is_some() {
                kind = if is_playable_at(stack_heights, *identity) {
                    HGroupMoveKind::GentlemansDiscard
                } else {
                    HGroupMoveKind::BatonDiscard
                };
            }
        }
    }
    let Some((target, target_card)) = transfer else {
        return;
    };
    pending.reveal_identity(entry.turn, target, target_card, *identity);
    invisibly_clued.insert(target_card);
    if is_playable_at(stack_heights, *identity) {
        already_playing.insert(target_card);
        pending.start(
            entry.turn,
            ConnectionObligation {
                promise: PromiseId::UNASSIGNED,
                actor: target,
                cards: vec![target_card],
                expected: *identity,
                focus_identity: *identity,
                kind: HGroupConnectionKind::Finesse,
                focus: target_card,
                step: 0,
            },
        );
    }
    push_signal(
        signals,
        entry,
        *player,
        Some(target),
        HGroupMoveKind::TransferDiscard,
        vec![target_card],
        Some(*identity),
    );
    push_signal(
        signals,
        entry,
        *player,
        Some(target),
        kind,
        vec![target_card],
        Some(*identity),
    );
}

/// A giver who can see an already-touched recipient card cannot knowingly
/// touch its useful duplicate. Discarding the later-touched card does not
/// revoke that evidence and manufacture a Sarcastic target.
/// <https://hanabi.github.io/level-1/#good-touch-principle>
/// <https://hanabi.github.io/level-10/#the-gentlemans-discard-gd>
fn transfer_was_clued_after_visible_touch(
    view: &PlayerView,
    turn: u32,
    transferred: hanabi_core::CardId,
    candidate: hanabi_core::CardId,
) -> bool {
    view.history.iter().any(|entry| {
        let ObservedEvent::Clued { giver, touched, .. } = &entry.event else {
            return false;
        };
        entry.turn < turn
            && *giver != view.observer
            && touched.contains(&transferred)
            && !was_clued_before(view, entry.turn, transferred)
            && was_clued_before(view, entry.turn, candidate)
    })
}

pub(in crate::h_group) fn apply_special_finesse_discard_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) {
    // Sources:
    // - https://hanabi.github.io/level-10/#the-sarcastic-finesse
    // - https://hanabi.github.io/level-10/#the-certain-finesse--the-certain-discard
    // - https://hanabi.github.io/level-10/#the-composition-finesse
    let entry = context.entry;
    let ObservedEvent::Clued { giver, target, .. } = entry.event else {
        return;
    };
    let Some(meaning) = effects
        .clues
        .iter()
        .rev()
        .find(|meaning| meaning.turn == entry.turn && meaning.target == target)
    else {
        return;
    };
    let Some(focus_identity) = context.historical.identity(meaning.focus) else {
        return;
    };
    let connection = effects
        .pending
        .iter()
        .find(|connection| connection.focus == meaning.focus);
    let sarcastic = connection.is_some()
        && same_turn_signal(effects.signals, entry.turn, HGroupMoveKind::Finesse)
        && context.before.hands[giver.index()]
            .iter()
            .copied()
            .any(|card| {
                was_clued_before(view, entry.turn, card)
                    && context.before.facts[card.index()]
                        .identity_mask()
                        .count_ones()
                        > 1
                    && context.historical.identity(card) == Some(focus_identity)
            });
    let certain = connection.is_some_and(|connection| {
        context.before.hands[giver.index()]
            .iter()
            .copied()
            .any(|card| {
                was_clued_before(view, entry.turn, card)
                    && context.historical.identity(card) == Some(connection.expected)
            })
    });
    if sarcastic && !effects.discard_now.contains(&meaning.focus) {
        effects.discard_now.push(meaning.focus);
    }
    if certain {
        if let Some(card) = connection
            .and_then(|connection| connection.cards.first())
            .copied()
        {
            if !effects.discard_now.contains(&card) {
                effects.discard_now.push(card);
            }
        }
    }
    for (recognized, kind, cards) in [
        (
            sarcastic,
            HGroupMoveKind::SarcasticFinesse,
            vec![meaning.focus],
        ),
        (
            certain,
            HGroupMoveKind::CertainFinesse,
            connection.map_or_else(Vec::new, |connection| connection.cards.clone()),
        ),
        (
            certain,
            HGroupMoveKind::CertainDiscard,
            connection.map_or_else(Vec::new, |connection| connection.cards.clone()),
        ),
        (
            sarcastic && certain,
            HGroupMoveKind::CompositionFinesse,
            connection.map_or_else(
                || vec![meaning.focus],
                |connection| {
                    connection
                        .cards
                        .iter()
                        .copied()
                        .chain(core::iter::once(meaning.focus))
                        .collect()
                },
            ),
        ),
    ] {
        if recognized {
            push_signal(
                effects.signals,
                entry,
                giver,
                Some(target),
                kind,
                cards,
                None,
            );
        }
    }
}
