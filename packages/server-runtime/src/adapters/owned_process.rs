//! Dedicated process ownership. Keep the unreaped leader until group cleanup
//! finishes so its PID cannot be recycled into a foreign process group.
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tempfile::TempDir;
use tokio::{
    io::AsyncReadExt,
    process::Command,
    sync::{Semaphore, oneshot, watch},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OwnedProcessError {
    Configuration,
    Unavailable,
    Busy,
    Cancelled,
    Deadline,
    OutputLimit,
    MemoryLimit,
    Failed,
    CleanupFailed,
    ResourceLimit,
    DiskLimit,
}

#[derive(Clone, Copy, Debug)]
pub struct OwnedProcessLimits {
    deadline: Duration,
    output_bytes: usize,
    memory_bytes: u64,
    file_bytes: u64,
}
impl OwnedProcessLimits {
    pub fn new(
        deadline: Duration,
        output_bytes: usize,
        memory_bytes: u64,
        file_bytes: u64,
    ) -> Result<Self, OwnedProcessError> {
        if deadline.is_zero()
            || deadline > Duration::from_secs(600)
            || output_bytes == 0
            || output_bytes > 4 * 1024 * 1024
            || !(16 * 1024 * 1024..=1024 * 1024 * 1024).contains(&memory_bytes)
            || file_bytes == 0
            || file_bytes > 128 * 1024 * 1024
        {
            return Err(OwnedProcessError::Configuration);
        }
        Ok(Self {
            deadline,
            output_bytes,
            memory_bytes,
            file_bytes,
        })
    }
    pub fn deadline(self) -> Duration {
        self.deadline
    }
    pub fn output_bytes(self) -> usize {
        self.output_bytes
    }
    pub fn memory_bytes(self) -> u64 {
        self.memory_bytes
    }
    pub fn file_bytes(self) -> u64 {
        self.file_bytes
    }
}

/// Owned temporary storage follows the detached cleanup task on cancellation.
/// Callers cannot construct a directory from a request-provided filesystem path.
pub struct OwnedProcessDirectory {
    inner: TempDir,
}
impl OwnedProcessDirectory {
    pub fn create(private_root: &Path) -> Result<Self, OwnedProcessError> {
        require_private_directory(private_root)?;
        let mut builder = tempfile::Builder::new();
        builder.prefix("erpx-owned-process-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(fs::Permissions::from_mode(0o700));
        }
        let inner = builder
            .tempdir_in(private_root)
            .map_err(|_| OwnedProcessError::Unavailable)?;
        Ok(Self { inner })
    }
    pub fn path(&self) -> &Path {
        self.inner.path()
    }
    pub fn write_private(&self, name: &str, bytes: &[u8]) -> Result<(), OwnedProcessError> {
        require_name(name)?;
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(nix::libc::O_NOFOLLOW);
        }
        options
            .open(self.path().join(name))
            .and_then(|mut f| f.write_all(bytes))
            .map_err(|_| OwnedProcessError::Unavailable)
    }
    pub fn read_bounded(&self, name: &str, limit: usize) -> Result<Vec<u8>, OwnedProcessError> {
        require_name(name)?;
        if limit == 0 || limit > 128 * 1024 * 1024 {
            return Err(OwnedProcessError::Configuration);
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
        }
        let file = options
            .open(self.path().join(name))
            .map_err(|_| OwnedProcessError::Failed)?;
        let metadata = file.metadata().map_err(|_| OwnedProcessError::Failed)?;
        if !metadata.is_file() || metadata.len() > limit as u64 {
            return Err(OwnedProcessError::OutputLimit);
        }
        let mut bytes = Vec::new();
        file.take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| OwnedProcessError::Failed)?;
        if bytes.len() > limit {
            return Err(OwnedProcessError::OutputLimit);
        }
        Ok(bytes)
    }
}
fn require_name(name: &str) -> Result<(), OwnedProcessError> {
    if name.is_empty()
        || name.len() > 100
        || name.starts_with('.')
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    {
        return Err(OwnedProcessError::Configuration);
    }
    Ok(())
}
fn require_private_directory(path: &Path) -> Result<(), OwnedProcessError> {
    if !path.is_absolute() {
        return Err(OwnedProcessError::Configuration);
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| OwnedProcessError::Unavailable)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(OwnedProcessError::Configuration);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(OwnedProcessError::Configuration);
        }
    }
    Ok(())
}

pub struct OwnedProcessRequest {
    program: PathBuf,
    directory: OwnedProcessDirectory,
    arguments: Vec<OsString>,
    environment: Vec<(OsString, OsString)>,
}
impl OwnedProcessRequest {
    /// Trusted operator/owner adapter inputs only; no shell interpretation.
    pub fn new(
        program: &Path,
        directory: OwnedProcessDirectory,
        arguments: Vec<OsString>,
        environment: Vec<(OsString, OsString)>,
    ) -> Result<Self, OwnedProcessError> {
        if !program.is_absolute()
            || arguments.len() > 64
            || environment.len() > 32
            || arguments.iter().any(|v| v.len() > 8192)
            || environment
                .iter()
                .any(|(k, v)| k.is_empty() || k.len() > 100 || v.len() > 8192)
        {
            return Err(OwnedProcessError::Configuration);
        }
        let program = program
            .canonicalize()
            .map_err(|_| OwnedProcessError::Unavailable)?;
        if !program.is_file() {
            return Err(OwnedProcessError::Unavailable);
        }
        Ok(Self {
            program,
            directory,
            arguments,
            environment,
        })
    }
}
pub struct OwnedProcessOutput {
    pub directory: OwnedProcessDirectory,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}
#[derive(Clone)]
pub struct OwnedProcessRunner {
    permit: Arc<Semaphore>,
    shutdown: watch::Sender<bool>,
    healthy: Arc<AtomicBool>,
}
impl Default for OwnedProcessRunner {
    fn default() -> Self {
        Self::new()
    }
}
impl OwnedProcessRunner {
    pub fn new() -> Self {
        let (shutdown, _) = watch::channel(false);
        Self {
            permit: Arc::new(Semaphore::new(1)),
            shutdown,
            healthy: Arc::new(AtomicBool::new(true)),
        }
    }
    pub fn can_accept_work(&self) -> bool {
        self.healthy.load(Ordering::Acquire)
            && !*self.shutdown.borrow()
            && self.permit.available_permits() > 0
    }
    pub fn request_shutdown(&self) {
        self.shutdown.send_replace(true);
    }
    pub async fn drain(&self) {
        if let Ok(permit) = self.permit.acquire().await {
            drop(permit);
        }
    }
    pub async fn run(
        &self,
        request: OwnedProcessRequest,
        limits: OwnedProcessLimits,
    ) -> Result<OwnedProcessOutput, OwnedProcessError> {
        if !self.can_accept_work() {
            return Err(OwnedProcessError::Unavailable);
        }
        let permit = self
            .permit
            .clone()
            .try_acquire_owned()
            .map_err(|_| OwnedProcessError::Busy)?;
        let runner = self.clone();
        let (mut sender, receiver) = oneshot::channel();
        let deadline = tokio::time::Instant::now() + limits.deadline;
        tokio::spawn(async move {
            let result = run(&runner, request, limits, deadline, &mut sender).await;
            drop(permit);
            let _ = sender.send(result);
        });
        receiver.await.unwrap_or(Err(OwnedProcessError::Failed))
    }
}
async fn run(
    runner: &OwnedProcessRunner,
    request: OwnedProcessRequest,
    limits: OwnedProcessLimits,
    deadline: tokio::time::Instant,
    sender: &mut oneshot::Sender<Result<OwnedProcessOutput, OwnedProcessError>>,
) -> Result<OwnedProcessOutput, OwnedProcessError> {
    if sender.is_closed() || *runner.shutdown.borrow() {
        return Err(OwnedProcessError::Cancelled);
    }
    if tokio::time::Instant::now() >= deadline {
        return Err(OwnedProcessError::Deadline);
    }
    let mut command = Command::new(&request.program);
    command
        .args(request.arguments)
        .current_dir(request.directory.path())
        .env_clear()
        .env("LANG", "C")
        .env("TZ", "UTC")
        .envs(request.environment)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|_| OwnedProcessError::Unavailable)?;
    let pid = child.id().ok_or(OwnedProcessError::Failed)?;
    let stdout = child.stdout.take().ok_or(OwnedProcessError::Failed)?;
    let stderr = child.stderr.take().ok_or(OwnedProcessError::Failed)?;
    let counter = Arc::new(AtomicUsize::new(0));
    let work = async {
        let streams = tokio::try_join!(
            read(stdout, counter.clone(), limits.output_bytes),
            read(stderr, counter, limits.output_bytes)
        )?;
        wait_leader_exit(pid).await?;
        Ok(streams)
    };
    let mut shutdown = runner.shutdown.subscribe();
    let result = tokio::select! {
        biased;
        _=sender.closed()=>Err(OwnedProcessError::Cancelled),
        _=shutdown.wait_for(|stop|*stop)=>Err(OwnedProcessError::Cancelled),
        _=tokio::time::sleep_until(deadline)=>Err(OwnedProcessError::Deadline),
        result=work=>result,
        error=monitor_group(pid,limits.memory_bytes)=>Err(error),
        error=monitor_directory(request.directory.path(), limits.file_bytes.saturating_mul(3))=>Err(error),
    };
    let clean = cleanup_group(pid, &runner.healthy).await;
    let status = child.wait().await.map_err(|_| OwnedProcessError::Failed)?;
    clean?;
    let (stdout, stderr) = result?;
    if !status.success() {
        return Err(OwnedProcessError::Failed);
    }
    Ok(OwnedProcessOutput {
        directory: request.directory,
        stdout,
        stderr,
    })
}
async fn read(
    mut stream: impl tokio::io::AsyncRead + Unpin,
    counter: Arc<AtomicUsize>,
    limit: usize,
) -> Result<Vec<u8>, OwnedProcessError> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let count = stream
            .read(&mut buffer)
            .await
            .map_err(|_| OwnedProcessError::Failed)?;
        if count == 0 {
            return Ok(bytes);
        }
        if counter
            .fetch_add(count, Ordering::Relaxed)
            .saturating_add(count)
            > limit
        {
            return Err(OwnedProcessError::OutputLimit);
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
}

/// Call in the dedicated Rust child before opening any untrusted input or
/// spawning native decoders. Descendants inherit these hard resource ceilings.
pub fn apply_owned_process_resource_limits(
    limits: OwnedProcessLimits,
) -> Result<(), OwnedProcessError> {
    use nix::sys::resource::{Resource, setrlimit};
    for (resource, limit) in [
        (
            Resource::RLIMIT_CPU,
            limits.deadline.as_secs().saturating_add(1),
        ),
        (Resource::RLIMIT_STACK, 16 * 1024 * 1024),
        (Resource::RLIMIT_NOFILE, 64),
        (Resource::RLIMIT_FSIZE, limits.file_bytes),
        (Resource::RLIMIT_CORE, 0),
    ] {
        setrlimit(resource, limit, limit).map_err(|_| OwnedProcessError::ResourceLimit)?;
    }
    #[cfg(target_os = "linux")]
    for (resource, limit) in [
        (Resource::RLIMIT_DATA, limits.memory_bytes),
        (Resource::RLIMIT_AS, limits.memory_bytes.saturating_mul(2)),
    ] {
        setrlimit(resource, limit, limit).map_err(|_| OwnedProcessError::ResourceLimit)?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct GroupProcess {
    pid: i32,
    parent: i32,
    memory: u64,
    zombie: bool,
}
#[cfg(target_os = "macos")]
fn group_members(group: u32) -> Result<Vec<GroupProcess>, OwnedProcessError> {
    use sysinfo::{ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, RefreshKind, System};
    let mut system = System::new_with_specifics(RefreshKind::nothing());
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_memory(),
    );
    let mut members = Vec::new();
    for (pid, process) in system.processes() {
        let pid = pid.as_u32() as i32;
        if nix::unistd::getpgid(Some(nix::unistd::Pid::from_raw(pid)))
            .is_ok_and(|id| id.as_raw() == group as i32)
        {
            members.push(GroupProcess {
                pid,
                parent: process.parent().map(|pid| pid.as_u32() as i32).unwrap_or(0),
                memory: process.memory(),
                zombie: process.status() == ProcessStatus::Zombie,
            });
        }
    }
    Ok(members)
}
#[cfg(target_os = "linux")]
fn group_members(group: u32) -> Result<Vec<GroupProcess>, OwnedProcessError> {
    let mut members = Vec::new();
    for entry in fs::read_dir("/proc")
        .map_err(|_| OwnedProcessError::Unavailable)?
        .flatten()
    {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<i32>().ok())
        else {
            continue;
        };
        if !nix::unistd::getpgid(Some(nix::unistd::Pid::from_raw(pid)))
            .is_ok_and(|id| id.as_raw() == group as i32)
        {
            continue;
        }
        let Ok(status) = fs::read_to_string(entry.path().join("status")) else {
            continue;
        };
        let parent = status
            .lines()
            .find_map(|line| {
                line.strip_prefix("PPid:")
                    .and_then(|value| value.trim().parse().ok())
            })
            .unwrap_or(0);
        let memory = status
            .lines()
            .find_map(|line| {
                line.strip_prefix("VmRSS:")
                    .and_then(|value| value.split_whitespace().next()?.parse::<u64>().ok())
            })
            .unwrap_or(0)
            * 1024;
        let zombie = status
            .lines()
            .any(|line| line.starts_with("State:") && line.contains("Z (zombie)"));
        members.push(GroupProcess {
            pid,
            parent,
            memory,
            zombie,
        });
    }
    Ok(members)
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn group_members(_: u32) -> Result<Vec<GroupProcess>, OwnedProcessError> {
    Err(OwnedProcessError::Unavailable)
}
async fn monitor_group(group: u32, limit: u64) -> OwnedProcessError {
    let mut interval = tokio::time::interval(Duration::from_millis(25));
    loop {
        interval.tick().await;
        match group_members(group) {
            Ok(members) => {
                if members
                    .iter()
                    .fold(0_u64, |sum, p| sum.saturating_add(p.memory))
                    > limit
                {
                    return OwnedProcessError::MemoryLimit;
                }
            }
            Err(error) => return error,
        }
    }
}
async fn wait_leader_exit(group: u32) -> Result<(), OwnedProcessError> {
    use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
    let pid = Pid::from_raw(group as i32).ok_or(OwnedProcessError::Failed)?;
    loop {
        match waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG,
        ) {
            Ok(Some(_)) => return Ok(()),
            Ok(None) => {}
            Err(error) if error == rustix::io::Errno::INTR => continue,
            Err(_) => return Err(OwnedProcessError::Failed),
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}
async fn cleanup_group(group: u32, healthy: &AtomicBool) -> Result<(), OwnedProcessError> {
    #[cfg(unix)]
    {
        use nix::{
            sys::{
                signal::{Signal, killpg},
                wait::{WaitPidFlag, waitpid},
            },
            unistd::Pid,
        };
        let start = tokio::time::Instant::now();
        let mut over_budget = false;
        let mut empty_observations = 0;
        loop {
            let _ = killpg(Pid::from_raw(group as i32), Signal::SIGKILL);
            let members = match group_members(group) {
                Ok(members) => members,
                Err(_) => {
                    healthy.store(false, Ordering::Release);
                    tokio::time::sleep(Duration::from_millis(25)).await;
                    continue;
                }
            };
            let mut live = false;
            for member in members {
                if member.pid == group as i32 {
                    continue;
                }
                if member.zombie {
                    // A container init/subreaper may adopt a descendant. Reap
                    // only our own child and never the pinned group leader.
                    if member.parent == std::process::id() as i32 {
                        let _ = waitpid(Pid::from_raw(member.pid), Some(WaitPidFlag::WNOHANG));
                    }
                } else {
                    live = true;
                }
            }
            // Repeated signals/observations cover a descendant whose fork was
            // in progress when the first group signal reached its parent.
            if !live {
                empty_observations += 1;
            } else {
                empty_observations = 0;
            }
            if empty_observations >= 3 {
                return if over_budget {
                    Err(OwnedProcessError::CleanupFailed)
                } else {
                    Ok(())
                };
            }
            if start.elapsed() > Duration::from_secs(60) {
                over_budget = true;
                healthy.store(false, Ordering::Release);
            }
            // An uninterruptible kernel wait cannot be abandoned as success.
            // Keep ownership/permit/tempfiles until the group has no live work.
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (group, healthy);
        Err(OwnedProcessError::CleanupFailed)
    }
}

// Sampled aggregate storage guard complements inherited per-file RLIMIT_FSIZE.
// Only the owned flat temporary directory is observed; no symlink or nested
// directory is followed. Native tool profiles must be qualified for this shape.
async fn monitor_directory(path: &Path, limit: u64) -> OwnedProcessError {
    let mut interval = tokio::time::interval(Duration::from_millis(25));
    loop {
        interval.tick().await;
        match directory_size(path) {
            Ok(bytes) if bytes <= limit => {}
            Ok(_) => return OwnedProcessError::DiskLimit,
            Err(error) => return error,
        }
    }
}
fn directory_size(path: &Path) -> Result<u64, OwnedProcessError> {
    let mut bytes = 0_u64;
    for (index, entry) in fs::read_dir(path)
        .map_err(|_| OwnedProcessError::Failed)?
        .enumerate()
    {
        if index >= 128 {
            return Err(OwnedProcessError::DiskLimit);
        }
        let entry = entry.map_err(|_| OwnedProcessError::Failed)?;
        let metadata = match fs::symlink_metadata(entry.path()) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(OwnedProcessError::Failed),
        };
        if !metadata.is_file() {
            return Err(OwnedProcessError::DiskLimit);
        }
        bytes = bytes.saturating_add(metadata.len());
    }
    Ok(bytes)
}
