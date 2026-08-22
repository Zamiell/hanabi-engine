use core::{fmt, str::FromStr};

use hanabi_core::FullState;
use rand::Rng;

use crate::{
    ConventionAgnosticPolicy, InformationSet, LogicalDeductions, RolloutPolicy, SampleError,
};

/// Convention-specific conclusions kept separate from logical certainties.
///
/// This is a closed registry parallel to [`SupportedConvention`]. Each newly
/// supported framework adds its own typed inference payload as a variant,
/// avoiding a lowest-common-denominator collection of convention concepts.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum ConventionInferences {
    /// No meaning is assigned to why actions were taken.
    #[default]
    None,
}

/// Convention frameworks built into this engine.
///
/// Keeping this registry closed makes command-line configuration, persisted
/// analysis requests, and match dispatch exhaustive and reproducible.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum SupportedConvention {
    /// Direct clues and card counts only.
    #[default]
    None,
}

impl SupportedConvention {
    /// All convention identifiers accepted by the built-in registry.
    pub const ALL: [Self; 1] = [Self::None];

    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::None => "none",
        }
    }

    #[must_use]
    pub const fn policy_id(self) -> &'static str {
        match self {
            Self::None => "convention_agnostic",
        }
    }
}

impl fmt::Display for SupportedConvention {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.id())
    }
}

impl FromStr for SupportedConvention {
    type Err = ParseConventionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "none" => Ok(Self::None),
            _ => Err(ParseConventionError(value.to_owned())),
        }
    }
}

/// A convention supplies both player behavior and the root belief sampler.
///
/// Search deliberately requires this trait instead of accepting only a
/// [`RolloutPolicy`]. Consequently a future convention cannot change how clues
/// are interpreted during rollouts while silently leaving root worlds sampled
/// from a different belief model.
pub trait ConventionFramework: RolloutPolicy {
    /// Derives framework-specific interpretations without altering
    /// [`LogicalDeductions`].
    #[must_use]
    fn infer(&self, deductions: &LogicalDeductions) -> ConventionInferences;

    /// Samples one root world according to this framework's beliefs.
    ///
    /// The no-convention implementation preserves the exact card-copy-weighted
    /// logical distribution. A convention with soft or hard assumptions should
    /// override this method rather than modifying [`InformationSet`].
    ///
    /// # Errors
    ///
    /// Returns [`SampleError`] when no consistent root world can be sampled.
    fn sample_root_world<R: Rng + ?Sized>(
        &self,
        information_set: &InformationSet,
        rng: &mut R,
    ) -> Result<FullState, SampleError>;
}

impl RolloutPolicy for SupportedConvention {
    fn uses_history(&self) -> bool {
        match self {
            Self::None => ConventionAgnosticPolicy.uses_history(),
        }
    }

    fn select_action(
        &self,
        deductions: &LogicalDeductions,
    ) -> Result<hanabi_core::Action, crate::PolicyError> {
        match self {
            Self::None => ConventionAgnosticPolicy.select_action(deductions),
        }
    }

    fn select_policy_action(
        &self,
        deductions: &crate::PolicyDeductions<'_>,
    ) -> Result<hanabi_core::Action, crate::PolicyError> {
        match self {
            Self::None => ConventionAgnosticPolicy.select_policy_action(deductions),
        }
    }
}

impl ConventionFramework for SupportedConvention {
    fn infer(&self, _deductions: &LogicalDeductions) -> ConventionInferences {
        match self {
            Self::None => ConventionInferences::None,
        }
    }

    fn sample_root_world<R: Rng + ?Sized>(
        &self,
        information_set: &InformationSet,
        rng: &mut R,
    ) -> Result<FullState, SampleError> {
        match self {
            Self::None => information_set.sample(rng),
        }
    }
}

impl ConventionFramework for ConventionAgnosticPolicy {
    fn infer(&self, _deductions: &LogicalDeductions) -> ConventionInferences {
        ConventionInferences::None
    }

    fn sample_root_world<R: Rng + ?Sized>(
        &self,
        information_set: &InformationSet,
        rng: &mut R,
    ) -> Result<FullState, SampleError> {
        information_set.sample(rng)
    }
}

/// Error returned when a convention identifier is not in the built-in
/// registry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseConventionError(String);

impl fmt::Display for ParseConventionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown convention {:?}; expected none", self.0)
    }
}

impl std::error::Error for ParseConventionError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    use hanabi_core::{Action, FullState, PlayerId, standard_deck};
    use rand::{SeedableRng, rngs::StdRng};

    use crate::{
        IsmctsConfig, MonteCarloConfig, PolicyDeductions, PolicyError, evaluate_actions,
        ismcts_search,
    };

    struct CountingFramework {
        samples: Cell<u32>,
    }

    impl RolloutPolicy for CountingFramework {
        fn uses_history(&self) -> bool {
            false
        }

        fn select_action(&self, deductions: &LogicalDeductions) -> Result<Action, PolicyError> {
            ConventionAgnosticPolicy.select_action(deductions)
        }

        fn select_policy_action(
            &self,
            deductions: &PolicyDeductions<'_>,
        ) -> Result<Action, PolicyError> {
            ConventionAgnosticPolicy.select_policy_action(deductions)
        }
    }

    impl ConventionFramework for CountingFramework {
        fn infer(&self, _deductions: &LogicalDeductions) -> ConventionInferences {
            ConventionInferences::None
        }

        fn sample_root_world<R: Rng + ?Sized>(
            &self,
            information_set: &InformationSet,
            rng: &mut R,
        ) -> Result<FullState, SampleError> {
            self.samples.set(self.samples.get() + 1);
            information_set.sample(rng)
        }
    }

    #[test]
    fn none_is_the_default_and_only_registered_framework() {
        assert_eq!(SupportedConvention::default(), SupportedConvention::None);
        assert_eq!(SupportedConvention::ALL, [SupportedConvention::None]);
        assert_eq!("none".parse(), Ok(SupportedConvention::None));
        assert!("h-group".parse::<SupportedConvention>().is_err());
        assert_eq!(SupportedConvention::None.to_string(), "none");
    }

    #[test]
    fn none_preserves_logical_inferences_and_uniform_sampling() {
        let state = FullState::new_standard(2, standard_deck()).unwrap();
        let view = state.view_for(PlayerId::new(0)).unwrap();
        let information_set = InformationSet::new(view.clone()).unwrap();
        let deductions = LogicalDeductions::new(view).unwrap();

        assert_eq!(
            SupportedConvention::None.infer(&deductions),
            ConventionInferences::None
        );

        let mut framework_rng = StdRng::seed_from_u64(42);
        let mut logical_rng = StdRng::seed_from_u64(42);
        assert_eq!(
            SupportedConvention::None
                .sample_root_world(&information_set, &mut framework_rng)
                .unwrap(),
            information_set.sample(&mut logical_rng).unwrap()
        );
    }

    #[test]
    fn both_search_modes_use_the_framework_root_sampler() {
        let state = FullState::new_standard(2, standard_deck()).unwrap();
        let information_set =
            InformationSet::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
        let framework = CountingFramework {
            samples: Cell::new(0),
        };

        ismcts_search(
            &information_set,
            &framework,
            IsmctsConfig {
                iterations: 7,
                exploration: core::f64::consts::SQRT_2,
                seed: 1,
            },
        )
        .unwrap();
        assert_eq!(framework.samples.get(), 7);

        evaluate_actions(
            &information_set,
            &framework,
            MonteCarloConfig {
                samples_per_action: 3,
                seed: 1,
            },
        )
        .unwrap();
        assert_eq!(framework.samples.get(), 10);
    }
}
