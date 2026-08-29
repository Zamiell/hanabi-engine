use hanabi_core::{Action, PlayerId};

/// The primary convention role of an admitted clue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CluePurpose {
    Fix,
    Play,
    Save,
    Advanced,
    Tempo,
    Fallback,
}

/// Evidence supporting prospective admission. Some clues have meaning that
/// legitimately branches on the giver's hidden hand once the recipient sees
/// it; those retain their focused construction proof instead of pretending a
/// single unresolved projection can assign one universal label.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ClueRecognition {
    /// The semantic generator proved the clue, but nested recipient replay
    /// could not assign one universal interpretation across hidden worlds.
    GeneratorProof,
    /// The recipient's canonical replay independently reconstructed it.
    RecipientReplay,
}

/// Scheduling consequences kept together instead of accumulating unrelated
/// booleans on `ClueCandidate`.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ClueSchedule {
    urgent_save: bool,
    immediate_play: bool,
    preserves_visible_continuation: bool,
}

impl ClueSchedule {
    pub(super) const fn new(urgent_save: bool, immediate_play: bool) -> Self {
        Self {
            urgent_save,
            immediate_play,
            preserves_visible_continuation: false,
        }
    }
}

/// Named components of clue utility.
///
/// The total deliberately reproduces the previous integer ordering while the
/// engine migrates comparisons onto semantic outcomes. Keeping adjustments in
/// separate fields prevents an information or directness rule from silently
/// overwriting an unrelated teamwork adjustment.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ClueValue {
    base: u16,
    information: u16,
    teamwork_bonus: u16,
    teamwork_penalty: u16,
    delay_penalty: u16,
    indirectness_penalty: u16,
}

impl ClueValue {
    pub(super) const fn new(base: u16) -> Self {
        Self {
            base,
            information: 0,
            teamwork_bonus: 0,
            teamwork_penalty: 0,
            delay_penalty: 0,
            indirectness_penalty: 0,
        }
    }

    pub(super) fn total(self) -> u16 {
        self.base
            .saturating_add(self.information)
            .saturating_add(self.teamwork_bonus)
            .saturating_sub(self.teamwork_penalty)
            .saturating_sub(self.delay_penalty)
            .saturating_sub(self.indirectness_penalty)
    }

    pub(super) fn semantic_strength(self) -> u16 {
        self.base
            .saturating_add(self.information)
            .saturating_add(self.teamwork_bonus)
    }

    pub(super) fn add_information(&mut self, value: u16) {
        self.information = self.information.saturating_add(value);
    }

    pub(super) const fn set_base(&mut self, value: u16) {
        self.base = value;
    }

    pub(super) fn penalize_teamwork(&mut self, value: u16) {
        self.teamwork_penalty = self.teamwork_penalty.saturating_add(value);
    }

    pub(super) fn reward_teamwork(&mut self, value: u16) {
        self.teamwork_bonus = self.teamwork_bonus.saturating_add(value);
    }

    pub(super) fn penalize_delay(&mut self, value: u16) {
        self.delay_penalty = self.delay_penalty.saturating_add(value);
    }

    pub(super) fn penalize_indirectness(&mut self, value: u16) {
        self.indirectness_penalty = self.indirectness_penalty.saturating_add(value);
    }
}

/// Convention-valid clue plus its semantic role and structured value.
#[derive(Clone, Copy, Debug)]
pub(super) struct ClueCandidate {
    pub(super) action: Action,
    pub(super) value: ClueValue,
    pub(super) purpose: CluePurpose,
    pub(super) target: PlayerId,
    pub(super) save: bool,
    pub(super) schedule: ClueSchedule,
    pub(super) connection_steps: u8,
    pub(super) action_coverage: u8,
    /// Total cards secured by the canonical named convention line.
    pub(super) convention_action_count: Option<u8>,
    /// Blind plays required by that named line, including off-suit layers.
    pub(super) convention_connection_steps: Option<u8>,
    /// Giving this clue now avoids forcing an occupied teammate to delay a
    /// play that unlocks a stronger visible continuation than the giver's
    /// currently available play.
    pub(super) recognition: ClueRecognition,
}

impl ClueCandidate {
    pub(super) fn score(self) -> u16 {
        self.value.total()
    }

    /// A save important enough to preempt a convention-promised play.
    pub(super) fn is_urgent_save(self) -> bool {
        // Comparative Teamwork and delay penalties order otherwise-valid
        // clues; they must not erase the semantic urgency of a critical Save.
        self.schedule.urgent_save && self.value.semantic_strength() >= 400
    }

    /// A non-immediate setup clue that may interrupt a demonstrated layer.
    pub(super) fn can_defer_demonstrated_layer(self) -> bool {
        // Comparative penalties order otherwise-valid clues; they must not
        // retroactively make a semantically strong setup clue illegal while
        // a demonstrated connection is parked.
        !self.save && !self.schedule.immediate_play && self.value.semantic_strength() >= 365
    }

    /// A long connection line may park an ordinary play while it advances at
    /// least two blind-play steps. Immediate multi-action clues have a
    /// narrower exception for exact transferred plays, evaluated with the
    /// current play obligation in `decision`.
    pub(super) fn can_preempt_ordinary_play(self) -> bool {
        (self.purpose == CluePurpose::Play && self.connection_steps >= 2)
            || self.schedule.preserves_visible_continuation
    }

    /// A time-sensitive clue to the next player that may preempt occupancy.
    pub(super) fn is_urgent_for_next_player(self) -> bool {
        self.score() >= 450 && (self.schedule.urgent_save || self.schedule.immediate_play)
    }

    pub(super) const fn set_recognition(&mut self, recognition: ClueRecognition) {
        self.recognition = recognition;
    }

    pub(super) const fn immediate_play(self) -> bool {
        self.schedule.immediate_play
    }

    pub(super) const fn preserves_visible_continuation(self) -> bool {
        self.schedule.preserves_visible_continuation
    }

    pub(super) const fn set_preserves_visible_continuation(&mut self, preserves: bool) {
        self.schedule.preserves_visible_continuation = preserves;
    }
}
