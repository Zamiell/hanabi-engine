use super::{
    Action, Card, CardId, ClueCandidate, HGroupConnection, HGroupProfile, HGroupRuleId,
    LogicalDeductions, PlayerId, PlayerView, identity_of, infer_h_group_from_replay,
    is_eventually_useful, projected_h_group_replay, prospective_clue_view, rule_enabled,
};

const TEAM_ACTION_COVERAGE_PENALTY: u16 = 80;
const INDIRECT_CONNECTION_PENALTY: u16 = 24;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ClueLineValue {
    commitments: Vec<(CardId, Card, PlayerId)>,
    new_connections: usize,
}

impl ClueLineValue {
    fn covered_players(&self) -> usize {
        let mut players = self
            .commitments
            .iter()
            .map(|(_, _, player)| *player)
            .collect::<Vec<_>>();
        players.sort_unstable_by_key(|player| player.index());
        players.dedup();
        players.len()
    }

    fn outcome(&self) -> Vec<(CardId, Card)> {
        self.commitments
            .iter()
            .map(|(card, identity, _)| (*card, *identity))
            .collect()
    }
}

/// Compares whole clue outcomes after ordinary legality and convention
/// interpretation have produced the candidate set.
///
/// Level 10's Directness Principle prefers the least complicated route to an
/// identical set of promised cards. Team action coverage separately rewards a
/// clue that establishes useful future actions for more than one teammate;
/// this keeps a token-refunding play from winning when the extra token has no
/// immediate job but another line prepares the rest of the team.
pub(super) fn apply_strategic_clue_values(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    candidates: &mut [ClueCandidate],
) {
    if !rule_enabled(profile, HGroupRuleId::SpecialDiscards) || candidates.len() < 2 {
        return;
    }
    let source = deductions.view();
    let baselines = (0..source.hands.len())
        .map(|player| {
            let observer = PlayerId::new(
                u8::try_from(player).expect("standard Hanabi has at most five players"),
            );
            projected_line_state(source, profile, observer)
        })
        .collect::<Option<Vec<_>>>();
    let Some(baselines) = baselines else {
        return;
    };
    let values = candidates
        .iter()
        .map(|candidate| clue_line_value(source, profile, candidate.action, &baselines))
        .collect::<Vec<_>>();
    let best_coverage = values
        .iter()
        .filter_map(|value| value.as_ref().map(ClueLineValue::covered_players))
        .max()
        .unwrap_or(0);

    for (index, candidate) in candidates.iter_mut().enumerate() {
        let Some(value) = &values[index] else {
            continue;
        };
        let uncovered_players = best_coverage.saturating_sub(value.covered_players());
        candidate.score = candidate.score.saturating_sub(
            TEAM_ACTION_COVERAGE_PENALTY
                .saturating_mul(u16::try_from(uncovered_players).unwrap_or(u16::MAX)),
        );

        let fewest_equivalent_connections = values
            .iter()
            .filter_map(Option::as_ref)
            .filter(|other| other.outcome() == value.outcome())
            .map(|other| other.new_connections)
            .min()
            .unwrap_or(value.new_connections);
        let unnecessary_connections = value
            .new_connections
            .saturating_sub(fewest_equivalent_connections);
        candidate.score = candidate.score.saturating_sub(
            INDIRECT_CONNECTION_PENALTY
                .saturating_mul(u16::try_from(unnecessary_connections).unwrap_or(u16::MAX)),
        );
    }
}

#[derive(Clone)]
struct ProjectedLineState {
    useful_commitments: Vec<(CardId, Card)>,
    connection: Option<HGroupConnection>,
}

fn projected_line_state(
    source: &PlayerView,
    profile: HGroupProfile,
    observer: PlayerId,
) -> Option<ProjectedLineState> {
    let (deductions, replay) = projected_h_group_replay(source, profile, observer)?;
    let inferred = infer_h_group_from_replay(&deductions, replay, profile);
    let mut useful_commitments = inferred
        .cards
        .iter()
        .filter_map(|note| {
            (note.identities.len() == 1)
                .then(|| note.identities.iter().next())
                .flatten()
                .filter(|identity| is_eventually_useful(source, *identity))
                .map(|identity| (note.card, identity))
        })
        .collect::<Vec<_>>();
    useful_commitments.extend(inferred.playable_now.iter().filter_map(|card| {
        identity_of(source, *card)
            .filter(|identity| is_eventually_useful(source, *identity))
            .map(|identity| (*card, identity))
    }));
    useful_commitments.sort_unstable_by_key(|(card, identity)| (card.index(), identity.index()));
    useful_commitments.dedup();
    Some(ProjectedLineState {
        useful_commitments,
        connection: inferred.connection,
    })
}

fn clue_line_value(
    source: &PlayerView,
    profile: HGroupProfile,
    action: Action,
    baselines: &[ProjectedLineState],
) -> Option<ClueLineValue> {
    let Action::Clue { target, clue } = action else {
        return None;
    };
    let touched = source.hands[target.index()]
        .iter()
        .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let after_clue = prospective_clue_view(source, target, clue, &touched);
    let mut value = ClueLineValue::default();

    for (player, baseline) in baselines.iter().enumerate() {
        let observer =
            PlayerId::new(u8::try_from(player).expect("standard Hanabi has at most five players"));
        let after = projected_line_state(&after_clue, profile, observer)?;
        value.commitments.extend(
            after
                .useful_commitments
                .iter()
                .copied()
                .filter(|commitment| !baseline.useful_commitments.contains(commitment))
                .map(|(card, identity)| (card, identity, observer)),
        );
        if let Some(connection) = after.connection.filter(|connection| {
            baseline
                .connection
                .is_none_or(|prior| prior.card != connection.card)
                && identity_of(source, connection.card) == Some(connection.identity)
        }) {
            value
                .commitments
                .push((connection.card, connection.identity, observer));
            value.new_connections += 1;
        }
    }
    value
        .commitments
        .sort_unstable_by_key(|(card, identity, player)| {
            (card.index(), identity.index(), player.index())
        });
    value.commitments.dedup();
    Some(value)
}
