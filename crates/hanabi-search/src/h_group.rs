//! H-Group convention inference.
//!
//! This module deliberately contains interpretations, not game rules or
//! logical clue facts. H-Group profiles are cumulative: a level-N profile
//! enables every interpretation through level N, while `max` also enables the
//! rare moves in the extras chapters of the pinned ruleset.

use std::collections::HashSet;

use hanabi_core::{
    Action, Card, CardId, Clue, ClueFacts, MAX_CLUE_TOKENS, ObservedEvent, ObservedHistoryEntry,
    PlayerId, PlayerView, Rank,
};

use crate::{HGroupLevel, HGroupProfile, IdentitySet, LogicalDeductions};

/// Semantic families used by the cumulative H-Group interpreter.
///
/// The documentation gives many combinations their own names. The engine
/// represents combinations as a sequence of these primitive effects instead
/// of duplicating state-transition code for every name. For example, a Trash
/// Push Finesse is represented by `TrashPush` followed by `Finesse`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum HGroupMoveKind {
    PlayClue,
    SaveClue,
    FiveStall,
    Prompt,
    Finesse,
    ReverseFinesse,
    SelfFinesse,
    LayeredFinesse,
    FixClue,
    SarcasticDiscard,
    ChopMove,
    TempoClue,
    EmergencyDiscard,
    PositionalDiscard,
    Stall,
    TransferDiscard,
    Bluff,
    DoubleBluff,
    SelfishClue,
    Context,
    TrashPush,
    Ejection,
    Discharge,
    Duplication,
    Elimination,
    FivePull,
    OccupiedPlay,
    Ignition,
    PhantomPlayable,
    Charm,
    UnnecessaryMove,
    Priority,
    Extra,
}

/// Machine-readable coverage metadata for one cumulative learning-path level.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HGroupLevelDescriptor {
    pub profile: HGroupProfile,
    pub title: &'static str,
    pub effects: &'static [HGroupMoveKind],
}

/// The cumulative learning path, with `max` represented as effective level 26.
pub const H_GROUP_LEVELS: [HGroupLevelDescriptor; 26] = [
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level1),
        title: "Basic conventions",
        effects: &[
            HGroupMoveKind::PlayClue,
            HGroupMoveKind::SaveClue,
            HGroupMoveKind::Prompt,
            HGroupMoveKind::Finesse,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level2),
        title: "Basic moves",
        effects: &[
            HGroupMoveKind::FiveStall,
            HGroupMoveKind::ReverseFinesse,
            HGroupMoveKind::SelfFinesse,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level3),
        title: "Basic strategy",
        effects: &[HGroupMoveKind::FixClue, HGroupMoveKind::SarcasticDiscard],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level4),
        title: "Chop moves",
        effects: &[HGroupMoveKind::ChopMove],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level5),
        title: "Special finesses",
        effects: &[HGroupMoveKind::LayeredFinesse],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level6),
        title: "Tempo clues",
        effects: &[HGroupMoveKind::TempoClue],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level7),
        title: "Emergency discards",
        effects: &[HGroupMoveKind::EmergencyDiscard],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level8),
        title: "End-game",
        effects: &[HGroupMoveKind::PositionalDiscard],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level9),
        title: "Stalling",
        effects: &[HGroupMoveKind::Stall],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level10),
        title: "Special discards",
        effects: &[HGroupMoveKind::TransferDiscard],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level11),
        title: "Bluffs",
        effects: &[HGroupMoveKind::Bluff],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level12),
        title: "Context",
        effects: &[HGroupMoveKind::SelfishClue, HGroupMoveKind::Context],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level13),
        title: "Intermediate bluffs",
        effects: &[HGroupMoveKind::Bluff],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level14),
        title: "Trash moves",
        effects: &[HGroupMoveKind::TrashPush],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level15),
        title: "Double bluffs",
        effects: &[HGroupMoveKind::DoubleBluff],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level16),
        title: "Ejections and discharges",
        effects: &[HGroupMoveKind::Ejection, HGroupMoveKind::Discharge],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level17),
        title: "Duplication",
        effects: &[HGroupMoveKind::Duplication],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level18),
        title: "Elimination",
        effects: &[HGroupMoveKind::Elimination],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level19),
        title: "5 tech",
        effects: &[HGroupMoveKind::FivePull],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level20),
        title: "Out-of-order play",
        effects: &[HGroupMoveKind::OccupiedPlay],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level21),
        title: "Ignition",
        effects: &[HGroupMoveKind::Ignition],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level22),
        title: "Phantom playable cards",
        effects: &[HGroupMoveKind::PhantomPlayable],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level23),
        title: "Charms",
        effects: &[HGroupMoveKind::Charm],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level24),
        title: "Unnecessary moves",
        effects: &[HGroupMoveKind::UnnecessaryMove],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level25),
        title: "Priority",
        effects: &[HGroupMoveKind::Priority],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Max,
        title: "Max",
        effects: &[HGroupMoveKind::Extra],
    },
];

/// Returns the cumulative rules enabled by `profile`.
#[must_use]
pub fn enabled_h_group_levels(profile: HGroupProfile) -> &'static [HGroupLevelDescriptor] {
    &H_GROUP_LEVELS[..usize::from(profile.effective_level())]
}

/// One convention interpretation found while replaying public history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HGroupSignal {
    pub turn: u32,
    pub actor: PlayerId,
    pub target: Option<PlayerId>,
    pub kind: HGroupMoveKind,
    /// Cards whose conventional status changed, in resolution order.
    pub cards: Vec<CardId>,
    /// Identity promised by the signal when it has one.
    pub identity: Option<Card>,
}

/// Coarse phase used by the stalling, 5-tech, and end-game rules.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HGroupPhase {
    #[default]
    EarlyGame,
    LowScore,
    Normal,
    EndGame,
}

/// The Level 1 meaning assigned to a clue's focus.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HGroupClueKind {
    Play,
    Save(HGroupSaveKind),
    /// The observer cannot distinguish a Play clue from a critical Save.
    PlayOrSave,
    /// The clue has no meaning in the implemented Level 1 vocabulary.
    Unrecognized,
}

/// Why a Level 1 Save clue protects a chop card.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HGroupSaveKind {
    Five,
    Two,
    Critical,
}

/// How a delayed Play clue identifies its next connecting card.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HGroupConnectionKind {
    /// A previously-clued card matching the connection, preferred by Level 1.
    Prompt,
    /// The newest unclued card when no Prompt exists.
    Finesse,
}

/// One public clue interpreted using H-Group Level 1 focus rules.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HGroupClueInterpretation {
    pub turn: u32,
    pub giver: PlayerId,
    pub target: PlayerId,
    pub clue: Clue,
    pub focus: CardId,
    pub focus_was_chop: bool,
    pub kind: HGroupClueKind,
    /// Convention identities retained in the focus card's note.
    pub focus_identities: IdentitySet,
    pub play_identities: IdentitySet,
    pub save_identities: IdentitySet,
    /// Cards first touched by this clue other than the focus.
    pub new_non_focus: Vec<CardId>,
    /// Good-Touch identities for each newly touched non-focus card.
    pub non_focus_identities: Vec<(CardId, IdentitySet)>,
    /// Explicit and invisible clues that existed before this clue.
    pub previously_gotten: Vec<CardId>,
}

/// A card promised to be the next card in a delayed Play clue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HGroupConnection {
    pub card: CardId,
    pub identity: Card,
    pub kind: HGroupConnectionKind,
    pub focus: CardId,
}

/// A disjunctive Prompt promise: the first matching card is the connection,
/// while every earlier candidate must instead be immediately playable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HGroupConnectionPromise {
    pub cards: Vec<CardId>,
    pub identity: Card,
}

/// Convention knowledge attached to one card in the observer's hand.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HGroupCardInference {
    pub card: CardId,
    pub identities: IdentitySet,
    pub focused: bool,
    pub saved: bool,
    pub finessed: bool,
}

/// H-Group-specific conclusions for the player owning the view.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HGroupInferences {
    pub clues: Vec<HGroupClueInterpretation>,
    /// Current chop for every player, in player order.
    pub chops: Vec<Option<CardId>>,
    /// Own cards promised playable now by an H-Group interpretation.
    pub playable_now: Vec<CardId>,
    /// Own cards protected by a Save interpretation.
    pub saved: Vec<CardId>,
    /// An immediate Prompt or Finesse obligation for the acting player.
    pub connection: Option<HGroupConnection>,
    /// Convention-narrowed notes for cards in the observer's own hand.
    pub cards: Vec<HGroupCardInference>,
    /// Whether no player has yet discarded their chop.
    pub early_game: bool,
    /// Cards treated as clued because a Finesse already got them.
    pub invisibly_clued: Vec<CardId>,
    /// Ambiguous Prompt and layered-Finesse chains that root worlds must
    /// satisfy.
    pub connection_promises: Vec<HGroupConnectionPromise>,
    /// All convention effects recognized in public history.
    pub signals: Vec<HGroupSignal>,
    /// Cards carrying a permanent invisible chop-move clue.
    pub chop_moved: Vec<CardId>,
    /// Own cards conventionally required to be discarded before ordinary
    /// fallback behavior (e.g. a Certain or positional discard).
    pub discard_now: Vec<CardId>,
    /// Players who have been forbidden to discard for their next action.
    pub must_clue: Vec<PlayerId>,
    pub phase: HGroupPhase,
}

#[derive(Clone, Debug)]
struct Replay {
    hands: Vec<Vec<CardId>>,
    explicitly_clued: HashSet<CardId>,
    invisibly_clued: HashSet<CardId>,
    clues: Vec<HGroupClueInterpretation>,
    pending_connections: Vec<PendingConnection>,
    already_playing: HashSet<CardId>,
    early_game: bool,
    signals: Vec<HGroupSignal>,
    chop_moved: HashSet<CardId>,
    discard_now: Vec<CardId>,
    must_clue: HashSet<PlayerId>,
    forced_playable: HashSet<CardId>,
}

#[derive(Clone, Debug)]
struct PendingConnection {
    actor: PlayerId,
    cards: Vec<CardId>,
    expected: Card,
    kind: HGroupConnectionKind,
    focus: CardId,
    /// Zero-based position in a multi-connection chain.
    step: u8,
}

/// Applies the implemented cumulative H-Group semantics to a logical view.
#[must_use]
pub fn infer_h_group(deductions: &LogicalDeductions, profile: HGroupProfile) -> HGroupInferences {
    let view = deductions.view();
    let replay = replay_h_group(deductions, profile);
    let mut gotten = replay
        .explicitly_clued
        .union(&replay.invisibly_clued)
        .copied()
        .collect::<HashSet<_>>();
    gotten.extend(replay.chop_moved.iter().copied());
    let chops = replay
        .hands
        .iter()
        .map(|hand| chop(hand, &gotten))
        .collect::<Vec<_>>();
    let cards = convention_card_inferences(deductions, &replay);
    let mut inferred = HGroupInferences {
        clues: replay.clues,
        chops,
        cards,
        early_game: replay.early_game,
        invisibly_clued: replay.invisibly_clued.iter().copied().collect(),
        signals: replay.signals,
        chop_moved: replay.chop_moved.iter().copied().collect(),
        discard_now: replay.discard_now,
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
        if card.saved {
            inferred.saved.push(card.card);
        }
        if !card.identities.is_empty()
            && card
                .identities
                .iter()
                .all(|identity| is_playable_now(view, identity))
        {
            inferred.playable_now.push(card.card);
        }
    }

    let connection = replay
        .pending_connections
        .iter()
        .filter(|pending| {
            pending.actor == view.observer
                && pending_is_active(pending, &replay.pending_connections)
        })
        .min_by_key(|pending| match pending.kind {
            HGroupConnectionKind::Prompt => 0,
            HGroupConnectionKind::Finesse => 1,
        })
        .and_then(|pending| pending.cards.first().map(|card| (pending, *card)));
    if let Some((pending, card)) = connection {
        inferred.connection = Some(HGroupConnection {
            card,
            identity: pending.expected,
            kind: pending.kind,
            focus: pending.focus,
        });
    } else if matches!(
        view.history.last().map(|entry| &entry.event),
        Some(ObservedEvent::Clued { .. })
    ) {
        if let Some(latest) = inferred
            .clues
            .last()
            .filter(|latest| latest.target == view.observer)
            .cloned()
        {
            let previously_gotten = latest.previously_gotten.iter().copied().collect();
            infer_clue_to_self(deductions, &latest, &previously_gotten, &mut inferred);
        }
    }
    inferred
}

/// Actions permitted by the implemented Level 1 principles, in policy order.
#[must_use]
pub(crate) fn h_group_candidate_actions(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
) -> Vec<Action> {
    let view = deductions.view();
    let legal_actions = view.legal_actions();
    if legal_actions.is_empty() {
        return Vec::new();
    }
    let inferred = infer_h_group(deductions, profile);
    let mut clue_candidates = h_group_clue_candidates(deductions, profile);
    clue_candidates.sort_by_key(|candidate| core::cmp::Reverse(candidate.score));

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
        legal_connection_actions(connection, &clue_candidates, &legal_actions)
    }) {
        return actions;
    }

    let urgent = clue_candidates
        .iter()
        .filter(|candidate| candidate.score >= 450)
        .map(|candidate| candidate.action)
        .collect::<Vec<_>>();
    if !urgent.is_empty() {
        return urgent;
    }
    let mut actions = inferred
        .discard_now
        .iter()
        .copied()
        .map(Action::Discard)
        .collect::<Vec<_>>();
    actions.extend(
        ordered_playable_cards(view, &inferred, profile)
            .into_iter()
            .map(Action::Play),
    );
    actions.extend(clue_candidates.iter().map(|candidate| candidate.action));
    actions.dedup();
    actions.retain(|action| legal_actions.contains(action));
    if !actions.is_empty() {
        return actions;
    }

    let gotten = inferred
        .clues
        .iter()
        .flat_map(|clue| core::iter::once(clue.focus).chain(clue.new_non_focus.iter().copied()))
        .chain(inferred.invisibly_clued.iter().copied())
        .collect::<HashSet<_>>();
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
            if !inferred.saved.contains(&chop) {
                return vec![Action::Discard(chop)];
            }
        }
    }
    if view.clue_tokens < MAX_CLUE_TOKENS {
        if let Some(forced) = own_hand
            .iter()
            .find(|card| !inferred.saved.contains(&card.id))
        {
            return vec![Action::Discard(forced.id)];
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

fn legal_connection_actions(
    connection: HGroupConnection,
    clue_candidates: &[ClueCandidate],
    legal_actions: &[Action],
) -> Option<Vec<Action>> {
    let mut actions = clue_candidates
        .iter()
        .filter(|candidate| candidate.score >= 450)
        .map(|candidate| candidate.action)
        .chain(core::iter::once(Action::Play(connection.card)))
        .collect::<Vec<_>>();
    actions.dedup();
    actions.retain(|action| legal_actions.contains(action));
    (!actions.is_empty()).then_some(actions)
}

fn ordered_playable_cards(
    view: &PlayerView,
    inferred: &HGroupInferences,
    profile: HGroupProfile,
) -> Vec<CardId> {
    let mut cards = inferred.playable_now.clone();
    if !profile.includes(HGroupLevel::Level3) || cards.len() < 2 {
        return cards;
    }
    let own_hand = &view.hands[view.observer.index()];
    let initial_hand_size = if view.hands.len() <= 3 { 5 } else { 4 };
    let initial_cards = initial_hand_size * view.hands.len();
    cards.sort_by_key(|card| {
        let position = own_hand
            .iter()
            .position(|candidate| candidate.id == *card)
            .unwrap_or(0);
        let note = inferred.cards.iter().find(|note| note.card == *card);
        let singleton = note
            .filter(|note| note.identities.len() == 1)
            .and_then(|note| note.identities.iter().next());
        let rank = singleton.map_or(6, |identity| identity.rank.number());
        let fresh_one = rank == 1 && card.index() >= initial_cards;
        let starting_one = rank == 1 && !fresh_one;
        let chop_focused = inferred.clues.iter().any(|clue| {
            clue.focus == *card
                && clue.focus_was_chop
                && clue
                    .focus_identities
                    .iter()
                    .all(|identity| identity.rank == Rank::One)
        });
        if !profile.includes(HGroupLevel::Level25) || inferred.phase == HGroupPhase::EndGame {
            return (
                !chop_focused,
                !fresh_one,
                !starting_one,
                if fresh_one {
                    usize::MAX - position
                } else {
                    position
                },
                0_u8,
                0_u8,
            );
        }
        let blind = note.is_some_and(|note| note.finessed);
        let leads_other = singleton.is_some_and(|identity| {
            let next = Card::new(
                identity.suit,
                Rank::ALL
                    .get(identity.rank.index() + 1)
                    .copied()
                    .unwrap_or(Rank::Five),
            );
            identity.rank != Rank::Five
                && view
                    .hands
                    .iter()
                    .enumerate()
                    .filter(|(player, _)| *player != view.observer.index())
                    .flat_map(|(_, hand)| hand)
                    .any(|candidate| {
                        candidate.identity == Some(next) && !candidate.clues.is_empty()
                    })
        });
        let leads_self = singleton.is_some_and(|identity| {
            if identity.rank == Rank::Five {
                return false;
            }
            let next = Card::new(identity.suit, Rank::ALL[identity.rank.index() + 1]);
            inferred
                .cards
                .iter()
                .any(|candidate| candidate.card != *card && candidate.identities.contains(next))
        });
        (
            !blind,
            !leads_other,
            !leads_self,
            usize::from(rank != 5),
            rank,
            u8::try_from(own_hand.len().saturating_sub(position)).unwrap_or(u8::MAX),
        )
    });
    cards
}

#[must_use]
pub(crate) fn select_h_group_action(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
) -> Option<Action> {
    h_group_candidate_actions(deductions, profile)
        .into_iter()
        .next()
}

/// Selects a conservative, unambiguous Level 1 clue.
///
/// Candidate clues must satisfy focus and Minimum Clue Value. Play clues also
/// satisfy Good Touch and either play now or create exactly one valid Prompt
/// or Finesse connection. Save clues are restricted to the Level 1 5, 2, and
/// critical-card forms.
#[derive(Clone, Copy, Debug)]
struct ClueCandidate {
    action: Action,
    score: u16,
    target: PlayerId,
    save: bool,
    immediate_play: bool,
}

#[allow(clippy::too_many_lines)]
fn h_group_clue_candidates(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
) -> Vec<ClueCandidate> {
    let view = deductions.view();
    if view.clue_tokens == 0 {
        return Vec::new();
    }
    let replay = replay_h_group(deductions, profile);
    let mut gotten = replay
        .explicitly_clued
        .union(&replay.invisibly_clued)
        .copied()
        .collect::<HashSet<_>>();
    gotten.extend(replay.chop_moved.iter().copied());
    let next_player = PlayerId::new(
        u8::try_from((view.current_player.index() + 1) % view.hands.len())
            .expect("standard Hanabi has at most five players"),
    );
    let mut candidates = Vec::new();

    for action in view.legal_actions() {
        let Action::Clue { target, clue } = action else {
            continue;
        };
        let hand = &view.hands[target.index()];
        let layout = &replay.hands[target.index()];
        let touched = hand
            .iter()
            .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
            .map(|card| card.id)
            .collect::<Vec<_>>();
        let newly_touched = touched
            .iter()
            .copied()
            .filter(|card| !gotten.contains(card))
            .collect::<Vec<_>>();
        // Minimum Clue Value: Level 1 does not spend tempo clues.
        if newly_touched.is_empty() {
            continue;
        }
        let old_chop = chop(layout, &gotten);
        let Some(focus) = focus(layout, &touched, old_chop, &gotten) else {
            continue;
        };
        let focus_identity = hand
            .iter()
            .find(|card| card.id == focus)
            .and_then(|card| card.identity)
            .expect("another player's cards are visible");

        let save_score = if old_chop == Some(focus) {
            save_clue_score(
                view,
                hand,
                focus,
                focus_identity,
                clue,
                target,
                next_player,
                &replay.hands,
                &gotten,
            )
        } else {
            None
        };
        let play_score = play_clue_score(
            view,
            target,
            focus,
            focus_identity,
            clue,
            &newly_touched,
            &gotten,
            &replay.already_playing,
        );
        if let Some(mut score) = play_score {
            if old_chop == Some(focus)
                && target == next_player
                && is_unique_visible(view, focus, focus_identity)
            {
                score += 120;
            }
            candidates.push(ClueCandidate {
                action,
                score,
                target,
                save: false,
                immediate_play: is_playable_now(view, focus_identity),
            });
        } else if let Some(score) = save_score {
            candidates.push(ClueCandidate {
                action,
                score,
                target,
                save: true,
                immediate_play: false,
            });
        }
    }

    let endangered_targets = candidates
        .iter()
        .filter(|candidate| candidate.save)
        .map(|candidate| candidate.target)
        .collect::<HashSet<_>>();
    for candidate in &mut candidates {
        if candidate.immediate_play && endangered_targets.contains(&candidate.target) {
            candidate.score = 500;
        }
    }

    if profile.includes(HGroupLevel::Level2) {
        for candidate in advanced_clue_candidates(view, &replay, &gotten, profile) {
            if !candidates
                .iter()
                .any(|existing| existing.action == candidate.action)
            {
                candidates.push(candidate);
            }
        }
    }
    if profile.includes(HGroupLevel::Level19)
        && view.play_stacks.iter().map(Vec::len).sum::<usize>() < 5
    {
        candidates.retain(|candidate| {
            !matches!(
                candidate.action,
                Action::Clue {
                    clue: Clue::Rank(Rank::Five),
                    ..
                }
            ) || candidate.save
                || !candidate.immediate_play
        });
    }

    let observer_chop = chop(&replay.hands[view.observer.index()], &gotten);
    if candidates.is_empty()
        && (observer_chop.is_none()
            || view.clue_tokens == MAX_CLUE_TOKENS
            || has_out_of_order_prompt(view, &gotten))
    {
        candidates.extend(tempo_clue_candidates(view, &replay, &gotten));
    }
    if candidates.is_empty() && view.clue_tokens == MAX_CLUE_TOKENS {
        // A pathological deal can leave a Level 1 player with no legal
        // Play/Save clue while the game rules also forbid discarding. In that
        // genuinely forced situation, retain all rule-legal clues so search
        // can choose the least harmful one.
        candidates.extend(view.legal_actions().into_iter().filter_map(|action| {
            let Action::Clue { target, clue } = action else {
                return None;
            };
            Some(ClueCandidate {
                action,
                score: 1 + u16::from(matches!(clue, Clue::Suit(_))),
                target,
                save: false,
                immediate_play: false,
            })
        }));
    }
    candidates
}

#[allow(clippy::too_many_lines)]
fn advanced_clue_candidates(
    view: &PlayerView,
    replay: &Replay,
    gotten: &HashSet<CardId>,
    profile: HGroupProfile,
) -> Vec<ClueCandidate> {
    if view.clue_tokens == 0 {
        return Vec::new();
    }
    let actor_locked = replay.hands[view.observer.index()]
        .iter()
        .all(|card| gotten.contains(card) || replay.chop_moved.contains(card));
    let stalling = replay.early_game || actor_locked || view.clue_tokens == MAX_CLUE_TOKENS;
    let mut candidates = Vec::new();
    for action in view.legal_actions() {
        let Action::Clue { target, clue } = action else {
            continue;
        };
        let hand = &view.hands[target.index()];
        let layout = &replay.hands[target.index()];
        let touched = hand
            .iter()
            .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
            .map(|card| card.id)
            .collect::<Vec<_>>();
        if touched.is_empty() {
            continue;
        }
        let newly_touched = touched
            .iter()
            .copied()
            .filter(|card| !gotten.contains(card) && !replay.chop_moved.contains(card))
            .collect::<Vec<_>>();
        let identities = touched
            .iter()
            .filter_map(|card| current_card_identity(view, *card))
            .collect::<Vec<_>>();
        let all_trash = identities
            .iter()
            .all(|identity| card_is_trash(view, *identity));
        let playable = identities
            .iter()
            .filter(|identity| is_playable_now(view, **identity))
            .count();
        let off_chop_five = clue == Clue::Rank(Rank::Five)
            && identities
                .iter()
                .any(|identity| identity.rank == Rank::Five)
            && chop(layout, gotten).is_none_or(|card| !touched.contains(&card));
        let tempo = newly_touched.is_empty()
            && touched.iter().any(|card| {
                current_card_identity(view, *card)
                    .is_some_and(|identity| is_playable_now(view, identity))
            });
        let fix = newly_touched.is_empty() && playable == 0;
        let five_ejection = matches!(clue, Clue::Suit(_))
            && identities
                .iter()
                .any(|identity| identity.rank == Rank::Five)
            && identities
                .iter()
                .any(|identity| identity.rank != Rank::Five);
        let elimination = touched.len() == 1 && gotten.contains(&touched[0]);
        let delayed = identities.iter().find(|identity| {
            usize::from(identity.rank.number()) > view.play_stacks[identity.suit.index()].len() + 1
        });
        let bluff = delayed.is_some_and(|focus| {
            let actor = next_player(view.current_player, view.hands.len());
            if actor == target {
                return false;
            }
            visible_playable_in_hand(view, actor, None).is_some_and(|(_, actual)| {
                let height = view.play_stacks[focus.suit.index()].len();
                height < Rank::ALL.len() && actual != Card::new(focus.suit, Rank::ALL[height])
            })
        });

        let classification = if profile.includes(HGroupLevel::Level21) && playable >= 2 {
            Some((HGroupMoveKind::Ignition, 360))
        } else if profile.includes(HGroupLevel::Level16) && five_ejection {
            Some((HGroupMoveKind::Ejection, 290))
        } else if profile.includes(HGroupLevel::Level11) && bluff {
            Some((HGroupMoveKind::Bluff, 280))
        } else if profile.includes(HGroupLevel::Level18) && elimination {
            Some((HGroupMoveKind::Elimination, 230))
        } else if profile.includes(HGroupLevel::Level20) && delayed.is_some() {
            Some((HGroupMoveKind::OccupiedPlay, 220))
        } else if profile.includes(HGroupLevel::Level4) && all_trash {
            Some((HGroupMoveKind::ChopMove, 210))
        } else if profile.includes(HGroupLevel::Level6) && tempo {
            let valuable = playable >= 2 || actor_locked;
            if valuable || stalling {
                Some((HGroupMoveKind::TempoClue, if valuable { 205 } else { 90 }))
            } else {
                Some((HGroupMoveKind::ChopMove, 180))
            }
        } else if profile.includes(HGroupLevel::Level3) && fix {
            Some((HGroupMoveKind::FixClue, 170))
        } else if profile.includes(HGroupLevel::Level19) && off_chop_five {
            Some((HGroupMoveKind::FivePull, 150))
        } else if profile.is_max()
            && !newly_touched.is_empty()
            && touched.len() > newly_touched.len()
        {
            Some((HGroupMoveKind::Extra, 145))
        } else if off_chop_five && stalling {
            Some((HGroupMoveKind::FiveStall, 80))
        } else if profile.includes(HGroupLevel::Level23)
            && clue == Clue::Rank(Rank::Four)
            && stalling
        {
            Some((HGroupMoveKind::Charm, 70))
        } else if profile.includes(HGroupLevel::Level9) && stalling {
            Some((HGroupMoveKind::Stall, 40))
        } else {
            None
        };
        let Some((_kind, score)) = classification else {
            continue;
        };
        candidates.push(ClueCandidate {
            action,
            score: score + u16::from(matches!(clue, Clue::Suit(_))),
            target,
            save: false,
            immediate_play: playable > 0,
        });
    }
    candidates
}

#[allow(clippy::too_many_arguments)]
fn save_clue_score(
    view: &PlayerView,
    target_hand: &[hanabi_core::ObservedCard],
    focus: CardId,
    identity: Card,
    clue: Clue,
    target: PlayerId,
    next_player: PlayerId,
    layouts: &[Vec<CardId>],
    gotten: &HashSet<CardId>,
) -> Option<u16> {
    let chops = layouts
        .iter()
        .map(|hand| chop(hand, gotten))
        .collect::<Vec<_>>();
    let valid = match (clue, identity.rank) {
        (Clue::Rank(Rank::Five), Rank::Five) => true,
        (Clue::Rank(Rank::Two), Rank::Two) => {
            !convention_playable(view, gotten, focus, identity)
                && two_save_allowed(view, focus, identity, &chops)
        }
        (_, Rank::Five) => false,
        _ => is_critical(view, identity),
    };
    if !valid || !target_hand.iter().any(|card| card.id == focus) {
        return None;
    }
    // Save Principle, with urgency as a deterministic tie-break.
    Some(if target == next_player { 450 } else { 400 })
}

#[allow(clippy::too_many_arguments)]
fn play_clue_score(
    view: &PlayerView,
    target: PlayerId,
    focus: CardId,
    focus_identity: Card,
    clue: Clue,
    newly_touched: &[CardId],
    explicitly_clued: &HashSet<CardId>,
    already_playing: &HashSet<CardId>,
) -> Option<u16> {
    if !good_touch(view, newly_touched, explicitly_clued) {
        return None;
    }
    let height = view.play_stacks[focus_identity.suit.index()].len();
    let rank = usize::from(focus_identity.rank.number());
    if rank <= height {
        return None;
    }
    let base = if rank == height + 1 {
        330
    } else {
        delayed_connection_score(
            view,
            target,
            focus,
            focus_identity,
            explicitly_clued,
            already_playing,
        )?
    };
    Some(
        base + 2 * u16::try_from(newly_touched.len()).unwrap_or(0)
            + u16::from(matches!(clue, Clue::Suit(_))),
    )
}

fn good_touch(
    view: &PlayerView,
    newly_touched: &[CardId],
    explicitly_clued: &HashSet<CardId>,
) -> bool {
    let mut identities = HashSet::new();
    for card in newly_touched {
        let Some(identity) = identity_of(view, *card) else {
            return false;
        };
        if !is_eventually_useful(view, identity) || !identities.insert(identity) {
            return false;
        }
        if view.hands.iter().flatten().any(|candidate| {
            candidate.id != *card
                && explicitly_clued.contains(&candidate.id)
                && candidate.identity == Some(identity)
        }) {
            return false;
        }
    }
    true
}

fn is_eventually_useful(view: &PlayerView, identity: Card) -> bool {
    let stack_height = view.play_stacks[identity.suit.index()].len();
    if usize::from(identity.rank.number()) <= stack_height {
        return false;
    }
    Rank::ALL
        .iter()
        .copied()
        .filter(|rank| {
            usize::from(rank.number()) > stack_height && rank.number() < identity.rank.number()
        })
        .all(|rank| {
            let lower = Card::new(identity.suit, rank);
            view.discard_pile
                .iter()
                .filter(|(_, card)| *card == lower)
                .count()
                < usize::from(rank.copies())
        })
}

fn is_convention_trash(
    view: &PlayerView,
    identity: Card,
    gotten: &HashSet<CardId>,
    own_notes: &[HGroupCardInference],
) -> bool {
    if !is_eventually_useful(view, identity) {
        return true;
    }
    view.hands
        .iter()
        .flatten()
        .filter(|card| gotten.contains(&card.id))
        .filter(|card| {
            card.identity == Some(identity)
                || (card.identity.is_none()
                    && own_notes.iter().any(|note| {
                        note.card == card.id
                            && note.identities.len() == 1
                            && note.identities.contains(identity)
                    }))
        })
        .take(2)
        .count()
        >= 2
}

fn is_unique_visible(view: &PlayerView, excluded: CardId, identity: Card) -> bool {
    !view
        .hands
        .iter()
        .flatten()
        .any(|card| card.id != excluded && card.identity == Some(identity))
}

fn delayed_connection_score(
    view: &PlayerView,
    target: PlayerId,
    focus: CardId,
    focus_identity: Card,
    explicitly_clued: &HashSet<CardId>,
    already_playing: &HashSet<CardId>,
) -> Option<u16> {
    let stack_height = view.play_stacks[focus_identity.suit.index()].len();
    let connector = Card::new(focus_identity.suit, Rank::ALL[stack_height]);
    let next = (view.current_player.index() + 1) % view.hands.len();
    let connection_hand = &view.hands[next];
    let prompt_candidates = connection_hand
        .iter()
        .rev()
        .filter(|card| {
            card.id != focus
                && explicitly_clued.contains(&card.id)
                && !already_playing.contains(&card.id)
                && card.clues.allows(connector)
        })
        .collect::<Vec<_>>();
    let first_connection_valid = if prompt_candidates.is_empty() {
        if target.index() == next {
            false
        } else {
            connection_hand
                .iter()
                .rev()
                .find(|card| !explicitly_clued.contains(&card.id))
                .is_some_and(|card| card.identity == Some(connector))
        }
    } else {
        prompt_candidates
            .iter()
            .position(|card| card.identity == Some(connector))
            .is_some_and(|correct| {
                prompt_candidates[..correct].iter().all(|card| {
                    card.identity
                        .is_some_and(|identity| is_playable_now(view, identity))
                })
            })
    };
    if !first_connection_valid {
        return None;
    }

    for needed_rank in (stack_height + 2)..usize::from(focus_identity.rank.number()) {
        let needed = Card::new(focus_identity.suit, Rank::ALL[needed_rank - 1]);
        if !view
            .hands
            .iter()
            .flatten()
            .any(|card| explicitly_clued.contains(&card.id) && card.identity == Some(needed))
        {
            return None;
        }
    }
    Some(if prompt_candidates.is_empty() {
        370
    } else {
        380
    })
}

fn tempo_clue_candidates(
    view: &PlayerView,
    replay: &Replay,
    gotten: &HashSet<CardId>,
) -> Vec<ClueCandidate> {
    let mut candidates = Vec::new();
    for action in view.legal_actions() {
        let Action::Clue { target, clue } = action else {
            continue;
        };
        let hand = &view.hands[target.index()];
        let touched = hand
            .iter()
            .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
            .map(|card| card.id)
            .collect::<Vec<_>>();
        if touched.iter().any(|card| !gotten.contains(card)) {
            continue;
        }
        let target_chop = chop(&replay.hands[target.index()], gotten);
        let Some(focus) = focus(&replay.hands[target.index()], &touched, target_chop, gotten)
        else {
            continue;
        };
        let Some(identity) = hand
            .iter()
            .find(|card| card.id == focus)
            .and_then(|card| card.identity)
        else {
            continue;
        };
        if is_playable_now(view, identity) {
            candidates.push(ClueCandidate {
                action,
                score: 100 + u16::from(matches!(clue, Clue::Suit(_))),
                target,
                save: false,
                immediate_play: true,
            });
        }
    }
    candidates
}

fn has_out_of_order_prompt(view: &PlayerView, gotten: &HashSet<CardId>) -> bool {
    for action in view.legal_actions() {
        let Action::Clue { target, clue } = action else {
            continue;
        };
        let hand = &view.hands[target.index()];
        let touched = hand
            .iter()
            .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
            .map(|card| card.id)
            .collect::<Vec<_>>();
        let Some(focus) = hand
            .iter()
            .rev()
            .find(|card| touched.contains(&card.id))
            .map(|card| card.id)
        else {
            continue;
        };
        let Some(focus_identity) = identity_of(view, focus) else {
            continue;
        };
        let height = view.play_stacks[focus_identity.suit.index()].len();
        if usize::from(focus_identity.rank.number()) != height + 2 {
            continue;
        }
        let expected = Card::new(focus_identity.suit, Rank::ALL[height]);
        let next = (view.current_player.index() + 1) % view.hands.len();
        let prompt_candidates = view.hands[next]
            .iter()
            .rev()
            .filter(|card| gotten.contains(&card.id) && card.clues.allows(expected))
            .collect::<Vec<_>>();
        if let Some(correct) = prompt_candidates
            .iter()
            .position(|card| card.identity == Some(expected))
        {
            if prompt_candidates[..correct].iter().any(|card| {
                card.identity
                    .is_some_and(|identity| !is_playable_now(view, identity))
            }) {
                return true;
            }
        }
    }
    false
}

fn infer_clue_to_self(
    deductions: &LogicalDeductions,
    clue: &HGroupClueInterpretation,
    explicitly_clued: &HashSet<CardId>,
    inferred: &mut HGroupInferences,
) {
    match clue.kind {
        HGroupClueKind::Save(_) | HGroupClueKind::Unrecognized => return,
        HGroupClueKind::PlayOrSave if inferred.saved.contains(&clue.focus) => return,
        HGroupClueKind::Play | HGroupClueKind::PlayOrSave => {}
    }

    let Some(focus_possibilities) = deductions.possible_identities(clue.focus) else {
        return;
    };
    let view = deductions.view();
    let direct = identities_at_distance(focus_possibilities, view, 0);
    let delayed = identities_at_distance(focus_possibilities, view, 1);

    // A previously-clued connecting card takes precedence over interpreting
    // the focus as directly playable (the Level 1 Self-Prompt rule).
    if let Some(connection) = find_prompt(
        deductions,
        explicitly_clued,
        clue.focus,
        delayed,
        clue.focus,
    ) {
        inferred.connection = Some(connection);
    } else if !direct.is_empty() && !inferred.playable_now.contains(&clue.focus) {
        inferred.playable_now.push(clue.focus);
    }
}

fn find_prompt(
    deductions: &LogicalDeductions,
    explicitly_clued: &HashSet<CardId>,
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
        let possibilities = deductions.possible_identities(card.id)?;
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

fn identities_at_distance(identities: IdentitySet, view: &PlayerView, distance: u8) -> IdentitySet {
    let mask = identities
        .iter()
        .filter(|identity| {
            let height = u8::try_from(view.play_stacks[identity.suit.index()].len())
                .expect("a standard stack has at most five cards");
            identity.rank.number() == height + distance + 1
        })
        .fold(0, |mask, identity| mask | (1 << identity.index()));
    IdentitySet::from_mask(mask)
}

fn convention_card_inferences(
    deductions: &LogicalDeductions,
    replay: &Replay,
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
                    finessed: replay.invisibly_clued.contains(&card.id),
                })
        })
        .collect::<Vec<_>>();

    for clue in &replay.clues {
        if let Some(card) = cards.iter_mut().find(|card| card.card == clue.focus) {
            let clue_time = clue.play_identities.union(clue.save_identities);
            let live_play = IdentitySet::from_mask(
                clue.play_identities
                    .iter()
                    .filter(|identity| is_eventually_useful(view, *identity))
                    .fold(0, |mask, identity| mask | (1 << identity.index())),
            );
            let mut narrowed = card
                .identities
                .intersection(live_play.union(clue.save_identities));
            if narrowed.is_empty() {
                // Once every Play possibility becomes trash, retain the
                // clue-time note so the card is recognized as known trash.
                narrowed = card.identities.intersection(clue_time);
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
        for (non_focus, good_touch) in &clue.non_focus_identities {
            let convention_dupes = cards
                .iter()
                .filter(|other| other.card != *non_focus && other.identities.len() == 1)
                .fold(IdentitySet::default(), |duplicates, other| {
                    duplicates.union(other.identities)
                });
            if let Some(card) = cards.iter_mut().find(|card| card.card == *non_focus) {
                let narrowed = card
                    .identities
                    .intersection(good_touch.without(convention_dupes));
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
    for forced in &replay.forced_playable {
        let Some(card) = cards.iter_mut().find(|card| card.card == *forced) else {
            continue;
        };
        let playable = identities_at_distance(card.identities, view, 0);
        if !playable.is_empty() {
            card.identities = playable;
        }
        card.finessed = true;
    }
    cards
}

fn convention_playable(
    view: &PlayerView,
    gotten: &HashSet<CardId>,
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

fn two_save_allowed(
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

fn clue_kind_from_masks(clue: Clue, play: IdentitySet, save: IdentitySet) -> HGroupClueKind {
    match (play.is_empty(), save.is_empty()) {
        (false, false) => HGroupClueKind::PlayOrSave,
        (false, true) => HGroupClueKind::Play,
        (true, false) => match clue {
            Clue::Rank(Rank::Five) => HGroupClueKind::Save(HGroupSaveKind::Five),
            Clue::Rank(Rank::Two) => HGroupClueKind::Save(HGroupSaveKind::Two),
            _ => HGroupClueKind::Save(HGroupSaveKind::Critical),
        },
        (true, true) => HGroupClueKind::Unrecognized,
    }
}

fn is_playable_now(view: &PlayerView, identity: Card) -> bool {
    identity.rank.number()
        == u8::try_from(view.play_stacks[identity.suit.index()].len())
            .expect("a standard stack has at most five cards")
            + 1
}

#[allow(clippy::too_many_arguments)]
fn snapshot_play_identities(
    identities: IdentitySet,
    giver: PlayerId,
    target: PlayerId,
    focus: CardId,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    facts: &[ClueFacts],
    gotten: &HashSet<CardId>,
    already_playing: &HashSet<CardId>,
    stack_heights: [u8; 5],
) -> IdentitySet {
    let mask = identities
        .iter()
        .filter(|identity| {
            snapshot_playable(
                *identity,
                giver,
                target,
                focus,
                view,
                hands,
                facts,
                gotten,
                already_playing,
                stack_heights,
            )
        })
        .fold(0, |mask, identity| mask | (1 << identity.index()));
    IdentitySet::from_mask(mask)
}

#[allow(clippy::too_many_arguments)]
fn snapshot_playable(
    identity: Card,
    giver: PlayerId,
    target: PlayerId,
    focus: CardId,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    facts: &[ClueFacts],
    gotten: &HashSet<CardId>,
    already_playing: &HashSet<CardId>,
    stack_heights: [u8; 5],
) -> bool {
    let height = usize::from(stack_heights[identity.suit.index()]);
    let rank = usize::from(identity.rank.number());
    if rank <= height {
        return false;
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
    let first = Card::new(identity.suit, Rank::ALL[height]);
    if snapshot_accounted(first, focus, view, hands, facts, gotten) {
        return true;
    }

    let actor_index = (giver.index() + 1) % hands.len();
    let prompt_candidates = hands[actor_index]
        .iter()
        .rev()
        .copied()
        .filter(|card| {
            *card != focus
                && gotten.contains(card)
                && !already_playing.contains(card)
                && facts[card.index()].allows(first)
        })
        .collect::<Vec<_>>();
    if !prompt_candidates.is_empty() {
        if actor_index == view.observer.index() {
            return true;
        }
        return prompt_candidates
            .iter()
            .position(|card| identity_of(view, *card) == Some(first))
            .is_some_and(|correct| {
                prompt_candidates[..correct].iter().all(|card| {
                    identity_of(view, *card).is_some_and(|candidate| {
                        candidate.rank.number() == stack_heights[candidate.suit.index()] + 1
                    })
                })
            });
    }
    if target.index() == actor_index {
        return false;
    }
    let finesse = hands[actor_index]
        .iter()
        .rev()
        .copied()
        .find(|card| !gotten.contains(card));
    finesse.is_some_and(|card| {
        actor_index == view.observer.index() || identity_of(view, card) == Some(first)
    })
}

fn snapshot_accounted(
    identity: Card,
    excluded: CardId,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    facts: &[ClueFacts],
    gotten: &HashSet<CardId>,
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
fn snapshot_save_identities(
    identities: IdentitySet,
    clue: Clue,
    focus: CardId,
    focus_was_chop: bool,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    gotten: &HashSet<CardId>,
    play_identities: IdentitySet,
    discarded: [u8; 25],
) -> IdentitySet {
    if !focus_was_chop {
        return IdentitySet::default();
    }
    let chops = hands
        .iter()
        .map(|hand| chop(hand, gotten))
        .collect::<Vec<_>>();
    let mask = identities
        .iter()
        .filter(|identity| match clue {
            Clue::Rank(Rank::Five) => identity.rank == Rank::Five,
            Clue::Rank(Rank::Two) if identity.rank == Rank::Two => {
                !play_identities.contains(*identity)
                    && snapshot_two_save_allowed(view, hands, focus, *identity, &chops)
            }
            _ => {
                identity.rank != Rank::Five
                    && !play_identities.contains(*identity)
                    && discarded[identity.index()] + 1 == identity.rank.copies()
            }
        })
        .fold(0, |mask, identity| mask | (1 << identity.index()));
    IdentitySet::from_mask(mask)
}

fn snapshot_two_save_allowed(
    view: &PlayerView,
    hands: &[Vec<CardId>],
    focus: CardId,
    identity: Card,
    chops: &[Option<CardId>],
) -> bool {
    let visible = hands
        .iter()
        .flatten()
        .copied()
        .filter(|card| *card != focus && identity_of(view, *card) == Some(identity))
        .collect::<Vec<_>>();
    visible.is_empty() || visible.iter().all(|card| chops.contains(&Some(*card)))
}

#[allow(clippy::too_many_arguments)]
fn snapshot_good_touch_identities(
    card: CardId,
    identities: IdentitySet,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    gotten: &HashSet<CardId>,
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

#[allow(clippy::too_many_lines)]
fn replay_h_group(deductions: &LogicalDeductions, profile: HGroupProfile) -> Replay {
    let view = deductions.view();
    let hand_size = if view.hands.len() <= 3 { 5 } else { 4 };
    let mut hands = (0..view.hands.len())
        .map(|player| {
            let first = player * hand_size;
            (first..first + hand_size).map(CardId::new).collect()
        })
        .collect::<Vec<Vec<CardId>>>();
    let mut explicitly_clued = HashSet::new();
    let mut invisibly_clued = HashSet::new();
    let mut clues = Vec::new();
    let mut public_removed = [0_u8; 25];
    let mut facts = vec![ClueFacts::default(); 50];
    let mut stack_heights = [0_u8; 5];
    let mut pending_connections = Vec::<PendingConnection>::new();
    let mut already_playing = HashSet::<CardId>::new();
    let mut early_game = true;
    let mut signals = Vec::new();
    let mut chop_moved = HashSet::new();
    let mut discard_now = Vec::new();
    let mut must_clue = HashSet::new();
    let mut forced_playable = HashSet::new();

    for entry in &view.history {
        match &entry.event {
            ObservedEvent::Clued {
                giver,
                target,
                clue,
                touched,
                untouched,
            } => {
                let mut gotten = explicitly_clued
                    .union(&invisibly_clued)
                    .copied()
                    .collect::<HashSet<_>>();
                gotten.extend(chop_moved.iter().copied());
                let hand = &hands[target.index()];
                let old_chop = chop(hand, &gotten);
                let newly_touched = touched
                    .iter()
                    .copied()
                    .filter(|card| !gotten.contains(card))
                    .collect::<Vec<_>>();
                if let Some(focus) = focus(hand, touched, old_chop, &gotten) {
                    let focus_identity = identity_of(view, focus);
                    let focus_was_chop = old_chop == Some(focus);
                    for card in touched {
                        facts[card.index()].add_positive_clue(*clue);
                    }
                    for card in untouched {
                        facts[card.index()].add_negative_clue(*clue);
                    }
                    explicitly_clued.extend(touched.iter().copied());
                    let focus_identities = focus_identity.map_or_else(
                        || IdentitySet::from_mask(facts[focus.index()].identity_mask()),
                        IdentitySet::singleton,
                    );
                    let play_identities = snapshot_play_identities(
                        focus_identities,
                        *giver,
                        *target,
                        focus,
                        view,
                        &hands,
                        &facts,
                        &gotten,
                        &already_playing,
                        stack_heights,
                    );
                    let save_identities = snapshot_save_identities(
                        focus_identities,
                        *clue,
                        focus,
                        focus_was_chop,
                        view,
                        &hands,
                        &gotten,
                        play_identities,
                        public_removed,
                    );
                    let kind = clue_kind_from_masks(*clue, play_identities, save_identities);
                    let focus_identities = if focus_was_chop {
                        play_identities.union(save_identities)
                    } else {
                        play_identities
                    };
                    let new_non_focus = newly_touched
                        .iter()
                        .copied()
                        .filter(|card| *card != focus)
                        .collect::<Vec<_>>();
                    let non_focus_identities = new_non_focus
                        .iter()
                        .copied()
                        .map(|card| {
                            let direct = identity_of(view, card).map_or_else(
                                || IdentitySet::from_mask(facts[card.index()].identity_mask()),
                                IdentitySet::singleton,
                            );
                            (
                                card,
                                snapshot_good_touch_identities(
                                    card,
                                    direct,
                                    view,
                                    &hands,
                                    &gotten,
                                    stack_heights,
                                    public_removed,
                                ),
                            )
                        })
                        .collect();
                    clues.push(HGroupClueInterpretation {
                        turn: entry.turn,
                        giver: *giver,
                        target: *target,
                        clue: *clue,
                        focus,
                        focus_was_chop,
                        kind,
                        focus_identities,
                        play_identities,
                        save_identities,
                        new_non_focus,
                        non_focus_identities,
                        previously_gotten: gotten.iter().copied().collect(),
                    });
                    let signal_kind = match kind {
                        HGroupClueKind::Play | HGroupClueKind::PlayOrSave => {
                            Some(HGroupMoveKind::PlayClue)
                        }
                        HGroupClueKind::Save(_) => Some(HGroupMoveKind::SaveClue),
                        HGroupClueKind::Unrecognized => None,
                    };
                    if let Some(signal_kind) = signal_kind {
                        signals.push(HGroupSignal {
                            turn: entry.turn,
                            actor: *giver,
                            target: Some(*target),
                            kind: signal_kind,
                            cards: vec![focus],
                            identity: focus_identity,
                        });
                    }
                    if matches!(kind, HGroupClueKind::Play) {
                        let previous_pending = pending_connections.len();
                        schedule_connection(
                            profile,
                            view,
                            *giver,
                            *target,
                            focus,
                            focus_identity,
                            &hands,
                            &facts,
                            &explicitly_clued,
                            &already_playing,
                            &mut invisibly_clued,
                            stack_heights,
                            &mut pending_connections,
                        );
                        for connection in &pending_connections[previous_pending..] {
                            push_signal(
                                &mut signals,
                                entry,
                                *giver,
                                Some(connection.actor),
                                if connection.kind == HGroupConnectionKind::Finesse
                                    && connection.cards.len() > 1
                                {
                                    HGroupMoveKind::LayeredFinesse
                                } else {
                                    match connection.kind {
                                        HGroupConnectionKind::Prompt => HGroupMoveKind::Prompt,
                                        HGroupConnectionKind::Finesse => HGroupMoveKind::Finesse,
                                    }
                                },
                                connection.cards.clone(),
                                Some(connection.expected),
                            );
                        }
                        already_playing.insert(focus);
                    }
                } else {
                    for card in touched {
                        facts[card.index()].add_positive_clue(*clue);
                    }
                    for card in untouched {
                        facts[card.index()].add_negative_clue(*clue);
                    }
                    explicitly_clued.extend(touched.iter().copied());
                }
            }
            ObservedEvent::Played {
                player,
                card,
                identity,
                successful,
            } => {
                advance_pending_connections(
                    &mut pending_connections,
                    *player,
                    *card,
                    *identity,
                    *successful,
                );
                remove_card(&mut hands[player.index()], *card);
                invisibly_clued.remove(card);
                already_playing.remove(card);
                if *successful {
                    stack_heights[identity.suit.index()] = identity.rank.number();
                } else {
                    public_removed[identity.index()] += 1;
                }
                must_clue.remove(player);
            }
            ObservedEvent::Discarded {
                player,
                card,
                identity,
            } => {
                let mut gotten = explicitly_clued
                    .union(&invisibly_clued)
                    .copied()
                    .collect::<HashSet<_>>();
                gotten.extend(chop_moved.iter().copied());
                if chop(&hands[player.index()], &gotten) == Some(*card) {
                    early_game = false;
                }
                for pending in pending_connections
                    .iter_mut()
                    .filter(|pending| pending.actor == *player)
                {
                    if pending.cards.first() == Some(card) {
                        pending.cards.clear();
                    } else {
                        pending.cards.retain(|candidate| candidate != card);
                    }
                }
                pending_connections
                    .retain(|pending| !pending.cards.is_empty() && pending.focus != *card);
                remove_card(&mut hands[player.index()], *card);
                invisibly_clued.remove(card);
                already_playing.remove(card);
                public_removed[identity.index()] += 1;
                must_clue.remove(player);
            }
            ObservedEvent::Drew { player, card, .. } => hands[player.index()].push(*card),
        }

        if profile.includes(HGroupLevel::Level2) {
            apply_level_two_effects(entry, view, &hands, &explicitly_clued, &mut signals);
        }
        if profile.includes(HGroupLevel::Level3) {
            apply_level_three_effects(entry, view, &hands, &explicitly_clued, &mut signals);
        }
        if profile.includes(HGroupLevel::Level4)
            && (!profile.includes(HGroupLevel::Level8)
                || h_group_phase(view, early_game) != HGroupPhase::EndGame)
        {
            apply_chop_move_effects(
                entry,
                view,
                &hands,
                &explicitly_clued,
                &mut chop_moved,
                &mut signals,
            );
        }
        if profile.includes(HGroupLevel::Level6) {
            apply_tempo_effects(
                entry,
                view,
                &hands,
                &explicitly_clued,
                &mut chop_moved,
                &mut signals,
            );
        }
        if profile.includes(HGroupLevel::Level7) {
            apply_emergency_discard_effects(
                entry,
                view,
                &hands,
                &explicitly_clued,
                &mut chop_moved,
                &mut must_clue,
                &mut signals,
            );
        }
        if profile.includes(HGroupLevel::Level8) {
            apply_positional_effects(
                entry,
                view,
                &hands,
                &mut pending_connections,
                &mut forced_playable,
                &mut signals,
            );
        }
        if profile.includes(HGroupLevel::Level9) {
            apply_stall_effects(entry, view, &mut signals);
        }
        if profile.includes(HGroupLevel::Level10) {
            apply_transfer_effects(
                entry,
                view,
                &hands,
                &explicitly_clued,
                &mut invisibly_clued,
                &mut pending_connections,
                &mut signals,
            );
        }
        if profile.includes(HGroupLevel::Level11) {
            apply_bluff_effects(entry, view, &hands, &mut pending_connections, &mut signals);
        }
        if profile.includes(HGroupLevel::Level12) {
            apply_context_effects(entry, view, &hands, &explicitly_clued, &mut signals);
        }
        if profile.includes(HGroupLevel::Level13) {
            apply_intermediate_bluff_effects(entry, view, &hands, &mut signals);
        }
        if profile.includes(HGroupLevel::Level14) {
            apply_trash_effects(
                entry,
                view,
                &hands,
                &mut chop_moved,
                &mut pending_connections,
                &mut signals,
            );
        }
        if profile.includes(HGroupLevel::Level15) {
            apply_double_bluff_effects(entry, view, &hands, &mut signals);
        }
        if profile.includes(HGroupLevel::Level16) {
            apply_ejection_discharge_effects(
                entry,
                view,
                &hands,
                &mut pending_connections,
                &mut forced_playable,
                &mut signals,
            );
        }
        if profile.includes(HGroupLevel::Level17) {
            apply_duplication_effects(entry, view, &explicitly_clued, &mut signals);
        }
        if profile.includes(HGroupLevel::Level18) {
            apply_elimination_effects(entry, view, &hands, &mut signals);
        }
        if profile.includes(HGroupLevel::Level19) {
            apply_five_tech_effects(
                entry,
                view,
                &hands,
                &mut pending_connections,
                &mut forced_playable,
                &mut signals,
            );
        }
        if profile.includes(HGroupLevel::Level20) {
            apply_out_of_order_effects(
                entry,
                view,
                &hands,
                &mut pending_connections,
                &mut forced_playable,
                &mut signals,
            );
        }
        if profile.includes(HGroupLevel::Level21) {
            apply_ignition_effects(entry, view, &hands, &mut forced_playable, &mut signals);
        }
        if profile.includes(HGroupLevel::Level22) {
            apply_phantom_effects(entry, view, &hands, &mut forced_playable, &mut signals);
        }
        if profile.includes(HGroupLevel::Level23) {
            apply_charm_effects(entry, view, &hands, &mut signals);
        }
        if profile.includes(HGroupLevel::Level24) {
            apply_unnecessary_move_effects(entry, view, &hands, &mut signals);
        }
        if profile.includes(HGroupLevel::Level25) {
            apply_priority_effects(
                entry,
                view,
                &hands,
                &explicitly_clued,
                &mut forced_playable,
                &mut signals,
            );
        }
        if profile.is_max() {
            apply_extra_effects(
                entry,
                view,
                &hands,
                &explicitly_clued,
                &mut pending_connections,
                &mut chop_moved,
                &mut must_clue,
                &mut forced_playable,
                &mut signals,
            );
        }
    }
    if profile.includes(HGroupLevel::Level9) && view.clue_tokens == 0 {
        let gotten = explicitly_clued
            .iter()
            .chain(invisibly_clued.iter())
            .chain(chop_moved.iter())
            .copied()
            .collect::<HashSet<_>>();
        let own_hand = &hands[view.observer.index()];
        if !own_hand.is_empty() && own_hand.iter().all(|card| gotten.contains(card)) {
            if let Some(leftmost) = own_hand.last() {
                forced_playable.insert(*leftmost);
            }
        }
    }
    if profile.includes(HGroupLevel::Level10) {
        for pending in pending_connections.iter().filter(|pending| {
            pending.actor == view.observer
                && pending.kind == HGroupConnectionKind::Finesse
                && pending_is_active(pending, &pending_connections)
        }) {
            let Some(blind) = pending.cards.first() else {
                continue;
            };
            let duplicated_in_own_hand = view.hands[view.observer.index()].iter().any(|card| {
                card.id != *blind
                    && explicitly_clued.contains(&card.id)
                    && card.clues.allows(pending.expected)
            });
            let bluff = signals
                .iter()
                .any(|signal| signal.kind == HGroupMoveKind::Bluff && signal.cards.contains(blind));
            if duplicated_in_own_hand && !bluff {
                discard_now.push(*blind);
            }
        }
    }
    Replay {
        hands,
        explicitly_clued,
        invisibly_clued,
        clues,
        pending_connections,
        already_playing,
        early_game,
        signals,
        chop_moved,
        discard_now,
        must_clue,
        forced_playable,
    }
}

fn h_group_phase(view: &PlayerView, early_game: bool) -> HGroupPhase {
    let score = view.play_stacks.iter().map(Vec::len).sum::<usize>();
    let remaining_plays = 25_usize.saturating_sub(score);
    let remaining_turns = view.deck_size.saturating_add(view.hands.len());
    let pace = isize::try_from(remaining_turns).unwrap_or(isize::MAX)
        - isize::try_from(remaining_plays).unwrap_or(isize::MAX);
    if pace < isize::try_from(view.hands.len()).unwrap_or(isize::MAX) {
        HGroupPhase::EndGame
    } else if early_game {
        HGroupPhase::EarlyGame
    } else if score < 5 {
        HGroupPhase::LowScore
    } else {
        HGroupPhase::Normal
    }
}

fn push_signal(
    signals: &mut Vec<HGroupSignal>,
    entry: &ObservedHistoryEntry,
    actor: PlayerId,
    target: Option<PlayerId>,
    kind: HGroupMoveKind,
    cards: Vec<CardId>,
    identity: Option<Card>,
) {
    if signals.iter().any(|signal| {
        signal.turn == entry.turn
            && signal.actor == actor
            && signal.kind == kind
            && signal.cards == cards
    }) {
        return;
    }
    signals.push(HGroupSignal {
        turn: entry.turn,
        actor,
        target,
        kind,
        cards,
        identity,
    });
}

fn next_player(player: PlayerId, player_count: usize) -> PlayerId {
    PlayerId::new(
        u8::try_from((player.index() + 1) % player_count)
            .expect("standard Hanabi has at most five players"),
    )
}

fn was_clued_before(view: &PlayerView, turn: u32, card: CardId) -> bool {
    view.history.iter().take_while(|entry| entry.turn < turn).any(
        |entry| matches!(&entry.event, ObservedEvent::Clued { touched, .. } if touched.contains(&card)),
    )
}

fn current_card_identity(view: &PlayerView, card: CardId) -> Option<Card> {
    identity_of(view, card)
}

fn card_is_trash(view: &PlayerView, identity: Card) -> bool {
    usize::from(identity.rank.number()) <= view.play_stacks[identity.suit.index()].len()
}

fn visible_playable_in_hand(
    view: &PlayerView,
    player: PlayerId,
    excluded: Option<CardId>,
) -> Option<(CardId, Card)> {
    view.hands[player.index()].iter().rev().find_map(|card| {
        (Some(card.id) != excluded)
            .then_some(card.identity)
            .flatten()
            .filter(|identity| is_playable_now(view, *identity))
            .map(|identity| (card.id, identity))
    })
}

fn apply_level_two_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    explicitly_clued: &HashSet<CardId>,
    signals: &mut Vec<HGroupSignal>,
) {
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
    if *clue == Clue::Rank(Rank::Five)
        && touched.iter().any(|card| {
            current_card_identity(view, *card).is_some_and(|identity| identity.rank == Rank::Five)
        })
        && hands[target.index()]
            .first()
            .is_none_or(|chop| !touched.contains(chop))
    {
        push_signal(
            signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::FiveStall,
            touched.clone(),
            None,
        );
    }

    // A clue to one's own next connecting card is the Self-Finesse form. A
    // connection that has to pass the target before resolving is the Reverse
    // form. Multi-card Prompts/Finesses are represented by repeated primitive
    // connection signals in resolution order.
    let delayed_focus = touched.last().copied().and_then(|focus| {
        current_card_identity(view, focus)
            .filter(|identity| !is_playable_now(view, *identity))
            .map(|identity| (focus, identity))
    });
    if let Some((focus, identity)) = delayed_focus {
        let actor = next_player(*giver, hands.len());
        let kind = if actor == *target {
            HGroupMoveKind::SelfFinesse
        } else if target.index() < actor.index() {
            HGroupMoveKind::ReverseFinesse
        } else if explicitly_clued.contains(&focus) {
            HGroupMoveKind::Prompt
        } else {
            HGroupMoveKind::Finesse
        };
        push_signal(
            signals,
            entry,
            *giver,
            Some(*target),
            kind,
            vec![focus],
            Some(identity),
        );
    }
}

fn apply_level_three_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    _hands: &[Vec<CardId>],
    _explicitly_clued: &HashSet<CardId>,
    signals: &mut Vec<HGroupSignal>,
) {
    match &entry.event {
        ObservedEvent::Clued {
            giver,
            target,
            touched,
            ..
        } if !touched.is_empty()
            && touched
                .iter()
                .all(|card| was_clued_before(view, entry.turn, *card)) =>
        {
            push_signal(
                signals,
                entry,
                *giver,
                Some(*target),
                HGroupMoveKind::FixClue,
                touched.clone(),
                None,
            );
        }
        ObservedEvent::Discarded {
            player,
            card,
            identity,
        } if was_clued_before(view, entry.turn, *card)
            && view.hands.iter().flatten().any(|candidate| {
                candidate.id != *card && candidate.identity == Some(*identity)
            }) =>
        {
            push_signal(
                signals,
                entry,
                *player,
                None,
                HGroupMoveKind::SarcasticDiscard,
                vec![*card],
                Some(*identity),
            );
        }
        _ => {}
    }
}

fn apply_chop_move_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    explicitly_clued: &HashSet<CardId>,
    chop_moved: &mut HashSet<CardId>,
    signals: &mut Vec<HGroupSignal>,
) {
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
    let hand = &hands[target.index()];
    let all_trash = !touched.is_empty()
        && touched.iter().all(|card| {
            current_card_identity(view, *card).is_some_and(|identity| card_is_trash(view, identity))
        });
    let five_chop_move = *clue == Clue::Rank(Rank::Five)
        && touched.iter().any(|card| {
            current_card_identity(view, *card).is_some_and(|identity| identity.rank == Rank::Five)
        });
    if !all_trash && !five_chop_move {
        return;
    }
    let boundary = touched
        .iter()
        .filter_map(|card| hand.iter().position(|candidate| candidate == card))
        .min();
    let Some(boundary) = boundary else {
        return;
    };
    let count = if five_chop_move { 1 } else { boundary };
    let moved = hand[..boundary]
        .iter()
        .rev()
        .filter(|card| !explicitly_clued.contains(card) && !chop_moved.contains(card))
        .take(count.max(1))
        .copied()
        .collect::<Vec<_>>();
    if moved.is_empty() {
        return;
    }
    chop_moved.extend(moved.iter().copied());
    push_signal(
        signals,
        entry,
        *giver,
        Some(*target),
        HGroupMoveKind::ChopMove,
        moved,
        None,
    );
}

fn apply_tempo_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    explicitly_clued: &HashSet<CardId>,
    chop_moved: &mut HashSet<CardId>,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Clued {
        giver,
        target,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    if touched.is_empty()
        || !touched
            .iter()
            .all(|card| was_clued_before(view, entry.turn, *card) || chop_moved.contains(card))
    {
        return;
    }
    push_signal(
        signals,
        entry,
        *giver,
        Some(*target),
        HGroupMoveKind::TempoClue,
        touched.clone(),
        None,
    );
    let playable_count = touched
        .iter()
        .filter(|card| {
            current_card_identity(view, **card)
                .is_some_and(|identity| is_playable_now(view, identity))
        })
        .count();
    if playable_count < 2 {
        if let Some(card) = chop(&hands[target.index()], explicitly_clued) {
            chop_moved.insert(card);
            push_signal(
                signals,
                entry,
                *giver,
                Some(*target),
                HGroupMoveKind::ChopMove,
                vec![card],
                None,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_emergency_discard_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    explicitly_clued: &HashSet<CardId>,
    chop_moved: &mut HashSet<CardId>,
    must_clue: &mut HashSet<PlayerId>,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Discarded { player, card, .. } = &entry.event else {
        return;
    };
    let known_playable = view.hands[player.index()].iter().any(|candidate| {
        candidate
            .identity
            .is_some_and(|identity| is_playable_now(view, identity))
            && (explicitly_clued.contains(&candidate.id) || chop_moved.contains(&candidate.id))
    });
    let known_trash = was_clued_before(view, entry.turn, *card)
        && current_card_identity(view, *card).is_some_and(|identity| card_is_trash(view, identity));
    if !known_playable && !known_trash {
        return;
    }
    let target = next_player(*player, hands.len());
    if let Some(target_chop) = chop(&hands[target.index()], explicitly_clued) {
        chop_moved.insert(target_chop);
        must_clue.insert(target);
        push_signal(
            signals,
            entry,
            *player,
            Some(target),
            HGroupMoveKind::EmergencyDiscard,
            vec![target_chop],
            current_card_identity(view, target_chop),
        );
    }
}

fn apply_positional_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    pending: &mut Vec<PendingConnection>,
    forced_playable: &mut HashSet<CardId>,
    signals: &mut Vec<HGroupSignal>,
) {
    let (player, card, is_misplay) = match &entry.event {
        ObservedEvent::Discarded { player, card, .. } => (*player, *card, false),
        ObservedEvent::Played {
            player,
            card,
            successful: false,
            ..
        } => (*player, *card, true),
        _ => return,
    };
    if view.deck_size > view.hands.len() || was_clued_before(view, entry.turn, card) {
        return;
    }
    let indicated_slot = hands[player.index()]
        .iter()
        .filter(|candidate| candidate.index() < card.index())
        .count();
    let target_and_card = (1..hands.len())
        .filter_map(|distance| {
            let index = (player.index() + distance) % hands.len();
            let target = PlayerId::new(u8::try_from(index).ok()?);
            let card = hands[index].get(indicated_slot).copied()?;
            let playable = target == view.observer
                || current_card_identity(view, card)
                    .is_some_and(|identity| is_playable_now(view, identity));
            playable.then_some((target, card))
        })
        .next_back();
    let Some((target, indicated)) = target_and_card else {
        return;
    };
    forced_playable.insert(indicated);
    if let Some(identity) = current_card_identity(view, indicated) {
        pending.push(PendingConnection {
            actor: target,
            cards: vec![indicated],
            expected: identity,
            kind: HGroupConnectionKind::Finesse,
            focus: indicated,
            step: 0,
        });
    }
    push_signal(
        signals,
        entry,
        player,
        Some(target),
        HGroupMoveKind::PositionalDiscard,
        vec![indicated],
        current_card_identity(view, indicated),
    );
    if is_misplay {
        // A positional misplay carries the same play indication; strike count
        // is already represented by the authoritative game state.
    }
}

fn apply_stall_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Clued {
        giver,
        target,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    if touched.is_empty()
        || !touched
            .iter()
            .all(|card| was_clued_before(view, entry.turn, *card))
    {
        return;
    }
    push_signal(
        signals,
        entry,
        *giver,
        Some(*target),
        HGroupMoveKind::Stall,
        touched.clone(),
        None,
    );
}

fn apply_context_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    explicitly_clued: &HashSet<CardId>,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Clued {
        giver,
        target,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    let Some(focus) = touched.last().copied() else {
        return;
    };
    let Some(identity) = current_card_identity(view, focus) else {
        return;
    };
    let height = view.play_stacks[identity.suit.index()].len();
    if usize::from(identity.rank.number()) <= height + 1 {
        return;
    }
    let connector = Card::new(identity.suit, Rank::ALL[height]);
    let selfish = hands[giver.index()].iter().any(|card| {
        explicitly_clued.contains(card) && current_card_identity(view, *card) == Some(connector)
    });
    push_signal(
        signals,
        entry,
        *giver,
        Some(*target),
        if selfish {
            HGroupMoveKind::SelfishClue
        } else {
            HGroupMoveKind::Context
        },
        vec![focus],
        Some(identity),
    );
}

fn apply_intermediate_bluff_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    signals: &mut Vec<HGroupSignal>,
) {
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
    let actor = next_player(*giver, hands.len());
    let specialized = *clue == Clue::Rank(Rank::Three)
        || touched.iter().any(|card| {
            current_card_identity(view, *card).is_some_and(|identity| identity.rank.number() >= 3)
        });
    if specialized && visible_playable_in_hand(view, actor, None).is_some() {
        push_signal(
            signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::Bluff,
            touched.clone(),
            None,
        );
    }
}

fn apply_double_bluff_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Clued {
        giver,
        target,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    let Some(identity) = touched
        .last()
        .and_then(|card| current_card_identity(view, *card))
    else {
        return;
    };
    let distance = usize::from(identity.rank.number())
        .saturating_sub(view.play_stacks[identity.suit.index()].len() + 1);
    if distance < 2 {
        return;
    }
    let first = next_player(*giver, hands.len());
    let second = next_player(first, hands.len());
    if visible_playable_in_hand(view, first, None).is_some()
        && visible_playable_in_hand(view, second, None).is_some()
    {
        push_signal(
            signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::DoubleBluff,
            touched.clone(),
            Some(identity),
        );
    }
}

fn apply_duplication_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    explicitly_clued: &HashSet<CardId>,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Clued {
        giver,
        target,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    let duplicated = touched.iter().find_map(|card| {
        let identity = current_card_identity(view, *card)?;
        view.hands
            .iter()
            .flatten()
            .any(|other| {
                other.id != *card
                    && explicitly_clued.contains(&other.id)
                    && other.identity == Some(identity)
            })
            .then_some((*card, identity))
    });
    if let Some((card, identity)) = duplicated {
        push_signal(
            signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::Duplication,
            vec![card],
            Some(identity),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_transfer_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    explicitly_clued: &HashSet<CardId>,
    invisibly_clued: &mut HashSet<CardId>,
    pending: &mut Vec<PendingConnection>,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Discarded {
        player,
        card,
        identity,
    } = &entry.event
    else {
        return;
    };
    if !was_clued_before(view, entry.turn, *card) {
        return;
    }
    let mut transfer = None;
    let mut kind = HGroupMoveKind::TransferDiscard;
    for distance in 1..hands.len() {
        let index = (player.index() + distance) % hands.len();
        if let Some(target_card) = hands[index].iter().rev().copied().find(|candidate| {
            explicitly_clued.contains(candidate)
                && current_card_identity(view, *candidate) == Some(*identity)
        }) {
            transfer = Some((PlayerId::new(u8::try_from(index).unwrap_or(0)), target_card));
            kind = HGroupMoveKind::SarcasticDiscard;
            break;
        }
    }
    if transfer.is_none() {
        for distance in 1..hands.len() {
            let index = (player.index() + distance) % hands.len();
            if let Some(target_card) = hands[index].iter().rev().copied().find(|candidate| {
                !explicitly_clued.contains(candidate)
                    && !invisibly_clued.contains(candidate)
                    && current_card_identity(view, *candidate) == Some(*identity)
            }) {
                transfer = Some((PlayerId::new(u8::try_from(index).unwrap_or(0)), target_card));
                break;
            }
        }
    }
    let Some((target, target_card)) = transfer else {
        return;
    };
    invisibly_clued.insert(target_card);
    if is_playable_now(view, *identity) {
        pending.push(PendingConnection {
            actor: target,
            cards: vec![target_card],
            expected: *identity,
            kind: HGroupConnectionKind::Finesse,
            focus: target_card,
            step: 0,
        });
    }
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

fn apply_bluff_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    pending: &mut Vec<PendingConnection>,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Clued {
        giver,
        target,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    let Some(focus) = touched.last().copied() else {
        return;
    };
    let Some(focus_identity) = current_card_identity(view, focus) else {
        return;
    };
    if is_playable_now(view, focus_identity) {
        return;
    }
    let actor = next_player(*giver, hands.len());
    if actor == *target {
        return;
    }
    let Some((bluff_card, bluff_identity)) = visible_playable_in_hand(view, actor, Some(focus))
    else {
        return;
    };
    let stack_height = view.play_stacks[focus_identity.suit.index()].len();
    if stack_height == Rank::ALL.len() {
        return;
    }
    let expected_connector = Card::new(focus_identity.suit, Rank::ALL[stack_height]);
    if bluff_identity == expected_connector {
        return;
    }
    pending.push(PendingConnection {
        actor,
        cards: vec![bluff_card],
        expected: bluff_identity,
        kind: HGroupConnectionKind::Finesse,
        focus,
        step: 0,
    });
    push_signal(
        signals,
        entry,
        *giver,
        Some(actor),
        HGroupMoveKind::Bluff,
        vec![bluff_card, focus],
        Some(bluff_identity),
    );
}

fn apply_trash_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    chop_moved: &mut HashSet<CardId>,
    pending: &mut Vec<PendingConnection>,
    signals: &mut Vec<HGroupSignal>,
) {
    match &entry.event {
        ObservedEvent::Clued {
            giver,
            target,
            touched,
            ..
        } if !touched.is_empty()
            && touched.iter().all(|card| {
                current_card_identity(view, *card)
                    .is_some_and(|identity| card_is_trash(view, identity))
            }) =>
        {
            let hand = &hands[target.index()];
            let focus = touched
                .iter()
                .filter_map(|card| {
                    hand.iter()
                        .position(|candidate| candidate == card)
                        .map(|p| (p, *card))
                })
                .max_by_key(|(position, _)| *position)
                .map(|(_, card)| card);
            if let Some(focus) = focus {
                push_signal(
                    signals,
                    entry,
                    *giver,
                    Some(*target),
                    HGroupMoveKind::TrashPush,
                    vec![focus],
                    current_card_identity(view, focus),
                );
                if let Some(position) = hand.iter().position(|card| *card == focus) {
                    if let Some(pushed) = hand.get(position + 1).copied() {
                        chop_moved.insert(pushed);
                    }
                }
            }
        }
        ObservedEvent::Discarded {
            player,
            card,
            identity,
        } if card_is_trash(view, *identity) && !was_clued_before(view, entry.turn, *card) => {
            let target = next_player(*player, hands.len());
            let playable_finesse = hands[target.index()].last().copied().and_then(|finesse| {
                current_card_identity(view, finesse)
                    .filter(|expected| is_playable_now(view, *expected))
                    .map(|expected| (finesse, expected))
            });
            if let Some((finesse, expected)) = playable_finesse {
                pending.push(PendingConnection {
                    actor: target,
                    cards: vec![finesse],
                    expected,
                    kind: HGroupConnectionKind::Finesse,
                    focus: finesse,
                    step: 0,
                });
                push_signal(
                    signals,
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

fn apply_ejection_discharge_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    pending: &mut Vec<PendingConnection>,
    forced_playable: &mut HashSet<CardId>,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Clued {
        giver,
        target: _,
        clue,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    let five_ejection = matches!(clue, Clue::Suit(_))
        && touched.iter().any(|card| {
            current_card_identity(view, *card).is_some_and(|identity| {
                identity.rank == Rank::Five
                    && 5_usize.saturating_sub(view.play_stacks[identity.suit.index()].len()) >= 2
            })
        });
    let unknown_discharge = touched.len() >= 2
        && touched.iter().any(|card| {
            current_card_identity(view, *card).is_some_and(|identity| card_is_trash(view, identity))
        });
    let (kind, position) = if five_ejection {
        (Some(HGroupMoveKind::Ejection), 1)
    } else if unknown_discharge {
        (Some(HGroupMoveKind::Discharge), 2)
    } else {
        (None, 0)
    };
    if let Some(kind) = kind {
        let actor = next_player(*giver, hands.len());
        if let Some(card) = hands[actor.index()].iter().rev().nth(position).copied() {
            pending.retain(|connection| connection.actor != actor);
            forced_playable.insert(card);
        }
        push_signal(
            signals,
            entry,
            *giver,
            Some(actor),
            kind,
            touched.clone(),
            None,
        );
    }
}

fn apply_elimination_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    _hands: &[Vec<CardId>],
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Clued {
        giver,
        target,
        touched,
        untouched,
        ..
    } = &entry.event
    else {
        return;
    };
    let singled_out = touched.len() == 1 || untouched.len() == 1;
    if singled_out
        && touched
            .iter()
            .all(|card| was_clued_before(view, entry.turn, *card))
    {
        let cards = if touched.len() == 1 {
            touched.clone()
        } else {
            untouched.clone()
        };
        push_signal(
            signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::Elimination,
            cards,
            None,
        );
    }
}

fn apply_five_tech_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    pending: &mut Vec<PendingConnection>,
    forced_playable: &mut HashSet<CardId>,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Clued {
        giver,
        target,
        clue: Clue::Rank(Rank::Five),
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    if touched.is_empty() {
        return;
    }
    let actor = next_player(*giver, hands.len());
    let focus_is_new = touched
        .iter()
        .any(|card| !was_clued_before(view, entry.turn, *card));
    if focus_is_new {
        if let Some(ejected) = hands[actor.index()].iter().rev().nth(1).copied() {
            pending.retain(|connection| connection.actor != actor);
            forced_playable.insert(ejected);
        }
    }
    push_signal(
        signals,
        entry,
        *giver,
        Some(*target),
        HGroupMoveKind::FivePull,
        touched.clone(),
        touched
            .iter()
            .find_map(|card| current_card_identity(view, *card)),
    );
}

fn apply_out_of_order_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    pending: &mut Vec<PendingConnection>,
    forced_playable: &mut HashSet<CardId>,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Clued {
        giver,
        target,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    if let Some(card) = touched.iter().find(|card| {
        current_card_identity(view, **card).is_some_and(|identity| {
            usize::from(identity.rank.number()) > view.play_stacks[identity.suit.index()].len() + 1
        })
    }) {
        let focus = *card;
        let focus_identity = current_card_identity(view, focus);
        let actor = next_player(*giver, hands.len());
        let connector = focus_identity.and_then(|identity| {
            let height = view.play_stacks[identity.suit.index()].len();
            Rank::ALL
                .get(height)
                .copied()
                .map(|rank| Card::new(identity.suit, rank))
        });
        if let Some(out_of_order) = connector.and_then(|connector| {
            hands[actor.index()]
                .iter()
                .rev()
                .skip(1)
                .copied()
                .find(|candidate| current_card_identity(view, *candidate) == Some(connector))
        }) {
            pending.retain(|connection| connection.focus != focus);
            forced_playable.insert(out_of_order);
        }
        push_signal(
            signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::OccupiedPlay,
            vec![focus],
            focus_identity,
        );
    }
}

fn apply_ignition_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    _hands: &[Vec<CardId>],
    forced_playable: &mut HashSet<CardId>,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Clued {
        giver,
        target,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    let playable = touched
        .iter()
        .filter(|card| {
            current_card_identity(view, **card)
                .is_some_and(|identity| is_playable_now(view, identity))
        })
        .count();
    if playable >= 2 {
        forced_playable.extend(touched.iter().copied());
        push_signal(
            signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::Ignition,
            touched.clone(),
            None,
        );
    }
}

fn apply_phantom_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    forced_playable: &mut HashSet<CardId>,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Discarded { player, card, .. } = &entry.event else {
        return;
    };
    let target = next_player(*player, hands.len());
    if visible_playable_in_hand(view, target, None).is_none() {
        return;
    }
    if let Some(card) = hands[target.index()].last() {
        forced_playable.insert(*card);
    }
    push_signal(
        signals,
        entry,
        *player,
        Some(target),
        HGroupMoveKind::PhantomPlayable,
        vec![*card],
        None,
    );
}

fn apply_charm_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    _hands: &[Vec<CardId>],
    signals: &mut Vec<HGroupSignal>,
) {
    match &entry.event {
        ObservedEvent::Clued {
            giver,
            target,
            clue: Clue::Rank(Rank::Four),
            touched,
            ..
        } => push_signal(
            signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::Charm,
            touched.clone(),
            None,
        ),
        ObservedEvent::Discarded {
            player,
            card,
            identity,
        } if was_clued_before(view, entry.turn, *card) && !card_is_trash(view, *identity) => {
            push_signal(
                signals,
                entry,
                *player,
                None,
                HGroupMoveKind::Charm,
                vec![*card],
                Some(*identity),
            );
        }
        _ => {}
    }
}

fn apply_unnecessary_move_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    _hands: &[Vec<CardId>],
    signals: &mut Vec<HGroupSignal>,
) {
    let (actor, cards, unnecessary) = match &entry.event {
        ObservedEvent::Clued { giver, touched, .. } => (
            *giver,
            touched.clone(),
            !touched.is_empty()
                && touched.iter().all(|card| {
                    current_card_identity(view, *card)
                        .is_some_and(|identity| card_is_trash(view, identity))
                }),
        ),
        ObservedEvent::Discarded {
            player,
            card,
            identity,
        } => (*player, vec![*card], !card_is_trash(view, *identity)),
        _ => return,
    };
    if unnecessary {
        push_signal(
            signals,
            entry,
            actor,
            None,
            HGroupMoveKind::UnnecessaryMove,
            cards,
            None,
        );
    }
}

fn apply_priority_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    explicitly_clued: &HashSet<CardId>,
    forced_playable: &mut HashSet<CardId>,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Played {
        player,
        card,
        identity,
        successful: true,
        ..
    } = &entry.event
    else {
        return;
    };
    if visible_playable_in_hand(view, *player, Some(*card)).is_some() {
        let target = next_player(*player, hands.len());
        if identity.rank != Rank::Five {
            let connector = Card::new(identity.suit, Rank::ALL[identity.rank.index() + 1]);
            let prompt = hands[target.index()]
                .iter()
                .rev()
                .copied()
                .find(|candidate| {
                    explicitly_clued.contains(candidate)
                        && current_card_identity(view, *candidate) == Some(connector)
                });
            let finesse = hands[target.index()]
                .iter()
                .rev()
                .copied()
                .find(|candidate| {
                    !explicitly_clued.contains(candidate)
                        && (target == view.observer
                            || current_card_identity(view, *candidate) == Some(connector))
                });
            if let Some(connection) = prompt.or(finesse) {
                forced_playable.insert(connection);
            }
        }
        push_signal(
            signals,
            entry,
            *player,
            None,
            HGroupMoveKind::Priority,
            vec![*card],
            current_card_identity(view, *card),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_extra_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    explicitly_clued: &HashSet<CardId>,
    pending: &mut Vec<PendingConnection>,
    chop_moved: &mut HashSet<CardId>,
    must_clue: &mut HashSet<PlayerId>,
    forced_playable: &mut HashSet<CardId>,
    signals: &mut Vec<HGroupSignal>,
) {
    match &entry.event {
        ObservedEvent::Clued {
            target, touched, ..
        } => {
            let pending_cards = pending
                .iter()
                .filter(|connection| connection.actor == *target)
                .flat_map(|connection| connection.cards.iter().copied())
                .collect::<HashSet<_>>();
            let continuation = touched.iter().any(|card| pending_cards.contains(card))
                && touched
                    .iter()
                    .any(|card| !was_clued_before(view, entry.turn, *card));
            if !continuation {
                let invalidated = pending
                    .iter()
                    .filter(|connection| connection.actor == *target)
                    .filter(|connection| {
                        connection.cards.iter().any(|pending_card| {
                            view.hands[target.index()]
                                .iter()
                                .find(|card| card.id == *pending_card)
                                .is_some_and(|card| !card.clues.allows(connection.expected))
                        })
                    })
                    .map(|connection| connection.focus)
                    .collect::<HashSet<_>>();
                pending.retain(|connection| !invalidated.contains(&connection.focus));
            }
        }
        ObservedEvent::Played {
            player,
            card,
            successful,
            ..
        } => {
            if *successful && !was_clued_before(view, entry.turn, *card) {
                forced_playable.remove(card);
            } else if !successful {
                let target = next_player(*player, hands.len());
                let gotten = explicitly_clued
                    .iter()
                    .chain(chop_moved.iter())
                    .copied()
                    .collect::<HashSet<_>>();
                if let Some(card) = chop(&hands[target.index()], &gotten) {
                    chop_moved.insert(card);
                    must_clue.insert(target);
                }
            }
        }
        ObservedEvent::Discarded { .. } | ObservedEvent::Drew { .. } => {}
    }
    // Extras refine or compose the numbered primitives. Preserve an explicit
    // marker when a single turn already matched two or more primitive effects;
    // consumers can inspect the preceding same-turn signals to recover the
    // exact composition without a parallel hierarchy of bespoke state types.
    let same_turn = signals
        .iter()
        .filter(|signal| signal.turn == entry.turn && signal.kind != HGroupMoveKind::Extra)
        .count();
    if same_turn >= 2 {
        let actor = match &entry.event {
            ObservedEvent::Clued { giver, .. } => *giver,
            ObservedEvent::Played { player, .. }
            | ObservedEvent::Discarded { player, .. }
            | ObservedEvent::Drew { player, .. } => *player,
        };
        let cards = match &entry.event {
            ObservedEvent::Clued { touched, .. } => touched.clone(),
            ObservedEvent::Played { card, .. }
            | ObservedEvent::Discarded { card, .. }
            | ObservedEvent::Drew { card, .. } => vec![*card],
        };
        push_signal(
            signals,
            entry,
            actor,
            None,
            HGroupMoveKind::Extra,
            cards,
            None,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn schedule_connection(
    profile: HGroupProfile,
    view: &PlayerView,
    giver: PlayerId,
    target: PlayerId,
    focus: CardId,
    focus_identity: Option<Card>,
    hands: &[Vec<CardId>],
    facts: &[ClueFacts],
    explicitly_clued: &HashSet<CardId>,
    already_playing: &HashSet<CardId>,
    invisibly_clued: &mut HashSet<CardId>,
    stack_heights: [u8; 5],
    pending: &mut Vec<PendingConnection>,
) {
    let Some(focus_identity) = focus_identity else {
        return;
    };
    let height = stack_heights[focus_identity.suit.index()];
    if focus_identity.rank.number() <= height + 1 {
        return;
    }
    let connection_count = if profile.includes(HGroupLevel::Level2) {
        focus_identity.rank.number().saturating_sub(height + 1)
    } else {
        1
    };
    let mut actor_index = (giver.index() + 1) % hands.len();
    for offset in 0..connection_count {
        let expected_rank = usize::from(height + offset);
        let expected = Card::new(focus_identity.suit, Rank::ALL[expected_rank]);
        let mut found = None;
        let search_len = if profile.includes(HGroupLevel::Level2) {
            hands.len()
        } else {
            1
        };
        for distance in 0..search_len {
            let candidate_index = (actor_index + distance) % hands.len();
            let actor = PlayerId::new(
                u8::try_from(candidate_index).expect("standard Hanabi has at most five players"),
            );
            let prompt_cards = hands[candidate_index]
                .iter()
                .rev()
                .copied()
                .filter(|card| {
                    *card != focus
                        && explicitly_clued.contains(card)
                        && !already_playing.contains(card)
                        && facts[card.index()].allows(expected)
                })
                .collect::<Vec<_>>();
            if !prompt_cards.is_empty() {
                found = Some((actor, prompt_cards, HGroupConnectionKind::Prompt));
                actor_index = candidate_index;
                break;
            }
            if target == actor {
                continue;
            }
            let gotten = explicitly_clued
                .union(invisibly_clued)
                .copied()
                .collect::<HashSet<_>>();
            let unclued = hands[candidate_index]
                .iter()
                .rev()
                .copied()
                .filter(|card| !gotten.contains(card) && *card != focus)
                .collect::<Vec<_>>();
            let cards = if profile.includes(HGroupLevel::Level5) {
                if actor == view.observer {
                    unclued
                } else {
                    unclued
                        .iter()
                        .position(|card| identity_of(view, *card) == Some(expected))
                        .map_or_else(Vec::new, |position| unclued[..=position].to_vec())
                }
            } else {
                unclued.first().copied().into_iter().collect()
            };
            if !cards.is_empty() {
                found = Some((actor, cards, HGroupConnectionKind::Finesse));
                actor_index = candidate_index;
                break;
            }
        }
        let Some((actor, cards, kind)) = found else {
            break;
        };
        if kind == HGroupConnectionKind::Finesse {
            invisibly_clued.extend(cards.iter().copied());
        }
        pending.push(PendingConnection {
            actor,
            cards,
            expected,
            kind,
            focus,
            step: offset,
        });
        actor_index = (actor_index + 1) % hands.len();
    }
}

fn pending_is_active(candidate: &PendingConnection, pending: &[PendingConnection]) -> bool {
    !pending.iter().any(|other| {
        other.focus == candidate.focus && other.step < candidate.step && !other.cards.is_empty()
    })
}

fn advance_pending_connections(
    pending: &mut Vec<PendingConnection>,
    player: PlayerId,
    card: CardId,
    identity: Card,
    successful: bool,
) {
    let active = pending
        .iter()
        .enumerate()
        .filter(|(_, item)| item.actor == player && pending_is_active(item, pending))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    for index in active {
        let connection = &mut pending[index];
        if connection.cards.first() != Some(&card) {
            connection.cards.retain(|candidate| *candidate != card);
            continue;
        }
        connection.cards.remove(0);
        if identity == connection.expected || !successful {
            connection.cards.clear();
        }
    }
    pending.retain(|connection| !connection.cards.is_empty() && connection.focus != card);
}

fn chop(hand: &[CardId], explicitly_clued: &HashSet<CardId>) -> Option<CardId> {
    hand.iter()
        .copied()
        .find(|card| !explicitly_clued.contains(card))
}

fn focus(
    hand: &[CardId],
    touched: &[CardId],
    chop: Option<CardId>,
    explicitly_clued: &HashSet<CardId>,
) -> Option<CardId> {
    let newly_touched = touched
        .iter()
        .copied()
        .filter(|card| !explicitly_clued.contains(card))
        .collect::<Vec<_>>();
    match newly_touched.as_slice() {
        [] => hand
            .iter()
            .rev()
            .copied()
            .find(|card| touched.contains(card)),
        [only] => Some(*only),
        _ if chop.is_some_and(|card| touched.contains(&card)) => chop,
        _ => hand
            .iter()
            .rev()
            .copied()
            .find(|card| newly_touched.contains(card)),
    }
}

fn identity_of(view: &PlayerView, card: CardId) -> Option<Card> {
    view.hands
        .iter()
        .flatten()
        .find(|candidate| candidate.id == card)
        .and_then(|candidate| candidate.identity)
        .or_else(|| {
            view.play_stacks
                .iter()
                .flatten()
                .chain(view.discard_pile.iter())
                .find_map(|(candidate, identity)| (*candidate == card).then_some(*identity))
        })
        .or_else(|| {
            view.history.iter().find_map(|entry| match entry.event {
                ObservedEvent::Played {
                    card: candidate,
                    identity,
                    ..
                }
                | ObservedEvent::Discarded {
                    card: candidate,
                    identity,
                    ..
                } if candidate == card => Some(identity),
                ObservedEvent::Drew {
                    card: candidate,
                    identity: Some(identity),
                    ..
                } if candidate == card => Some(identity),
                _ => None,
            })
        })
}

fn is_critical(view: &PlayerView, identity: Card) -> bool {
    identity.rank != Rank::Five
        && view
            .discard_pile
            .iter()
            .filter(|(_, discarded)| *discarded == identity)
            .count()
            + 1
            == usize::from(identity.rank.copies())
}

fn remove_card(hand: &mut Vec<CardId>, card: CardId) {
    if let Some(position) = hand.iter().position(|candidate| *candidate == card) {
        hand.remove(position);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hanabi_core::{
        Action, FullState, GameStatus, ObservedCard, ObservedHistoryEntry, PlayerView, Suit,
        standard_deck,
    };
    use rand::{SeedableRng, rngs::StdRng};

    fn state_with_prefix(num_players: u8, prefix: &[Card]) -> FullState {
        let mut deck = standard_deck();
        for (slot, wanted) in prefix.iter().copied().enumerate() {
            let found = deck[slot..]
                .iter()
                .position(|card| *card == wanted)
                .map(|offset| slot + offset)
                .expect("standard deck contains requested prefix");
            deck.swap(slot, found);
        }
        FullState::new_standard(num_players, deck).unwrap()
    }

    fn observed(id: usize, identity: Option<Card>, clues: &[Clue]) -> ObservedCard {
        let mut facts = ClueFacts::default();
        for clue in clues {
            facts.add_positive_clue(*clue);
        }
        ObservedCard {
            id: CardId::new(id),
            identity,
            clues: facts,
        }
    }

    #[test]
    fn learning_path_metadata_covers_every_cumulative_level() {
        assert_eq!(H_GROUP_LEVELS.len(), 26);
        for (index, descriptor) in H_GROUP_LEVELS.iter().enumerate() {
            assert_eq!(usize::from(descriptor.profile.effective_level()), index + 1);
            assert!(!descriptor.title.is_empty());
            assert!(!descriptor.effects.is_empty());
            assert_eq!(enabled_h_group_levels(descriptor.profile).len(), index + 1);
        }
        assert_eq!(enabled_h_group_levels(HGroupProfile::Max), &H_GROUP_LEVELS);
        assert_eq!(HGroupProfile::Max.effective_level(), 26);
    }

    #[test]
    fn level_two_enables_an_off_chop_five_stall() {
        let state = state_with_prefix(
            2,
            &[
                Card::new(Suit::Blue, Rank::Three),
                Card::new(Suit::Green, Rank::Three),
                Card::new(Suit::Yellow, Rank::Four),
                Card::new(Suit::Purple, Rank::Four),
                Card::new(Suit::Red, Rank::Three),
                Card::new(Suit::Blue, Rank::One),
                Card::new(Suit::Red, Rank::Five),
                Card::new(Suit::Green, Rank::Four),
                Card::new(Suit::Yellow, Rank::Three),
                Card::new(Suit::Purple, Rank::Four),
            ],
        );
        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
        let five_stall = Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        };
        let level_one =
            h_group_clue_candidates(&deductions, HGroupProfile::Level(HGroupLevel::Level1));
        let level_two =
            h_group_clue_candidates(&deductions, HGroupProfile::Level(HGroupLevel::Level2));
        let level_one_score = level_one
            .iter()
            .find(|candidate| candidate.action == five_stall)
            .map(|candidate| candidate.score);
        let level_two_score = level_two
            .iter()
            .find(|candidate| candidate.action == five_stall)
            .map(|candidate| candidate.score);
        assert!(level_two_score > level_one_score);
    }

    #[test]
    fn level_five_keeps_a_layered_finesse_as_an_exact_disjunction() {
        let mut state = state_with_prefix(
            3,
            &[
                Card::new(Suit::Blue, Rank::Four),
                Card::new(Suit::Green, Rank::Four),
                Card::new(Suit::Yellow, Rank::Three),
                Card::new(Suit::Purple, Rank::Four),
                Card::new(Suit::Blue, Rank::Five),
                Card::new(Suit::Yellow, Rank::Four),
                Card::new(Suit::Purple, Rank::Three),
                Card::new(Suit::Blue, Rank::Four),
                Card::new(Suit::Red, Rank::One),
                Card::new(Suit::Green, Rank::One),
                Card::new(Suit::Red, Rank::Two),
                Card::new(Suit::Blue, Rank::Three),
                Card::new(Suit::Green, Rank::Four),
                Card::new(Suit::Yellow, Rank::Five),
                Card::new(Suit::Purple, Rank::Four),
            ],
        );
        state
            .apply(Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Suit(Suit::Red),
            })
            .unwrap();
        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
        let level_four = infer_h_group(&deductions, HGroupProfile::Level(HGroupLevel::Level4));
        let level_five = infer_h_group(&deductions, HGroupProfile::Level(HGroupLevel::Level5));
        assert_eq!(
            level_four.connection_promises[0].cards,
            vec![CardId::new(9)]
        );
        assert_eq!(
            level_five.connection_promises[0].cards[..2],
            [CardId::new(9), CardId::new(8)]
        );
        assert!(
            level_five
                .signals
                .iter()
                .any(|signal| signal.kind == HGroupMoveKind::LayeredFinesse)
        );
    }

    #[test]
    fn level_sixteen_ejects_the_second_finesse_position() {
        let mut state = state_with_prefix(
            3,
            &[
                Card::new(Suit::Blue, Rank::Three),
                Card::new(Suit::Green, Rank::Four),
                Card::new(Suit::Yellow, Rank::Three),
                Card::new(Suit::Purple, Rank::Four),
                Card::new(Suit::Blue, Rank::Five),
                Card::new(Suit::Yellow, Rank::Four),
                Card::new(Suit::Purple, Rank::Three),
                Card::new(Suit::Blue, Rank::Four),
                Card::new(Suit::Green, Rank::One),
                Card::new(Suit::Yellow, Rank::One),
                Card::new(Suit::Red, Rank::Five),
                Card::new(Suit::Blue, Rank::Three),
                Card::new(Suit::Green, Rank::Four),
                Card::new(Suit::Yellow, Rank::Five),
                Card::new(Suit::Purple, Rank::Four),
            ],
        );
        state
            .apply(Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Suit(Suit::Red),
            })
            .unwrap();
        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
        let level_fifteen = infer_h_group(&deductions, HGroupProfile::Level(HGroupLevel::Level15));
        let level_sixteen = infer_h_group(&deductions, HGroupProfile::Level(HGroupLevel::Level16));
        assert!(!level_fifteen.playable_now.contains(&CardId::new(8)));
        assert!(level_sixteen.connection.is_none());
        assert!(level_sixteen.playable_now.contains(&CardId::new(8)));
        assert!(
            level_sixteen
                .signals
                .iter()
                .any(|signal| signal.kind == HGroupMoveKind::Ejection)
        );
    }

    #[test]
    fn focus_prefers_chop_when_a_clue_newly_touches_multiple_cards() {
        let red_one = Card::new(Suit::Red, Rank::One);
        let mut state = state_with_prefix(
            2,
            &[
                red_one,
                Card::new(Suit::Blue, Rank::Three),
                Card::new(Suit::Green, Rank::Four),
                Card::new(Suit::Yellow, Rank::Three),
                Card::new(Suit::Purple, Rank::Four),
                red_one,
                Card::new(Suit::Blue, Rank::One),
                Card::new(Suit::Green, Rank::Four),
                Card::new(Suit::Yellow, Rank::One),
                red_one,
            ],
        );
        state
            .apply(Action::Clue {
                target: PlayerId::new(1),
                clue: Clue::Rank(Rank::One),
            })
            .unwrap();
        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
        let inferred = infer_h_group(
            &deductions,
            HGroupProfile::Level(crate::HGroupLevel::Level1),
        );
        assert_eq!(inferred.clues[0].focus, CardId::new(5));
        assert!(inferred.clues[0].focus_was_chop);
    }

    #[test]
    fn focus_rules_cover_retouched_single_and_leftmost_new_cards() {
        let hand = (0..5).map(CardId::new).collect::<Vec<_>>();
        let gotten = [CardId::new(0), CardId::new(1)]
            .into_iter()
            .collect::<HashSet<_>>();
        let current_chop = chop(&hand, &gotten);
        assert_eq!(current_chop, Some(CardId::new(2)));
        assert_eq!(
            focus(
                &hand,
                &[CardId::new(0), CardId::new(1)],
                current_chop,
                &gotten,
            ),
            Some(CardId::new(1))
        );
        assert_eq!(
            focus(
                &hand,
                &[CardId::new(0), CardId::new(3)],
                current_chop,
                &gotten,
            ),
            Some(CardId::new(3))
        );
        assert_eq!(
            focus(
                &hand,
                &[CardId::new(3), CardId::new(4)],
                current_chop,
                &gotten,
            ),
            Some(CardId::new(4))
        );
        assert_eq!(
            focus(
                &hand,
                &[CardId::new(2), CardId::new(3)],
                current_chop,
                &gotten,
            ),
            Some(CardId::new(2))
        );
    }

    #[test]
    fn five_on_chop_is_a_save_and_is_not_played() {
        let mut state = state_with_prefix(
            2,
            &[
                Card::new(Suit::Red, Rank::One),
                Card::new(Suit::Blue, Rank::One),
                Card::new(Suit::Green, Rank::Three),
                Card::new(Suit::Yellow, Rank::Four),
                Card::new(Suit::Purple, Rank::One),
                Card::new(Suit::Red, Rank::Five),
            ],
        );
        state
            .apply(Action::Clue {
                target: PlayerId::new(1),
                clue: Clue::Rank(Rank::Five),
            })
            .unwrap();
        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
        let inferred = infer_h_group(
            &deductions,
            HGroupProfile::Level(crate::HGroupLevel::Level1),
        );
        assert_eq!(
            inferred.clues[0].kind,
            HGroupClueKind::Save(HGroupSaveKind::Five)
        );
        assert_eq!(inferred.saved, vec![CardId::new(5)]);
        assert!(inferred.playable_now.is_empty());
    }

    #[test]
    fn delayed_play_clue_finesses_newest_unclued_card() {
        let mut state = state_with_prefix(
            3,
            &[
                Card::new(Suit::Blue, Rank::Four),
                Card::new(Suit::Green, Rank::Four),
                Card::new(Suit::Yellow, Rank::Three),
                Card::new(Suit::Purple, Rank::Four),
                Card::new(Suit::Blue, Rank::Five),
                Card::new(Suit::Red, Rank::Three),
                Card::new(Suit::Blue, Rank::Two),
                Card::new(Suit::Green, Rank::Four),
                Card::new(Suit::Yellow, Rank::Five),
                Card::new(Suit::Red, Rank::One),
                Card::new(Suit::Red, Rank::Two),
            ],
        );
        state
            .apply(Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Suit(Suit::Red),
            })
            .unwrap();
        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
        let convention =
            crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
        let inferred = infer_h_group(
            &deductions,
            HGroupProfile::Level(crate::HGroupLevel::Level1),
        );
        assert_eq!(
            inferred.connection,
            Some(HGroupConnection {
                card: CardId::new(9),
                identity: Card::new(Suit::Red, Rank::One),
                kind: HGroupConnectionKind::Finesse,
                focus: CardId::new(10),
            })
        );

        // A reconstructed or otherwise arbitrary view can invalidate a
        // convention promise while retaining its public clue history. Such a
        // stale promise must never escape as an illegal search candidate.
        let mut stale_view = state.view_for(PlayerId::new(1)).unwrap();
        stale_view.hands[1]
            .iter_mut()
            .find(|card| card.id == CardId::new(9))
            .unwrap()
            .id = CardId::new(15);
        let stale_deductions = LogicalDeductions::new(stale_view).unwrap();
        let stale_legal = stale_deductions.view().legal_actions();
        let stale_candidates = h_group_candidate_actions(
            &stale_deductions,
            HGroupProfile::Level(crate::HGroupLevel::Level1),
        );
        assert!(!stale_candidates.contains(&Action::Play(CardId::new(9))));
        assert!(
            stale_candidates
                .iter()
                .all(|action| stale_legal.contains(action))
        );

        let finesse = crate::RolloutPolicy::select_action(&convention, &deductions).unwrap();
        assert_eq!(finesse, Action::Play(CardId::new(9)));
        state.apply(finesse).unwrap();

        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
        assert_eq!(
            crate::RolloutPolicy::select_action(&convention, &deductions).unwrap(),
            Action::Play(CardId::new(10))
        );
    }

    #[test]
    fn prompt_takes_precedence_over_finesse_and_policy_plays_it() {
        let mut state = state_with_prefix(
            3,
            &[
                Card::new(Suit::Blue, Rank::Three),
                Card::new(Suit::Green, Rank::Four),
                Card::new(Suit::Yellow, Rank::Three),
                Card::new(Suit::Yellow, Rank::Five),
                Card::new(Suit::Purple, Rank::Two),
                Card::new(Suit::Red, Rank::Four),
                Card::new(Suit::Red, Rank::Four),
                Card::new(Suit::Red, Rank::One),
                Card::new(Suit::Yellow, Rank::Four),
                Card::new(Suit::Blue, Rank::Four),
                Card::new(Suit::Red, Rank::Two),
                Card::new(Suit::Blue, Rank::Three),
                Card::new(Suit::Green, Rank::Two),
                Card::new(Suit::Yellow, Rank::Three),
                Card::new(Suit::Purple, Rank::Five),
            ],
        );
        state
            .apply(Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Rank(Rank::Five),
            })
            .unwrap();
        state.apply(Action::Discard(CardId::new(5))).unwrap();
        state
            .apply(Action::Clue {
                target: PlayerId::new(1),
                clue: Clue::Suit(Suit::Red),
            })
            .unwrap();
        state
            .apply(Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Suit(Suit::Red),
            })
            .unwrap();

        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
        let convention =
            crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
        let inferred = infer_h_group(
            &deductions,
            HGroupProfile::Level(crate::HGroupLevel::Level1),
        );
        assert_eq!(
            inferred.connection,
            Some(HGroupConnection {
                card: CardId::new(15),
                identity: Card::new(Suit::Red, Rank::One),
                kind: HGroupConnectionKind::Prompt,
                focus: CardId::new(10),
            })
        );
        assert_eq!(
            crate::RolloutPolicy::select_action(&convention, &deductions).unwrap(),
            Action::Play(CardId::new(15))
        );
    }

    #[test]
    fn policy_gives_and_recipient_plays_a_direct_level_one_play_clue() {
        let mut state = state_with_prefix(
            2,
            &[
                Card::new(Suit::Blue, Rank::Three),
                Card::new(Suit::Green, Rank::Three),
                Card::new(Suit::Yellow, Rank::Four),
                Card::new(Suit::Purple, Rank::Five),
                Card::new(Suit::Blue, Rank::Three),
                Card::new(Suit::Red, Rank::One),
                Card::new(Suit::Blue, Rank::Four),
                Card::new(Suit::Green, Rank::Five),
                Card::new(Suit::Yellow, Rank::Two),
                Card::new(Suit::Purple, Rank::Three),
            ],
        );
        let convention =
            crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
        let clue = crate::RolloutPolicy::select_action(&convention, &deductions).unwrap();
        assert_eq!(
            clue,
            Action::Clue {
                target: PlayerId::new(1),
                clue: Clue::Suit(Suit::Red),
            }
        );
        state.apply(clue).unwrap();

        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
        assert_eq!(
            crate::RolloutPolicy::select_action(&convention, &deductions).unwrap(),
            Action::Play(CardId::new(5))
        );
    }

    #[test]
    fn save_principle_prefers_a_five_save_over_a_play_clue() {
        let state = state_with_prefix(
            3,
            &[
                Card::new(Suit::Blue, Rank::Two),
                Card::new(Suit::Green, Rank::Three),
                Card::new(Suit::Yellow, Rank::Four),
                Card::new(Suit::Purple, Rank::Two),
                Card::new(Suit::Blue, Rank::Three),
                Card::new(Suit::Red, Rank::Five),
                Card::new(Suit::Blue, Rank::Four),
                Card::new(Suit::Green, Rank::Two),
                Card::new(Suit::Yellow, Rank::Three),
                Card::new(Suit::Purple, Rank::Four),
                Card::new(Suit::Red, Rank::One),
                Card::new(Suit::Blue, Rank::Five),
                Card::new(Suit::Green, Rank::Four),
                Card::new(Suit::Yellow, Rank::Two),
                Card::new(Suit::Purple, Rank::Three),
            ],
        );
        let convention =
            crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
        assert_eq!(
            crate::RolloutPolicy::select_action(&convention, &deductions).unwrap(),
            Action::Clue {
                target: PlayerId::new(1),
                clue: Clue::Rank(Rank::Five),
            }
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn chop_clue_note_keeps_both_play_and_critical_save_possibilities() {
        let blue = Clue::Suit(Suit::Blue);
        let blue_one = Card::new(Suit::Blue, Rank::One);
        let blue_two = Card::new(Suit::Blue, Rank::Two);
        let blue_four = Card::new(Suit::Blue, Rank::Four);
        let view = PlayerView {
            observer: PlayerId::new(1),
            current_player: PlayerId::new(1),
            turn: 7,
            hands: vec![
                vec![
                    observed(2, Some(Card::new(Suit::Green, Rank::One)), &[]),
                    observed(3, Some(Card::new(Suit::Yellow, Rank::One)), &[]),
                    observed(4, Some(Card::new(Suit::Purple, Rank::One)), &[]),
                    observed(15, Some(Card::new(Suit::Red, Rank::Two)), &[]),
                    observed(17, Some(Card::new(Suit::Red, Rank::One)), &[]),
                ],
                vec![
                    observed(5, None, &[blue]),
                    observed(6, None, &[]),
                    observed(7, None, &[]),
                    observed(8, None, &[]),
                    observed(9, None, &[]),
                ],
                vec![
                    observed(11, Some(Card::new(Suit::Green, Rank::Three)), &[]),
                    observed(12, Some(Card::new(Suit::Yellow, Rank::Three)), &[]),
                    observed(13, Some(Card::new(Suit::Purple, Rank::Three)), &[]),
                    observed(14, Some(Card::new(Suit::Red, Rank::Four)), &[]),
                    observed(16, Some(Card::new(Suit::Blue, Rank::Five)), &[]),
                ],
            ],
            deck_size: 32,
            play_stacks: [
                Vec::new(),
                Vec::new(),
                Vec::new(),
                vec![(CardId::new(0), blue_one), (CardId::new(10), blue_two)],
                Vec::new(),
            ],
            discard_pile: vec![(CardId::new(1), blue_four)],
            clue_tokens: 7,
            strikes: 0,
            final_turns_remaining: None,
            status: GameStatus::InProgress,
            history: vec![
                ObservedHistoryEntry {
                    turn: 0,
                    event: ObservedEvent::Played {
                        player: PlayerId::new(0),
                        card: CardId::new(0),
                        identity: blue_one,
                        successful: true,
                    },
                },
                ObservedHistoryEntry {
                    turn: 0,
                    event: ObservedEvent::Drew {
                        player: PlayerId::new(0),
                        card: CardId::new(15),
                        identity: Some(Card::new(Suit::Red, Rank::Two)),
                    },
                },
                ObservedHistoryEntry {
                    turn: 2,
                    event: ObservedEvent::Played {
                        player: PlayerId::new(2),
                        card: CardId::new(10),
                        identity: blue_two,
                        successful: true,
                    },
                },
                ObservedHistoryEntry {
                    turn: 2,
                    event: ObservedEvent::Drew {
                        player: PlayerId::new(2),
                        card: CardId::new(16),
                        identity: Some(Card::new(Suit::Blue, Rank::Five)),
                    },
                },
                ObservedHistoryEntry {
                    turn: 3,
                    event: ObservedEvent::Discarded {
                        player: PlayerId::new(0),
                        card: CardId::new(1),
                        identity: blue_four,
                    },
                },
                ObservedHistoryEntry {
                    turn: 3,
                    event: ObservedEvent::Drew {
                        player: PlayerId::new(0),
                        card: CardId::new(17),
                        identity: Some(Card::new(Suit::Red, Rank::One)),
                    },
                },
                ObservedHistoryEntry {
                    turn: 6,
                    event: ObservedEvent::Clued {
                        giver: PlayerId::new(0),
                        target: PlayerId::new(1),
                        clue: blue,
                        touched: vec![CardId::new(5)],
                        untouched: vec![
                            CardId::new(6),
                            CardId::new(7),
                            CardId::new(8),
                            CardId::new(9),
                        ],
                    },
                },
            ],
        };
        let deductions = LogicalDeductions::new(view).unwrap();
        let inferred = infer_h_group(
            &deductions,
            HGroupProfile::Level(crate::HGroupLevel::Level1),
        );
        let focus = inferred
            .cards
            .iter()
            .find(|card| card.card == CardId::new(5))
            .unwrap();
        assert_eq!(
            focus.identities,
            IdentitySet::singleton(Card::new(Suit::Blue, Rank::Three))
                .union(IdentitySet::singleton(blue_four))
        );
        assert!(focus.saved);
        assert!(!inferred.playable_now.contains(&CardId::new(5)));
    }

    #[test]
    fn double_chop_twos_are_an_exception_to_the_visible_rule() {
        let red_two = Card::new(Suit::Red, Rank::Two);
        let state = state_with_prefix(
            3,
            &[
                Card::new(Suit::Blue, Rank::Three),
                Card::new(Suit::Green, Rank::Four),
                Card::new(Suit::Yellow, Rank::Five),
                Card::new(Suit::Purple, Rank::Three),
                Card::new(Suit::Blue, Rank::Four),
                red_two,
                Card::new(Suit::Green, Rank::Three),
                Card::new(Suit::Yellow, Rank::Four),
                Card::new(Suit::Purple, Rank::Five),
                Card::new(Suit::Blue, Rank::Five),
                red_two,
                Card::new(Suit::Green, Rank::Two),
                Card::new(Suit::Yellow, Rank::Three),
                Card::new(Suit::Purple, Rank::Four),
                Card::new(Suit::Blue, Rank::Two),
            ],
        );
        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
        let candidates =
            h_group_clue_candidates(&deductions, HGroupProfile::Level(HGroupLevel::Level1));
        for target in [PlayerId::new(1), PlayerId::new(2)] {
            assert!(candidates.iter().any(|candidate| {
                candidate.action
                    == Action::Clue {
                        target,
                        clue: Clue::Rank(Rank::Two),
                    }
            }));
        }
    }

    #[test]
    fn finesse_card_is_invisibly_clued_for_later_focus() {
        let mut state = state_with_prefix(
            3,
            &[
                Card::new(Suit::Blue, Rank::Three),
                Card::new(Suit::Green, Rank::Four),
                Card::new(Suit::Yellow, Rank::Five),
                Card::new(Suit::Purple, Rank::Three),
                Card::new(Suit::Blue, Rank::Four),
                Card::new(Suit::Green, Rank::Three),
                Card::new(Suit::Yellow, Rank::Four),
                Card::new(Suit::Purple, Rank::Five),
                Card::new(Suit::Blue, Rank::One),
                Card::new(Suit::Red, Rank::One),
                Card::new(Suit::Green, Rank::Five),
                Card::new(Suit::Blue, Rank::Two),
                Card::new(Suit::Yellow, Rank::Three),
                Card::new(Suit::Purple, Rank::Four),
                Card::new(Suit::Red, Rank::Two),
            ],
        );
        state
            .apply(Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Suit(Suit::Red),
            })
            .unwrap();
        state
            .apply(Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Rank(Rank::Five),
            })
            .unwrap();
        state
            .apply(Action::Clue {
                target: PlayerId::new(0),
                clue: Clue::Suit(Suit::Blue),
            })
            .unwrap();
        state
            .apply(Action::Clue {
                target: PlayerId::new(1),
                clue: Clue::Rank(Rank::One),
            })
            .unwrap();

        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
        let inferred = infer_h_group(
            &deductions,
            HGroupProfile::Level(crate::HGroupLevel::Level1),
        );
        assert!(inferred.invisibly_clued.contains(&CardId::new(9)));
        assert_eq!(inferred.clues.last().unwrap().focus, CardId::new(8));
    }

    #[test]
    fn early_game_ends_only_when_a_chop_is_discarded() {
        let mut state = state_with_prefix(
            2,
            &[
                Card::new(Suit::Blue, Rank::Two),
                Card::new(Suit::Green, Rank::Three),
                Card::new(Suit::Yellow, Rank::Four),
                Card::new(Suit::Purple, Rank::Five),
                Card::new(Suit::Blue, Rank::Three),
                Card::new(Suit::Red, Rank::One),
                Card::new(Suit::Blue, Rank::Four),
                Card::new(Suit::Green, Rank::Five),
                Card::new(Suit::Yellow, Rank::Two),
                Card::new(Suit::Purple, Rank::Three),
            ],
        );
        let initial = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
        assert!(
            infer_h_group(&initial, HGroupProfile::Level(crate::HGroupLevel::Level1)).early_game
        );
        state
            .apply(Action::Clue {
                target: PlayerId::new(1),
                clue: Clue::Suit(Suit::Red),
            })
            .unwrap();
        state.apply(Action::Discard(CardId::new(6))).unwrap();
        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
        assert!(
            !infer_h_group(
                &deductions,
                HGroupProfile::Level(crate::HGroupLevel::Level1)
            )
            .early_game
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn prompt_promise_continues_left_to_right_after_a_wrong_card_plays() {
        let mut state = state_with_prefix(
            3,
            &[
                Card::new(Suit::Blue, Rank::Two),
                Card::new(Suit::Green, Rank::Three),
                Card::new(Suit::Yellow, Rank::Four),
                Card::new(Suit::Purple, Rank::Five),
                Card::new(Suit::Blue, Rank::Three),
                Card::new(Suit::Green, Rank::Four),
                Card::new(Suit::Yellow, Rank::Three),
                Card::new(Suit::Red, Rank::One),
                Card::new(Suit::Blue, Rank::One),
                Card::new(Suit::Purple, Rank::One),
                Card::new(Suit::Red, Rank::Two),
                Card::new(Suit::Green, Rank::Four),
                Card::new(Suit::Yellow, Rank::Five),
                Card::new(Suit::Purple, Rank::Two),
                Card::new(Suit::Blue, Rank::Four),
            ],
        );
        state
            .apply(Action::Clue {
                target: PlayerId::new(1),
                clue: Clue::Rank(Rank::One),
            })
            .unwrap();
        state
            .apply(Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Suit(Suit::Green),
            })
            .unwrap();
        state
            .apply(Action::Clue {
                target: PlayerId::new(0),
                clue: Clue::Suit(Suit::Blue),
            })
            .unwrap();
        state
            .apply(Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Suit(Suit::Red),
            })
            .unwrap();

        let information_set =
            crate::InformationSet::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
        let inferred = infer_h_group(
            &information_set,
            HGroupProfile::Level(crate::HGroupLevel::Level1),
        );
        assert_eq!(inferred.connection.unwrap().card, CardId::new(8));
        assert_eq!(
            inferred.connection_promises,
            vec![HGroupConnectionPromise {
                cards: vec![CardId::new(8), CardId::new(7)],
                identity: Card::new(Suit::Red, Rank::One),
            }]
        );
        let convention =
            crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
        let expected = Card::new(Suit::Red, Rank::One);
        let mut saw_first_match = false;
        let mut saw_continuation = false;
        for seed in 0..128 {
            let sampled = crate::ConventionFramework::sample_root_world(
                &convention,
                &information_set,
                &mut StdRng::seed_from_u64(seed),
            )
            .unwrap();
            if sampled.card(CardId::new(8)) == Some(expected) {
                saw_first_match = true;
            } else {
                saw_continuation = true;
                assert_eq!(sampled.card(CardId::new(8)).unwrap().rank, Rank::One);
                assert_eq!(sampled.card(CardId::new(7)), Some(expected));
            }
        }
        assert!(saw_first_match && saw_continuation);
        state.apply(Action::Play(CardId::new(8))).unwrap();
        state
            .apply(Action::Clue {
                target: PlayerId::new(0),
                clue: Clue::Suit(Suit::Green),
            })
            .unwrap();
        state
            .apply(Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Suit(Suit::Blue),
            })
            .unwrap();

        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
        let inferred = infer_h_group(
            &deductions,
            HGroupProfile::Level(crate::HGroupLevel::Level1),
        );
        assert_eq!(
            inferred.connection,
            Some(HGroupConnection {
                card: CardId::new(7),
                identity: Card::new(Suit::Red, Rank::One),
                kind: HGroupConnectionKind::Prompt,
                focus: CardId::new(10),
            })
        );
    }

    #[test]
    fn delayed_play_can_use_one_finesse_then_an_accounted_chain() {
        let mut state = state_with_prefix(
            3,
            &[
                Card::new(Suit::Blue, Rank::One),
                Card::new(Suit::Green, Rank::Three),
                Card::new(Suit::Yellow, Rank::Four),
                Card::new(Suit::Purple, Rank::Five),
                Card::new(Suit::Blue, Rank::Three),
                Card::new(Suit::Green, Rank::Four),
                Card::new(Suit::Yellow, Rank::Three),
                Card::new(Suit::Purple, Rank::Four),
                Card::new(Suit::Blue, Rank::Five),
                Card::new(Suit::Red, Rank::One),
                Card::new(Suit::Red, Rank::Two),
                Card::new(Suit::Red, Rank::Three),
                Card::new(Suit::Red, Rank::Four),
                Card::new(Suit::Yellow, Rank::Five),
                Card::new(Suit::Purple, Rank::Two),
            ],
        );
        state
            .apply(Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Rank(Rank::Two),
            })
            .unwrap();
        state
            .apply(Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Rank(Rank::Three),
            })
            .unwrap();
        state
            .apply(Action::Clue {
                target: PlayerId::new(0),
                clue: Clue::Suit(Suit::Blue),
            })
            .unwrap();

        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
        let convention =
            crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
        let candidates = crate::ConventionFramework::candidate_actions(&convention, &deductions);
        assert!(
            candidates.contains(&Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Suit(Suit::Red),
            }),
            "candidates: {candidates:?}; clues: {:?}",
            h_group_clue_candidates(&deductions, HGroupProfile::Level(HGroupLevel::Level1))
        );
    }

    #[test]
    fn good_touch_notes_and_root_sampling_exclude_a_duplicate_focus() {
        let mut state = state_with_prefix(
            2,
            &[
                Card::new(Suit::Blue, Rank::Two),
                Card::new(Suit::Green, Rank::Three),
                Card::new(Suit::Yellow, Rank::Four),
                Card::new(Suit::Purple, Rank::Five),
                Card::new(Suit::Blue, Rank::Three),
                Card::new(Suit::Green, Rank::Two),
                Card::new(Suit::Yellow, Rank::Three),
                Card::new(Suit::Purple, Rank::Four),
                Card::new(Suit::Red, Rank::Two),
                Card::new(Suit::Red, Rank::One),
            ],
        );
        state
            .apply(Action::Clue {
                target: PlayerId::new(1),
                clue: Clue::Suit(Suit::Red),
            })
            .unwrap();
        let information_set =
            crate::InformationSet::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
        let inferred = infer_h_group(
            &information_set,
            HGroupProfile::Level(crate::HGroupLevel::Level1),
        );
        let focus = inferred
            .cards
            .iter()
            .find(|card| card.card == CardId::new(9))
            .unwrap();
        let non_focus = inferred
            .cards
            .iter()
            .find(|card| card.card == CardId::new(8))
            .unwrap();
        assert_eq!(
            focus.identities,
            IdentitySet::singleton(Card::new(Suit::Red, Rank::One))
        );
        assert!(
            !non_focus
                .identities
                .contains(Card::new(Suit::Red, Rank::One))
        );

        let convention =
            crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
        let mut rng = StdRng::seed_from_u64(7);
        for _ in 0..32 {
            let sampled = crate::ConventionFramework::sample_root_world(
                &convention,
                &information_set,
                &mut rng,
            )
            .unwrap();
            assert_eq!(
                sampled.card(CardId::new(9)),
                Some(Card::new(Suit::Red, Rank::One))
            );
            assert_ne!(
                sampled.card(CardId::new(8)),
                Some(Card::new(Suit::Red, Rank::One))
            );
        }
    }

    #[test]
    fn visible_two_off_chop_prevents_a_two_save() {
        let red_two = Card::new(Suit::Red, Rank::Two);
        let mut state = state_with_prefix(
            3,
            &[
                Card::new(Suit::Blue, Rank::Three),
                Card::new(Suit::Green, Rank::Three),
                Card::new(Suit::Yellow, Rank::Four),
                Card::new(Suit::Purple, Rank::Four),
                Card::new(Suit::Blue, Rank::Four),
                red_two,
                Card::new(Suit::Green, Rank::Four),
                Card::new(Suit::Yellow, Rank::Three),
                Card::new(Suit::Purple, Rank::Three),
                Card::new(Suit::Blue, Rank::Five),
                Card::new(Suit::Green, Rank::Five),
                red_two,
                Card::new(Suit::Yellow, Rank::Two),
                Card::new(Suit::Purple, Rank::Two),
                Card::new(Suit::Blue, Rank::Two),
            ],
        );
        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
        let convention =
            crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
        assert!(
            !crate::ConventionFramework::candidate_actions(&convention, &deductions).contains(
                &Action::Clue {
                    target: PlayerId::new(1),
                    clue: Clue::Rank(Rank::Two),
                }
            )
        );

        state
            .apply(Action::Clue {
                target: PlayerId::new(1),
                clue: Clue::Rank(Rank::Two),
            })
            .unwrap();
        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
        let inferred = infer_h_group(
            &deductions,
            HGroupProfile::Level(crate::HGroupLevel::Level1),
        );
        assert_eq!(
            inferred.clues.last().unwrap().kind,
            HGroupClueKind::Unrecognized
        );
        assert!(inferred.clues.last().unwrap().save_identities.is_empty());
    }

    #[test]
    fn playable_two_on_chop_is_a_play_clue_not_a_save() {
        let mut state = state_with_prefix(
            3,
            &[
                Card::new(Suit::Red, Rank::One),
                Card::new(Suit::Blue, Rank::Three),
                Card::new(Suit::Green, Rank::Four),
                Card::new(Suit::Yellow, Rank::Three),
                Card::new(Suit::Purple, Rank::Four),
                Card::new(Suit::Blue, Rank::Four),
                Card::new(Suit::Green, Rank::Three),
                Card::new(Suit::Yellow, Rank::Four),
                Card::new(Suit::Purple, Rank::Three),
                Card::new(Suit::Blue, Rank::Five),
                Card::new(Suit::Red, Rank::Two),
                Card::new(Suit::Green, Rank::Five),
                Card::new(Suit::Yellow, Rank::Two),
                Card::new(Suit::Purple, Rank::Two),
                Card::new(Suit::Blue, Rank::Two),
            ],
        );
        state.apply(Action::Play(CardId::new(0))).unwrap();
        state
            .apply(Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Rank(Rank::Two),
            })
            .unwrap();
        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
        let inferred = infer_h_group(
            &deductions,
            HGroupProfile::Level(crate::HGroupLevel::Level1),
        );
        assert_eq!(inferred.clues.last().unwrap().kind, HGroupClueKind::Play);
        assert!(inferred.clues.last().unwrap().save_identities.is_empty());
    }

    #[test]
    fn convention_known_trash_is_discarded_before_the_chop() {
        let mut state = state_with_prefix(
            2,
            &[
                Card::new(Suit::Red, Rank::One),
                Card::new(Suit::Blue, Rank::Three),
                Card::new(Suit::Green, Rank::Three),
                Card::new(Suit::Yellow, Rank::Four),
                Card::new(Suit::Purple, Rank::Four),
                Card::new(Suit::Red, Rank::One),
                Card::new(Suit::Blue, Rank::Four),
                Card::new(Suit::Green, Rank::Four),
                Card::new(Suit::Yellow, Rank::Three),
                Card::new(Suit::Purple, Rank::Three),
                Card::new(Suit::Blue, Rank::Four),
            ],
        );
        state
            .apply(Action::Clue {
                target: PlayerId::new(1),
                clue: Clue::Suit(Suit::Red),
            })
            .unwrap();
        state
            .apply(Action::Clue {
                target: PlayerId::new(0),
                clue: Clue::Suit(Suit::Red),
            })
            .unwrap();
        state.apply(Action::Play(CardId::new(0))).unwrap();

        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
        let convention =
            crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
        let inferred = infer_h_group(
            &deductions,
            HGroupProfile::Level(crate::HGroupLevel::Level1),
        );
        assert_eq!(inferred.chops[1], Some(CardId::new(6)));
        assert_eq!(
            inferred
                .cards
                .iter()
                .find(|card| card.card == CardId::new(5))
                .unwrap()
                .identities,
            IdentitySet::singleton(Card::new(Suit::Red, Rank::One))
        );
        assert_eq!(
            crate::RolloutPolicy::select_action(&convention, &deductions).unwrap(),
            Action::Discard(CardId::new(5))
        );
    }

    #[test]
    fn no_chop_forces_a_tempo_clue_instead_of_a_card_action() {
        let mut state = state_with_prefix(
            2,
            &[
                Card::new(Suit::Red, Rank::Five),
                Card::new(Suit::Blue, Rank::Five),
                Card::new(Suit::Green, Rank::Five),
                Card::new(Suit::Yellow, Rank::Five),
                Card::new(Suit::Purple, Rank::Five),
                Card::new(Suit::Red, Rank::One),
                Card::new(Suit::Blue, Rank::One),
                Card::new(Suit::Green, Rank::One),
                Card::new(Suit::Yellow, Rank::One),
                Card::new(Suit::Purple, Rank::One),
            ],
        );
        state
            .apply(Action::Clue {
                target: PlayerId::new(1),
                clue: Clue::Rank(Rank::One),
            })
            .unwrap();
        state
            .apply(Action::Clue {
                target: PlayerId::new(0),
                clue: Clue::Rank(Rank::Five),
            })
            .unwrap();
        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
        let convention =
            crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
        let inferred = infer_h_group(
            &deductions,
            HGroupProfile::Level(crate::HGroupLevel::Level1),
        );
        assert_eq!(inferred.chops[0], None);
        let candidates = crate::ConventionFramework::candidate_actions(&convention, &deductions);
        assert!(!candidates.is_empty());
        assert!(
            candidates
                .iter()
                .all(|action| matches!(action, Action::Clue { .. }))
        );
    }

    #[test]
    fn rank_clue_beats_color_only_when_it_gets_more_cards() {
        let state = state_with_prefix(
            2,
            &[
                Card::new(Suit::Blue, Rank::Three),
                Card::new(Suit::Green, Rank::Three),
                Card::new(Suit::Yellow, Rank::Four),
                Card::new(Suit::Purple, Rank::Four),
                Card::new(Suit::Blue, Rank::Four),
                Card::new(Suit::Red, Rank::One),
                Card::new(Suit::Blue, Rank::One),
                Card::new(Suit::Green, Rank::Four),
                Card::new(Suit::Yellow, Rank::Three),
                Card::new(Suit::Purple, Rank::Three),
            ],
        );
        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
        let convention =
            crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
        assert_eq!(
            crate::RolloutPolicy::select_action(&convention, &deductions).unwrap(),
            Action::Clue {
                target: PlayerId::new(1),
                clue: Clue::Rank(Rank::One),
            }
        );
    }

    #[test]
    fn play_clue_to_the_same_player_preoccupies_before_a_save() {
        let state = state_with_prefix(
            2,
            &[
                Card::new(Suit::Red, Rank::Three),
                Card::new(Suit::Green, Rank::Three),
                Card::new(Suit::Yellow, Rank::Four),
                Card::new(Suit::Purple, Rank::Four),
                Card::new(Suit::Blue, Rank::Three),
                Card::new(Suit::Red, Rank::Five),
                Card::new(Suit::Green, Rank::Four),
                Card::new(Suit::Yellow, Rank::Three),
                Card::new(Suit::Purple, Rank::Three),
                Card::new(Suit::Blue, Rank::One),
            ],
        );
        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
        let convention =
            crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
        assert_eq!(
            crate::RolloutPolicy::select_action(&convention, &deductions).unwrap(),
            Action::Clue {
                target: PlayerId::new(1),
                clue: Clue::Suit(Suit::Blue),
            }
        );
    }

    #[test]
    fn next_players_unique_playable_chop_preempts_own_play() {
        let mut state = state_with_prefix(
            3,
            &[
                Card::new(Suit::Red, Rank::One),
                Card::new(Suit::Green, Rank::Three),
                Card::new(Suit::Yellow, Rank::Four),
                Card::new(Suit::Purple, Rank::Four),
                Card::new(Suit::Blue, Rank::Three),
                Card::new(Suit::Blue, Rank::One),
                Card::new(Suit::Green, Rank::Four),
                Card::new(Suit::Yellow, Rank::Three),
                Card::new(Suit::Purple, Rank::Three),
                Card::new(Suit::Red, Rank::Three),
                Card::new(Suit::Purple, Rank::Five),
                Card::new(Suit::Green, Rank::Two),
                Card::new(Suit::Yellow, Rank::Two),
                Card::new(Suit::Purple, Rank::Two),
                Card::new(Suit::Blue, Rank::Four),
            ],
        );
        state
            .apply(Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Rank(Rank::Five),
            })
            .unwrap();
        state
            .apply(Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Suit(Suit::Purple),
            })
            .unwrap();
        state
            .apply(Action::Clue {
                target: PlayerId::new(0),
                clue: Clue::Suit(Suit::Red),
            })
            .unwrap();

        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
        let convention =
            crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
        let candidates = crate::ConventionFramework::candidate_actions(&convention, &deductions);
        assert_eq!(
            candidates.first(),
            Some(&Action::Clue {
                target: PlayerId::new(1),
                clue: Clue::Suit(Suit::Blue),
            })
        );
        assert!(candidates.iter().all(|action| matches!(
            action,
            Action::Clue {
                target,
                ..
            } if *target == PlayerId::new(1)
        )));
    }

    #[test]
    fn level_one_policy_can_roll_a_game_to_completion() {
        let convention =
            crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
        for players in 2..=5 {
            let mut deck = standard_deck();
            deck.rotate_left(usize::from(players) * 3);
            let state = FullState::new_standard(players, deck).unwrap();
            let outcome = crate::rollout_to_terminal(state, &convention).unwrap();
            assert!(outcome.turns() > 0);
            assert!(outcome.turns() < crate::MAX_ROLLOUT_TURNS as usize);
        }
    }

    #[test]
    fn every_numbered_and_max_profile_rolls_to_completion() {
        let profiles = H_GROUP_LEVELS.iter().map(|descriptor| descriptor.profile);
        for profile in profiles {
            let mut deck = standard_deck();
            deck.rotate_left(11);
            let state = FullState::new_standard(3, deck).unwrap();
            let convention = crate::SupportedConvention::HGroup(profile);
            let outcome = crate::rollout_to_terminal(state, &convention)
                .unwrap_or_else(|error| panic!("{profile} rollout failed: {error}"));
            assert!(outcome.turns() > 0, "{profile}");
            assert!(
                outcome.turns() < crate::MAX_ROLLOUT_TURNS as usize,
                "{profile}"
            );
        }
    }
}
