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

use super::HGroupMoveKind;

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

/// Canonical observer-relative convention state produced by the history reducer.
#[derive(Clone, Debug)]
pub(super) struct HGroupState {
    pub(super) hands: Vec<Vec<CardId>>,
    pub(super) explicitly_clued: CardSet,
    pub(super) invisibly_clued: CardSet,
    pub(super) clues: Vec<HGroupClueInterpretation>,
    pub(super) pending_connections: Vec<ConnectionObligation>,
    pub(super) already_playing: CardSet,
    pub(super) early_game: bool,
    pub(super) signals: Vec<HGroupSignal>,
    pub(super) chop_moved: CardSet,
    pub(super) discard_now: Vec<CardId>,
    pub(super) must_clue: PlayerSet,
    pub(super) forced_playable: CardSet,
    /// Play-clue focuses whose Prompt/Finesse chain was disproved by a
    /// successful play of the wrong connector identity.
    pub(super) invalidated_focuses: CardSet,
    pub(super) implicit_saves: Vec<(CardId, IdentitySet)>,
    pub(super) required_fix: Option<RequiredFix>,
}

impl HGroupState {
    /// Cards eligible to act as Prompts. Chop movement alone is not a clue.
    pub(super) fn promptable(&self) -> CardSet {
        self.explicitly_clued
            .union(&self.invisibly_clued)
            .copied()
            .collect()
    }

    /// Extends one Prompt set with permanent chop movement for chop purposes.
    pub(super) fn gotten_from(&self, promptable: &CardSet) -> CardSet {
        let mut gotten = promptable.clone();
        gotten.extend(self.chop_moved.iter().copied());
        gotten
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

#[derive(Clone, Copy, Debug)]
pub(super) struct RequiredFix {
    pub(super) actor: PlayerId,
    pub(super) target: PlayerId,
    pub(super) focus: CardId,
    pub(super) identity: Card,
}

/// One active, typed step in a Prompt or Finesse chain.
#[derive(Clone, Debug)]
pub(super) struct ConnectionObligation {
    pub(super) actor: PlayerId,
    pub(super) cards: Vec<CardId>,
    pub(super) expected: Card,
    pub(super) kind: HGroupConnectionKind,
    pub(super) focus: CardId,
    /// Zero-based position in a multi-connection chain.
    pub(super) step: u8,
}
