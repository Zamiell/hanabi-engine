use hanabi_core::Action;

/// Why convention semantics restricted the set that strategy may score.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConstraintReason {
    UrgentClue,
    ConnectionResponse,
    RequiredDiscard,
    MustClue,
}

/// Hard convention obligations, kept separate from heuristic utility.
///
/// An empty required set means every legal action is admissible. Once a rule
/// supplies requirements, numeric strategy may rank only those actions.
#[derive(Clone, Debug, Default)]
pub(super) struct ConventionConstraints {
    required: Vec<Action>,
    reason: Option<ConstraintReason>,
}

impl ConventionConstraints {
    pub(super) fn require(
        reason: ConstraintReason,
        actions: impl IntoIterator<Item = Action>,
    ) -> Self {
        let mut required = Vec::new();
        for action in actions {
            if !required.contains(&action) {
                required.push(action);
            }
        }
        Self {
            required,
            reason: Some(reason),
        }
    }

    pub(super) fn allows(&self, action: Action) -> bool {
        self.required.is_empty() || self.required.contains(&action)
    }

    pub(super) const fn reason(&self) -> Option<ConstraintReason> {
        self.reason
    }
}

#[cfg(test)]
mod tests {
    use hanabi_core::CardId;

    use super::*;

    #[test]
    fn hard_requirement_excludes_a_higher_scored_unrelated_action() {
        let required = Action::Play(CardId::new(1));
        let constraints =
            ConventionConstraints::require(ConstraintReason::ConnectionResponse, [required]);
        assert!(constraints.allows(required));
        assert!(!constraints.allows(Action::Discard(CardId::new(2))));
        assert_eq!(
            constraints.reason(),
            Some(ConstraintReason::ConnectionResponse)
        );
    }
}
