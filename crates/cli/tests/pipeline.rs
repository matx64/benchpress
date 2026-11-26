use std::path::Path;
use std::process::Command;

#[test]
fn fixture_suite_runs_isolated_from_user_build_config() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let data = tempfile::tempdir().expect("tempdir");
    let out = Command::new(env!("CARGO_BIN_EXE_benchpress"))
        .args(["run", "--json", "--yes", "--cooldown", "0", "--suite-file"])
        .arg(root.join("fixture-tiny@1.toml"))
        .arg("--source-dir")
        .arg(root.join("tiny"))
        .env("BENCHPRESS_DATA_DIR", data.path())
        .env("RUSTC_WRAPPER", "/bin/false")
        .env("RUSTFLAGS", "--bogus")
        .env("CARGO_BUILD_RUSTFLAGS", "--bogus")
        .env("CARGO_TARGET_DIR", "/nonexistent/benchpress-test")
        .output()
        .expect("run benchpress");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "benchpress failed:\n{stderr}");

    let json = String::from_utf8(out.stdout).expect("utf-8 stdout");
    let result: benchpress_core::BenchResult =
        serde_json::from_str(&json).expect("stdout is only the result JSON");
    let ids: Vec<&str> = result.scenarios.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "clean-debug",
            "incremental-leaf",
            "incremental-core",
            "clean-release"
        ]
    );
    assert!(
        result
            .scenarios
            .iter()
            .all(|s| s.runs.len() == 2 && s.runs.iter().all(|r| r.exit_ok && r.wall_ms > 0))
    );
    for var in [
        "RUSTC_WRAPPER",
        "RUSTFLAGS",
        "CARGO_BUILD_RUSTFLAGS",
        "CARGO_TARGET_DIR",
    ] {
        assert!(
            result
                .environment
                .overrides_cleared
                .iter()
                .any(|v| v == var),
            "{var} not recorded"
        );
    }
    assert!(
        !json.contains("--bogus") && !json.contains("/nonexistent"),
        "override values leaked into the result"
    );

    let user = std::env::var("USER").unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_default();
    let host = nix_hostname();
    for needle in [
        user.as_str(),
        home.as_str(),
        host.as_str(),
        data.path().to_str().unwrap_or_default(),
    ] {
        if needle.len() >= 3 {
            assert!(!json.contains(needle), "result contains `{needle}`");
        }
    }
    let saved = std::fs::read_dir(data.path().join("results"))
        .expect("results dir")
        .count();
    assert_eq!(saved, 2, "result JSON and samples file are saved locally");
}

fn nix_hostname() -> String {
    Command::new("hostname")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}
