//! Owns an entire dedicated process group, including native OCR descendants.
//! The group leader is deliberately NOT reaped before group cleanup: its PID
//! pins our PGID so it cannot be recycled into an unrelated process group.
use crate::{
    adapters::document_engine::{
        DocumentChildRequest, DocumentChildResponse, DocumentToolConfiguration,
    },
    application::document_extraction::{
        DocumentExtractor, finish_document_extraction, require_extraction_report,
    },
    domain::document_extraction::{
        DOCUMENT_INPUT_LIMIT, DocumentComputeBudget, DocumentExtractionFailure as Failure,
        DocumentExtractionInput, DocumentExtractionKind, DocumentExtractionReport,
        DocumentExtractionSource, EXTRACTION_DEADLINE_SECONDS, EXTRACTION_OUTPUT_LIMIT,
        ExtractedDocument, ExtractionMethod,
    },
};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
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

#[derive(Clone, Copy, Debug)]
pub struct DocumentProcessLimits {
    deadline: Duration,
    output_bytes: usize,
    memory_bytes: u64,
}
impl Default for DocumentProcessLimits {
    fn default() -> Self {
        Self {
            deadline: Duration::from_secs(EXTRACTION_DEADLINE_SECONDS),
            output_bytes: EXTRACTION_OUTPUT_LIMIT,
            memory_bytes: 512 * 1024 * 1024,
        }
    }
}
impl DocumentProcessLimits {
    /// Operators/tests may reduce a ceiling, never extend the lease budget.
    pub fn new(
        deadline: Duration,
        output_bytes: usize,
        memory_bytes: u64,
    ) -> Result<Self, Failure> {
        if deadline.is_zero()
            || deadline > Duration::from_secs(EXTRACTION_DEADLINE_SECONDS)
            || output_bytes == 0
            || output_bytes > EXTRACTION_OUTPUT_LIMIT
            || !(16 * 1024 * 1024..=512 * 1024 * 1024).contains(&memory_bytes)
        {
            return Err(Failure::InvalidToolConfiguration);
        }
        Ok(Self {
            deadline,
            output_bytes,
            memory_bytes,
        })
    }
}
#[derive(Clone)]
pub struct BoundedDocumentExtractor {
    binary: PathBuf,
    private_root: PathBuf,
    tools: DocumentToolConfiguration,
    limits: DocumentProcessLimits,
    permit: Arc<Semaphore>,
    shutdown: watch::Sender<bool>,
    healthy: Arc<AtomicBool>,
}
impl BoundedDocumentExtractor {
    pub fn new(
        binary: &Path,
        private_root: &Path,
        tools: DocumentToolConfiguration,
        limits: DocumentProcessLimits,
    ) -> Result<Self, Failure> {
        if !binary.is_absolute()
            || !private_root.is_absolute()
            || !cfg!(any(target_os = "macos", target_os = "linux"))
        {
            return Err(Failure::WorkerUnavailable);
        }
        let metadata =
            fs::symlink_metadata(private_root).map_err(|_| Failure::WorkerUnavailable)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(Failure::WorkerUnavailable);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(Failure::WorkerUnavailable);
            }
        }
        let binary = binary
            .canonicalize()
            .map_err(|_| Failure::WorkerUnavailable)?;
        if !binary.is_file() {
            return Err(Failure::WorkerUnavailable);
        }
        // Full tools/tessdata availability is checked only for formats needing
        // them. DOCX remains usable without an installed OCR language bundle.
        let (shutdown, _) = watch::channel(false);
        Ok(Self {
            binary,
            private_root: private_root
                .canonicalize()
                .map_err(|_| Failure::WorkerUnavailable)?,
            tools,
            limits,
            permit: Arc::new(Semaphore::new(1)),
            shutdown,
            healthy: Arc::new(AtomicBool::new(true)),
        })
    }
    pub fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::Acquire)
    }
    /// Capacity/cleanup status only, not a substitute for provider probe proof
    /// or current Company, release, purchase, Files and IAM authority.
    pub fn can_accept_work(&self) -> bool {
        self.is_healthy() && !*self.shutdown.borrow() && self.permit.available_permits() > 0
    }
    pub async fn current_provider_profile(
        &self,
    ) -> Result<super::document_readiness::DocumentProviderProfile, Failure> {
        if !self.is_healthy() || *self.shutdown.borrow() {
            return Err(Failure::WorkerUnavailable);
        }
        super::document_readiness::inspect_profile(self.binary.clone(), self.tools.clone()).await
    }
    /// Bootstrap outside every database lease. A profile is observations of
    /// configured executable/model bytes, not an authorization/accuracy claim.
    pub async fn probe_readiness(
        &self,
        clock: &(impl erpx_server_shared::public::Clock + Sync),
    ) -> Result<super::document_readiness::DocumentProviderReadiness, Failure> {
        if !self.can_accept_work() {
            return Err(Failure::WorkerUnavailable);
        }
        let profile = self.current_provider_profile().await?;
        let mut probes = Vec::new();
        for (name, input) in super::document_readiness::inputs()? {
            let report = self
                .extract_with_budget(input, DocumentComputeBudget::new(15_000)?)
                .await?;
            super::document_readiness::require_probe(name, &report)?;
            probes.push(name.to_owned());
        }
        if !self.can_accept_work() || self.current_provider_profile().await? != profile {
            return Err(Failure::InvalidToolConfiguration);
        }
        Ok(super::document_readiness::ready(
            profile,
            clock.now().get(),
            probes,
        ))
    }
    pub async fn shutdown_and_drain(&self) {
        self.shutdown.send_replace(true);
        if let Ok(permit) = self.permit.acquire().await {
            drop(permit);
        }
    }
}
impl DocumentExtractor for BoundedDocumentExtractor {
    async fn extract_with_budget(
        &self,
        input: DocumentExtractionInput,
        budget: DocumentComputeBudget,
    ) -> Result<DocumentExtractionReport, Failure> {
        if *self.shutdown.borrow() || !self.is_healthy() {
            return Err(Failure::WorkerUnavailable);
        }
        if input.bytes().len() > DOCUMENT_INPUT_LIMIT {
            return Err(Failure::InputSizeLimit);
        }
        if hex::encode(Sha256::digest(input.bytes())) != input.checksum_sha256.to_ascii_lowercase()
        {
            return Err(Failure::InputChecksum);
        }
        if let DocumentExtractionSource::InlineText { body } = &input.source {
            return finish_document_extraction(
                &input.checksum_sha256,
                DocumentExtractionKind::InlineText,
                ExtractedDocument {
                    method: ExtractionMethod::Inline,
                    text: body.clone(),
                    pages: Vec::new(),
                },
            );
        }
        let permit = self
            .permit
            .clone()
            .try_acquire_owned()
            .map_err(|_| Failure::WorkerUnavailable)?;
        if *self.shutdown.borrow() || !self.is_healthy() {
            return Err(Failure::WorkerUnavailable);
        }
        let adapter = self.clone();
        let deadline = tokio::time::Instant::now()
            + Duration::from_millis(budget.millis()).min(self.limits.deadline);
        let (mut sender, receiver) = oneshot::channel();
        tokio::spawn(async move {
            let result = match prepare(&adapter, input) {
                Ok(prepared) if !sender.is_closed() && !*adapter.shutdown.borrow() => {
                    run(&adapter, prepared, &mut sender, deadline).await
                }
                Ok(_) => Err(Failure::WorkerCancelled),
                Err(error) => Err(error),
            };
            drop(permit);
            let _ = sender.send(result);
        });
        receiver.await.unwrap_or(Err(Failure::WorkerFailed))
    }
}
struct Prepared {
    directory: TempDir,
    request: DocumentChildRequest,
}
fn prepare(
    adapter: &BoundedDocumentExtractor,
    input: DocumentExtractionInput,
) -> Result<Prepared, Failure> {
    let mut builder = tempfile::Builder::new();
    builder.prefix("erpx-document-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o700));
    }
    let directory = builder
        .tempdir_in(&adapter.private_root)
        .map_err(|_| Failure::WorkerUnavailable)?;
    let DocumentExtractionSource::Original { media_type, bytes } = input.source else {
        return Err(Failure::WorkerProtocol);
    };
    private_file(&directory.path().join("input"), &bytes)?;
    let request = DocumentChildRequest {
        schema: 1,
        media_type,
        checksum_sha256: input.checksum_sha256.to_ascii_lowercase(),
        tools: adapter.tools.clone(),
    };
    let bytes = serde_json::to_vec(&request).map_err(|_| Failure::InvalidToolConfiguration)?;
    if bytes.len() > 64 * 1024 {
        return Err(Failure::InvalidToolConfiguration);
    }
    private_file(&directory.path().join("request.json"), &bytes)?;
    Ok(Prepared { directory, request })
}
fn private_file(path: &Path, bytes: &[u8]) -> Result<(), Failure> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .and_then(|mut file| file.write_all(bytes))
        .map_err(|_| Failure::WorkerUnavailable)
}
async fn run(
    adapter: &BoundedDocumentExtractor,
    prepared: Prepared,
    sender: &mut oneshot::Sender<Result<DocumentExtractionReport, Failure>>,
    deadline: tokio::time::Instant,
) -> Result<DocumentExtractionReport, Failure> {
    if tokio::time::Instant::now() >= deadline {
        return Err(Failure::DeadlineExceeded);
    }
    let mut command = Command::new(&adapter.binary);
    command
        .arg(prepared.directory.path().join("input"))
        .arg(prepared.directory.path().join("request.json"))
        .current_dir(prepared.directory.path())
        .env_clear()
        .env("LANG", "C")
        .env("TZ", "UTC")
        .env("OMP_THREAD_LIMIT", "1")
        .env("OMP_NUM_THREADS", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
    }
    let mut child = command.spawn().map_err(|_| Failure::WorkerUnavailable)?;
    let pid = child.id().ok_or(Failure::WorkerFailed)?;
    let stdout = child.stdout.take().ok_or(Failure::WorkerFailed)?;
    let stderr = child.stderr.take().ok_or(Failure::WorkerFailed)?;
    let counter = Arc::new(AtomicUsize::new(0));
    let mut shutdown = adapter.shutdown.subscribe();
    let work = async {
        let (out, _) = tokio::try_join!(
            read(stdout, counter.clone(), adapter.limits.output_bytes),
            read(stderr, counter, adapter.limits.output_bytes)
        )?;
        // EOF alone does not establish process exit. Observe an unreaped
        // zombie, under the same aggregate deadline, before normal cleanup.
        wait_leader_exit(pid).await?;
        Ok(out)
    };
    let deadline = if prepared.request.media_type
        == crate::domain::document_extraction::LegalDocumentMediaType::Docx
    {
        deadline.min(tokio::time::Instant::now() + Duration::from_secs(30))
    } else {
        deadline
    };
    let result = tokio::select! {
        biased;
        _=sender.closed()=>Err(Failure::WorkerCancelled),
        _=shutdown.wait_for(|stop|*stop)=>Err(Failure::WorkerCancelled),
        _=tokio::time::sleep_until(deadline)=>Err(Failure::DeadlineExceeded),
        result=work=>result,
        error=monitor_group(pid,adapter.limits.memory_bytes)=>Err(error),
    };
    // Never wait/try_wait the leader before this completes. Even if the leader
    // exited, its unreaped PID anchors the owned process group during signals.
    let clean = cleanup_group(pid, &adapter.healthy).await;
    let status = child.wait().await.map_err(|_| Failure::WorkerFailed)?;
    clean?;
    let output = result?;
    if !status.success() {
        return Err(Failure::WorkerFailed);
    }
    let response: DocumentChildResponse =
        serde_json::from_slice(&output).map_err(|_| Failure::WorkerProtocol)?;
    match response {
        DocumentChildResponse::Failed { failure } => Err(failure),
        DocumentChildResponse::Extracted { report } => {
            require_extraction_report(
                &prepared.request.checksum_sha256,
                DocumentExtractionKind::Original(prepared.request.media_type),
                &report,
            )?;
            Ok(report)
        }
    }
}
async fn read(
    mut stream: impl tokio::io::AsyncRead + Unpin,
    counter: Arc<AtomicUsize>,
    limit: usize,
) -> Result<Vec<u8>, Failure> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let count = stream
            .read(&mut buffer)
            .await
            .map_err(|_| Failure::WorkerFailed)?;
        if count == 0 {
            return Ok(bytes);
        }
        if counter.fetch_add(count, Ordering::Relaxed) + count > limit {
            return Err(Failure::WorkerOutputLimit);
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
}
#[derive(Clone, Copy)]
struct GroupProcess {
    pid: i32,
    parent: i32,
    memory: u64,
    zombie: bool,
}
#[cfg(target_os = "macos")]
fn group_members(group: u32) -> Result<Vec<GroupProcess>, Failure> {
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
fn group_members(group: u32) -> Result<Vec<GroupProcess>, Failure> {
    let mut members = Vec::new();
    for entry in fs::read_dir("/proc")
        .map_err(|_| Failure::WorkerUnavailable)?
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
fn group_members(_: u32) -> Result<Vec<GroupProcess>, Failure> {
    Err(Failure::WorkerUnavailable)
}
async fn monitor_group(group: u32, limit: u64) -> Failure {
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
                    return Failure::WorkerMemoryLimit;
                }
            }
            Err(error) => return error,
        }
    }
}
async fn wait_leader_exit(group: u32) -> Result<(), Failure> {
    use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
    let pid = Pid::from_raw(group as i32).ok_or(Failure::WorkerFailed)?;
    loop {
        match waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG,
        ) {
            Ok(Some(_)) => return Ok(()),
            Ok(None) => {}
            Err(error) if error == rustix::io::Errno::INTR => continue,
            Err(_) => return Err(Failure::WorkerFailed),
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}
async fn cleanup_group(group: u32, healthy: &AtomicBool) -> Result<(), Failure> {
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
                    Err(Failure::WorkerCleanupFailed)
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
        Err(Failure::WorkerCleanupFailed)
    }
}
