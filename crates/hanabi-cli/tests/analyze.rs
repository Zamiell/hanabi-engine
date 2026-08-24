use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

fn live_snapshot() -> serde_json::Value {
    serde_json::json!({
        "tableID": 17,
        "playerNames": ["Bot", "Alice"],
        "ourPlayerIndex": 0,
        "spectating": false,
        "replay": false,
        "options": {"variantName": "No Variant"},
        "actions": [
            {"type": "draw", "playerIndex": 0, "order": 0, "suitIndex": -1, "rank": -1},
            {"type": "draw", "playerIndex": 0, "order": 1, "suitIndex": -1, "rank": -1},
            {"type": "draw", "playerIndex": 0, "order": 2, "suitIndex": -1, "rank": -1},
            {"type": "draw", "playerIndex": 0, "order": 3, "suitIndex": -1, "rank": -1},
            {"type": "draw", "playerIndex": 0, "order": 4, "suitIndex": -1, "rank": -1},
            {"type": "draw", "playerIndex": 1, "order": 5, "suitIndex": 0, "rank": 1},
            {"type": "draw", "playerIndex": 1, "order": 6, "suitIndex": 0, "rank": 1},
            {"type": "draw", "playerIndex": 1, "order": 7, "suitIndex": 0, "rank": 1},
            {"type": "draw", "playerIndex": 1, "order": 8, "suitIndex": 0, "rank": 2},
            {"type": "draw", "playerIndex": 1, "order": 9, "suitIndex": 0, "rank": 2},
            {
                "type": "clue",
                "clue": {"type": 1, "value": 1},
                "giver": 0,
                "list": [5, 6, 7],
                "target": 1,
                "turn": 0
            },
            {"type": "status", "clues": 7, "score": 0, "maxScore": 25},
            {"type": "turn", "num": 1, "currentPlayerIndex": 1},
            {"type": "play", "playerIndex": 1, "order": 5, "suitIndex": 0, "rank": 1},
            {"type": "draw", "playerIndex": 1, "order": 10, "suitIndex": 2, "rank": 1},
            {"type": "status", "clues": 7, "score": 1, "maxScore": 25},
            {"type": "turn", "num": 2, "currentPlayerIndex": 0}
        ]
    })
}

fn traced_opening_snapshot() -> serde_json::Value {
    serde_json::json!({
        "tableID": 39,
        "playerNames": ["hanabi-engine", "red_hedgehog", "James"],
        "ourPlayerIndex": 0,
        "spectating": false,
        "replay": false,
        "options": {"variantName": "No Variant"},
        "actions": [
            {"type": "draw", "playerIndex": 0, "order": 0, "suitIndex": -1, "rank": -1},
            {"type": "draw", "playerIndex": 0, "order": 1, "suitIndex": -1, "rank": -1},
            {"type": "draw", "playerIndex": 0, "order": 2, "suitIndex": -1, "rank": -1},
            {"type": "draw", "playerIndex": 0, "order": 3, "suitIndex": -1, "rank": -1},
            {"type": "draw", "playerIndex": 0, "order": 4, "suitIndex": -1, "rank": -1},
            {"type": "draw", "playerIndex": 1, "order": 5, "suitIndex": 0, "rank": 2},
            {"type": "draw", "playerIndex": 1, "order": 6, "suitIndex": 1, "rank": 3},
            {"type": "draw", "playerIndex": 1, "order": 7, "suitIndex": 3, "rank": 3},
            {"type": "draw", "playerIndex": 1, "order": 8, "suitIndex": 0, "rank": 1},
            {"type": "draw", "playerIndex": 1, "order": 9, "suitIndex": 0, "rank": 3},
            {"type": "draw", "playerIndex": 2, "order": 10, "suitIndex": 4, "rank": 3},
            {"type": "draw", "playerIndex": 2, "order": 11, "suitIndex": 0, "rank": 1},
            {"type": "draw", "playerIndex": 2, "order": 12, "suitIndex": 0, "rank": 1},
            {"type": "draw", "playerIndex": 2, "order": 13, "suitIndex": 3, "rank": 1},
            {"type": "draw", "playerIndex": 2, "order": 14, "suitIndex": 1, "rank": 1}
        ]
    })
}

#[test]
fn deterministic_planner_selects_the_rank_two_opening() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .args(["live-action", "--include-planning-details"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "{}", traced_opening_snapshot()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["action"]["type"], 3);
    assert_eq!(response["action"]["target"], 1);
    assert_eq!(response["action"]["value"], 1);
    assert_eq!(response["planning"]["phase"], "symbolic");
    assert_eq!(response["planning"]["worldCountExact"], false);
}

#[test]
fn help_describes_turn_semantics_and_planner_options() {
    let output = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .arg("--help")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("turn 0 is the initial deal"));
    assert!(stdout.contains("--convention <none|h-group>"));
    assert!(stdout.contains("--h-group-level <1-25|max>"));
    assert!(stdout.contains("hanabi-engine live-action"));
    assert!(stdout.contains("hanabi-engine live-session"));
    assert!(stdout.contains("Convention framework (default: h-group)"));
    assert!(stdout.contains("H-Group profile (default: max)"));
}

#[test]
fn live_action_defaults_to_h_group_max_and_emits_server_json() {
    let snapshot = live_snapshot().to_string();
    let mut child = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .arg("live-action")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(snapshot.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let command: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(command["tableID"], 17);
    assert!(command["type"].as_u64().is_some_and(|kind| kind <= 3));
    assert!(command["target"].as_u64().is_some());
}

#[test]
fn live_session_serves_multiple_requests_from_one_process() {
    let initialize = serde_json::json!({
        "kind": "initialize",
        "snapshot": live_snapshot(),
    });
    let append = serde_json::json!({
        "kind": "append",
        "tableID": 17,
        "actions": [],
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .arg("live-session")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut input = child.stdin.take().unwrap();
        writeln!(input, "{initialize}").unwrap();
        writeln!(input, "{append}").unwrap();
    }
    let output = child.wait_with_output().unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = String::from_utf8(output.stdout).unwrap();
    let responses = responses
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), 2);
    assert!(responses.iter().all(|response| response["tableID"] == 17));
    assert!(
        responses
            .iter()
            .all(|response| response.get("error").is_none())
    );
}

#[test]
fn live_session_can_emit_player_safe_planning_details() {
    let initialize = serde_json::json!({
        "kind": "initialize",
        "snapshot": live_snapshot(),
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .args(["live-session", "--include-planning-details"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{initialize}").unwrap();
    let output = child.wait_with_output().unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["action"]["tableID"], 17);
    assert_eq!(response["planning"]["phase"], "symbolic");
    assert!(response["planning"]["rootActions"].is_array());
    assert!(response["logicalDeductions"]["ownCards"].is_array());
    assert_eq!(response["conventionInferences"]["framework"], "h-group");
}

#[test]
fn rejects_an_unregistered_convention() {
    let output = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .args([
            "analyze",
            "unused.json",
            "--turn",
            "0",
            "--convention",
            "rainbow",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("unknown convention \"rainbow\"; expected none or h-group"));
}

#[test]
fn validates_h_group_profile_options_before_reading_a_replay() {
    let missing = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .args([
            "analyze",
            "unused.json",
            "--turn",
            "0",
            "--convention",
            "h-group",
        ])
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(
        String::from_utf8(missing.stderr)
            .unwrap()
            .contains("--h-group-level is required when --convention h-group")
    );

    let irrelevant = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .args([
            "analyze",
            "unused.json",
            "--turn",
            "0",
            "--h-group-level",
            "5",
        ])
        .output()
        .unwrap();
    assert!(!irrelevant.status.success());
    assert!(
        String::from_utf8(irrelevant.stderr)
            .unwrap()
            .contains("--h-group-level requires --convention h-group")
    );

    let out_of_range = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .args([
            "analyze",
            "unused.json",
            "--turn",
            "0",
            "--convention",
            "h-group",
            "--h-group-level",
            "26",
        ])
        .output()
        .unwrap();
    assert!(!out_of_range.status.success());
    assert!(
        String::from_utf8(out_of_range.stderr)
            .unwrap()
            .contains("expected 1 through 25, or max")
    );
}

#[test]
fn analyzes_a_real_hanabi_live_prefix() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../hanabi-live/packages/client/test_data/no_variant.json");
    if !fixture.exists() {
        return;
    }

    let output = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .args(["analyze", fixture.to_str().unwrap(), "--turn", "17"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Turn: 17  Actor: Cathy (P2)"));
    assert!(stdout.contains("Convention: none"));
    assert!(stdout.contains("Planning: deterministic"));
    assert!(stdout.contains("consistent worlds"));
    assert!(stdout.contains("slot 1 is newest"));

    let h_group = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .args([
            "analyze",
            fixture.to_str().unwrap(),
            "--turn",
            "17",
            "--convention",
            "h-group",
            "--h-group-level",
            "5",
        ])
        .output()
        .unwrap();
    assert!(
        h_group.status.success(),
        "{}",
        String::from_utf8_lossy(&h_group.stderr)
    );
    let stdout = String::from_utf8(h_group.stdout).unwrap();
    assert!(stdout.contains("Convention: h-group (level 5)"));
    assert!(
        stdout.contains("Convention ruleset revision: 1ef83242d71c62f2db6422f09e83abddba9611dd")
    );
}
