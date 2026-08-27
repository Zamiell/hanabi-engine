use std::collections::HashSet;
use std::hash::{BuildHasherDefault, Hasher};

use hanabi_core::{Card, CardId, Clue, PlayerId};

use crate::IdentitySet;

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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

/// Convention knowledge attached to one card in the observer's hand.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HGroupCardInference {
    pub card: CardId,
    /// Convention-compatible physical identities for information-set and
    /// contingency analysis. This may include successful wrong-play outcomes.
    pub identities: IdentitySet,
    /// Exact identity promised by the card's currently active connection.
    /// Unlike `identities`, this records what the player must act as though the
    /// card is; a Finesse promised as yellow 1 remains yellow 1 even when the
    /// card could physically be another successful play.
    pub promised_identity: Option<Card>,
    pub identity_status: HGroupIdentityStatus,
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

impl HGroupInferences {
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
    pub(super) required_fix: Option<RequiredFix>,
    pub(super) transitions: Vec<ConventionTransitionResult>,
}

impl HGroupState {
    /// Cards eligible to act as Prompts. Chop movement alone is not a clue.
    pub(super) fn promptable(&self) -> CardSet {
        self.cards
            .explicitly_clued
            .union(&self.cards.invisibly_clued)
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
        Ok(())
    }
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
