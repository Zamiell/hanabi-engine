use core::{fmt, str::FromStr};

use crate::{
    BeliefConstraints, ConventionAgnosticPolicy, HGroupInferences, IdentitySet, LogicalDeductions,
};

/// H-Group documentation revision implemented by this engine.
///
/// Keeping the source revision next to the convention implementation makes
/// analyses reproducible as the living convention framework changes.
pub const H_GROUP_RULESET_REVISION: &str = "2db23dc5bc8bba067fec6c79b3323bef21ed6e1c";

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

/// One convention-permitted action and its deterministic ordering priority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConventionAction {
    pub action: hanabi_core::Action,
    pub priority: i32,
    pub reason: ConventionActionReason,
}

/// Structured explanation for why a convention admitted an action.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ConventionActionReason {
    ConventionFree,
    Connection,
    RequiredDiscard,
    PromisedPlay,
    PlayClue,
    SaveClue,
    OtherClue,
    Discard,
    #[default]
    Fallback,
}

/// Semantic reason a legal action was excluded by the selected convention.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConventionRejectionReason {
    NoNewInformation,
    NoFocus,
    RepeatsKnownIdentity,
    NoConventionMeaning,
    UnsafeConnection,
    RedundantOutcome,
}

/// A legal game action rejected before strategic planning.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RejectedConventionAction {
    pub action: hanabi_core::Action,
    pub reason: ConventionRejectionReason,
}

/// Complete convention interpretation of one observer-relative position.
///
/// Planning and diagnostics consume this object together so candidate
/// admissibility, ordering, forced continuations, and inferred identities
/// cannot be reconstructed by separate call paths.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConventionAnalysis {
    pub inferences: ConventionInferences,
    pub actions: Vec<ConventionAction>,
    pub rejected_actions: Vec<RejectedConventionAction>,
    pub preferred_action: Option<hanabi_core::Action>,
    pub forced_action: Option<hanabi_core::Action>,
    pub belief_constraints: BeliefConstraints,
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

impl SupportedConvention {
    /// Interprets one position and constructs the complete convention decision
    /// consumed by deterministic planning and diagnostics.
    #[must_use]
    pub fn analyze(self, deductions: &LogicalDeductions) -> ConventionAnalysis {
        match self {
            Self::None => ConventionAnalysis {
                inferences: ConventionInferences::None,
                actions: deductions
                    .view()
                    .legal_actions()
                    .into_iter()
                    .map(|action| ConventionAction {
                        action,
                        priority: 100,
                        reason: ConventionActionReason::ConventionFree,
                    })
                    .collect(),
                rejected_actions: Vec::new(),
                preferred_action: ConventionAgnosticPolicy.select_action(deductions).ok(),
                forced_action: None,
                belief_constraints: BeliefConstraints::default(),
            },
            Self::HGroup(profile) => {
                let decision = crate::h_group::analyze_h_group_convention(deductions, profile);
                let actions = decision
                    .actions
                    .into_iter()
                    .map(|(action, priority, reason)| ConventionAction {
                        action,
                        priority,
                        reason,
                    })
                    .collect();
                let belief_constraints =
                    h_group_belief_constraints(deductions, &decision.inferences);
                ConventionAnalysis {
                    inferences: ConventionInferences::HGroup(Box::new(decision.inferences)),
                    actions,
                    rejected_actions: decision.rejected_actions,
                    preferred_action: decision.preferred,
                    forced_action: decision.forced,
                    belief_constraints,
                }
            }
        }
    }

    pub(crate) fn project_symbolic_line(
        self,
        view: &hanabi_core::PlayerView,
        root: hanabi_core::Action,
        limit: u8,
    ) -> crate::SymbolicLineOutcome {
        match self {
            Self::None => crate::SymbolicLineOutcome::default(),
            Self::HGroup(profile) => {
                crate::h_group::project_h_group_line(view, profile, root, limit)
            }
        }
    }
}

fn h_group_belief_constraints(
    deductions: &LogicalDeductions,
    inferred: &HGroupInferences,
) -> BeliefConstraints {
    let constraints = inferred
        .cards
        .iter()
        .map(|card| (card.card, card.identities))
        .collect::<Vec<_>>();
    if inferred.connection_promises.is_empty() {
        return BeliefConstraints {
            constraints,
            branches: Vec::new(),
        };
    }

    let view = deductions.view();
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
    BeliefConstraints {
        constraints,
        branches,
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

    use hanabi_core::{FullState, PlayerId, standard_deck};

    #[test]
    fn registry_separates_framework_metadata_from_concrete_selections() {
        assert_eq!(SupportedConvention::default(), SupportedConvention::None);
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

        assert!(HGroupProfile::Max.includes(HGroupLevel::Level25));
        assert_eq!(HGroupProfile::Max.effective_level(), 26);
        assert_eq!("1".parse(), Ok(HGroupProfile::Level(HGroupLevel::Level1)));
        assert_eq!("25".parse(), Ok(HGroupProfile::Level(HGroupLevel::Level25)));
        assert!("0".parse::<HGroupProfile>().is_err());
        assert!("26".parse::<HGroupProfile>().is_err());
    }

    #[test]
    fn none_preserves_logical_inferences() {
        let state = FullState::new_standard(2, standard_deck()).unwrap();
        let view = state.view_for(PlayerId::new(0)).unwrap();
        let deductions = LogicalDeductions::new(view).unwrap();

        assert_eq!(
            SupportedConvention::None.analyze(&deductions).inferences,
            ConventionInferences::None
        );
    }

    #[test]
    fn h_group_selection_has_typed_inferences_and_revision_metadata() {
        let state = FullState::new_standard(2, standard_deck()).unwrap();
        let view = state.view_for(PlayerId::new(0)).unwrap();
        let deductions = LogicalDeductions::new(view).unwrap();
        let convention = SupportedConvention::HGroup(HGroupProfile::Level(HGroupLevel::Level3));

        assert!(matches!(
            convention.analyze(&deductions).inferences,
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
    }
}
