use super::{
    Action, Arc, Card, CardId, Clue, ClueFacts, ConnectionObligation, GameStatus,
    HGroupCardInference, HGroupClueInterpretation, HGroupClueKind, HGroupInferences,
    HGroupMoveKind, HGroupProfile, HGroupSaveKind, HGroupState, IdentitySet, LogicalDeductions,
    MAX_CLUE_TOKENS, ObservedCard, ObservedEvent, ObservedHistoryEntry, PerspectiveDepth,
    PerspectiveProjector, PlayerId, PlayerView, ProspectiveTransition, Rank, RefCell, chop,
    convention_card_inferences, identity_of, infer_h_group, infer_h_group_from_replay,
    is_playable_now, next_player, replay_h_group_inner, replay_identity_is_queued,
};
use crate::information_set::HandAssignmentVisitEnd;
use std::rc::Rc;

pub(super) fn subjective_convention_cards(
    source: &PlayerView,
    profile: HGroupProfile,
    observer: PlayerId,
) -> Option<Vec<HGroupCardInference>> {
    let (deductions, replay) = subjective_h_group_replay(source, profile, observer)?;
    Some(convention_card_inferences(&deductions, &replay))
}

/// Cards that one named player is already convention-bound to play from their
/// own information. Candidate generation uses this projection instead of the
/// clue giver's visible card faces when deciding whether a proposed clue
/// creates a genuinely new Prompt.
pub(super) fn subjective_playable_cards(
    source: &PlayerView,
    profile: HGroupProfile,
    observer: PlayerId,
) -> Option<Vec<CardId>> {
    let (deductions, replay) = subjective_h_group_replay(source, profile, observer)?;
    Some(infer_h_group_from_replay(&deductions, replay, profile).playable_now)
}

fn subjective_h_group_replay(
    source: &PlayerView,
    profile: HGroupProfile,
    observer: PlayerId,
) -> Option<(LogicalDeductions, HGroupState)> {
    projected_h_group_replay_inner(
        source,
        profile,
        observer,
        PerspectiveDepth::NestedRecipients,
    )
}

pub(super) fn projected_h_group_replay(
    source: &PlayerView,
    profile: HGroupProfile,
    observer: PlayerId,
) -> Option<(LogicalDeductions, HGroupState)> {
    projected_h_group_replay_inner(
        source,
        profile,
        observer,
        PerspectiveDepth::NestedRecipients,
    )
}

#[derive(Clone)]
pub(super) struct CompiledObserverProjection {
    pub(super) deductions: Arc<LogicalDeductions>,
    pub(super) replay: HGroupState,
    pub(super) inferred: HGroupInferences,
}

/// One coherent public position with lazily materialized epistemic overlays
/// for every player. All consumers of a hypothetical share this object, so an
/// observer projection cannot be reconstructed under a different convention
/// lifecycle halfway through candidate evaluation.
#[derive(Clone)]
pub(super) struct TeamConventionSnapshot {
    source: PlayerView,
    profile: HGroupProfile,
    projections: Rc<RefCell<Vec<Option<CompiledObserverProjection>>>>,
}

impl TeamConventionSnapshot {
    pub(super) fn new(source: PlayerView, profile: HGroupProfile) -> Self {
        let player_count = source.hands.len();
        Self {
            source,
            profile,
            projections: Rc::new(RefCell::new(vec![None; player_count])),
        }
    }

    pub(super) fn projection(&self, observer: PlayerId) -> Option<CompiledObserverProjection> {
        if let Some(cached) = self.projections.borrow()[observer.index()].clone() {
            return Some(cached);
        }
        let (deductions, replay) = projected_h_group_replay(&self.source, self.profile, observer)?;
        let deductions = Arc::new(deductions);
        let inferred = infer_h_group_from_replay(&deductions, replay.clone(), self.profile);
        let projection = CompiledObserverProjection {
            deductions,
            replay,
            inferred,
        };
        self.projections.borrow_mut()[observer.index()] = Some(projection.clone());
        Some(projection)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProspectiveClueKey {
    target: PlayerId,
    clue: Clue,
    touched: Vec<CardId>,
}

/// One hypothetical clue transition compiled exactly once by the normal
/// history reducer. Admission, recipient validation, hazard checks, and
/// strategic evaluation all query this immutable result rather than
/// independently reconstructing the clue's meaning.
#[derive(Clone)]
pub(super) struct CompiledProspectiveClue {
    action: Action,
    turn: u32,
    after: PlayerView,
    team: TeamConventionSnapshot,
}

impl CompiledProspectiveClue {
    pub(super) const fn after(&self) -> &PlayerView {
        &self.after
    }

    pub(super) fn projection(&self, observer: PlayerId) -> Option<CompiledObserverProjection> {
        self.team.projection(observer)
    }

    pub(super) fn signal_kinds(&self, observer: PlayerId) -> Option<Vec<HGroupMoveKind>> {
        let projection = self.projection(observer)?;
        Some(
            projection
                .replay
                .signals
                .iter()
                .filter(|signal| signal.turn == self.turn)
                .map(|signal| signal.kind)
                .collect(),
        )
    }

    fn primary_interpretation(&self) -> Option<HGroupClueInterpretation> {
        let Action::Clue { target, .. } = self.action else {
            return None;
        };
        self.projection(target)?
            .replay
            .clues
            .iter()
            .rev()
            .find(|interpretation| interpretation.turn == self.turn)
            .cloned()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProspectiveSaveKey {
    clue: ProspectiveClueKey,
    focus: CardId,
}

struct ProspectiveAnalysisCache {
    source_address: usize,
    profile: HGroupProfile,
    baseline_team: TeamConventionSnapshot,
    clue_snapshots: Vec<(ProspectiveClueKey, Option<CompiledProspectiveClue>)>,
    save_validations: Vec<(ProspectiveSaveKey, bool)>,
}

thread_local! {
    static PROSPECTIVE_ANALYSIS_CACHE: RefCell<Option<ProspectiveAnalysisCache>> =
        const { RefCell::new(None) };
}

#[cfg(test)]
thread_local! {
    static PROSPECTIVE_SNAPSHOT_REDUCTIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

struct ProspectiveAnalysisCacheGuard(Option<ProspectiveAnalysisCache>);

impl Drop for ProspectiveAnalysisCacheGuard {
    fn drop(&mut self) {
        let previous = self.0.take();
        PROSPECTIVE_ANALYSIS_CACHE.with(|cache| {
            cache.replace(previous);
        });
    }
}

pub(super) fn with_prospective_analysis_cache<T>(
    source: &PlayerView,
    profile: HGroupProfile,
    operation: impl FnOnce() -> T,
) -> T {
    let replacement = ProspectiveAnalysisCache {
        source_address: core::ptr::from_ref(source).addr(),
        profile,
        baseline_team: TeamConventionSnapshot::new(source.clone(), profile),
        clue_snapshots: Vec::new(),
        save_validations: Vec::new(),
    };
    let previous = PROSPECTIVE_ANALYSIS_CACHE.with(|cache| cache.replace(Some(replacement)));
    let _guard = ProspectiveAnalysisCacheGuard(previous);
    operation()
}

fn prospective_baseline_projection(
    source: &PlayerView,
    profile: HGroupProfile,
    observer: PlayerId,
) -> Option<CompiledObserverProjection> {
    let source_address = core::ptr::from_ref(source).addr();
    let cached_team = PROSPECTIVE_ANALYSIS_CACHE.with(|cache| {
        let cache = cache.borrow();
        let cache = cache.as_ref()?;
        if cache.source_address != source_address || cache.profile != profile {
            return None;
        }
        Some(cache.baseline_team.clone())
    });
    if let Some(team) = cached_team {
        return team.projection(observer);
    }
    TeamConventionSnapshot::new(source.clone(), profile).projection(observer)
}

/// Reuses the current position's team projection inside one candidate
/// compilation pass. Strategic comparison and safety validation therefore
/// share the same observer reductions instead of building parallel baselines.
pub(super) fn compiled_baseline_team(
    source: &PlayerView,
    profile: HGroupProfile,
) -> TeamConventionSnapshot {
    let source_address = core::ptr::from_ref(source).addr();
    PROSPECTIVE_ANALYSIS_CACHE.with(|cache| {
        cache
            .borrow()
            .as_ref()
            .filter(|cache| cache.source_address == source_address && cache.profile == profile)
            .map_or_else(
                || TeamConventionSnapshot::new(source.clone(), profile),
                |cache| cache.baseline_team.clone(),
            )
    })
}

/// Applies a hypothetical clue once and materializes the recipient-relative
/// convention snapshot consumed by candidate admission, hazard checking, and
/// strategic evaluation. All consumers therefore observe the same reducer
/// result instead of independently replaying the hypothetical history.
pub(super) fn compiled_prospective_clue(
    source: &PlayerView,
    profile: HGroupProfile,
    target: PlayerId,
    clue: Clue,
    touched: &[CardId],
) -> Option<CompiledProspectiveClue> {
    let key = ProspectiveClueKey {
        target,
        clue,
        touched: touched.to_vec(),
    };
    let source_address = core::ptr::from_ref(source).addr();
    if let Some(cached) = PROSPECTIVE_ANALYSIS_CACHE.with(|cache| {
        let cache = cache.borrow();
        let cache = cache.as_ref()?;
        (cache.source_address == source_address && cache.profile == profile)
            .then(|| {
                cache
                    .clue_snapshots
                    .iter()
                    .find(|(candidate, _)| candidate == &key)
                    .map(|(_, snapshot)| snapshot.clone())
            })
            .flatten()
    }) {
        return cached;
    }

    let after = prospective_clue_view(source, target, clue, touched);
    #[cfg(test)]
    PROSPECTIVE_SNAPSHOT_REDUCTIONS.with(|count| count.set(count.get() + 1));
    let team = TeamConventionSnapshot::new(after.clone(), profile);
    let computed = team.projection(target).map(|_| CompiledProspectiveClue {
        action: Action::Clue { target, clue },
        turn: source.turn,
        after,
        team,
    });
    PROSPECTIVE_ANALYSIS_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(cache) = cache.as_mut() {
            if cache.source_address == source_address && cache.profile == profile {
                cache.clue_snapshots.push((key, computed.clone()));
            }
        }
    });
    computed
}

#[cfg(test)]
mod tests {
    use hanabi_core::{FullState, standard_deck};

    use super::*;

    #[test]
    fn candidate_consumers_share_one_convention_snapshot() {
        let state = FullState::new_standard(3, standard_deck()).expect("standard game");
        let source = state.view_for(PlayerId::new(0)).expect("observer exists");
        let target = PlayerId::new(1);
        let identity = source.hands[target.index()][0]
            .identity
            .expect("teammate card is visible");
        let clue = Clue::Suit(identity.suit);
        let touched = source.hands[target.index()]
            .iter()
            .filter(|card| card.identity.is_some_and(|card| clue.matches(card)))
            .map(|card| card.id)
            .collect::<Vec<_>>();
        let focus = touched[0];

        with_prospective_analysis_cache(&source, HGroupProfile::Max, || {
            PROSPECTIVE_SNAPSHOT_REDUCTIONS.with(|count| count.set(0));
            let _ =
                prospective_clue_signal_kinds(&source, HGroupProfile::Max, target, clue, &touched);
            let _ = prospective_clue_hazard(
                &source,
                HGroupProfile::Max,
                target,
                focus,
                clue,
                &touched,
                false,
            );
            PROSPECTIVE_SNAPSHOT_REDUCTIONS.with(|count| assert_eq!(count.get(), 1));
        });
    }

    #[test]
    fn incomplete_world_traversal_cannot_prove_contextual_save_safety() {
        assert!(exhaustive_world_validation_is_safe(
            HandAssignmentVisitEnd::Exhausted,
            false
        ));
        assert!(!exhaustive_world_validation_is_safe(
            HandAssignmentVisitEnd::Exhausted,
            true
        ));
        assert!(!exhaustive_world_validation_is_safe(
            HandAssignmentVisitEnd::LimitReached,
            false
        ));
        assert!(!exhaustive_world_validation_is_safe(
            HandAssignmentVisitEnd::VisitorStopped,
            true
        ));
    }
}

fn cached_save_validation(
    source: &PlayerView,
    profile: HGroupProfile,
    key: &ProspectiveSaveKey,
) -> Option<bool> {
    let source_address = core::ptr::from_ref(source).addr();
    PROSPECTIVE_ANALYSIS_CACHE.with(|cache| {
        let cache = cache.borrow();
        let cache = cache.as_ref()?;
        if cache.source_address != source_address || cache.profile != profile {
            return None;
        }
        cache
            .save_validations
            .iter()
            .find_map(|(candidate, safe)| (candidate == key).then_some(*safe))
    })
}

fn cache_save_validation(
    source: &PlayerView,
    profile: HGroupProfile,
    key: ProspectiveSaveKey,
    safe: bool,
) {
    let source_address = core::ptr::from_ref(source).addr();
    PROSPECTIVE_ANALYSIS_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(cache) = cache.as_mut() {
            if cache.source_address == source_address && cache.profile == profile {
                cache.save_validations.push((key, safe));
            }
        }
    });
}

const fn exhaustive_world_validation_is_safe(
    end: HandAssignmentVisitEnd,
    unsafe_world_found: bool,
) -> bool {
    !unsafe_world_found && matches!(end, HandAssignmentVisitEnd::Exhausted)
}

fn projected_h_group_replay_inner(
    source: &PlayerView,
    profile: HGroupProfile,
    observer: PlayerId,
    depth: PerspectiveDepth,
) -> Option<(LogicalDeductions, HGroupState)> {
    PerspectiveProjector::new(source, profile).project(observer, depth)
}

pub(super) fn prospective_play_has_unsafe_inference(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    card: CardId,
) -> bool {
    let source = deductions.view();
    let inferred = infer_h_group(deductions, profile);
    let identity = deductions
        .possible_identities(card)
        .filter(|identities| identities.len() == 1)
        .and_then(|identities| identities.iter().next())
        .or_else(|| {
            inferred
                .cards
                .iter()
                .find(|note| note.card == card && note.identities.len() == 1)
                .and_then(|note| note.identities.iter().next())
        });
    let Some(identity) = identity else {
        return false;
    };
    let following = next_player(source.observer, source.hands.len());
    let Some((baseline_deductions, baseline_replay)) =
        projected_h_group_replay(source, profile, following)
    else {
        return true;
    };
    let baseline = infer_h_group_from_replay(&baseline_deductions, baseline_replay, profile);
    let after_play = prospective_play_view(source, source.observer, card, identity);
    let Some((after_deductions, after_replay)) =
        projected_h_group_replay(&after_play, profile, following)
    else {
        return true;
    };
    let causal_cards = after_replay
        .signals
        .iter()
        .filter(|signal| signal.turn == source.turn)
        .flat_map(|signal| signal.cards.iter().copied())
        .collect::<super::CardSet>();
    let after = infer_h_group_from_replay(&after_deductions, after_replay, profile);
    let mut newly_promised = after
        .playable_now
        .iter()
        .copied()
        .filter(|candidate| !baseline.playable_now.contains(candidate))
        .chain(
            after
                .connection
                .map(|connection| connection.card)
                .filter(|candidate| {
                    baseline
                        .connection
                        .is_none_or(|prior| prior.card != *candidate)
                }),
        );

    newly_promised.any(|candidate| {
        identity_of(source, candidate).is_some_and(|actual| {
            let caused_by_play = causal_cards.contains(&candidate)
                || (actual.suit == identity.suit && actual.rank.number() > identity.rank.number());
            caused_by_play && !is_playable_now(&after_play, actual)
        })
    })
}

pub(super) fn prospective_clue_marks_focus_saved(
    source: &PlayerView,
    profile: HGroupProfile,
    target: PlayerId,
    focus: CardId,
    clue: Clue,
    touched: &[CardId],
) -> bool {
    let key = ProspectiveSaveKey {
        clue: ProspectiveClueKey {
            target,
            clue,
            touched: touched.to_vec(),
        },
        focus,
    };
    if let Some(safe) = cached_save_validation(source, profile, &key) {
        return safe;
    }
    let Some(compiled) = compiled_prospective_clue(source, profile, target, clue, touched) else {
        return false;
    };
    if !compiled.projection(target).is_some_and(|projection| {
        projection.inferred.is_saved(focus)
            && identity_of(source, focus).is_some_and(|actual| {
                projection
                    .inferred
                    .cards
                    .iter()
                    .any(|note| note.card == focus && note.identities.contains(actual))
            })
    }) {
        cache_save_validation(source, profile, key, false);
        return false;
    }

    // Ordinary Level 1 Save precedence is independent of the giver's hidden
    // hand. Rank 2/5 clues may be represented as `PlayOrSave` when the focus
    // spans both categories, but it remains saved in every branch. A genuinely
    // critical focus is likewise invariant. Resolving more of the giver's
    // cards may narrow the recipient's Save identities but cannot turn the
    // focus into a non-Save. Eight-Clue Saves remain contextual and require
    // the exhaustive check below.
    let primary_save_is_hidden_hand_invariant =
        compiled
            .primary_interpretation()
            .is_some_and(|interpretation| {
                interpretation.focus == focus
                    && (matches!(
                        interpretation.kind,
                        HGroupClueKind::Save(
                            HGroupSaveKind::Five | HGroupSaveKind::Two | HGroupSaveKind::Critical
                        )
                    ) || matches!(interpretation.kind, HGroupClueKind::PlayOrSave)
                        && !interpretation.save_identities.is_empty()
                        && matches!(clue, Clue::Rank(Rank::Two | Rank::Five)))
            });
    if primary_save_is_hidden_hand_invariant {
        cache_save_validation(source, profile, key, true);
        return true;
    }

    // The recipient sees the giver's complete hand even though the giver does
    // not. Stream joint, card-count-consistent giver hands and stop at the
    // first counterexample. A traversal limit is not a proof of safety.
    let mut unsafe_world_found = false;
    let Some(visit) =
        PerspectiveProjector::new(source, profile).visit_source_hand_worlds(256, |world| {
            let after_clue = prospective_clue_view(world, target, clue, touched);
            let unsafe_world =
                PerspectiveProjector::project_resolved_owned(after_clue, profile, target)
                    .is_none_or(|(deductions, replay)| {
                        replay.cards.chop_moved.contains(&focus)
                            || !infer_h_group_from_replay(&deductions, replay, profile)
                                .is_saved(focus)
                    });
            unsafe_world_found = unsafe_world;
            unsafe_world
        })
    else {
        cache_save_validation(source, profile, key, false);
        return false;
    };
    let safe = exhaustive_world_validation_is_safe(visit.end, unsafe_world_found);
    cache_save_validation(source, profile, key, safe);
    safe
}

/// Exact convention signals the recipient would attach to a hypothetical
/// clue. Candidate generation uses the same reducer as replay instead of
/// maintaining a second, inevitably incomplete catalog of Max-only moves.
pub(super) fn prospective_clue_signal_kinds(
    source: &PlayerView,
    profile: HGroupProfile,
    target: PlayerId,
    clue: Clue,
    touched: &[CardId],
) -> Vec<HGroupMoveKind> {
    let Some(snapshot) = compiled_prospective_clue(source, profile, target, clue, touched) else {
        return Vec::new();
    };
    snapshot.signal_kinds(target).unwrap_or_default()
}

/// Convention names recognized anywhere on the team after a hypothetical
/// clue. Some moves are deliberately hidden from the clue receiver: the
/// Bluff actor recognizes a Bluff, while observers who can see a layered
/// connector recognize a Clandestine Finesse. Candidate classification must
/// retain those signals even when the recipient can only write the ordinary
/// delayed Play interpretation.
pub(super) fn prospective_team_clue_signal_kinds(
    source: &PlayerView,
    profile: HGroupProfile,
    target: PlayerId,
    clue: Clue,
    touched: &[CardId],
) -> Vec<HGroupMoveKind> {
    let Some(snapshot) = compiled_prospective_clue(source, profile, target, clue, touched) else {
        return Vec::new();
    };
    let mut kinds = Vec::new();
    for player in 0..source.hands.len() {
        let observer =
            PlayerId::new(u8::try_from(player).expect("standard Hanabi has at most five players"));
        let Some(signals) = snapshot.signal_kinds(observer) else {
            continue;
        };
        for kind in signals {
            if !kinds.contains(&kind) {
                kinds.push(kind);
            }
        }
    }
    kinds
}

/// Card ordered to blind-play by a team-recognized Stacked Ejection or
/// Stacked Discharge in a hypothetical clue.
pub(super) fn prospective_stacked_ejection_card(
    source: &PlayerView,
    profile: HGroupProfile,
    target: PlayerId,
    clue: Clue,
    touched: &[CardId],
) -> Option<CardId> {
    let turn = source.turn;
    let snapshot = compiled_prospective_clue(source, profile, target, clue, touched)?;
    for player in 0..source.hands.len() {
        let observer =
            PlayerId::new(u8::try_from(player).expect("standard Hanabi has at most five players"));
        let Some(projection) = snapshot.team.projection(observer) else {
            continue;
        };
        if let Some(card) = projection
            .replay
            .signals
            .iter()
            .find(|signal| {
                signal.turn == turn
                    && matches!(
                        signal.kind,
                        HGroupMoveKind::StackedEjection | HGroupMoveKind::StackedDischarge
                    )
            })
            .and_then(|signal| signal.cards.first().copied())
        {
            return Some(card);
        }
    }
    None
}

/// Primary meaning assigned by the same replay reducer that handles a real
/// clue. Candidate admission uses this instead of maintaining a second
/// Play/Save/Fix precedence ladder.
pub(super) fn prospective_clue_primary_kind(
    source: &PlayerView,
    profile: HGroupProfile,
    target: PlayerId,
    clue: Clue,
    touched: &[CardId],
) -> Option<super::HGroupClueKind> {
    compiled_prospective_clue(source, profile, target, clue, touched)?
        .primary_interpretation()
        .map(|interpretation| interpretation.kind)
}

/// Complete primary interpretation assigned by the recipient-side replay.
/// Candidate construction uses this when convention focus differs from the
/// raw physical focus (for example, a Focus Inversion).
pub(super) fn prospective_clue_primary_interpretation(
    source: &PlayerView,
    profile: HGroupProfile,
    target: PlayerId,
    clue: Clue,
    touched: &[CardId],
) -> Option<super::HGroupClueInterpretation> {
    compiled_prospective_clue(source, profile, target, clue, touched)?.primary_interpretation()
}

pub(super) fn prospective_clue_has_unsafe_connection(
    source: &PlayerView,
    profile: HGroupProfile,
    target: PlayerId,
    focus: CardId,
    clue: Clue,
    touched: &[CardId],
    expect_immediate_focus: bool,
) -> bool {
    let hazard = prospective_clue_hazard(
        source,
        profile,
        target,
        focus,
        clue,
        touched,
        expect_immediate_focus,
    );
    hazard.is_some()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProspectiveClueHazard {
    ProjectionFailed,
    RecipientMissingFocusPlay,
    RecipientWrongSave,
    RecipientWrongPlay,
    RecipientWrongConnection,
    OtherPlayerWrongPromise,
    DuplicateGoodTouchPromise,
    FalseConnectionIdentity,
}

fn prospective_new_good_touch_identities(
    source: &PlayerView,
    replay: &HGroupState,
    inferred: &HGroupInferences,
    target: PlayerId,
    focus: CardId,
) -> (IdentitySet, Vec<CardId>) {
    let interpretation = replay.clues.iter().rev().find(|interpretation| {
        interpretation.turn == source.turn
            && interpretation.target == target
            && interpretation.focus == focus
    });
    let interpreted_focus = interpretation.map_or_else(IdentitySet::default, |interpretation| {
        interpretation.play_identities
    });
    let connection_cards = interpretation.map_or_else(Vec::new, |interpretation| {
        interpretation
            .hypotheses
            .iter()
            .filter(|hypothesis| interpreted_focus.contains(hypothesis.focus_identity))
            .flat_map(|hypothesis| &hypothesis.connection_steps)
            .flat_map(|step| step.cards.iter().copied())
            .collect()
    });
    if !inferred.playable_now.contains(&focus) {
        return (interpreted_focus, connection_cards);
    }
    let focus_identities = inferred
        .cards
        .iter()
        .find(|card| card.card == focus)
        .map_or(interpreted_focus, |card| {
            interpreted_focus.intersection(card.identities)
        });
    (focus_identities, connection_cards)
}

fn duplicates_good_touch_superposition(
    source: &PlayerView,
    profile: HGroupProfile,
    focus: CardId,
    new_play_identities: IdentitySet,
    new_connection_cards: &[CardId],
) -> Option<bool> {
    let giver_hand = source.hands[source.observer.index()]
        .iter()
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let baseline_team = compiled_baseline_team(source, profile);
    let mut baseline_possible_promises = IdentitySet::default();
    for player in 0..source.hands.len() {
        let observer =
            PlayerId::new(u8::try_from(player).expect("standard Hanabi has at most five players"));
        let projection = baseline_team.projection(observer)?;
        for interpretation in &projection.replay.clues {
            let focus_is_live = source.hands.iter().flatten().any(|card| {
                card.id == interpretation.focus
                    && !projection
                        .replay
                        .cards
                        .invalidated_focuses
                        .contains(&card.id)
            });
            let settled_identity = projection
                .replay
                .cards
                .facts
                .known_identity(interpretation.focus)
                .or_else(|| {
                    projection
                        .inferred
                        .cards
                        .iter()
                        .find(|card| {
                            card.card == interpretation.focus && card.identities.len() == 1
                        })
                        .and_then(|card| card.identities.iter().next())
                });
            let interpretation_is_still_possible = settled_identity
                .is_none_or(|identity| interpretation.play_identities.contains(identity));
            if focus_is_live
                && interpretation.focus != focus
                && giver_hand.contains(&interpretation.focus)
                && new_play_identities.len() == 1
                && settled_identity.is_some_and(|identity| new_play_identities.contains(identity))
                && !new_connection_cards.contains(&interpretation.focus)
            {
                return Some(true);
            }
            if focus_is_live
                && interpretation_is_still_possible
                && interpretation.focus != focus
                && !giver_hand.contains(&interpretation.focus)
            {
                // Once later public information settles a clue focus, only
                // that surviving identity remains a Good Touch promise. Raw
                // alternatives from the clue turn are historical branches,
                // not permanent reservations (for example, a fixed Green 4
                // must not continue reserving Yellow 4).
                let live_identities = settled_identity
                    .map_or(interpretation.play_identities, |id| {
                        IdentitySet::singleton(id)
                    });
                let reserved = live_identities.iter().filter(|identity| {
                    live_identities.len() == 1
                        || usize::from(identity.rank.number())
                            > usize::from(interpretation.stack_heights[identity.suit.index()]) + 1
                });
                for identity in reserved {
                    baseline_possible_promises =
                        baseline_possible_promises.union(IdentitySet::singleton(identity));
                }
            }
        }
    }
    Some(
        new_play_identities
            .iter()
            .any(|identity| baseline_possible_promises.contains(identity)),
    )
}

pub(super) fn prospective_clue_hazard(
    source: &PlayerView,
    profile: HGroupProfile,
    target: PlayerId,
    focus: CardId,
    clue: Clue,
    touched: &[CardId],
    expect_immediate_focus: bool,
) -> Option<ProspectiveClueHazard> {
    let Some(snapshot) = compiled_prospective_clue(source, profile, target, clue, touched) else {
        return Some(ProspectiveClueHazard::ProjectionFailed);
    };
    let after_clue = &snapshot.after;
    let Some(recipient) = snapshot.team.projection(target) else {
        return Some(ProspectiveClueHazard::ProjectionFailed);
    };
    let replay = &recipient.replay;
    let inferred = &recipient.inferred;
    let Some(baseline) = prospective_baseline_projection(source, profile, target) else {
        return Some(ProspectiveClueHazard::ProjectionFailed);
    };
    let intervening_bluff = target != next_player(source.current_player, source.hands.len())
        && (0..source.hands.len()).any(|player| {
            let observer = PlayerId::new(
                u8::try_from(player).expect("standard Hanabi has at most five players"),
            );
            snapshot
                .team
                .projection(observer)
                .is_some_and(|projection| {
                    projection.replay.signals.iter().any(|signal| {
                        signal.turn == source.turn
                            && signal.kind == HGroupMoveKind::Bluff
                            && signal.target
                                == Some(next_player(source.current_player, source.hands.len()))
                            && signal.cards.last() == Some(&focus)
                    })
                })
        });
    if let Some(hazard) = recipient_projection_hazard(
        RecipientProjectionComparison {
            source,
            replay,
            inferred,
            baseline: &baseline.replay,
            baseline_inferred: &baseline.inferred,
        },
        focus,
        expect_immediate_focus,
        intervening_bluff,
    ) {
        return Some(hazard);
    }
    // Candidate safety must use the recipient's resolved action note, not the
    // raw set of every clue-time branch. A rank clue can initially admit both
    // direct and delayed identities, then settle to its sole direct play after
    // connection validation. Treating a discarded delayed branch as a live
    // Good Touch promise incorrectly rejects the direct clue as duplication.
    let (new_play_identities, new_connection_cards) =
        prospective_new_good_touch_identities(source, replay, inferred, target, focus);
    let Some(duplicates_existing_promise) = duplicates_good_touch_superposition(
        source,
        profile,
        focus,
        new_play_identities,
        &new_connection_cards,
    ) else {
        return Some(ProspectiveClueHazard::ProjectionFailed);
    };
    if duplicates_existing_promise {
        // Good Touch reserves every delayed branch of a clue's convention
        // superposition, not just the focus card's visible face in the
        // giver's hand. Immediate multi-1 alternatives are not independent
        // identity promises. An unresolved promise in the giver's own hand is
        // exempt because the recipient sees its identity and resolves the
        // correlation. Once the giver also knows its exact identity, however,
        // Good Touch forbids creating the same promise in another hand.
        // Source: https://hanabi.github.io/level-1/#good-touch-principle
        return Some(ProspectiveClueHazard::DuplicateGoodTouchPromise);
    }
    if expect_immediate_focus
        && identity_of(source, focus).is_some_and(|identity| is_playable_now(source, identity))
        && recipient_follow_up_is_unsafe(source, after_clue, profile, target, focus)
    {
        return Some(ProspectiveClueHazard::RecipientWrongConnection);
    }
    if other_player_projection_is_unsafe(source, after_clue, profile, target, &snapshot.team) {
        return Some(ProspectiveClueHazard::OtherPlayerWrongPromise);
    }
    if touched.len() <= 1 {
        return None;
    }
    replay
        .pending_connections
        .iter()
        .filter(|connection| connection.focus == focus)
        .filter(|connection| is_new_connection(connection, &baseline.replay))
        .any(|connection| {
            connection.cards.iter().copied().any(|card| {
                identity_of(source, card).is_some_and(|actual| {
                    actual != connection.expected && !is_playable_now(source, actual)
                })
            })
        })
        .then_some(ProspectiveClueHazard::FalseConnectionIdentity)
}

/// Checks the deterministic continuation immediately implied by a direct Play
/// clue. Some false Prompts remain dormant until the focus is played, so a
/// snapshot taken only on the clue event cannot observe them.
fn recipient_follow_up_is_unsafe(
    source: &PlayerView,
    after_clue: &PlayerView,
    profile: HGroupProfile,
    target: PlayerId,
    focus: CardId,
) -> bool {
    let Some(identity) = identity_of(source, focus) else {
        return true;
    };
    let after_play = ProspectiveTransition::successful_play(after_clue, target, focus, identity);
    let Some((deductions, replay)) = PerspectiveProjector::new(&after_play, profile)
        .project(target, PerspectiveDepth::NestedRecipients)
    else {
        return true;
    };
    let inferred = infer_h_group_from_replay(&deductions, replay, profile);
    inferred.playable_now.iter().copied().any(|card| {
        identity_of(source, card).is_some_and(|actual| !is_playable_now(&after_play, actual))
    }) || inferred.connection.is_some_and(|connection| {
        identity_of(source, connection.card).is_some_and(|actual| {
            actual != connection.identity && !is_playable_now(&after_play, actual)
        })
    })
}

#[derive(Clone, Copy)]
struct RecipientProjectionComparison<'a> {
    source: &'a PlayerView,
    replay: &'a HGroupState,
    inferred: &'a HGroupInferences,
    baseline: &'a HGroupState,
    baseline_inferred: &'a HGroupInferences,
}

fn recipient_projection_hazard(
    comparison: RecipientProjectionComparison<'_>,
    focus: CardId,
    expect_immediate_focus: bool,
    intervening_bluff: bool,
) -> Option<ProspectiveClueHazard> {
    let RecipientProjectionComparison {
        source,
        replay,
        inferred,
        baseline,
        baseline_inferred,
    } = comparison;
    let intervening_predecessor = has_intervening_playable_predecessor(source, replay, focus);
    let missing_focus = expect_immediate_focus
        && identity_of(source, focus).is_some_and(|actual| is_playable_now(source, actual))
        && !inferred.playable_now.contains(&focus);
    let competing_connection = expect_immediate_focus
        && identity_of(source, focus).is_some_and(|actual| is_playable_now(source, actual))
        && [
            super::HGroupMoveKind::Prompt,
            super::HGroupMoveKind::Finesse,
            super::HGroupMoveKind::ReverseFinesse,
            super::HGroupMoveKind::SelfFinesse,
            super::HGroupMoveKind::LayeredFinesse,
            super::HGroupMoveKind::Bluff,
            super::HGroupMoveKind::DoubleBluff,
        ]
        .into_iter()
        .any(|kind| {
            replay.signals.of_kind(kind).any(|signal| {
                !baseline.signals.contains(signal) && signal.cards.iter().any(|card| *card != focus)
            })
        });
    let wrong_save = replay
        .implicit_saves
        .iter()
        .filter(|(card, identities)| {
            !baseline
                .implicit_saves
                .iter()
                .any(|(prior_card, prior_identities)| {
                    prior_card == card && prior_identities == identities
                })
        })
        .any(|(card, identities)| {
            identity_of(source, *card).is_some_and(|actual| !identities.contains(actual))
        });
    // The recipient does not act on their provisional direct interpretation
    // before an intervening Bluff resolves. Evaluating their clue-time note as
    // an immediate move falsely rejects legal lines such as Alice's yellow
    // Bluff to Cathy through Bob's off-suit playable card.
    // https://hanabi.github.io/level-11/#the-bluff
    let wrong_play = !intervening_bluff
        && inferred
            .playable_now
            .iter()
            .copied()
            .filter(|card| {
                if !baseline_inferred.playable_now.contains(card) {
                    return true;
                }
                let after_note = inferred.cards.iter().find(|note| note.card == *card);
                let before_note = baseline_inferred
                    .cards
                    .iter()
                    .find(|note| note.card == *card);
                after_note.map(|note| note.identities) != before_note.map(|note| note.identities)
            })
            .any(|card| {
                identity_of(source, card).is_some_and(|actual| !is_playable_now(source, actual))
            });
    let wrong_connection = !intervening_predecessor
        && inferred.connection.is_some_and(|connection| {
            identity_of(source, connection.card).is_some_and(|actual| {
                actual != connection.identity && !is_playable_now(source, actual)
            })
        });
    if missing_focus {
        Some(ProspectiveClueHazard::RecipientMissingFocusPlay)
    } else if competing_connection {
        Some(ProspectiveClueHazard::RecipientWrongConnection)
    } else if wrong_save {
        Some(ProspectiveClueHazard::RecipientWrongSave)
    } else if wrong_play {
        Some(ProspectiveClueHazard::RecipientWrongPlay)
    } else if wrong_connection {
        Some(ProspectiveClueHazard::RecipientWrongConnection)
    } else {
        None
    }
}

fn has_intervening_playable_predecessor(
    source: &PlayerView,
    replay: &HGroupState,
    focus: CardId,
) -> bool {
    identity_of(source, focus)
        .filter(|identity| identity.rank != Rank::One)
        .is_some_and(|focus_identity| {
            let predecessor = Card::new(
                focus_identity.suit,
                Rank::ALL[focus_identity.rank.index() - 1],
            );
            let target = source
                .hands
                .iter()
                .position(|hand| hand.iter().any(|candidate| candidate.id == focus));
            target.is_some_and(|target| {
                (1..source.hands.len())
                    .map(|distance| (source.current_player.index() + distance) % source.hands.len())
                    .take_while(|player| *player != target)
                    .any(|player| {
                        replay.hands[player].iter().copied().any(|card| {
                            (replay.cards.already_playing.contains(&card)
                                || replay.cards.explicitly_clued.contains(&card))
                                && identity_of(source, card) == Some(predecessor)
                                && is_playable_now(source, predecessor)
                        })
                    })
            })
        })
}

fn other_player_projection_is_unsafe(
    source: &PlayerView,
    after_clue: &PlayerView,
    profile: HGroupProfile,
    target: PlayerId,
    after_team: &TeamConventionSnapshot,
) -> bool {
    let Some(giver_baseline) = prospective_baseline_projection(source, profile, source.observer)
    else {
        return true;
    };
    let giver_promptable = giver_baseline.replay.promptable();
    for player in 0..source.hands.len() {
        let observer =
            PlayerId::new(u8::try_from(player).expect("standard Hanabi has at most five players"));
        if observer == target || observer == source.observer {
            continue;
        }
        let Some(other_baseline) = prospective_baseline_projection(source, profile, observer)
        else {
            return true;
        };
        let Some(other_projection) = after_team.projection(observer) else {
            return true;
        };
        let other_after = &other_projection.inferred;
        let wrong_new_play = other_after
            .playable_now
            .iter()
            .copied()
            .filter(|card| !other_baseline.inferred.playable_now.contains(card))
            .any(|card| {
                identity_of(source, card).is_some_and(|actual| !is_playable_now(after_clue, actual))
            });
        let wrong_new_connection = other_after.connection.is_some_and(|connection| {
            let height = other_projection.deductions.view().play_stacks
                [connection.identity.suit.index()]
            .len();
            let connection_is_reachable =
                (height + 1..usize::from(connection.identity.rank.number())).all(|rank| {
                    replay_identity_is_queued(
                        other_projection.deductions.view(),
                        &other_projection.replay,
                        Card::new(connection.identity.suit, Rank::ALL[rank - 1]),
                    )
                });
            let duplicates_existing_promise = replay_identity_is_queued(
                other_projection.deductions.view(),
                &other_baseline.replay,
                connection.identity,
            );
            let duplicates_possible_giver_promise =
                giver_baseline.inferred.cards.iter().any(|card| {
                    card.card != connection.card
                        && giver_promptable.contains(&card.card)
                        && card.identities.contains(connection.identity)
                });
            other_baseline
                .inferred
                .connection
                .is_none_or(|prior| prior.card != connection.card)
                && (duplicates_existing_promise
                    || duplicates_possible_giver_promise
                    || identity_of(source, connection.card).is_some_and(|actual| {
                        (actual != connection.identity || !connection_is_reachable)
                            && !is_playable_now(after_clue, actual)
                    }))
        });
        if wrong_new_play || wrong_new_connection {
            return true;
        }
    }
    false
}

fn is_new_connection(connection: &ConnectionObligation, baseline: &HGroupState) -> bool {
    !replay_contains_connection(baseline, connection)
}

fn replay_contains_connection(replay: &HGroupState, connection: &ConnectionObligation) -> bool {
    replay.pending_connections.iter().any(|prior| {
        prior.actor == connection.actor
            && prior.cards == connection.cards
            && prior.expected == connection.expected
            && prior.focus_identity == connection.focus_identity
            && prior.kind == connection.kind
            && prior.focus == connection.focus
            && prior.step == connection.step
    })
}

pub(super) fn prospective_clue_view(
    source: &PlayerView,
    target: PlayerId,
    clue: Clue,
    touched: &[CardId],
) -> PlayerView {
    ProspectiveTransition::clue(source, target, clue, touched)
}

pub(super) fn prospective_play_view(
    source: &PlayerView,
    player: PlayerId,
    card: CardId,
    identity: Card,
) -> PlayerView {
    ProspectiveTransition::successful_play(source, player, card, identity)
}

/// Inputs for one actor-relative reconstruction immediately before a public
/// action. Bundling them prevents callers from pairing history, hands, clue
/// facts, or deck size from different temporal snapshots.
#[derive(Clone, Copy)]
pub(super) struct SubjectiveReplayRequest<'a> {
    pub(super) source: &'a PlayerView,
    pub(super) profile: HGroupProfile,
    pub(super) observer: PlayerId,
    pub(super) history: &'a [ObservedHistoryEntry],
    pub(super) hands: &'a [Vec<CardId>],
    pub(super) facts: &'a [ClueFacts],
    pub(super) deck_size: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct SubjectiveActionContext {
    pub(super) chop: Option<CardId>,
    pub(super) known_identity: Option<Card>,
}

/// Reconstructs the actor's chop and exact knowledge in one replay. Special
/// discard rules must consume this result rather than an observing teammate's
/// visible card faces.
pub(super) fn subjective_action_context_before(
    request: SubjectiveReplayRequest<'_>,
    card: CardId,
) -> Option<SubjectiveActionContext> {
    let (deductions, replay) = subjective_h_group_replay_before_action(
        request.source,
        request.profile,
        request.observer,
        request.history,
        request.hands,
        request.facts,
        request.deck_size,
    )?;
    let promptable = replay.promptable();
    let gotten = replay.gotten_from(&promptable);
    let actor_chop = chop(&replay.hands[request.observer.index()], &gotten);
    let known_identity = convention_card_inferences(&deductions, &replay)
        .into_iter()
        .find(|inference| inference.card == card)
        .and_then(|inference| {
            (inference.identities.len() == 1)
                .then(|| inference.identities.iter().next())
                .flatten()
        });
    Some(SubjectiveActionContext {
        chop: actor_chop,
        known_identity,
    })
}

fn subjective_h_group_replay_before_action(
    source: &PlayerView,
    profile: HGroupProfile,
    observer: PlayerId,
    history: &[ObservedHistoryEntry],
    hands: &[Vec<CardId>],
    facts: &[ClueFacts],
    deck_size: usize,
) -> Option<(LogicalDeductions, HGroupState)> {
    let observed_hands = hands
        .iter()
        .enumerate()
        .map(|(player, hand)| {
            hand.iter()
                .map(|card| ObservedCard {
                    id: *card,
                    identity: (player != observer.index())
                        .then(|| identity_of(source, *card))
                        .flatten(),
                    clues: facts[card.index()],
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let projected_history = history
        .iter()
        .cloned()
        .map(|mut entry| {
            if let ObservedEvent::Drew {
                player,
                card,
                identity,
            } = &mut entry.event
            {
                *identity = (*player != observer)
                    .then(|| identity_of(source, *card))
                    .flatten();
            }
            entry
        })
        .collect::<Vec<_>>();
    let mut play_stacks: [Vec<(CardId, Card)>; 5] = std::array::from_fn(|_| Vec::new());
    let mut discard_pile = Vec::new();
    let mut clue_tokens = MAX_CLUE_TOKENS;
    let mut strikes = 0;
    for entry in history {
        match entry.event {
            ObservedEvent::Clued { .. } => clue_tokens = clue_tokens.saturating_sub(1),
            ObservedEvent::Played {
                card,
                identity,
                successful: true,
                ..
            } => {
                play_stacks[identity.suit.index()].push((card, identity));
                if identity.rank == Rank::Five {
                    clue_tokens = clue_tokens.saturating_add(1).min(MAX_CLUE_TOKENS);
                }
            }
            ObservedEvent::Played {
                card,
                identity,
                successful: false,
                ..
            } => {
                discard_pile.push((card, identity));
                strikes += 1;
            }
            ObservedEvent::Discarded { card, identity, .. } => {
                discard_pile.push((card, identity));
                clue_tokens = clue_tokens.saturating_add(1).min(MAX_CLUE_TOKENS);
            }
            ObservedEvent::Drew { .. } => {}
        }
    }
    let view = PlayerView {
        observer,
        current_player: observer,
        turn: history
            .last()
            .map_or(0, |entry| entry.turn.saturating_add(1)),
        hands: observed_hands,
        deck_size,
        play_stacks,
        discard_pile,
        clue_tokens,
        strikes,
        final_turns_remaining: None,
        status: GameStatus::InProgress,
        history: projected_history,
    };
    let deductions = LogicalDeductions::new(view).ok()?;
    let replay = replay_h_group_inner(&deductions, profile, PerspectiveDepth::ObserverOnly, false);
    Some((deductions, replay))
}
