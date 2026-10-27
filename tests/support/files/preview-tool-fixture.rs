//! Test-only native tool fault fixture. No successful decoder implementation.
use serde::Deserialize;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    entered: PathBuf,
    release: PathBuf,
    heartbeat: PathBuf,
}
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() == 3 && args[1] == "--descendant" {
        let path = Path::new(&args[2]);
        let mut tick = 0;
        loop {
            tick += 1;
            fs::write(path, tick.to_string()).unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    let expected = [
        "-f",
        "1",
        "-l",
        "1",
        "-singlefile",
        "-scale-to",
        "2400",
        "-jpeg",
        "input",
        "preview",
    ];
    if args.len() != expected.len() + 1 || args.iter().skip(1).zip(expected).any(|(a, b)| a != b) {
        std::process::exit(80);
    }
    let executable = std::env::current_exe().unwrap();
    let bytes = fs::read(executable.parent().unwrap().join("fixture.json")).unwrap();
    assert!(bytes.len() < 4096);
    let config: Configuration = serde_json::from_slice(&bytes).unwrap();
    assert!(
        config.entered.is_absolute()
            && config.release.is_absolute()
            && config.heartbeat.is_absolute()
    );
    // Deliberately retain a live descendant after this fixture exits. The external
    // Runtime group owner must stop it before releasing its pinned worker leader.
    let child = Command::new(&executable)
        .arg("--descendant")
        .arg(&config.heartbeat)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(child.id() > 0);
    for _ in 0..1000 {
        if config.heartbeat.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(config.heartbeat.exists());
    fs::write(&config.entered, std::process::id().to_string()).unwrap();
    while !config.release.exists() {
        std::thread::sleep(Duration::from_millis(5));
    }
    let _ = std::io::stderr().write_all(b"owned native tool rejected input\n");
    std::process::exit(2);
}
