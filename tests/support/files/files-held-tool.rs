//! Test-only gate that forwards one exact PDF preview invocation to the actual
//! qualified decoder. It never implements a fake successful preview.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    actual: PathBuf,
    actual_sha256: String,
    entered: PathBuf,
    release: PathBuf,
    heartbeat: PathBuf,
}
fn main() {
    let args = std::env::args_os().collect::<Vec<_>>();
    if args.len() == 3 && args[1] == "--heartbeat" {
        let path = Path::new(&args[2]);
        let started = Instant::now();
        for i in 0_u64.. {
            if started.elapsed() > Duration::from_secs(10) {
                return;
            }
            fs::write(path, i.to_string()).unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
        return;
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
    assert_eq!(args.len(), expected.len() + 1);
    assert!(args.iter().skip(1).zip(expected).all(|(a, b)| a == b));
    let own = std::env::current_exe().unwrap();
    let raw = fs::read(own.parent().unwrap().join("fixture.json")).unwrap();
    assert!(raw.len() < 4096);
    let c: Configuration = serde_json::from_slice(&raw).unwrap();
    assert!(
        [&c.actual, &c.entered, &c.release, &c.heartbeat]
            .into_iter()
            .all(|p| p.is_absolute())
    );
    let actual = fs::File::open(&c.actual).unwrap();
    assert!(actual.metadata().unwrap().len() <= 512 * 1024 * 1024);
    let mut bytes = Vec::new();
    actual
        .take(512 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(hex::encode(Sha256::digest(&bytes)), c.actual_sha256);
    let descendant = Command::new(&own)
        .arg("--heartbeat")
        .arg(&c.heartbeat)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(descendant.id() > 0);
    fs::write(
        &c.entered,
        format!("{}\n{}", std::process::id(), descendant.id()),
    )
    .unwrap();
    let entered = Instant::now();
    while !c.release.exists() {
        if entered.elapsed() > Duration::from_secs(10) {
            std::process::exit(82);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let status = Command::new(&c.actual)
        .args(args.iter().skip(1))
        .env_clear()
        .env("LANG", "C")
        .env("TZ", "UTC")
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .unwrap();
    std::process::exit(status.code().unwrap_or(3));
}
