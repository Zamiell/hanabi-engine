use std::collections::HashSet;
use std::hash::{BuildHasherDefault, Hasher};

use hanabi_core::{Card, CardId, Clue, PlayerId};

use crate::IdentitySet;

use super::{CardKnowledgeEffect, ConventionKnowledge, KnowledgeSource};

/// Card and player identifiers are already compact, collision-free keys in a
/// standard game. Using the general-purpose randomized hasher for the many
/// small convention sets adds work without improving their safety.
#[derive(Default)]
pub(super) struct CompactIdHasher(u64);

impl Hasher for CompactIdHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        self.0 = bytes
            .iter()
            .fold(0_u64, |value, byte| (value << 8) | u64::from(*byte));
    }
}

pub(super) type CardSet = HashSet<CardId, BuildHasherDefault<CompactIdHasher>>;
pub(super) type PlayerSet = HashSet<PlayerId, BuildHasherDefault<CompactIdHasher>>;

use super::{
    ConnectionManager, ConventionFacts, ConventionTransitionResult, EffectSource, HGroupMoveKind,
    MutationDomain, ProvenancedCardSet, SignalHistory,
};

/// How far observer projection may recurse while interpreting conventions.
/// A named mode prevents call sites from silently disagreeing about the
/// meaning of a bare boolean.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum PerspectiveDepth {
    ObserverOnly,
    NestedRecipients,
}

impl PerspectiveDepth {
    pub(super) const fn models_other_players(self) -> bool {
        matches!(self, Self::NestedRecipients)
    }
}

/// One convention interpretation found while reducing public history.
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
    EightClue,
}

/// How a delayed Play clue identifies its next connecting card.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HGroupConnectionKind {
    /// A previously-clued card matching the connection, preferred by Level 1.
    Prompt,
    /// The newest unclued card when no Prompt exists.
    Finesse,
}

/// One public clue interpreted using H-Group focus rules.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HGroupClueInterpretation {
    pub turn: u32,
    pub giver: PlayerId,
    pub target: PlayerId,
    pub clue: Clue,
    /// Every card physically touched by the clue, including cards that were
    /// already gotten. This preserves causal evidence for later Good Touch
    /// closure without reconstructing it from current hand state.
    pub touched: Vec<CardId>,
    /// Stack heights when the clue was given. Direct and delayed meanings are
    /// fixed at clue time; later plays must not retroactively create Prompts.
    pub stack_heights: [u8; 5],
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
    /// Trash identities established on non-focus cards when the focused Play
    /// Clue completes the clue's suit. These cards are useful collateral: the
    /// recipient gets both a play and a future safe discard.
    pub non_focus_trash_identities: Vec<(CardId, IdentitySet)>,
    /// Explicit and invisible clues that existed before this clue.
    pub previously_gotten: Vec<CardId>,
    /// Correlated identity readings retained before one canonical branch is
    /// selected for immediate action scheduling. Each reading owns its
    /// connection steps and conditional repair instead of flattening those
    /// consequences into the focus card's identity union.
    pub(super) hypotheses: Vec<ClueInterpretationHypothesis>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ClueConnectionStep {
    pub(super) actor: PlayerId,
    pub(super) cards: Vec<CardId>,
    pub(super) expected: Card,
    pub(super) kind: HGroupConnectionKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ClueInterpretationHypothesis {
    pub(super) focus_identity: Card,
    pub(super) connection_steps: Vec<ClueConnectionStep>,
    pub(super) required_fix: Option<RequiredFix>,
    pub(super) loaded: bool,
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
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HGroupIdentityStatus {
    #[default]
    Settled,
    /// A direct identity written while a different queued suit plan remains
    /// unresolved. It is knowledge, but not yet a play or discard permission.
    Provisional,
}

/// Why convention knowledge requires a card to be played blindly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HGroupPlayObligation {
    Connection(HGroupConnectionKind),
    Forced,
    Anxiety,
}

/// Convention knowledge attached to one card in the observer's hand.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HGroupCardInference {
    pub card: CardId,
    /// Convention-compatible physical identities for information-set and
    /// contingency analysis. This may include successful wrong-play outcomes.
    pub identities: IdentitySet,
    /// Exact identity promised by a deterministic connection step, whether the
    /// step is currently active or queued behind an earlier connector. Unlike
    /// `identities`, this records what the player must act as though the card
    /// is; a Finesse promised as yellow 1 remains yellow 1 even when the card
    /// could physically be another successful play.
    pub promised_identity: Option<Card>,
    pub identity_status: HGroupIdentityStatus,
    /// Whether this card is the focus of the latest completed clue action.
    /// Historical focus remains on `HGroupClueInterpretation`; this transient
    /// marker is cleared as soon as another action occurs.
    pub focused: bool,
    pub saved: bool,
    /// Whether this card is a deterministic member of a live Finesse chain.
    /// This remains true while the card is queued behind earlier connectors;
    /// `play_obligation` separately records when the card must act now.
    pub finessed: bool,
    pub play_obligation: Option<HGroupPlayObligation>,
}

/// H-Group-specific conclusions for the player owning the view.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HGroupInferences {
    pub clues: Vec<HGroupClueInterpretation>,
    /// Current chop for every player, in player order.
    pub chops: Vec<Option<CardId>>,
    /// Own cards promised playable now by an H-Group interpretation.
    pub playable_now: Vec<CardId>,
    /// Decision-facing order constraints derived by `ActionSchedule`. This is
    /// intentionally not reconstructed from historical signals by consumers.
    pub(crate) priority_plays: Vec<CardId>,
    /// Demonstrated connection steps and completed focuses are lifecycle
    /// projections owned by `ActionSchedule`, not deductions reconstructed
    /// from the signal log by action selection.
    pub(crate) demonstrated_connections: Vec<CardId>,
    pub(crate) completed_connection_focuses: Vec<CardId>,
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

impl HGroupInferences {
    #[must_use]
    pub fn is_saved(&self, card: CardId) -> bool {
        self.cards
            .iter()
            .any(|inference| inference.card == card && inference.saved)
    }

    pub fn saved_cards(&self) -> impl Iterator<Item = CardId> + '_ {
        self.cards
            .iter()
            .filter_map(|inference| inference.saved.then_some(inference.card))
    }

    /// Cards protected from ordinary chop by any recognized clue or movement.
    pub(super) fn gotten(&self) -> CardSet {
        self.clues
            .iter()
            .flat_map(|clue| core::iter::once(clue.focus).chain(clue.new_non_focus.iter().copied()))
            .chain(self.invisibly_clued.iter().copied())
            .chain(self.chop_moved.iter().copied())
            .collect()
    }
}

/// Canonical observer-relative convention facts produced by the history
/// reducer. Card facts may describe any hand visible in this perspective;
/// action selection must project them onto the acting observer's hand.
#[derive(Clone, Debug)]
pub(super) struct ConventionCardState {
    pub(super) explicitly_clued: ProvenancedCardSet,
    pub(super) invisibly_clued: ProvenancedCardSet,
    pub(super) already_playing: ProvenancedCardSet,
    pub(super) chop_moved: ProvenancedCardSet,
    pub(super) discard_now: Vec<CardId>,
    pub(super) forced_playable: ProvenancedCardSet,
    pub(super) invalidated_focuses: CardSet,
    /// Direct Play interpretations their owner publicly declined. This is an
    /// action-selection fact, not a rewrite of the underlying clue facts.
    pub(super) declined_direct_plays: CardSet,
    /// Incremental semantic facts and relational identity claims.
    pub(super) facts: ConventionFacts,
}

impl ConventionCardState {
    pub(super) fn validate(&self) -> Result<(), String> {
        self.facts.validate()?;
        self.explicitly_clued.validate()?;
        self.invisibly_clued.validate()?;
        self.already_playing.validate()?;
        self.chop_moved.validate()?;
        self.forced_playable.validate()?;
        let mut seen_discards = CardSet::default();
        if self
            .discard_now
            .iter()
            .any(|card| !seen_discards.insert(*card))
        {
            return Err("required discard is duplicated".to_owned());
        }
        Ok(())
    }
}

/// Canonical observer-relative convention state produced by the history reducer.
#[derive(Clone, Debug)]
pub(super) struct HGroupState {
    pub(super) hands: Vec<Vec<CardId>>,
    pub(super) cards: ConventionCardState,
    pub(super) clues: Vec<HGroupClueInterpretation>,
    pub(super) pending_connections: ConnectionManager,
    pub(super) early_game: bool,
    pub(super) signals: SignalHistory,
    pub(super) must_clue: PlayerSet,
    pub(super) implicit_saves: Vec<(CardId, IdentitySet)>,
    pub(super) required_fixes: FixObligations,
    pub(super) transitions: Vec<ConventionTransitionResult>,
    /// Canonical owner-relative epistemic program compiled once from this
    /// replay. Consumers reduce typed effects instead of reinterpreting clues.
    pub(super) knowledge: ConventionKnowledge,
}

impl HGroupState {
    /// A normal Gentleman's Discard gives the recipient an exact identity
    /// note. Level 25 therefore treats the transferred card as clued for
    /// Priority, not as an urgent blind-play that jumps ahead of every older
    /// play obligation.
    ///
    /// Source: <https://hanabi.github.io/level-25/#priority-with-blind-plays>
    pub(super) fn is_exact_transfer(&self, card: CardId, identity: Card) -> bool {
        self.cards.facts.is_exact_transfer(card, identity)
    }

    /// Cards eligible to act as Prompts. Chop movement alone is not a clue.
    pub(super) fn promptable(&self) -> CardSet {
        let active_invisible =
            active_invisibly_clued(&self.cards.invisibly_clued, &self.pending_connections);
        self.cards
            .explicitly_clued
            .union(&active_invisible)
            .copied()
            .collect()
    }

    /// Returns cards that are conventionally clued or moved for chop purposes.
    ///
    /// An ordered Finesse obligation keeps its conditional suffix reserved so
    /// the connection can advance after a wrong successful play. Those later
    /// candidates are not current Finesse Positions, however, and therefore
    /// do not remove the player's next unclued discard from chop.
    pub(super) fn gotten_from(&self, promptable: &CardSet) -> CardSet {
        let mut gotten = promptable.clone();
        gotten.extend(self.cards.chop_moved.iter().copied());
        let active_finesse_positions = self
            .pending_connections
            .iter()
            .filter(|connection| connection.kind == HGroupConnectionKind::Finesse)
            .filter_map(|connection| connection.cards.first().copied())
            .collect::<CardSet>();
        for card in &self.cards.invisibly_clued {
            let only_connection_sources = !self.cards.invisibly_clued.sources(*card).is_empty()
                && self
                    .cards
                    .invisibly_clued
                    .sources(*card)
                    .iter()
                    .all(|source| matches!(source, EffectSource::Promise(_)));
            if only_connection_sources
                && !active_finesse_positions.contains(card)
                && !self.cards.explicitly_clued.contains(card)
                && !self.cards.chop_moved.contains(card)
            {
                gotten.remove(card);
            }
        }
        gotten
    }

    pub(super) fn validate(&self) -> Result<(), String> {
        self.cards.validate()?;
        self.pending_connections.validate()?;
        self.validate_clue_hypotheses()?;
        for connection in self.pending_connections.iter() {
            if let Some(card) = connection
                .cards
                .iter()
                .find(|card| !self.hands[connection.actor.index()].contains(card))
            {
                return Err(format!(
                    "connection candidate {card:?} is not in actor {:?}'s hand {:?}: {connection:?}",
                    connection.actor,
                    self.hands[connection.actor.index()]
                ));
            }
        }
        for pair in self.transitions.windows(2) {
            if pair[0].turn > pair[1].turn {
                return Err("convention transitions are not in turn order".to_owned());
            }
        }
        for transition in &self.transitions {
            let mut prior_phase = None;
            for proposal in &transition.proposals {
                if prior_phase.is_some_and(|phase| phase > proposal.phase) {
                    return Err("rule proposals are not in semantic phase order".to_owned());
                }
                prior_phase = Some(proposal.phase);
                let Some(signals) = self.signals.get(proposal.signal_range.clone()) else {
                    return Err("rule proposal signal range is invalid".to_owned());
                };
                let Some(promises) = self
                    .pending_connections
                    .transitions()
                    .get(proposal.promise_transition_range.clone())
                else {
                    return Err("rule proposal promise range is invalid".to_owned());
                };
                if signals.iter().any(|signal| signal.turn != transition.turn)
                    || promises
                        .iter()
                        .any(|promise| promise.turn != transition.turn)
                {
                    return Err(format!(
                        "rule {:?} emitted an effect for a different turn",
                        proposal.rule
                    ));
                }
                if !signals.is_empty() && !proposal.mutations.contains(MutationDomain::CurrentFacts)
                {
                    return Err("signal proposal did not mark current facts changed".to_owned());
                }
                if proposal.is_empty() {
                    return Err("empty rule proposal was retained".to_owned());
                }
            }
        }
        for retraction in self
            .cards
            .invisibly_clued
            .retractions()
            .iter()
            .chain(self.cards.already_playing.retractions())
            .chain(self.cards.forced_playable.retractions())
        {
            let super::EffectSource::Promise(promise) = retraction.source else {
                return Err("non-promise lifecycle retraction was journaled".to_owned());
            };
            if self.pending_connections.provenance(promise).is_none() {
                return Err("retracted convention fact has no promise provenance".to_owned());
            }
        }
        self.validate_knowledge()
    }

    fn validate_clue_hypotheses(&self) -> Result<(), String> {
        for clue in &self.clues {
            let mut seen_identities = HashSet::new();
            for hypothesis in &clue.hypotheses {
                if !clue.play_identities.contains(hypothesis.focus_identity) {
                    return Err(format!(
                        "clue hypothesis {:?} is outside the clue's Play domain {:?}",
                        hypothesis.focus_identity, clue.play_identities
                    ));
                }
                if !seen_identities.insert(hypothesis.focus_identity) {
                    return Err("clue has duplicate identity hypotheses".to_owned());
                }
                if hypothesis
                    .connection_steps
                    .iter()
                    .any(|step| step.cards.is_empty())
                {
                    return Err("clue hypothesis has an empty connection step".to_owned());
                }
            }
            if seen_identities.len() != clue.play_identities.len() {
                return Err("clue hypotheses do not cover its Play domain".to_owned());
            }
        }
        for obligation in self.required_fixes.iter() {
            let FixCondition::FocusIdentity {
                clue_turn,
                focus,
                identity,
            } = obligation.condition
            else {
                continue;
            };
            let branch_exists = self.clues.iter().any(|clue| {
                clue.turn == clue_turn
                    && clue.focus == focus
                    && clue.hypotheses.iter().any(|hypothesis| {
                        hypothesis.focus_identity == identity
                            && hypothesis.required_fix == Some(obligation.required)
                    })
            });
            if !branch_exists {
                return Err("conditional Fix has no originating clue hypothesis".to_owned());
            }
        }
        Ok(())
    }

    fn validate_knowledge(&self) -> Result<(), String> {
        if self.knowledge.effects().iter().any(|effect| {
            !self
                .hands
                .iter()
                .flatten()
                .any(|card| *card == effect.card())
        }) {
            return Err(
                "owner-knowledge effect references a card outside the live hands".to_owned(),
            );
        }
        if self.knowledge.effects().iter().any(|effect| {
            matches!(
                effect,
                CardKnowledgeEffect::ReplaceDomain { source, .. }
                    if !matches!(source, KnowledgeSource::Reinterpretation(_))
            )
        }) {
            return Err("ordinary owner-knowledge inference widened an identity domain".to_owned());
        }
        let transition_effects = self
            .transitions
            .iter()
            .flat_map(|transition| transition.delta.knowledge_changes.iter())
            .collect::<Vec<_>>();
        if self.transitions.iter().any(|transition| {
            transition
                .delta
                .knowledge_changes
                .iter()
                .any(|effect| effect.source().turn() != transition.turn)
        }) {
            return Err(
                "owner-knowledge effect was attached to a non-causal transition".to_owned(),
            );
        }
        if transition_effects.len() != self.knowledge.effects().len()
            || self.knowledge.effects().iter().any(|effect| {
                transition_effects
                    .iter()
                    .filter(|candidate| ***candidate == *effect)
                    .count()
                    != 1
            })
        {
            return Err(
                "transition knowledge deltas do not partition canonical knowledge".to_owned(),
            );
        }
        Ok(())
    }
}

/// Materializes only the presently actionable member of each ambiguous
/// Finesse promise. Later cards in `connection.cards` are conditional suffixes:
/// they become the Finesse Position only after every earlier candidate has
/// produced a successful alternative play. Treating the whole suffix as
/// already clued corrupts later focus and Prompt selection.
pub(super) fn active_invisibly_clued(
    invisibly_clued: &ProvenancedCardSet,
    pending: &ConnectionManager,
) -> CardSet {
    invisibly_clued
        .iter()
        .copied()
        .filter(|card| {
            let sources = invisibly_clued.sources(*card);
            sources.is_empty()
                || sources
                    .iter()
                    .any(|source| !matches!(source, EffectSource::Promise(_)))
                || sources.iter().any(|source| {
                    let EffectSource::Promise(promise) = source else {
                        return false;
                    };
                    pending.iter().any(|connection| {
                        connection.promise == *promise
                            && connection.kind == HGroupConnectionKind::Finesse
                            && connection.cards.first() == Some(card)
                    })
                })
        })
        .collect()
}

pub(super) fn protected_cards(
    explicitly_clued: &CardSet,
    invisibly_clued: &CardSet,
    chop_moved: &CardSet,
) -> CardSet {
    explicitly_clued
        .union(invisibly_clued)
        .copied()
        .chain(chop_moved.iter().copied())
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RequiredFix {
    pub(super) actor: PlayerId,
    pub(super) target: PlayerId,
    pub(super) focus: CardId,
    pub(super) identity: Card,
}

/// The interpretation branch under which a repair is required.
///
/// A visible exact focus makes the repair unconditional. When the recipient
/// still has several clue identities in superposition, the repair remains
/// attached to the identity branch that created the lie instead of leaking
/// into every interpretation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FixCondition {
    Unconditional,
    FocusIdentity {
        clue_turn: u32,
        focus: CardId,
        identity: Card,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FixObligation {
    pub(super) required: RequiredFix,
    pub(super) condition: FixCondition,
}

/// Branch-aware repair obligations retained by the public-history reducer.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct FixObligations {
    entries: Vec<FixObligation>,
}

impl FixObligations {
    pub(super) fn insert_unconditional(&mut self, required: RequiredFix) {
        self.insert(FixObligation {
            required,
            condition: FixCondition::Unconditional,
        });
    }

    pub(super) fn insert_conditional(
        &mut self,
        clue_turn: u32,
        focus: CardId,
        identity: Card,
        required: RequiredFix,
    ) {
        self.insert(FixObligation {
            required,
            condition: FixCondition::FocusIdentity {
                clue_turn,
                focus,
                identity,
            },
        });
    }

    fn insert(&mut self, obligation: FixObligation) {
        if !self.entries.contains(&obligation) {
            self.entries.push(obligation);
        }
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = FixObligation> + '_ {
        self.entries.iter().copied()
    }

    pub(super) fn retain(&mut self, retain: impl FnMut(&FixObligation) -> bool) {
        self.entries.retain(retain);
    }
}
