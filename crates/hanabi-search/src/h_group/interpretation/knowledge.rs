use super::super::blind_reverse_finesse_is_eligible;
use super::{
    Card, CardId, CardSet, Clue, ClueFacts, ConnectionObligation, ConventionFacts,
    HGroupCardInference, HGroupConnection, HGroupConnectionKind, HGroupMoveKind, HGroupProfile,
    HGroupRuleId, HGroupState, HistoricalView, IdentitySet, LogicalDeductions, PlayerId,
    PlayerView, Rank, chop, elimination_finesse_connection, identity_of, is_eventually_useful,
    is_playable_at, loaded_connection_plan, pending_identity_is_queued, pending_is_active,
    replay_identity_is_queued, rule_enabled,
};

pub(in crate::h_group) fn delayed_focus_identities(
    identities: IdentitySet,
    stack_heights: [u8; 5],
    view: &PlayerView,
    gotten: &CardSet,
    excluded: CardId,
) -> IdentitySet {
    let mask = identities
        .iter()
        .filter(|identity| {
            let height = usize::from(stack_heights[identity.suit.index()]);
            let rank = usize::from(identity.rank.number());
            rank > height + 1
                && ((height + 2)..rank).all(|needed_rank| {
                    let needed = Card::new(identity.suit, Rank::ALL[needed_rank - 1]);
                    view.hands.iter().flatten().any(|card| {
                        card.id != excluded
                            && gotten.contains(&card.id)
                            && card.identity.map_or_else(
                                || card.clues.allows(needed),
                                |actual| actual == needed,
                            )
                    })
                })
        })
        .fold(0, |mask, identity| mask | (1 << identity.index()));
    IdentitySet::from_mask(mask)
}

pub(in crate::h_group) fn find_prompt(
    deductions: &LogicalDeductions,
    explicitly_clued: &CardSet,
    convention_cards: &[HGroupCardInference],
    prefer_convention_identities: bool,
    excluded: CardId,
    connection_identities: IdentitySet,
    focus: CardId,
) -> Option<HGroupConnection> {
    let hand = &deductions.view().hands[deductions.view().observer.index()];
    for card in hand
        .iter()
        .rev()
        .filter(|card| card.id != excluded && explicitly_clued.contains(&card.id))
    {
        let possibilities = if prefer_convention_identities {
            convention_cards
                .iter()
                .find(|note| note.card == card.id)
                .map(|note| note.identities)
                .or_else(|| deductions.possible_identities(card.id))?
        } else {
            deductions.possible_identities(card.id)?
        };
        let matching = possibilities.intersection(connection_identities);
        if matching.is_empty() {
            continue;
        }
        let identity = matching.iter().next()?;
        return Some(HGroupConnection {
            card: card.id,
            identity,
            kind: HGroupConnectionKind::Prompt,
            focus,
        });
    }
    None
}

pub(in crate::h_group) fn identities_at_distance(
    identities: IdentitySet,
    view: &PlayerView,
    distance: u8,
) -> IdentitySet {
    let stack_heights = std::array::from_fn(|index| {
        u8::try_from(view.play_stacks[index].len())
            .expect("a standard stack has at most five cards")
    });
    identities_at_distance_at(identities, stack_heights, distance)
}

pub(in crate::h_group) fn identities_at_distance_at(
    identities: IdentitySet,
    stack_heights: [u8; 5],
    distance: u8,
) -> IdentitySet {
    let mask = identities
        .iter()
        .filter(|identity| {
            let height = stack_heights[identity.suit.index()];
            identity.rank.number() == height + distance + 1
        })
        .fold(0, |mask, identity| mask | (1 << identity.index()));
    IdentitySet::from_mask(mask)
}

#[allow(clippy::too_many_lines)]
pub(in crate::h_group) fn convention_card_inferences(
    deductions: &LogicalDeductions,
    replay: &HGroupState,
) -> Vec<HGroupCardInference> {
    let view = deductions.view();
    let mut cards = view.hands[view.observer.index()]
        .iter()
        .filter_map(|card| {
            deductions
                .possible_identities(card.id)
                .map(|identities| HGroupCardInference {
                    card: card.id,
                    identities,
                    focused: false,
                    saved: false,
                    // Invisible touch also covers passive transfer-discard
                    // knowledge. Only an active pending connection (handled
                    // below) or a forced-play effect creates a play promise.
                    finessed: false,
                })
        })
        .collect::<Vec<_>>();
    for card in &mut cards {
        let narrowed = card
            .identities
            .without(replay.cards.facts.excluded_identities(card.card));
        if !narrowed.is_empty() {
            card.identities = narrowed;
        }
    }

    for clue in &replay.clues {
        let has_existing_prompt_for_delayed_identity = clue.clue == Clue::Rank(Rank::Two)
            && clue.play_identities.iter().any(|identity| {
                let height = usize::from(clue.stack_heights[identity.suit.index()]);
                let rank = usize::from(identity.rank.number());
                if rank <= height + 1 || height >= Rank::ALL.len() {
                    return false;
                }
                let connector = Card::new(identity.suit, Rank::ALL[height]);
                clue.previously_gotten.iter().any(|prior| {
                    cards
                        .iter()
                        .find(|card| card.card == *prior)
                        .is_some_and(|card| card.identities.contains(connector))
                })
            });
        if !replay.cards.invalidated_focuses.contains(&clue.focus) {
            if let Some(card) = cards.iter_mut().find(|card| card.card == clue.focus) {
                let resolved_bluff = replay.signals.of_kind(HGroupMoveKind::Bluff).any(|signal| {
                    signal.cards.len() >= 2
                        && signal.turn >= clue.turn
                        && signal.cards.last() == Some(&clue.focus)
                });
                if resolved_bluff {
                    let one_away =
                        identities_at_distance_at(card.identities, clue.stack_heights, 1);
                    if !one_away.is_empty() {
                        card.identities = one_away;
                    }
                    card.focused = true;
                    card.saved = false;
                } else {
                    let clue_time = clue.play_identities.union(clue.save_identities);
                    // A Play promise is fixed at clue time. When a matching copy
                    // reaches the stack later, the old focus becomes known trash;
                    // it does not silently migrate to the next still-live rank.
                    // Only an explicit Fix may reinterpret that promise.
                    let mut narrowed = card.identities.intersection(clue_time);
                    if let Some(promised) = replay
                        .pending_connections
                        .iter()
                        .find(|pending| pending.focus == clue.focus)
                        .map(|pending| pending.focus_identity)
                    {
                        let demonstrated = narrowed.intersection(IdentitySet::singleton(promised));
                        if !demonstrated.is_empty() {
                            narrowed = demonstrated;
                        }
                    } else if !clue.play_identities.is_empty()
                        && clue.save_identities.is_empty()
                        && ![
                            HGroupMoveKind::Prompt,
                            HGroupMoveKind::Finesse,
                            HGroupMoveKind::LayeredFinesse,
                        ]
                        .into_iter()
                        .any(|kind| {
                            replay
                                .signals
                                .at_turn(clue.turn, kind)
                                .any(|signal| !signal.cards.contains(&clue.focus))
                        })
                    {
                        let has_queued_delayed_identity = narrowed.iter().any(|identity| {
                            let height = usize::from(clue.stack_heights[identity.suit.index()]);
                            let rank = usize::from(identity.rank.number());
                            rank > height + 1
                                && ((height + 1)..rank).all(|needed_rank| {
                                    replay_identity_is_queued(
                                        view,
                                        replay,
                                        Card::new(identity.suit, Rank::ALL[needed_rank - 1]),
                                    )
                                })
                        });
                        if !has_queued_delayed_identity && !has_existing_prompt_for_delayed_identity
                        {
                            // Without an actual Prompt/Finesse obligation or
                            // an existing clued Prompt candidate, an
                            // immediately playable identity has precedence
                            // over a merely hypothetical delayed
                            // interpretation. An existing Prompt candidate
                            // keeps the delayed identity semantically valid,
                            // but a simultaneous direct-play possibility means
                            // that the Prompt is not yet actionable; the focus
                            // remains a superposition instead.
                            let direct = identities_at_distance_at(narrowed, clue.stack_heights, 0);
                            if !direct.is_empty() {
                                narrowed = direct;
                            }
                        }
                    }
                    if clue.play_identities.len() > 1 {
                        // An ambiguous delayed Play clue is conditional on its
                        // connector. Once the lower candidate has actually
                        // reached the stack, the still-live alternative is the
                        // focus identity. Treating the per-card clue note as an
                        // independent fact forgot that implication as soon as
                        // the connection obligation resolved.
                        let live = IdentitySet::from_mask(
                            narrowed
                                .iter()
                                .filter(|identity| {
                                    identity.rank.number()
                                        > u8::try_from(
                                            view.play_stacks[identity.suit.index()].len(),
                                        )
                                        .expect("a standard stack has at most five cards")
                                })
                                .fold(0, |mask, identity| mask | (1 << identity.index())),
                        );
                        if !live.is_empty() {
                            narrowed = live;
                        }
                    }
                    if !narrowed.is_empty() {
                        card.identities = narrowed;
                    }
                    card.focused = true;
                    card.saved |= !card
                        .identities
                        .intersection(clue.save_identities)
                        .is_empty();
                }
            }
        }
        let intentionally_duplicates = [HGroupMoveKind::FixClue, HGroupMoveKind::Duplication]
            .into_iter()
            .any(|kind| replay.signals.has_at_turn(clue.turn, kind));
        if !intentionally_duplicates && clue.focus_identities.len() == 1 {
            for previous in &clue.previously_gotten {
                let Some(card) = cards.iter_mut().find(|card| card.card == *previous) else {
                    continue;
                };
                if clue.giver == view.observer && card.identities.len() > 1 {
                    // A clue giver cannot use the hidden identity of their
                    // own ambiguous card to retroactively apply Good Touch.
                    // Only an exact note makes a duplicate intentional from
                    // the giver's perspective.
                    continue;
                }
                let narrowed = card.identities.without(clue.focus_identities);
                if !narrowed.is_empty() {
                    card.identities = narrowed;
                }
            }
        }
        for (non_focus, good_touch) in &clue.non_focus_identities {
            let convention_dupes = cards
                .iter()
                .filter(|other| other.card != *non_focus && other.identities.len() == 1)
                .fold(IdentitySet::default(), |duplicates, other| {
                    duplicates.union(other.identities)
                });
            if let Some(card) = cards.iter_mut().find(|card| card.card == *non_focus) {
                // Good Touch is a continuing promise that the non-focus card
                // will eventually play, not a mask frozen at clue time. As
                // the stack advances, identities that have become trash fall
                // away, and completed Prompt/Finesse identities remain claimed
                // by the cards that demonstrated them. This is how an older
                // touched Purple card becomes the Purple 5 automatically after
                // the promised Purple 4 plays.
                let still_useful = IdentitySet::from_mask(
                    good_touch
                        .iter()
                        .filter(|identity| is_eventually_useful(view, *identity))
                        .fold(0, |mask, identity| mask | (1 << identity.index())),
                );
                let narrowed = card
                    .identities
                    .intersection(still_useful.without(convention_dupes));
                if !narrowed.is_empty() {
                    card.identities = narrowed;
                }
            }
        }
        for (non_focus, trash) in &clue.non_focus_trash_identities {
            if let Some(card) = cards.iter_mut().find(|card| card.card == *non_focus) {
                let narrowed = card.identities.intersection(*trash);
                if !narrowed.is_empty() {
                    card.identities = narrowed;
                }
            }
        }
    }

    for pending in replay.pending_connections.iter().filter(|pending| {
        pending.actor == view.observer && pending_is_active(pending, &replay.pending_connections)
    }) {
        for pending_card in &pending.cards {
            let Some(card) = cards.iter_mut().find(|card| card.card == *pending_card) else {
                continue;
            };
            let expected = IdentitySet::singleton(pending.expected);
            if pending.kind == HGroupConnectionKind::Finesse {
                let allowed = if pending.cards.len() == 1 {
                    expected
                } else {
                    expected.union(identities_at_distance(card.identities, view, 0))
                };
                let narrowed = card.identities.intersection(allowed);
                if !narrowed.is_empty() {
                    card.identities = narrowed;
                }
                card.finessed = true;
            } else {
                let successful_alternatives = identities_at_distance(card.identities, view, 0);
                let narrowed = card
                    .identities
                    .intersection(expected.union(successful_alternatives));
                if !narrowed.is_empty() {
                    card.identities = narrowed;
                }
                // Later candidates are conditionally constrained only after
                // every earlier candidate is a wrong successful play.
                break;
            }
        }
    }
    for forced in &replay.cards.forced_playable {
        let Some(card) = cards.iter_mut().find(|card| card.card == *forced) else {
            continue;
        };
        let playable = identities_at_distance(card.identities, view, 0);
        if !playable.is_empty() {
            card.identities = playable;
        }
        card.finessed = true;
    }
    for (saved, identities) in &replay.implicit_saves {
        let Some(card) = cards.iter_mut().find(|card| card.card == *saved) else {
            continue;
        };
        let narrowed = card.identities.intersection(*identities);
        if !narrowed.is_empty() {
            card.identities = narrowed;
        }
        card.saved = true;
    }
    cards
}

pub(in crate::h_group) fn convention_playable(
    view: &PlayerView,
    gotten: &CardSet,
    excluded: CardId,
    identity: Card,
) -> bool {
    let stack_height = view.play_stacks[identity.suit.index()].len();
    let rank = usize::from(identity.rank.number());
    if rank <= stack_height {
        return false;
    }
    ((stack_height + 1)..rank).all(|needed_rank| {
        let needed = Card::new(identity.suit, Rank::ALL[needed_rank - 1]);
        view.hands.iter().flatten().any(|card| {
            card.id != excluded
                && gotten.contains(&card.id)
                && card
                    .identity
                    .map_or_else(|| card.clues.allows(needed), |actual| actual == needed)
        })
    })
}

pub(in crate::h_group) fn two_save_allowed(
    view: &PlayerView,
    focus: CardId,
    identity: Card,
    chops: &[Option<CardId>],
) -> bool {
    let visible_copies = view
        .hands
        .iter()
        .flatten()
        .filter(|card| card.id != focus && card.identity == Some(identity))
        .collect::<Vec<_>>();
    visible_copies.is_empty()
        || visible_copies
            .iter()
            .all(|card| chops.contains(&Some(card.id)))
}

#[allow(clippy::too_many_arguments)]
pub(in crate::h_group) fn snapshot_play_identities(
    profile: HGroupProfile,
    identities: IdentitySet,
    giver: PlayerId,
    target: PlayerId,
    focus: CardId,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    facts: &[ClueFacts],
    gotten: &CardSet,
    already_playing: &CardSet,
    pending_connections: &[ConnectionObligation],
    convention_facts: &ConventionFacts,
    chop_moved: &CardSet,
    stack_heights: [u8; 5],
    historical_turn: u32,
    allow_blind_reverse_empathy: bool,
) -> IdentitySet {
    let mask = identities
        .iter()
        .filter(|identity| {
            snapshot_playable(
                profile,
                *identity,
                giver,
                target,
                focus,
                view,
                hands,
                facts,
                gotten,
                already_playing,
                pending_connections,
                convention_facts,
                chop_moved,
                stack_heights,
                Some(HistoricalView::new(view, historical_turn)),
                allow_blind_reverse_empathy,
            )
        })
        .fold(0, |mask, identity| mask | (1 << identity.index()));
    IdentitySet::from_mask(mask)
}

#[allow(clippy::too_many_arguments)]
pub(in crate::h_group) fn snapshot_playable(
    profile: HGroupProfile,
    identity: Card,
    giver: PlayerId,
    target: PlayerId,
    focus: CardId,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    facts: &[ClueFacts],
    gotten: &CardSet,
    already_playing: &CardSet,
    pending_connections: &[ConnectionObligation],
    convention_facts: &ConventionFacts,
    chop_moved: &CardSet,
    stack_heights: [u8; 5],
    historical_view: Option<HistoricalView<'_>>,
    allow_blind_reverse_empathy: bool,
) -> bool {
    let height = usize::from(stack_heights[identity.suit.index()]);
    let rank = usize::from(identity.rank.number());
    if rank <= height {
        return false;
    }
    if rule_enabled(profile, HGroupRuleId::Extras)
        && loaded_connection_plan(
            view,
            Some(hands),
            Some(facts),
            historical_view,
            giver,
            target,
            focus,
            identity,
            gotten,
            already_playing,
            pending_connections,
            stack_heights,
        )
        .is_some()
    {
        return true;
    }
    let accounted_after_first = ((height + 2)..rank).all(|needed_rank| {
        let needed = Card::new(identity.suit, Rank::ALL[needed_rank - 1]);
        snapshot_accounted(needed, focus, view, hands, facts, gotten)
    });
    if !accounted_after_first {
        return false;
    }
    if rank == height + 1 {
        return true;
    }
    if ((height + 1)..rank).all(|needed_rank| {
        let needed = Card::new(identity.suit, Rank::ALL[needed_rank - 1]);
        pending_identity_is_queued(pending_connections, needed)
    }) {
        return true;
    }
    let first = Card::new(identity.suit, Rank::ALL[height]);
    if snapshot_accounted(first, focus, view, hands, facts, gotten) {
        return true;
    }

    snapshot_connection_exists(
        profile,
        first,
        giver,
        target,
        focus,
        view,
        hands,
        facts,
        gotten,
        already_playing,
        stack_heights,
        allow_blind_reverse_empathy,
    ) || (rule_enabled(profile, HGroupRuleId::Elimination)
        && elimination_finesse_connection(
            view,
            hands,
            Some(facts),
            historical_view,
            convention_facts,
            chop_moved,
            stack_heights,
            focus,
            identity,
        )
        .is_some())
}

#[allow(clippy::too_many_arguments)]
pub(in crate::h_group) fn snapshot_connection_exists(
    profile: HGroupProfile,
    expected: Card,
    giver: PlayerId,
    target: PlayerId,
    focus: CardId,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    facts: &[ClueFacts],
    gotten: &CardSet,
    already_playing: &CardSet,
    stack_heights: [u8; 5],
    allow_blind_reverse_empathy: bool,
) -> bool {
    let first_actor = (giver.index() + 1) % hands.len();
    let ordinary_search_len = if rule_enabled(profile, HGroupRuleId::BasicMoves) {
        (target.index() + hands.len() - first_actor) % hands.len() + 1
    } else {
        1
    };
    // A newly touched delayed focus normally searches only through its
    // recipient. A Level-2 Reverse Finesse is the exception: when the
    // recipient can see the exact connector in a later player's immediate
    // Finesse Position, the connection deliberately wraps past them. Keep
    // this snapshot test aligned with `schedule_connection`; previously only
    // candidate validation knew about this exception, so a clue could be
    // admitted as a Reverse Finesse but replayed as a Stall.
    // Source: https://hanabi.github.io/level-2/#the-reverse-finesse
    let direct_reverse_finesse = rule_enabled(profile, HGroupRuleId::BasicMoves)
        && snapshot_direct_reverse_finesse_exists(
            expected,
            giver,
            target,
            focus,
            view,
            hands,
            facts,
            gotten,
            already_playing,
            first_actor,
            ordinary_search_len,
            allow_blind_reverse_empathy,
        );
    let search_len = if direct_reverse_finesse {
        hands.len()
    } else {
        ordinary_search_len
    };
    let player_order = (0..search_len)
        .map(|distance| (first_actor + distance) % hands.len())
        .collect::<Vec<_>>();

    // Prompts take precedence over Finesses even when the prompted player is
    // later in turn order. This is the same ordering used when the connection
    // obligations are materialized by `schedule_connection` below.
    if snapshot_prompt_exists(
        expected,
        giver,
        focus,
        view,
        hands,
        facts,
        gotten,
        already_playing,
        stack_heights,
        &player_order,
    ) {
        return true;
    }

    let layered = rule_enabled(profile, HGroupRuleId::SpecialFinesses);
    let mut unknown_observer_finesse = false;
    for actor_index in player_order {
        if actor_index == target.index() {
            continue;
        }
        let unclued = hands[actor_index]
            .iter()
            .rev()
            .copied()
            .filter(|card| {
                *card != focus && !gotten.contains(card) && !already_playing.contains(card)
            })
            .collect::<Vec<_>>();
        if unclued.is_empty() {
            continue;
        }
        if actor_index == view.observer.index() && giver != view.observer {
            unknown_observer_finesse = true;
            continue;
        }
        let mut simulated = stack_heights;
        for (position, card) in unclued.iter().enumerate() {
            let Some(identity) = identity_of(view, *card) else {
                break;
            };
            if identity == expected {
                if position == 0 || layered {
                    return true;
                }
                break;
            }
            if position > 0 && !layered || !is_playable_at(simulated, identity) {
                break;
            }
            simulated[identity.suit.index()] = identity.rank.number();
        }
    }
    unknown_observer_finesse
}

#[allow(clippy::too_many_arguments)]
fn snapshot_prompt_exists(
    expected: Card,
    giver: PlayerId,
    focus: CardId,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    facts: &[ClueFacts],
    gotten: &CardSet,
    already_playing: &CardSet,
    stack_heights: [u8; 5],
    player_order: &[usize],
) -> bool {
    let mut unknown_observer_prompt = false;
    for &actor_index in player_order {
        let candidates = hands[actor_index]
            .iter()
            .rev()
            .copied()
            .filter(|card| {
                *card != focus
                    && gotten.contains(card)
                    && !already_playing.contains(card)
                    && facts[card.index()].allows(expected)
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            continue;
        }
        if actor_index == view.observer.index() && giver != view.observer {
            unknown_observer_prompt = true;
            continue;
        }
        if candidates
            .iter()
            .position(|card| identity_of(view, *card) == Some(expected))
            .is_some_and(|correct| {
                candidates[..correct].iter().all(|card| {
                    identity_of(view, *card).map_or_else(
                        || {
                            let possibilities =
                                IdentitySet::from_mask(facts[card.index()].identity_mask());
                            !possibilities.is_empty()
                                && possibilities
                                    .iter()
                                    .all(|identity| is_playable_at(stack_heights, identity))
                        },
                        |identity| is_playable_at(stack_heights, identity),
                    )
                })
            })
        {
            return true;
        }
    }
    unknown_observer_prompt
}

#[allow(clippy::too_many_arguments)]
fn snapshot_direct_reverse_finesse_exists(
    expected: Card,
    giver: PlayerId,
    target: PlayerId,
    focus: CardId,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    facts: &[ClueFacts],
    gotten: &CardSet,
    already_playing: &CardSet,
    first_actor: usize,
    ordinary_search_len: usize,
    allow_blind_reverse_empathy: bool,
) -> bool {
    let finesse_positions = (ordinary_search_len..hands.len())
        .filter_map(|distance| {
            let actor_index = (first_actor + distance) % hands.len();
            (actor_index != target.index())
                .then(|| {
                    hands[actor_index]
                        .iter()
                        .rev()
                        .copied()
                        .find(|card| {
                            *card != focus
                                && !gotten.contains(card)
                                && !already_playing.contains(card)
                        })
                        .map(|card| (actor_index, card))
                })
                .flatten()
        })
        .collect::<Vec<_>>();
    let visible = finesse_positions.iter().any(|(actor_index, card)| {
        identity_of(view, *card) == Some(expected)
            || (*actor_index == view.observer.index()
                && facts[card.index()].identity_mask() == 1 << expected.index())
    });
    visible
        || (blind_reverse_finesse_is_eligible(view, giver, allow_blind_reverse_empathy)
            && finesse_positions.iter().any(|(actor_index, card)| {
                *actor_index == view.observer.index() && facts[card.index()].allows(expected)
            }))
}

pub(in crate::h_group) fn snapshot_accounted(
    identity: Card,
    excluded: CardId,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    facts: &[ClueFacts],
    gotten: &CardSet,
) -> bool {
    hands.iter().flatten().copied().any(|card| {
        card != excluded
            && gotten.contains(&card)
            && if hands[view.observer.index()].contains(&card) {
                facts[card.index()].allows(identity)
            } else {
                identity_of(view, card) == Some(identity)
            }
    })
}

#[allow(clippy::too_many_arguments)]
pub(in crate::h_group) fn snapshot_save_identities(
    identities: IdentitySet,
    clue: Clue,
    giver: PlayerId,
    focus: CardId,
    focus_was_chop: bool,
    eight_clue_save: bool,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    gotten: &CardSet,
    _play_identities: IdentitySet,
    stack_heights: [u8; 5],
    discarded: [u8; 25],
) -> IdentitySet {
    if !focus_was_chop && !eight_clue_save {
        return IdentitySet::default();
    }
    let chops = hands
        .iter()
        .map(|hand| chop(hand, gotten))
        .collect::<Vec<_>>();
    let mask = identities
        .iter()
        .filter(|identity| {
            if eight_clue_save {
                return true;
            }
            match clue {
                Clue::Rank(Rank::Five) => identity.rank == Rank::Five,
                Clue::Rank(Rank::Two) if identity.rank == Rank::Two => {
                    identity.rank.number() > stack_heights[identity.suit.index()]
                        && snapshot_two_save_allowed(view, hands, giver, focus, *identity, &chops)
                }
                _ => {
                    identity.rank != Rank::Five
                    // A critical card on chop is a Save even when a delayed
                    // finesse line could eventually play it. Only an
                    // immediately playable focus takes Play precedence.
                    && !is_playable_at(stack_heights, *identity)
                    && discarded[identity.index()] + 1 == identity.rank.copies()
                    && !hands.iter().flatten().copied().any(|card| {
                        card != focus && identity_of(view, card) == Some(*identity)
                    })
                }
            }
        })
        .fold(0, |mask, identity| mask | (1 << identity.index()));
    IdentitySet::from_mask(mask)
}

pub(in crate::h_group) fn snapshot_two_save_allowed(
    view: &PlayerView,
    hands: &[Vec<CardId>],
    giver: PlayerId,
    focus: CardId,
    identity: Card,
    chops: &[Option<CardId>],
) -> bool {
    let visible = hands
        .iter()
        .enumerate()
        .filter(|(player, _)| *player != giver.index())
        .flat_map(|(_, hand)| hand)
        .copied()
        .filter(|card| *card != focus && identity_of(view, *card) == Some(identity))
        .collect::<Vec<_>>();
    visible.is_empty() || visible.iter().all(|card| chops.contains(&Some(*card)))
}

#[allow(clippy::too_many_arguments)]
pub(in crate::h_group) fn snapshot_good_touch_identities(
    card: CardId,
    identities: IdentitySet,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    gotten: &CardSet,
    stack_heights: [u8; 5],
    discarded: [u8; 25],
) -> IdentitySet {
    let mask = identities
        .iter()
        .filter(|identity| {
            let rank = identity.rank.number();
            rank > stack_heights[identity.suit.index()]
                && Rank::ALL
                    .iter()
                    .copied()
                    .filter(|lower| {
                        lower.number() > stack_heights[identity.suit.index()]
                            && lower.number() < rank
                    })
                    .all(|lower| {
                        discarded[Card::new(identity.suit, lower).index()] < lower.copies()
                    })
                && !hands.iter().flatten().copied().any(|candidate| {
                    candidate != card
                        && gotten.contains(&candidate)
                        && identity_of(view, candidate) == Some(*identity)
                })
        })
        .fold(0, |mask, identity| mask | (1 << identity.index()));
    IdentitySet::from_mask(mask)
}
