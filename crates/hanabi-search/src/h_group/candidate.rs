use hanabi_core::{Action, PlayerId};

use super::HGroupMoveKind;

/// The primary convention role of an admitted clue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CluePurpose {
    Fix,
    Play,
    Save,
    Advanced,
    Tempo,
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
/// booleans on `CompiledClueAction`.
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
/// separate fields prevents an information or clarity rule from silently
/// overwriting an unrelated teamwork adjustment.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ClueValue {
    base: u16,
    information: u16,
    teamwork_bonus: u16,
    teamwork_penalty: u16,
    delay_penalty: u16,
    complexity_penalty: u16,
}

impl ClueValue {
    pub(super) const fn new(base: u16) -> Self {
        Self {
            base,
            information: 0,
            teamwork_bonus: 0,
            teamwork_penalty: 0,
            delay_penalty: 0,
            complexity_penalty: 0,
        }
    }

    pub(super) fn total(self) -> u16 {
        self.base
            .saturating_add(self.information)
            .saturating_add(self.teamwork_bonus)
            .saturating_sub(self.teamwork_penalty)
            .saturating_sub(self.delay_penalty)
            .saturating_sub(self.complexity_penalty)
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

    pub(super) fn penalize_complexity(&mut self, value: u16) {
        self.complexity_penalty = self.complexity_penalty.saturating_add(value);
    }
}

/// Shape of the deterministic line established by a clue. These values are
/// compiled once from the recipient-relative projection and then consumed by
/// policy and strategy; downstream code must not replay the clue to recover
/// them independently.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct CompiledClueLine {
    connection_steps: u8,
    action_coverage: u8,
    convention_action_count: Option<u8>,
    convention_connection_steps: Option<u8>,
}

/// Canonical semantic meaning attached to one admitted clue.
#[derive(Clone, Copy, Debug)]
pub(super) struct CompiledClueSemantics {
    move_kind: Option<HGroupMoveKind>,
    purpose: CluePurpose,
    schedule: ClueSchedule,
    line: CompiledClueLine,
}

/// Convention-valid clue plus the one compiled semantic result consumed by
/// recipient validation, policy, strategy, and planning.
#[derive(Clone, Copy, Debug)]
pub(super) struct CompiledClueAction {
    pub(super) action: Action,
    semantics: CompiledClueSemantics,
    pub(super) value: ClueValue,
    recognition: ClueRecognition,
}

impl CompiledClueAction {
    pub(super) const fn new(
        action: Action,
        move_kind: Option<HGroupMoveKind>,
        value: ClueValue,
        purpose: CluePurpose,
        schedule: ClueSchedule,
        connection_steps: u8,
    ) -> Self {
        debug_assert!(matches!(action, Action::Clue { .. }));
        Self {
            action,
            semantics: CompiledClueSemantics {
                move_kind,
                purpose,
                schedule,
                line: CompiledClueLine {
                    connection_steps,
                    action_coverage: 0,
                    convention_action_count: None,
                    convention_connection_steps: None,
                },
            },
            value,
            recognition: ClueRecognition::GeneratorProof,
        }
    }

    pub(super) fn score(self) -> u16 {
        self.value.total()
    }

    /// Internal consistency check for the compiled semantic boundary. Target
    /// and Save status are derived rather than stored, so contradictory
    /// candidates cannot cross into policy or planning unnoticed.
    pub(super) fn validate(&self) -> Result<(), &'static str> {
        if !matches!(self.action, Action::Clue { .. }) {
            return Err("compiled clue action contains a non-clue action");
        }
        match self.semantics.purpose {
            CluePurpose::Fix if self.semantics.move_kind != Some(HGroupMoveKind::FixClue) => {
                Err("Fix purpose lacks FixClue semantics")
            }
            CluePurpose::Tempo if self.semantics.move_kind != Some(HGroupMoveKind::TempoClue) => {
                Err("Tempo purpose lacks TempoClue semantics")
            }
            _ => Ok(()),
        }
    }

    pub(super) const fn move_kind(self) -> Option<HGroupMoveKind> {
        self.semantics.move_kind
    }

    pub(super) const fn purpose(self) -> CluePurpose {
        self.semantics.purpose
    }

    pub(super) fn target(self) -> PlayerId {
        match self.action {
            Action::Clue { target, .. } => target,
            Action::Play(_) | Action::Discard(_) => {
                unreachable!("compiled clue action must contain a clue")
            }
        }
    }

    pub(super) const fn is_save(self) -> bool {
        matches!(self.semantics.purpose, CluePurpose::Save)
    }

    pub(super) const fn connection_steps(self) -> u8 {
        self.semantics.line.connection_steps
    }

    pub(super) const fn action_coverage(self) -> u8 {
        self.semantics.line.action_coverage
    }

    #[cfg(test)]
    pub(super) const fn convention_action_count(self) -> Option<u8> {
        self.semantics.line.convention_action_count
    }

    #[cfg(test)]
    pub(super) const fn convention_connection_steps(self) -> Option<u8> {
        self.semantics.line.convention_connection_steps
    }

    pub(super) fn set_compiled_line(&mut self, outcome: &super::LineOutcome) {
        self.semantics.line.action_coverage =
            u8::try_from(outcome.action_coverage).unwrap_or(u8::MAX);
        self.semantics.line.convention_action_count = outcome
            .convention_action_count
            .map(|count| u8::try_from(count).unwrap_or(u8::MAX));
        self.semantics.line.convention_connection_steps = outcome
            .convention_connection_steps
            .map(|count| u8::try_from(count).unwrap_or(u8::MAX));
    }

    #[cfg(test)]
    pub(super) const fn recognition(self) -> ClueRecognition {
        self.recognition
    }

    /// A save important enough to preempt a convention-promised play.
    pub(super) fn is_urgent_save(self) -> bool {
        // Comparative Teamwork and delay penalties order otherwise-valid
        // clues; they must not erase the semantic urgency of a critical Save.
        self.semantics.schedule.urgent_save && self.value.semantic_strength() >= 400
    }

    /// A non-immediate setup clue that may interrupt a demonstrated layer.
    pub(super) fn can_defer_demonstrated_layer(self) -> bool {
        // Comparative penalties order otherwise-valid clues; they must not
        // retroactively make a semantically strong setup clue illegal while
        // a demonstrated connection is parked.
        !self.is_save()
            && !self.semantics.schedule.immediate_play
            && self.value.semantic_strength() >= 365
    }

    /// A long connection line may park an ordinary play while it advances at
    /// least two blind-play steps. Immediate multi-action clues have a
    /// narrower exception for exact transferred plays, evaluated with the
    /// current play obligation in `decision`.
    pub(super) fn can_preempt_ordinary_play(self) -> bool {
        (self.purpose() == CluePurpose::Play
            && self.connection_steps() >= 2
            && self.action_coverage() >= 2)
            || self.semantics.schedule.preserves_visible_continuation
    }

    /// A time-sensitive clue to the next player that may preempt occupancy.
    pub(super) fn is_urgent_for_next_player(self) -> bool {
        self.score() >= 450
            && (self.semantics.schedule.urgent_save || self.semantics.schedule.immediate_play)
    }

    pub(super) const fn set_recognition(&mut self, recognition: ClueRecognition) {
        self.recognition = recognition;
    }

    pub(super) const fn immediate_play(self) -> bool {
        self.semantics.schedule.immediate_play
    }

    pub(super) const fn preserves_visible_continuation(self) -> bool {
        self.semantics.schedule.preserves_visible_continuation
    }

    pub(super) const fn set_preserves_visible_continuation(&mut self, preserves: bool) {
        self.semantics.schedule.preserves_visible_continuation = preserves;
    }
}
