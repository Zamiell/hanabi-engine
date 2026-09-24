//! The only construction boundary for convention-admitted clues.
use super::{
    ClueProposal, HGroupProfile, HGroupState, LogicalDeductions, RejectedConventionAction,
    apply_strategic_clue_values, recipient_replay_assessment,
};
use std::ops::Deref;

/// Immutable admitted action. Generators can construct only `ClueProposal`.
#[derive(Clone, Copy, Debug)]
pub(super) struct CompiledClueAction {
    proposal: ClueProposal,
    checks: [crate::CluePrincipleCheck; 3],
}

impl Deref for CompiledClueAction {
    type Target = ClueProposal;
    fn deref(&self) -> &Self::Target {
        &self.proposal
    }
}

impl CompiledClueAction {
    pub(super) fn explanation(self) -> crate::ClueExplanation {
        let mut explanation = self.proposal.explanation();
        explanation.validation = self.checks.to_vec();
        explanation
    }
    pub(super) fn conditional(self) -> bool {
        self.checks
            .iter()
            .any(|check| check.verdict == crate::PrincipleVerdict::Unresolved)
    }
    pub(super) const fn checks(self) -> [crate::CluePrincipleCheck; 3] {
        self.checks
    }
    #[cfg(test)]
    pub(super) const fn proposal(self) -> ClueProposal {
        self.proposal
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct ClueCompilation {
    pub(super) admitted: Vec<CompiledClueAction>,
    pub(super) rejected: Vec<RejectedConventionAction>,
}

pub(super) fn compile(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    replay: &HGroupState,
) -> ClueCompilation {
    let proposals = super::interpretation::h_group_clue_candidates_from_replay_inner(
        deductions, profile, replay,
    );
    let mut compilation = validate_proposals(deductions, profile, replay, proposals);
    if compilation.admitted.is_empty() {
        let fallback = super::interpretation::fallback_clue_proposals(deductions, profile, replay);
        let extra = validate_proposals(deductions, profile, replay, fallback);
        compilation.admitted.extend(extra.admitted);
        compilation.rejected.extend(extra.rejected);
    }
    compilation
}

fn validate_proposals(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    replay: &HGroupState,
    proposals: Vec<ClueProposal>,
) -> ClueCompilation {
    let mut retained = Vec::new();
    let mut evidence = Vec::new();
    let mut rejected = Vec::new();
    for mut proposal in proposals {
        assert!(proposal.validate().is_ok(), "malformed clue proposal");
        let outcome =
            super::clue_outcome::scheduled_clue_outcome(deductions.view(), profile, &proposal);
        let validation = super::principle_validation::validate(
            deductions,
            profile,
            replay,
            &proposal,
            outcome.as_ref(),
        );
        if let Some(reason) = validation.rejection {
            rejected.push(RejectedConventionAction {
                action: proposal.action,
                reason,
                validation: Some(validation.checks),
            });
        } else {
            proposal.set_recognition(recipient_replay_assessment(
                deductions.view(),
                profile,
                &proposal,
            ));
            if let Some(outcome) = &outcome {
                proposal.set_compiled_line(outcome);
            }
            if let Some(index) = retained
                .iter()
                .position(|old: &ClueProposal| old.action == proposal.action)
            {
                let bluff = proposal.move_kind() == Some(super::HGroupMoveKind::Bluff)
                    && validation.checks[2].verdict == crate::PrincipleVerdict::Pass;
                if super::interpretation_resolution::candidate_replaces(
                    retained[index],
                    proposal,
                    bluff,
                ) {
                    retained[index] = proposal;
                    evidence[index] = validation.checks;
                }
            } else {
                retained.push(proposal);
                evidence.push(validation.checks);
            }
        }
    }
    // Stall precedence is applied only after admission: an invalid proposed
    // alternative cannot suppress the only lawful Burn.
    if retained
        .iter()
        .any(|proposal| proposal.move_kind() != Some(super::HGroupMoveKind::Burn))
    {
        let keep = retained
            .iter()
            .map(|proposal| proposal.move_kind() != Some(super::HGroupMoveKind::Burn))
            .collect::<Vec<_>>();
        let mut index = 0;
        retained.retain(|_| {
            let result = keep[index];
            index += 1;
            result
        });
        let mut index = 0;
        evidence.retain(|_| {
            let result = keep[index];
            index += 1;
            result
        });
    }
    apply_strategic_clue_values(deductions, profile, &mut retained);
    ClueCompilation {
        admitted: retained
            .into_iter()
            .zip(evidence)
            .map(|(proposal, checks)| CompiledClueAction { proposal, checks })
            .collect(),
        rejected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hanabi_core::{Action, Card, CardId, Clue, PlayerId, Rank, Suit};

    #[test]
    fn proposed_bluff_must_prove_its_declared_response() {
        // Counterfactual from recorded p4v0s1 turn 3, not an optimal-move oracle:
        // blue to Bob, 1s to Alice, y1, b1. Unknown draws remain blank.
        let fixture = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s1.json"
        ))
        .unwrap();
        let state = fixture.state_at_turn(2).unwrap();
        let mut view = state.view_for(PlayerId::new(2)).unwrap();
        for (giver, target, clue, touched) in [
            (
                2,
                1,
                Clue::Suit(Suit::Blue),
                vec![CardId::new(4), CardId::new(6)],
            ),
            (3, 0, Clue::Rank(Rank::One), vec![CardId::new(1)]),
        ] {
            view = super::super::ProspectiveTransition::clue_by(
                &view,
                PlayerId::new(giver),
                PlayerId::new(target),
                clue,
                &touched,
            );
        }
        for (actor, card, identity) in [
            (0, 1, Card::new(Suit::Yellow, Rank::One)),
            (1, 4, Card::new(Suit::Blue, Rank::One)),
        ] {
            view = super::super::ProspectiveTransition::successful_play(
                &view,
                PlayerId::new(actor),
                CardId::new(card),
                identity,
            );
        }
        let (d, replay) = super::super::PerspectiveProjector::new(&view, HGroupProfile::Max)
            .project(
                view.current_player,
                super::super::PerspectiveDepth::NestedRecipients,
            )
            .unwrap();
        let purple = Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Purple),
        };
        // Inject a proposal to test the boundary itself. Generators may stop
        // proposing this meaning, but a future generator must not bypass proof.
        let mut proposal = ClueProposal::new(
            purple,
            Some(super::super::HGroupMoveKind::Bluff),
            super::super::ClueValue::new(334),
            super::super::CluePurpose::Advanced,
            super::super::ClueSchedule::new(false, false),
            0,
        );
        proposal.required_response = Some((PlayerId::new(3), CardId::new(12)));
        let batch =
            super::super::with_prospective_analysis_cache(d.view(), HGroupProfile::Max, || {
                validate_proposals(&d, HGroupProfile::Max, &replay, vec![proposal])
            });
        assert!(
            batch
                .rejected
                .iter()
                .any(|rejected| rejected.action == purple
                    && rejected.reason == crate::ConventionRejectionReason::UnprovenResponse),
            "{batch:#?}"
        );
        assert!(
            !batch
                .admitted
                .iter()
                .any(|candidate| candidate.action == purple
                    && candidate.move_kind() == Some(super::super::HGroupMoveKind::Bluff))
        );
        // The actual finesse-position card is due in inference, but the
        // existing response policy passes it back to avoid misleading Bob.
        proposal.required_response = Some((PlayerId::new(3), CardId::new(15)));
        let passed_back = validate_proposals(&d, HGroupProfile::Max, &replay, vec![proposal]);
        assert!(
            passed_back
                .rejected
                .iter()
                .any(|r| r.reason == crate::ConventionRejectionReason::UnprovenResponse),
            "{passed_back:#?}"
        );
    }
}
