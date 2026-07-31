use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

fn temp_dir() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    loop {
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "erpx-migrator-cli-{}-{suffix}-{sequence}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return path,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("temp directory: {error}"),
        }
    }
}

fn run(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_erpx-migrator"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run migrator")
}

#[test]
fn help_and_plan_are_available_without_writing() {
    let dir = temp_dir();
    let help = run(&dir, &["--help"]);
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("reconcile"));

    let unknown = run(&dir, &["unknown"]);
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("unknown command"));

    let fixture = run(&dir, &["fixture"]);
    assert!(fixture.status.success());
    let fixture: Value = serde_json::from_slice(&fixture.stdout).expect("fixture JSON");
    assert_eq!(fixture["mode"], "bounded_summary");
    assert!(fixture["counts"].is_object());
    assert!(fixture["totals_minor"].is_object());

    let plan = run(&dir, &["plan"]);
    assert!(plan.status.success());
    let plan: Value = serde_json::from_slice(&plan.stdout).expect("plan JSON");
    assert_eq!(plan["read_only"], true);
    assert_eq!(plan["state_change"], false);
    for step in [
        "freeze_source",
        "map_ids",
        "backfill",
        "project",
        "shadow_replay",
        "compare",
        "explain",
        "pilot",
        "rollback_rehearsal",
        "final_delta",
        "command_owner_switch",
        "legacy_read_only_retention",
    ] {
        assert!(
            plan["steps"]
                .as_array()
                .expect("steps")
                .iter()
                .any(|item| item["id"] == step)
        );
    }
    assert_eq!(fs::read_dir(&dir).expect("read directory").count(), 0);
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn map_is_repeatable_and_writes_only_when_requested() {
    let dir = temp_dir();
    fs::write(
        dir.join("ids.json"),
        r#"{"customer":"00000000-0000-0000-0000-000000000001","order":"00000000-0000-0000-0000-000000000002"}"#,
    )
    .expect("input");
    let first = run(&dir, &["map", "--namespace", "test", "--input", "ids.json"]);
    let second = run(&dir, &["map", "--namespace", "test", "--input", "ids.json"]);
    assert!(first.status.success());
    assert_eq!(first.stdout, second.stdout);
    assert!(!dir.join("output.json").exists());

    let written = run(
        &dir,
        &[
            "map",
            "--namespace",
            "test",
            "--input",
            "ids.json",
            "--output",
            "output.json",
        ],
    );
    assert!(written.status.success());
    assert_eq!(written.stdout, b"");
    assert_eq!(
        fs::read_to_string(dir.join("output.json")).expect("output"),
        String::from_utf8(first.stdout).expect("UTF-8")
    );
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn reconcile_handles_explanations_and_rejects_unsafe_or_oversized_inputs() {
    let dir = temp_dir();
    fs::write(
        dir.join("source.json"),
        r#"{"counts":{"orders":2,"customers":1},"totals_minor":{}}"#,
    )
    .expect("source");
    fs::write(
        dir.join("target.json"),
        r#"{"counts":{"orders":3,"customers":2},"totals_minor":{}}"#,
    )
    .expect("target");
    fs::write(
        dir.join("explanations.json"),
        r#"{"orders":"late source event"}"#,
    )
    .expect("explanation");
    let report = run(
        &dir,
        &[
            "reconcile",
            "--source",
            "source.json",
            "--target",
            "target.json",
            "--explanations",
            "explanations.json",
        ],
    );
    assert!(report.status.success());
    let report: Value = serde_json::from_slice(&report.stdout).expect("report JSON");
    assert_eq!(report["reconciled"], false);
    assert_eq!(
        report["differences"].as_array().expect("differences").len(),
        2
    );
    assert!(
        report["differences"]
            .as_array()
            .expect("differences")
            .iter()
            .any(|item| item["key"] == "orders" && item["explained"] == true)
    );
    assert!(
        report["differences"]
            .as_array()
            .expect("differences")
            .iter()
            .any(|item| item["key"] == "customers" && item["explained"] == false)
    );

    let traversal = run(&dir, &["plan", "--output", "../unsafe.json"]);
    assert!(!traversal.status.success());
    assert!(!dir.parent().expect("parent").join("unsafe.json").exists());

    fs::write(dir.join("large.json"), vec![b' '; 128 * 1024 + 1]).expect("large input");
    let oversized = run(
        &dir,
        &["map", "--namespace", "test", "--input", "large.json"],
    );
    assert!(!oversized.status.success());
    assert!(!String::from_utf8_lossy(&oversized.stderr).contains("large"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn secret_looking_input_is_rejected_without_redaction_leak() {
    let dir = temp_dir();
    let secret = "do-not-print-this-bearer-token";
    fs::write(
        dir.join("secret.json"),
        format!(r#"{{"api_token":"{secret}"}}"#),
    )
    .expect("secret input");
    let result = run(
        &dir,
        &["map", "--namespace", "test", "--input", "secret.json"],
    );
    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("secret-looking input is rejected"));
    assert!(!stderr.contains(secret));
    fs::remove_dir_all(dir).expect("cleanup");
}
