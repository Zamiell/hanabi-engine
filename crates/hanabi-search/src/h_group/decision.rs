use super::{
    Action, ActionSchedule, BeliefConstraints, Card, CardId, CardSet, Clue, ClueCandidate,
    CluePurpose, ClueRecognition, ClueValue, ConstraintReason, ConventionActionReason,
    ConventionConstraintGraph, ConventionConstraints, HGroupActionKind, HGroupActionSet,
    HGroupAnalyzedAction, HGroupClueKind, HGroupConnection, HGroupConnectionKind,
    HGroupConnectionPromise, HGroupIdentityStatus, HGroupInferences, HGroupMoveKind, HGroupPhase,
    HGroupPlayObligation, HGroupProfile, HGroupRuleId, HGroupState, IdentitySet, LogicalDeductions,
    MAX_CLUE_TOKENS, ObservedCard, OnceLock, PerspectiveDepth, PerspectiveProjector, PlayerId,
    PlayerView, ProspectiveTransition, Rank, RejectedConventionAction, Suit,
    TeamConventionSnapshot, chop, convention_card_inferences, creates_false_anxiety, focus,
    h_group_clue_candidates_from_replay, h_group_phase, h_group_rejected_clues_from_replay,
    identity_of, infer_clue_to_self, is_convention_trash, is_critical, is_playable_at,
    is_playable_now, next_player, owner_knowledge_read_model, pending_is_active,
    projected_h_group_replay, prospective_clue_has_unsafe_connection,
    prospective_clue_marks_focus_saved, prospective_clue_view,
    prospective_play_has_unsafe_inference, replay_h_group, rule_enabled,
};

#[derive(Clone, Debug)]
pub(crate) struct HGroupAnalysis {
    replay: HGroupState,
    inferences: HGroupInferences,
    clue_candidates: OnceLock<Vec<ClueCandidate>>,
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
) -> &'a [ClueCandidate] {
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
    let blocked_connection_cards = replay
        .pending_connections
        .iter()
        .flat_map(|pending| {
            let blocked_candidates = if pending.actor != view.observer {
                &[][..]
            } else if pending_is_active(pending, &replay.pending_connections) {
                // A layered Finesse is an ordered, conditional sequence. Only
                // its first card is currently due; later cards become due one
                // at a time after each successful non-connecting play.
                pending.cards.get(1..).unwrap_or_default()
            } else {
                pending.cards.as_slice()
            }
            .iter()
            .copied();
            // A clue's focus is not playable until every connecting card has
            // resolved, even when the next connection belongs to another
            // player. Otherwise the recipient can skip a queued connector
            // (for example, play Red 4 while a teammate still owes Red 3).
            blocked_candidates.chain(core::iter::once(pending.focus))
        })
        .collect::<CardSet>();
    let promptable = replay.promptable();
    let gotten = replay.gotten_from(&promptable);
    let chops = replay
        .hands
        .iter()
        .map(|hand| chop(hand, &gotten))
        .collect::<Vec<_>>();
    let cards = convention_card_inferences(deductions, &replay);
    let fixed_cards = replay.cards.facts.fixed_cards();
    let own_required_discards = action_schedule
        .required_discards_for(view.observer)
        .collect();
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
                && pending_is_active(pending, &replay.pending_connections)
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
        if (!fixed_cards.contains(&card.card) || replay.cards.forced_playable.contains(&card.card))
            && !replay.cards.invalidated_focuses.contains(&card.card)
            && !replay.cards.declined_direct_plays.contains(&card.card)
            && !blocked_connection_cards.contains(&card.card)
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

    if rule_enabled(profile, HGroupRuleId::Stalling)
        && view.clue_tokens == 0
        && inferred.playable_now.is_empty()
        && !replay.pending_connections.iter().any(|connection| {
            connection.actor == view.observer
                && pending_is_active(connection, &replay.pending_connections)
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

    let connection = replay
        .pending_connections
        .iter()
        .filter(|pending| {
            // A later connection can be established while an earlier card in
            // the same suit is already promised. It becomes actionable only
            // after that predecessor reaches the stack; otherwise connection
            // priority would make the successor misplay first.
            pending.actor == view.observer
                && pending_is_active(pending, &replay.pending_connections)
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
        });
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
                    && pending_is_active(pending, &replay.pending_connections)
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
    let mut clue_candidates = analysis_clue_candidates(deductions, profile, analysis).to_vec();
    clue_candidates.sort_by_key(|candidate| core::cmp::Reverse(candidate.score()));
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

    let early_game_has_extinguishing_clue = analysis.replay.early_game
        && inferred.discard_now.is_empty()
        && inferred.playable_now.is_empty()
        && clue_candidates
            .iter()
            .any(|candidate| candidate.purpose == CluePurpose::Play || candidate.save);

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
    if !early_game_has_extinguishing_clue {
        if let Some((card, _)) = scored_discard_candidate(view, inferred) {
            actions.push(Action::Discard(card));
        }
    }
    actions.extend(clue_candidates.iter().map(|candidate| candidate.action));
    // https://hanabi.github.io/level-1/#the-early-game
    // A player may not end the Early Game while a genuine Play or Save Clue
    // remains. Remove discards at candidate admission so a generic discard
    // score cannot overrule the convention; other valid clues remain choices.
    if early_game_has_extinguishing_clue {
        actions.retain(|action| !matches!(action, Action::Discard(_)));
    }
    actions.dedup();
    actions.retain(|action| legal_actions.contains(action));
    if inferred.phase == HGroupPhase::EndGame {
        let ordinary_chop = inferred.chops[view.observer.index()];
        actions.retain(|action| match action {
            Action::Discard(card) => {
                ordinary_chop == Some(*card) || positional_discard_is_valid(view, *card)
            }
            Action::Play(_) | Action::Clue { .. } => true,
        });
    }
    if !actions.is_empty() {
        let urgent_next_save = clue_candidates.iter().any(|candidate| {
            candidate.is_urgent_save()
                && candidate.target == next_player(view.current_player, view.hands.len())
        });
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
                                scored_discard_candidate(view, inferred)
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
        if let Some(trash) = own_hand.iter().find(|card| {
            gotten.contains(&card.id)
                && inferred
                    .cards
                    .iter()
                    .find(|knowledge| knowledge.card == card.id)
                    .is_some_and(|knowledge| {
                        !knowledge.identities.is_empty()
                            && knowledge.identities.iter().all(|identity| {
                                is_convention_trash(view, identity, &gotten, &inferred.cards)
                            })
                    })
        }) {
            return vec![Action::Discard(trash.id)];
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
            HGroupAnalyzedAction {
                action,
                kind: classify_h_group_action(action, inferred, clue),
                priority: raw_h_group_action_priority(deductions, profile, analysis, action),
            }
        })
        .collect::<Vec<_>>();

    let constraints = derive_convention_constraints(
        deductions.view(),
        inferred,
        &analysis.replay,
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
    pub(crate) actions: Vec<(Action, i32, ConventionActionReason)>,
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
    clue: Option<&ClueCandidate>,
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
            target: candidate.target,
            save: candidate.save,
            immediate_play: candidate.immediate_play,
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
    clues: &[ClueCandidate],
    analyzed: &[HGroupAnalyzedAction],
    constraints: &ConventionConstraints,
) -> (Option<Action>, Option<ConstraintReason>) {
    let mut candidates = analyzed
        .iter()
        .filter(|candidate| constraints.allows(candidate.action))
        .collect::<Vec<_>>();
    candidates.sort_by_key(|candidate| core::cmp::Reverse(candidate.priority));
    let preferred = candidates
        .into_iter()
        .find(|analysis| {
            analysis.kind == HGroupActionKind::Connection
                || h_group_planning_action_safe(deductions, profile, analysis.action)
        })
        .map(|analysis| analysis.action)
        .or_else(|| clues.first().map(|candidate| candidate.action));
    (preferred, constraints.reason())
}

fn derive_convention_constraints(
    view: &PlayerView,
    inferred: &HGroupInferences,
    replay: &HGroupState,
    clues: &[ClueCandidate],
    analyzed: &[HGroupAnalyzedAction],
) -> ConventionConstraints {
    if let Some(urgent) = clues
        .iter()
        .find(|candidate| hard_clue_obligation(view, replay, candidate))
    {
        return ConventionConstraints::require(
            ConstraintReason::UrgentClue,
            clues
                .iter()
                .filter(|candidate| {
                    candidate.action == urgent.action
                        || (candidate.target == urgent.target && candidate.immediate_play)
                })
                .map(|candidate| candidate.action),
        );
    }
    if inferred.connection.is_some() {
        return ConventionConstraints::require(
            ConstraintReason::ConnectionResponse,
            analyzed
                .iter()
                .filter(|candidate| {
                    candidate.kind == HGroupActionKind::Connection || candidate.priority >= 800
                })
                .map(|candidate| candidate.action),
        );
    }
    if !inferred.discard_now.is_empty() {
        return ConventionConstraints::require(
            ConstraintReason::RequiredDiscard,
            inferred.discard_now.iter().copied().map(Action::Discard),
        );
    }
    if inferred.must_clue.contains(&view.observer) {
        return ConventionConstraints::require(
            ConstraintReason::MustClue,
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

fn derive_predictable_action(
    deductions: &LogicalDeductions,
    inferred: &HGroupInferences,
    replay: &HGroupState,
    profile: HGroupProfile,
    clues: &[ClueCandidate],
) -> Option<Action> {
    let view = deductions.view();
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
    } else if !clues.iter().any(|candidate| {
        candidate.is_urgent_save()
            || (candidate.can_preempt_ordinary_play()
                && !completed_connection_focus_is_due(inferred))
    }) && inferred.playable_now.len() == 1
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
    let candidate = ClueCandidate {
        action,
        value: ClueValue::new(u16::from(score)),
        purpose: CluePurpose::Fallback,
        target,
        save,
        urgent_save: save && (identity.rank == Rank::Five || is_critical(view, identity)),
        immediate_play: !save,
        connection_steps: 0,
        action_coverage: 0,
        recognition: ClueRecognition::GeneratorProof,
    };
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
) -> Option<(CardId, u16)> {
    if view.clue_tokens == MAX_CLUE_TOKENS {
        return None;
    }
    let gotten = inferred.gotten();
    let own_hand = &view.hands[view.observer.index()];
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

fn convention_known_trash_discard(
    view: &PlayerView,
    inferred: &HGroupInferences,
) -> Option<CardId> {
    let gotten = inferred.gotten();
    view.hands[view.observer.index()].iter().find_map(|card| {
        inferred
            .cards
            .iter()
            .find(|note| note.card == card.id)
            .filter(|note| {
                !note.identities.is_empty()
                    && note.identities.iter().all(|identity| {
                        is_convention_trash(view, identity, &gotten, &inferred.cards)
                    })
            })
            .map(|_| card.id)
    })
}

/// Returns the remaining visible 5s whose owners do not yet know to play.
///
/// This deliberately recognizes only the cleanup state in which every stack
/// has reached 4. A missing 5 in the deck or in the observer's hidden hand
/// leaves the completion plan unresolved and disables progress dominance.
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
                match view.play_stacks[suit.index()].len() {
                    5 => continue,
                    4 => {}
                    _ => return None,
                }
                let identity = Card::new(suit, Rank::Five);
                let (owner, card) = view
                    .hands
                    .iter()
                    .enumerate()
                    .filter(|(owner, _)| *owner != view.observer.index())
                    .find_map(|(owner, hand)| {
                        hand.iter()
                            .find(|card| card.identity == Some(identity))
                            .map(|card| (owner, card.id))
                    })?;
                let owner = PlayerId::new(
                    u8::try_from(owner).expect("standard Hanabi has at most five players"),
                );
                let projection = team.projection(owner)?;
                if !projection.inferred.playable_now.contains(&card) {
                    unresolved_fives.insert(card);
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
    candidate: &ClueCandidate,
) -> Option<i32> {
    let view = deductions.view();
    if candidate.purpose != CluePurpose::Play
        || !candidate.immediate_play
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
    let advances_plan = view.hands[target.index()].iter().any(|card| {
        plan.unresolved_fives.contains(&card.id)
            && card.identity.is_some_and(|identity| clue.matches(identity))
    });
    if !advances_plan {
        return None;
    }
    let (_, discard_score) = scored_discard_candidate(view, &analysis.inferences)?;
    let ordinary_priority = 100 + i32::from(candidate.score());
    Some(ordinary_priority.max(101 + i32::from(discard_score)))
}

fn raw_h_group_action_priority(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    analysis: &HGroupAnalysis,
    action: Action,
) -> i32 {
    let inferred = &analysis.inferences;
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
        if let Some((candidate, score)) = scored_discard_candidate(deductions.view(), inferred) {
            if candidate == card {
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
    if !inferred.playable_now.is_empty()
        && !completed_connection_focus_is_due(inferred)
        && clue_candidate.is_some_and(|candidate| candidate.can_preempt_ordinary_play())
    {
        // A semantically strong setup clue can occupy several teammates while
        // the giver's exact play remains safely parked. Treating every known
        // play as forced made this multi-play line disappear from planning.
        clue_priority.max(550)
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
        .map(|candidate| candidate.action_coverage)
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
                .map(|candidate| candidate.action_coverage)
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

/// Once teammates have demonstrated a connection for a clue, its focus is a
/// due convention response rather than an ordinary exact play that can be
/// parked for a new setup.
fn completed_connection_focus_is_due(inferred: &HGroupInferences) -> bool {
    inferred.clues.iter().any(|clue| {
        inferred.playable_now.contains(&clue.focus)
            && inferred.signals.iter().any(|signal| {
                signal.turn == clue.turn && signal.kind == HGroupMoveKind::SuboptimalConnection
            })
    })
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

fn connection_layer_demonstrated(
    view: &PlayerView,
    inferred: &HGroupInferences,
    profile: HGroupProfile,
    connection: HGroupConnection,
) -> bool {
    rule_enabled(profile, HGroupRuleId::Priority)
        && inferred.signals.iter().any(|signal| {
            signal.kind == super::HGroupMoveKind::LayeredFinesse
                && signal.target == Some(view.observer)
                && signal.identity == Some(connection.identity)
                && signal.cards.contains(&connection.card)
                && signal.cards.first() != Some(&connection.card)
        })
}

fn clue_can_defer_connection(
    view: &PlayerView,
    inferred: &HGroupInferences,
    profile: HGroupProfile,
    connection: HGroupConnection,
    candidate: &ClueCandidate,
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
    clue_candidates: &[ClueCandidate],
    legal_actions: &[Action],
) -> Option<Vec<Action>> {
    let required_fixes = clue_candidates
        .iter()
        .filter(|candidate| candidate.purpose == CluePurpose::Fix)
        .map(|candidate| candidate.action)
        .collect::<Vec<_>>();
    if !required_fixes.is_empty() {
        return Some(required_fixes);
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
    candidate: &ClueCandidate,
) -> bool {
    hard_clue_obligation(view, replay, candidate)
        || (candidate.target == next_player(view.current_player, view.hands.len())
            && candidate.is_urgent_for_next_player())
}

/// Hard clue obligations are distinct from clues that are merely permitted
/// to preempt a play. A strong immediate Play Clue to the next player may be
/// considered alongside an occupied player's action, but only a Fix or an
/// at-risk critical Save excludes that action from planning altogether.
fn hard_clue_obligation(
    view: &PlayerView,
    replay: &HGroupState,
    candidate: &ClueCandidate,
) -> bool {
    if candidate.purpose == CluePurpose::Fix {
        return true;
    }
    let player_count = view.hands.len();
    let target_distance =
        (candidate.target.index() + player_count - view.current_player.index()) % player_count;
    let every_intervening_player_is_occupied = (1..target_distance).all(|distance| {
        let player = PlayerId::new(
            u8::try_from((view.current_player.index() + distance) % player_count)
                .expect("standard Hanabi has at most five players"),
        );
        replay.pending_connections.iter().any(|connection| {
            connection.actor == player && pending_is_active(connection, &replay.pending_connections)
        }) || replay.hands[player.index()]
            .iter()
            .any(|card| replay.cards.already_playing.contains(card))
    });
    candidate.is_urgent_save() && every_intervening_player_is_occupied
}

pub(super) fn ordered_playable_cards(
    view: &PlayerView,
    inferred: &HGroupInferences,
    profile: HGroupProfile,
) -> Vec<CardId> {
    let mut cards = inferred.playable_now.clone();
    if !rule_enabled(profile, HGroupRuleId::BasicStrategy) || cards.len() < 2 {
        return cards;
    }
    let own_hand = &view.hands[view.observer.index()];
    let initial_hand_size = if view.hands.len() <= 3 { 5 } else { 4 };
    let initial_cards = initial_hand_size * view.hands.len();
    let gotten = inferred.gotten();
    let ordering = PlayableOrderContext {
        view,
        inferred,
        profile,
        own_hand,
        initial_cards,
        gotten: &gotten,
    };
    cards.sort_by_key(|card| playable_order_key(&ordering, *card));
    cards
}

type PlayableOrderKey = (bool, bool, bool, bool, usize, u8, u8);

struct PlayableOrderContext<'a> {
    view: &'a PlayerView,
    inferred: &'a HGroupInferences,
    profile: HGroupProfile,
    own_hand: &'a [ObservedCard],
    initial_cards: usize,
    gotten: &'a CardSet,
}

fn playable_order_key(context: &PlayableOrderContext<'_>, card: CardId) -> PlayableOrderKey {
    let position = context
        .own_hand
        .iter()
        .position(|candidate| candidate.id == card)
        .unwrap_or(0);
    let note = context.inferred.cards.iter().find(|note| note.card == card);
    let convention_singleton = note
        .filter(|note| note.identities.len() == 1)
        .and_then(|note| note.identities.iter().next());
    let logical_singleton = context
        .own_hand
        .iter()
        .find(|candidate| candidate.id == card)
        .map(|candidate| IdentitySet::from_mask(candidate.clues.identity_mask()))
        .filter(|identities| identities.len() == 1)
        .and_then(|identities| identities.iter().next());
    let singleton = convention_singleton.or(logical_singleton);
    let rank = singleton.map_or(6, |identity| identity.rank.number());
    let fresh_one = rank == 1 && card.index() >= context.initial_cards;
    let starting_one = rank == 1 && !fresh_one;
    let chop_focused = context
        .inferred
        .clues
        .iter()
        .any(|clue| clue.focus == card && clue.focus_was_chop);
    let saved_five_focus = context.inferred.clues.iter().any(|clue| {
        clue.focus == card && clue.focus_was_chop && clue.clue == Clue::Rank(Rank::Five)
    });
    let blind = note.is_some_and(|note| note.play_obligation.is_some());
    if !rule_enabled(context.profile, HGroupRuleId::Priority)
        || context.inferred.phase == HGroupPhase::EndGame
    {
        let age = if fresh_one {
            usize::MAX - position
        } else if starting_one {
            0
        } else {
            position + 1
        };
        return (!blind, !chop_focused, !fresh_one, false, age, 0, 0);
    }
    priority_playable_order_key(
        context,
        card,
        singleton,
        blind,
        saved_five_focus,
        rank,
        position,
    )
}

fn priority_playable_order_key(
    context: &PlayableOrderContext<'_>,
    card: CardId,
    singleton: Option<Card>,
    blind: bool,
    saved_five_focus: bool,
    rank: u8,
    position: usize,
) -> PlayableOrderKey {
    // When a 5 Save lands on two already-playable 5s, play the chop-focused
    // card first: that is the card the clue specifically secured. Other ranks
    // retain the normal Priority ordering (notably the touched-1 ordering),
    // where clue focus is not itself play precedence.
    // A Finesse/Discharge card remains the outstanding blind obligation even
    // when its identity is exact. Advancing the stack with another copy first
    // can turn that promised play into a duplicate misplay.
    let leads_other = singleton.is_some_and(|identity| {
        let next = Card::new(
            identity.suit,
            Rank::ALL
                .get(identity.rank.index() + 1)
                .copied()
                .unwrap_or(Rank::Five),
        );
        identity.rank != Rank::Five
            && context
                .view
                .hands
                .iter()
                .enumerate()
                .filter(|(player, _)| *player != context.view.observer.index())
                .any(|(_, hand)| {
                    hand.iter()
                        .any(|candidate| candidate.identity == Some(next))
                })
    });
    let leads_self = singleton.is_some_and(|identity| {
        if identity.rank == Rank::Five {
            return false;
        }
        let next = Card::new(identity.suit, Rank::ALL[identity.rank.index() + 1]);
        context.inferred.cards.iter().any(|candidate| {
            candidate.card != card
                && context.gotten.contains(&candidate.card)
                && candidate.identities.contains(next)
        })
    });
    (
        !blind,
        !saved_five_focus,
        !leads_other,
        !leads_self,
        usize::from(rank != 5),
        rank,
        u8::try_from(context.own_hand.len().saturating_sub(position)).unwrap_or(u8::MAX),
    )
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
            let known_trash = own_hand.iter().find_map(|card| {
                inferred
                    .cards
                    .iter()
                    .find(|note| note.card == card.id)
                    .filter(|note| {
                        !note.identities.is_empty()
                            && note.identities.iter().all(|identity| {
                                is_convention_trash(view, identity, &gotten, &inferred.cards)
                            })
                    })
                    .map(|_| card.id)
            });
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
