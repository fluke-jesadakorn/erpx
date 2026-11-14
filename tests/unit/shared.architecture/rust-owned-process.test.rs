use erpx_server_runtime::public::owned_process::{
    OwnedProcessDirectory, OwnedProcessError, OwnedProcessLimits, OwnedProcessRequest,
    OwnedProcessRunner, apply_owned_process_resource_limits,
};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};
fn root() -> tempfile::TempDir {
    let t = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(t.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    t
}
fn limits() -> OwnedProcessLimits {
    OwnedProcessLimits::new(Duration::from_secs(2), 1024, 256 * 1024 * 1024, 1024 * 1024).unwrap()
}
fn request(root: &Path, mode: &str) -> OwnedProcessRequest {
    let dir = OwnedProcessDirectory::create(root).unwrap();
    OwnedProcessRequest::new(
        &std::env::current_exe().unwrap(),
        dir,
        vec![
            "--exact".into(),
            "owned_process_fixture_entry".into(),
            "--nocapture".into(),
        ],
        vec![("ERPX_OWNED_PROCESS_FIXTURE_MODE".into(), mode.into())],
    )
    .unwrap()
}
#[test]
fn owned_process_fixture_entry() {
    let Ok(mode) = std::env::var("ERPX_OWNED_PROCESS_FIXTURE_MODE") else {
        return;
    };
    match mode.as_str() {
        "echo" => {
            assert!(std::env::var_os("HOME").is_none());
            assert!(std::env::var_os("PATH").is_none());
            assert!(std::env::var_os("DATABASE_URL").is_none());
            println!("owned-env-cleared");
        }
        "limit" => {
            apply_owned_process_resource_limits(limits()).unwrap();
            let (size, _) =
                nix::sys::resource::getrlimit(nix::sys::resource::Resource::RLIMIT_FSIZE).unwrap();
            assert_eq!(size, 1024 * 1024);
            println!("resource-limit-applied");
        }
        "flood" => {
            let _ = std::io::stdout().write_all(&[b'x'; 2048]);
            let _ = std::io::stderr().write_all(&[b'y'; 2048]);
        }
        "leader-exit" | "leader-wait" => {
            // Deliberately let the test leader exit while its child is alive.
            // The external owned-group runner must stop that child; waiting
            // here would remove the exact lifecycle regression under test.
            #[expect(
                clippy::zombie_processes,
                reason = "intentional live descendant owned by the external process-group runner"
            )]
            let child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "owned_process_fixture_entry", "--nocapture"])
                .env_clear()
                .env("ERPX_OWNED_PROCESS_FIXTURE_MODE", "descendant")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            std::fs::write("leader-pid", std::process::id().to_string()).unwrap();
            std::fs::write("descendant-pid", child.id().to_string()).unwrap();
            for _ in 0..500 {
                if Path::new("heartbeat").exists() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            assert!(Path::new("heartbeat").exists());
            if mode == "leader-wait" {
                loop {
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
        }
        "descendant" => {
            let mut counter = 0;
            loop {
                counter += 1;
                std::fs::write("heartbeat", counter.to_string()).unwrap();
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        "closed-pipes" => {
            nix::unistd::close(1).unwrap();
            nix::unistd::close(2).unwrap();
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        "disk" => {
            for index in 0..4 {
                std::fs::write(format!("output-{index}"), vec![0u8; 1024 * 1024]).unwrap();
            }
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        "memory" => {
            let mut bytes = vec![0u8; 128 * 1024 * 1024];
            for i in (0..bytes.len()).step_by(4096) {
                bytes[i] = 1;
            }
            std::hint::black_box(&bytes);
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        _ => panic!("unexpected private fixture mode"),
    }
}
#[tokio::test]
async fn exact_cleared_environment_and_child_hard_limits() {
    let root = root();
    let runner = OwnedProcessRunner::new();
    for mode in ["echo", "limit"] {
        let out = runner
            .run(request(root.path(), mode), limits())
            .await
            .unwrap();
        let text = std::str::from_utf8(&out.stdout).unwrap();
        assert!(text.contains(if mode == "echo" {
            "owned-env-cleared"
        } else {
            "resource-limit-applied"
        }));
        assert!(out.stderr.is_empty());
    }
}
#[tokio::test]
async fn aggregate_stdout_stderr_is_bounded() {
    let root = root();
    let r = OwnedProcessRunner::new()
        .run(request(root.path(), "flood"), limits())
        .await;
    assert!(matches!(r, Err(OwnedProcessError::OutputLimit)));
}
#[tokio::test]
async fn exited_leader_remains_pinned_until_live_descendant_is_stopped() {
    let root = root();
    let out = OwnedProcessRunner::new()
        .run(request(root.path(), "leader-exit"), limits())
        .await
        .unwrap();
    let first = out.directory.read_bounded("heartbeat", 100).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(first, out.directory.read_bounded("heartbeat", 100).unwrap());
}
#[tokio::test]
async fn eof_does_not_claim_leader_exit() {
    let root = root();
    let l = OwnedProcessLimits::new(
        Duration::from_millis(150),
        1024,
        256 * 1024 * 1024,
        1024 * 1024,
    )
    .unwrap();
    let r = OwnedProcessRunner::new()
        .run(request(root.path(), "closed-pipes"), l)
        .await;
    assert!(matches!(r, Err(OwnedProcessError::Deadline)));
}
#[tokio::test]
async fn cancellation_keeps_owned_directory_and_permit_until_group_cleanup() {
    let root = root();
    let runner = OwnedProcessRunner::new();
    let q = request(root.path(), "leader-wait");
    let dirs = std::fs::read_dir(root.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    let work_dir = dirs[0].clone();
    let task = {
        let runner = runner.clone();
        tokio::spawn(async move { runner.run(q, limits()).await })
    };
    for _ in 0..500 {
        if work_dir.join("heartbeat").exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert!(work_dir.join("heartbeat").exists());
    assert!(!runner.can_accept_work());
    task.abort();
    let _ = task.await;
    runner.drain().await;
    assert!(runner.can_accept_work());
    assert!(!work_dir.exists());
}
#[tokio::test]
async fn stopping_cancels_the_owned_group_and_prevents_further_admission() {
    let root = root();
    let runner = OwnedProcessRunner::new();
    let task = {
        let runner = runner.clone();
        let request = request(root.path(), "leader-wait");
        tokio::spawn(async move { runner.run(request, limits()).await })
    };
    for _ in 0..100 {
        if !runner.can_accept_work() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    runner.request_shutdown();
    assert!(matches!(
        task.await.unwrap(),
        Err(OwnedProcessError::Cancelled)
    ));
    runner.drain().await;
    assert!(!runner.can_accept_work());
}
#[tokio::test]
async fn group_resident_memory_is_bounded() {
    let root = root();
    let l = OwnedProcessLimits::new(Duration::from_secs(2), 1024, 32 * 1024 * 1024, 1024 * 1024)
        .unwrap();
    let r = OwnedProcessRunner::new()
        .run(request(root.path(), "memory"), l)
        .await;
    assert!(matches!(r, Err(OwnedProcessError::MemoryLimit)));
}
#[test]
fn private_files_cannot_follow_symlinks_or_escape_directory() {
    let root = root();
    let d = OwnedProcessDirectory::create(root.path()).unwrap();
    assert!(matches!(
        d.write_private("../escape", b"x"),
        Err(OwnedProcessError::Configuration)
    ));
    d.write_private("input", b"data").unwrap();
    assert!(d.write_private("input", b"changed").is_err());
    assert_eq!(d.read_bounded("input", 4).unwrap(), b"data");
    assert!(matches!(
        d.read_bounded("input", 3),
        Err(OwnedProcessError::OutputLimit)
    ));
    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let outside = root.path().join("outside");
        std::fs::write(&outside, b"private").unwrap();
        symlink(&outside, d.path().join("link")).unwrap();
        assert!(d.read_bounded("link", 100).is_err());
        assert_eq!(
            std::fs::metadata(d.path().join("input"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(d.path()).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
}

#[tokio::test]
async fn aggregate_private_directory_storage_is_bounded() {
    let root = root();
    let result = OwnedProcessRunner::new()
        .run(request(root.path(), "disk"), limits())
        .await;
    assert!(matches!(result, Err(OwnedProcessError::DiskLimit)));
}
