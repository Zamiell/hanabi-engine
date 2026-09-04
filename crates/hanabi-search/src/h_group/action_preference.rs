use crate::ConventionPolicyTier;

/// Structured comparison key for convention actions. Numeric priority remains
/// available for planner diagnostics, but semantic categories are compared
/// explicitly by H-Group action selection.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct ActionPreference {
    policy_tier: ConventionPolicyTier,
    advances_terminal_plan: bool,
    within_category: i32,
}

impl ActionPreference {
    pub(super) const fn new(within_category: i32, advances_terminal_plan: bool) -> Self {
        Self {
            policy_tier: ConventionPolicyTier::Admitted,
            advances_terminal_plan,
            within_category,
        }
    }

    pub(super) const fn set_policy_tier(&mut self, policy_tier: ConventionPolicyTier) {
        self.policy_tier = policy_tier;
    }
}

/// Named components of the legacy scalar exported to the generic planner.
/// Keeping this encoding at the boundary prevents decision rules from adding
/// unrelated score constants inline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TerminalPlanProgress {
    discard_threshold: i32,
    clue_value: i32,
}

impl TerminalPlanProgress {
    pub(super) const fn new(discard_threshold: i32, clue_value: i32) -> Self {
        Self {
            discard_threshold,
            clue_value,
        }
    }

    pub(super) const fn encoded_priority(self) -> i32 {
        101 + self.discard_threshold + self.clue_value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_plan_progress_is_a_distinct_comparison_dimension() {
        assert!(ActionPreference::new(1, true) > ActionPreference::new(10_000, false));
    }
}
