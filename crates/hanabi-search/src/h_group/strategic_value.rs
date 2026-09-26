use super::clue_outcome::clue_line_value;
use super::line_state::{ProjectedLineState, card_owner, projected_line_state};
use super::{
    Action, Card, CardId, ClueProposal, CluePurpose, HGroupMoveKind, HGroupProfile, HGroupRuleId,
    IdentitySet, LineOutcome, LogicalDeductions, PlayerId, PlayerView, Rank,
    RecipientCardDisposition, compiled_baseline_team, compiled_prospective_clue, identity_of,
    is_eventually_useful, is_playable_at, is_playable_now, rule_enabled,
};

const TEAM_ACTION_COVERAGE_PENALTY: u16 = 80;
const TEAM_ACTION_COUNT_PENALTY: u16 = 100;
const CLUE_EFFICIENCY_DEFICIT_PENALTY: u16 = 80;
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
    candidates: &mut [ClueProposal],
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
                .map(|projection| projected_line_state(source, &projection))
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
    let best_connection_action_count = values
        .iter()
        .filter_map(|value| value.as_ref()?.convention_action_count)
        .max();
    let best_observed_efficiency = values
        .iter()
        .flatten()
        .filter(|value| value.convention_action_count.is_some())
        .map(|value| value.clue_efficiency)
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
            value.as_ref().map_or(0, |value| {
                bottom_deck_risk_protection(source, &baselines, value)
            })
        })
        .collect::<Vec<_>>();
    let has_unoccupied_immediate_target = candidates.iter().any(|candidate| {
        candidate.immediate_play() && !target_has_scheduled_play(candidate.target())
    });
    let mut best_immediate_action_count_by_target: Vec<Option<usize>> =
        vec![None; source.hands.len()];
    for (candidate, value) in candidates.iter().zip(&values) {
        let Some(value) = value else {
            continue;
        };
        if !candidate.immediate_play()
            && !occupies_next_turn(
                source,
                &baselines,
                value,
                candidate.target(),
                current_stack_heights,
            )
        {
            continue;
        }
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
        let positional = super::positional_value::evaluate(deductions, profile, *candidate);
        candidate.value.penalize_opportunity(
            POSITIONAL_OPPORTUNITY_LOSS_PENALTY * u16::from(positional.foregone_blind_plays),
        );
        candidate.value.reward_teamwork(
            POSITIONAL_OPPORTUNITY_LOSS_PENALTY * u16::from(positional.conditional_prompt_chains),
        );
        candidate.set_compiled_line(value);
        candidate.set_expiring_multi_card_opportunity(expiring_multi_card_opportunity(
            source, profile, *candidate, value, &baselines,
        ));
        if opportunity_losses[index] {
            candidate
                .value
                .penalize_opportunity(POSITIONAL_OPPORTUNITY_LOSS_PENALTY);
        }
        // Credit actual protection, independently of the other candidates.
        // A deficit from the best clue unfairly penalizes productive clues
        // against ordinary discards, which are outside this clue-only pass
        // and protect none of these identities either.
        // Net protection, not a reward for merely moving the chopping block.
        // The endpoint guards unsafe dominance claims; this soft preference
        // also applies when neither endpoint dominates the other.
        // Reviewed p4v0s1 turn 7: protect p4, expose the needed r3.
        if value.worsened_chop_exposure {
            candidate
                .value
                .penalize_teamwork(BOTTOM_DECK_RISK_PROTECTION_BONUS);
        } else {
            candidate.value.reward_protection(
                BOTTOM_DECK_RISK_PROTECTION_BONUS.saturating_mul(
                    u16::try_from(bottom_deck_risk_values[index]).unwrap_or(u16::MAX),
                ),
            );
        }
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
        if let Some(best) = best_connection_action_count {
            // Connection metrics can include unresolved blind layers. A
            // direct clue has no corresponding layer forecast, so compare
            // its observed gains with the same observed gains on other
            // lines, not with the length of their alternative-slot lists.
            let (best, actual) = value.convention_action_count.map_or_else(
                || (best_observed_efficiency.unwrap_or(0), value.clue_efficiency),
                |actual| (best, actual),
            );
            // Compare cards obtained per clue separately from the downstream
            // plays that those cards make available. This remains observer-
            // relative and does not award a bonus for a convention's name.
            candidate
                .value
                .penalize_teamwork(CLUE_EFFICIENCY_DEFICIT_PENALTY.saturating_mul(
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
fn bottom_deck_risk_protection(
    source: &PlayerView,
    baselines: &[ProjectedLineState],
    value: &LineOutcome,
) -> usize {
    let heights = std::array::from_fn(|suit| {
        u8::try_from(source.play_stacks[suit].len()).expect("standard stack")
    });
    value
        .protected_cards
        .iter()
        .copied()
        // A Play Clue can protect the same threatened chop by occupying its
        // owner. Count that protection just as for a direct Save; otherwise
        // a Save gets an artificial bonus over the productive alternative.
        .chain(
            baselines
                .iter()
                .enumerate()
                .filter_map(|(player, baseline)| {
                    let owner = PlayerId::new(u8::try_from(player).expect("standard player count"));
                    occupies_next_turn(source, baselines, value, owner, heights)
                        .then_some(baseline.chop)
                        .flatten()
                }),
        )
        .filter(|card| {
            let Some(owner) = card_owner(source, *card) else {
                return false;
            };
            let baseline = &baselines[owner.index()];
            // Protection is not automatically prevention of a loss. Credit
            // only an unoccupied hand's loss exposure here. An already
            // available play leaves time to arrange the clue later; do not
            // charge other productive lines for declining an Early Save.
            // https://hanabi.github.io/beginner/other-general-strategy/#give-play-clues-over-save-clues
            scheduled_play_continuation_value(source, baseline, owner, heights).is_none()
        })
        .filter_map(|card| identity_of(source, card))
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

/// A positional 2-for-1 expires when the next player's already-available
/// play draws a blank over its blind-play anchor. Count newly obtained cards,
/// not automatic plays, and require a real scheduled draw rather than assuming
/// every advanced clue is urgent. Reviewed in p4v0s1, turn 22.
/// <https://hanabi.github.io/level-11/#the-bluff>
fn expiring_multi_card_opportunity(
    source: &PlayerView,
    profile: HGroupProfile,
    candidate: ClueProposal,
    outcome: &LineOutcome,
    baselines: &[ProjectedLineState],
) -> Option<CardId> {
    if source.deck_size == 0 || outcome.clue_efficiency < 2 {
        return None;
    }
    let Action::Clue { target, clue } = candidate.action else {
        return None;
    };
    let touched = source.hands[target.index()]
        .iter()
        .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let evidence = compiled_prospective_clue(source, profile, target, clue, &touched)?
        .line_evidence(source, candidate.move_kind())?;
    let reactor = super::next_player(source.current_player, source.hands.len());
    let anchor = evidence.positional_anchor?;
    (card_owner(source, anchor) == Some(reactor)
        && identity_of(source, anchor).is_some_and(|identity| is_playable_now(source, identity))
        && !baselines[reactor.index()].playable_now.is_empty()
        && !baselines[reactor.index()].playable_now.contains(&anchor))
    .then_some(anchor)
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
    candidates: &[ClueProposal],
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
        let Some(evidence) = compiled.line_evidence(source, alternative.move_kind()) else {
            continue;
        };
        let anchor = evidence.positional_anchor;
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
                        .is_some_and(|identity| super::is_critical_save_identity(source, identity))
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
            if identity.rank != Rank::Five && !super::is_critical_save_identity(source, identity) {
                return None;
            }
            let protected = value.protects(chop);
            let occupied = occupies_next_turn(source, baselines, value, actor, stack_heights);
            (protected || occupied).then_some(player_count - distance)
        })
        .sum()
}

/// A delayed play protects a chop if its already-scheduled predecessor plays
/// before the recipient acts. Testing the stacks at clue time incorrectly
/// treats this as a missed Save deadline. Advance only single, known existing
/// plays, never a guessed hidden card or an optional choice between plays.
fn occupies_next_turn(
    source: &PlayerView,
    baselines: &[ProjectedLineState],
    value: &LineOutcome,
    target: PlayerId,
    mut heights: [u8; 5],
) -> bool {
    for offset in 1..source.hands.len() {
        let seat = (source.current_player.index() + offset) % source.hands.len();
        if seat == target.index() {
            break;
        }
        let state = &baselines[seat];
        if let [card] = state.playable_now.as_slice() {
            if let Some(identity) = identity_of(source, *card)
                .or_else(|| state.epistemic.belief(*card)?.known_identity())
                .filter(|identity| is_playable_at(heights, *identity))
            {
                heights[identity.suit.index()] = identity.rank.number();
            }
        }
    }
    value.play_consequences().any(|consequence| {
        consequence.owner == target
            && !consequence.identities.is_empty()
            && consequence
                .identities
                .iter()
                .all(|identity| is_playable_at(heights, identity))
    })
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
    candidate: ClueProposal,
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

#[cfg(test)]
mod tests {
    use super::*;
    use hanabi_core::{Clue, Suit};
    use hanabi_protocol::HanabiLiveReplay;

    #[test]
    fn reviewed_turn_nineteen_counts_new_cards_not_old_connectors() {
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "tests/fixtures/game-p4v0s1-before-turn27-revision.json"
        ))
        .unwrap();
        let root = replay.state_at_turn(18).unwrap();
        let purple = Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Purple),
        };
        let mut alternative = root.clone();
        alternative.apply(purple).unwrap();
        let cases = [
            // User-reviewed turn 33: the blind p3 merely replaces Donald's
            // already secured p3; g4 is also already clued. Zero for one.
            (
                replay.state_at_turn(32).unwrap(),
                Action::Clue {
                    target: PlayerId::new(2),
                    clue: Clue::Suit(Suit::Green),
                },
                HGroupMoveKind::Bluff,
                0,
            ),
            (
                root.clone(),
                Action::Clue {
                    target: PlayerId::new(0),
                    clue: Clue::Rank(Rank::Four),
                },
                HGroupMoveKind::Bluff,
                2,
            ),
            (
                replay.state_at_turn(20).unwrap(),
                Action::Clue {
                    target: PlayerId::new(3),
                    clue: Clue::Suit(Suit::Purple),
                },
                HGroupMoveKind::Bluff,
                2,
            ),
            (root, purple, HGroupMoveKind::PlayClue, 2),
            (
                alternative,
                Action::Clue {
                    target: PlayerId::new(1),
                    clue: Clue::Suit(Suit::Green),
                },
                HGroupMoveKind::PlayClue,
                1,
            ),
        ];
        for (state, action, kind, efficiency) in cases {
            let source = state.view_for(state.current_player()).unwrap();
            let team = compiled_baseline_team(&source, HGroupProfile::Max);
            let baselines = (0..source.hands.len())
                .map(|player| {
                    let observer = PlayerId::new(u8::try_from(player).unwrap());
                    projected_line_state(&source, &team.projection(observer).unwrap())
                })
                .collect::<Vec<_>>();
            let outcome =
                clue_line_value(&source, HGroupProfile::Max, action, &baselines, Some(kind))
                    .unwrap();
            assert_eq!(
                outcome.clue_efficiency, efficiency,
                "{action:?}: {outcome:?}"
            );
        }
    }

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
                projected_line_state(&source, &team.projection(PlayerId::new(player)).unwrap())
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
        assert_eq!(outcomes[0].clue_efficiency, 2);
        let deductions = LogicalDeductions::new(source).unwrap();
        let candidates = super::super::h_group_clue_candidates(&deductions, HGroupProfile::Max)
            .into_iter()
            .map(super::super::candidate_pipeline::CompiledClueAction::proposal)
            .collect::<Vec<_>>();
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
    fn first_seed_delayed_red_five_protects_the_chop_before_its_deadline() {
        // Current reviewed fixture, turn 35: Donald's r4 will play before
        // Alice acts. A red clue gives Alice r5 to play, protecting y4 on chop.
        let replay = HanabiLiveReplay::from_json(include_str!(
            "tests/fixtures/game-p4v0s1-before-turn18-revision.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(34).unwrap();
        let source = state.view_for(state.current_player()).unwrap();
        let team = compiled_baseline_team(&source, HGroupProfile::Max);
        let mut baselines = (0..4)
            .map(|player| {
                projected_line_state(&source, &team.projection(PlayerId::new(player)).unwrap())
            })
            .collect::<Vec<_>>();
        let value = clue_line_value(
            &source,
            HGroupProfile::Max,
            Action::Clue {
                target: PlayerId::new(0),
                clue: Clue::Suit(Suit::Red),
            },
            &baselines,
            Some(HGroupMoveKind::PlayClue),
        )
        .unwrap();
        let heights =
            std::array::from_fn(|suit| u8::try_from(source.play_stacks[suit].len()).unwrap());
        assert!(occupies_next_turn(
            &source,
            &baselines,
            &value,
            PlayerId::new(0),
            heights
        ));
        assert_eq!(
            secured_critical_chop_deadline_value(&source, &baselines, &value, heights, &[]),
            2
        );
        assert_eq!(bottom_deck_risk_protection(&source, &baselines, &value), 1);
        baselines[3].playable_now.clear();
        assert!(!occupies_next_turn(
            &source,
            &baselines,
            &value,
            PlayerId::new(0),
            heights
        ));
        assert_eq!(bottom_deck_risk_protection(&source, &baselines, &value), 0);
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
                projected_line_state(&source, &team.projection(PlayerId::new(player)).unwrap())
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
            bottom_deck_risk_protection(&source, &baselines, &outcomes[0]),
            bottom_deck_risk_protection(&source, &baselines, &outcomes[1])
        );
        assert!(outcomes[0].strictly_improves_owner_knowledge(&outcomes[1]));
        assert!(!outcomes[1].strictly_improves_owner_knowledge(&outcomes[0]));
        // Invariant: losing a protected card cannot qualify as refinement.
        let mut loses_protection = outcomes[0].clone();
        loses_protection.protected_cards.clear();
        assert!(!loses_protection.strictly_improves_owner_knowledge(&outcomes[1]));
        let deductions = LogicalDeductions::new(source.clone()).unwrap();
        let available = super::super::h_group_clue_candidates(&deductions, HGroupProfile::Max)
            .into_iter()
            .map(super::super::candidate_pipeline::CompiledClueAction::proposal)
            .collect::<Vec<_>>();
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
            "tests/fixtures/game-p4v0s1-before-turn18-revision.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(0).unwrap();
        let mut source = state.view_for(state.current_player()).unwrap();
        let deductions = LogicalDeductions::new(source.clone()).unwrap();
        let candidates = super::super::h_group_clue_candidates(&deductions, HGroupProfile::Max)
            .into_iter()
            .map(super::super::candidate_pipeline::CompiledClueAction::proposal)
            .collect::<Vec<_>>();
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
                projected_line_state(&source, &team.projection(PlayerId::new(player)).unwrap())
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
        assert_eq!(outcome.protected_cards.len(), 2);
        assert_eq!(
            bottom_deck_risk_protection(&source, &baselines, &outcome),
            2
        );
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
                projected_line_state(&source, &team.projection(PlayerId::new(player)).unwrap())
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
                projected_line_state(&source, &team.projection(PlayerId::new(player)).unwrap())
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
                bottom_deck_risk_protection(&source, &baselines, &outcome),
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

    #[test]
    fn reviewed_double_bluff_alternative_does_not_rescue_a_safe_red_two() {
        // Human-reviewed p4v0s1 turn 14: Cathy plays, rather than discards;
        // Alice or Bob has time to arrange the red-2 clue afterwards.
        let replay = HanabiLiveReplay::from_json(include_str!(
            "tests/fixtures/game-p4v0s1-before-turn18-revision.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(13).unwrap();
        let source = state.view_for(state.current_player()).unwrap();
        let team = compiled_baseline_team(&source, HGroupProfile::Max);
        let baselines = (0..4)
            .map(|player| {
                projected_line_state(&source, &team.projection(PlayerId::new(player)).unwrap())
            })
            .collect::<Vec<_>>();
        let outcome = clue_line_value(
            &source,
            HGroupProfile::Max,
            Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Suit(Suit::Red),
            },
            &baselines,
            Some(HGroupMoveKind::PlayClue),
        )
        .unwrap();
        assert!(outcome.protected_cards.contains(&CardId::new(18)));
        assert_eq!(
            bottom_deck_risk_protection(&source, &baselines, &outcome),
            0
        );
    }
}
