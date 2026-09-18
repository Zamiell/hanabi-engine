use std::process::Command;

fn main() {
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    println!(
        "cargo:rustc-env=HANABI_BUILD_REVISION={}",
        git(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_owned())
    );
    println!(
        "cargo:rustc-env=HANABI_BUILD_DIRTY={}",
        git(&["status", "--porcelain"]).map_or_else(
            || "unknown".to_owned(),
            |status| (!status.is_empty()).to_string()
        )
    );
    for path in [
        "src",
        "../hanabi-search/src",
        "../hanabi-core/src",
        "../hanabi-protocol/src",
        "../../Cargo.lock",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    for name in ["HEAD", "index"] {
        if let Some(path) = git(&["rev-parse", "--git-path", name]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    if let Some(reference) = git(&["symbolic-ref", "-q", "HEAD"]) {
        if let Some(path) = git(&["rev-parse", "--git-path", &reference]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
}
