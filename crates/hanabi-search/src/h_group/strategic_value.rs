use super::{
    Action, ActionCommitment, Card, CardId, CluePurpose, CluedCardSuperposition,
    CompiledClueAction, CompiledObserverProjection, CompiledProspectiveClue, EpistemicState,
    HGroupConnection, HGroupMoveKind, HGroupProfile, HGroupRuleId, IdentitySet, LineOutcome,
    LogicalDeductions, PlayerId, PlayerView, Rank, RecipientCardConsequence,
    RecipientCardDisposition, card_is_trash, compiled_baseline_team, compiled_prospective_clue,
    identity_of, is_eventually_useful, is_playable_at, is_playable_now, rule_enabled,
};

const TEAM_ACTION_COVERAGE_PENALTY: u16 = 80;
const TEAM_ACTION_COUNT_PENALTY: u16 = 100;
const NAMED_LINE_ACTION_DEFICIT_PENALTY: u16 = 80;
const TEAM_MULTI_CARD_PROTECTION_BONUS: u16 = 80;
const TEAM_ACTION_DELAY_PENALTY: u16 = 2;
const TEAM_OCCUPIED_TARGET_PENALTY: u16 = 20;
const PLAY_OVER_SAVE_PENALTY: u16 = 80;
const CRITICAL_CHOP_DEADLINE_PENALTY: u16 = 80;
const BOTTOM_DECK_RISK_PROTECTION_BONUS: u16 = 80;
const UNNECESSARY_CONNECTION_COMPLEXITY_PENALTY: u16 = 24;
const STALLED_MULTI_STEP_CONNECTION_PENALTY: u16 = 280;
// A heuristic cost for destroying a verified positional opportunity while
// postponing it in favor of a recoverable direct clue. Not a Bluff bonus.
const POSITIONAL_OPPORTUNITY_LOSS_PENALTY: u16 = 40;

/// Compares whole clue outcomes after ordinary legality and convention
/// interpretation have produced the candidate set.
///
/// [Level 6's Clarity Principle](https://hanabi.github.io/level-6/#clarity-principle-part-1)
/// prefers the least complicated route when both the promised actions and
/// every clued card's owner-visible identity superposition are identical. Its
/// broader [Part 2](https://hanabi.github.io/level-6/#clarity-principle-part-2)
/// is represented by the general penalties for confusing, stalled multi-step
/// connections. Team action coverage separately rewards a clue that establishes
/// useful future actions for more than one teammate.
#[allow(clippy::too_many_lines)]
pub(super) fn apply_strategic_clue_values(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    candidates: &mut [CompiledClueAction],
) {
    if !rule_enabled(profile, HGroupRuleId::TempoCluesAndClarity) {
        return;
    }
    let source = deductions.view();
    let baseline_team = compiled_baseline_team(source, profile);
    let baselines = (0..source.hands.len())
        .map(|player| {
            let observer = PlayerId::new(
                u8::try_from(player).expect("standard Hanabi has at most five players"),
            );
            baseline_team
                .projection(observer)
                .map(|projection| projected_line_state(source, projection))
        })
        .collect::<Option<Vec<_>>>();
    let Some(baselines) = baselines else {
        return;
    };
    let values = candidates
        .iter()
        .map(|candidate| {
            clue_line_value(
                source,
                profile,
                candidate.action,
                &baselines,
                candidate.move_kind(),
            )
        })
        .collect::<Vec<_>>();
    let best_coverage = values
        .iter()
        .filter_map(|value| value.as_ref().map(LineOutcome::covered_players))
        .max()
        .unwrap_or(0);
    let opportunity_losses = positional_opportunity_losses(source, profile, candidates);
    let immediately_actionable = values
        .iter()
        .map(|value| {
            value.as_ref().is_some_and(|value| {
                let mut actions = value.play_consequences().peekable();
                actions.peek().is_some()
                    && actions.all(|consequence| {
                        consequence.disposition == RecipientCardDisposition::PlayNow
                    })
            })
        })
        .collect::<Vec<_>>();
    let best_action_count = values
        .iter()
        .zip(&immediately_actionable)
        .filter_map(|(value, immediate)| {
            (*immediate).then(|| value.as_ref().map(|value| value.action_coverage))?
        })
        .max()
        .unwrap_or(0);
    let best_action_distance = values
        .iter()
        .filter_map(|value| {
            value
                .as_ref()
                .map(|value| value.first_action_distance(source.current_player, source.hands.len()))
        })
        .min()
        .unwrap_or(source.hands.len());
    let best_named_line_action_count = values
        .iter()
        .filter_map(|value| value.as_ref()?.convention_action_count)
        .max();
    let current_stack_heights = std::array::from_fn(|suit| {
        u8::try_from(source.play_stacks[suit].len())
            .expect("a standard stack has at most five cards")
    });
    let target_has_scheduled_play = |target: PlayerId| {
        scheduled_play_continuation_value(
            source,
            &baselines[target.index()],
            target,
            current_stack_heights,
        )
        .is_some()
    };
    let occupied_by_connection = baseline_team
        .projection(source.observer)
        .map(|projection| occupied_by_visible_connection(source, &projection.replay))
        .unwrap_or_default();
    let critical_chop_deadline_values = values
        .iter()
        .map(|value| {
            value.as_ref().map_or(0, |value| {
                secured_critical_chop_deadline_value(
                    source,
                    &baselines,
                    value,
                    current_stack_heights,
                    &occupied_by_connection,
                )
            })
        })
        .collect::<Vec<_>>();
    let best_critical_chop_deadline_value = critical_chop_deadline_values
        .iter()
        .copied()
        .max()
        .unwrap_or(0);
    let bottom_deck_risk_values = values
        .iter()
        .map(|value| {
            value
                .as_ref()
                .map_or(0, |value| bottom_deck_risk_protection(source, value))
        })
        .collect::<Vec<_>>();
    let has_unoccupied_immediate_target = candidates.iter().any(|candidate| {
        candidate.immediate_play() && !target_has_scheduled_play(candidate.target())
    });
    let mut best_immediate_action_count_by_target: Vec<Option<usize>> =
        vec![None; source.hands.len()];
    for (candidate, value) in candidates.iter().zip(&values) {
        if !candidate.immediate_play() {
            continue;
        }
        let Some(value) = value else {
            continue;
        };
        let action_count = value
            .convention_action_count
            .unwrap_or(value.action_coverage)
            .max(value.action_coverage);
        let best = &mut best_immediate_action_count_by_target[candidate.target().index()];
        *best = Some(best.map_or(action_count, |prior| prior.max(action_count)));
    }
    for (index, candidate) in candidates.iter_mut().enumerate() {
        let Some(value) = &values[index] else {
            continue;
        };
        candidate.set_preserves_visible_continuation(preserves_visible_continuation(
            source, *candidate, &baselines,
        ));
        let action_coverage = value.action_coverage;
        candidate.set_compiled_line(value);
        if opportunity_losses[index] {
            candidate
                .value
                .penalize_opportunity(POSITIONAL_OPPORTUNITY_LOSS_PENALTY);
        }
        // Credit actual protection, independently of the other candidates.
        // A deficit from the best clue unfairly penalizes productive clues
        // against ordinary discards, which are outside this clue-only pass
        // and protect none of these identities either.
        candidate.value.reward_protection(
            BOTTOM_DECK_RISK_PROTECTION_BONUS
                .saturating_mul(u16::try_from(bottom_deck_risk_values[index]).unwrap_or(u16::MAX)),
        );
        let candidate_action_count = value
            .convention_action_count
            .unwrap_or(value.action_coverage)
            .max(value.action_coverage);
        let missed_critical_chop_deadline =
            best_critical_chop_deadline_value.saturating_sub(critical_chop_deadline_values[index]);
        candidate.value.penalize_teamwork(
            CRITICAL_CHOP_DEADLINE_PENALTY
                .saturating_mul(u16::try_from(missed_critical_chop_deadline).unwrap_or(u16::MAX)),
        );
        let same_target_immediate_play_is_at_least_as_productive =
            best_immediate_action_count_by_target[candidate.target().index()]
                .is_some_and(|best| best >= candidate_action_count);
        if candidate.is_save() && same_target_immediate_play_is_at_least_as_productive {
            // An immediate Play Clue occupies the recipient, so their critical
            // chop cannot be discarded on this turn. Prefer advancing the
            // stack over spending the clue only to Save that same player,
            // unless the Save secures a strictly longer action line.
            // Source: https://hanabi.github.io/beginner/other-general-strategy/#give-play-clues-over-save-clues
            candidate.value.penalize_teamwork(PLAY_OVER_SAVE_PENALTY);
        }
        if let (Some(best), Some(actual)) =
            (best_named_line_action_count, value.convention_action_count)
        {
            // Compare named convention lines by the actions they actually
            // secure, not by an apparent connection's raw depth. This remains
            // observer-relative and applies to opening clues as well.
            candidate
                .value
                .penalize_teamwork(NAMED_LINE_ACTION_DEFICIT_PENALTY.saturating_mul(
                    u16::try_from(best.saturating_sub(actual)).unwrap_or(u16::MAX),
                ));
        }
        let extends_existing_owner_promise = match candidate.action {
            Action::Clue { target, clue } => source.hands[target.index()].iter().any(|card| {
                card.identity.is_some_and(|identity| clue.matches(identity))
                    && baselines[target.index()]
                        .owner_promises
                        .iter()
                        .any(|(promised, _)| *promised == card.id)
            }),
            Action::Play(_) | Action::Discard(_) => false,
        };
        let actor_recognizes =
            clue_establishes_actor_recognized_action(source, profile, candidate.action);
        let giver_commitments =
            baselines[source.observer.index()].closed_public_commitments(source);
        let continues_established_suit = value.play_consequences().any(|consequence| {
            let identity = consequence.identities.iter().next();
            identity.is_some_and(|identity| {
                consequence.identities.len() == 1
                    && usize::from(identity.rank.number())
                        > source.play_stacks[identity.suit.index()].len() + 1
                    && ((source.play_stacks[identity.suit.index()].len() + 1)
                        ..usize::from(identity.rank.number()))
                        .all(|rank| {
                            giver_commitments.iter().any(|(_, promised)| {
                                promised.suit == identity.suit
                                    && usize::from(promised.rank.number()) == rank
                            })
                        })
            })
        });
        if candidate.purpose() == super::CluePurpose::Play
            && !candidate.immediate_play()
            && source.turn > 0
            && !extends_existing_owner_promise
            && !continues_established_suit
            && value.action_coverage == 0
            && actor_recognizes != Some(true)
        {
            candidate
                .value
                .penalize_teamwork(STALLED_MULTI_STEP_CONNECTION_PENALTY);
        }
        let uncovered_players = best_coverage.saturating_sub(value.covered_players());
        candidate.value.penalize_teamwork(
            TEAM_ACTION_COVERAGE_PENALTY
                .saturating_mul(u16::try_from(uncovered_players).unwrap_or(u16::MAX)),
        );
        // The single-step projection is reliable for comparing concrete
        // remaining plays once the deck is short. Earlier in the game,
        // advanced clues (especially Bluffs) deliberately defer actions past
        // this projection horizon, so a raw action-count penalty would make
        // ordinary multi-card clues incorrectly beat them.
        let cards_in_hands = source.hands.iter().map(Vec::len).sum::<usize>();
        let effective_action_count = value
            .convention_action_count
            .unwrap_or(action_coverage)
            .max(action_coverage);
        let missing_actions = if source.deck_size <= cards_in_hands && immediately_actionable[index]
        {
            // A recognized Bluff/Finesse line can have more deterministic
            // actions than the giver's one-step public-commitment projection.
            // Penalizing only the latter made a two-action Bluff lose to a
            // direct one-for-one clue at the endgame boundary.
            best_action_count.saturating_sub(effective_action_count)
        } else {
            0
        };
        candidate.value.penalize_teamwork(
            TEAM_ACTION_COUNT_PENALTY
                .saturating_mul(u16::try_from(missing_actions).unwrap_or(u16::MAX)),
        );
        if has_unoccupied_immediate_target
            && candidate.immediate_play()
            && target_has_scheduled_play(candidate.target())
            && action_coverage <= 1
        {
            // Giving a player a second immediate play delays the action they
            // already own. For otherwise equivalent one-action clues, occupy
            // an unoccupied teammate so both promises can advance during the
            // same rotation. Do not erase the value of a clue that establishes
            // multiple actions merely because its recipient is already loaded.
            candidate
                .value
                .penalize_teamwork(TEAM_OCCUPIED_TARGET_PENALTY);
        }
        let consolidates_chop_move = match candidate.action {
            Action::Clue { target, clue } => source.hands[target.index()].iter().any(|card| {
                card.identity.is_some_and(|identity| clue.matches(identity))
                    && baselines[source.observer.index()]
                        .chop_moved
                        .contains(&card.id)
            }),
            Action::Play(_) | Action::Discard(_) => false,
        };
        if consolidates_chop_move {
            let extra_protection = match candidate.action {
                Action::Clue { target, clue } => source.hands[target.index()]
                    .iter()
                    .filter_map(|card| card.identity)
                    .filter(|identity| clue.matches(*identity))
                    .filter(|identity| is_eventually_useful(source, *identity))
                    .count()
                    .saturating_sub(1),
                Action::Play(_) | Action::Discard(_) => 0,
            };
            candidate.value.reward_teamwork(
                TEAM_MULTI_CARD_PROTECTION_BONUS
                    .saturating_mul(u16::try_from(extra_protection).unwrap_or(u16::MAX)),
            );
        }
        let action_delay = value
            .first_action_distance(source.current_player, source.hands.len())
            .saturating_sub(best_action_distance);
        candidate.value.penalize_delay(
            TEAM_ACTION_DELAY_PENALTY
                .saturating_mul(u16::try_from(action_delay).unwrap_or(u16::MAX)),
        );

        let fewest_equivalent_connections = values
            .iter()
            .filter_map(Option::as_ref)
            .filter(|other| other.has_same_clarity_outcome(value))
            .map(|other| other.new_connections)
            .min()
            .unwrap_or(value.new_connections);
        let unnecessary_connections = value
            .new_connections
            .saturating_sub(fewest_equivalent_connections);
        candidate.value.penalize_complexity(
            UNNECESSARY_CONNECTION_COMPLEXITY_PENALTY
                .saturating_mul(u16::try_from(unnecessary_connections).unwrap_or(u16::MAX)),
        );
    }
    // Semantic refinement overrides legacy base-category preferences only
    // when all protected cards and public actions are unchanged. Process the
    // strongest knowledge first so chains of refinements remain ordered.
    let mut order = (0..candidates.len()).collect::<Vec<_>>();
    order.sort_by_key(|index| {
        core::cmp::Reverse(
            values[*index]
                .as_ref()
                .map_or(0, |value| value.owner_actions.len()),
        )
    });
    for index in order {
        let Some(value) = &values[index] else {
            continue;
        };
        let ceiling = values
            .iter()
            .enumerate()
            .filter(|(other_index, other)| {
                candidates[*other_index].target() == candidates[index].target()
                    && other
                        .as_ref()
                        .is_some_and(|other| other.strictly_improves_owner_knowledge(value))
            })
            .map(|(other_index, _)| candidates[other_index].score())
            .min();
        if let Some(better_score) = ceiling {
            candidates[index]
                .value
                .rank_below_owner_refinement(better_score);
        }
    }
}

/// Counts distinct, not-yet-playable identities that this clue protects from
/// bottom-deck risk. If every physical copy is already visible, the team can
/// trivially give a normal Save Clue later; protecting that identity now does
/// not mitigate a hidden-copy ordering risk.
/// Protection comes from the compiled causal outcome, including indirect
/// connections, not just physical touches in the clue recipient's hand.
fn bottom_deck_risk_protection(source: &PlayerView, value: &LineOutcome) -> usize {
    value
        .protected_cards
        .iter()
        .filter_map(|card| identity_of(source, *card))
        .filter(|identity| {
            is_eventually_useful(source, *identity)
                && !is_playable_now(source, *identity)
                && source
                    .hands
                    .iter()
                    .flatten()
                    .filter(|card| card.identity == Some(*identity))
                    .count()
                    < usize::from(identity.rank.copies())
        })
        .fold(IdentitySet::default(), |identities, identity| {
            identities.union(IdentitySet::singleton(identity))
        })
        .len()
}

/// Compare two concrete orders: a direct play followed by a positional
/// clue, versus the positional clue followed by the still-available direct
/// clue. Drawing after the direct play replaces the known blind-play position
/// with an unknown card. The latter order preserves the direct clue's touch.
///
/// This deliberately does not predict the unknown draw, or treat all advanced
/// clues as urgent. The original direct touch must remain intact after the
/// blind play, and it must not protect a critical card requiring a Save now.
/// <https://hanabi.github.io/level-11/#bluffs>
fn positional_opportunity_losses(
    source: &PlayerView,
    profile: HGroupProfile,
    candidates: &[CompiledClueAction],
) -> Vec<bool> {
    let mut losses = vec![false; candidates.len()];
    if source.deck_size == 0 {
        return losses;
    }
    let reactor = super::next_player(source.current_player, source.hands.len());
    for alternative in candidates {
        if !matches!(
            alternative.move_kind(),
            Some(HGroupMoveKind::Bluff | HGroupMoveKind::Ejection)
        ) {
            continue;
        }
        let Action::Clue { target, clue } = alternative.action else {
            continue;
        };
        let touched = source.hands[target.index()]
            .iter()
            .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
            .map(|card| card.id)
            .collect::<Vec<_>>();
        let Some(compiled) = compiled_prospective_clue(source, profile, target, clue, &touched)
        else {
            continue;
        };
        let Some(projection) = compiled.projection(source.observer) else {
            continue;
        };
        let anchor = projection.replay.signals.iter().find_map(|signal| {
            (signal.turn == source.turn
                && signal.target == Some(reactor)
                && matches!(
                    signal.kind,
                    HGroupMoveKind::Bluff
                        | HGroupMoveKind::FiveColorEjection
                        | HGroupMoveKind::StackedEjection
                )
                && signal.cards.len() >= 2)
                .then(|| signal.cards[0])
        });
        let Some(anchor) = anchor.filter(|card| {
            identity_of(source, *card).is_some_and(|identity| is_playable_now(source, identity))
        }) else {
            continue;
        };
        let hand = &source.hands[reactor.index()];
        let Some(anchor_position) = hand.iter().position(|card| card.id == anchor) else {
            continue;
        };
        for (index, direct) in candidates.iter().enumerate() {
            if direct.target() != reactor
                || !direct.immediate_play()
                || direct.purpose() != CluePurpose::Play
                || direct.is_urgent_save()
            {
                continue;
            }
            let Action::Clue { clue, .. } = direct.action else {
                continue;
            };
            let direct_touch = hand
                .iter()
                .enumerate()
                .filter(|(_, card)| card.identity.is_some_and(|identity| clue.matches(identity)))
                .collect::<Vec<_>>();
            if direct_touch.iter().any(|(_, card)| {
                card.id == anchor
                    || card
                        .identity
                        .is_some_and(|identity| super::is_critical(source, identity))
            }) {
                continue;
            }
            let playable = direct_touch
                .iter()
                .filter(|(_, card)| {
                    card.identity
                        .is_some_and(|identity| is_playable_now(source, identity))
                })
                .collect::<Vec<_>>();
            let [(position, _)] = playable.as_slice() else {
                continue;
            };
            // Playing an older card then drawing shifts the known blind slot.
            // Playing a newer card can leave that slot unchanged. Conversely,
            // playing the anchor leaves this untouched direct focus in hand.
            losses[index] |= *position < anchor_position;
        }
    }
    losses
}

/// Finds players occupied by a visible, successful pending blind play.
fn occupied_by_visible_connection(
    source: &PlayerView,
    replay: &super::HGroupState,
) -> Vec<PlayerId> {
    // A pending blind play occupies its owner even when the owner-relative
    // identity closure does not resolve the card. The giver may verify that
    // the visible first layer succeeds without exposing it to its owner.
    replay
        .pending_connections
        .iter()
        .filter(|connection| {
            replay.pending_connections.is_active(connection)
                && connection.cards.first().is_some_and(|card| {
                    identity_of(source, *card).is_some_and(|actual| is_playable_now(source, actual))
                })
        })
        .map(|connection| connection.actor)
        .collect()
}

/// Values protection by the turn on which an otherwise-unoccupied player
/// would reach a critical chop. Earlier deadlines dominate later ones.
fn secured_critical_chop_deadline_value(
    source: &PlayerView,
    baselines: &[ProjectedLineState],
    value: &LineOutcome,
    stack_heights: [u8; 5],
    occupied_by_connection: &[PlayerId],
) -> usize {
    let player_count = source.hands.len();
    baselines
        .iter()
        .enumerate()
        .filter_map(|(player, baseline)| {
            let actor = PlayerId::new(
                u8::try_from(player).expect("standard Hanabi has at most five players"),
            );
            let distance = (player + player_count - source.current_player.index()) % player_count;
            if distance == 0
                || occupied_by_connection.contains(&actor)
                || scheduled_play_continuation_value(source, baseline, actor, stack_heights)
                    .is_some()
            {
                return None;
            }
            let chop = baseline.chop?;
            let identity = identity_of(source, chop)?;
            if identity.rank != Rank::Five && !super::is_critical(source, identity) {
                return None;
            }
            let protected = value.protects(chop);
            let occupied = value.play_consequences().any(|consequence| {
                consequence.owner == actor
                    && !consequence.identities.is_empty()
                    && consequence
                        .identities
                        .iter()
                        .all(|identity| is_playable_at(stack_heights, identity))
            });
            (protected || occupied).then_some(player_count - distance)
        })
        .sum()
}

/// Whether giving a clue now preserves a more valuable sequence of already
/// scheduled plays than taking the giver's own play and passing the clue to a
/// teammate before the target's turn.
///
/// This is an action-ordering comparison, not hidden-card speculation. Every
/// intervening player must already have a certain play. The clue wins only
/// when even the least valuable displaced play unlocks a longer chain of
/// visible successors than the giver's current play.
fn preserves_visible_continuation(
    source: &PlayerView,
    candidate: CompiledClueAction,
    baselines: &[ProjectedLineState],
) -> bool {
    let Action::Clue { target, clue } = candidate.action else {
        return false;
    };
    if !candidate.immediate_play() {
        return false;
    }
    let useful_touched = source.hands[target.index()]
        .iter()
        .filter_map(|card| card.identity)
        .filter(|identity| clue.matches(*identity))
        .filter(|identity| is_eventually_useful(source, *identity))
        .count();
    if useful_touched < 2 {
        // A one-for-one direct play clue merely substitutes one play for
        // another; it does not create enough additional team progress to
        // preempt the giver's existing play. The scheduling exception is for
        // multi-card setups such as a 4 clue that both plays purple 4 and
        // protects yellow 4.
        return false;
    }
    let player_count = source.hands.len();
    let target_distance =
        (target.index() + player_count - source.current_player.index()) % player_count;
    if target_distance <= 1 {
        return false;
    }
    let mut projected_heights = std::array::from_fn(|suit| {
        u8::try_from(source.play_stacks[suit].len())
            .expect("a standard stack has at most five cards")
    });
    let Some((giver_value, _)) = scheduled_play_continuation_value(
        source,
        &baselines[source.current_player.index()],
        source.current_player,
        projected_heights,
    ) else {
        return false;
    };
    let mut least_intervening_value = usize::MAX;
    for distance in 1..target_distance {
        let player = PlayerId::new(
            u8::try_from((source.current_player.index() + distance) % player_count)
                .expect("standard Hanabi has at most five players"),
        );
        let Some((value, identity)) = scheduled_play_continuation_value(
            source,
            &baselines[player.index()],
            player,
            projected_heights,
        ) else {
            // An unoccupied teammate can give the clue later without delaying
            // a promised play, so giving it now has no scheduling advantage.
            return false;
        };
        least_intervening_value = least_intervening_value.min(value);
        projected_heights[identity.suit.index()] += 1;
    }
    // If the target already has a play when their turn arrives, the clue has
    // no this-rotation deadline. It can wait until a later orbit without
    // displacing any of the intervening plays. This is what distinguishes a
    // genuinely urgent setup clue from merely offering the target a second
    // playable card.
    if scheduled_play_continuation_value(
        source,
        &baselines[target.index()],
        target,
        projected_heights,
    )
    .is_some()
    {
        return false;
    }
    least_intervening_value > giver_value
}

fn scheduled_play_continuation_value(
    source: &PlayerView,
    state: &ProjectedLineState,
    player: PlayerId,
    stack_heights: [u8; 5],
) -> Option<(usize, Card)> {
    let mut commitments = state.closed_public_commitments(source);
    // A target can already be occupied by an exact owner-relative promise
    // even while that promise is waiting for an intervening connector. The
    // public-closure helper intentionally excludes such not-yet-playable
    // promises, but the scheduling projection below advances the stacks and
    // must then recognize them.
    commitments.extend(
        state
            .owner_promises
            .iter()
            .filter(|(_, identities)| identities.len() == 1)
            .map(|(card, identities)| (*card, identities.iter().next().expect("singleton"))),
    );
    commitments.extend(state.playable_now.iter().copied().filter_map(|card| {
        identity_of(source, card)
            .or_else(|| state.epistemic.belief(card)?.known_identity())
            .map(|identity| (card, identity))
    }));
    commitments.sort_unstable_by_key(|(card, identity)| (card.index(), identity.index()));
    commitments.dedup();
    commitments
        .into_iter()
        .filter(|(card, _)| card_owner(source, *card) == Some(player))
        .filter(|(_, identity)| is_playable_at(stack_heights, *identity))
        .map(|(_, identity)| (visible_successor_depth(source, identity), identity))
        .max_by_key(|(value, _)| *value)
}

fn visible_successor_depth(source: &PlayerView, identity: Card) -> usize {
    let mut depth = 0;
    let mut rank = identity.rank.number();
    while rank < Rank::Five.number() {
        rank += 1;
        let successor = Card::new(identity.suit, Rank::ALL[usize::from(rank - 1)]);
        if !source
            .hands
            .iter()
            .flatten()
            .any(|card| card.identity == Some(successor))
        {
            break;
        }
        depth += 1;
    }
    depth
}

fn clue_establishes_actor_recognized_action(
    source: &PlayerView,
    profile: HGroupProfile,
    action: Action,
) -> Option<bool> {
    let Action::Clue { target, clue } = action else {
        return Some(false);
    };
    let touched = source.hands[target.index()]
        .iter()
        .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let compiled = compiled_prospective_clue(source, profile, target, clue, &touched)?;
    let baseline_team = compiled_baseline_team(source, profile);
    for player in 0..source.hands.len() {
        let observer =
            PlayerId::new(u8::try_from(player).expect("standard Hanabi has at most five players"));
        let baseline = baseline_team.projection(observer)?;
        let projected = compiled.projection(observer)?;
        let gained_play = projected
            .inferred
            .playable_now
            .iter()
            .any(|card| !baseline.inferred.playable_now.contains(card));
        let gained_connection = projected.inferred.connection.is_some_and(|connection| {
            baseline
                .inferred
                .connection
                .is_none_or(|prior| prior != connection)
        });
        let gained_connection_promise = projected
            .inferred
            .connection_promises
            .iter()
            .any(|promise| !baseline.inferred.connection_promises.contains(promise));
        if gained_play || gained_connection || gained_connection_promise {
            return Some(true);
        }
    }
    Some(false)
}

#[derive(Clone)]
struct ProjectedLineState {
    giver_visible_commitments: Vec<(CardId, Card)>,
    giver_visible_promises: Vec<(CardId, Card)>,
    epistemic: EpistemicState,
    owner_promises: Vec<(CardId, IdentitySet)>,
    owner_clued_superpositions: Vec<(CardId, IdentitySet)>,
    connection: Option<HGroupConnection>,
    connection_lines: Vec<(PlayerId, CardId, Card, Vec<CardId>)>,
    playable_now: Vec<CardId>,
    chop: Option<CardId>,
    chop_moved: super::CardSet,
    causal_cards: super::CardSet,
}

impl ProjectedLineState {
    /// Team coverage is evaluated by the clue giver, who may legally use the
    /// visible identities in teammates' hands. This projection is kept
    /// separate from owner knowledge so it can never establish Clarity equivalence.
    fn closed_public_commitments(&self, source: &PlayerView) -> Vec<(CardId, Card)> {
        let mut closed = self.giver_visible_commitments.clone();
        loop {
            let mut changed = false;
            for (card, identity) in &self.giver_visible_promises {
                if closed.iter().any(|(known, _)| known == card) {
                    continue;
                }
                let stack_height = source.play_stacks[identity.suit.index()].len();
                let lower_promises_are_secured = Rank::ALL.iter().copied().all(|rank| {
                    let number = usize::from(rank.number());
                    number <= stack_height
                        || number >= usize::from(identity.rank.number())
                        || closed.iter().any(|(_, secured)| {
                            secured.suit == identity.suit && secured.rank == rank
                        })
                });
                if lower_promises_are_secured {
                    closed.push((*card, *identity));
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        closed.sort_unstable_by_key(|(card, identity)| (card.index(), identity.index()));
        closed.dedup();
        closed
    }

    /// Owner-relative counterpart used only to decide whether two clues have
    /// identical outcomes for the Clarity Principle. Team coverage retains
    /// the established public projection above; equivalence is stricter and
    /// must not use identities visible only to another player.
    fn closed_owner_commitments(&self, source: &PlayerView) -> Vec<(CardId, Card)> {
        let mut closed = self
            .epistemic
            .own_beliefs()
            .filter_map(|belief| {
                belief
                    .known_identity()
                    .filter(|identity| is_eventually_useful(source, *identity))
                    .map(|identity| (belief.card, identity))
            })
            .collect::<Vec<_>>();
        loop {
            let mut changed = false;
            for (card, identities) in &self.owner_promises {
                if closed.iter().any(|(known, _)| known == card) {
                    continue;
                }
                // Good Touch excludes identities already committed to other
                // useful cards, but it does not reveal which of several
                // remaining future identities this card is. In particular, a
                // purple card that could be purple 4 or purple 5 does not
                // become a promised purple 4 merely because purple 2 and 3
                // are scheduled to play.
                let claimed = closed
                    .iter()
                    .fold(IdentitySet::default(), |set, (_, identity)| {
                        set.union(IdentitySet::singleton(*identity))
                    });
                let remaining = identities.without(claimed);
                let Some(identity) = (remaining.len() == 1)
                    .then(|| remaining.iter().next())
                    .flatten()
                else {
                    continue;
                };
                let stack_height = source.play_stacks[identity.suit.index()].len();
                let lower_promises_are_secured = Rank::ALL.iter().copied().all(|rank| {
                    let number = usize::from(rank.number());
                    number <= stack_height
                        || number >= usize::from(identity.rank.number())
                        || closed.iter().any(|(_, secured)| {
                            secured.suit == identity.suit && secured.rank == rank
                        })
                });
                if lower_promises_are_secured {
                    closed.push((*card, identity));
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        closed.sort_unstable_by_key(|(card, identity)| (card.index(), identity.index()));
        closed.dedup();
        closed
    }
}

#[allow(clippy::too_many_lines)]
fn projected_line_state(
    source: &PlayerView,
    projection: CompiledObserverProjection,
) -> ProjectedLineState {
    let observer = projection.deductions.view().observer;
    let replay = projection.replay;
    let connection_lines = replay
        .pending_connections
        .iter()
        .map(|connection| {
            (
                connection.actor,
                connection.focus,
                connection.expected,
                connection.cards.clone(),
            )
        })
        .collect();
    let promised = replay
        .cards
        .explicitly_clued
        .union(&replay.cards.invisibly_clued)
        .copied()
        .collect::<Vec<_>>();
    let chop_moved = replay.cards.chop_moved.materialized().clone();
    let causal_cards = replay
        .transitions
        .iter()
        .rev()
        .find(|transition| Some(transition.turn) == source.history.last().map(|entry| entry.turn))
        .into_iter()
        .flat_map(|transition| transition.delta.added_cards())
        .collect();
    let inferred = projection.inferred;
    let chop = inferred.chops[observer.index()];
    let playable_now = inferred.playable_now.clone();
    let epistemic = EpistemicState::from_analysis(&projection.deductions, &inferred);
    let mut giver_visible_commitments = inferred
        .cards
        .iter()
        .filter_map(|note| {
            note.identities
                .iter()
                .next()
                .filter(|_| note.identities.len() == 1)
                .filter(|identity| {
                    identity_of(source, note.card).is_none_or(|actual| actual == *identity)
                })
                .filter(|identity| is_eventually_useful(source, *identity))
                .map(|identity| (note.card, identity))
        })
        .collect::<Vec<_>>();
    giver_visible_commitments.extend(inferred.playable_now.iter().filter_map(|card| {
        identity_of(source, *card)
            .filter(|identity| is_playable_now(source, *identity))
            .map(|identity| (*card, identity))
    }));
    giver_visible_commitments
        .sort_unstable_by_key(|(card, identity)| (card.index(), identity.index()));
    giver_visible_commitments.dedup();
    let mut giver_visible_promises = promised
        .iter()
        .copied()
        .filter_map(|card| {
            identity_of(source, card)
                .or_else(|| {
                    inferred
                        .cards
                        .iter()
                        .find(|note| note.card == card && note.identities.len() == 1)
                        .and_then(|note| note.identities.iter().next())
                })
                .filter(|identity| is_eventually_useful(source, *identity))
                // Invisible alternative slots promise the connection's
                // identity, not every visible face in the candidate list.
                // Otherwise a red-1 layer containing a red 4 manufactures
                // an unrelated red-4 play once the lower stack is secured.
                .filter(|identity| {
                    replay.cards.explicitly_clued.contains(&card)
                        || !replay.pending_connections.iter().any(|connection| {
                            replay.pending_connections.is_active(connection)
                                && connection.cards.contains(&card)
                        })
                        || replay.pending_connections.iter().any(|connection| {
                            replay.pending_connections.is_active(connection)
                                && connection.cards.contains(&card)
                                && connection.expected == *identity
                        })
                })
                .map(|identity| (card, identity))
        })
        .collect::<Vec<_>>();
    giver_visible_promises
        .sort_unstable_by_key(|(card, identity)| (card.index(), identity.index()));
    giver_visible_promises.dedup();
    let owner_clued_superpositions =
        collect_owner_clued_superpositions(source, observer, &epistemic, &promised);
    let mut owner_promises = promised
        .into_iter()
        .filter_map(|card| {
            if card_owner(source, card) != Some(observer) {
                return None;
            }
            epistemic
                .belief(card)
                .map(|belief| belief.identities)
                .map(|identities| {
                    IdentitySet::from_mask(
                        identities
                            .iter()
                            .filter(|identity| is_eventually_useful(source, *identity))
                            .fold(0, |mask, identity| mask | (1 << identity.index())),
                    )
                })
                .filter(|identities| !identities.is_empty())
                .map(|identities| (card, identities))
        })
        .collect::<Vec<_>>();
    owner_promises.sort_unstable_by_key(|(card, _)| card.index());
    owner_promises.dedup();
    ProjectedLineState {
        giver_visible_commitments,
        giver_visible_promises,
        epistemic,
        owner_promises,
        owner_clued_superpositions,
        connection: inferred.connection,
        connection_lines,
        playable_now,
        chop,
        chop_moved,
        causal_cards,
    }
}

fn collect_owner_clued_superpositions(
    source: &PlayerView,
    observer: PlayerId,
    epistemic: &EpistemicState,
    clued_cards: &[CardId],
) -> Vec<(CardId, IdentitySet)> {
    let mut superpositions = clued_cards
        .iter()
        .copied()
        .filter(|card| card_owner(source, *card) == Some(observer))
        .filter_map(|card| {
            epistemic
                .belief(card)
                .map(|belief| (card, belief.identities))
        })
        .collect::<Vec<_>>();
    superpositions.sort_unstable_by_key(|(card, _)| card.index());
    superpositions.dedup();
    superpositions
}

/// Compile a scheduling alternative with the same owner-relative outcome
/// calculation used for ordinary clue valuation.
pub(super) fn scheduled_clue_outcome(
    source: &PlayerView,
    profile: HGroupProfile,
    candidate: &CompiledClueAction,
) -> Option<LineOutcome> {
    let team = compiled_baseline_team(source, profile);
    let baselines = (0..source.hands.len())
        .map(|player| {
            let observer = PlayerId::new(u8::try_from(player).ok()?);
            Some(projected_line_state(source, team.projection(observer)?))
        })
        .collect::<Option<Vec<_>>>()?;
    clue_line_value(
        source,
        profile,
        candidate.action,
        &baselines,
        candidate.move_kind(),
    )
}

#[allow(clippy::too_many_lines)]
fn clue_line_value(
    source: &PlayerView,
    profile: HGroupProfile,
    action: Action,
    baselines: &[ProjectedLineState],
    canonical_kind: Option<HGroupMoveKind>,
) -> Option<LineOutcome> {
    let Action::Clue { target, clue } = action else {
        return None;
    };
    let touched = source.hands[target.index()]
        .iter()
        .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let compiled = compiled_prospective_clue(source, profile, target, clue, &touched)?;
    let after_clue = compiled.after();
    let mut value = LineOutcome::default();
    let named_line = canonical_named_line_metrics(source, &compiled, canonical_kind);
    let giver_projection = compiled.projection(source.observer)?;
    let ignition_cards = giver_projection
        .replay
        .signals
        .iter()
        .filter(|signal| {
            signal.turn == source.turn
                && matches!(
                    signal.kind,
                    HGroupMoveKind::ReplayDoubleIgnition
                        | HGroupMoveKind::UnnecessaryIgnition
                        | HGroupMoveKind::UnnecessaryMove
                        | HGroupMoveKind::TrashDoubleIgnition
                        | HGroupMoveKind::PokeDoubleIgnition
                        | HGroupMoveKind::BombDoubleIgnition
                        | HGroupMoveKind::BombTripleIgnition
                )
        })
        .flat_map(|signal| signal.cards.iter().copied())
        .collect::<Vec<_>>();
    let charm_focus = giver_projection
        .replay
        .signals
        .has_at_turn(source.turn, HGroupMoveKind::Charm)
        .then(|| {
            giver_projection
                .replay
                .clues
                .iter()
                .rev()
                .find(|clue| clue.turn == source.turn)
                .map(|clue| clue.focus)
        })
        .flatten();
    let mut giver_public_actions = Vec::new();
    let caused_by_clue = |card: CardId, identity: Card| {
        touched.contains(&card)
            || touched
                .iter()
                .chain(&ignition_cards)
                .copied()
                .any(|touched_card| {
                    identity_of(source, touched_card).is_some_and(|touched_identity| {
                        touched_identity.suit == identity.suit
                            && touched_identity.rank.number() < identity.rank.number()
                    })
                })
    };
    let connects_to_clue_focus = |identity: Card| {
        touched.iter().copied().any(|touched_card| {
            identity_of(source, touched_card).is_some_and(|focus_identity| {
                focus_identity.suit == identity.suit
                    && identity.rank.number() < focus_identity.rank.number()
            })
        })
    };
    for (player, baseline) in baselines.iter().enumerate() {
        let observer =
            PlayerId::new(u8::try_from(player).expect("standard Hanabi has at most five players"));
        let projection = compiled.projection(observer)?;
        let conflicts_with_giver = charm_focus.is_none()
            && projection
                .replay
                .signals
                .has_at_turn(source.turn, HGroupMoveKind::Charm);
        let after = projected_line_state(after_clue, projection);
        record_clued_superpositions(&mut value, observer, &after);
        let changed_connection_cards = after
            .connection_lines
            .iter()
            .flat_map(|(actor, focus, expected, cards)| {
                let prior = baseline.connection_lines.iter().find(
                    |(old_actor, old_focus, old_expected, _)| {
                        old_actor == actor && old_focus == focus && old_expected == expected
                    },
                );
                cards.iter().copied().filter(move |card| {
                    prior.is_none_or(|(_, _, _, old_cards)| !old_cards.contains(card))
                })
            })
            .collect::<super::CardSet>();
        let commitment_caused = |card: CardId, identity: Card| {
            caused_by_clue(card, identity)
                || after.causal_cards.contains(&card)
                || changed_connection_cards.contains(&card)
        };
        let baseline_public_commitments = baseline.closed_public_commitments(source);
        if observer == target {
            giver_public_actions.extend(
                after
                    .closed_public_commitments(source)
                    .iter()
                    .copied()
                    .filter(|commitment| !baseline_public_commitments.contains(commitment))
                    .filter(|(card, identity)| commitment_caused(*card, *identity))
                    .filter_map(|(card, identity)| {
                        card_owner(source, card)
                            .map(|owner| ActionCommitment::exact(card, owner, identity))
                    }),
            );
            giver_public_actions.extend(changed_connection_cards.iter().copied().filter_map(
                |card| {
                    identity_of(source, card)
                        .filter(|identity| is_eventually_useful(source, *identity))
                        .filter(|identity| {
                            !baseline_public_commitments.contains(&(card, *identity))
                        })
                        .and_then(|identity| {
                            card_owner(source, card)
                                .map(|owner| ActionCommitment::exact(card, owner, identity))
                        })
                },
            ));
            giver_public_actions.extend(
                after
                    .connection_lines
                    .iter()
                    .flat_map(|(_, _, _, cards)| cards.iter().copied())
                    .filter_map(|card| {
                        identity_of(source, card)
                            .filter(|identity| {
                                caused_by_clue(card, *identity)
                                    || connects_to_clue_focus(*identity)
                                    || changed_connection_cards.contains(&card)
                            })
                            .filter(|identity| {
                                !baseline_public_commitments.contains(&(card, *identity))
                            })
                            .and_then(|identity| {
                                card_owner(source, card)
                                    .map(|owner| ActionCommitment::exact(card, owner, identity))
                            })
                    }),
            );
        }
        if !conflicts_with_giver {
            value.public_actions.extend(
                after
                    .closed_public_commitments(source)
                    .iter()
                    .copied()
                    .filter(|commitment| !baseline_public_commitments.contains(commitment))
                    .filter(|(card, identity)| commitment_caused(*card, *identity))
                    .filter_map(|(card, identity)| {
                        card_owner(source, card)
                            .map(|owner| ActionCommitment::exact(card, owner, identity))
                    }),
            );
        }
        let baseline_owner_commitments = baseline.closed_owner_commitments(source);
        let new_actions = after
            .closed_owner_commitments(source)
            .iter()
            .copied()
            .filter(|commitment| !baseline_owner_commitments.contains(commitment))
            .filter(|(card, identity)| commitment_caused(*card, *identity))
            .filter_map(|(card, identity)| {
                card_owner(source, card).map(|owner| ActionCommitment::exact(card, owner, identity))
            })
            .collect::<Vec<_>>();
        value.owner_actions.extend(new_actions);
        value
            .protected_cards
            .extend(
                after
                    .owner_promises
                    .iter()
                    .filter_map(|(card, identities)| {
                        (!baseline.owner_promises.iter().any(|(old, _)| old == card)
                            // A connection can list alternative hidden slots.
                            // The giver must not count every visible face in
                            // that list as secured by the expected identity.
                            && (touched.contains(card)
                                || after.chop_moved.contains(card)
                                || !changed_connection_cards.contains(card)
                                || identity_of(source, *card).is_none_or(|actual| {
                                    after.connection_lines.iter().any(|(_, _, expected, cards)| {
                                        *expected == actual && cards.contains(card)
                                    })
                                }))
                            && identities
                                .iter()
                                .any(|identity| commitment_caused(*card, identity)))
                        .then_some(*card)
                    }),
            );
        value
            .known_trash
            .extend(after.epistemic.own_beliefs().filter_map(|belief| {
                if !touched.contains(&belief.card) {
                    return None;
                }
                belief
                    .known_identity()
                    .filter(|identity| card_is_trash(source, *identity))
                    .and_then(|_| {
                        baseline
                            .epistemic
                            .belief(belief.card)
                            .is_none_or(|prior| prior.known_identity().is_none())
                            .then_some(belief.card)
                    })
            }));
        if let Some(connection) = (!conflicts_with_giver)
            .then_some(after.connection)
            .flatten()
            .filter(|connection| {
                baseline
                    .connection
                    .is_none_or(|prior| prior.card != connection.card)
                    && !baseline_public_commitments.iter().any(|(card, identity)| {
                        *card == connection.card && *identity == connection.identity
                    })
            })
        {
            record_new_connection(&mut value, source, connection);
        }
    }
    giver_public_actions.extend(ignition_cards.into_iter().filter_map(|card| {
        identity_of(source, card)
            .and_then(|identity| card_owner(source, card).map(|owner| (card, owner, identity)))
            .map(|(card, owner, identity)| ActionCommitment::exact(card, owner, identity))
    }));
    if let Some(focus) = charm_focus {
        // A Charm immediately schedules the Fourth-Finesse-Position card.
        // Its untouched 4 remains a valuable long-term promise, but is not a
        // deterministic continuation until the intervening 1, 2, and 3 are
        // secured. Keep it in owner knowledge/protection without inflating
        // immediate team-action coverage.
        giver_public_actions.retain(|commitment| commitment.card != focus);
        value
            .public_actions
            .retain(|commitment| commitment.card != focus);
    }
    // An observer's alternative Finesse reading is not an additional action
    // in the canonical Bluff/Clandestine line. Keep only that line's cards,
    // and never count two different identities on the same visible card.
    // https://hanabi.github.io/level-11/#mistaking-a-layered-finesse-for-a-bluff
    let canonical_cards = named_line.as_ref().and_then(|(_, _, cards)| cards.as_ref());
    let consistent = |commitment: &ActionCommitment| {
        canonical_cards.is_none_or(|cards| {
            cards.contains(&commitment.card)
                && identity_of(source, commitment.card)
                    .is_none_or(|actual| commitment.identities.contains(actual))
        })
    };
    giver_public_actions.retain(consistent);
    value.public_actions.retain(consistent);
    value.owner_actions.retain(consistent);
    giver_public_actions
        .sort_unstable_by_key(|commitment| (commitment.card.index(), commitment.owner.index()));
    giver_public_actions.dedup();
    value
        .recipient_consequences
        .extend(value.public_actions.iter().map(|commitment| {
            RecipientCardConsequence {
                card: commitment.card,
                owner: commitment.owner,
                identities: commitment.identities,
                disposition: if !commitment.identities.is_empty()
                    && commitment
                        .identities
                        .iter()
                        .all(|identity| is_playable_now(source, identity))
                {
                    RecipientCardDisposition::PlayNow
                } else {
                    RecipientCardDisposition::PlayAfterConnection
                },
            }
        }));
    value
        .recipient_consequences
        .extend(value.known_trash.iter().filter_map(|card| {
            card_owner(source, *card).map(|owner| RecipientCardConsequence {
                card: *card,
                owner,
                identities: IdentitySet::default(),
                disposition: RecipientCardDisposition::KnownTrash,
            })
        }));
    value
        .recipient_consequences
        .extend(value.protected_cards.iter().filter_map(|card| {
            card_owner(source, *card).map(|owner| RecipientCardConsequence {
                card: *card,
                owner,
                identities: IdentitySet::default(),
                disposition: RecipientCardDisposition::Protected,
            })
        }));
    value.action_coverage = giver_public_actions.len();
    if let Some((action_count, connection_steps, _)) = named_line {
        value.convention_action_count = Some(if canonical_kind == Some(HGroupMoveKind::PlayClue) {
            // A normal Play line earns only its newly secured cards, not
            // older scheduled predecessors or every alternative blind slot.
            // Cap by the named line's size so unrelated downstream benefits
            // do not become extra steps in that convention line.
            let mut secured = value
                .public_actions
                .iter()
                .map(|action| action.card)
                .collect::<Vec<_>>();
            secured.extend(value.protected_cards.iter().copied());
            secured.sort_unstable();
            secured.dedup();
            secured.len().min(action_count)
        } else {
            action_count
        });
        value.convention_connection_steps = Some(connection_steps);
    }
    value.normalize();
    Some(value)
}

/// Returns the action count and blind-play depth of the canonical named line.
///
/// Different observers can retain provisional alternatives for the same
/// clue. In Bluff Seat, a recognized Bluff takes precedence over an apparent
/// Layered Finesse. A Clandestine Finesse, meanwhile, includes every layered
/// blind play plus the clued focus. Keeping this precedence here prevents the
/// outcome comparison from adding mutually exclusive observer projections.
#[allow(clippy::too_many_lines)]
fn canonical_named_line_metrics(
    source: &PlayerView,
    team: &CompiledProspectiveClue,
    canonical_kind: Option<HGroupMoveKind>,
) -> Option<(usize, usize, Option<Vec<CardId>>)> {
    let mut bluff = None;
    let mut clandestine = None;
    let mut bluff_cards = Vec::new();
    let mut clandestine_cards = Vec::new();
    let mut layered = None;
    let mut ejection = None;
    let mut ignition = None;
    for player in 0..source.hands.len() {
        let observer =
            PlayerId::new(u8::try_from(player).expect("standard Hanabi has at most five players"));
        let projection = team.projection(observer)?;
        for signal in projection
            .replay
            .signals
            .iter()
            .filter(|signal| signal.turn == source.turn)
        {
            match signal.kind {
                HGroupMoveKind::Bluff => {
                    if canonical_kind != Some(HGroupMoveKind::Bluff) {
                        continue;
                    }
                    let blind_plays = signal.cards.len().saturating_sub(1);
                    // Efficiency includes the protected focus even for a 3
                    // Bluff. It is not a claim that the 3 can play yet.
                    let mut secured_cards = signal.cards.clone();
                    if let Some(clue) = projection
                        .replay
                        .clues
                        .iter()
                        .find(|clue| clue.turn == source.turn)
                    {
                        secured_cards.extend(clue.new_non_focus.iter().copied().filter(|card| {
                            identity_of(source, *card)
                                .is_some_and(|identity| is_eventually_useful(source, identity))
                        }));
                    }
                    secured_cards.sort_unstable();
                    secured_cards.dedup();
                    bluff = Some((secured_cards.len(), blind_plays));
                    bluff_cards.clone_from(&signal.cards);
                    if bluff_cards.last().is_some_and(|card| {
                        identity_of(source, *card).is_none_or(|identity| {
                            view_distance_from_playable(source, identity) > 1
                        })
                    }) {
                        bluff_cards.pop();
                    }
                }
                HGroupMoveKind::ClandestineFinesse => {
                    // An alternative observer's Clandestine reading cannot
                    // replace the admitted ordinary line's connector cards.
                    // Ordinary candidate classification can include a
                    // Clandestine line, but it must be the giver's reading.
                    if observer != source.observer {
                        continue;
                    }
                    clandestine = Some((signal.cards.len() + 1, signal.cards.len()));
                    clandestine_cards.clone_from(&signal.cards);
                    if let Some(clue) = projection
                        .replay
                        .clues
                        .iter()
                        .find(|clue| clue.turn == source.turn)
                    {
                        clandestine_cards.push(clue.focus);
                    }
                }
                HGroupMoveKind::LayeredFinesse
                | HGroupMoveKind::HiddenFinesse
                | HGroupMoveKind::QueuedFinesse
                | HGroupMoveKind::AmbiguousFinesse => {
                    layered = Some((signal.cards.len() + 1, signal.cards.len()));
                }
                HGroupMoveKind::FiveColorEjection
                | HGroupMoveKind::Ejection
                | HGroupMoveKind::OutOfPositionEjection
                | HGroupMoveKind::StackedEjection => {
                    // Focus-only identity annotations are not action signals.
                    // The full signal contains one ejected card followed by
                    // touched cards; those touches are not additional plays.
                    // https://hanabi.github.io/level-16/#ejections
                    if signal.cards.len() < 2 {
                        continue;
                    }
                    let commitments =
                        projected_line_state(source, team.projection(source.observer)?)
                            .closed_public_commitments(source);
                    let focus_is_secured = projection
                        .replay
                        .clues
                        .iter()
                        .find(|clue| clue.turn == source.turn)
                        .and_then(|clue| identity_of(source, clue.focus))
                        .is_some_and(|focus| {
                            is_eventually_useful(source, focus)
                                && (1..focus.rank.number()).all(|rank| {
                                    usize::from(rank)
                                        <= source.play_stacks[focus.suit.index()].len()
                                        || commitments.iter().any(|(_, promised)| {
                                            promised.suit == focus.suit
                                                && promised.rank.number() == rank
                                        })
                                })
                        });
                    ejection = Some((1 + usize::from(focus_is_secured), 1));
                }
                HGroupMoveKind::Charm => {
                    if observer != source.observer {
                        // The blind player necessarily treats their hidden
                        // Fourth Finesse Position as possibly playable. Only
                        // the clue giver can verify that the Charm is safe;
                        // another observer's provisional reading must not
                        // inflate the deterministic team line.
                        continue;
                    }
                    // The signal contains the immediate Fourth-Finesse-
                    // Position blind play and the long-term focused 4. Only
                    // the former is a deterministic continuation now; the 4
                    // still depends on its ordinary intervening stack cards.
                    // Source: https://hanabi.github.io/level-23/#the-4-charm
                    ejection = Some((1, 1));
                }
                HGroupMoveKind::UnnecessaryIgnition => {
                    let pushed = projection
                        .replay
                        .signals
                        .iter()
                        .filter(|other| {
                            other.turn == source.turn
                                && other.kind == HGroupMoveKind::UnnecessaryMove
                        })
                        .map(|other| other.cards.len())
                        .sum::<usize>();
                    ignition = Some((signal.cards.len() + pushed, signal.cards.len() + pushed));
                }
                HGroupMoveKind::ReplayDoubleIgnition
                | HGroupMoveKind::TrashDoubleIgnition
                | HGroupMoveKind::PokeDoubleIgnition
                | HGroupMoveKind::BombDoubleIgnition
                | HGroupMoveKind::BombTripleIgnition => {
                    // Every card named by an Ignition signal is an immediate
                    // blind-play obligation. These remain real line actions
                    // even when the clue giver sees that the physical cards
                    // happen to be playable.
                    ignition = Some((signal.cards.len(), signal.cards.len()));
                }
                _ => {}
            }
        }
    }
    ignition
        .or(ejection)
        .map(|(count, depth)| (count, depth, None))
        .or_else(|| bluff.map(|(count, depth)| (count, depth, Some(bluff_cards))))
        .or_else(|| clandestine.map(|(count, depth)| (count, depth, Some(clandestine_cards))))
        .or_else(|| layered.map(|(count, depth)| (count, depth, None)))
}

fn view_distance_from_playable(source: &PlayerView, identity: Card) -> usize {
    usize::from(identity.rank.number())
        .saturating_sub(source.play_stacks[identity.suit.index()].len() + 1)
}

fn record_clued_superpositions(
    value: &mut LineOutcome,
    observer: PlayerId,
    state: &ProjectedLineState,
) {
    value
        .clued_superpositions
        .extend(
            state
                .owner_clued_superpositions
                .iter()
                .map(|(card, identities)| CluedCardSuperposition {
                    card: *card,
                    owner: observer,
                    identities: *identities,
                }),
        );
}

fn record_new_connection(
    value: &mut LineOutcome,
    source: &PlayerView,
    connection: HGroupConnection,
) {
    if let Some(owner) = card_owner(source, connection.card) {
        let commitment = ActionCommitment::exact(connection.card, owner, connection.identity);
        value.public_actions.push(commitment);
        value.owner_actions.push(commitment);
    }
    value.new_connections += 1;
}

fn card_owner(source: &PlayerView, card: CardId) -> Option<PlayerId> {
    source
        .hands
        .iter()
        .position(|hand| hand.iter().any(|candidate| candidate.id == card))
        .and_then(|index| u8::try_from(index).ok())
        .map(PlayerId::new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hanabi_core::{Clue, Suit};
    use hanabi_protocol::HanabiLiveReplay;

    #[test]
    fn reviewed_red_finesse_adds_two_plays_not_one() {
        // User-reviewed p4v0s2 turn 5: red to Cathy secures r3 + r4;
        // red to Donald secures only r3. Older red-1 alternative slots
        // must not manufacture a separate red-4 promise.
        let replay = HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s2.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(4).unwrap();
        let source = state.view_for(state.current_player()).unwrap();
        let team = compiled_baseline_team(&source, HGroupProfile::Max);
        let baselines = (0..4)
            .map(|player| {
                projected_line_state(&source, team.projection(PlayerId::new(player)).unwrap())
            })
            .collect::<Vec<_>>();
        let outcomes = [2, 3].map(|target| {
            clue_line_value(
                &source,
                HGroupProfile::Max,
                Action::Clue {
                    target: PlayerId::new(target),
                    clue: Clue::Suit(Suit::Red),
                },
                &baselines,
                Some(HGroupMoveKind::PlayClue),
            )
            .unwrap()
        });
        let cards = outcomes.each_ref().map(|outcome| {
            outcome
                .public_actions
                .iter()
                .map(|action| action.card)
                .collect::<Vec<_>>()
        });
        assert_eq!(cards[0], vec![CardId::new(8), CardId::new(17)]);
        assert_eq!(cards[1], vec![CardId::new(17)]);
        assert_eq!(outcomes[0].convention_action_count, Some(2));
        let deductions = LogicalDeductions::new(source).unwrap();
        let candidates = super::super::h_group_clue_candidates(&deductions, HGroupProfile::Max);
        assert_eq!(
            candidates
                .iter()
                .max_by_key(|candidate| candidate.value.total())
                .unwrap()
                .action,
            Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Suit(Suit::Red)
            }
        );
    }

    #[test]
    fn reviewed_yellow_five_protection_is_independent_of_clue_label() {
        // User-reviewed p4v0s3 turn 5: both clues protect y5 #8, but
        // yellow gives Cathy the exact identity and its future play.
        let replay = HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s3.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(4).unwrap();
        let source = state.view_for(state.current_player()).unwrap();
        let team = compiled_baseline_team(&source, HGroupProfile::Max);
        let baselines = (0..4)
            .map(|player| {
                projected_line_state(&source, team.projection(PlayerId::new(player)).unwrap())
            })
            .collect::<Vec<_>>();
        let outcomes = [Clue::Suit(Suit::Yellow), Clue::Rank(Rank::Five)].map(|clue| {
            clue_line_value(
                &source,
                HGroupProfile::Max,
                Action::Clue {
                    target: PlayerId::new(2),
                    clue,
                },
                &baselines,
                Some(HGroupMoveKind::PlayClue),
            )
            .unwrap()
        });
        assert_eq!(outcomes[0].protected_cards, vec![CardId::new(8)]);
        assert_eq!(outcomes[0].protected_cards, outcomes[1].protected_cards);
        assert_eq!(
            bottom_deck_risk_protection(&source, &outcomes[0]),
            bottom_deck_risk_protection(&source, &outcomes[1])
        );
        assert!(outcomes[0].strictly_improves_owner_knowledge(&outcomes[1]));
        assert!(!outcomes[1].strictly_improves_owner_knowledge(&outcomes[0]));
        // Invariant: losing a protected card cannot qualify as refinement.
        let mut loses_protection = outcomes[0].clone();
        loses_protection.protected_cards.clear();
        assert!(!loses_protection.strictly_improves_owner_knowledge(&outcomes[1]));
        let deductions = LogicalDeductions::new(source.clone()).unwrap();
        let available = super::super::h_group_clue_candidates(&deductions, HGroupProfile::Max);
        let mut pair = [Clue::Suit(Suit::Yellow), Clue::Rank(Rank::Five)].map(|clue| {
            *available
                .iter()
                .find(|candidate| {
                    candidate.action
                        == Action::Clue {
                            target: PlayerId::new(2),
                            clue,
                        }
                })
                .unwrap()
        });
        pair[0].value = super::super::ClueValue::new(383);
        pair[1].value = super::super::ClueValue::new(400);
        let mut with_third = pair.to_vec();
        let mut third = *available
            .iter()
            .find(|candidate| {
                candidate.action
                    == Action::Clue {
                        target: PlayerId::new(3),
                        clue: Clue::Rank(Rank::Two),
                    }
            })
            .unwrap();
        third.value = super::super::ClueValue::new(400);
        with_third.push(third);
        apply_strategic_clue_values(&deductions, HGroupProfile::Max, &mut pair);
        apply_strategic_clue_values(&deductions, HGroupProfile::Max, &mut with_third);
        assert!(pair[0].score() > pair[1].score());
        assert!(with_third[0].score() > with_third[1].score());
        // Adding a protection option must not lower unrelated clues' scores
        // against actions outside this clue-only comparison (e.g. discard).
        assert_eq!(pair[0].score(), with_third[0].score());
        assert_eq!(pair[1].score(), with_third[1].score());
    }

    #[test]
    fn reviewed_opening_preserves_the_three_bluff_before_a_draw() {
        // User-reviewed p4v0s1 turn 1: Bob's p1 position enables the 3
        // Bluff saving p3/r3. Blue plays b1 and draws over that position;
        // playing p1 first leaves both blue cards available for a later clue.
        let replay = HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s1.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(0).unwrap();
        let mut source = state.view_for(state.current_player()).unwrap();
        let deductions = LogicalDeductions::new(source.clone()).unwrap();
        let candidates = super::super::h_group_clue_candidates(&deductions, HGroupProfile::Max);
        let losses = positional_opportunity_losses(&source, HGroupProfile::Max, &candidates);
        let blue = candidates
            .iter()
            .position(|candidate| {
                candidate.action
                    == Action::Clue {
                        target: PlayerId::new(1),
                        clue: Clue::Suit(Suit::Blue),
                    }
            })
            .unwrap();
        let bluff = candidates
            .iter()
            .position(|candidate| {
                candidate.action
                    == Action::Clue {
                        target: PlayerId::new(3),
                        clue: Clue::Rank(Rank::Three),
                    }
            })
            .unwrap();
        assert!(losses[blue]);
        assert!(!losses[bluff]);
        let team = compiled_baseline_team(&source, HGroupProfile::Max);
        let baselines = (0..4)
            .map(|player| {
                projected_line_state(&source, team.projection(PlayerId::new(player)).unwrap())
            })
            .collect::<Vec<_>>();
        let outcome = clue_line_value(
            &source,
            HGroupProfile::Max,
            candidates[bluff].action,
            &baselines,
            candidates[bluff].move_kind(),
        )
        .unwrap();
        assert_eq!(bottom_deck_risk_protection(&source, &outcome), 2);
        // Algorithm-only counterfactual: without a draw, removing an older
        // card does not replace the first finesse position.
        source.deck_size = 0;
        assert!(
            positional_opportunity_losses(&source, HGroupProfile::Max, &candidates)
                .iter()
                .all(|loss| !loss)
        );
    }

    #[test]
    fn reviewed_opening_does_not_merge_bluff_and_clandestine_alternatives() {
        // User-reviewed p4v0s2 turn 2: 3 Bluff is 2-for-1; the Reverse
        // Clandestine Finesse is 3-for-1. Neither invents extra purple plays.
        let replay = HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s2.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(1).unwrap();
        let source = state.view_for(state.current_player()).unwrap();
        let team = compiled_baseline_team(&source, HGroupProfile::Max);
        let baselines = (0..4)
            .map(|player| {
                projected_line_state(&source, team.projection(PlayerId::new(player)).unwrap())
            })
            .collect::<Vec<_>>();
        for (target, rank, count, allowed) in [
            (0, Rank::Three, 2, vec![CardId::new(11)]),
            (
                3,
                Rank::Two,
                3,
                vec![CardId::new(11), CardId::new(10), CardId::new(13)],
            ),
        ] {
            let outcome = clue_line_value(
                &source,
                HGroupProfile::Max,
                Action::Clue {
                    target: PlayerId::new(target),
                    clue: Clue::Rank(rank),
                },
                &baselines,
                Some(if rank == Rank::Three {
                    HGroupMoveKind::Bluff
                } else {
                    HGroupMoveKind::PlayClue
                }),
            )
            .unwrap();
            assert_eq!(outcome.convention_action_count, Some(count));
            let occupied = occupied_by_visible_connection(
                &source,
                &team.projection(source.observer).unwrap().replay,
            );
            assert!(
                occupied.contains(&PlayerId::new(3)),
                "Donald's green-1 obligation occupies him before either clue"
            );
            assert_eq!(
                secured_critical_chop_deadline_value(
                    &source, &baselines, &outcome, [0; 5], &occupied
                ),
                0,
                "neither clue has to rescue Donald's blue-5 chop on this turn"
            );
            assert!(!outcome.public_actions.is_empty());
            for action in &outcome.public_actions {
                assert!(allowed.contains(&action.card), "{outcome:#?}");
                assert_eq!(
                    action.identities,
                    IdentitySet::singleton(identity_of(&source, action.card).unwrap()),
                    "{outcome:#?}"
                );
            }
        }
    }

    #[test]
    fn reviewed_turn_ten_credits_direct_and_indirect_purple_three_protection() {
        // User-reviewed p4v0s9 turn 10: both alternatives protect Donald's
        // p3 (#17), although the rank-4 clue only physically touches Cathy.
        let replay = HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s9.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(9).unwrap();
        let source = state.view_for(state.current_player()).unwrap();
        let team = compiled_baseline_team(&source, HGroupProfile::Max);
        let baselines = (0..4)
            .map(|player| {
                projected_line_state(&source, team.projection(PlayerId::new(player)).unwrap())
            })
            .collect::<Vec<_>>();
        for action in [
            Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Rank(Rank::Four),
            },
            Action::Clue {
                target: PlayerId::new(3),
                clue: Clue::Suit(Suit::Purple),
            },
        ] {
            let outcome = clue_line_value(
                &source,
                HGroupProfile::Max,
                action,
                &baselines,
                Some(HGroupMoveKind::PlayClue),
            )
            .unwrap();
            assert!(
                outcome.protected_cards.contains(&CardId::new(17)),
                "{action:?}: {outcome:#?}"
            );
            assert!(!outcome.protected_cards.contains(&CardId::new(14)));
            assert!(!outcome.protected_cards.contains(&CardId::new(15)));
            assert_eq!(
                bottom_deck_risk_protection(&source, &outcome),
                1,
                "{action:?}: {:?}",
                outcome
                    .protected_cards
                    .iter()
                    .map(|card| (*card, identity_of(&source, *card)))
                    .collect::<Vec<_>>()
            );
        }
    }
}
