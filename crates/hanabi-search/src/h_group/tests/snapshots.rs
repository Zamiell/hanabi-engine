use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::*;

const SNAPSHOT_SCHEMA_VERSION: u8 = 5;
const UPDATE_ENVIRONMENT_VARIABLE: &str = "HANABI_UPDATE_SUPERPOSITIONS";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReplayEpistemicSnapshot {
    schema_version: u8,
    source: &'static str,
    convention: &'static str,
    initial: Vec<InitialPlayerSnapshot>,
    turn_deltas: Vec<TurnDelta>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PlayerStateSnapshot {
    hand: Vec<CardSnapshot>,
    connection: Option<ConnectionSnapshot>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InitialPlayerSnapshot {
    player: usize,
    hand: Vec<CardSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    connection: Option<ConnectionSnapshot>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TurnDelta {
    /// Number of completed player actions. Turn one is the position after the
    /// first action; the initial deal is stored separately in `initial`.
    turn: u32,
    changes: Vec<PlayerDelta>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlayerDelta {
    player: usize,
    /// Complete replacement snapshots for cards that are new or changed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    cards: Vec<CardDelta>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    removed_cards: Vec<usize>,
    /// Absent means unchanged, an object is the new connection, and `null`
    /// means that the previous connection was cleared.
    #[serde(skip_serializing_if = "Option::is_none")]
    connection: Option<ConnectionDelta>,
}

impl PlayerDelta {
    fn is_empty(&self) -> bool {
        self.cards.is_empty() && self.removed_cards.is_empty() && self.connection.is_none()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(untagged)]
enum ConnectionDelta {
    Set(ConnectionSnapshot),
    Cleared,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct CardDelta {
    card: usize,
    actual: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    convention: Option<String>,
    /// Complete current flag set after this turn.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    flags: Vec<&'static str>,
    /// Flags present before this turn that are no longer present.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    removed_flags: Vec<&'static str>,
}

impl CardDelta {
    fn between(before: Option<&CardSnapshot>, after: &CardSnapshot) -> Option<Self> {
        if before == Some(after) {
            return None;
        }
        let removed_flags = before.map_or_else(Vec::new, |before| {
            before
                .flags
                .iter()
                .copied()
                .filter(|flag| !after.flags.contains(flag))
                .collect()
        });
        Some(Self {
            card: after.card,
            actual: after.actual.clone(),
            convention: after.convention.clone(),
            flags: after.flags.clone(),
            removed_flags,
        })
    }

    fn state(&self) -> CardSnapshot {
        CardSnapshot {
            card: self.card,
            actual: self.actual.clone(),
            convention: self.convention.clone(),
            flags: self.flags.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct CardSnapshot {
    card: usize,
    /// Simulator truth, included only to make human review convenient.
    actual: String,
    /// H-Group identities, omitted when ordinary logical deduction agrees.
    #[serde(skip_serializing_if = "Option::is_none")]
    convention: Option<String>,
    /// Stable semantic states that currently apply to the card.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    flags: Vec<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionSnapshot {
    card: usize,
    identity: String,
    kind: &'static str,
    focus: usize,
}

#[test]
fn optimized_expert_replay_owner_superpositions_match_snapshot() {
    let replay = expert_replay_194321();
    let actual = render_snapshot(&replay);
    let path = snapshot_path();

    if std::env::var_os(UPDATE_ENVIRONMENT_VARIABLE).is_some() {
        fs::create_dir_all(path.parent().expect("snapshot has a parent directory"))
            .expect("snapshot directory can be created");
        fs::write(&path, &actual).expect("snapshot can be updated");
    }

    let expected = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "cannot read {}: {error}; regenerate with {UPDATE_ENVIRONMENT_VARIABLE}=1 cargo test -p hanabi-search optimized_expert_replay_owner_superpositions_match_snapshot",
            path.display()
        )
    });
    assert_eq!(
        actual, expected,
        "owner-relative convention superpositions or semantic states changed; review the semantic diff, then regenerate with {UPDATE_ENVIRONMENT_VARIABLE}=1 if it is intentional"
    );
}

#[test]
fn superposition_deltas_reconstruct_every_replay_position() {
    let replay = expert_replay_194321();
    let action_count = u32::try_from(replay.actions.len()).expect("replay length fits in u32");
    let mut reconstructed = player_states(&replay, 0);

    for turn in 1..=action_count {
        let expected = player_states(&replay, turn);
        let delta = position_delta(&reconstructed, &expected);
        apply_position_delta(&mut reconstructed, &delta);
        assert_eq!(
            reconstructed, expected,
            "delta after turn {turn} reconstructs the complete position"
        );
    }
}

fn render_snapshot(replay: &HanabiLiveReplay) -> String {
    let action_count = u32::try_from(replay.actions.len()).expect("replay length fits in u32");
    let states = (0..=action_count)
        .map(|turn| player_states(replay, turn))
        .collect::<Vec<_>>();
    let initial = states[0]
        .iter()
        .cloned()
        .enumerate()
        .map(|(player, state)| InitialPlayerSnapshot {
            player,
            hand: state.hand,
            connection: state.connection,
        })
        .collect();
    let turn_deltas = states
        .windows(2)
        .enumerate()
        .map(|(index, positions)| TurnDelta {
            turn: u32::try_from(index + 1).expect("replay length fits in u32"),
            changes: position_delta(&positions[0], &positions[1]),
        })
        .collect();
    let snapshot = ReplayEpistemicSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        source: "game-194321.json",
        convention: "H-Group max",
        initial,
        turn_deltas,
    };
    let mut json = serde_json::to_string_pretty(&snapshot).expect("snapshot is serializable");
    json.push('\n');
    json
}

fn player_states(replay: &HanabiLiveReplay, turn: u32) -> Vec<PlayerStateSnapshot> {
    let state = replay
        .state_at_turn(turn)
        .unwrap_or_else(|error| panic!("replay position {turn} is valid: {error}"));
    replay
        .players
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let player = PlayerId::new(
                u8::try_from(index).expect("standard Hanabi has at most five players"),
            );
            player_snapshot(&state, player)
        })
        .collect()
}

fn position_delta(
    before: &[PlayerStateSnapshot],
    after: &[PlayerStateSnapshot],
) -> Vec<PlayerDelta> {
    assert_eq!(before.len(), after.len(), "player count remains stable");
    before
        .iter()
        .zip(after)
        .enumerate()
        .filter_map(|(player, (before, after))| {
            let cards = after
                .hand
                .iter()
                .filter_map(|card| {
                    CardDelta::between(
                        before.hand.iter().find(|prior| prior.card == card.card),
                        card,
                    )
                })
                .collect();
            let removed_cards = before
                .hand
                .iter()
                .filter(|card| !after.hand.iter().any(|current| current.card == card.card))
                .map(|card| card.card)
                .collect();
            let connection = (before.connection != after.connection).then(|| {
                after
                    .connection
                    .clone()
                    .map_or(ConnectionDelta::Cleared, ConnectionDelta::Set)
            });
            let delta = PlayerDelta {
                player,
                cards,
                removed_cards,
                connection,
            };
            (!delta.is_empty()).then_some(delta)
        })
        .collect()
}

fn apply_position_delta(states: &mut [PlayerStateSnapshot], changes: &[PlayerDelta]) {
    for change in changes {
        let state = &mut states[change.player];
        state
            .hand
            .retain(|card| !change.removed_cards.contains(&card.card));
        for changed_card in &change.cards {
            if let Some(card) = state
                .hand
                .iter_mut()
                .find(|card| card.card == changed_card.card)
            {
                *card = changed_card.state();
            } else {
                state.hand.push(changed_card.state());
            }
        }
        if let Some(connection) = &change.connection {
            match connection {
                ConnectionDelta::Set(connection) => state.connection = Some(connection.clone()),
                ConnectionDelta::Cleared => state.connection = None,
            }
        }
    }
}

fn player_snapshot(state: &FullState, player: PlayerId) -> PlayerStateSnapshot {
    let view = state.view_for(player).expect("fixture player has a view");
    let deductions = LogicalDeductions::new(view.clone()).expect("fixture view is logical");
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    let gotten = inferred.gotten();
    let visible_player = PlayerId::new(
        u8::try_from((player.index() + 1) % view.hands.len())
            .expect("standard Hanabi has at most five players"),
    );
    let truth_view = state
        .view_for(visible_player)
        .expect("another player can see this hand");
    let hand = view.hands[player.index()]
        .iter()
        .map(|observed| {
            let note = inferred
                .cards
                .iter()
                .find(|note| note.card == observed.id)
                .expect("every own card has convention inference");
            let logical = deductions
                .possible_identities(observed.id)
                .expect("every own card has a logical domain");
            let actual = truth_view.hands[player.index()]
                .iter()
                .find(|visible| visible.id == observed.id)
                .and_then(|visible| visible.identity)
                .expect("another player sees the card identity");
            let trash = note.identity_status != HGroupIdentityStatus::Provisional
                && !note.identities.is_empty()
                && note
                    .identities
                    .iter()
                    .all(|identity| is_convention_trash(&view, identity, &gotten, &inferred.cards));
            let mut flags = Vec::new();
            flags.extend(note.focused.then_some("focused"));
            flags.extend(
                (note.identity_status == HGroupIdentityStatus::Provisional)
                    .then_some("provisional"),
            );
            flags.extend(
                inferred
                    .playable_now
                    .contains(&observed.id)
                    .then_some("playable"),
            );
            flags.extend(trash.then_some("trash"));
            flags.extend(note.saved.then_some("saved"));
            flags.extend(note.finessed.then_some("finessed"));
            flags.extend((inferred.chops[player.index()] == Some(observed.id)).then_some("chop"));
            flags.extend(
                inferred
                    .chop_moved
                    .contains(&observed.id)
                    .then_some("chop-moved"),
            );
            flags.extend(
                inferred
                    .discard_now
                    .contains(&observed.id)
                    .then_some("discard-now"),
            );
            CardSnapshot {
                card: observed.id.index(),
                actual: identity_label(actual),
                convention: (note.identities != logical).then(|| identity_labels(note.identities)),
                flags,
            }
        })
        .collect();
    let connection = inferred.connection.map(|connection| ConnectionSnapshot {
        card: connection.card.index(),
        identity: identity_label(connection.identity),
        kind: match connection.kind {
            HGroupConnectionKind::Prompt => "prompt",
            HGroupConnectionKind::Finesse => "finesse",
        },
        focus: connection.focus.index(),
    });
    PlayerStateSnapshot { hand, connection }
}

fn identity_labels(identities: IdentitySet) -> String {
    identities
        .iter()
        .map(identity_label)
        .collect::<Vec<_>>()
        .join(" ")
}

fn identity_label(identity: Card) -> String {
    let suit = match identity.suit {
        Suit::Red => 'r',
        Suit::Yellow => 'y',
        Suit::Green => 'g',
        Suit::Blue => 'b',
        Suit::Purple => 'p',
    };
    format!("{suit}{}", identity.rank.number())
}

fn snapshot_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/h_group/tests/fixtures/game-194321-superpositions.json")
}
