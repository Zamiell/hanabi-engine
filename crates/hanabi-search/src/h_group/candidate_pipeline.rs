use super::{
    ClueCandidate, HGroupProfile, LogicalDeductions, apply_strategic_clue_values,
    recipient_replay_assessment,
};

/// Candidates that have passed legality, focus, semantic-admissibility, and
/// convention-safety checks in the interpretation layer.
pub(super) struct SemanticallyAdmittedCandidates(Vec<ClueCandidate>);

/// Candidates whose proposed meaning has an explicit recipient assessment.
/// Generator-only candidates remain visible as correlated hidden-world
/// branches; they are no longer mislabeled as recipient-confirmed.
struct RecipientAssessedCandidates(Vec<ClueCandidate>);

/// Candidates after structured causal outcomes and strategic preferences
/// have been compared.
struct RankedCandidates(Vec<ClueCandidate>);

impl SemanticallyAdmittedCandidates {
    pub(super) fn new(candidates: Vec<ClueCandidate>) -> Self {
        Self(candidates)
    }

    fn check_recipient(
        mut self,
        deductions: &LogicalDeductions,
        profile: HGroupProfile,
    ) -> RecipientAssessedCandidates {
        let view = deductions.view();
        for candidate in &mut self.0 {
            candidate.set_recognition(recipient_replay_assessment(view, profile, candidate));
        }
        RecipientAssessedCandidates(self.0)
    }

    pub(super) fn finalize(
        self,
        deductions: &LogicalDeductions,
        profile: HGroupProfile,
    ) -> Vec<ClueCandidate> {
        self.check_recipient(deductions, profile)
            .compare_outcomes(deductions, profile)
            .into_vec()
    }
}

impl RecipientAssessedCandidates {
    fn compare_outcomes(
        mut self,
        deductions: &LogicalDeductions,
        profile: HGroupProfile,
    ) -> RankedCandidates {
        apply_strategic_clue_values(deductions, profile, &mut self.0);
        RankedCandidates(self.0)
    }
}

impl RankedCandidates {
    fn into_vec(self) -> Vec<ClueCandidate> {
        self.0
    }
}
