//! Files-local ownership of finite bounded private-file I/O. A dropped async
//! waiter never releases admission while its blocking closure still executes.
use nix::{
    fcntl::{OFlag, open},
    sys::stat::Mode,
    unistd::geteuid,
};
use std::{
    fs::File,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::sync::{Notify, Semaphore};

#[derive(Clone)]
pub(crate) struct SanitizationIo {
    state: Arc<State>,
}
struct State {
    root: File,
    admission: Arc<Semaphore>,
    gate: Mutex<Gate>,
    abandoned: AtomicUsize,
    changed: Notify,
}
struct Gate {
    stopping: bool,
    active: usize,
}
enum Phase {
    Running,
    Abandoned,
    Finished,
}
struct Operation {
    state: Arc<State>,
    phase: Mutex<Phase>,
}
struct Caller(Arc<Operation>);
impl Drop for Caller {
    fn drop(&mut self) {
        let mut phase = self
            .0
            .phase
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if matches!(*phase, Phase::Running) {
            *phase = Phase::Abandoned;
            self.0.state.abandoned.fetch_add(1, Ordering::AcqRel);
        }
    }
}
struct Blocking(Arc<Operation>);
impl Drop for Blocking {
    fn drop(&mut self) {
        let mut phase = self
            .0
            .phase
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if matches!(*phase, Phase::Abandoned) {
            self.0.state.abandoned.fetch_sub(1, Ordering::AcqRel);
        }
        *phase = Phase::Finished;
        let mut gate = self
            .0
            .state
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        gate.active -= 1;
        self.0.state.changed.notify_waiters();
    }
}
#[derive(Debug)]
pub(crate) enum IoFailure<E> {
    Unavailable,
    Owner(E),
}
impl SanitizationIo {
    pub(crate) fn open(root: &Path, concurrency: usize) -> Result<Self, ()> {
        Self::open_for_owner(root, concurrency, geteuid().as_raw())
    }
    pub(super) fn open_for_owner(
        root: &Path,
        concurrency: usize,
        expected_owner: u32,
    ) -> Result<Self, ()> {
        if !root.is_absolute() || !(1..=4).contains(&concurrency) {
            return Err(());
        }
        let root = File::from(
            open(
                root,
                OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::empty(),
            )
            .map_err(|_| ())?,
        );
        require_private_owner(&root, true, expected_owner)?;
        Ok(Self {
            state: Arc::new(State {
                root,
                admission: Arc::new(Semaphore::new(concurrency)),
                gate: Mutex::new(Gate {
                    stopping: false,
                    active: 0,
                }),
                abandoned: AtomicUsize::new(0),
                changed: Notify::new(),
            }),
        })
    }
    pub(crate) fn available(&self) -> bool {
        let gate = self
            .state
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        !gate.stopping && self.state.abandoned.load(Ordering::Acquire) == 0
    }
    pub(crate) fn stop(&self) {
        let mut gate = self
            .state
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        gate.stopping = true;
        self.state.admission.close();
        self.state.changed.notify_waiters();
    }
    pub(crate) async fn drain(&self) {
        loop {
            let changed = self.state.changed.notified();
            tokio::pin!(changed);
            // notify_waiters does not store a permit. Enable before reading
            // the gate so a finish between the check and await cannot be lost.
            changed.as_mut().enable();
            if self
                .state
                .gate
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .active
                == 0
            {
                return;
            }
            changed.await;
        }
    }
    /// Run a bounded critical operation in the caller's poll after admission.
    /// There is no suspension after the operation starts: cancellation or
    /// runtime shutdown cannot drop its database guards before it completes.
    /// Multi-thread runtimes can replace the blocked worker; on a current-thread
    /// runtime the operation deliberately blocks that thread until completion.
    pub(crate) async fn run_critical<T, E, F>(&self, operation: F) -> Result<T, IoFailure<E>>
    where
        F: FnOnce(&File) -> Result<T, E>,
    {
        let _permit = self
            .state
            .admission
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| IoFailure::Unavailable)?;
        let owner = {
            let mut gate = self
                .state
                .gate
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if gate.stopping || self.state.abandoned.load(Ordering::Acquire) != 0 {
                return Err(IoFailure::Unavailable);
            }
            gate.active += 1;
            Arc::new(Operation {
                state: self.state.clone(),
                phase: Mutex::new(Phase::Running),
            })
        };
        let guard = Blocking(owner);
        let result = match tokio::runtime::Handle::try_current() {
            Ok(handle) if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
                tokio::task::block_in_place(|| operation(&guard.0.state.root))
            }
            _ => operation(&guard.0.state.root),
        };
        drop(guard);
        result.map_err(IoFailure::Owner)
    }
    pub(crate) async fn run<T, E, F>(&self, operation: F) -> Result<T, IoFailure<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&File) -> Result<T, E> + Send + 'static,
    {
        let permit = self
            .state
            .admission
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| IoFailure::Unavailable)?;
        let owner = {
            let mut gate = self
                .state
                .gate
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if gate.stopping || self.state.abandoned.load(Ordering::Acquire) != 0 {
                return Err(IoFailure::Unavailable);
            }
            // stop/drain and admission share this gate. A new operation cannot
            // increment active after stop observed the final zero.
            gate.active += 1;
            Arc::new(Operation {
                state: self.state.clone(),
                phase: Mutex::new(Phase::Running),
            })
        };
        let caller = Caller(owner.clone());
        let blocking = Blocking(owner);
        let result = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let _blocking = blocking;
            operation(&_blocking.0.state.root)
        })
        .await
        .map_err(|_| IoFailure::Unavailable)?;
        drop(caller);
        result.map_err(IoFailure::Owner)
    }
}
pub(super) fn require_private_object(file: &File, directory: bool) -> Result<(), ()> {
    require_private_owner(file, directory, geteuid().as_raw())
}
fn require_private_owner(file: &File, directory: bool, expected_owner: u32) -> Result<(), ()> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata().map_err(|_| ())?;
    if (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
        || metadata.mode() & 0o077 != 0
        || metadata.uid() != expected_owner
    {
        return Err(());
    }
    Ok(())
}
