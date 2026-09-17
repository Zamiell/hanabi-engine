//! Conservative, model-relative inference from a declined opportunity.
//!
//! A higher heuristic score or a longer projected line is NOT a certificate.
//! We require a resource-neutral substitution: at a common calendar frontier,
//! both clues lead to the same subsequent actions, but the alternative secures
//! strictly more resources under the planner's non-speculative Pareto order.
//! Every compatible joint hand assignment must have such a witness. Different
//! assignments may have different witnesses: the giver sees that hand.
//!
//! This assumes a teammate does not knowingly decline a uniformly dominated
//! alternative. It is convention/strategy knowledge, never logical card-count
//! knowledge. See docs/inverse-planning.md for the deliberately limited proof
//! contract and its rational-play assumption.
//!
//! Strategy sources:
//! <https://hanabi.github.io/level-3/#efficiency>
//! <https://hanabi.github.io/level-3/#tempo>

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use hanabi_core::{Action, Card, CardId, ClueFacts, ObservedEvent, PlayerView};

use crate::{
    IdentitySet, LogicalDeductions, ProjectedPositionValue, ProjectionEvidence,
    SupportedConvention, SymbolicLineOutcome, information_set::HandAssignmentVisitEnd,
};

use super::{
    HGroupCardInference, HGroupClueKind, HGroupProfile, HGroupRuleId, HGroupState,
    convention_card_inferences, is_playable_now, rule_enabled, was_clued_before,
};

#[derive(Clone, Copy)]
struct ProofLimits {
    assignments: usize,
    giver_views: usize,
}
const PROOF_LIMITS: ProofLimits = ProofLimits {
    assignments: 4_096,
    giver_views: 128,
};
const LINE_LIMIT: u8 = 16;
const CHOICE_LIMIT: usize = 8;
const CACHE_LIMIT: usize = 32;
const WITNESS_CACHE_LIMIT: usize = 256;

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static CACHE: RefCell<Vec<CacheEntry>> = const { RefCell::new(Vec::new()) };
    static WITNESS_CACHE: RefCell<Vec<WitnessCacheEntry>> = const { RefCell::new(Vec::new()) };
    #[cfg(test)]
    static BYPASS_PROJECTION_CACHE: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn is_active() -> bool {
    ACTIVE.get()
}

#[cfg(test)]
pub(super) fn clear_test_caches() {
    CACHE.with_borrow_mut(Vec::clear);
    WITNESS_CACHE.with_borrow_mut(Vec::clear);
}

struct Guard(bool);
impl Drop for Guard {
    fn drop(&mut self) {
        ACTIVE.set(self.0);
    }
}

/// Construct lower-order knowledge without recursively proving strategic
/// inferences for every historical observer needed by the convention reducer.
pub(super) fn baseline<T>(operation: impl FnOnce() -> T) -> T {
    let _guard = Guard(ACTIVE.replace(true));
    operation()
}

pub(super) fn enrich(
    deductions: &LogicalDeductions,
    replay: &mut HGroupState,
    profile: HGroupProfile,
) {
    replay.strategic_deductions = deduce(deductions, replay, profile);
    if replay.strategic_deductions.is_empty() {
        return;
    }
    let ordinary_knowledge = replay.knowledge.clone();
    replay.knowledge = super::build_convention_knowledge(deductions, replay);
    // Several individually plausible strategic exclusions must remain jointly
    // consistent with card multiplicities and correlated convention clauses.
    // A teammate may not have followed the rational-choice assumption (most
    // notably in bug reproductions). Do not publish an impossible belief or
    // arbitrarily choose which strategic claim to retain.
    let consistent = baseline(|| {
        let inferred = super::infer_h_group_from_replay(deductions, replay.clone(), profile);
        let belief = super::ConventionConstraintGraph::from_replay(deductions, replay, &inferred)
            .into_belief_constraints();
        crate::InformationSet::new(deductions.view()).is_ok_and(|information| {
            information.world_count_up_to(&belief, 1) != crate::WorldCount::Exact(0)
        })
    });
    if !consistent {
        replay.strategic_deductions.clear();
        replay.knowledge = ordinary_knowledge;
        return;
    }
    for transition in &mut replay.transitions {
        transition.delta.knowledge_changes.clear();
    }
    replay
        .knowledge
        .attach_to_transitions(&mut replay.transitions);
    debug_assert!(replay.validate().is_ok());
}

#[derive(Clone)]
struct CacheEntry {
    view: PlayerView,
    profile: HGroupProfile,
    notes: Vec<HGroupCardInference>,
    clues: Vec<super::HGroupClueInterpretation>,
    deductions: Vec<StrategicDeduction>,
}

#[derive(Clone)]
struct WitnessCacheEntry {
    view: PlayerView,
    profile: HGroupProfile,
    chosen: Action,
    witness: Option<Arc<SubstitutionWitness>>,
}

/// Evidence belongs to the time the observer can establish it, which may be
/// later than the observed choice (for example after a hidden card is revealed).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct StrategicDeduction {
    pub(super) inferred_turn: u32,
    pub(super) observed_turn: u32,
    pub(super) card: CardId,
    pub(super) excluded: IdentitySet,
    pub(super) checked_assignments: usize,
    pub(super) witnesses: Vec<Arc<SubstitutionWitness>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SubstitutionWitness {
    pub(super) chosen: Action,
    pub(super) alternative: Action,
    pub(super) chosen_line: ProjectionEvidence,
    pub(super) alternative_line: ProjectionEvidence,
    pub(super) chosen_value: ProjectedPositionValue,
    pub(super) alternative_value: ProjectedPositionValue,
}

impl StrategicDeduction {
    pub(super) fn is_valid(&self) -> bool {
        self.observed_turn <= self.inferred_turn
            && self.checked_assignments > 0
            && !self.excluded.is_empty()
            && !self.witnesses.is_empty()
            && self.witnesses.iter().all(|witness| {
                witness.chosen != witness.alternative
                    && witness.chosen_line.steps.len() >= 2
                    && witness.alternative_line.steps.len() >= 2
                    && witness
                        .chosen_line
                        .steps
                        .first()
                        .is_some_and(|step| step.projected.action == witness.chosen)
                    && witness
                        .alternative_line
                        .steps
                        .first()
                        .is_some_and(|step| step.projected.action == witness.alternative)
                    && witness.chosen_line.steps[1..] == witness.alternative_line.steps[1..]
                    && witness.chosen_line.assumptions == witness.alternative_line.assumptions
                    && witness.alternative_value.dominates(witness.chosen_value)
            })
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn deduce(
    deductions: &LogicalDeductions,
    replay: &HGroupState,
    profile: HGroupProfile,
) -> Vec<StrategicDeduction> {
    #[cfg(test)]
    let _profile = crate::test_profile::span("inverse_planning");
    let view = deductions.view();
    if is_active()
        || !rule_enabled(profile, HGroupRuleId::BasicStrategy)
        || view.deck_size == 0
        || view.turn == 0
        // We quantify this observer's hidden hand, not unresolved cards in
        // other hands. A nested/blank-draw projection with additional hidden
        // cards does not supply enough information for this proof contract.
        || view.hands.iter().enumerate().any(|(player, hand)| {
            player != view.observer.index() && hand.iter().any(|card| card.identity.is_none())
        })
    {
        return Vec::new();
    }
    let notes = convention_card_inferences(deductions, replay);
    let eligible = notes
        .iter()
        .filter(|note| {
            note.identities.len() > 1
                && note.play_obligation.is_none()
                && note.identity_status == super::HGroupIdentityStatus::Settled
                && note
                    .identities
                    .iter()
                    .any(|identity| is_playable_now(view, identity))
                && note.identities.iter().next().is_some_and(|first| {
                    note.identities
                        .iter()
                        .all(|identity| identity.rank == first.rank)
                })
                && was_clued_before(view, view.turn, note.card)
        })
        .collect::<Vec<_>>();
    if eligible.is_empty() {
        return Vec::new();
    }
    if let Some(cached) = CACHE.with_borrow(|cache| {
        cache
            .iter()
            .find(|entry| {
                entry.view == *view
                    && entry.profile == profile
                    && entry.notes == notes
                    && entry.clues == replay.clues
            })
            .map(|entry| entry.deductions.clone())
    }) {
        return cached;
    }

    // All hypothetical interpretations use the lower-order model. The replay
    // memo key includes this stage, so a baseline result cannot masquerade as
    // a result with strategic knowledge (or vice versa).
    let _guard = Guard(ACTIVE.replace(true));
    let mut result = Vec::new();
    for note in eligible {
        for identity in note
            .identities
            .iter()
            .filter(|identity| !is_playable_now(view, *identity))
        {
            for clue in replay
                .clues
                .iter()
                .rev()
                .filter(|clue| {
                    clue.giver != view.observer
                        && clue.kind == HGroupClueKind::Play
                        && clue.touched.len() == 1
                        && was_clued_before(view, clue.turn, note.card)
                })
                .take(CHOICE_LIMIT)
            {
                let chosen = Action::Clue {
                    target: clue.target,
                    clue: clue.clue,
                };
                let Some(before) = HistoricalChoice::new(view, clue.turn, chosen) else {
                    continue;
                };
                if let Some(proof) = prove_exclusion(
                    deductions,
                    &notes,
                    profile,
                    &before,
                    note.card,
                    identity,
                    PROOF_LIMITS,
                ) {
                    result.push(proof);
                    break;
                }
            }
        }
    }
    CACHE.with_borrow_mut(|cache| {
        if cache.len() == CACHE_LIMIT {
            cache.remove(0);
        }
        cache.push(CacheEntry {
            view: view.clone(),
            profile,
            notes,
            clues: replay.clues.clone(),
            deductions: result.clone(),
        });
    });
    result
}

struct HistoricalChoice {
    turn: u32,
    chosen: Action,
    giver: hanabi_core::PlayerId,
    history_len: usize,
    hands: Vec<Vec<CardId>>,
    facts: Vec<ClueFacts>,
    deck_size: usize,
}

impl HistoricalChoice {
    fn new(source: &PlayerView, turn: u32, chosen: Action) -> Option<Self> {
        let hand_size = if source.hands.len() <= 3 { 5 } else { 4 };
        let mut hands = (0..source.hands.len())
            .map(|player| {
                (player * hand_size..(player + 1) * hand_size)
                    .map(CardId::new)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let mut facts = vec![ClueFacts::default(); 50];
        let mut deck_size = 50 - source.hands.len() * hand_size;
        for (index, entry) in source.history.iter().enumerate() {
            if entry.turn == turn {
                if let ObservedEvent::Clued { giver, .. } = entry.event {
                    return (deck_size > 0).then_some(Self {
                        turn,
                        chosen,
                        giver,
                        history_len: index,
                        hands,
                        facts,
                        deck_size,
                    });
                }
            }
            match &entry.event {
                ObservedEvent::Clued {
                    clue,
                    touched,
                    untouched,
                    ..
                } => {
                    for card in touched {
                        facts[card.index()].add_positive_clue(*clue);
                    }
                    for card in untouched {
                        facts[card.index()].add_negative_clue(*clue);
                    }
                }
                ObservedEvent::Played { player, card, .. }
                | ObservedEvent::Discarded { player, card, .. } => {
                    hands[player.index()].retain(|held| held != card);
                }
                ObservedEvent::Drew { player, card, .. } => {
                    hands[player.index()].push(*card);
                    deck_size = deck_size.checked_sub(1)?;
                }
            }
        }
        None
    }

    fn giver_view(&self, world: &PlayerView) -> PlayerView {
        super::prospective::subjective_view_before_action(
            world,
            self.giver,
            &world.history[..self.history_len],
            &self.hands,
            &self.facts,
            self.deck_size,
        )
    }
}

fn prove_exclusion(
    deductions: &LogicalDeductions,
    notes: &[HGroupCardInference],
    profile: HGroupProfile,
    before: &HistoricalChoice,
    focal: CardId,
    identity: Card,
    limits: ProofLimits,
) -> Option<StrategicDeduction> {
    if !before.hands[deductions.view().observer.index()].contains(&focal) {
        return None;
    }
    let mut worlds = Vec::<(PlayerView, Option<Arc<SubstitutionWitness>>)>::new();
    let mut checked_assignments = 0;
    let mut inconclusive = false;
    let visit = deductions.visit_hand_assignments(limits.assignments, |assignment| {
        if !assignment.contains(&(focal, identity))
            || !assignment.iter().all(|(card, identity)| {
                notes
                    .iter()
                    .find(|note| note.card == *card)
                    .is_some_and(|note| note.identities.contains(*identity))
            })
        {
            return false;
        }
        let mut world = deductions.view().clone();
        for card in &mut world.hands[world.observer.index()] {
            card.identity = assignment
                .iter()
                .find(|(id, _)| *id == card.id)
                .map(|(_, identity)| *identity);
        }
        let historical = before.giver_view(&world);
        let index = if let Some(index) = worlds.iter().position(|(view, _)| *view == historical) {
            index
        } else {
            if worlds.len() == limits.giver_views {
                inconclusive = true;
                return true;
            }
            let proof = cached_substitution_witness(&historical, profile, before.chosen);
            worlds.push((historical, proof));
            worlds.len() - 1
        };
        if worlds[index].1.is_none() {
            inconclusive = true;
            return true;
        }
        checked_assignments += 1;
        false
    });
    if inconclusive || checked_assignments == 0 || visit.end != HandAssignmentVisitEnd::Exhausted {
        return None;
    }
    Some(StrategicDeduction {
        inferred_turn: deductions.view().turn - 1,
        observed_turn: before.turn,
        card: focal,
        excluded: IdentitySet::singleton(identity),
        checked_assignments,
        witnesses: worlds
            .into_iter()
            .filter_map(|(_, witness)| witness)
            .collect(),
    })
}

fn project(
    view: &PlayerView,
    profile: HGroupProfile,
    action: Action,
    limit: u8,
) -> Option<(SymbolicLineOutcome, ProjectionEvidence)> {
    super::symbolic_line::project_h_group_projection(
        view,
        profile,
        action,
        limit,
        &crate::AnalysisControl::default(),
    )
    .ok()
}

type SharedProjection = Rc<(SymbolicLineOutcome, ProjectionEvidence)>;

/// Scoped to one immutable historical view and profile. Cache the requested
/// horizon, not the number of actions reached: uncertainty and a turn limit
/// produce different evidence even when the action sequences are identical.
struct ProjectionCache<'a> {
    view: &'a PlayerView,
    profile: HGroupProfile,
    entries: Vec<(Action, u8, Option<SharedProjection>)>,
}

impl ProjectionCache<'_> {
    fn get(&mut self, action: Action, limit: u8) -> Option<SharedProjection> {
        #[cfg(test)]
        if BYPASS_PROJECTION_CACHE.get() {
            return project(self.view, self.profile, action, limit).map(Rc::new);
        }
        if let Some((_, _, result)) = self
            .entries
            .iter()
            .find(|(a, l, _)| *a == action && *l == limit)
        {
            return result.clone();
        }
        let result = project(self.view, self.profile, action, limit).map(Rc::new);
        self.entries.push((action, limit, result.clone()));
        result
    }
}

fn incompatible_action_prefixes(
    baseline: &ProjectionEvidence,
    alternate: &ProjectionEvidence,
    common: u8,
) -> bool {
    #[cfg(test)]
    if BYPASS_PROJECTION_CACHE.get() {
        return false;
    }
    // The horizon only stops the deterministic projector; it never influences
    // action selection. Different completed suffix steps therefore cannot
    // become equal by projecting those same prefixes again at a shorter limit.
    // This is only an early rejection. Equal prefixes still need the original
    // common-frontier projections (including their assumptions and resources).
    let range = 1..usize::from(common);
    match (
        baseline.steps.get(range.clone()),
        alternate.steps.get(range),
    ) {
        (Some(baseline), Some(alternate)) => baseline != alternate,
        // A branching projection can summarize a longer common continuation
        // than its unbranched prefix. This shortcut proves nothing about it.
        _ => false,
    }
}

/// Later observations can yield the exact same historical giver query. Reuse
/// its lower-order certificate, not an inference made in a different present
/// view. Present-day hand coverage is always enumerated again independently.
fn cached_substitution_witness(
    view: &PlayerView,
    profile: HGroupProfile,
    chosen: Action,
) -> Option<Arc<SubstitutionWitness>> {
    if let Some(result) = WITNESS_CACHE.with_borrow(|cache| {
        cache
            .iter()
            .find(|entry| entry.view == *view && entry.profile == profile && entry.chosen == chosen)
            .map(|entry| entry.witness.clone())
    }) {
        return result;
    }
    let result = substitution_witness(view, profile, chosen).map(Arc::new);
    WITNESS_CACHE.with_borrow_mut(|cache| {
        if cache.len() == WITNESS_CACHE_LIMIT {
            cache.remove(0);
        }
        cache.push(WitnessCacheEntry {
            view: view.clone(),
            profile,
            chosen,
            witness: result.clone(),
        });
    });
    result
}

fn substitution_witness(
    view: &PlayerView,
    profile: HGroupProfile,
    chosen: Action,
) -> Option<SubstitutionWitness> {
    let deductions = LogicalDeductions::new(view.clone()).ok()?;
    let analysis = SupportedConvention::HGroup(profile).analyze(&deductions);
    if analysis.forced_action.is_some() {
        return None;
    }
    let original = analysis
        .actions
        .iter()
        .find(|candidate| candidate.action == chosen)?;
    if original.preference.policy_tier() != crate::ConventionPolicyTier::Admitted {
        return None;
    }
    // A certificate often compares several alternatives at the same frontier.
    // Reuse only identical (action, requested horizon) queries: a line stopped
    // by uncertainty is NOT equivalent to one stopped at an action limit.
    let mut projections = ProjectionCache {
        view,
        profile,
        entries: Vec::new(),
    };
    let baseline = projections.get(chosen, LINE_LIMIT)?;
    for candidate in &analysis.actions {
        if candidate.action == chosen
            || !matches!(candidate.action, Action::Clue { .. })
            || candidate.preference.policy_tier() != original.preference.policy_tier()
        {
            continue;
        }
        let Some(alternate) = projections.get(candidate.action, LINE_LIMIT) else {
            continue;
        };
        let common = baseline.0.actions.min(alternate.0.actions);
        // Require demonstrated follow-through, not just the new root promise.
        if common < 2 {
            continue;
        }
        if incompatible_action_prefixes(&baseline.1, &alternate.1, common) {
            continue;
        }
        let Some(alternate_common) = projections.get(candidate.action, common) else {
            continue;
        };
        let Some(baseline_common) = projections.get(chosen, common) else {
            continue;
        };
        let (a, a_line) = alternate_common.as_ref();
        let (b, b_line) = baseline_common.as_ref();
        if a.actions != common
            || b.actions != common
            // A substitution certificate currently proves one identical
            // continuation, not equivalence between conditional branch trees.
            || !a_line.clue_branches.is_empty()
            || !b_line.clue_branches.is_empty()
            || a.strikes != 0
            || b.strikes != 0
            || a_line.maximum_save_violations() != 0
            || b_line.maximum_save_violations() != 0
            || a_line.steps[1..] != b_line.steps[1..]
            || a_line.assumptions != b_line.assumptions
            || a_line.resources.unfunded_turn.is_some()
            || b_line.resources.unfunded_turn.is_some()
            || a_line
                .dependencies
                .iter()
                .any(|dependency| dependency.status != super::DependencyStatus::Supported)
            || b_line
                .dependencies
                .iter()
                .any(|dependency| dependency.status != super::DependencyStatus::Supported)
        {
            continue;
        }
        let (Some(a_value), Some(b_value)) = (a.position_value, b.position_value) else {
            continue;
        };
        if a_value.dominates(b_value) {
            return Some(SubstitutionWitness {
                chosen,
                alternative: candidate.action,
                chosen_line: b_line.clone(),
                alternative_line: a_line.clone(),
                chosen_value: b_value,
                alternative_value: a_value,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use hanabi_core::{Clue, PlayerId, Rank, Suit};

    fn reviewed_position() -> PlayerView {
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s2.json"
        ))
        .unwrap();
        replay
            .state_at_turn(36)
            .unwrap()
            .view_for(PlayerId::new(0))
            .unwrap()
    }

    /// An expensive differential check, not a second strategic authority. Both
    /// runs enumerate the full reviewed hand space with cold certificate caches.
    #[test]
    #[ignore = "full cached/uncached inverse-proof comparison; run for cache changes"]
    fn projection_cache_preserves_complete_reviewed_proof() {
        let deductions = LogicalDeductions::new(reviewed_position()).unwrap();
        let run = |bypass| {
            CACHE.with_borrow_mut(Vec::clear);
            WITNESS_CACHE.with_borrow_mut(Vec::clear);
            BYPASS_PROJECTION_CACHE.set(bypass);
            let started = std::time::Instant::now();
            let replay = super::super::replay_h_group(&deductions, HGroupProfile::Max);
            eprintln!("projection cache bypass={bypass}: {:?}", started.elapsed());
            BYPASS_PROJECTION_CACHE.set(false);
            assert!(!replay.strategic_deductions.is_empty());
            (
                convention_card_inferences(&deductions, &replay),
                replay.knowledge.effects().to_vec(),
                replay.strategic_deductions,
            )
        };
        assert_eq!(run(true), run(false));
    }

    #[test]
    fn incomplete_joint_enumeration_never_excludes_an_identity() {
        baseline(|| {
            let deductions = LogicalDeductions::new(reviewed_position()).unwrap();
            let replay = super::super::replay_h_group(&deductions, HGroupProfile::Max);
            let notes = convention_card_inferences(&deductions, &replay);
            let choice = HistoricalChoice::new(
                deductions.view(),
                30,
                Action::Clue {
                    target: PlayerId::new(0),
                    clue: Clue::Rank(Rank::Two),
                },
            )
            .unwrap();
            for limits in [
                ProofLimits {
                    assignments: 1,
                    giver_views: 128,
                },
                ProofLimits {
                    assignments: 4096,
                    giver_views: 0,
                },
            ] {
                assert!(
                    prove_exclusion(
                        &deductions,
                        &notes,
                        HGroupProfile::Max,
                        &choice,
                        CardId::new(2),
                        Card::new(Suit::Green, Rank::Four),
                        limits
                    )
                    .is_none()
                );
            }
        });
    }

    #[test]
    fn historical_giver_view_excludes_later_events_and_own_card_faces() {
        let view = reviewed_position();
        let choice = HistoricalChoice::new(
            &view,
            30,
            Action::Clue {
                target: PlayerId::new(0),
                clue: Clue::Rank(Rank::Two),
            },
        )
        .unwrap();
        let before = choice.giver_view(&view);
        assert_eq!(before.observer, PlayerId::new(2));
        assert_eq!(before.turn, 30);
        assert!(before.history.iter().all(|entry| entry.turn < 30));
        assert!(before.hands[2].iter().all(|card| card.identity.is_none()));
        assert!(before.history.iter().all(|entry| !matches!(&entry.event,
            ObservedEvent::Drew { player, identity: Some(_), .. } if *player == PlayerId::new(2))));
        assert!(
            !before
                .hands
                .iter()
                .flatten()
                .any(|card| card.id == CardId::new(36))
        );
    }

    #[test]
    fn unresolved_teammate_cards_cannot_support_a_hand_only_proof() {
        let mut view = reviewed_position();
        let d = LogicalDeductions::new(view.clone()).unwrap();
        let replay = baseline(|| super::super::replay_h_group(&d, HGroupProfile::Max));
        view.hands[1][0].identity = None;
        let d = LogicalDeductions::new(view).unwrap();
        assert!(deduce(&d, &replay, HGroupProfile::Max).is_empty());
    }

    #[test]
    fn nested_baseline_scopes_restore_the_previous_stage() {
        assert!(!is_active());
        baseline(|| {
            assert!(is_active());
            baseline(|| assert!(is_active()));
            assert!(is_active());
        });
        assert!(!is_active());
    }
}
