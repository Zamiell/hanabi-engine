use crate::ConventionPolicyTier;

use super::{
    Action, ActionPreference, ActionSchedule, BeliefConstraints, Card, CardId, CardSet, Clue,
    CluePurpose, ClueSchedule, ClueValue, CompiledClueAction, CompiledHGroupAction,
    ConventionActionReason, ConventionConstraintGraph, ConventionConstraints,
    ConventionRequirementKind, HGroupActionKind, HGroupActionSet, HGroupClueKind, HGroupConnection,
    HGroupConnectionKind, HGroupConnectionPromise, HGroupIdentityStatus, HGroupInferences,
    HGroupMoveKind, HGroupPhase, HGroupPlayObligation, HGroupProfile, HGroupRuleId, HGroupState,
    IdentitySet, LogicalDeductions, MAX_CLUE_TOKENS, ObservedEvent, OnceLock, PerspectiveDepth,
    PerspectiveProjector, PlayerId, PlayerView, ProspectiveTransition, Rank,
    RejectedConventionAction, Suit, TeamConventionSnapshot, TerminalPlanProgress, chop,
    convention_card_inferences, creates_false_anxiety, finesse_position, focus,
    h_group_clue_candidates_from_replay, h_group_phase, h_group_rejected_clues_from_replay,
    identity_of, infer_clue_to_self, is_convention_trash, is_critical, is_eventually_useful,
    is_playable_at, is_playable_now, next_player, ordered_playable_cards,
    owner_knowledge_read_model, projected_h_group_replay, prospective_clue_has_unsafe_connection,
    prospective_clue_marks_focus_saved, prospective_clue_primary_kind, prospective_clue_view,
    prospective_play_has_unsafe_inference, prospective_team_clue_signal_kinds, replay_h_group,
    rule_enabled, was_clued_before,
};

#[derive(Clone, Debug)]
pub(crate) struct HGroupAnalysis {
    replay: HGroupState,
    inferences: HGroupInferences,
    clue_candidates: OnceLock<Vec<CompiledClueAction>>,
    endgame_completion: OnceLock<Option<EndgameCompletionPlan>>,
    action_set: OnceLock<HGroupActionSet>,
}

#[derive(Clone, Debug)]
struct EndgameCompletionPlan {
    unresolved_fives: CardSet,
}

pub(super) fn build_h_group_analysis(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
) -> HGroupAnalysis {
    let replay = replay_h_group(deductions, profile);
    let inferences = infer_h_group_from_replay(deductions, replay.clone(), profile);
    HGroupAnalysis {
        replay,
        inferences,
        clue_candidates: OnceLock::new(),
        endgame_completion: OnceLock::new(),
        action_set: OnceLock::new(),
    }
}

pub(super) fn analysis_clue_candidates<'a>(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    analysis: &'a HGroupAnalysis,
) -> &'a [CompiledClueAction] {
    analysis
        .clue_candidates
        .get_or_init(|| h_group_clue_candidates_from_replay(deductions, profile, &analysis.replay))
        .as_slice()
}

/// Applies the implemented cumulative H-Group semantics to a logical view.
#[must_use]
pub fn infer_h_group(deductions: &LogicalDeductions, profile: HGroupProfile) -> HGroupInferences {
    build_h_group_analysis(deductions, profile).inferences
}

#[allow(clippy::too_many_lines)]
pub(super) fn infer_h_group_from_replay(
    deductions: &LogicalDeductions,
    replay: HGroupState,
    profile: HGroupProfile,
) -> HGroupInferences {
    let view = deductions.view();
    let action_schedule = ActionSchedule::from_replay(view, &replay);
    let blocked_connection_cards = action_schedule.blocked_cards();
    let promptable = replay.promptable();
    let gotten = replay.gotten_from(&promptable);
    let chops = replay
        .hands
        .iter()
        .map(|hand| chop(hand, &gotten))
        .collect::<Vec<_>>();
    let cards = convention_card_inferences(deductions, &replay);
    let fixed_cards = replay.cards.facts.fixed_cards();
    let mut own_required_discards: Vec<CardId> = action_schedule
        .required_discards_for(view.observer)
        .collect();
    own_required_discards.retain(|card| {
        !replay.cards.forced_playable.contains(card)
            && cards
                .iter()
                .find(|knowledge| knowledge.card == *card)
                .is_none_or(|knowledge| knowledge.play_obligation.is_none())
    });
    let mut held_save_collateral = CardSet::default();
    for (index, clue) in replay.clues.iter().enumerate() {
        if !matches!(clue.kind, HGroupClueKind::Save(_)) {
            continue;
        }
        for card in &clue.new_non_focus {
            let later_play = replay.clues[index + 1..]
                .iter()
                .any(|later| later.focus == *card && matches!(later.kind, HGroupClueKind::Play));
            if !later_play {
                held_save_collateral.insert(*card);
            }
        }
    }
    let mut inferred = HGroupInferences {
        clues: replay.clues,
        chops,
        cards,
        early_game: replay.early_game,
        invisibly_clued: replay.cards.invisibly_clued.iter().copied().collect(),
        signals: replay.signals.into_vec(),
        chop_moved: replay.cards.chop_moved.iter().copied().collect(),
        discard_now: own_required_discards,
        must_clue: replay.must_clue.iter().copied().collect(),
        phase: h_group_phase(view, replay.early_game),
        ..HGroupInferences::default()
    };

    inferred.connection_promises = replay
        .pending_connections
        .iter()
        .filter(|pending| {
            pending.actor == view.observer
                && replay.pending_connections.is_active(pending)
                && pending.cards.iter().any(|candidate| {
                    inferred.cards.iter().any(|card| {
                        card.card == *candidate && card.identities.contains(pending.expected)
                    })
                })
        })
        .map(|pending| HGroupConnectionPromise {
            cards: pending.cards.clone(),
            identity: pending.expected,
        })
        .collect();

    for card in &inferred.cards {
        let logically_playable =
            deductions
                .possible_identities(card.card)
                .is_some_and(|identities| {
                    !identities.is_empty()
                        && identities
                            .iter()
                            .all(|identity| is_playable_now(view, identity))
                });
        let fixed_before_identity_became_playable = logically_playable
            && action_schedule.fix_predated_playability(card.card, card.identities);
        if (!fixed_cards.contains(&card.card)
            || replay.cards.forced_playable.contains(&card.card)
            || fixed_before_identity_became_playable)
            && !replay.cards.invalidated_focuses.contains(&card.card)
            && !replay.cards.declined_direct_plays.contains(&card.card)
            && (!blocked_connection_cards.contains(&card.card)
                || replay.cards.forced_playable.contains(&card.card))
            && card.identity_status != HGroupIdentityStatus::Provisional
            && (!held_save_collateral.contains(&card.card) || logically_playable)
            && !card.identities.is_empty()
            && card
                .identities
                .iter()
                .all(|identity| is_playable_now(view, identity))
        {
            inferred.playable_now.push(card.card);
        }
    }

    let stacked_preemption = inferred
        .signals
        .iter()
        .rev()
        .find(|signal| {
            signal.target == Some(view.observer)
                && matches!(
                    signal.kind,
                    HGroupMoveKind::StackedEjection | HGroupMoveKind::StackedDischarge
                )
        })
        .and_then(|signal| signal.cards.first().copied())
        .filter(|card| replay.cards.forced_playable.contains(card));
    if let Some(forced) = stacked_preemption {
        // A Stacked Ejection/Discharge explicitly tells a loaded player to
        // play a different Finesse Position before their existing connector.
        // Keeping both actions due lets ordinary connection priority erase
        // the very precedence communicated by the advanced move.
        inferred.playable_now.retain(|card| *card == forced);
    }

    if let Some(focus) = action_schedule.preferred_rank_focus(&inferred.playable_now) {
        inferred.priority_plays.push(focus);
    }
    if rule_enabled(profile, HGroupRuleId::Stalling)
        && view.clue_tokens == 0
        && inferred.playable_now.is_empty()
        && !replay.pending_connections.iter().any(|connection| {
            connection.actor == view.observer && replay.pending_connections.is_active(connection)
        })
    {
        let own_hand = &replay.hands[view.observer.index()];
        if !own_hand.is_empty() && own_hand.iter().all(|card| gotten.contains(card)) {
            let mut best = None::<(CardId, usize, usize)>;
            for card in own_hand.iter().rev().copied() {
                if replay.cards.invalidated_focuses.contains(&card) {
                    continue;
                }
                let Some(note) = inferred.cards.iter().find(|note| note.card == card) else {
                    continue;
                };
                let total = note.identities.len();
                let playable = note
                    .identities
                    .iter()
                    .filter(|identity| is_playable_now(view, *identity))
                    .count();
                if total == 0 || playable == 0 {
                    continue;
                }
                let playable_identities = IdentitySet::from_mask(
                    note.identities
                        .iter()
                        .filter(|identity| is_playable_now(view, *identity))
                        .fold(0, |mask, identity| mask | (1 << identity.index())),
                );
                if inferred.cards.iter().any(|other| {
                    other.card != card
                        && !other
                            .identities
                            .intersection(playable_identities)
                            .is_empty()
                }) {
                    // Anxiety does not distinguish between two cards that can
                    // represent the same currently playable identity. Picking
                    // one by position would manufacture information that no
                    // clue or convention supplied.
                    continue;
                }
                if best.is_none_or(|(_, best_playable, best_total)| {
                    playable * best_total > best_playable * total
                }) {
                    best = Some((card, playable, total));
                }
            }
            if let Some((card, _, _)) = best {
                inferred.playable_now.push(card);
                if let Some(note) = inferred.cards.iter_mut().find(|note| note.card == card) {
                    note.play_obligation = Some(HGroupPlayObligation::Anxiety);
                }
            }
        }
    }

    let connection = stacked_preemption
        .is_none()
        .then(|| {
            replay
                .pending_connections
                .iter()
                .filter(|pending| {
                    // A later connection can be established while an earlier card in
                    // the same suit is already promised. It becomes actionable only
                    // after that predecessor reaches the stack; otherwise connection
                    // priority would make the successor misplay first.
                    pending.actor == view.observer
                        && replay.pending_connections.is_active(pending)
                        && pending.cards.first().is_none_or(|card| {
                            !replay
                                .cards
                                .facts
                                .is_exact_transfer(*card, pending.expected)
                        })
                        && is_playable_now(view, pending.expected)
                })
                .min_by_key(|pending| match pending.kind {
                    HGroupConnectionKind::Prompt => 0,
                    HGroupConnectionKind::Finesse => 1,
                })
                .and_then(|pending| {
                    // A disjunctive Prompt is an ordered obligation: play its newest
                    // candidate first, then continue left-to-right if that card was
                    // merely playable. Independent per-card notes cannot safely skip
                    // a candidate because Good Touch creates correlated alternatives
                    // ("if the focus is R1 this card is not R1", and vice versa).
                    pending.cards.first().copied().map(|card| (pending, card))
                })
        })
        .flatten();
    if let Some((pending, card)) = connection {
        inferred.connection = Some(HGroupConnection {
            card,
            identity: pending.expected,
            kind: pending.kind,
            focus: pending.focus,
        });
    } else {
        // Self-Prompt and unresolved play promises survive intervening turns.
        // A promise its owner has explicitly declined is excluded: replay
        // already recorded that lifecycle transition in `invalidated_focuses`.
        let own_cards = replay.hands[view.observer.index()]
            .iter()
            .copied()
            .collect::<CardSet>();
        let mut seen_focus = CardSet::default();
        let unresolved = inferred
            .clues
            .iter()
            .rev()
            .filter(|clue| {
                clue.target == view.observer
                    && matches!(clue.kind, HGroupClueKind::Play | HGroupClueKind::PlayOrSave)
                    && own_cards.contains(&clue.focus)
                    && !fixed_cards.contains(&clue.focus)
                    && !replay.cards.invalidated_focuses.contains(&clue.focus)
                    && !replay.cards.declined_direct_plays.contains(&clue.focus)
                    && seen_focus.insert(clue.focus)
            })
            .cloned()
            .collect::<Vec<_>>();
        for clue in unresolved {
            let waiting_on_other_player = replay.pending_connections.iter().any(|pending| {
                pending.focus == clue.focus
                    && pending.actor != view.observer
                    && replay.pending_connections.is_active(pending)
            });
            if waiting_on_other_player {
                continue;
            }
            let previously_gotten = clue.previously_gotten.iter().copied().collect();
            infer_clue_to_self(deductions, &clue, &previously_gotten, &mut inferred);
            if inferred.connection.is_some() {
                break;
            }
        }
    }
    if rule_enabled(profile, HGroupRuleId::Priority)
        && inferred
            .connection
            .is_some_and(|connection| action_schedule.connection_layer_demonstrated(connection))
    {
        inferred
            .demonstrated_connections
            .extend(inferred.connection.map(|connection| connection.card));
    }
    inferred.completed_connection_focuses = action_schedule
        .completed_connection_focuses(&inferred.playable_now)
        .iter()
        .copied()
        .collect();
    // Build the canonical owner read model in production, not only in the
    // snapshot tests. Public convenience collections are materialized from
    // the same state so they cannot drift from per-card knowledge.
    let owner_knowledge = owner_knowledge_read_model(deductions, &replay.knowledge, &inferred);
    let _convention_only_trash_count = owner_knowledge
        .iter()
        .filter(|card| card.classifications.convention_only_trash)
        .count();
    debug_assert!(owner_knowledge.iter().all(|card| {
        let note = inferred.cards.iter().find(|note| note.card == card.card);
        note.is_some_and(|note| {
            note.play_obligation == card.play_obligation
                && note.focused == card.facts.focused
                && note.saved == card.facts.saved
                && note.finessed == card.facts.finessed
                && card.classifications.playable
                    == (inferred.playable_now.contains(&card.card)
                        || inferred
                            .connection
                            .is_some_and(|connection| connection.card == card.card))
                && card.position.chop == (inferred.chops[view.observer.index()] == Some(card.card))
                && card.position.chop_moved == inferred.chop_moved.contains(&card.card)
                && card.classifications.discard_now == inferred.discard_now.contains(&card.card)
                && (card.sources.is_empty()
                    || card.sources.iter().all(|source| source.turn() <= view.turn))
        })
    }));
    inferred
}

/// Actions permitted by the implemented Level 1 principles, in policy order.
#[must_use]
#[allow(clippy::too_many_lines)]
#[cfg(test)]
pub(crate) fn ordered_h_group_actions(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
) -> Vec<Action> {
    let analysis = build_h_group_analysis(deductions, profile);
    ordered_h_group_actions_from_analysis(deductions, profile, &analysis)
}

#[allow(clippy::too_many_lines)]
fn ordered_h_group_actions_from_analysis(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    analysis: &HGroupAnalysis,
) -> Vec<Action> {
    let view = deductions.view();
    let legal_actions = view.legal_actions();
    if legal_actions.is_empty() {
        return Vec::new();
    }
    let inferred = &analysis.inferences;
    let fresh_trash_chop_move_focus = analysis.replay.clues.iter().rev().find_map(|clue| {
        let recipient_has_acted = view.history.iter().any(|entry| {
            entry.turn > clue.turn
                && match entry.event {
                    ObservedEvent::Played { player, .. }
                    | ObservedEvent::Discarded { player, .. } => player == clue.target,
                    ObservedEvent::Clued { giver, .. } => giver == clue.target,
                    ObservedEvent::Drew { .. } => false,
                }
        });
        (clue.target == view.observer
            && !recipient_has_acted
            && analysis
                .replay
                .signals
                .has_at_turn(clue.turn, HGroupMoveKind::TrashChopMove)
            && view.hands[view.observer.index()]
                .iter()
                .any(|card| card.id == clue.focus))
        .then_some(clue.focus)
    });
    let mut clue_candidates = analysis_clue_candidates(deductions, profile, analysis).to_vec();
    clue_candidates.sort_by_key(|candidate| core::cmp::Reverse(candidate.score()));
    // https://hanabi.github.io/level-2/#the-5-stall-cluing-off-chop-5s
    // https://hanabi.github.io/level-9/#early-game-5-stalls
    // A known-trash or special discard does not end the Early Game. Before
    // the first completely unknown chop discard, the team must collectively
    // perform one available 5 Stall. This is a semantic obligation, so its
    // low intrinsic clue value must not lose to the null discard action.
    let required_first_five_stall = required_first_five_stall_actions(
        view,
        inferred,
        &analysis.replay,
        profile,
        &clue_candidates,
    );
    let first_five_stall_is_due = required_first_five_stall.is_some();
    if let Some(required) = &required_first_five_stall {
        clue_candidates.retain(|candidate| required.contains(&candidate.action));
    }
    if inferred.must_clue.contains(&view.observer) {
        let actions = clue_candidates
            .iter()
            .map(|candidate| candidate.action)
            .collect::<Vec<_>>();
        if !actions.is_empty() {
            return actions;
        }
    }

    if let Some(actions) = inferred.connection.and_then(|connection| {
        let demonstrated = connection_layer_demonstrated(view, inferred, profile, connection);
        legal_connection_actions(
            view,
            &analysis.replay,
            connection,
            paused_priority_play(view, inferred, profile, connection),
            demonstrated,
            &clue_candidates,
            &legal_actions,
        )
    }) {
        return actions;
    }

    let permission_to_discard_target =
        permission_to_discard_target(view, &analysis.replay, profile);
    let early_game_has_extinguishing_clue = first_five_stall_is_due
        || analysis.replay.early_game
            && inferred.discard_now.is_empty()
            && inferred.playable_now.is_empty()
            && clue_candidates.iter().any(|candidate| {
                Some(candidate.target()) != permission_to_discard_target
                    && (candidate.purpose() == CluePurpose::Play || candidate.is_save())
            });

    let mut actions = inferred
        .discard_now
        .iter()
        .copied()
        .map(Action::Discard)
        .collect::<Vec<_>>();
    actions.extend(
        ordered_playable_cards(view, inferred, profile)
            .into_iter()
            .map(Action::Play),
    );
    if let Some((card, _)) = scored_discard_candidate(view, inferred, profile) {
        if !early_game_has_extinguishing_clue || fresh_trash_chop_move_focus == Some(card) {
            actions.push(Action::Discard(card));
        }
    }
    actions.extend(clue_candidates.iter().map(|candidate| candidate.action));
    // https://hanabi.github.io/level-1/#the-early-game
    // A player may not end the Early Game while a genuine Play or Save Clue
    // remains. The recipient may still respond to a fresh Trash Chop Move by
    // discarding its known-trash focus; that action consumes the clue's
    // explicit safe-discard message rather than generically preferring any
    // surplus off-chop trash over forward progress.
    if early_game_has_extinguishing_clue {
        let gotten = inferred.gotten();
        let transfer =
            gentlemans_discard_candidate(view, inferred, profile, &gotten).map(|(card, _)| card);
        actions.retain(|action| match action {
            Action::Discard(card) => {
                transfer == Some(*card) || fresh_trash_chop_move_focus == Some(*card)
            }
            Action::Play(_) | Action::Clue { .. } => true,
        });
    }
    actions.dedup();
    actions.retain(|action| legal_actions.contains(action));
    if inferred.phase == HGroupPhase::EndGame {
        let ordinary_chop = inferred.chops[view.observer.index()];
        let ordinary_trash = convention_known_trash_discard(view, inferred);
        actions.retain(|action| match action {
            Action::Discard(card) => {
                // A normal known-trash discard is not a positional signal.
                ordinary_chop == Some(*card)
                    || ordinary_trash == Some(*card)
                    || positional_discard_is_valid(view, *card)
            }
            Action::Play(_) | Action::Clue { .. } => true,
        });
    }
    if !actions.is_empty() {
        let urgent_next_save = clue_candidates
            .iter()
            .any(|candidate| hard_clue_obligation(view, &analysis.replay, candidate));
        actions.sort_by(|left, right| {
            let score = |action: &Action| {
                clue_candidates
                    .iter()
                    .find(|candidate| candidate.action == *action)
                    .map_or_else(
                        || {
                            if inferred
                                .playable_now
                                .iter()
                                .any(|card| *action == Action::Play(*card))
                            {
                                if urgent_next_save { 300 } else { 425 }
                            } else if inferred
                                .discard_now
                                .iter()
                                .any(|card| *action == Action::Discard(*card))
                            {
                                575
                            } else if let Action::Discard(card) = action {
                                scored_discard_candidate(view, inferred, profile)
                                    .filter(|(candidate, _)| candidate == card)
                                    .map_or(0, |(_, score)| score)
                            } else {
                                0
                            }
                        },
                        |candidate| {
                            if !inferred.playable_now.is_empty()
                                && !clue_preempts_play_obligation(view, &analysis.replay, candidate)
                            {
                                candidate.score().min(400)
                            } else {
                                candidate.score()
                            }
                        },
                    )
            };
            score(right).cmp(&score(left))
        });
        return actions;
    }

    let gotten = inferred.gotten();
    let own_hand = &view.hands[view.observer.index()];
    if view.clue_tokens < MAX_CLUE_TOKENS {
        if let Some(trash) =
            convention_known_trash_discard(view, inferred).filter(|card| gotten.contains(card))
        {
            return vec![Action::Discard(trash)];
        }
    }
    if view.clue_tokens < MAX_CLUE_TOKENS {
        if let Some(chop) = inferred.chops[view.observer.index()] {
            if !inferred.is_saved(chop) {
                return vec![Action::Discard(chop)];
            }
        }
    }
    if view.clue_tokens < MAX_CLUE_TOKENS && view.deck_size <= view.hands.len() {
        if let Some(forced) = own_hand.iter().find(|card| {
            !gotten.contains(&card.id)
                && !inferred.is_saved(card.id)
                && positional_discard_is_valid(view, card.id)
        }) {
            return vec![Action::Discard(forced.id)];
        }
    }
    if own_hand.iter().all(|card| gotten.contains(&card.id)) {
        if let Some(clue) = legal_actions
            .iter()
            .copied()
            .filter_map(|action| {
                fallback_clue_score(view, profile, action, &gotten).map(|score| (score, action))
            })
            .max_by_key(|(score, _)| *score)
            .map(|(_, action)| action)
        {
            return vec![clue];
        }
    }
    if view.deck_size <= view.hands.len() {
        if let Some(clue) = legal_actions
            .iter()
            .copied()
            .filter_map(|action| {
                fallback_clue_score(view, profile, action, &gotten).map(|score| (score, action))
            })
            .max_by_key(|(score, _)| *score)
            .map(|(_, action)| action)
        {
            return vec![clue];
        }
    }
    if view.clue_tokens < MAX_CLUE_TOKENS {
        if let Some(chop) = inferred.chops[view.observer.index()] {
            if !inferred.is_saved(chop) {
                return vec![Action::Discard(chop)];
            }
        }
    }
    // https://hanabi.github.io/extras/miscellaneous/#no-valid-first-turn-clues
    // At Max, a first player with no convention-valid clue chooses the least
    // damaging lie instead of falling through to the convention-agnostic
    // blind-play fallback.
    if profile == HGroupProfile::Max && view.turn == 0 {
        if let Some(clue) = legal_actions
            .iter()
            .copied()
            .filter(|action| matches!(action, Action::Clue { .. }))
            .min_by_key(|action| no_valid_first_turn_damage(view, profile, *action))
        {
            return vec![clue];
        }
    }
    // Convention-inconsistent arbitrary inputs still need a total policy.
    // Retain the convention-agnostic emergency behavior selected for this
    // engine: oldest discard, or newest blind play when discarding is illegal.
    if view.clue_tokens < MAX_CLUE_TOKENS {
        if let Some(oldest) = own_hand.first() {
            return vec![Action::Discard(oldest.id)];
        }
    }
    own_hand
        .last()
        .map_or_else(Vec::new, |newest| vec![Action::Play(newest.id)])
}

fn candidate_is_five_stall(candidate: &CompiledClueAction) -> bool {
    candidate.move_kind() == Some(HGroupMoveKind::FiveStall)
}

fn guided_blind_play(kind: HGroupMoveKind) -> bool {
    matches!(
        kind,
        HGroupMoveKind::Finesse
            | HGroupMoveKind::ReverseFinesse
            | HGroupMoveKind::SelfFinesse
            | HGroupMoveKind::LayeredFinesse
            | HGroupMoveKind::HiddenFinesse
            | HGroupMoveKind::ClandestineFinesse
            | HGroupMoveKind::QueuedFinesse
            | HGroupMoveKind::AmbiguousFinesse
            | HGroupMoveKind::Bluff
            | HGroupMoveKind::SelfBluff
    )
}

/// The current player may end the Early Game without extinguishing a clue to
/// the player who acted immediately before them. The exception is a previous
/// player who was blind-playing into a Finesse or Bluff under Guide Principle:
/// that action did not communicate that the current player's hand was safe.
///
/// Sources:
/// - <https://hanabi.github.io/level-9/#permission-to-discard-ptd>
/// - <https://hanabi.github.io/level-11/#guide-principle>
fn permission_to_discard_target(
    view: &PlayerView,
    replay: &HGroupState,
    profile: HGroupProfile,
) -> Option<PlayerId> {
    if !replay.early_game || !rule_enabled(profile, HGroupRuleId::Stalling) {
        return None;
    }
    let player_count = view.hands.len();
    let previous = PlayerId::new(
        u8::try_from((view.observer.index() + player_count - 1) % player_count)
            .expect("standard Hanabi has at most five players"),
    );
    let previous_action = view
        .history
        .iter()
        .rev()
        .find(|entry| !matches!(entry.event, ObservedEvent::Drew { .. }))?;
    match &previous_action.event {
        ObservedEvent::Discarded { player, .. } if *player == previous => Some(previous),
        ObservedEvent::Played { player, card, .. } if *player == previous => {
            let guide_exception = rule_enabled(profile, HGroupRuleId::Bluffs)
                && !was_clued_before(view, previous_action.turn, *card)
                && replay.signals.iter().any(|signal| {
                    signal.turn < previous_action.turn
                        && signal.cards.contains(card)
                        && guided_blind_play(signal.kind)
                });
            (!guide_exception).then_some(previous)
        }
        // A clue to the current player is an explicit instruction, not the
        // previous player's silent indication that no useful clue existed.
        ObservedEvent::Clued { giver, .. } if *giver == previous => None,
        _ => None,
    }
}

fn required_first_five_stall_actions(
    view: &PlayerView,
    inferred: &HGroupInferences,
    replay: &HGroupState,
    profile: HGroupProfile,
    clues: &[CompiledClueAction],
) -> Option<Vec<Action>> {
    let gotten = inferred.gotten();
    let permission_to_discard_target = permission_to_discard_target(view, replay, profile);
    let actor_has_known_safe_discard = !inferred.discard_now.is_empty()
        || replay.hands[view.observer.index()].iter().any(|card| {
            inferred
                .cards
                .iter()
                .find(|note| note.card == *card)
                .is_some_and(|note| {
                    !note.identities.is_empty()
                        && note.identities.iter().all(|identity| {
                            is_convention_trash(view, identity, &gotten, &inferred.cards)
                        })
                })
        });
    let has_normal_play_or_save = clues
        .iter()
        .any(|candidate| candidate.purpose() == CluePurpose::Play || candidate.is_save());
    let due = replay.early_game
        && rule_enabled(profile, HGroupRuleId::BasicMoves)
        && !replay
            .signals
            .iter()
            .any(|signal| signal.kind == HGroupMoveKind::FiveStall)
        && inferred.playable_now.is_empty()
        && !actor_has_known_safe_discard
        && !has_normal_play_or_save;
    if !due {
        return None;
    }
    let closest = clues
        .iter()
        .filter(|candidate| {
            candidate_is_five_stall(candidate)
                && Some(candidate.target()) != permission_to_discard_target
        })
        .map(|candidate| five_stall_distance_from_chop(view, candidate, &gotten))
        .min()?;
    // https://hanabi.github.io/level-9/#5-stalls-closest-to-chop
    // A genuine Trash Chop Move remains an equivalent urgent alternative;
    // every other clue is excluded by policy rather than by numeric value.
    Some(
        clues
            .iter()
            .filter(|candidate| {
                candidate.move_kind() == Some(HGroupMoveKind::TrashChopMove)
                    || candidate_is_five_stall(candidate)
                        && Some(candidate.target()) != permission_to_discard_target
                        && five_stall_distance_from_chop(view, candidate, &gotten) == closest
            })
            .map(|candidate| candidate.action)
            .collect(),
    )
}

fn five_stall_distance_from_chop(
    view: &PlayerView,
    candidate: &CompiledClueAction,
    gotten: &CardSet,
) -> usize {
    let Action::Clue { target, clue } = candidate.action else {
        return usize::MAX;
    };
    view.hands[target.index()]
        .iter()
        .enumerate()
        .filter(|(_, card)| {
            card.identity
                .is_some_and(|identity| clue.matches(identity) && identity.rank == Rank::Five)
        })
        .map(|(position, _)| {
            view.hands[target.index()][position + 1..]
                .iter()
                .filter(|card| !gotten.contains(&card.id))
                .count()
        })
        .min()
        .unwrap_or(usize::MAX)
}

fn no_valid_first_turn_damage(
    view: &PlayerView,
    profile: HGroupProfile,
    action: Action,
) -> (usize, core::cmp::Reverse<usize>, core::cmp::Reverse<usize>) {
    let Action::Clue { target, clue } = action else {
        return (usize::MAX, core::cmp::Reverse(0), core::cmp::Reverse(0));
    };
    let touched = view.hands[target.index()]
        .iter()
        .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let after = prospective_clue_view(view, target, clue, &touched);
    let inferred = projected_h_group_replay(&after, profile, target)
        .map(|(deductions, replay)| infer_h_group_from_replay(&deductions, replay, profile));
    let false_plays = inferred.as_ref().map_or(usize::MAX / 2, |inferred| {
        inferred
            .playable_now
            .iter()
            .filter(|card| {
                identity_of(view, **card).is_some_and(|identity| !is_playable_now(view, identity))
            })
            .count()
    });
    let protected = touched
        .iter()
        .filter(|card| {
            identity_of(view, **card)
                .is_some_and(|identity| identity.rank == Rank::Five || is_critical(view, identity))
        })
        .count();
    (
        false_plays,
        core::cmp::Reverse(protected),
        core::cmp::Reverse(touched.len()),
    )
}

/// Builds the single action analysis consumed by convention decisions and planning.
/// Semantic admissibility, ordering, priorities, and predictability must be derived
/// here instead of being independently reconstructed by each consumer.
fn analyze_h_group_actions_from_analysis(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    analysis: &HGroupAnalysis,
) -> HGroupActionSet {
    if let Some(cached) = analysis.action_set.get() {
        return cached.clone();
    }
    let inferred = &analysis.inferences;
    let mut clue_candidates = analysis_clue_candidates(deductions, profile, analysis).to_vec();
    clue_candidates.sort_by_key(|candidate| core::cmp::Reverse(candidate.score()));

    let ordered = ordered_h_group_actions_from_analysis(deductions, profile, analysis);
    let analyzed = ordered
        .iter()
        .copied()
        .map(|action| {
            let clue = clue_candidates
                .iter()
                .find(|candidate| candidate.action == action);
            let priority = raw_h_group_action_priority(deductions, profile, analysis, action);
            let advances_terminal_plan = clue
                .and_then(|candidate| {
                    endgame_progress_priority(deductions, profile, analysis, candidate)
                        .filter(|progress| *progress > 100 + i32::from(candidate.score()))
                })
                .is_some();
            CompiledHGroupAction {
                action,
                kind: classify_h_group_action(action, inferred, clue),
                policy_tier: ConventionPolicyTier::Admitted,
                priority,
                preference: ActionPreference::new(priority, advances_terminal_plan),
            }
        })
        .collect::<Vec<_>>();

    let constraints = derive_convention_constraints(
        deductions.view(),
        inferred,
        &analysis.replay,
        profile,
        &clue_candidates,
        &analyzed,
    );
    let predictable = derive_predictable_action(
        deductions,
        inferred,
        &analysis.replay,
        profile,
        &clue_candidates,
    )
    .filter(|action| constraints.allows(*action))
    .or_else(|| constraints.single_required());
    let mut analyzed = analyzed
        .into_iter()
        .filter(|candidate| constraints.allows(candidate.action))
        .collect::<Vec<_>>();
    for candidate in &mut analyzed {
        candidate.policy_tier = if constraints.kind().is_some() {
            ConventionPolicyTier::Required
        } else if candidate.kind == HGroupActionKind::Fallback {
            ConventionPolicyTier::Fallback
        } else {
            ConventionPolicyTier::Admitted
        };
        candidate.preference.set_policy_tier(candidate.policy_tier);
    }
    let (ranked_preferred, _constraint_reason) = derive_preferred_action(
        deductions,
        profile,
        &clue_candidates,
        &analyzed,
        &constraints,
    );
    // A predictable convention response is a hard semantic conclusion, not a
    // second recommendation that may conflict with heuristic ranking.
    let preferred = predictable.or(ranked_preferred);

    debug_assert!(analyzed.iter().all(|analysis| match analysis.kind {
        HGroupActionKind::RequiredDiscard | HGroupActionKind::Discard => {
            matches!(analysis.action, Action::Discard(_))
        }
        HGroupActionKind::PromisedPlay | HGroupActionKind::Connection => {
            matches!(analysis.action, Action::Play(_))
        }
        HGroupActionKind::Clue {
            target,
            save: _,
            immediate_play: _,
        } => matches!(analysis.action, Action::Clue { target: actual, .. } if actual == target),
        HGroupActionKind::Fallback => true,
    }));
    let decision = HGroupActionSet {
        actions: analyzed,
        preferred,
        predictable,
    };
    let _ = analysis.action_set.set(decision.clone());
    decision
}

/// Constructs every convention-facing result from one history replay and one
/// inference pass.
pub(crate) struct HGroupConventionDecision {
    pub(crate) inferences: HGroupInferences,
    pub(crate) actions: Vec<(Action, ConventionPolicyTier, i32, ConventionActionReason)>,
    pub(crate) rejected_actions: Vec<RejectedConventionAction>,
    pub(crate) preferred: Option<Action>,
    pub(crate) forced: Option<Action>,
    pub(crate) belief_constraints: BeliefConstraints,
}

pub(crate) fn analyze_h_group_convention(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
) -> HGroupConventionDecision {
    let analysis = build_h_group_analysis(deductions, profile);
    let actions = analyze_h_group_actions_from_analysis(deductions, profile, &analysis);
    let admitted_actions = actions
        .actions
        .iter()
        .map(|candidate| candidate.action)
        .collect::<Vec<_>>();
    let rejected_actions = h_group_rejected_clues_from_replay(
        deductions,
        profile,
        &analysis.replay,
        &admitted_actions,
    );
    let ranked = actions
        .actions
        .iter()
        .map(|candidate| {
            (
                candidate.action,
                candidate.policy_tier,
                candidate.priority,
                convention_action_reason(candidate.kind),
            )
        })
        .collect();
    let preferred = select_h_group_action_from_analysis(deductions, profile, &analysis);
    let forced = h_group_predictable_action_from_analysis(deductions, profile, &analysis);
    let belief_constraints =
        ConventionConstraintGraph::from_replay(deductions, &analysis.replay, &analysis.inferences)
            .into_belief_constraints();
    HGroupConventionDecision {
        inferences: analysis.inferences.clone(),
        actions: ranked,
        rejected_actions,
        preferred,
        forced,
        belief_constraints,
    }
}

fn convention_action_reason(kind: HGroupActionKind) -> ConventionActionReason {
    match kind {
        HGroupActionKind::Connection => ConventionActionReason::Connection,
        HGroupActionKind::RequiredDiscard => ConventionActionReason::RequiredDiscard,
        HGroupActionKind::PromisedPlay => ConventionActionReason::PromisedPlay,
        HGroupActionKind::Clue { save: true, .. } => ConventionActionReason::SaveClue,
        HGroupActionKind::Clue {
            immediate_play: true,
            ..
        } => ConventionActionReason::PlayClue,
        HGroupActionKind::Clue { .. } => ConventionActionReason::OtherClue,
        HGroupActionKind::Discard => ConventionActionReason::Discard,
        HGroupActionKind::Fallback => ConventionActionReason::Fallback,
    }
}

fn classify_h_group_action(
    action: Action,
    inferred: &HGroupInferences,
    clue: Option<&CompiledClueAction>,
) -> HGroupActionKind {
    if inferred
        .connection
        .is_some_and(|connection| action == Action::Play(connection.card))
    {
        HGroupActionKind::Connection
    } else if inferred
        .discard_now
        .iter()
        .any(|card| action == Action::Discard(*card))
    {
        HGroupActionKind::RequiredDiscard
    } else if inferred
        .playable_now
        .iter()
        .any(|card| action == Action::Play(*card))
    {
        HGroupActionKind::PromisedPlay
    } else if let Some(candidate) = clue {
        HGroupActionKind::Clue {
            target: candidate.target(),
            save: candidate.is_save(),
            immediate_play: candidate.immediate_play(),
        }
    } else if matches!(action, Action::Discard(_)) {
        HGroupActionKind::Discard
    } else {
        HGroupActionKind::Fallback
    }
}

fn derive_preferred_action(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    clues: &[CompiledClueAction],
    analyzed: &[CompiledHGroupAction],
    constraints: &ConventionConstraints,
) -> (Option<Action>, Option<ConventionRequirementKind>) {
    let mut candidates = analyzed
        .iter()
        .filter(|candidate| constraints.allows(candidate.action))
        .collect::<Vec<_>>();
    candidates.sort_by_key(|candidate| core::cmp::Reverse(candidate.preference));
    let preferred = candidates
        .into_iter()
        .find(|analysis| {
            analysis.kind == HGroupActionKind::Connection
                || h_group_planning_action_safe(deductions, profile, analysis.action)
        })
        .map(|analysis| analysis.action)
        .or_else(|| clues.first().map(|candidate| candidate.action));
    (preferred, constraints.kind())
}

fn derive_convention_constraints(
    view: &PlayerView,
    inferred: &HGroupInferences,
    replay: &HGroupState,
    profile: HGroupProfile,
    clues: &[CompiledClueAction],
    analyzed: &[CompiledHGroupAction],
) -> ConventionConstraints {
    let has_forced_play = inferred.cards.iter().any(|card| {
        inferred.playable_now.contains(&card.card)
            && card.play_obligation == Some(HGroupPlayObligation::Forced)
    });
    if let Some(urgent) = (!has_forced_play)
        .then(|| {
            clues
                .iter()
                .find(|candidate| hard_clue_obligation(view, replay, candidate))
        })
        .flatten()
    {
        return ConventionConstraints::require(
            ConventionRequirementKind::UrgentProtection,
            clues
                .iter()
                .filter(|candidate| {
                    candidate.action == urgent.action
                        || (candidate.target() == urgent.target() && candidate.immediate_play())
                })
                .map(|candidate| candidate.action),
        );
    }
    if inferred.connection.is_some() {
        return ConventionConstraints::require(
            ConventionRequirementKind::ConnectionResponse,
            analyzed
                .iter()
                .filter(|candidate| {
                    candidate.kind == HGroupActionKind::Connection || candidate.priority >= 800
                })
                .map(|candidate| candidate.action),
        );
    }
    if let Some(required) =
        required_first_five_stall_actions(view, inferred, replay, profile, clues)
    {
        return ConventionConstraints::require(ConventionRequirementKind::EarlyFiveStall, required);
    }
    if !inferred.discard_now.is_empty() {
        return ConventionConstraints::require(
            ConventionRequirementKind::RequiredDiscard,
            inferred.discard_now.iter().copied().map(Action::Discard),
        );
    }
    if inferred.must_clue.contains(&view.observer) {
        return ConventionConstraints::require(
            ConventionRequirementKind::MustClue,
            clues.iter().map(|candidate| candidate.action),
        );
    }
    ConventionConstraints::default()
}

fn h_group_planning_action_safe(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    action: Action,
) -> bool {
    match action {
        Action::Play(card) => !prospective_play_has_unsafe_inference(deductions, profile, card),
        Action::Discard(_) | Action::Clue { .. } => true,
    }
}

fn candidate_can_preempt_current_play(
    candidate: &CompiledClueAction,
    inferred: &HGroupInferences,
    replay: &HGroupState,
) -> bool {
    candidate.can_preempt_ordinary_play()
        || (candidate.purpose() == CluePurpose::Play
            && candidate.immediate_play()
            && candidate.action_coverage() >= 2
            && inferred.playable_now.iter().any(|playable| {
                inferred
                    .cards
                    .iter()
                    .find(|card| card.card == *playable)
                    .and_then(|card| {
                        card.promised_identity.or_else(|| {
                            (card.identities.len() == 1)
                                .then(|| card.identities.iter().next())
                                .flatten()
                        })
                    })
                    .is_some_and(|identity| replay.is_exact_transfer(*playable, identity))
            }))
}

fn candidate_is_lie_component_finesse(
    view: &PlayerView,
    profile: HGroupProfile,
    candidate: &CompiledClueAction,
) -> bool {
    let Action::Clue { target, clue } = candidate.action else {
        return false;
    };
    let touched = view.hands[target.index()]
        .iter()
        .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
        .map(|card| card.id)
        .collect::<Vec<_>>();
    prospective_clue_primary_kind(view, profile, target, clue, &touched)
        == Some(HGroupClueKind::Unrecognized)
        && prospective_team_clue_signal_kinds(view, profile, target, clue, &touched)
            .contains(&HGroupMoveKind::LieComponentFinesse)
}

fn derive_predictable_action(
    deductions: &LogicalDeductions,
    inferred: &HGroupInferences,
    replay: &HGroupState,
    profile: HGroupProfile,
    clues: &[CompiledClueAction],
) -> Option<Action> {
    let view = deductions.view();
    let has_forced_play = inferred.playable_now.iter().any(|playable| {
        inferred.cards.iter().any(|card| {
            card.card == *playable && card.play_obligation == Some(HGroupPlayObligation::Forced)
        })
    });
    let safe_at_last_strike = |action: Action| {
        if view.strikes < 2 {
            return Some(action);
        }
        match action {
            Action::Play(card) => deductions
                .possible_identities(card)
                .is_some_and(|identities| {
                    !identities.is_empty()
                        && identities
                            .iter()
                            .all(|identity| is_playable_now(view, identity))
                }),
            Action::Discard(_) | Action::Clue { .. } => true,
        }
        .then_some(action)
    };

    if let Some(connection) = inferred.connection {
        legal_connection_actions(
            view,
            replay,
            connection,
            paused_priority_play(view, inferred, profile, connection),
            connection_layer_demonstrated(view, inferred, profile, connection),
            clues,
            &view.legal_actions(),
        )
        .filter(|actions| actions.len() == 1)
        .and_then(|actions| safe_at_last_strike(actions[0]))
    } else if let [card] = inferred.discard_now.as_slice() {
        safe_at_last_strike(Action::Discard(*card))
    } else if gentlemans_discard_candidate(view, inferred, profile, &inferred.gotten())
        .is_none_or(|(_, identity)| identity.rank == Rank::One)
        && !clues.iter().any(|candidate| {
            (candidate.is_urgent_save() && hard_clue_obligation(view, replay, candidate))
                || (!has_forced_play
                    && candidate_can_preempt_current_play(candidate, inferred, replay)
                    && (!completed_connection_focus_is_due(inferred)
                        || candidate_is_lie_component_finesse(view, profile, candidate)))
        })
        && inferred.playable_now.len() == 1
    {
        safe_at_last_strike(Action::Play(inferred.playable_now[0]))
    } else if inferred.must_clue.contains(&view.observer) && clues.len() == 1 {
        safe_at_last_strike(clues[0].action)
    } else {
        None
    }
}

fn fallback_clue_score(
    view: &PlayerView,
    profile: HGroupProfile,
    action: Action,
    gotten: &CardSet,
) -> Option<u8> {
    let Action::Clue { target, clue } = action else {
        return None;
    };
    let hand = &view.hands[target.index()];
    let layout = hand.iter().map(|card| card.id).collect::<Vec<_>>();
    let touched = hand
        .iter()
        .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
        .map(|card| card.id)
        .collect::<Vec<_>>();
    if touched.iter().all(|card| {
        hand.iter()
            .find(|candidate| candidate.id == *card)
            .is_some_and(|card| card.clues.has_positive_clue(clue))
    }) {
        return None;
    }
    let old_chop = chop(&layout, gotten);
    let focus = focus(&layout, &touched, old_chop, gotten)?;
    let identity = hand
        .iter()
        .find(|card| card.id == focus)
        .and_then(|card| card.identity)?;
    let (score, save) = if is_playable_now(view, identity) {
        (3, false)
    } else if old_chop == Some(focus)
        && (matches!(
            (clue, identity.rank),
            (Clue::Rank(Rank::Five), Rank::Five) | (Clue::Rank(Rank::Two), Rank::Two)
        ) || is_critical(view, identity))
    {
        (2, true)
    } else {
        return None;
    };
    let candidate = CompiledClueAction::new(
        action,
        None,
        ClueValue::new(u16::from(score)),
        if save {
            CluePurpose::FallbackSave
        } else {
            CluePurpose::FallbackPlay
        },
        ClueSchedule::new(
            save && (identity.rank == Rank::Five || is_critical(view, identity)),
            !save,
        ),
        0,
    );
    if prospective_clue_has_unsafe_connection(view, profile, target, focus, clue, &touched, !save) {
        return None;
    }
    if save && !prospective_clue_marks_focus_saved(view, profile, target, focus, clue, &touched) {
        return None;
    }
    if view.clue_tokens == 1 && creates_false_anxiety(view, profile, gotten, &candidate) {
        return None;
    }
    Some(score)
}

fn positional_discard_is_valid(view: &PlayerView, discard: CardId) -> bool {
    positional_discard_is_valid_for(view, view.observer, discard)
}

fn positional_discard_is_valid_for(view: &PlayerView, player: PlayerId, discard: CardId) -> bool {
    if view.deck_size > view.hands.len() {
        return true;
    }
    let hand = &view.hands[player.index()];
    let indicated_slot = hand
        .iter()
        .filter(|candidate| candidate.id.index() < discard.index())
        .count();
    (1..view.hands.len()).any(|distance| {
        let target = (player.index() + distance) % view.hands.len();
        view.hands[target]
            .get(indicated_slot)
            .and_then(|card| card.identity)
            .is_some_and(|identity| is_playable_now(view, identity))
    })
}

pub(super) fn positional_discard_candidate(
    deductions: &LogicalDeductions,
    player: PlayerId,
    gotten: &CardSet,
) -> Option<CardId> {
    let view = deductions.view();
    if view.deck_size > view.hands.len() {
        return None;
    }
    let hand = &view.hands[player.index()];
    let layout = hand.iter().map(|card| card.id).collect::<Vec<_>>();
    let candidates = chop(&layout, gotten)
        .into_iter()
        .chain(
            hand.iter()
                .map(|card| card.id)
                .filter(|card| !gotten.contains(card)),
        )
        .collect::<Vec<_>>();
    for candidate in candidates {
        let indicated_slot = hand
            .iter()
            .filter(|card| card.id.index() < candidate.index())
            .count();
        let mut possibly_valid = false;
        let mut definitely_valid = false;
        for distance in 1..view.hands.len() {
            let target = (player.index() + distance) % view.hands.len();
            let Some(card) = view.hands[target].get(indicated_slot) else {
                continue;
            };
            if let Some(identity) = card.identity {
                if is_playable_now(view, identity) {
                    possibly_valid = true;
                    definitely_valid = true;
                }
                continue;
            }
            let Some(possibilities) = deductions.possible_identities(card.id) else {
                possibly_valid = true;
                continue;
            };
            let playable = possibilities
                .iter()
                .filter(|identity| is_playable_now(view, *identity))
                .count();
            possibly_valid |= playable > 0;
            definitely_valid |= playable > 0 && playable == possibilities.len();
        }
        if definitely_valid {
            return Some(candidate);
        }
        if possibly_valid {
            // The target can see the giver's hidden slot and may use this
            // earlier positional discard. The giver cannot safely infer a
            // later endangered card from their own information set.
            return None;
        }
    }
    None
}

pub(super) fn positional_discard_is_valid_snapshot(
    view: &PlayerView,
    hands: &[Vec<CardId>],
    player: PlayerId,
    discard: CardId,
    deck_size: usize,
    stack_heights: [u8; 5],
) -> bool {
    if deck_size > hands.len() {
        return false;
    }
    let indicated_slot = hands[player.index()]
        .iter()
        .filter(|candidate| candidate.index() < discard.index())
        .count();
    (1..hands.len()).any(|distance| {
        let target = (player.index() + distance) % hands.len();
        hands[target].get(indicated_slot).is_some_and(|card| {
            identity_of(view, *card).map_or_else(
                // The clue itself establishes that the giver saw a playable
                // card in this hidden slot. Conditioning on the focused Save
                // avoids mistaking an earlier unknown slot for the intended
                // positional discard.
                || target == view.observer.index(),
                |identity| is_playable_at(stack_heights, identity),
            )
        })
    })
}

fn scored_discard_candidate(
    view: &PlayerView,
    inferred: &HGroupInferences,
    profile: HGroupProfile,
) -> Option<(CardId, u16)> {
    if view.clue_tokens == MAX_CLUE_TOKENS {
        return None;
    }
    let gotten = inferred.gotten();
    let own_hand = &view.hands[view.observer.index()];
    if let Some((card, identity)) = gentlemans_discard_candidate(view, inferred, profile, &gotten) {
        // A Gentleman's Discard exchanges one tempo for a clue token while
        // preserving the playable identity on another player's Finesse
        // Position. That is a small but real improvement over consuming the
        // actor's copy when the transfer is available.
        // Transferring one of three 1s generally loses tempo without enough
        // compensating card value, so retain it as a legal option below the
        // ordinary play. Higher ranks have only two copies and receive the
        // full transfer premium.
        return Some((card, if identity.rank == Rank::One { 400 } else { 450 }));
    }
    let known_trash = convention_known_trash_discard(view, inferred);
    if let Some(card) = known_trash {
        // A known-trash discard recovers a clue with no card-value cost. It
        // beats an ordinary Play Clue, but not an already promised play.
        return Some((card, 410));
    }
    if let Some(card) =
        inferred.chops[view.observer.index()].filter(|card| !inferred.is_saved(*card))
    {
        // Spending a chop is preferable to a low-value tempo/stall clue, but
        // a useful direct Play Clue still takes priority.
        return Some((card, 300));
    }
    (view.deck_size <= view.hands.len())
        .then(|| {
            own_hand.iter().map(|card| card.id).find(|card| {
                !gotten.contains(card)
                    && !inferred.is_saved(*card)
                    && positional_discard_is_valid(view, *card)
            })
        })
        .flatten()
        .map(|card| (card, 275))
}

/// Finds a playable, positively clued own card that can be transferred to an
/// exact matching card on another player's Finesse Position.
///
/// [Gentleman's Discard](https://hanabi.github.io/level-10/#the-gentlemans-discard-gd)
/// is evaluated from the actor's perspective: the actor can see the recipient
/// card even though the earlier clue giver could only project an ordinary play
/// from their own hidden-hand perspective.
fn gentlemans_discard_candidate(
    view: &PlayerView,
    inferred: &HGroupInferences,
    profile: HGroupProfile,
    gotten: &CardSet,
) -> Option<(CardId, Card)> {
    if !rule_enabled(profile, HGroupRuleId::SpecialDiscards) {
        return None;
    }
    if inferred.playable_now.len() != 1 {
        // An independent promised play takes Priority. Transferring a
        // different playable card first would delay that established action
        // and is not the tempo-neutral comparison handled here.
        return None;
    }
    let own_hand = &view.hands[view.observer.index()];
    inferred.playable_now.iter().copied().find_map(|candidate| {
        let observed = own_hand.iter().find(|card| card.id == candidate)?;
        let positively_clued = Suit::ALL
            .iter()
            .copied()
            .any(|suit| observed.clues.has_positive_clue(Clue::Suit(suit)))
            || Rank::ALL
                .iter()
                .copied()
                .any(|rank| observed.clues.has_positive_clue(Clue::Rank(rank)));
        if !positively_clued {
            return None;
        }
        let identity = inferred
            .cards
            .iter()
            .find(|note| note.card == candidate)
            .and_then(|note| (note.identities.len() == 1).then(|| note.identities.iter().next()))
            .flatten()?;
        (1..view.hands.len())
            .any(|distance| {
                let player = (view.observer.index() + distance) % view.hands.len();
                finesse_position(&view.hands[player], gotten, 0)
                    .is_some_and(|card| card.identity == Some(identity))
            })
            .then_some((candidate, identity))
    })
}

fn convention_known_trash_discard(
    view: &PlayerView,
    inferred: &HGroupInferences,
) -> Option<CardId> {
    let gotten = inferred.gotten();
    // Hands are stored oldest first; leftmost means newest first.
    // https://hanabi.github.io/level-14/#known-trash-discard-order
    // Required discharge discards are handled before this ordinary fallback.
    let hand = &view.hands[view.observer.index()];
    let is_trash = |card: &hanabi_core::ObservedCard| {
        inferred
            .cards
            .iter()
            .find(|note| note.card == card.id)
            .is_some_and(|note| {
                !note.identities.is_empty()
                    && note.identities.iter().all(|identity| {
                        is_convention_trash(view, identity, &gotten, &inferred.cards)
                    })
            })
    };
    hand.iter()
        .rev()
        .find(|card| {
            let positively_clued = Suit::ALL
                .iter()
                .any(|suit| card.clues.has_positive_clue(Clue::Suit(*suit)))
                || Rank::ALL
                    .iter()
                    .any(|rank| card.clues.has_positive_clue(Clue::Rank(*rank)));
            positively_clued && is_trash(card)
        })
        .or_else(|| hand.iter().find(|card| is_trash(card)))
        .map(|card| card.id)
}

/// Returns the remaining visible 5s whose owners do not yet know to play.
///
/// Stacks below 4 are accepted only when every intervening card is visible and
/// already committed by its owner's convention state. A missing connector in
/// the deck or in the observer's hidden hand leaves the completion plan
/// unresolved and disables progress dominance.
fn endgame_completion_plan<'analysis>(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    analysis: &'analysis HGroupAnalysis,
) -> Option<&'analysis EndgameCompletionPlan> {
    analysis
        .endgame_completion
        .get_or_init(|| {
            let view = deductions.view();
            let team = TeamConventionSnapshot::new(view.clone(), profile);
            let mut unresolved_fives = CardSet::default();
            for suit in Suit::ALL {
                let height = view.play_stacks[suit.index()].len();
                if height == Rank::ALL.len() {
                    continue;
                }
                for rank in Rank::ALL.iter().copied().skip(height) {
                    let identity = Card::new(suit, rank);
                    let visible_copies = view
                        .hands
                        .iter()
                        .enumerate()
                        .filter(|(owner, _)| *owner != view.observer.index())
                        .flat_map(|(owner, hand)| {
                            hand.iter()
                                .filter(move |card| card.identity == Some(identity))
                                .map(move |card| (owner, card.id))
                        })
                        .collect::<Vec<_>>();
                    if visible_copies.is_empty() {
                        return None;
                    }
                    let committed = visible_copies.iter().find_map(|(owner, card)| {
                        let owner = PlayerId::new(
                            u8::try_from(*owner).expect("standard Hanabi has at most five players"),
                        );
                        let projection = team.projection(owner)?;
                        let owns_commitment = projection.inferred.playable_now.contains(card)
                            || projection.inferred.cards.iter().any(|note| {
                                note.card == *card
                                    && (note.finessed || note.play_obligation.is_some())
                            })
                            || projection.inferred.signals.iter().any(|signal| {
                                signal.target == Some(owner)
                                    && signal.cards.contains(card)
                                    && signal.identity == Some(identity)
                                    && matches!(
                                        signal.kind,
                                        HGroupMoveKind::Prompt
                                            | HGroupMoveKind::Finesse
                                            | HGroupMoveKind::ReverseFinesse
                                            | HGroupMoveKind::SelfFinesse
                                            | HGroupMoveKind::LayeredFinesse
                                            | HGroupMoveKind::HiddenFinesse
                                            | HGroupMoveKind::ClandestineFinesse
                                            | HGroupMoveKind::QueuedFinesse
                                            | HGroupMoveKind::AmbiguousFinesse
                                    )
                            })
                            || projection.inferred.clues.iter().rev().any(|clue| {
                                clue.focus == *card
                                    && matches!(
                                        clue.kind,
                                        HGroupClueKind::Play | HGroupClueKind::PlayOrSave
                                    )
                                    && clue.play_identities.contains(identity)
                            });
                        owns_commitment.then_some(*card)
                    });
                    if rank == Rank::Five {
                        if committed.is_none() {
                            // There is only one copy of every 5.
                            unresolved_fives.insert(visible_copies[0].1);
                        }
                    } else if committed.is_none() {
                        return None;
                    }
                }
            }
            Some(EndgameCompletionPlan { unresolved_fives })
        })
        .as_ref()
}

/// A known-trash discard is dominated when it only creates a surplus token
/// while leaving an inevitable final Play Clue for the next teammate to give.
fn endgame_progress_priority(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    analysis: &HGroupAnalysis,
    candidate: &CompiledClueAction,
) -> Option<i32> {
    let view = deductions.view();
    let is_multi_action_ignition = matches!(
        candidate.move_kind(),
        Some(
            HGroupMoveKind::ReplayDoubleIgnition
                | HGroupMoveKind::TrashDoubleIgnition
                | HGroupMoveKind::PokeDoubleIgnition
                | HGroupMoveKind::ChopMoveIgnition
                | HGroupMoveKind::BombDoubleIgnition
                | HGroupMoveKind::BombTripleIgnition
        )
    ) && candidate.action_coverage() >= 2;
    if !((candidate.purpose() == CluePurpose::Play && candidate.immediate_play())
        || is_multi_action_ignition)
        || convention_known_trash_discard(view, &analysis.inferences).is_none()
    {
        return None;
    }
    let plan = endgame_completion_plan(deductions, profile, analysis)?;
    if plan.unresolved_fives.is_empty()
        || usize::from(view.clue_tokens) < plan.unresolved_fives.len()
    {
        return None;
    }
    let Action::Clue { target, clue } = candidate.action else {
        return None;
    };
    let advances_plan = (is_multi_action_ignition
        && candidate.action_coverage()
            >= u8::try_from(plan.unresolved_fives.len()).unwrap_or(u8::MAX))
        || view.hands[target.index()].iter().any(|card| {
            plan.unresolved_fives.contains(&card.id)
                && card.identity.is_some_and(|identity| clue.matches(identity))
        });
    if !advances_plan {
        return None;
    }
    let best_clue_coverage = analysis_clue_candidates(deductions, profile, analysis)
        .iter()
        .map(|candidate| candidate.action_coverage())
        .max()
        .unwrap_or(0);
    if candidate.action_coverage() < best_clue_coverage {
        // The endgame-progress override exists to prefer completing the plan
        // over manufacturing an unnecessary clue token. It must not promote
        // a direct one-for-one 5 clue above an available convention line that
        // deterministically advances more of that same plan (for example, a
        // Trash Double Ignition of two final 5s).
        return None;
    }
    let (_, discard_score) = scored_discard_candidate(view, &analysis.inferences, profile)?;
    let ordinary_priority = 100 + i32::from(candidate.score());
    let progress_priority =
        TerminalPlanProgress::new(i32::from(discard_score), i32::from(candidate.score()))
            .encoded_priority();
    Some(ordinary_priority.max(progress_priority))
}

fn raw_h_group_action_priority(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    analysis: &HGroupAnalysis,
    action: Action,
) -> i32 {
    let inferred = &analysis.inferences;
    if let Action::Play(card) = action
        && inferred.cards.iter().any(|inference| {
            inference.card == card
                && inference.play_obligation == Some(HGroupPlayObligation::Forced)
        })
    {
        return 900;
    }
    if inferred
        .connection
        .is_some_and(|connection| action == Action::Play(connection.card))
    {
        return 800;
    }
    if inferred.connection.is_some_and(|connection| {
        paused_priority_play(deductions.view(), inferred, profile, connection)
            .is_some_and(|card| action == Action::Play(card))
    }) {
        return 825;
    }
    if inferred.connection.is_some_and(|connection| {
        analysis_clue_candidates(deductions, profile, analysis)
            .iter()
            .find(|candidate| candidate.action == action)
            .is_some_and(|candidate| {
                clue_can_defer_connection(
                    deductions.view(),
                    inferred,
                    profile,
                    connection,
                    candidate,
                )
            })
    }) {
        // Starting another valid connection is the strongest permitted
        // deferral: it creates multiple future plays while the demonstrated
        // layer remains safely parked.
        return 850;
    }
    if inferred
        .discard_now
        .iter()
        .any(|card| action == Action::Discard(*card))
    {
        return 600;
    }
    if inferred
        .playable_now
        .iter()
        .any(|card| action == Action::Play(*card))
    {
        // A guaranteed play should beat a non-urgent save (score 400), while
        // an emergency save for the very next player (450+) still preempts it.
        return 525;
    }
    if let Action::Discard(card) = action {
        if let Some((candidate, score)) =
            scored_discard_candidate(deductions.view(), inferred, profile)
        {
            if candidate == card {
                if let Some(priority) = super::draw_distribution::discard_priority(
                    deductions,
                    inferred,
                    profile,
                    analysis_clue_candidates(deductions, profile, analysis),
                    card,
                ) {
                    return priority;
                }
                if let Some(priority) =
                    deferred_teamwork_priority(deductions, profile, analysis, card)
                {
                    return priority;
                }
                return 100 + i32::from(score);
            }
        }
    }
    let clue_candidate = analysis_clue_candidates(deductions, profile, analysis)
        .iter()
        .find(|candidate| candidate.action == action);
    let clue_priority = clue_candidate.map_or(25, |candidate| {
        endgame_progress_priority(deductions, profile, analysis, candidate)
            .unwrap_or_else(|| 100 + i32::from(candidate.score()))
    });
    adjust_clue_priority(
        deductions,
        profile,
        analysis,
        action,
        clue_candidate,
        clue_priority,
    )
}

/// Applies play-obligation precedence after the candidate's within-tier clue
/// value has been computed. Keeping this phase separate prevents base clue
/// scoring from silently overriding a forced or already-promised action.
fn adjust_clue_priority(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    analysis: &HGroupAnalysis,
    action: Action,
    clue_candidate: Option<&CompiledClueAction>,
    clue_priority: i32,
) -> i32 {
    let inferred = &analysis.inferences;
    let has_forced_play = inferred.playable_now.iter().any(|playable| {
        inferred.cards.iter().any(|card| {
            card.card == *playable && card.play_obligation == Some(HGroupPlayObligation::Forced)
        })
    });
    if has_forced_play
        && clue_candidate.is_some_and(|candidate| {
            !hard_clue_obligation(deductions.view(), &analysis.replay, candidate)
        })
    {
        // A Bluff/Ejection play is a demonstrated convention obligation, not
        // an ordinary exact play that may be parked for a more efficient clue.
        // Only a genuinely hard Fix or endangered Save can interrupt it.
        clue_priority.min(500)
    } else if !inferred.playable_now.is_empty()
        && clue_candidate.is_some_and(|candidate| {
            candidate_can_preempt_current_play(candidate, inferred, &analysis.replay)
                && (!completed_connection_focus_is_due(inferred)
                    || candidate_is_lie_component_finesse(deductions.view(), profile, candidate))
        })
    {
        // A semantically strong setup clue can occupy several teammates while
        // the giver's exact play remains safely parked. Treating every known
        // play as forced made this multi-play line disappear from planning.
        let preemption_value =
            if clue_candidate.is_some_and(|candidate| candidate.preserves_visible_continuation()) {
                let Action::Clue { target, clue } = action else {
                    unreachable!("only clues have convention candidates")
                };
                i32::try_from(
                    deductions.view().hands[target.index()]
                        .iter()
                        .filter_map(|card| card.identity)
                        .filter(|identity| clue.matches(*identity))
                        .filter(|identity| is_eventually_useful(deductions.view(), *identity))
                        .count(),
                )
                .unwrap_or(i32::MAX - 550)
            } else {
                i32::from(matches!(
                    action,
                    Action::Clue {
                        clue: Clue::Suit(_),
                        ..
                    }
                ))
            };
        clue_priority.max(550 + preemption_value)
    } else if !inferred.playable_now.is_empty()
        && clue_candidate.is_some_and(|candidate| {
            !clue_preempts_play_obligation(deductions.view(), &analysis.replay, candidate)
        })
    {
        // An Occupied player normally takes their promised play. This applies
        // to a valuable ordinary 2 Save for the next player as well as to an
        // off-turn clue; only a genuinely urgent Save or immediate occupancy
        // clue may preempt the obligation.
        clue_priority.min(500)
    } else {
        clue_priority
    }
}

/// Values passing the final clue token when the next player can use the
/// recovered token for a strictly more efficient convention line.
///
/// This is a one-action symbolic continuation, not a hidden-world rollout.
/// The current player does not know the discarded identity, so every identity
/// in its logical domain is projected. Deferral is rewarded only when the
/// next player has a convention-valid clue with greater action coverage in
/// every branch.
fn deferred_teamwork_priority(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    analysis: &HGroupAnalysis,
    discard: CardId,
) -> Option<i32> {
    let source = deductions.view();
    if source.clue_tokens != 1 || !rule_enabled(profile, HGroupRuleId::SpecialFinesses) {
        return None;
    }
    let current_candidates = analysis_clue_candidates(deductions, profile, analysis);
    let current_coverage = current_candidates
        .iter()
        .map(deferred_teamwork_action_count)
        .max()
        .unwrap_or(0);
    if current_coverage < 2 {
        return None;
    }
    let identities = deductions.possible_identities(discard)?;
    if identities.is_empty() {
        return None;
    }
    let mut guaranteed_future_coverage = u8::MAX;
    for identity in identities.iter() {
        let after = ProspectiveTransition::discard(source, source.observer, discard, identity);
        let next = after.current_player;
        let (next_deductions, next_replay) = PerspectiveProjector::new(&after, profile)
            .project(next, PerspectiveDepth::NestedRecipients)?;
        let best_future_coverage =
            h_group_clue_candidates_from_replay(&next_deductions, profile, &next_replay)
                .iter()
                .map(deferred_teamwork_action_count)
                .max()
                .unwrap_or(0);
        guaranteed_future_coverage = guaranteed_future_coverage.min(best_future_coverage);
    }
    let extra_actions = guaranteed_future_coverage.checked_sub(current_coverage)?;
    if extra_actions == 0 {
        return None;
    }
    let best_immediate_clue = current_candidates
        .iter()
        .map(|candidate| 100 + i32::from(candidate.score()))
        .max()
        .unwrap_or(0);
    Some(best_immediate_clue + 40 * i32::from(extra_actions))
}

/// Counts only a convention-established multi-action line when deciding
/// whether to spend a turn manufacturing a clue token. Projection closure can
/// expose incidental future actions (including conclusions drawn later from a
/// declined alternative), but an ordinary one-for-one Play Clue is still a
/// one-action comparison for Teamwork deferral.
fn deferred_teamwork_action_count(candidate: &CompiledClueAction) -> u8 {
    if candidate.connection_steps() > 0 {
        candidate.action_coverage()
    } else {
        u8::from(candidate.action_coverage() > 0)
    }
}

/// Once teammates have demonstrated a connection for a clue, its focus is a
/// due convention response rather than an ordinary exact play that can be
/// parked for a new setup.
fn completed_connection_focus_is_due(inferred: &HGroupInferences) -> bool {
    !inferred.completed_connection_focuses.is_empty()
}

pub(crate) fn h_group_predictable_action(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
) -> Option<Action> {
    let analysis = build_h_group_analysis(deductions, profile);
    h_group_predictable_action_from_analysis(deductions, profile, &analysis)
}

fn h_group_predictable_action_from_analysis(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    analysis: &HGroupAnalysis,
) -> Option<Action> {
    analyze_h_group_actions_from_analysis(deductions, profile, analysis).predictable
}

fn paused_priority_play(
    view: &PlayerView,
    inferred: &HGroupInferences,
    profile: HGroupProfile,
    connection: HGroupConnection,
) -> Option<CardId> {
    if !connection_layer_demonstrated(view, inferred, profile, connection) {
        return None;
    }

    // Once an unrelated card has publicly demonstrated a Layered Finesse,
    // H-Group permits the player to pause the remaining layer for a newer,
    // explicit Play/Load clue. The newest such promise controls the pause.
    inferred.clues.iter().rev().find_map(|clue| {
        (clue.target == view.observer
            && clue.focus != connection.card
            && matches!(clue.kind, HGroupClueKind::Play | HGroupClueKind::PlayOrSave)
            && inferred.playable_now.contains(&clue.focus))
        .then_some(clue.focus)
    })
}

/// Selects the play an observer is conventionally due to make before any
/// hidden identity is revealed.
///
/// Priority can temporarily park an older, demonstrated connection when a
/// newer explicit Play Clue gives the player a different playable focus. Any
/// prospective evaluator that advances an observer's line must use this same
/// ordering instead of assuming that `connection` is always the next action.
pub(super) fn preferred_due_play_card(
    view: &PlayerView,
    inferred: &HGroupInferences,
    profile: HGroupProfile,
) -> Option<CardId> {
    inferred
        .playable_now
        .iter()
        .copied()
        .find(|playable| {
            inferred.cards.iter().any(|card| {
                card.card == *playable && card.play_obligation == Some(HGroupPlayObligation::Forced)
            })
        })
        .or_else(|| {
            inferred.connection.and_then(|connection| {
                paused_priority_play(view, inferred, profile, connection).or(Some(connection.card))
            })
        })
        .or_else(|| {
            ordered_playable_cards(view, inferred, profile)
                .first()
                .copied()
        })
}

fn connection_layer_demonstrated(
    _view: &PlayerView,
    inferred: &HGroupInferences,
    _profile: HGroupProfile,
    connection: HGroupConnection,
) -> bool {
    inferred.demonstrated_connections.contains(&connection.card)
}

fn clue_can_defer_connection(
    view: &PlayerView,
    inferred: &HGroupInferences,
    profile: HGroupProfile,
    connection: HGroupConnection,
    candidate: &CompiledClueAction,
) -> bool {
    connection_layer_demonstrated(view, inferred, profile, connection)
        && candidate.can_defer_demonstrated_layer()
}

fn legal_connection_actions(
    view: &PlayerView,
    replay: &HGroupState,
    connection: HGroupConnection,
    paused_priority: Option<CardId>,
    layer_demonstrated: bool,
    clue_candidates: &[CompiledClueAction],
    legal_actions: &[Action],
) -> Option<Vec<Action>> {
    let required_fixes = clue_candidates
        .iter()
        .filter(|candidate| candidate.purpose() == CluePurpose::Fix)
        .map(|candidate| candidate.action)
        .collect::<Vec<_>>();
    if !required_fixes.is_empty() {
        return Some(required_fixes);
    }
    let forced_plays = view.hands[view.observer.index()]
        .iter()
        .filter(|card| replay.cards.forced_playable.contains(&card.id))
        .map(|card| Action::Play(card.id))
        .filter(|action| legal_actions.contains(action))
        .collect::<Vec<_>>();
    if !forced_plays.is_empty() {
        let mut actions = clue_candidates
            .iter()
            .filter(|candidate| hard_clue_obligation(view, replay, candidate))
            .map(|candidate| candidate.action)
            .chain(forced_plays)
            .collect::<Vec<_>>();
        actions.dedup();
        return Some(actions);
    }
    let mut actions = clue_candidates
        .iter()
        .filter(|candidate| {
            clue_preempts_play_obligation(view, replay, candidate)
                || (layer_demonstrated && candidate.can_defer_demonstrated_layer())
        })
        .map(|candidate| candidate.action)
        .chain(paused_priority.map(Action::Play))
        .chain(core::iter::once(Action::Play(connection.card)))
        .collect::<Vec<_>>();
    actions.dedup();
    actions.retain(|action| legal_actions.contains(action));
    (!actions.is_empty()).then_some(actions)
}

fn clue_preempts_play_obligation(
    view: &PlayerView,
    replay: &HGroupState,
    candidate: &CompiledClueAction,
) -> bool {
    hard_clue_obligation(view, replay, candidate)
        || (candidate.target() == next_player(view.current_player, view.hands.len())
            && candidate.is_urgent_for_next_player()
            && (!candidate.is_urgent_save()
                || !target_is_occupied(view, replay, candidate.target())))
}

fn target_is_occupied(view: &PlayerView, replay: &HGroupState, target: PlayerId) -> bool {
    replay.pending_connections.iter().any(|connection| {
        connection.actor == target
            && replay.pending_connections.is_active(connection)
            && is_playable_now(view, connection.expected)
    }) || replay.hands[target.index()].iter().any(|card| {
        replay.cards.already_playing.contains(card)
            && identity_of(view, *card).is_some_and(|identity| is_playable_now(view, identity))
    })
}

/// Hard clue obligations are distinct from clues that are merely permitted
/// to preempt a play. A strong immediate Play Clue to the next player may be
/// considered alongside an occupied player's action, but only a Fix or an
/// at-risk critical Save excludes that action from planning altogether.
fn hard_clue_obligation(
    view: &PlayerView,
    replay: &HGroupState,
    candidate: &CompiledClueAction,
) -> bool {
    if candidate.purpose() == CluePurpose::Fix {
        return true;
    }
    let player_count = view.hands.len();
    if target_is_occupied(view, replay, candidate.target()) {
        // An urgent Save preempts only a discard that can actually happen on
        // the target's next turn. A player already bound to play cannot
        // discard their chop, leaving another full turn cycle to save it.
        // Source: https://hanabi.github.io/level-1/#save-principle
        return false;
    }
    let target_distance =
        (candidate.target().index() + player_count - view.current_player.index()) % player_count;
    let every_intervening_player_is_occupied = (1..target_distance).all(|distance| {
        let player = PlayerId::new(
            u8::try_from((view.current_player.index() + distance) % player_count)
                .expect("standard Hanabi has at most five players"),
        );
        replay.pending_connections.iter().any(|connection| {
            connection.actor == player
                && replay.pending_connections.is_active(connection)
                && is_playable_now(view, connection.expected)
        })
    });
    candidate.is_urgent_save() && every_intervening_player_is_occupied
}

#[allow(clippy::too_many_lines)]
#[cfg(test)]
pub(crate) fn select_h_group_action(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
) -> Option<Action> {
    let analysis = build_h_group_analysis(deductions, profile);
    select_h_group_action_from_analysis(deductions, profile, &analysis)
}

fn select_h_group_action_from_analysis(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    analysis: &HGroupAnalysis,
) -> Option<Action> {
    let view = deductions.view();
    let decision = analyze_h_group_actions_from_analysis(deductions, profile, analysis);
    let last_strike_inferences = (view.strikes >= 2).then(|| analysis.inferences.clone());
    let safe = |action: &Action| match action {
        Action::Play(card) => {
            deductions
                .possible_identities(*card)
                .is_some_and(|identities| {
                    !identities.is_empty()
                        && identities
                            .iter()
                            .all(|identity| is_playable_now(view, identity))
                })
                || last_strike_inferences
                    .as_ref()
                    .is_some_and(|inferred| inferred.playable_now.contains(card))
        }
        Action::Discard(_) | Action::Clue { .. } => true,
    };
    if view.strikes >= 2 {
        if let Some(action) = decision
            .preferred
            .filter(|action| {
                h_group_planning_action_safe(deductions, profile, *action) && safe(action)
            })
            .or_else(|| {
                decision
                    .actions
                    .iter()
                    .map(|analysis| analysis.action)
                    .find(|action| {
                        h_group_planning_action_safe(deductions, profile, *action) && safe(action)
                    })
            })
        {
            return Some(action);
        }
        if view.clue_tokens < MAX_CLUE_TOKENS {
            let inferred = last_strike_inferences
                .as_ref()
                .expect("two-strike inference was initialized");
            let own_hand = &view.hands[view.observer.index()];
            let gotten = inferred.gotten();
            let known_trash = convention_known_trash_discard(view, inferred);
            if let Some(discard) = known_trash
                .or_else(|| {
                    inferred.chops[view.observer.index()].filter(|card| !inferred.is_saved(*card))
                })
                .filter(|card| !inferred.is_saved(*card))
                .or_else(|| {
                    own_hand
                        .iter()
                        .map(|card| card.id)
                        .find(|card| !gotten.contains(card) && !inferred.is_saved(*card))
                })
                .or_else(|| {
                    own_hand
                        .iter()
                        .map(|card| card.id)
                        .find(|card| !inferred.is_saved(*card))
                })
            {
                return Some(Action::Discard(discard));
            }
            return crate::ConventionAgnosticPolicy
                .select_action(deductions)
                .ok();
        }
        return view
            .legal_actions()
            .into_iter()
            .find(|action| matches!(action, Action::Clue { .. }));
    }
    if let Some(action) = decision.preferred {
        return Some(action);
    }

    if view.clue_tokens == MAX_CLUE_TOKENS {
        return view
            .legal_actions()
            .into_iter()
            .find(|action| matches!(action, Action::Clue { .. }));
    }
    crate::ConventionAgnosticPolicy
        .select_action(deductions)
        .ok()
}
