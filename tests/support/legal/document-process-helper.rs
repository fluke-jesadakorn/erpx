//! Disposable descendant-process fixture, never shipped in runtime artifacts.
use std::{
    env, fs,
    io::Write,
    path::Path,
    process::{self, Command},
    thread,
    time::Duration,
};
fn main() {
    let arguments: Vec<_> = env::args().collect();
    if arguments.get(1).map(String::as_str) == Some("descendant") {
        let root = Path::new(&arguments[2]);
        fs::write(root.join(format!("{}.pid", process::id())), b"descendant").unwrap();
        if arguments.get(3).map(String::as_str) == Some("nested") {
            Command::new(env::current_exe().unwrap())
                .arg("descendant")
                .arg(root)
                .spawn()
                .unwrap();
        }
        thread::sleep(Duration::from_secs(60));
        return;
    }
    assert_eq!(arguments.len(), 3);
    let root = env::current_dir().unwrap().parent().unwrap().to_owned();
    fs::write(root.join(format!("{}.pid", process::id())), b"leader").unwrap();
    let mode = fs::read(&arguments[1]).unwrap();
    match mode.as_slice() {
        b"nested" | b"root-exits" | b"root-exits-closed" => {
            let mut child = Command::new(env::current_exe().unwrap());
            child.arg("descendant").arg(&root).arg("nested");
            if mode == b"root-exits-closed" {
                child
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());
            }
            child.spawn().unwrap();
            // Establish real nested descendants before testing leader exit;
            // an early scheduling timeout must not count as descendant proof.
            let started = std::time::Instant::now();
            while fs::read_dir(&root)
                .unwrap()
                .flatten()
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "pid"))
                .count()
                < 3
            {
                assert!(started.elapsed() < Duration::from_secs(5));
                thread::sleep(Duration::from_millis(2));
            }
            if mode == b"root-exits" || mode == b"root-exits-closed" {
                return;
            }
            thread::sleep(Duration::from_secs(60));
        }
        b"stdout" => loop {
            if std::io::stdout().write_all(&[b'x'; 8192]).is_err() {
                return;
            }
        },
        b"stderr" => loop {
            if std::io::stderr().write_all(&[b'x'; 8192]).is_err() {
                return;
            }
        },
        b"memory" => {
            let mut blocks = Vec::new();
            for _ in 0..32 {
                blocks.push(vec![1_u8; 4 * 1024 * 1024]);
                std::hint::black_box(&blocks);
                thread::sleep(Duration::from_millis(20));
            }
            thread::sleep(Duration::from_secs(60));
        }
        b"exit" => process::exit(7),
        b"inspect" => {
            assert!(env::var_os("HOME").is_none());
            assert!(env::var_os("PATH").is_none());
            assert!(env::var_os("DATABASE_URL").is_none());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    fs::metadata(env::current_dir().unwrap())
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    0o700
                );
                for path in &arguments[1..] {
                    assert_eq!(
                        fs::metadata(path).unwrap().permissions().mode() & 0o777,
                        0o600
                    );
                }
            }
            let request = fs::read_to_string(&arguments[2]).unwrap();
            let checksum = request
                .split("\"checksumSha256\":\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap();
            print!(
                "{{\"outcome\":\"extracted\",\"report\":{{\"status\":\"extracted\",\"method\":\"docx-text\",\"text\":\"draft\\n\\n\",\"pages\":[{{\"page\":1,\"text\":\"draft\\n\\n\"}}],\"pageMapping\":false,\"truncated\":false,\"requiresReview\":true,\"checksum\":\"{checksum}\",\"retryable\":false,\"reasons\":[]}}}}"
            );
        }
        _ => print!("invalid protocol"),
    }
}
