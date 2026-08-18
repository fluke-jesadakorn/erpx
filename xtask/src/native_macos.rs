use std::{
    env,
    error::Error,
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::{fs::OpenOptionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub(crate) type NativeResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

const PROCESS_MARKERS: &[&str] = &[
    "target/debug/erpx-console",
    "target/release/erpx-console",
    "-p erpx-console --no-default-features --features native",
];

pub(crate) fn start_detached(root: &Path) -> NativeResult {
    require_macos()?;
    let paths = NativePaths::new(root);
    fs::create_dir_all(&paths.runtime)?;
    if let Some(pid) = read_pid(&paths.pid)? {
        if process_is_native(pid) {
            println!("native-macos: running (pid {pid})");
            return Ok(());
        }
        remove_pid(&paths.pid)?;
    }

    let log = secure_log(&paths.log)?;
    let stderr = log.try_clone()?;
    let mut command = native_cargo_command(root);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr));
    // A dedicated group lets native-macos-stop terminate Cargo and the
    // renderer it owns without touching the Web/server process group.
    command.process_group(0);
    let child = command.spawn()?;
    write_pid_atomically(&paths.pid, child.id())?;
    println!(
        "native-macos: starting (pid {}, log {})",
        child.id(),
        paths.log.display()
    );
    Ok(())
}

pub(crate) fn status(root: &Path) -> NativeResult {
    let paths = NativePaths::new(root);
    match read_pid(&paths.pid)? {
        Some(pid) if process_is_native(pid) => println!("native-macos: running (pid {pid})"),
        Some(pid) => println!("native-macos: stopped (stale pid {pid})"),
        None => println!("native-macos: stopped"),
    }
    Ok(())
}

pub(crate) fn stop(root: &Path) -> NativeResult {
    let paths = NativePaths::new(root);
    let Some(pid) = read_pid(&paths.pid)? else {
        println!("native-macos: stopped");
        return Ok(());
    };
    if process_is_native(pid) {
        signal_process_group(pid, "-TERM")?;
        wait_until_stopped(pid, Duration::from_secs(10))?;
    }
    remove_pid(&paths.pid)?;
    println!("native-macos: stopped");
    Ok(())
}

pub(crate) fn native_cargo_command(root: &Path) -> Command {
    let mut command = Command::new("cargo");
    command.current_dir(root).args([
        "run",
        "--locked",
        "--offline",
        "-p",
        "erpx-console",
        "--no-default-features",
        "--features",
        "native",
    ]);
    // Do not leak xtask's package-scoped Cargo variables into the nested
    // Console build; they otherwise perturb build-script fingerprints.
    for key in env::vars_os().map(|(key, _)| key).filter(|key| {
        let key = key.to_string_lossy();
        key.starts_with("CARGO_PKG_")
            || matches!(
                key.as_ref(),
                "CARGO_BIN_NAME"
                    | "CARGO_CRATE_NAME"
                    | "CARGO_MANIFEST_DIR"
                    | "CARGO_MANIFEST_PATH"
                    | "CARGO_PRIMARY_PACKAGE"
                    | "CARGO_SBOM_PATH"
            )
    }) {
        command.env_remove(key);
    }
    command
}

pub(crate) fn process_is_native(pid: u32) -> bool {
    process_command(pid).is_some_and(|command| command_matches_native(&command))
}

fn command_matches_native(command: &str) -> bool {
    PROCESS_MARKERS
        .iter()
        .any(|marker| command.contains(marker))
}

fn require_macos() -> NativeResult {
    if cfg!(target_os = "macos") {
        Ok(())
    } else {
        Err("native-macos is supported only on a macOS host".into())
    }
}

fn process_command(pid: u32) -> Option<String> {
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn signal_process_group(pid: u32, signal: &str) -> NativeResult {
    let group = format!("-{pid}");
    let status = Command::new("kill").args([signal, &group]).status()?;
    if status.success() || !process_is_native(pid) {
        Ok(())
    } else {
        Err(format!("could not signal native-macos process group {pid}").into())
    }
}

fn wait_until_stopped(pid: u32, timeout: Duration) -> NativeResult {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if process_command(pid).is_none() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(format!("native-macos pid {pid} did not stop within {timeout:?}").into())
}

fn secure_log(path: &Path) -> NativeResult<File> {
    Ok(OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)?)
}

fn write_pid_atomically(path: &Path, pid: u32) -> NativeResult {
    let temporary = path.with_extension(format!("pid.{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)?;
    writeln!(file, "{pid}")?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    Ok(())
}

fn read_pid(path: &Path) -> NativeResult<Option<u32>> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(None);
    };
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(format!("unsafe native-macos PID record: {}", path.display()).into());
    }
    Ok(fs::read_to_string(path)?.trim().parse().ok())
}

fn remove_pid(path: &Path) -> NativeResult {
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

struct NativePaths {
    runtime: PathBuf,
    pid: PathBuf,
    log: PathBuf,
}

impl NativePaths {
    fn new(root: &Path) -> Self {
        let runtime = root.join(".runtime/erpx/native-macos");
        Self {
            pid: runtime.join("native.pid"),
            log: runtime.join("native.log"),
            runtime,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_matching_does_not_match_web_or_server() {
        assert!(command_matches_native(
            "cargo run -p erpx-console --no-default-features --features native"
        ));
        assert!(command_matches_native(
            "/workspace/target/debug/erpx-console"
        ));
        assert!(!command_matches_native("dx serve --platform web"));
        assert!(!command_matches_native(
            "/workspace/target/debug/erpx-server"
        ));
    }

    #[test]
    fn native_records_are_isolated_from_web_records() {
        let root = Path::new("/workspace");
        let paths = NativePaths::new(root);
        assert_eq!(
            paths.pid,
            root.join(".runtime/erpx/native-macos/native.pid")
        );
        assert_ne!(paths.pid, root.join(".runtime/erpx/web.pid"));
    }
}
