use core::{fmt, str::FromStr};

use hanabi_core::FullState;
use rand::Rng;

use crate::{
    ConventionAgnosticPolicy, HGroupInferences, IdentitySet, InformationSet, LogicalDeductions,
    RolloutPolicy, SampleError, h_group::select_h_group_action, infer_h_group,
};

/// H-Group documentation revision implemented by this engine.
///
/// Keeping the source revision next to the convention implementation makes
/// analyses reproducible as the living convention framework changes.
pub const H_GROUP_RULESET_REVISION: &str = "1ef83242d71c62f2db6422f09e83abddba9611dd";

/// Static metadata for a convention framework exposed by the built-in registry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ConventionDescriptor {
    pub id: &'static str,
    pub display_name: &'static str,
    pub requires_profile: bool,
}

/// Convention frameworks discoverable through the built-in registry.
pub const CONVENTION_DESCRIPTORS: [ConventionDescriptor; 2] = [
    ConventionDescriptor {
        id: "none",
        display_name: "No convention",
        requires_profile: false,
    },
    ConventionDescriptor {
        id: "h-group",
        display_name: "H-Group",
        requires_profile: true,
    },
];

/// One numbered level in the cumulative H-Group learning path.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum HGroupLevel {
    Level1 = 1,
    Level2 = 2,
    Level3 = 3,
    Level4 = 4,
    Level5 = 5,
    Level6 = 6,
    Level7 = 7,
    Level8 = 8,
    Level9 = 9,
    Level10 = 10,
    Level11 = 11,
    Level12 = 12,
    Level13 = 13,
    Level14 = 14,
    Level15 = 15,
    Level16 = 16,
    Level17 = 17,
    Level18 = 18,
    Level19 = 19,
    Level20 = 20,
    Level21 = 21,
    Level22 = 22,
    Level23 = 23,
    Level24 = 24,
    Level25 = 25,
}

impl HGroupLevel {
    pub const MIN: Self = Self::Level1;
    pub const MAX: Self = Self::Level25;

    #[must_use]
    pub const fn number(self) -> u8 {
        self as u8
    }
}

impl fmt::Display for HGroupLevel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.number().fmt(formatter)
    }
}

impl TryFrom<u8> for HGroupLevel {
    type Error = ParseHGroupProfileError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Level1),
            2 => Ok(Self::Level2),
            3 => Ok(Self::Level3),
            4 => Ok(Self::Level4),
            5 => Ok(Self::Level5),
            6 => Ok(Self::Level6),
            7 => Ok(Self::Level7),
            8 => Ok(Self::Level8),
            9 => Ok(Self::Level9),
            10 => Ok(Self::Level10),
            11 => Ok(Self::Level11),
            12 => Ok(Self::Level12),
            13 => Ok(Self::Level13),
            14 => Ok(Self::Level14),
            15 => Ok(Self::Level15),
            16 => Ok(Self::Level16),
            17 => Ok(Self::Level17),
            18 => Ok(Self::Level18),
            19 => Ok(Self::Level19),
            20 => Ok(Self::Level20),
            21 => Ok(Self::Level21),
            22 => Ok(Self::Level22),
            23 => Ok(Self::Level23),
            24 => Ok(Self::Level24),
            25 => Ok(Self::Level25),
            _ => Err(ParseHGroupProfileError(value.to_string())),
        }
    }
}

/// Cumulative H-Group conventions enabled for a game.
///
/// `Max` is the effective 26th cumulative level. It is spelled `max` in user
/// interfaces because that is the name used by H-Group players.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HGroupProfile {
    Level(HGroupLevel),
    Max,
}

impl HGroupProfile {
    #[must_use]
    pub const fn effective_level(self) -> u8 {
        match self {
            Self::Level(level) => level.number(),
            Self::Max => 26,
        }
    }

    #[must_use]
    pub const fn includes(self, required: HGroupLevel) -> bool {
        self.effective_level() >= required.number()
    }

    #[must_use]
    pub const fn is_max(self) -> bool {
        matches!(self, Self::Max)
    }
}

impl fmt::Display for HGroupProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Level(level) => write!(formatter, "level {level}"),
            Self::Max => formatter.write_str("max"),
        }
    }
}

impl FromStr for HGroupProfile {
    type Err = ParseHGroupProfileError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value == "max" {
            return Ok(Self::Max);
        }
        let number = value
            .parse::<u8>()
            .map_err(|_| ParseHGroupProfileError(value.to_owned()))?;
        HGroupLevel::try_from(number).map(Self::Level)
    }
}

/// Error returned for an invalid cumulative H-Group profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseHGroupProfileError(String);

impl fmt::Display for ParseHGroupProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid H-Group level {:?}; expected 1 through 25, or max",
            self.0
        )
    }
}

impl std::error::Error for ParseHGroupProfileError {}

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
    HGroup(Box<HGroupInferences>),
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
    /// H-Group with one explicitly selected cumulative profile.
    HGroup(HGroupProfile),
}

impl SupportedConvention {
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::HGroup(_) => "h-group",
        }
    }

    #[must_use]
    pub const fn policy_id(self) -> &'static str {
        match self {
            Self::None => "convention_agnostic",
            Self::HGroup(_) => "h_group",
        }
    }

    #[must_use]
    pub const fn profile(self) -> Option<HGroupProfile> {
        match self {
            Self::None => None,
            Self::HGroup(profile) => Some(profile),
        }
    }

    #[must_use]
    pub const fn ruleset_revision(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::HGroup(_) => Some(H_GROUP_RULESET_REVISION),
        }
    }
}

impl fmt::Display for SupportedConvention {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => formatter.write_str(self.id()),
            Self::HGroup(profile) => write!(formatter, "{} ({profile})", self.id()),
        }
    }
}

impl FromStr for SupportedConvention {
    type Err = ParseConventionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "none" => Ok(Self::None),
            value if value.starts_with("h-group:") => value["h-group:".len()..]
                .parse()
                .map(Self::HGroup)
                .map_err(ParseConventionError::InvalidHGroupProfile),
            _ => Err(ParseConventionError::Unknown(value.to_owned())),
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

    /// Rule-legal actions that are also permitted by this framework.
    #[must_use]
    fn candidate_actions(&self, deductions: &LogicalDeductions) -> Vec<hanabi_core::Action> {
        deductions.view().legal_actions()
    }

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
            Self::HGroup(_) => true,
        }
    }

    fn select_action(
        &self,
        deductions: &LogicalDeductions,
    ) -> Result<hanabi_core::Action, crate::PolicyError> {
        match self {
            Self::None => ConventionAgnosticPolicy.select_action(deductions),
            Self::HGroup(profile) => select_h_group_action(deductions, *profile)
                .ok_or(crate::PolicyError::NoConventionAction),
        }
    }

    fn select_policy_action(
        &self,
        deductions: &crate::PolicyDeductions<'_>,
    ) -> Result<hanabi_core::Action, crate::PolicyError> {
        match self {
            Self::None | Self::HGroup(_) => {
                ConventionAgnosticPolicy.select_policy_action(deductions)
            }
        }
    }
}

impl ConventionFramework for SupportedConvention {
    fn infer(&self, deductions: &LogicalDeductions) -> ConventionInferences {
        match self {
            Self::None => ConventionInferences::None,
            Self::HGroup(profile) => {
                ConventionInferences::HGroup(Box::new(infer_h_group(deductions, *profile)))
            }
        }
    }

    fn candidate_actions(&self, deductions: &LogicalDeductions) -> Vec<hanabi_core::Action> {
        match self {
            Self::None => deductions.view().legal_actions(),
            Self::HGroup(profile) => {
                crate::h_group::h_group_candidate_actions(deductions, *profile)
            }
        }
    }

    fn sample_root_world<R: Rng + ?Sized>(
        &self,
        information_set: &InformationSet,
        rng: &mut R,
    ) -> Result<FullState, SampleError> {
        match self {
            Self::None => information_set.sample(rng),
            Self::HGroup(profile) => {
                let inferred = infer_h_group(information_set, *profile);
                let constraints = inferred
                    .cards
                    .iter()
                    .map(|card| (card.card, card.identities))
                    .collect::<Vec<_>>();
                let view = information_set.view();
                let immediately_playable = IdentitySet::from_mask(
                    IdentitySet::all()
                        .iter()
                        .filter(|identity| {
                            identity.rank.number()
                                == u8::try_from(view.play_stacks[identity.suit.index()].len())
                                    .expect("a standard stack has at most five cards")
                                    + 1
                        })
                        .fold(0, |mask, identity| mask | (1 << identity.index())),
                );
                let mut branches = vec![Vec::new()];
                for promise in &inferred.connection_promises {
                    let expected = IdentitySet::singleton(promise.identity);
                    let wrong_success = immediately_playable.without(expected);
                    let alternatives = promise
                        .cards
                        .iter()
                        .enumerate()
                        .map(|(correct, card)| {
                            promise.cards[..correct]
                                .iter()
                                .copied()
                                .map(|prior| (prior, wrong_success))
                                .chain(core::iter::once((*card, expected)))
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>();
                    branches = branches
                        .into_iter()
                        .flat_map(|branch| {
                            alternatives.iter().map(move |alternative| {
                                branch
                                    .iter()
                                    .copied()
                                    .chain(alternative.iter().copied())
                                    .collect()
                            })
                        })
                        .collect();
                }
                if inferred.connection_promises.is_empty() {
                    information_set
                        .sample_constrained(&constraints, rng)
                        .or_else(|error| match error {
                            SampleError::NoConsistentWorld => information_set.sample(rng),
                            other @ SampleError::Determinization(_) => Err(other),
                        })
                } else {
                    information_set
                        .sample_constrained_branches(&constraints, &branches, rng)
                        .or_else(|error| match error {
                            SampleError::NoConsistentWorld => information_set.sample(rng),
                            other @ SampleError::Determinization(_) => Err(other),
                        })
                }
            }
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
pub enum ParseConventionError {
    Unknown(String),
    InvalidHGroupProfile(ParseHGroupProfileError),
}

impl fmt::Display for ParseConventionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(value) => write!(
                formatter,
                "unknown convention {value:?}; expected none or h-group:<1-25|max>"
            ),
            Self::InvalidHGroupProfile(error) => error.fmt(formatter),
        }
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
    fn registry_separates_framework_metadata_from_concrete_selections() {
        assert_eq!(SupportedConvention::default(), SupportedConvention::None);
        assert_eq!(CONVENTION_DESCRIPTORS[0].id, "none");
        assert_eq!(CONVENTION_DESCRIPTORS[1].id, "h-group");
        assert!(CONVENTION_DESCRIPTORS[1].requires_profile);
        assert_eq!("none".parse(), Ok(SupportedConvention::None));
        assert!("h-group".parse::<SupportedConvention>().is_err());
        assert_eq!(
            "h-group:5".parse(),
            Ok(SupportedConvention::HGroup(HGroupProfile::Level(
                HGroupLevel::Level5
            )))
        );
        assert_eq!(
            "h-group:max".parse(),
            Ok(SupportedConvention::HGroup(HGroupProfile::Max))
        );
        assert_eq!(SupportedConvention::None.to_string(), "none");
        assert_eq!(
            SupportedConvention::HGroup(HGroupProfile::Level(HGroupLevel::Level5)).to_string(),
            "h-group (level 5)"
        );
    }

    #[test]
    fn h_group_profiles_are_cumulative_and_max_is_level_26() {
        let level_five = HGroupProfile::Level(HGroupLevel::Level5);
        assert!(level_five.includes(HGroupLevel::Level1));
        assert!(level_five.includes(HGroupLevel::Level5));
        assert!(!level_five.includes(HGroupLevel::Level6));
        assert!(!level_five.is_max());

        assert!(HGroupProfile::Max.includes(HGroupLevel::Level25));
        assert!(HGroupProfile::Max.is_max());
        assert_eq!(HGroupProfile::Max.effective_level(), 26);
        assert_eq!("1".parse(), Ok(HGroupProfile::Level(HGroupLevel::Level1)));
        assert_eq!("25".parse(), Ok(HGroupProfile::Level(HGroupLevel::Level25)));
        assert!("0".parse::<HGroupProfile>().is_err());
        assert!("26".parse::<HGroupProfile>().is_err());
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
    fn h_group_selection_has_typed_inferences_and_revision_metadata() {
        let state = FullState::new_standard(2, standard_deck()).unwrap();
        let view = state.view_for(PlayerId::new(0)).unwrap();
        let information_set = InformationSet::new(view.clone()).unwrap();
        let deductions = LogicalDeductions::new(view).unwrap();
        let convention = SupportedConvention::HGroup(HGroupProfile::Level(HGroupLevel::Level3));

        assert!(matches!(
            convention.infer(&deductions),
            ConventionInferences::HGroup(inferred) if inferred.clues.is_empty()
        ));
        assert_eq!(
            convention.profile(),
            Some(HGroupProfile::Level(HGroupLevel::Level3))
        );
        assert_eq!(
            convention.ruleset_revision(),
            Some(H_GROUP_RULESET_REVISION)
        );

        let mut framework_rng = StdRng::seed_from_u64(42);
        let mut logical_rng = StdRng::seed_from_u64(42);
        assert_eq!(
            convention
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
