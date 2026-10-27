// Exact same-owner private primitive; no public root-fd/storage bypass export.
#[path = "../../../packages/server-files/src/adapters/local_sanitization_io.rs"]
mod owned_io;
use nix::{
    fcntl::{OFlag, openat},
    sys::stat::Mode,
    unistd::geteuid,
};
use owned_io::{IoFailure, SanitizationIo};
use std::{
    fs::{File, Permissions},
    io::Read,
    os::unix::fs::{PermissionsExt, symlink},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

fn private_tempdir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .permissions(Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
}

#[test]
fn root_admission_uses_actual_fd_type_private_mode_and_expected_owner() {
    let root = private_tempdir();
    assert!(SanitizationIo::open(root.path(), 1).is_ok());
    assert!(
        SanitizationIo::open_for_owner(root.path(), 1, geteuid().as_raw().wrapping_add(1)).is_err()
    );
    std::fs::set_permissions(root.path(), Permissions::from_mode(0o777)).unwrap();
    assert!(SanitizationIo::open(root.path(), 1).is_err());
    std::fs::set_permissions(root.path(), Permissions::from_mode(0o700)).unwrap();
    let parent = private_tempdir();
    let alias = parent.path().join("alias");
    symlink(root.path(), &alias).unwrap();
    assert!(SanitizationIo::open(&alias, 1).is_err());
    let ordinary = parent.path().join("ordinary");
    std::fs::write(&ordinary, b"owned").unwrap();
    assert!(SanitizationIo::open(&ordinary, 1).is_err());
    let ordinary_fd = File::open(&ordinary).unwrap();
    std::fs::set_permissions(&ordinary, Permissions::from_mode(0o600)).unwrap();
    assert!(owned_io::require_private_object(&ordinary_fd, false).is_ok());
    assert!(owned_io::require_private_object(&ordinary_fd, true).is_err());
    std::fs::set_permissions(&ordinary, Permissions::from_mode(0o644)).unwrap();
    assert!(owned_io::require_private_object(&ordinary_fd, false).is_err());
    assert!(SanitizationIo::open(std::path::Path::new("."), 1).is_err());
    assert!(SanitizationIo::open(root.path(), 0).is_err());
    assert!(SanitizationIo::open(root.path(), 5).is_err());
}
#[tokio::test]
async fn operations_keep_the_opened_directory_identity_after_path_replacement() {
    let parent = private_tempdir();
    let path = parent.path().join("root");
    std::fs::create_dir(&path).unwrap();
    std::fs::set_permissions(&path, Permissions::from_mode(0o700)).unwrap();
    std::fs::write(path.join("sentinel"), b"original").unwrap();
    let io = SanitizationIo::open(&path, 1).unwrap();
    std::fs::rename(&path, parent.path().join("moved")).unwrap();
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("sentinel"), b"replacement").unwrap();
    let value = io
        .run(|root| {
            let mut f = File::from(
                openat(
                    root,
                    "sentinel",
                    OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                    Mode::empty(),
                )
                .map_err(|_| ())?,
            );
            let mut s = String::new();
            f.read_to_string(&mut s).map_err(|_| ())?;
            Ok::<_, ()>(s)
        })
        .await
        .unwrap();
    assert_eq!(value, "original");
    io.stop();
    io.drain().await;
}
struct GateRelease(Arc<(Mutex<bool>, Condvar)>);
impl GateRelease {
    fn release(&self) {
        let mut value = self
            .0
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *value = true;
        self.0.1.notify_all();
    }
}
impl Drop for GateRelease {
    fn drop(&mut self) {
        self.release();
    }
}
fn release(gate: &GateRelease) {
    gate.release();
}
type Held = (
    tokio::task::JoinHandle<Result<(), IoFailure<()>>>,
    GateRelease,
);
async fn held(io: SanitizationIo) -> Held {
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let owned = gate.clone();
    // Holds release ownership even if this async helper is cancelled while
    // waiting for the blocking closure to announce entry.
    let cleanup = GateRelease(gate);
    let (tx, rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        io.run(move |_| {
            let _ = tx.send(());
            let mut released = owned.0.lock().unwrap();
            while !*released {
                released = owned.1.wait(released).unwrap();
            }
            Ok::<_, ()>(())
        })
        .await
    });
    rx.await.unwrap();
    (task, cleanup)
}
#[tokio::test]
async fn cancellation_retains_real_blocking_admission_and_stop_drain_waits_for_completion() {
    let root = private_tempdir();
    let io = SanitizationIo::open(root.path(), 2).unwrap();
    let (task, gate) = held(io.clone()).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(!io.available());
    let calls = Arc::new(AtomicUsize::new(0));
    let actual = calls.clone();
    assert!(matches!(
        io.run(move |_| {
            actual.fetch_add(1, Ordering::SeqCst);
            Ok::<_, ()>(())
        })
        .await,
        Err(IoFailure::Unavailable)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    io.stop();
    let waiting = io.clone();
    let mut drain = tokio::spawn(async move {
        waiting.drain().await;
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(25), &mut drain)
            .await
            .is_err()
    );
    release(&gate);
    tokio::time::timeout(Duration::from_secs(2), drain)
        .await
        .unwrap()
        .unwrap();
    assert!(!io.available());
}
#[tokio::test]
async fn abandoned_io_recovers_only_when_the_real_closure_finishes_and_stop_refuses_waiting_work() {
    let root = private_tempdir();
    let io = SanitizationIo::open(root.path(), 1).unwrap();
    let (task, gate) = held(io.clone()).await;
    let queued = io.clone();
    let next = tokio::spawn(async move { queued.run(|_| Ok::<_, ()>(())).await });
    io.stop();
    assert!(matches!(next.await.unwrap(), Err(IoFailure::Unavailable)));
    release(&gate);
    task.await.unwrap().unwrap();
    io.drain().await;
    let root = private_tempdir();
    let io = SanitizationIo::open(root.path(), 1).unwrap();
    let (task, gate) = held(io.clone()).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(!io.available());
    release(&gate);
    tokio::time::timeout(Duration::from_secs(2), io.drain())
        .await
        .unwrap();
    assert!(io.available());
    io.run(|_| Ok::<_, ()>(())).await.unwrap();
    io.stop();
    io.drain().await;
}
