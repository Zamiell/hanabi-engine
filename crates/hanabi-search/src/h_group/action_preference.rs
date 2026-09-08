use crate::ConventionPolicyTier;

/// Structured comparison key for convention actions. Numeric priority remains
/// available for planner diagnostics, but semantic categories are compared
/// explicitly by H-Group action selection.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ActionPreference {
    policy_tier: ConventionPolicyTier,
    advances_terminal_plan: bool,
    within_category: i32,
}

impl ActionPreference {
    pub(crate) const fn new(within_category: i32, advances_terminal_plan: bool) -> Self {
        Self {
            policy_tier: ConventionPolicyTier::Admitted,
            advances_terminal_plan,
            within_category,
        }
    }

    pub(crate) const fn set_policy_tier(&mut self, policy_tier: ConventionPolicyTier) {
        self.policy_tier = policy_tier;
    }

    #[must_use]
    pub const fn advances_terminal_plan(self) -> bool {
        self.advances_terminal_plan
    }

    #[must_use]
    pub const fn within_category(self) -> i32 {
        self.within_category
    }
}

/// Named terminal-progress components. The scalar encoding remains available
/// for diagnostics; decision ordering consumes the structured preference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TerminalPlanProgress {
    discard_threshold: i32,
    clue_value: i32,
}

impl TerminalPlanProgress {
    pub(super) const fn within_category(self) -> i32 {
        100 + self.clue_value
    }

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
