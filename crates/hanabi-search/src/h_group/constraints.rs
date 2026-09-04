use hanabi_core::Action;

/// Why convention semantics restricted the set that strategy may score.
/// These are hard obligations, not heuristic score adjustments.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConventionRequirementKind {
    UrgentProtection,
    ConnectionResponse,
    RequiredDiscard,
    MustClue,
    EarlyFiveStall,
}

/// One typed convention requirement and all semantically equivalent ways to
/// satisfy it. Strategy may rank alternatives inside this set but cannot
/// compare an unrelated higher-scored action against it.
#[derive(Clone, Debug)]
pub(super) struct ConventionRequirement {
    kind: ConventionRequirementKind,
    alternatives: Vec<Action>,
}

/// Hard convention obligations, kept separate from heuristic utility.
///
/// An empty required set means every legal action is admissible. Once a rule
/// supplies requirements, numeric strategy may rank only those actions.
#[derive(Clone, Debug, Default)]
pub(super) struct ConventionConstraints {
    requirement: Option<ConventionRequirement>,
}

impl ConventionConstraints {
    pub(super) fn require(
        kind: ConventionRequirementKind,
        actions: impl IntoIterator<Item = Action>,
    ) -> Self {
        let mut alternatives = Vec::new();
        for action in actions {
            if !alternatives.contains(&action) {
                alternatives.push(action);
            }
        }
        Self {
            requirement: (!alternatives.is_empty())
                .then_some(ConventionRequirement { kind, alternatives }),
        }
    }

    pub(super) fn allows(&self, action: Action) -> bool {
        self.requirement
            .as_ref()
            .is_none_or(|requirement| requirement.alternatives.contains(&action))
    }

    pub(super) fn kind(&self) -> Option<ConventionRequirementKind> {
        self.requirement
            .as_ref()
            .map(|requirement| requirement.kind)
    }

    /// Returns the action when convention semantics leave exactly one legal
    /// response. Planners must treat that response as forced rather than
    /// allowing a higher numeric heuristic to reintroduce excluded actions.
    pub(super) fn single_required(&self) -> Option<Action> {
        self.requirement
            .as_ref()?
            .alternatives
            .first()
            .copied()
            .filter(|_| {
                self.requirement
                    .as_ref()
                    .is_some_and(|requirement| requirement.alternatives.len() == 1)
            })
    }
}

#[cfg(test)]
mod tests {
    use hanabi_core::CardId;

    use super::*;

    #[test]
    fn unavailable_obligation_does_not_forbid_every_emergency_action() {
        let constraints = ConventionConstraints::require(ConventionRequirementKind::MustClue, []);
        assert!(constraints.allows(Action::Discard(CardId::new(1))));
        assert_eq!(constraints.kind(), None);
    }

    #[test]
    fn hard_requirement_excludes_a_higher_scored_unrelated_action() {
        let required = Action::Play(CardId::new(1));
        let constraints = ConventionConstraints::require(
            ConventionRequirementKind::ConnectionResponse,
            [required],
        );
        assert!(constraints.allows(required));
        assert!(!constraints.allows(Action::Discard(CardId::new(2))));
        assert_eq!(
            constraints.kind(),
            Some(ConventionRequirementKind::ConnectionResponse)
        );
    }
}
