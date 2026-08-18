use std::{
    collections::BTreeSet,
    error::Error,
    path::Path,
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::{architecture, native_macos};

type VerificationResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

const FUTURE_TARGETS: [(&str, &str); 4] = [
    ("Windows", "x86_64-pc-windows-msvc"),
    ("Linux", "x86_64-unknown-linux-gnu"),
    ("iOS", "aarch64-apple-ios"),
    ("Android", "aarch64-linux-android"),
];

pub(crate) fn verify_macos(root: &Path) -> VerificationResult {
    if !cfg!(target_os = "macos") {
        return Err("verify-platforms --macos requires a macOS host".into());
    }
    architecture::scan_native_boundaries(root)?;
    run_cargo(
        root,
        &[
            "build",
            "--locked",
            "--offline",
            "-p",
            "erpx-console",
            "--no-default-features",
            "--features",
            "native",
        ],
        "macOS native build",
    )?;
    bounded_launch_smoke(root, Duration::from_secs(3))?;

    let accessibility_status = root.join("audits/DIOXUS-NATIVE-MACOS/accessibility/status.json");
    if accessibility_status.is_file() {
        let status: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&accessibility_status)?)?;
        if status
            .pointer("/release_status/macos_native")
            .and_then(serde_json::Value::as_str)
            == Some("blocked")
        {
            return Err(format!(
                "macOS native release is blocked by retained accessibility evidence: {}",
                accessibility_status.display()
            )
            .into());
        }
    }

    let evidence = root.join("audits/DIOXUS-NATIVE-MACOS/evidence.json");
    if evidence.is_file() {
        println!("macOS manual evidence is present at {}", evidence.display());
    } else {
        println!(
            "macOS build and bounded launch smoke passed; manual performance/accessibility evidence is not captured"
        );
    }
    Ok(())
}

pub(crate) fn verify_future_contracts(root: &Path) -> VerificationResult {
    architecture::scan_native_boundaries(root)?;
    let installed = installed_targets()?;
    let mut checked = 0_u8;
    for (platform, target) in FUTURE_TARGETS {
        if !installed.contains(target) {
            println!("{platform}: Deferred (target {target} is not installed; skipped)");
            continue;
        }
        run_cargo(
            root,
            &[
                "check",
                "--locked",
                "--offline",
                "-p",
                "erpx-console",
                "--target",
                target,
                "--no-default-features",
                "--features",
                "native",
            ],
            &format!("{platform} future compile contract"),
        )?;
        checked = checked.saturating_add(1);
        println!("{platform}: Deferred (compile contract passed; not release-certified)");
    }
    println!(
        "future platform contracts complete: {checked} checked, {} skipped; all remain Deferred",
        FUTURE_TARGETS.len() - usize::from(checked)
    );
    Ok(())
}

fn bounded_launch_smoke(root: &Path, observation: Duration) -> VerificationResult {
    let mut command = native_macos::native_cargo_command(root);
    command
        .env("ERPX_NATIVE_MACOS_SMOKE", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command.spawn()?;
    let started = Instant::now();
    while started.elapsed() < observation {
        if let Some(status) = child.try_wait()? {
            return Err(format!(
                "macOS native launch exited before the {:?} smoke window with {status}",
                observation
            )
            .into());
        }
        thread::sleep(Duration::from_millis(100));
    }

    let group = format!("-{}", child.id());
    let signal = Command::new("kill").args(["-TERM", &group]).status()?;
    if !signal.success() {
        let _ = child.kill();
    }
    let stop_started = Instant::now();
    while stop_started.elapsed() < Duration::from_secs(5) {
        if child.try_wait()?.is_some() {
            println!(
                "macOS native deterministic smoke passed ({:.1}s observation)",
                observation.as_secs_f64()
            );
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    Err("macOS native smoke process did not stop within 5s".into())
}

fn installed_targets() -> VerificationResult<BTreeSet<String>> {
    let output = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()?;
    require_success(output.status, "rustup target list --installed")?;
    Ok(String::from_utf8(output.stdout)?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

fn run_cargo(root: &Path, args: &[&str], label: &str) -> VerificationResult {
    let status = Command::new("cargo")
        .current_dir(root)
        .args(args)
        .status()?;
    require_success(status, label)
}

fn require_success(status: ExitStatus, label: &str) -> VerificationResult {
    if status.success() {
        Ok(())
    } else {
        Err(format!("{label} failed with {status}").into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn future_contracts_are_explicitly_non_release_targets() {
        assert_eq!(FUTURE_TARGETS.len(), 4);
        assert_eq!(FUTURE_TARGETS[0], ("Windows", "x86_64-pc-windows-msvc"));
        assert_eq!(FUTURE_TARGETS[3], ("Android", "aarch64-linux-android"));
    }
}
