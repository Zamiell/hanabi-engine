use std::{path::PathBuf, process::Command};

#[test]
fn help_describes_turn_semantics_and_search_modes() {
    let output = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .arg("--help")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("turn 0 is the initial deal"));
    assert!(stdout.contains("--mode <ismcts|flat>"));
    assert!(stdout.contains("--convention <none>"));
    assert!(stdout.contains("hanabi-engine benchmark"));
    assert!(stdout.contains("versioned JSON report"));
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
            "h-group",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("unknown convention \"h-group\"; expected none"));
}

#[test]
fn analyzes_a_real_hanabi_live_prefix() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../hanabi-live/packages/client/test_data/no_variant.json");
    if !fixture.exists() {
        return;
    }

    let output = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .args([
            "analyze",
            fixture.to_str().unwrap(),
            "--turn",
            "17",
            "--iterations",
            "8",
            "--seed",
            "42",
        ])
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
    assert!(stdout.contains("Search: ISMCTS, 8 iterations, seed 42"));
    assert!(stdout.contains("Visits"));
    assert!(stdout.contains("Official"));
    assert!(stdout.contains("Raw"));
    assert!(stdout.contains("Utility"));
    assert!(stdout.contains("slot 1 is newest"));

    let flat = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .args([
            "analyze",
            fixture.to_str().unwrap(),
            "--turn",
            "17",
            "--mode",
            "flat",
            "--samples",
            "2",
            "--convention",
            "none",
            "--seed",
            "42",
        ])
        .output()
        .unwrap();
    assert!(
        flat.status.success(),
        "{}",
        String::from_utf8_lossy(&flat.stderr)
    );
    let stdout = String::from_utf8(flat.stdout).unwrap();
    assert!(stdout.contains("Search: flat Monte Carlo, 2 samples/action"));
    assert!(stdout.contains("Official"));
    assert!(stdout.contains("Raw"));
    assert!(stdout.contains("Utility"));
    assert!(stdout.contains("Variance"));
}

#[test]
fn benchmarks_both_search_modes_with_reproducible_trials() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../hanabi-live/packages/client/test_data/no_variant.json");
    if !fixture.exists() {
        return;
    }

    let output = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .args([
            "benchmark",
            fixture.to_str().unwrap(),
            "--turn",
            "17",
            "--trials",
            "2",
            "--convention",
            "none",
            "--iterations",
            "8",
            "--samples",
            "2",
            "--seed",
            "42",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], 2);
    assert_eq!(report["policy"], "convention_agnostic");
    assert_eq!(report["convention"], "none");
    assert_eq!(report["base_seed"], 42);
    let position = &report["positions"][0];
    assert_eq!(position["turn"], 17);
    assert_eq!(position["actor"], "Cathy");
    assert_eq!(position["starting_score"], 7);

    let searches = position["searches"].as_array().unwrap();
    assert_eq!(searches.len(), 2);
    for (search, mode, work_units, raw_scores, expected_diagnostics) in [
        (&searches[0], "ismcts", 8, [10.0, 10.0], [8, 0, 8, 8, 8, 1]),
        (&searches[1], "flat", 16, [10.5, 8.5], [2, 16, 0, 16, 16, 1]),
    ] {
        assert_eq!(search["mode"], mode);
        assert_eq!(search["trial_count"], 2);
        assert_eq!(search["action_stability"], 0.5);
        assert!(
            search["aggregate_throughput_per_second"]
                .as_f64()
                .is_some_and(|value| value > 0.0)
        );
        let trials = search["trials"].as_array().unwrap();
        assert_eq!(trials[0]["seed"], 42);
        assert_eq!(trials[1]["seed"], 43);
        assert_eq!(trials[0]["selected_action"]["key"], "play:22");
        assert_eq!(trials[1]["selected_action"]["key"], "play:20");
        for (trial, raw_score) in trials.iter().zip(raw_scores) {
            assert_eq!(trial["mean_official_score"], 0.0);
            assert_eq!(trial["mean_raw_score"], raw_score);
            assert_eq!(trial["mean_utility"], raw_score);
            assert_eq!(trial["strikeout_rate"], 1.0);
            assert_eq!(trial["work_units"], work_units);
            let diagnostics = &trial["diagnostics"];
            for (field, expected) in [
                ("worlds_sampled", expected_diagnostics[0]),
                ("candidate_state_clones", expected_diagnostics[1]),
                ("tree_nodes_expanded", expected_diagnostics[2]),
                ("search_actions_applied", expected_diagnostics[3]),
                ("rollouts", expected_diagnostics[4]),
                ("max_tree_depth", expected_diagnostics[5]),
            ] {
                assert_eq!(diagnostics[field], expected);
            }
            assert!(
                diagnostics["rollout_turns"].as_u64().unwrap()
                    > diagnostics["rollouts"].as_u64().unwrap()
            );
            let timing = &diagnostics["timing_seconds"];
            let accounted = timing["sampling"].as_f64().unwrap()
                + timing["tree"].as_f64().unwrap()
                + timing["rollout"].as_f64().unwrap();
            assert!((timing["total"].as_f64().unwrap() - accounted).abs() < 1e-9);
            let rollout_accounted = timing["rollout_observation"].as_f64().unwrap()
                + timing["rollout_deduction"].as_f64().unwrap()
                + timing["rollout_policy"].as_f64().unwrap()
                + timing["rollout_apply"].as_f64().unwrap()
                + timing["rollout_other"].as_f64().unwrap();
            assert!((timing["rollout"].as_f64().unwrap() - rollout_accounted).abs() < 1e-9);
        }
    }
}
