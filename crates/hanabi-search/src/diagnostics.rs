use std::time::Duration;

/// Work counters and timing collected during one search invocation.
///
/// `tree_time` is the portion of `total_time` not spent sampling hidden worlds
/// or executing terminal rollouts. For flat Monte Carlo it includes root
/// candidate cloning, action application, and result aggregation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SearchDiagnostics {
    /// Root-consistent hidden worlds successfully sampled.
    pub worlds_sampled: u64,
    /// Explicit authoritative-state clones made for root candidates.
    pub candidate_state_clones: u64,
    /// New child nodes added to a search tree.
    pub tree_nodes_expanded: u64,
    /// Actions applied while traversing the tree or applying flat root moves.
    pub search_actions_applied: u64,
    /// Terminal rollouts completed.
    pub rollouts: u64,
    /// Total actions selected inside terminal rollouts.
    pub rollout_turns: u64,
    /// Deepest number of search-tree actions applied before a rollout.
    pub max_tree_depth: u32,
    /// Complete measured search duration.
    pub total_time: Duration,
    /// Time spent sampling hidden worlds.
    pub sampling_time: Duration,
    /// Time spent in tree/root-action work and result bookkeeping.
    pub tree_time: Duration,
    /// Time spent executing terminal rollouts.
    pub rollout_time: Duration,
    /// Rollout time spent projecting legal player observations.
    pub rollout_observation_time: Duration,
    /// Rollout time spent deriving logical information sets.
    pub rollout_deduction_time: Duration,
    /// Rollout time spent selecting policy actions.
    pub rollout_policy_time: Duration,
    /// Rollout time spent applying actions to simulator state.
    pub rollout_apply_time: Duration,
    /// Rollout loop and result-bookkeeping time outside the measured stages.
    pub rollout_other_time: Duration,
}

impl SearchDiagnostics {
    pub(crate) fn finish_timing(&mut self, total_time: Duration) {
        self.total_time = total_time;
        let separately_measured = self.sampling_time.saturating_add(self.rollout_time);
        self.tree_time = total_time.saturating_sub(separately_measured);
    }

    pub(crate) fn observe_tree_depth(&mut self, depth: u32) {
        self.max_tree_depth = self.max_tree_depth.max(depth);
    }

    pub(crate) fn add_rollout_timing(&mut self, timing: crate::RolloutDiagnostics) {
        self.rollout_time += timing.total_time;
        self.rollout_observation_time += timing.observation_time;
        self.rollout_deduction_time += timing.deduction_time;
        self.rollout_policy_time += timing.policy_time;
        self.rollout_apply_time += timing.apply_time;
        self.rollout_other_time += timing.other_time;
    }
}
