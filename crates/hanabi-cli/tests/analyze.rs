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
    assert!(stdout.contains("Search: ISMCTS, 8 iterations, seed 42"));
    assert!(stdout.contains("Visits"));
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
    assert!(stdout.contains("Variance"));
}
