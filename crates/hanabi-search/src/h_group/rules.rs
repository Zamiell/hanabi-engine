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

/// Semantic stage in the post-event convention transition.
///
/// The phases make precedence explicit without changing the established
/// H-Group execution order. A rule may depend on an earlier rule even when
/// that rule is disabled by the selected cumulative profile; the dependency
/// constrains ordering whenever both rules are active.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum RulePhase {
    Precedence,
    Foundation,
    Refinement,
    Specialization,
    Extension,
    Finalization,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RuleSpec {
    pub(super) id: HGroupRuleId,
    pub(super) phase: RulePhase,
    pub(super) depends_on: &'static [HGroupRuleId],
}

impl HGroupRuleId {
    const fn index(self) -> usize {
        self as usize - 1
    }
}

/// Rules whose semantics are part of the core clue/connection reducer rather
/// than a post-event recognizer.
pub(super) const INLINE_RULES: [HGroupRuleId; 2] =
    [HGroupRuleId::Basic, HGroupRuleId::SpecialFinesses];

/// Ordered executable post-event registry. Refinements intentionally precede
/// lower-level provisional interpretations when convention semantics require
/// replacement rather than simple cumulative application.
pub(super) const POST_EVENT_RULES: [RuleSpec; 24] = [
    RuleSpec {
        id: HGroupRuleId::Priority,
        phase: RulePhase::Precedence,
        depends_on: &[],
    },
    RuleSpec {
        id: HGroupRuleId::BasicMoves,
        phase: RulePhase::Foundation,
        depends_on: &[],
    },
    RuleSpec {
        id: HGroupRuleId::BasicStrategy,
        phase: RulePhase::Foundation,
        depends_on: &[HGroupRuleId::BasicMoves],
    },
    RuleSpec {
        id: HGroupRuleId::Elimination,
        phase: RulePhase::Refinement,
        depends_on: &[HGroupRuleId::BasicStrategy],
    },
    RuleSpec {
        id: HGroupRuleId::TempoClues,
        phase: RulePhase::Refinement,
        depends_on: &[HGroupRuleId::BasicMoves],
    },
    RuleSpec {
        id: HGroupRuleId::EmergencyDiscards,
        phase: RulePhase::Refinement,
        depends_on: &[],
    },
    RuleSpec {
        id: HGroupRuleId::PhantomPlayable,
        phase: RulePhase::Refinement,
        depends_on: &[HGroupRuleId::Elimination],
    },
    RuleSpec {
        id: HGroupRuleId::EndGame,
        phase: RulePhase::Specialization,
        depends_on: &[],
    },
    RuleSpec {
        id: HGroupRuleId::Stalling,
        phase: RulePhase::Specialization,
        depends_on: &[HGroupRuleId::BasicMoves],
    },
    RuleSpec {
        id: HGroupRuleId::SpecialDiscards,
        phase: RulePhase::Specialization,
        depends_on: &[],
    },
    RuleSpec {
        id: HGroupRuleId::Bluffs,
        phase: RulePhase::Specialization,
        depends_on: &[HGroupRuleId::BasicMoves],
    },
    RuleSpec {
        id: HGroupRuleId::Context,
        phase: RulePhase::Specialization,
        depends_on: &[HGroupRuleId::BasicStrategy],
    },
    RuleSpec {
        id: HGroupRuleId::IntermediateBluffs,
        phase: RulePhase::Extension,
        depends_on: &[HGroupRuleId::Bluffs],
    },
    RuleSpec {
        id: HGroupRuleId::TrashMoves,
        phase: RulePhase::Extension,
        depends_on: &[HGroupRuleId::SpecialDiscards],
    },
    RuleSpec {
        id: HGroupRuleId::ChopMoves,
        phase: RulePhase::Extension,
        depends_on: &[HGroupRuleId::TrashMoves],
    },
    RuleSpec {
        id: HGroupRuleId::DoubleBluffs,
        phase: RulePhase::Extension,
        depends_on: &[HGroupRuleId::IntermediateBluffs],
    },
    RuleSpec {
        id: HGroupRuleId::EjectionsAndDischarges,
        phase: RulePhase::Extension,
        depends_on: &[HGroupRuleId::DoubleBluffs],
    },
    RuleSpec {
        id: HGroupRuleId::Duplication,
        phase: RulePhase::Extension,
        depends_on: &[],
    },
    RuleSpec {
        id: HGroupRuleId::FiveTech,
        phase: RulePhase::Extension,
        depends_on: &[],
    },
    RuleSpec {
        id: HGroupRuleId::OutOfOrderPlay,
        phase: RulePhase::Extension,
        depends_on: &[],
    },
    RuleSpec {
        id: HGroupRuleId::Ignition,
        phase: RulePhase::Extension,
        depends_on: &[],
    },
    RuleSpec {
        id: HGroupRuleId::Charms,
        phase: RulePhase::Extension,
        depends_on: &[],
    },
    RuleSpec {
        id: HGroupRuleId::UnnecessaryMoves,
        phase: RulePhase::Extension,
        depends_on: &[],
    },
    RuleSpec {
        id: HGroupRuleId::Extras,
        phase: RulePhase::Finalization,
        depends_on: &[],
    },
];

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
        debug_assert!(
            INLINE_RULES.contains(&rule) || POST_EVENT_RULES.iter().any(|spec| spec.id == rule)
        );
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

    #[test]
    fn every_level_rule_has_exactly_one_execution_path() {
        let mut execution = INLINE_RULES
            .into_iter()
            .chain(POST_EVENT_RULES.map(|spec| spec.id))
            .collect::<Vec<_>>();
        execution.sort_unstable_by_key(|rule| *rule as u8);
        assert_eq!(execution, RULES);
    }

    #[test]
    fn semantic_phases_and_dependencies_are_ordered() {
        for (index, spec) in POST_EVENT_RULES.iter().enumerate() {
            if let Some(previous) = index.checked_sub(1).map(|prior| POST_EVENT_RULES[prior]) {
                assert!(previous.phase <= spec.phase);
            }
            for dependency in spec.depends_on {
                assert!(
                    POST_EVENT_RULES[..index]
                        .iter()
                        .any(|candidate| candidate.id == *dependency),
                    "{:?} depends on {:?}, which must execute earlier",
                    spec.id,
                    dependency
                );
            }
        }
    }
}
