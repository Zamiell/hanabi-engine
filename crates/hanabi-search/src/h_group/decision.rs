use crate::ConventionPolicyTier;

use super::{
    Action, ActionPreference, ActionSchedule, BeliefConstraints, Card, CardId, CardSet, Clue,
    CluePurpose, CompiledClueAction, CompiledHGroupAction, ConventionActionReason,
    ConventionConstraintGraph, ConventionConstraints, ConventionRequirementKind, HGroupActionKind,
    HGroupActionSet, HGroupClueKind, HGroupConnection, HGroupConnectionKind,
    HGroupConnectionPromise, HGroupIdentityStatus, HGroupInferences, HGroupMoveKind, HGroupPhase,
    HGroupPlayObligation, HGroupProfile, HGroupRuleId, HGroupState, IdentitySet, LogicalDeductions,
    MAX_CLUE_TOKENS, ObservedEvent, OnceLock, PerspectiveDepth, PerspectiveProjector, PlayerId,
    PlayerView, ProspectiveTransition, Rank, RejectedConventionAction, Suit,
    TeamConventionSnapshot, TerminalPlanProgress, chop, convention_card_inferences,
    finesse_position, h_group_clue_candidates_from_replay, h_group_phase,
    h_group_rejected_clues_from_replay, identity_of, infer_clue_to_self, is_convention_trash,
    is_critical_save_identity, is_eventually_useful, is_playable_now, next_player,
    ordered_playable_cards, owner_knowledge_read_model, projected_h_group_replay,
    prospective_clue_primary_kind, prospective_clue_view, prospective_play_has_unsafe_inference,
    prospective_team_clue_signal_kinds, replay_h_group, rule_enabled, was_clued_before,
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
    unresolved_cards: CardSet,
    known_plays: Vec<(PlayerId, Card)>,
}

const KNOWN_PLAY_PRIORITY: i32 = 525;

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
            cards: pending
                .cards
                .iter()
                .copied()
                .filter(|card| {
                    super::model::connection_candidate_is_eligible(
                        pending.kind,
                        pending.expected,
                        *card,
                        &pending.cards,
                        &inferred.cards,
                        deductions,
                    )
                })
                .collect(),
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
        // Fully protected is not locked when a known-trash discard exists.
        // Do not turn a Trash Chop Move into an invented blind play.
        && convention_known_trash_discard(view, &inferred).is_none()
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
                // Anxiety explicitly resolves ambiguity by playability,
                // then leftmost position. Overlapping identity domains must
                // not suppress that convention-provided choice.
                // https://hanabi.github.io/level-9/#the-anxiety-play-forcing-a-locked-player-to-play
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
                .min_by_key(|pending| {
                    // A fresh blind response from Bluff Seat must resolve
                    // immediately, ahead of an ordinary clued Prompt. It
                    // cannot jump an older Finesse (Queued Bluffs are illegal).
                    // https://hanabi.github.io/level-11/#lie-principle
                    let immediate_blind_response = rule_enabled(profile, HGroupRuleId::Bluffs)
                        && pending.kind == HGroupConnectionKind::Finesse
                        && view.history.last().is_some_and(|entry| {
                            matches!(entry.event, ObservedEvent::Clued { giver, .. }
                                if next_player(giver, view.hands.len()) == view.observer)
                                && replay
                                    .pending_connections
                                    .was_created_on(pending, entry.turn)
                        })
                        && !super::bluff::bluff_is_queued(
                            &replay.pending_connections,
                            view.observer,
                            Some(pending.focus),
                        );
                    if immediate_blind_response {
                        0
                    } else {
                        match pending.kind {
                            HGroupConnectionKind::Prompt => 1,
                            HGroupConnectionKind::Finesse => 2,
                        }
                    }
                })
                .and_then(|pending| {
                    // A disjunctive Prompt is an ordered obligation: play its newest
                    // candidate first, then continue left-to-right if that card was
                    // merely playable. Ambiguous per-card notes cannot safely skip
                    // a candidate because Good Touch creates correlated alternatives
                    // ("if the focus is R1 this card is not R1", and vice versa).
                    // An exact known different identity does rule a candidate out.
                    pending
                        .cards
                        .iter()
                        .copied()
                        .find(|card| {
                            super::model::connection_candidate_is_eligible(
                                pending.kind,
                                pending.expected,
                                *card,
                                &pending.cards,
                                &inferred.cards,
                                deductions,
                            )
                        })
                        .map(|card| (pending, card))
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
    inferred.projection_requirements = super::projection_requirements::compile(view, &inferred);
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
    let fresh_trash_chop_move_focus = fresh_trash_chop_move_focus(view, &analysis.replay);
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
        // Use the same priority model exposed to planning and diagnostics.
        // A separate legacy sort used to demote even forced plays for an
        // apparent urgent Save while reporting those plays as priority 900.
        actions.sort_by_cached_key(|action| {
            core::cmp::Reverse(raw_h_group_action_priority(
                deductions, profile, analysis, *action,
            ))
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
    let gotten = inferred.clued_or_promised();
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
            identity_of(view, **card).is_some_and(|identity| {
                identity.rank == Rank::Five || is_critical_save_identity(view, identity)
            })
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
    let action_cache = &analysis.action_set;
    let pass_back = pass_back_analysis(deductions, profile, analysis);
    let analysis = pass_back.as_ref().unwrap_or(analysis);
    let inferred = &analysis.inferences;
    let mut clue_candidates = analysis_clue_candidates(deductions, profile, analysis).to_vec();
    clue_candidates.sort_by_key(|candidate| core::cmp::Reverse(candidate.score()));

    let ordered = ordered_h_group_actions_from_analysis(deductions, profile, analysis);
    let play_order = ordered_playable_cards(deductions.view(), inferred, profile);
    let analyzed = ordered
        .iter()
        .copied()
        .map(|action| {
            let clue = clue_candidates
                .iter()
                .find(|candidate| candidate.action == action);
            let priority = raw_h_group_action_priority(deductions, profile, analysis, action);
            let terminal_progress = clue
                .and_then(|candidate| endgame_progress(deductions, profile, analysis, candidate));
            CompiledHGroupAction {
                action,
                kind: classify_h_group_action(action, inferred, clue),
                preference: ActionPreference::new(
                    terminal_progress.map_or(priority, TerminalPlanProgress::within_category),
                    terminal_progress.is_some(),
                )
                .with_play_order(match action {
                    Action::Play(card) if rule_enabled(profile, HGroupRuleId::Priority) => {
                        play_order.iter().position(|candidate| *candidate == card)
                    }
                    _ => None,
                }),
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
    let early_saves = deferred_early_saves(deductions.view(), analysis, &clue_candidates);
    for candidate in &mut analyzed {
        let policy_tier = if constraints.kind().is_some() {
            ConventionPolicyTier::Required
        } else if candidate.kind == HGroupActionKind::Fallback {
            ConventionPolicyTier::Fallback
        } else if early_saves.contains(&candidate.action) {
            ConventionPolicyTier::Deferred
        } else {
            ConventionPolicyTier::Admitted
        };
        candidate.preference.set_policy_tier(policy_tier);
    }
    let (ranked_preferred, _constraint_reason) = derive_preferred_action(
        deductions,
        profile,
        &clue_candidates,
        &analyzed,
        &constraints,
    );
    // Keep the fast continuation policy consistent with its prediction.
    // Root search separately distinguishes a policy prediction from a hard
    // convention requirement in analyze_h_group_convention.
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
    let _ = action_cache.set(decision.clone());
    decision
}

/// Make a known play or queue a productive Play Clue before a single-card Save on
/// someone already occupied by a promised play.
/// The Save remains legal and projected, but has no immediate deadline.
/// Human-reviewed p4v0s2 turns 3 and 5. The play need not be on chop, nor
/// must its recipient be idle: queuing a continuation is productive too.
/// <https://hanabi.github.io/beginner/other-general-strategy/#give-play-clues-over-save-clues>
fn deferred_early_saves(
    view: &PlayerView,
    analysis: &HGroupAnalysis,
    clues: &[CompiledClueAction],
) -> Vec<Action> {
    let schedule = ActionSchedule::from_replay(view, &analysis.replay);
    let productive_play = !analysis.inferences.playable_now.is_empty()
        || clues
            .iter()
            .any(|candidate| candidate.purpose() == CluePurpose::Play);
    if !productive_play {
        return Vec::new();
    }
    clues.iter().filter(|candidate| {
        candidate.is_save()
            && !candidate.is_urgent_save()
            && schedule.occupied_after_clue(view, candidate.target())
            && matches!(candidate.action, Action::Clue { target, clue }
                if view.hands[target.index()].iter().filter(|card| card.identity.is_some_and(|identity| clue.matches(identity))).count() == 1)
    }).map(|candidate| candidate.action).collect()
}

fn pass_back_analysis(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    analysis: &HGroupAnalysis,
) -> Option<HGroupAnalysis> {
    let connection = analysis.inferences.connection?;
    if connection.kind != HGroupConnectionKind::Finesse
        || !rule_enabled(profile, HGroupRuleId::SpecialFinesses)
        || !super::prospective::assumed_play_has_unsafe_inference(
            deductions.view(),
            profile,
            connection.card,
            connection.identity,
        )
    {
        return None;
    }
    // AFPB suspends an unsafe blind-play obligation, not the underlying
    // identity promise. Choose an unrelated legal action instead; merely
    // lowering the play's score cannot override ConnectionResponse.
    // https://hanabi.github.io/extras/special-finesses/#the-ambiguous-finesse-pass-back-afpb
    let mut adjusted = analysis.clone();
    adjusted.inferences.connection = None;
    adjusted
        .inferences
        .playable_now
        .retain(|card| *card != connection.card);
    Some(adjusted)
}

/// Constructs every convention-facing result from one history replay and one
/// inference pass.
pub(crate) struct HGroupConventionDecision {
    pub(crate) clue_explanations: Vec<crate::ClueExplanation>,
    pub(crate) inferences: HGroupInferences,
    pub(crate) actions: Vec<crate::ConventionAction>,
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
        .map(|candidate| crate::ConventionAction {
            action: candidate.action,
            preference: candidate.preference,
            reason: convention_action_reason(candidate.kind),
        })
        .collect();
    let preferred = select_h_group_action_from_analysis(deductions, profile, &analysis);
    // A rollout-policy prediction is not a hard restriction on root search.
    // An ordinary known play may be deferred for a productive clue; only
    // actual convention requirements (or a single admitted action) can
    // prevent the planner from comparing the alternatives.
    let forced = actions.predictable.filter(|action| {
        actions.actions.len() == 1
            || matches!(action, Action::Play(card) if
            analysis.inferences.cards.iter().any(|note| {
                    note.card == *card
                        && note.play_obligation == Some(HGroupPlayObligation::Forced)
                }))
            || actions.actions.iter().any(|candidate| {
                candidate.action == *action
                    && candidate.preference.policy_tier() == ConventionPolicyTier::Required
            })
    });
    let belief_constraints =
        ConventionConstraintGraph::from_replay(deductions, &analysis.replay, &analysis.inferences)
            .into_belief_constraints();
    HGroupConventionDecision {
        clue_explanations: if crate::diagnostics::enabled() {
            analysis_clue_candidates(deductions, profile, &analysis)
                .iter()
                .map(|candidate| {
                    let mut explanation = candidate.explanation();
                    explanation.interpretation =
                        crate::diagnostics::meaning(deductions.view(), candidate.action);
                    explanation
                })
                .collect()
        } else {
            Vec::new()
        },
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
    if emergency_discard_is_required(view, inferred, replay, profile) {
        return ConventionConstraints::require(
            ConventionRequirementKind::UrgentProtection,
            analyzed.iter().filter_map(|candidate| {
                matches!(candidate.action, Action::Discard(_)).then_some(candidate.action)
            }),
        );
    }
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
                    // Keep every clue satisfying this protection obligation,
                    // including advanced moves such as a 5 Color Ejection.
                    // The first matching Save is not the only valid means.
                    candidate.action == urgent.action
                        || (candidate.target() == urgent.target()
                            && (candidate.immediate_play()
                                || hard_clue_obligation(view, replay, candidate)))
                })
                .map(|candidate| candidate.action),
        );
    }
    // A queued connection can coexist with an obligation to clue (for
    // example while its connector is not yet playable). Match the ordering
    // used by ordered_h_group_actions_from_analysis: do not remove every
    // required clue merely because there is also a pending connection.
    if inferred.must_clue.contains(&view.observer) && !clues.is_empty() {
        return ConventionConstraints::require(
            ConventionRequirementKind::MustClue,
            clues.iter().map(|candidate| candidate.action),
        );
    }
    if inferred.connection.is_some() {
        return ConventionConstraints::require(
            ConventionRequirementKind::ConnectionResponse,
            analyzed
                .iter()
                .filter(|candidate| {
                    candidate.kind == HGroupActionKind::Connection
                        || connection_response_allowed(
                            view,
                            inferred,
                            profile,
                            clues,
                            candidate.action,
                        )
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

/// At zero clues an ordinary known play cannot be chosen over the only
/// available protection for the next player's endangered chop. Recognition
/// already understands Scream/Shout Discards; planning must also choose them.
/// <https://hanabi.github.io/level-7/#the-scream-discard-chop-move-sdcm>
pub(super) fn emergency_discard_is_required(
    view: &PlayerView,
    inferred: &HGroupInferences,
    replay: &HGroupState,
    profile: HGroupProfile,
) -> bool {
    if view.clue_tokens != 0
        || !rule_enabled(profile, HGroupRuleId::EmergencyDiscards)
        || !inferred
            .playable_now
            .iter()
            .any(|card| replay.cards.explicitly_clued.contains(card))
    {
        return false;
    }
    // Completing a known playable 5 generates the token without sacrificing
    // a card. A Scream Discard is a last resort, not a mandatory substitute
    // for that play. Leave both actions to ordinary planning instead of
    // restricting the candidate set to discards. Use the actor's knowledge,
    // never the hidden actual identity or an assumed successful blind play.
    // https://hanabi.github.io/level-7/#the-scream-discard-chop-move-sdcm
    if inferred.cards.iter().any(|card| {
        inferred.playable_now.contains(&card.card)
            && !card.identities.is_empty()
            && card
                .identities
                .iter()
                .all(|identity| identity.rank == Rank::Five && is_playable_now(view, identity))
    }) {
        return false;
    }
    let target = next_player(view.current_player, view.hands.len());
    if emergency_chop_needs_protection(view, inferred, replay, profile, target)
        && !known_play_occupies_next_player(view, inferred, profile, target)
    {
        return true;
    }
    // Generation Discard: provide the token BEFORE the next player's turn,
    // so they can protect the player after them rather than merely discard
    // to generate a token too late. This also protects a needed 2 whose
    // predecessor would have been played by the action we are deferring.
    // https://hanabi.github.io/level-7/#the-generation-discard
    // User-reviewed p4v0s1 turn-20 projections: Alice generates for Bob to
    // clue Cathy instead of playing r1 and leaving Cathy to discard a 2.
    !target_is_occupied(view, replay, target)
        && projected_h_group_replay(view, profile, target).is_some_and(|(d, r)| {
            super::infer_h_group_from_replay(&d, r, profile)
                .playable_now
                .is_empty()
        })
        && emergency_chop_needs_protection(
            view,
            inferred,
            replay,
            profile,
            next_player(target, view.hands.len()),
        )
}

/// A Scream is unnecessary when playing the known predecessor gives the
/// endangered next player their own promised play. Evaluate the actual public
/// transition, including knowledge updates, without filling an unknown draw.
fn known_play_occupies_next_player(
    view: &PlayerView,
    inferred: &HGroupInferences,
    profile: HGroupProfile,
    target: PlayerId,
) -> bool {
    inferred.cards.iter().any(|note| {
        if !inferred.playable_now.contains(&note.card) || note.identities.len() != 1 {
            return false;
        }
        let identity = note.identities.iter().next().expect("singleton");
        if !is_playable_now(view, identity) {
            return false;
        }
        let after = ProspectiveTransition::play(view, view.observer, note.card, identity, true);
        projected_h_group_replay(&after, profile, target).is_some_and(|(d, r)| {
            let notes = super::infer_h_group_from_replay(&d, r, profile);
            !notes.playable_now.is_empty() && !notes.must_clue.contains(&target)
        })
    })
}

fn emergency_chop_needs_protection(
    view: &PlayerView,
    inferred: &HGroupInferences,
    replay: &HGroupState,
    profile: HGroupProfile,
    target: PlayerId,
) -> bool {
    if target_is_occupied(view, replay, target) {
        return false;
    }
    if projected_h_group_replay(view, profile, target).is_some_and(|(d, r)| {
        let target_notes = super::infer_h_group_from_replay(&d, r, profile);
        !target_notes.playable_now.is_empty()
            || convention_known_trash_discard(d.view(), &target_notes).is_some()
    }) {
        return false;
    }
    let Some(card) = inferred.chops[target.index()] else {
        return false;
    };
    let Some(identity) = identity_of(view, card) else {
        return false;
    };
    if !is_eventually_useful(view, identity) {
        return false;
    }
    let another_copy = view.hands.iter().flatten().any(|other| {
        other.id != card
            && (other.identity == Some(identity)
                || inferred.cards.iter().any(|note| {
                    note.card == other.id && note.identities == IdentitySet::singleton(identity)
                }))
    });
    identity.rank == Rank::Five
        || is_critical_save_identity(view, identity)
        || (!another_copy && (identity.rank == Rank::Two || is_playable_now(view, identity)))
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
                    && !transfer_delays_next_five(view, profile, candidate, identity, distance)
            })
            .then_some((candidate, identity))
    })
}

/// A transfer's token is not a compensating benefit when playing the card
/// instead lets the very next player finish the suit and refund that token.
/// No intervening player can need the token; transferring to a later seat
/// postpones both plays. This is a scheduling check, not a rank-based bonus.
/// <https://hanabi.github.io/level-10/#the-gentlemans-discard-gd>
fn transfer_delays_next_five(
    view: &PlayerView,
    profile: HGroupProfile,
    card: CardId,
    identity: Card,
    recipient_distance: usize,
) -> bool {
    if recipient_distance <= 1 || identity.rank != Rank::Four {
        return false;
    }
    let next = next_player(view.observer, view.hands.len());
    let five = Card::new(identity.suit, Rank::Five);
    if !view.hands[next.index()]
        .iter()
        .any(|held| held.identity == Some(five) && was_clued_before(view, view.turn, held.id))
    {
        return false;
    }
    let after = ProspectiveTransition::successful_play(view, view.observer, card, identity);
    let Some((deductions, _)) = PerspectiveProjector::new(&after, profile)
        .project(next, PerspectiveDepth::NestedRecipients)
    else {
        return false;
    };
    matches!(select_h_group_action(&deductions, profile), Some(Action::Play(due))
        if identity_of(view, due) == Some(five))
}

/// The convention supplies a safe discard even before literal domains prove
/// the focus trash. Share this scheduling fact with protection valuation.
pub(super) fn fresh_trash_chop_move_focus(
    view: &PlayerView,
    replay: &super::HGroupState,
) -> Option<CardId> {
    replay.clues.iter().rev().find_map(|clue| {
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
            && replay
                .signals
                .has_at_turn(clue.turn, HGroupMoveKind::TrashChopMove)
            && view.hands[view.observer.index()]
                .iter()
                .any(|card| card.id == clue.focus))
        .then_some(clue.focus)
    })
}

pub(super) fn convention_known_trash_discard(
    view: &PlayerView,
    inferred: &HGroupInferences,
) -> Option<CardId> {
    let gotten = inferred.clued_or_promised();
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

/// Remaining cards whose owners still need a clue to complete the stacks.
/// Every required identity must be visible or exactly known in the observer's
/// own hand. A missing connector disables this completion preference.
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
            let mut unresolved_cards = CardSet::default();
            let mut known_plays = Vec::new();
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
                        .flat_map(|(owner, hand)| {
                            hand.iter()
                                .filter(move |card| {
                                    card.identity == Some(identity)
                                        || (owner == view.observer.index()
                                            && analysis.inferences.cards.iter().any(|note| {
                                                note.card == card.id
                                                    && note.identities
                                                        == IdentitySet::singleton(identity)
                                            }))
                                })
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
                                    && (note.finessed
                                        || note.play_obligation.is_some()
                                        || note.identities == IdentitySet::singleton(identity))
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
                        owns_commitment.then_some(owner)
                    });
                    if let Some(owner) = committed {
                        known_plays.push((owner, identity));
                    } else {
                        unresolved_cards.insert(visible_copies[0].1);
                    }
                }
            }
            Some(EndgameCompletionPlan {
                unresolved_cards,
                known_plays,
            })
        })
        .as_ref()
}

/// A known-trash discard is dominated when it only creates a surplus token
/// while leaving an inevitable final Play Clue for the next teammate to give.
/// A safe Burn can also preserve the drawing clock for already-known plays.
/// <https://hanabi.github.io/level-8/#burning-end-game-stalling>
fn completion_without_discard(view: &PlayerView, cards: &[(PlayerId, Card)]) -> bool {
    if view.clue_tokens == 0 || cards.is_empty() {
        return false;
    }
    funded_completion_schedule(view, cards.to_vec(), Vec::new(), None)
}

fn burn_progress(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    analysis: &HGroupAnalysis,
    candidate: &CompiledClueAction,
) -> Option<TerminalPlanProgress> {
    let view = deductions.view();
    let plan = endgame_completion_plan(deductions, profile, analysis)?;
    if view.deck_size > view.hands.len()
        || !analysis.inferences.playable_now.is_empty()
        || !plan.unresolved_cards.is_empty()
        || !completion_without_discard(view, &plan.known_plays)
    {
        return None;
    }
    let (_, score) = scored_discard_candidate(view, &analysis.inferences, profile)?;
    Some(TerminalPlanProgress::new(
        i32::from(score),
        i32::from(candidate.score()),
    ))
}

/// A sufficient (not exhaustive) final schedule with no transfer or speculative
/// draws. Existing commitments play in stack order; unresolved cards require an
/// admitted direct Play Clue, and idle turns after all clues are given burn a
/// token. Charge every clue/burn and honor the final-round drawing clock.
///
/// This proves when a Gentleman's Discard's extra token has no remaining use,
/// so Clarity prefers playing the known card directly. Failure to prove this
/// leaves the transfer's ordinary value intact; it does not predict a loss.
/// <https://hanabi.github.io/level-6/#clarity-principle-part-1>
fn direct_play_completes_without_extra_token(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    analysis: &HGroupAnalysis,
    card: CardId,
) -> bool {
    let view = deductions.view();
    if !analysis.inferences.playable_now.contains(&card) {
        return false;
    }
    let Some(identity) = analysis.inferences.cards.iter().find_map(|note| {
        (note.card == card && note.identities.len() == 1)
            .then(|| note.identities.iter().next())
            .flatten()
    }) else {
        return false;
    };
    let Some(plan) = endgame_completion_plan(deductions, profile, analysis) else {
        return false;
    };
    let mut unannounced = Vec::new();
    for unresolved in &plan.unresolved_cards {
        let Some((owner, held)) = view.hands.iter().enumerate().find_map(|(owner, hand)| {
            hand.iter()
                .find(|held| held.id == *unresolved)
                .map(|held| (owner, held))
        }) else {
            return false;
        };
        let Some(identity) = held.identity else {
            return false;
        };
        // Reuse admission evidence, not the actual face alone, to establish
        // that a simple clue can get this outstanding card played.
        let can_clue = is_playable_now(view, identity) && analysis_clue_candidates(deductions, profile, analysis)
            .iter()
            .any(|candidate| {
                candidate.purpose() == CluePurpose::Play
                    && candidate.immediate_play()
                    && matches!(candidate.action, Action::Clue { target, clue }
                    if target.index() == owner && clue.matches(identity)
                        && super::prospective_clue_primary_interpretation(
                            view, profile, target, clue,
                            &view.hands[owner].iter().filter(|held| held.identity.is_some_and(|card| clue.matches(card))).map(|held| held.id).collect::<Vec<_>>()
                        ).is_some_and(|meaning| meaning.focus == *unresolved && meaning.play_identities.contains(identity)))
            });
        if !can_clue {
            return false;
        }
        unannounced.push((
            PlayerId::new(u8::try_from(owner).expect("player count")),
            identity,
        ));
    }
    funded_completion_schedule(view, plan.known_plays.clone(), unannounced, Some(identity))
}

/// Shared token/drawing-clock simulation for a direct play or an initial Burn.
/// Unknown draws never contribute a card or a clue refund to this certificate.
fn funded_completion_schedule(
    view: &PlayerView,
    mut plays: Vec<(PlayerId, Card)>,
    mut unannounced: Vec<(PlayerId, Card)>,
    initial_play: Option<Card>,
) -> bool {
    let mut heights = view.play_stacks.each_ref().map(Vec::len);
    let mut tokens = view.clue_tokens;
    let mut deck = view.deck_size;
    let mut final_turns = view.final_turns_remaining;
    let mut actor = view.current_player;
    let bound = (plays.len() + unannounced.len()) * view.hands.len() + 1;
    for step in 0..bound {
        if final_turns == Some(0) {
            return false;
        }
        let was_final = final_turns.is_some();
        let play = plays.iter().position(|(owner, candidate)| {
            *owner == actor
                && usize::from(candidate.rank.number()) == heights[candidate.suit.index()] + 1
                && (step != 0 || initial_play == Some(*candidate))
        });
        if let Some(index) = play {
            let (_, played) = plays.remove(index);
            heights[played.suit.index()] += 1;
            if plays.is_empty() && unannounced.is_empty() {
                return true;
            }
            if played.rank == Rank::Five {
                tokens = (tokens + 1).min(MAX_CLUE_TOKENS);
            }
            if deck > 0 {
                deck -= 1;
                if deck == 0 {
                    final_turns = Some(u8::try_from(view.hands.len()).expect("player count"));
                }
            }
        } else {
            if (step == 0 && initial_play.is_some()) || tokens == 0 {
                return false;
            }
            if let Some(index) = unannounced.iter().position(|(owner, _)| *owner != actor) {
                plays.push(unannounced.remove(index));
            } else if !unannounced.is_empty() {
                return false;
            }
            // Once all remaining plays are announced, an idle turn can burn.
            tokens -= 1;
        }
        if was_final {
            final_turns = final_turns.map(|turns| turns.saturating_sub(1));
        }
        actor = next_player(actor, view.hands.len());
    }
    false
}

fn endgame_progress(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    analysis: &HGroupAnalysis,
    candidate: &CompiledClueAction,
) -> Option<TerminalPlanProgress> {
    let view = deductions.view();
    if candidate.move_kind() == Some(HGroupMoveKind::Burn) {
        return burn_progress(deductions, profile, analysis, candidate);
    }
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
        // This override replaces an unnecessary token-generating discard,
        // not an already available play that advances the same final plan.
        // Reviewed p4v0s415 turn 45: playing p4 unlocks Alice's p5; the final
        // g5 clue must not categorically outrank that productive action.
        || !analysis.inferences.playable_now.is_empty()
    {
        return None;
    }
    let plan = endgame_completion_plan(deductions, profile, analysis)?;
    if plan.unresolved_cards.is_empty() || view.clue_tokens == 0 {
        return None;
    }
    // The aggregate Ignition count only certifies independent final 5s;
    // lower-rank dependencies require a move-by-move schedule instead.
    if is_multi_action_ignition
        && plan
            .unresolved_cards
            .iter()
            .any(|card| identity_of(view, *card).is_none_or(|identity| identity.rank != Rank::Five))
    {
        return None;
    }
    let Action::Clue { target, clue } = candidate.action else {
        return None;
    };
    let advances_plan = (is_multi_action_ignition
        && candidate.action_coverage()
            >= u8::try_from(plan.unresolved_cards.len()).unwrap_or(u8::MAX))
        || view.hands[target.index()].iter().any(|card| {
            plan.unresolved_cards.contains(&card.id)
                && card.identity.is_some_and(|identity| clue.matches(identity))
        });
    if !advances_plan {
        return None;
    }
    // Fund the sequence, not every remaining clue up front. One clue can
    // secure multiple 5s, and each completed 5 refunds a token before the
    // remaining one-for-one clues are needed. Never count an unseen 5.
    let secured_cards = if is_multi_action_ignition {
        plan.unresolved_cards.len()
    } else {
        view.hands[target.index()]
            .iter()
            .filter(|card| {
                plan.unresolved_cards.contains(&card.id)
                    && card.identity.is_some_and(|identity| clue.matches(identity))
            })
            .count()
    };
    let secured_fives = plan
        .unresolved_cards
        .iter()
        .filter(|card| {
            identity_of(view, **card).is_some_and(|identity| {
                identity.rank == Rank::Five
                    && (is_multi_action_ignition
                        || (view.hands[target.index()]
                            .iter()
                            .any(|held| held.id == **card)
                            && clue.matches(identity)))
            })
        })
        .count();
    let remaining_clues = plan.unresolved_cards.len().saturating_sub(secured_cards);
    if !super::ResourceSchedule::funds_final_fives(view.clue_tokens, secured_fives, remaining_clues)
    {
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
    Some(TerminalPlanProgress::new(
        i32::from(discard_score),
        i32::from(candidate.score()),
    ))
}

/// Semantic interruptions of a pending connection. Admissibility must never
/// be inferred from the magnitude of a heuristic score.
fn connection_response_allowed(
    view: &PlayerView,
    inferred: &HGroupInferences,
    profile: HGroupProfile,
    clues: &[CompiledClueAction],
    action: Action,
) -> bool {
    if let Action::Play(card) = action {
        if inferred.cards.iter().any(|inference| {
            inference.card == card
                && inference.play_obligation == Some(HGroupPlayObligation::Forced)
        }) {
            return true;
        }
    }
    inferred.connection.is_some_and(|connection| {
        paused_priority_play(view, inferred, profile, connection)
            .is_some_and(|card| action == Action::Play(card))
            || clues.iter().any(|candidate| {
                candidate.action == action
                    && clue_can_defer_connection(view, inferred, profile, connection, candidate)
            })
    })
}

fn raw_h_group_action_priority(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    analysis: &HGroupAnalysis,
    action: Action,
) -> i32 {
    let inferred = &analysis.inferences;
    if let Action::Play(card) = action {
        if inferred.cards.iter().any(|inference| {
            inference.card == card
                && inference.play_obligation == Some(HGroupPlayObligation::Forced)
        }) {
            return 900;
        }
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
        return KNOWN_PLAY_PRIORITY;
    }
    if let Action::Discard(card) = action {
        if let Some((candidate, score)) =
            scored_discard_candidate(deductions.view(), inferred, profile)
        {
            if candidate == card {
                if direct_play_completes_without_extra_token(deductions, profile, analysis, card) {
                    // A valid transfer remains a candidate, but a surplus
                    // token cannot outweigh the simpler funded completion.
                    return KNOWN_PLAY_PRIORITY - 1;
                }
                if let Some(priority) =
                    early_game_clue_handoff_priority(deductions, profile, analysis, card)
                {
                    return priority;
                }
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
        endgame_progress(deductions, profile, analysis, candidate).map_or_else(
            || 100 + i32::from(candidate.score()),
            TerminalPlanProgress::encoded_priority,
        )
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
            (candidate_can_preempt_current_play(candidate, inferred, &analysis.replay)
                || (candidate.expiring_multi_card_opportunity()
                    && can_park_surplus_five(deductions.view(), inferred)))
                && (!completed_connection_focus_is_due(inferred)
                    || candidate_is_lie_component_finesse(deductions.view(), profile, candidate))
        })
    {
        if clue_candidate.is_some_and(|candidate| {
            play_preserves_clue_for_free_teammate(deductions, profile, analysis, candidate)
        }) {
            return clue_priority.min(KNOWN_PLAY_PRIORITY - 1);
        }
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

/// Divide work before parking a known play for a setup clue. Moving the clue
/// one seat later is safe only when its recipient-relative consequences are
/// preserved and the next player has neither an obligation nor a better clue.
/// This checks a symbolic successful play; the new draw stays unknown.
/// <https://hanabi.github.io/beginner/other-general-strategy/#check-team-chops>
fn play_preserves_clue_for_free_teammate(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    analysis: &HGroupAnalysis,
    candidate: &CompiledClueAction,
) -> bool {
    let source = deductions.view();
    let next = next_player(source.observer, source.hands.len());
    if candidate.purpose() != CluePurpose::Play
        || candidate.target() == next
        || hard_clue_obligation(source, &analysis.replay, candidate)
    {
        return false;
    }
    let Some(outcome) = super::strategic_value::scheduled_clue_outcome(source, profile, candidate)
    else {
        return false;
    };
    if outcome.public_actions.is_empty()
        || outcome
            .public_actions
            .iter()
            .any(|action| action.owner == next)
    {
        return false;
    }
    analysis.inferences.cards.iter().any(|card| {
        if !analysis.inferences.playable_now.contains(&card.card) || card.identities.len() != 1 {
            return false;
        }
        let identity = card.identities.iter().next().expect("singleton identity");
        if !is_playable_now(source, identity) {
            return false;
        }
        let after =
            ProspectiveTransition::successful_play(source, source.observer, card.card, identity);
        let Some((future_deductions, future_replay)) = PerspectiveProjector::new(&after, profile)
            .project(next, PerspectiveDepth::NestedRecipients)
        else {
            return false;
        };
        let future_inferred =
            infer_h_group_from_replay(&future_deductions, future_replay.clone(), profile);
        if !super::ActionWindow::from_inferences(future_deductions.view(), &future_inferred)
            .is_free()
        {
            return false;
        }
        let future =
            h_group_clue_candidates_from_replay(&future_deductions, profile, &future_replay);
        let Some(same) = future.iter().find(|later| later.action == candidate.action) else {
            return false;
        };
        if future.iter().any(|other| {
            hard_clue_obligation(future_deductions.view(), &future_replay, other)
                || other.action_coverage() > same.action_coverage()
        }) {
            return false;
        }
        let Some(later) =
            super::strategic_value::scheduled_clue_outcome(future_deductions.view(), profile, same)
        else {
            return false;
        };
        later.public_actions == outcome.public_actions
            && later.owner_actions == outcome.owner_actions
            && later.protected_cards == outcome.protected_cards
            && later.new_connections == outcome.new_connections
    })
}

/// A token-refunding play can wait for an expiring efficient clue when the
/// existing token supply already covers that clue and visible urgent needs.
/// This is scheduling, not a change to level 25's order among actual plays.
/// No hidden identity or future draw is used to declare a refund unnecessary.
/// <https://hanabi.github.io/level-25/#the-priority-prompt--the-priority-finesse>
pub(super) fn can_park_surplus_five(source: &PlayerView, inferred: &HGroupInferences) -> bool {
    let critical_saves = inferred
        .chops
        .iter()
        .flatten()
        .filter(|card| {
            super::identity_of(source, **card)
                .is_some_and(|identity| super::is_critical_save_identity(source, identity))
        })
        .count();
    let reserve = super::ResourceSchedule::reserve(
        u8::try_from(critical_saves).unwrap_or(u8::MAX),
        u8::try_from(inferred.must_clue.len()).unwrap_or(u8::MAX),
        false,
        true,
    );
    source.clue_tokens >= reserve
        && !inferred.playable_now.is_empty()
        && inferred.playable_now.iter().all(|playable| {
            inferred.cards.iter().any(|card| {
                card.card == *playable
                    && !card.identities.is_empty()
                    && card
                        .identities
                        .iter()
                        .all(|identity| identity.rank == Rank::Five)
            })
        })
}

/// Prefer swapping an interchangeable clue and discard when only the current
/// player can discard without ending Early Game. This is a scheduling
/// preference, not permission to discard unknown cards or delay a repair.
/// [Ending the Early Game](https://hanabi.github.io/level-9/#ending-the-early-game).
fn early_game_clue_handoff_priority(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    analysis: &HGroupAnalysis,
    discard: CardId,
) -> Option<i32> {
    let source = deductions.view();
    super::ResourceSchedule::discard_then_clue(source.clue_tokens, source.turn)?;
    let inferred = &analysis.inferences;
    if !analysis.replay.early_game
        || !rule_enabled(profile, HGroupRuleId::Stalling)
        || source.clue_tokens == 0
        || source.clue_tokens == MAX_CLUE_TOKENS
        || inferred.connection.is_some()
        || !inferred.playable_now.is_empty()
        || !inferred.discard_now.is_empty()
        || inferred.must_clue.contains(&source.observer)
        || convention_known_trash_discard(source, inferred) != Some(discard)
    {
        return None;
    }
    let candidates = analysis_clue_candidates(deductions, profile, analysis);
    if candidates
        .iter()
        .any(|candidate| candidate.is_urgent_save() || candidate.purpose() == CluePurpose::Fix)
    {
        return None;
    }
    let best = candidates
        .iter()
        .max_by_key(|candidate| candidate.score())?;
    let next = next_player(source.observer, source.hands.len());
    if best.purpose() != CluePurpose::Play || best.target() == next {
        return None;
    }
    let outcome = super::strategic_value::scheduled_clue_outcome(source, profile, best)?;
    // No promised actor may lose a turn when the clue moves one seat later.
    if outcome.public_actions.is_empty()
        || outcome
            .public_actions
            .iter()
            .any(|action| action.owner == next)
    {
        return None;
    }
    let Action::Clue { target, clue } = best.action else {
        return None;
    };
    let touched = source.hands[target.index()]
        .iter()
        .filter_map(|card| {
            card.identity
                .filter(|identity| clue.matches(*identity))
                .map(|_| card.id)
        })
        .collect::<Vec<_>>();
    let clued = ProspectiveTransition::clue(source, target, clue, &touched);
    let (next_deductions, next_replay) = PerspectiveProjector::new(&clued, profile)
        .project(next, PerspectiveDepth::NestedRecipients)?;
    let next_inferred = infer_h_group_from_replay(&next_deductions, next_replay, profile);
    let (next_discard, _) =
        scored_discard_candidate(next_deductions.view(), &next_inferred, profile)?;
    if !super::ActionWindow::from_inferences(next_deductions.view(), &next_inferred).is_free()
        || next_inferred.chops[next.index()] != Some(next_discard)
        || convention_known_trash_discard(next_deductions.view(), &next_inferred).is_some()
    {
        return None;
    }
    let identities = deductions.possible_identities(discard)?;
    if identities.is_empty() {
        return None;
    }
    for identity in identities.iter() {
        let after = ProspectiveTransition::discard(source, source.observer, discard, identity);
        let (next_deductions, next_replay) = PerspectiveProjector::new(&after, profile)
            .project(next, PerspectiveDepth::NestedRecipients)?;
        if !next_replay.early_game {
            return None;
        }
        let future = h_group_clue_candidates_from_replay(&next_deductions, profile, &next_replay);
        let same = future
            .iter()
            .find(|candidate| candidate.action == best.action)?;
        let later =
            super::strategic_value::scheduled_clue_outcome(next_deductions.view(), profile, same)?;
        if later.public_actions != outcome.public_actions
            || later.owner_actions != outcome.owner_actions
            || later.protected_cards != outcome.protected_cards
            || later.new_connections != outcome.new_connections
        {
            return None;
        }
    }
    // Carry the value of the preserved clue, then apply only a one-point
    // scheduling tiebreak. Do not make safe trash intrinsically worth more.
    Some(101 + i32::from(best.score()))
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
    super::ResourceSchedule::discard_then_clue(source.clue_tokens, source.turn)?;
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
/// expose incidental future actions, but an ordinary one-for-one Play Clue is still a
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

#[cfg(test)]
mod early_game_handoff_tests {
    use super::*;

    #[test]
    fn reviewed_safe_discard_handoff_requires_early_game_and_safe_trash() {
        // User-reviewed p4v0s3 turn 9: Alice can pass green to Bob without
        // ending Early Game; Bob's ordinary chop discard would end it.
        let fixture = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s3.json"
        ))
        .expect("reviewed replay");
        let state = fixture.state_at_turn(8).expect("legal prefix");
        let deductions =
            LogicalDeductions::new(state.view_for(state.current_player()).expect("actor view"))
                .expect("valid deductions");
        let mut analysis = build_h_group_analysis(&deductions, HGroupProfile::Max);
        let priority = early_game_clue_handoff_priority(
            &deductions,
            HGroupProfile::Max,
            &analysis,
            CardId::new(2),
        )
        .expect("same clue remains available to Bob");
        let best = analysis_clue_candidates(&deductions, HGroupProfile::Max, &analysis)
            .iter()
            .map(|candidate| 100 + i32::from(candidate.score()))
            .max()
            .unwrap();
        assert_eq!(priority, best + 1);
        let chop = analysis.inferences.chops[0].expect("Alice has an ordinary chop");
        assert_eq!(
            early_game_clue_handoff_priority(&deductions, HGroupProfile::Max, &analysis, chop,),
            None,
            "unknown chop must not receive the safe-discard preference"
        );
        analysis.replay.early_game = false;
        assert_eq!(
            early_game_clue_handoff_priority(
                &deductions,
                HGroupProfile::Max,
                &analysis,
                CardId::new(2),
            ),
            None,
            "preserving an already-ended phase has no value"
        );
    }
}
