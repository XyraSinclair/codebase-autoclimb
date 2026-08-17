//! Integration test: scan a fixture Python project and verify state.json.
//!
//! Each test copies the fixture to a unique temp directory to avoid
//! parallel test interference.

use std::path::{Path, PathBuf};
use std::process::Command;

fn cli_bin() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_autoclimb") {
        return PathBuf::from(path);
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");
    let bin = workspace_root
        .join("target")
        .join("debug")
        .join(format!("autoclimb{}", std::env::consts::EXE_SUFFIX));
    if !bin.exists() {
        let status = Command::new("cargo")
            .args(["build", "--bin", "autoclimb"])
            .current_dir(workspace_root)
            .status()
            .expect("build canonical rust cli");
        assert!(status.success(), "cargo build --bin autoclimb failed");
    }
    bin
}

/// Source fixture path.
fn fixture_source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("sample_python")
}

/// Copy fixture to a temp directory and return the temp dir path.
fn copy_fixture() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("create tempdir");
    copy_dir_recursive(&fixture_source(), tmp.path()).expect("copy fixture");
    tmp
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

/// Run autoclimb scan on a temp copy of the fixture, return state.json as Value.
fn run_scan_in_tmp() -> (serde_json::Value, String, tempfile::TempDir) {
    let tmp = copy_fixture();
    let root = tmp.path();
    let state_file = root.join(".autoclimb").join("state.json");

    let output = Command::new(cli_bin())
        .args(["scan", "--lang", "python", "--path"])
        .arg(root)
        .output()
        .expect("failed to execute autoclimb");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    assert!(
        output.status.success(),
        "scan failed (exit {:?}):\nstdout: {stdout}\nstderr: {stderr}",
        output.status.code()
    );

    let state_json = std::fs::read_to_string(&state_file)
        .unwrap_or_else(|e| panic!("state.json at {}: {e}", state_file.display()));
    let state: serde_json::Value =
        serde_json::from_str(&state_json).expect("state.json is not valid JSON");

    (state, stdout, tmp)
}

#[test]
fn scan_produces_valid_state() {
    let (state, _stdout, _tmp) = run_scan_in_tmp();

    assert_eq!(state["version"], 1);
    assert!(state["scan_count"].as_u64().unwrap() >= 1);
    assert!(state["findings"].is_object());
    assert!(state["potentials"].is_object());
}

#[test]
fn scan_produces_findings() {
    let (state, _stdout, _tmp) = run_scan_in_tmp();

    let findings = state["findings"].as_object().expect("findings is object");
    assert!(
        !findings.is_empty(),
        "expected at least one finding from fixture project"
    );

    let detectors: std::collections::BTreeSet<String> = findings
        .values()
        .map(|f| f["detector"].as_str().unwrap().to_string())
        .collect();

    assert!(
        detectors.contains("unused") || detectors.contains("smells"),
        "expected unused or smells detector findings, got: {detectors:?}"
    );
}

#[test]
fn scan_scores_in_valid_range() {
    let (state, _stdout, _tmp) = run_scan_in_tmp();

    let overall = state["overall_score"].as_f64().expect("overall_score");
    let objective = state["objective_score"].as_f64().expect("objective_score");
    let strict = state["strict_score"].as_f64().expect("strict_score");
    let verified = state["verified_strict_score"]
        .as_f64()
        .expect("verified_strict_score");

    for (name, score) in [
        ("overall", overall),
        ("objective", objective),
        ("strict", strict),
        ("verified_strict", verified),
    ] {
        assert!(
            (0.0..=100.0).contains(&score),
            "{name} score {score} out of range [0, 100]"
        );
    }

    assert!(
        overall < 100.0,
        "expected overall < 100 with findings present, got {overall}"
    );
}

#[test]
fn scan_has_dimension_scores() {
    let (state, _stdout, _tmp) = run_scan_in_tmp();

    if let Some(dims) = state.get("dimension_scores") {
        let dims = dims.as_object().expect("dimension_scores is object");
        for (dim_name, dim_val) in dims {
            let score = dim_val["score"].as_f64().unwrap_or(-1.0);
            assert!(
                (0.0..=100.0).contains(&score),
                "dimension {dim_name} score {score} out of range"
            );
        }
    }
}

#[test]
fn scan_has_potentials() {
    let (state, _stdout, _tmp) = run_scan_in_tmp();

    let potentials = state["potentials"]
        .as_object()
        .expect("potentials is object");
    assert!(
        !potentials.is_empty(),
        "expected at least one detector potential"
    );

    for (det, val) in potentials {
        val.as_u64()
            .unwrap_or_else(|| panic!("potential for {det} is not a u64: {val}"));
    }
}

#[test]
fn scan_finding_fields_complete() {
    let (state, _stdout, _tmp) = run_scan_in_tmp();

    let findings = state["findings"].as_object().expect("findings is object");

    for (id, finding) in findings {
        assert!(finding["id"].is_string(), "finding {id} missing 'id'");
        assert!(
            finding["detector"].is_string(),
            "finding {id} missing 'detector'"
        );
        assert!(finding["file"].is_string(), "finding {id} missing 'file'");
        assert!(
            finding["status"].is_string(),
            "finding {id} missing 'status'"
        );
        assert!(
            finding["confidence"].is_string(),
            "finding {id} missing 'confidence'"
        );
        assert!(
            finding["summary"].is_string(),
            "finding {id} missing 'summary'"
        );

        let status = finding["status"].as_str().unwrap();
        assert!(
            ["open", "fixed", "wontfix", "false_positive"].contains(&status),
            "finding {id} has unknown status: {status}"
        );
    }
}

#[test]
fn scan_idempotent_on_second_run() {
    let tmp = copy_fixture();
    let root = tmp.path();
    let state_file = root.join(".autoclimb").join("state.json");

    // First scan
    let output1 = Command::new(cli_bin())
        .args(["scan", "--lang", "python", "--path"])
        .arg(root)
        .output()
        .expect("first scan failed");
    assert!(
        output1.status.success(),
        "first scan failed: {}",
        String::from_utf8_lossy(&output1.stderr)
    );

    let state1: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&state_file).unwrap()).unwrap();

    // Second scan
    let output2 = Command::new(cli_bin())
        .args(["scan", "--lang", "python", "--path"])
        .arg(root)
        .output()
        .expect("second scan failed");
    assert!(
        output2.status.success(),
        "second scan failed: {}",
        String::from_utf8_lossy(&output2.stderr)
    );

    let state2: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&state_file).unwrap()).unwrap();

    // scan_count should increment
    assert_eq!(
        state2["scan_count"].as_u64().unwrap(),
        state1["scan_count"].as_u64().unwrap() + 1
    );

    // Finding count should be the same (idempotent)
    let count1 = state1["findings"].as_object().unwrap().len();
    let count2 = state2["findings"].as_object().unwrap().len();
    assert_eq!(
        count1, count2,
        "finding count changed on second scan: {count1} -> {count2}"
    );

    // Scores should be identical
    assert_eq!(state1["overall_score"], state2["overall_score"]);
}
