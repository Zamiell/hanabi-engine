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
    GeneratorProof,
    RecipientReplay,
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
    pub(super) urgent_save: bool,
    pub(super) immediate_play: bool,
    pub(super) recognition: ClueRecognition,
}

impl ClueCandidate {
    pub(super) fn score(self) -> u16 {
        self.value.total()
    }

    /// A save important enough to preempt a convention-promised play.
    pub(super) fn is_urgent_save(self) -> bool {
        self.urgent_save && self.score() >= 400
    }

    /// A non-immediate setup clue that may interrupt a demonstrated layer.
    pub(super) fn can_defer_demonstrated_layer(self) -> bool {
        // Comparative penalties order otherwise-valid clues; they must not
        // retroactively make a semantically strong setup clue illegal while
        // a demonstrated connection is parked.
        !self.save && !self.immediate_play && self.value.semantic_strength() >= 365
    }

    /// A time-sensitive clue to the next player that may preempt occupancy.
    pub(super) fn is_urgent_for_next_player(self) -> bool {
        self.score() >= 450 && (self.urgent_save || self.immediate_play)
    }

    pub(super) fn mark_recipient_replay(&mut self) {
        self.recognition = ClueRecognition::RecipientReplay;
    }
}
