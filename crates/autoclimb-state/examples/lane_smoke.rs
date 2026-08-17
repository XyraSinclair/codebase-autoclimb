use autoclimb_state::lane::Lane;
use std::path::Path;
use std::process::Command;

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf-8 git output")
}

fn main() {
    let root = std::env::temp_dir().join(format!("autoclimb-lane-smoke-{}", std::process::id()));
    let repo = root.join("repo");
    let lanes = root.join("lanes");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&repo).expect("create scratch repo");
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.name", "Lane Smoke"]);
    git(
        &repo,
        &["config", "user.email", "lane-smoke@example.invalid"],
    );
    std::fs::write(repo.join("allowed.txt"), "base\n").expect("write allowed file");
    std::fs::write(repo.join("outside.txt"), "base\n").expect("write outside file");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "base"]);

    let base_head = git(&repo, &["rev-parse", "HEAD"]).trim().to_owned();
    let original_status = git(&repo, &["status", "--porcelain"]);
    let lane = Lane::create(&repo, &base_head, &lanes).expect("create lane");
    std::fs::write(lane.path.join("allowed.txt"), "changed\n").expect("edit allowed file");
    lane.enforce(&["allowed.txt".to_owned()], &[])
        .expect("allowed path passes");
    println!("allowed verdict: pass");

    std::fs::write(lane.path.join("outside.txt"), "changed\n").expect("edit outside file");
    let violation = lane
        .enforce(&["allowed.txt".to_owned()], &[])
        .expect_err("outside path is rejected");
    assert!(violation.to_string().contains("outside.txt"));
    println!("violation verdict: {violation}");

    let result_tree = lane.result_tree().expect("write result tree");
    assert_ne!(result_tree, lane.base_tree);
    println!("tree changed: {} -> {}", lane.base_tree, result_tree);
    let final_status = git(&repo, &["status", "--porcelain"]);
    assert_eq!(final_status, original_status);
    println!("real repo index/status untouched: yes");
    lane.remove_discarding().expect("discard lane");
    std::fs::remove_dir_all(root).expect("remove scratch repo");
}
