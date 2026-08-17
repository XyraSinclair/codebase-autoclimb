//! Integration tests for CLI commands beyond scan.

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
    let status = Command::new("cargo")
        .args(["build", "--bin", "autoclimb"])
        .current_dir(workspace_root)
        .status()
        .expect("build canonical rust cli");
    assert!(status.success(), "cargo build --bin autoclimb failed");
    bin
}

fn fixture_source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("sample_python")
}

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

fn run_scan(root: &Path) {
    let output = Command::new(cli_bin())
        .args(["scan", "--lang", "python", "--path"])
        .arg(root)
        .output()
        .expect("scan failed");
    assert!(output.status.success(), "scan failed");
}

#[test]
fn status_command_works() {
    let tmp = copy_fixture();
    let root = tmp.path();
    run_scan(root);

    let output = Command::new(cli_bin())
        .args(["status", "--path"])
        .arg(root)
        .output()
        .expect("status failed");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(output.status.success(), "status failed: {stdout}");
    assert!(stdout.contains("Score") || stdout.contains("score"));
}

#[test]
fn show_command_works() {
    let tmp = copy_fixture();
    let root = tmp.path();
    run_scan(root);

    let output = Command::new(cli_bin())
        .args(["show", "--path"])
        .arg(root)
        .output()
        .expect("show failed");

    assert!(output.status.success());
}

#[test]
fn show_json_output() {
    let tmp = copy_fixture();
    let root = tmp.path();
    run_scan(root);

    let output = Command::new(cli_bin())
        .args(["show", "--json", "--path"])
        .arg(root)
        .output()
        .expect("show --json failed");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(output.status.success());
    let _parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("show --json output is valid JSON");
}

#[test]
fn next_command_works() {
    let tmp = copy_fixture();
    let root = tmp.path();
    run_scan(root);

    let output = Command::new(cli_bin())
        .args(["next", "--path"])
        .arg(root)
        .output()
        .expect("next failed");

    assert!(output.status.success());
}

#[test]
fn langs_command_works() {
    let output = Command::new(cli_bin())
        .args(["langs"])
        .output()
        .expect("langs failed");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(output.status.success());
    assert!(stdout.contains("python"));
    assert!(stdout.contains("typescript"));
    assert!(stdout.contains("go"));
    assert!(stdout.contains("rust"));
}

#[test]
fn queue_command_works() {
    let tmp = copy_fixture();
    let root = tmp.path();
    run_scan(root);

    let output = Command::new(cli_bin())
        .args(["queue", "--count", "5", "--path"])
        .arg(root)
        .output()
        .expect("queue failed");

    assert!(output.status.success());
}

#[test]
fn tree_command_works() {
    let tmp = copy_fixture();
    let root = tmp.path();
    run_scan(root);

    let output = Command::new(cli_bin())
        .args(["tree", "--path"])
        .arg(root)
        .output()
        .expect("tree failed");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(output.status.success());
    // Tree output should contain something — at minimum the root node
    assert!(!stdout.is_empty());
}

#[test]
fn viz_command_generates_html() {
    let tmp = copy_fixture();
    let root = tmp.path();
    run_scan(root);

    let html_path = root.join("report.html");

    let output = Command::new(cli_bin())
        .args(["viz", "--output"])
        .arg(&html_path)
        .args(["--path"])
        .arg(root)
        .output()
        .expect("viz failed");

    assert!(output.status.success());
    assert!(html_path.exists(), "HTML report should be created");

    let html = std::fs::read_to_string(&html_path).unwrap();
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("Autoclimb Report"));
}

#[test]
fn exclude_list_initially_empty() {
    let tmp = copy_fixture();
    let root = tmp.path();

    let output = Command::new(cli_bin())
        .args(["exclude", "list", "--path"])
        .arg(root)
        .output()
        .expect("exclude list failed");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(output.status.success());
    assert!(stdout.contains("No exclusion") || stdout.contains("Exclusion"));
}

#[test]
fn exclude_add_and_list() {
    let tmp = copy_fixture();
    let root = tmp.path();

    // Ensure .autoclimb dir exists
    std::fs::create_dir_all(root.join(".autoclimb")).unwrap();

    // Add pattern
    let output = Command::new(cli_bin())
        .args(["exclude", "add", "*.generated.py", "--path"])
        .arg(root)
        .output()
        .expect("exclude add failed");
    assert!(output.status.success());

    // List should show it
    let output = Command::new(cli_bin())
        .args(["exclude", "list", "--path"])
        .arg(root)
        .output()
        .expect("exclude list failed");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(stdout.contains("*.generated.py"));
}

#[test]
fn config_list_works() {
    let tmp = copy_fixture();
    let root = tmp.path();

    let output = Command::new(cli_bin())
        .args(["config", "list", "--path"])
        .arg(root)
        .output()
        .expect("config list failed");

    assert!(output.status.success());
}

#[test]
fn plan_requires_scan() {
    let tmp = copy_fixture();
    let root = tmp.path();

    let output = Command::new(cli_bin())
        .args(["plan", "show", "--path"])
        .arg(root)
        .output()
        .expect("plan show failed");

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(!output.status.success(), "plan should require a scan");
    assert!(stderr.contains("requires a completed scan"));
}

#[test]
fn review_prepare_requires_scan() {
    let tmp = copy_fixture();
    let root = tmp.path();

    let output = Command::new(cli_bin())
        .args(["review", "--prepare", "--path"])
        .arg(root)
        .output()
        .expect("review prepare failed");

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        !output.status.success(),
        "review prepare should require a scan"
    );
    assert!(stderr.contains("requires a completed scan"));
}

#[test]
fn review_run_batches_rejects_unsupported_backend() {
    let tmp = copy_fixture();
    let root = tmp.path();
    run_scan(root);

    let output = Command::new(cli_bin())
        .args(["review", "--run-batches", "--backend", "gemini", "--path"])
        .arg(root)
        .output()
        .expect("review run-batches failed");

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        !output.status.success(),
        "unsupported review backend should fail fast"
    );
    assert!(stderr.contains("unsupported review batch backend"));
}

#[test]
fn resolve_nonexistent_finding() {
    let tmp = copy_fixture();
    let root = tmp.path();
    run_scan(root);

    let output = Command::new(cli_bin())
        .args([
            "resolve",
            "nonexistent_finding_id",
            "--status",
            "wontfix",
            "--path",
        ])
        .arg(root)
        .output()
        .expect("resolve failed");

    // Should fail gracefully (finding not found)
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    // Either exits with error or prints a message
    assert!(
        !output.status.success() || stderr.contains("not found") || stdout.contains("not found"),
        "expected error for nonexistent finding"
    );
}
