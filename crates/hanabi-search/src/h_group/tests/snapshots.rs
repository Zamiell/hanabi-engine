use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::*;

const SNAPSHOT_SCHEMA_VERSION: u8 = 1;
const UPDATE_ENVIRONMENT_VARIABLE: &str = "HANABI_UPDATE_SUPERPOSITIONS";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReplayEpistemicSnapshot {
    schema_version: u8,
    source: &'static str,
    convention: &'static str,
    identity_legend: IdentityLegend,
    positions: Vec<PositionSnapshot>,
}

#[derive(Serialize)]
struct IdentityLegend {
    r: &'static str,
    y: &'static str,
    g: &'static str,
    b: &'static str,
    p: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PositionSnapshot {
    /// Number of completed player actions. Turn zero is the initial deal.
    turn: u32,
    /// One-based move about to be played; absent after the final action.
    #[serde(skip_serializing_if = "Option::is_none")]
    before_move: Option<u32>,
    players: Vec<PlayerSnapshot>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlayerSnapshot {
    player_index: usize,
    player_name: String,
    hand: Vec<CardSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    connection: Option<ConnectionSnapshot>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CardSnapshot {
    card: usize,
    /// Simulator truth, included only to make human review convenient.
    actual: String,
    /// Identities allowed by direct clues, visible cards, and card counts.
    logical: String,
    /// Identities retained after applying H-Group convention semantics.
    convention: String,
    /// Stable semantic states that currently apply to the card.
    flags: Vec<&'static str>,
}

#[derive(Serialize)]
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
        "owner-relative logical or convention superpositions changed; review the semantic diff, then regenerate with {UPDATE_ENVIRONMENT_VARIABLE}=1 if it is intentional"
    );
}

fn render_snapshot(replay: &HanabiLiveReplay) -> String {
    let action_count = u32::try_from(replay.actions.len()).expect("replay length fits in u32");
    let positions = (0..=action_count)
        .map(|turn| position_snapshot(replay, turn, action_count))
        .collect();
    let snapshot = ReplayEpistemicSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        source: "game-194321.json",
        convention: "H-Group max",
        identity_legend: IdentityLegend {
            r: "red",
            y: "yellow",
            g: "green",
            b: "blue",
            p: "purple",
        },
        positions,
    };
    let mut json = serde_json::to_string_pretty(&snapshot).expect("snapshot is serializable");
    json.push('\n');
    json
}

fn position_snapshot(replay: &HanabiLiveReplay, turn: u32, action_count: u32) -> PositionSnapshot {
    let state = replay
        .state_at_turn(turn)
        .unwrap_or_else(|error| panic!("replay position {turn} is valid: {error}"));
    let players = replay
        .players
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let player = PlayerId::new(
                u8::try_from(index).expect("standard Hanabi has at most five players"),
            );
            player_snapshot(&state, player, name)
        })
        .collect();
    PositionSnapshot {
        turn,
        before_move: (turn < action_count).then_some(turn + 1),
        players,
    }
}

fn player_snapshot(state: &FullState, player: PlayerId, name: &str) -> PlayerSnapshot {
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
            let trash = !note.identities.is_empty()
                && note
                    .identities
                    .iter()
                    .all(|identity| is_convention_trash(&view, identity, &gotten, &inferred.cards));
            let mut flags = Vec::new();
            flags.extend(note.focused.then_some("focused"));
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
                logical: identity_labels(logical),
                convention: identity_labels(note.identities),
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
    PlayerSnapshot {
        player_index: player.index(),
        player_name: name.to_owned(),
        hand,
        connection,
    }
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
