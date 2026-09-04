use super::{CluePurpose, CompiledClueAction, HGroupMoveKind};

/// Whether a named interpretation is one of the Ignition family. Consumers
/// use this semantic family instead of maintaining subtly different lists of
/// concrete Ignition variants.
pub(super) const fn is_ignition(kind: HGroupMoveKind) -> bool {
    matches!(
        kind,
        HGroupMoveKind::Ignition
            | HGroupMoveKind::UnnecessaryIgnition
            | HGroupMoveKind::ReplayDoubleIgnition
            | HGroupMoveKind::TrashDoubleIgnition
            | HGroupMoveKind::PokeDoubleIgnition
            | HGroupMoveKind::ChopMoveIgnition
            | HGroupMoveKind::BombDoubleIgnition
            | HGroupMoveKind::BombTripleIgnition
    )
}

/// Explicit conflict relation between convention interpretations. A rule that
/// introduces a new interpretation must declare its precedence here instead
/// of making downstream consumers independently suppress competing signals.
pub(super) const fn supersedes(challenger: HGroupMoveKind, incumbent: HGroupMoveKind) -> bool {
    is_ignition(challenger)
        && matches!(
            incumbent,
            HGroupMoveKind::UnknownTrashDischarge
                | HGroupMoveKind::UnknownDupeDischarge
                | HGroupMoveKind::OutOfPositionDischarge
                | HGroupMoveKind::StackedDischarge
                | HGroupMoveKind::UnknownTrashCharm
                | HGroupMoveKind::JunkCharm
                | HGroupMoveKind::TrashPull
        )
}

/// Resolves two compiled meanings for the same physical clue.
pub(super) fn candidate_replaces(
    existing: CompiledClueAction,
    challenger: CompiledClueAction,
    bluff_recognized: bool,
) -> bool {
    challenger.purpose() == CluePurpose::Fix
        || challenger
            .move_kind()
            .is_some_and(named_interpretation_replaces_ordinary)
        || (challenger.purpose() == CluePurpose::Advanced && bluff_recognized)
        || challenger.move_kind().is_some_and(|challenger_kind| {
            existing
                .move_kind()
                .is_some_and(|existing_kind| supersedes(challenger_kind, existing_kind))
        })
}

pub(super) const fn named_interpretation_replaces_ordinary(kind: HGroupMoveKind) -> bool {
    matches!(
        kind,
        HGroupMoveKind::Ejection
            | HGroupMoveKind::UnnecessaryIgnition
            | HGroupMoveKind::Discharge
            | HGroupMoveKind::FiveColorEjection
            | HGroupMoveKind::UnknownTrashDischarge
            | HGroupMoveKind::UnknownDupeDischarge
            | HGroupMoveKind::OutOfPositionEjection
            | HGroupMoveKind::OutOfPositionDischarge
            | HGroupMoveKind::StackedEjection
            | HGroupMoveKind::StackedDischarge
            | HGroupMoveKind::TrashPushDischarge
            | HGroupMoveKind::TrashPushEjection
            | HGroupMoveKind::BadChopMoveEjection
            | HGroupMoveKind::BadTrashFinesseEjection
            | HGroupMoveKind::TrashFinessePushEjection
            | HGroupMoveKind::RankChoiceEjection
            | HGroupMoveKind::TrashEjection
            | HGroupMoveKind::ReplayEjection
            | HGroupMoveKind::PokeEjection
            | HGroupMoveKind::LieComponentFinesse
            | HGroupMoveKind::Charm
            | HGroupMoveKind::ReplayDoubleIgnition
            | HGroupMoveKind::TrashDoubleIgnition
            | HGroupMoveKind::PokeDoubleIgnition
            | HGroupMoveKind::ChopMoveIgnition
            | HGroupMoveKind::BombDoubleIgnition
            | HGroupMoveKind::BombTripleIgnition
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignition_supersedes_provisional_discharge_and_charm_readings() {
        for incumbent in [
            HGroupMoveKind::UnknownTrashDischarge,
            HGroupMoveKind::UnknownDupeDischarge,
            HGroupMoveKind::UnknownTrashCharm,
            HGroupMoveKind::JunkCharm,
        ] {
            assert!(supersedes(HGroupMoveKind::TrashDoubleIgnition, incumbent));
        }
    }
}
