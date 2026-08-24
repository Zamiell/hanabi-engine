use crate::HGroupProfile;

use super::H_GROUP_LEVELS;

/// Executable rule group corresponding to one cumulative H-Group level.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(super) enum HGroupRuleId {
    Basic = 1,
    BasicMoves = 2,
    BasicStrategy = 3,
    ChopMoves = 4,
    SpecialFinesses = 5,
    TempoClues = 6,
    EmergencyDiscards = 7,
    EndGame = 8,
    Stalling = 9,
    SpecialDiscards = 10,
    Bluffs = 11,
    Context = 12,
    IntermediateBluffs = 13,
    TrashMoves = 14,
    DoubleBluffs = 15,
    EjectionsAndDischarges = 16,
    Duplication = 17,
    Elimination = 18,
    FiveTech = 19,
    OutOfOrderPlay = 20,
    Ignition = 21,
    PhantomPlayable = 22,
    Charms = 23,
    UnnecessaryMoves = 24,
    Priority = 25,
    Extras = 26,
}

impl HGroupRuleId {
    const fn index(self) -> usize {
        self as usize - 1
    }
}

/// Registry-driven cumulative rule selection.
#[derive(Clone, Copy, Debug)]
pub(super) struct HGroupRules {
    profile: HGroupProfile,
}

impl HGroupRules {
    pub(super) const fn new(profile: HGroupProfile) -> Self {
        Self { profile }
    }

    pub(super) fn enabled(self, rule: HGroupRuleId) -> bool {
        let required = H_GROUP_LEVELS[rule.index()].profile.effective_level();
        self.profile.effective_level() >= required
    }
}

pub(super) fn rule_enabled(profile: HGroupProfile, rule: HGroupRuleId) -> bool {
    HGroupRules::new(profile).enabled(rule)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULES: [HGroupRuleId; 26] = [
        HGroupRuleId::Basic,
        HGroupRuleId::BasicMoves,
        HGroupRuleId::BasicStrategy,
        HGroupRuleId::ChopMoves,
        HGroupRuleId::SpecialFinesses,
        HGroupRuleId::TempoClues,
        HGroupRuleId::EmergencyDiscards,
        HGroupRuleId::EndGame,
        HGroupRuleId::Stalling,
        HGroupRuleId::SpecialDiscards,
        HGroupRuleId::Bluffs,
        HGroupRuleId::Context,
        HGroupRuleId::IntermediateBluffs,
        HGroupRuleId::TrashMoves,
        HGroupRuleId::DoubleBluffs,
        HGroupRuleId::EjectionsAndDischarges,
        HGroupRuleId::Duplication,
        HGroupRuleId::Elimination,
        HGroupRuleId::FiveTech,
        HGroupRuleId::OutOfOrderPlay,
        HGroupRuleId::Ignition,
        HGroupRuleId::PhantomPlayable,
        HGroupRuleId::Charms,
        HGroupRuleId::UnnecessaryMoves,
        HGroupRuleId::Priority,
        HGroupRuleId::Extras,
    ];

    #[test]
    fn executable_rule_order_matches_level_metadata() {
        for (index, rule) in RULES.into_iter().enumerate() {
            let descriptor = H_GROUP_LEVELS[index];
            assert!(rule_enabled(descriptor.profile, rule));
            if index > 0 {
                assert!(!rule_enabled(H_GROUP_LEVELS[index - 1].profile, rule));
            }
        }
    }
}
