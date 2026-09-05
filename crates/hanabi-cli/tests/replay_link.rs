use std::{
    path::PathBuf,
    process::{Command, Output},
};

use hanabi_protocol::HanabiLiveReplay;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../hanabi-search/tests/fixtures/self-play-p4v0s10-fix.json")
}

fn run(turn: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .arg("replay-link")
        .arg(fixture())
        .args(["--turn", turn])
        .output()
        .unwrap()
}

#[test]
fn seed_replay_link_matches_hanab_live_codec_and_turn_number() {
    let output = run("23");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "https://hanab.live/shared-replay-json/415howrnrhjibpqadkaq-glbsvtsfvpnulfyxumcc-kxeiwkagpufdm,02fcla-dkdpoblddqdolcefdsob-ddduejdtidicdydrdwrc-,0#23"
    );
}

#[test]
fn link_round_trips_every_expert_replay_deck_and_action() {
    const BASE62: &str = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    for seed in ["p4v0s415", "p4v0s9", "p4v0s2", "p4v0s3", "p4v0s1"] {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../hanabi-protocol/tests/fixtures/game-{seed}.json"
        ));
        let replay = HanabiLiveReplay::from_json(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
            .arg("replay-link")
            .arg(path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{seed}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let url = String::from_utf8(output.stdout).unwrap();
        let payload = url
            .trim()
            .strip_prefix("https://hanab.live/shared-replay-json/")
            .unwrap()
            .strip_suffix("#1")
            .unwrap()
            .replace('-', "");
        let parts = payload.split(',').collect::<Vec<_>>();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[2], "0");
        assert_eq!(
            parts[0][..1].parse::<usize>().unwrap(),
            replay.players.len()
        );
        assert_eq!(&parts[0][1..3], "15");
        let cards = parts[0][3..]
            .chars()
            .map(|c| BASE62.find(c).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(cards.len(), replay.deck.len());
        for (encoded, card) in cards.iter().zip(&replay.deck) {
            assert_eq!(*encoded / 5, usize::from(card.suit_index));
            assert_eq!(*encoded % 5 + 1, usize::from(card.rank));
        }
        let min = parts[1][..1].parse::<usize>().unwrap();
        let range = parts[1][1..2].parse::<usize>().unwrap() - min + 1;
        let actions = parts[1].as_bytes()[2..].chunks_exact(2).collect::<Vec<_>>();
        assert_eq!(actions.len(), replay.actions.len());
        for (encoded, action) in actions.iter().zip(&replay.actions) {
            let code = BASE62.find(char::from(encoded[0])).unwrap();
            assert_eq!(code % range + min, usize::from(action.action_type.code()));
            assert_eq!(code / range - 1, usize::from(action.value));
            assert_eq!(BASE62.find(char::from(encoded[1])).unwrap(), action.target);
        }
    }
}

#[test]
fn invalid_turns_fail_without_printing_a_link() {
    for turn in ["0", "24", "-1", "not-a-turn"] {
        let output = run(turn);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn missing_replay_fails_without_printing_a_link() {
    let output = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .args(["replay-link", "nonexistent-replay.json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn supports_empty_and_single_action_prefixes_but_rejects_illegal_actions() {
    let path = std::env::temp_dir().join(format!("hanabi-replay-link-{}.json", std::process::id()));
    let mut json = serde_json::json!({
        "seed": "p4v0s10", "players": ["Alice", "Bob", "Cathy", "Donald"], "actions": []
    });
    for (actions, expected) in [
        (serde_json::json!([]), Some(",00,0#1")),
        (
            serde_json::json!([{"type":2,"target":2,"value":0}]),
            Some(",22bc,0#1"),
        ),
        // #10 belongs to Cathy, not Alice, who acts first.
        (serde_json::json!([{"type":0,"target":10}]), None),
    ] {
        json["actions"] = actions;
        std::fs::write(&path, json.to_string()).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
            .arg("replay-link")
            .arg(&path)
            .output()
            .unwrap();
        if let Some(suffix) = expected {
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                String::from_utf8(output.stdout)
                    .unwrap()
                    .trim()
                    .replace('-', "")
                    .ends_with(suffix)
            );
        } else {
            assert!(!output.status.success());
            assert!(output.stdout.is_empty());
        }
    }
    std::fs::remove_file(path).unwrap();
}
