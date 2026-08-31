use crate::ConventionPolicyTier;
use hanabi_core::{Action, PlayerId};

/// Semantic role assigned to an action before policy ordering is applied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum HGroupActionKind {
    Connection,
    RequiredDiscard,
    PromisedPlay,
    Clue {
        target: PlayerId,
        save: bool,
        immediate_play: bool,
    },
    Discard,
    Fallback,
}

/// One canonical H-Group analysis consumed by planning and action selection.
#[derive(Clone, Copy, Debug)]
pub(super) struct CompiledHGroupAction {
    pub(super) action: Action,
    pub(super) kind: HGroupActionKind,
    pub(super) policy_tier: ConventionPolicyTier,
    pub(super) priority: i32,
}

/// Complete convention decision for one observer-relative position.
#[derive(Clone, Debug, Default)]
pub(super) struct HGroupActionSet {
    pub(super) actions: Vec<CompiledHGroupAction>,
    pub(super) preferred: Option<Action>,
    pub(super) predictable: Option<Action>,
}
