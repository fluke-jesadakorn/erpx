//! An owned, offline verifier process. No shell, inherited secrets or caller paths.
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use erpx_server_shared::public::TimestampMillis;
use sha2::{Digest, Sha256};
#[cfg(target_os = "macos")]
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};
use tempfile::TempDir;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    sync::{Semaphore, oneshot, watch},
};

use crate::{
    adapters::pdf_validation::TrustMaterial,
    application::pdf_verification::{PdfVerifierProcess, validate_admission},
    domain::{
        pdf_validation::{PdfValidationReport, ValidationState, aggregate},
        pdf_verification::{
            AuthorizedPdfSnapshot, PdfVerificationEvidence, PdfVerificationFailure,
            PdfVerificationSnapshotStamp, TRUST_ITEM_BYTES_LIMIT, TrustMaterialKind,
            TrustMaterialReceipt, VERIFICATION_ADMISSION_LIMIT, VERIFICATION_DEADLINE_SECONDS,
            VERIFICATION_OUTPUT_LIMIT,
        },
    },
};

/// Operators may reduce these ceilings, including for disposable child tests.
#[derive(Clone, Copy, Debug)]
pub struct PdfProcessLimits {
    deadline: Duration,
    output_bytes: usize,
    memory_bytes: u64,
}

impl PdfProcessLimits {
    pub fn new(deadline: Duration, output_bytes: usize) -> Result<Self, PdfVerificationFailure> {
        if deadline.is_zero()
            || deadline > Duration::from_secs(VERIFICATION_DEADLINE_SECONDS)
            || output_bytes == 0
            || output_bytes > VERIFICATION_OUTPUT_LIMIT
        {
            return Err(PdfVerificationFailure::InvalidSnapshot);
        }
        Ok(Self {
            deadline,
            output_bytes,
            memory_bytes: 512 * 1024 * 1024,
        })
    }

    #[cfg(target_os = "macos")]
    pub fn with_memory_limit(mut self, bytes: u64) -> Result<Self, PdfVerificationFailure> {
        if !(16 * 1024 * 1024..=512 * 1024 * 1024).contains(&bytes) {
            return Err(PdfVerificationFailure::InvalidSnapshot);
        }
        self.memory_bytes = bytes;
        Ok(self)
    }
}

impl Default for PdfProcessLimits {
    fn default() -> Self {
        Self {
            deadline: Duration::from_secs(VERIFICATION_DEADLINE_SECONDS),
            output_bytes: VERIFICATION_OUTPUT_LIMIT,
            memory_bytes: 512 * 1024 * 1024,
        }
    }
}

/// One adapter owns at most one child; busy submissions are rejected rather than
/// accumulating document buffers in an unbounded local queue.
#[derive(Clone)]
pub struct BoundedPdfVerifier {
    binary: PathBuf,
    private_root: PathBuf,
    limits: PdfProcessLimits,
    permit: Arc<Semaphore>,
    shutdown: watch::Sender<bool>,
}

impl BoundedPdfVerifier {
    pub fn new(
        binary: &Path,
        private_root: &Path,
        limits: PdfProcessLimits,
    ) -> Result<Self, PdfVerificationFailure> {
        if !binary.is_absolute()
            || !private_root.is_absolute()
            || !cfg!(any(target_os = "linux", target_os = "macos"))
        {
            return Err(PdfVerificationFailure::WorkerUnavailable);
        }
        let root_metadata = fs::symlink_metadata(private_root)
            .map_err(|_| PdfVerificationFailure::WorkerUnavailable)?;
        if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
            return Err(PdfVerificationFailure::WorkerUnavailable);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if root_metadata.permissions().mode() & 0o077 != 0 {
                return Err(PdfVerificationFailure::WorkerUnavailable);
            }
        }
        let binary = binary
            .canonicalize()
            .map_err(|_| PdfVerificationFailure::WorkerUnavailable)?;
        if !binary.is_file() {
            return Err(PdfVerificationFailure::WorkerUnavailable);
        }
        let (shutdown, _) = watch::channel(false);
        Ok(Self {
            binary,
            private_root: private_root
                .canonicalize()
                .map_err(|_| PdfVerificationFailure::WorkerUnavailable)?,
            limits,
            permit: Arc::new(Semaphore::new(1)),
            shutdown,
        })
    }

    /// The runtime must await this before dropping its Tokio executor. New work
    /// is refused, and the owned active child is killed, reaped and cleaned up.
    pub async fn shutdown_and_drain(&self) {
        self.shutdown.send_replace(true);
        if let Ok(permit) = self.permit.acquire().await {
            drop(permit);
        }
    }
}

impl PdfVerifierProcess for BoundedPdfVerifier {
    async fn verify(
        &self,
        snapshot: AuthorizedPdfSnapshot,
        validation_time: TimestampMillis,
    ) -> Result<PdfVerificationEvidence, PdfVerificationFailure> {
        validate_admission(&snapshot)?;
        let permit = self
            .permit
            .clone()
            .try_acquire_owned()
            .map_err(|_| PdfVerificationFailure::WorkerUnavailable)?;
        if *self.shutdown.borrow() {
            return Err(PdfVerificationFailure::WorkerUnavailable);
        }
        let (mut sender, receiver) = oneshot::channel();
        let verifier = self.clone();
        // This task owns the child and temporary files even if the requesting
        // future is dropped. Receiver cancellation requests kill + reap.
        tokio::spawn(async move {
            let private_root = verifier.private_root.clone();
            let prepared = tokio::task::spawn_blocking(move || {
                prepare(&private_root, snapshot, validation_time)
            })
            .await
            .unwrap_or(Err(PdfVerificationFailure::WorkerFailed));
            let result = match prepared {
                Ok(prepared) if !sender.is_closed() && !*verifier.shutdown.borrow() => {
                    run_child(&verifier, prepared, &mut sender).await
                }
                Ok(_) => Err(PdfVerificationFailure::WorkerCancelled),
                Err(error) => Err(error),
            };
            drop(permit);
            let _ = sender.send(result);
        });
        receiver
            .await
            .unwrap_or(Err(PdfVerificationFailure::WorkerFailed))
    }
}

struct PreparedVerification {
    directory: TempDir,
    stamp: PdfVerificationSnapshotStamp,
    validation_time: String,
    trust_digest: String,
}

fn prepare(
    private_root: &Path,
    mut snapshot: AuthorizedPdfSnapshot,
    validation_time: TimestampMillis,
) -> Result<PreparedVerification, PdfVerificationFailure> {
    validate_admission(&snapshot)?;
    if hex::encode(Sha256::digest(&snapshot.pdf))
        != snapshot.document_checksum_sha256.to_ascii_lowercase()
    {
        return Err(PdfVerificationFailure::InvalidSnapshot);
    }
    let validation_time =
        OffsetDateTime::from_unix_timestamp_nanos(i128::from(validation_time.get()) * 1_000_000)
            .map_err(|_| PdfVerificationFailure::InvalidValidationTime)?
            .format(&Rfc3339)
            .map_err(|_| PdfVerificationFailure::InvalidValidationTime)?;
    let mut directory_builder = tempfile::Builder::new();
    directory_builder.prefix("erpx-pdf-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        directory_builder.permissions(fs::Permissions::from_mode(0o700));
    }
    let directory = directory_builder
        .tempdir_in(private_root)
        .map_err(|_| PdfVerificationFailure::WorkerUnavailable)?;
    private_file(&directory.path().join("document.pdf"), &snapshot.pdf)?;
    let trust_directory = directory.path().join("trust");
    for name in ["", "roots", "intermediates", "revocation"] {
        private_directory(&trust_directory.join(name))?;
    }
    snapshot.trust.sort_by(|left, right| left.id.cmp(&right.id));
    let mut materials = Vec::new();
    let mut receipts = Vec::new();
    let mut total = snapshot.pdf.len() + 2048;
    for (index, material) in snapshot.trust.into_iter().enumerate() {
        let checksum = hex::encode(Sha256::digest(&material.content));
        if checksum != material.checksum_sha256.to_ascii_lowercase() {
            return Err(PdfVerificationFailure::TrustChecksum);
        }
        let (directory_name, extension) = match material.kind {
            TrustMaterialKind::Root => ("roots", "pem"),
            TrustMaterialKind::Intermediate => ("intermediates", "pem"),
            TrustMaterialKind::Crl => ("revocation", "crl"),
            TrustMaterialKind::Ocsp => ("revocation", "ocsp"),
        };
        let content = if matches!(
            material.kind,
            TrustMaterialKind::Root | TrustMaterialKind::Intermediate
        ) && !material.content.starts_with(b"-----BEGIN CERTIFICATE-----")
        {
            // Database certificates are DER. Framing does not parse untrusted
            // ASN.1 in the parent; the bounded child validates the certificate.
            let base64 = openssl::base64::encode_block(&material.content);
            let mut pem = String::from("-----BEGIN CERTIFICATE-----\n");
            for line in base64.as_bytes().chunks(64) {
                pem.push_str(
                    std::str::from_utf8(line).map_err(|_| PdfVerificationFailure::InvalidTrust)?,
                );
                pem.push('\n');
            }
            pem.push_str("-----END CERTIFICATE-----\n");
            pem.into_bytes()
        } else {
            material.content
        };
        total = total
            .checked_add(content.len() + 1024)
            .ok_or(PdfVerificationFailure::AdmissionLimit)?;
        if content.len() > TRUST_ITEM_BYTES_LIMIT || total > VERIFICATION_ADMISSION_LIMIT {
            return Err(PdfVerificationFailure::AdmissionLimit);
        }
        let path = format!("{directory_name}/{index:04}.{extension}");
        private_file(&trust_directory.join(&path), &content)?;
        materials.push(TrustMaterial { path, content });
        receipts.push(TrustMaterialReceipt {
            id: material.id,
            version: material.version,
            kind: material.kind,
            checksum_sha256: checksum,
        });
    }
    let trust_digest = crate::adapters::pdf_validation::material_digest(&materials)
        .map_err(|_| PdfVerificationFailure::InvalidTrust)?;
    Ok(PreparedVerification {
        directory,
        stamp: PdfVerificationSnapshotStamp {
            document_checksum_sha256: snapshot.document_checksum_sha256.to_ascii_lowercase(),
            authorization_revision: snapshot.authorization_revision,
            trust_revision: snapshot.trust_revision,
            trust: receipts,
        },
        validation_time,
        trust_digest,
    })
}

fn private_directory(path: &Path) -> Result<(), PdfVerificationFailure> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(path)
        .map_err(|_| PdfVerificationFailure::WorkerUnavailable)
}

fn private_file(path: &Path, bytes: &[u8]) -> Result<(), PdfVerificationFailure> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file: File = options
        .open(path)
        .map_err(|_| PdfVerificationFailure::WorkerUnavailable)?;
    file.write_all(bytes)
        .map_err(|_| PdfVerificationFailure::WorkerUnavailable)
}

async fn run_child(
    verifier: &BoundedPdfVerifier,
    prepared: PreparedVerification,
    sender: &mut oneshot::Sender<Result<PdfVerificationEvidence, PdfVerificationFailure>>,
) -> Result<PdfVerificationEvidence, PdfVerificationFailure> {
    let mut command = Command::new(&verifier.binary);
    command
        .arg(prepared.directory.path().join("document.pdf"))
        .arg(prepared.directory.path().join("trust"))
        .arg(&prepared.validation_time)
        .current_dir(prepared.directory.path())
        .env_clear()
        .env("LANG", "C")
        .env("TZ", "UTC")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|_| PdfVerificationFailure::WorkerUnavailable)?;
    let pid = child.id().ok_or(PdfVerificationFailure::WorkerFailed)?;
    let stdout = child
        .stdout
        .take()
        .ok_or(PdfVerificationFailure::WorkerFailed)?;
    let stderr = child
        .stderr
        .take()
        .ok_or(PdfVerificationFailure::WorkerFailed)?;
    let counter = Arc::new(AtomicUsize::new(0));
    let mut shutdown = verifier.shutdown.subscribe();
    let result = {
        let work = async {
            let (stdout, _, status) = tokio::try_join!(
                read_bounded(stdout, counter.clone(), verifier.limits.output_bytes),
                read_bounded(stderr, counter, verifier.limits.output_bytes),
                async {
                    child
                        .wait()
                        .await
                        .map_err(|_| PdfVerificationFailure::WorkerFailed)
                }
            )?;
            if !status.success() {
                return Err(PdfVerificationFailure::WorkerFailed);
            }
            Ok(stdout)
        };
        tokio::select! {
            biased;
            _ = sender.closed() => Err(PdfVerificationFailure::WorkerCancelled),
            _ = shutdown.wait_for(|stopping| *stopping) => Err(PdfVerificationFailure::WorkerCancelled),
            _ = tokio::time::sleep(verifier.limits.deadline) => Err(PdfVerificationFailure::WorkerTimeout),
            result = work => result,
            error = monitor_owned_child_memory(pid, verifier.limits.memory_bytes) => Err(error),
        }
    };
    let stdout = match result {
        Ok(stdout) => stdout,
        Err(error) => {
            // Only this Tokio Child is signalled. No process-name or port kill.
            let _ = child.start_kill();
            let _ = child.wait().await;
            return Err(error);
        }
    };
    let report: PdfValidationReport =
        serde_json::from_slice(&stdout).map_err(|_| PdfVerificationFailure::WorkerProtocol)?;
    validate_report(&report, &prepared)?;
    Ok(PdfVerificationEvidence {
        computation: None,
        snapshot: prepared.stamp,
        report,
        inspection_id: None,
    })
}

#[cfg(target_os = "macos")]
async fn monitor_owned_child_memory(pid: u32, ceiling: u64) -> PdfVerificationFailure {
    let pid = Pid::from_u32(pid);
    let mut system = System::new_with_specifics(RefreshKind::nothing());
    let mut interval = tokio::time::interval(Duration::from_millis(25));
    loop {
        interval.tick().await;
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing().with_memory(),
        );
        // An exited child can disappear before its pipes/exit status are read.
        // Only the owned Tokio Child is ever killed, never a PID obtained here.
        if let Some(process) = system.process(pid) {
            let memory = process.memory();
            if memory == 0 {
                return PdfVerificationFailure::WorkerUnavailable;
            }
            if memory > ceiling {
                return PdfVerificationFailure::WorkerMemoryLimit;
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
async fn monitor_owned_child_memory(_: u32, _: u64) -> PdfVerificationFailure {
    // Linux's verifier executable applies hard DATA/AS limits before parsing.
    std::future::pending().await
}

async fn read_bounded(
    mut stream: impl AsyncRead + Unpin,
    counter: Arc<AtomicUsize>,
    limit: usize,
) -> Result<Vec<u8>, PdfVerificationFailure> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let count = stream
            .read(&mut buffer)
            .await
            .map_err(|_| PdfVerificationFailure::WorkerFailed)?;
        if count == 0 {
            return Ok(bytes);
        }
        if counter.fetch_add(count, Ordering::Relaxed) + count > limit {
            return Err(PdfVerificationFailure::WorkerOutputLimit);
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
}

fn validate_report(
    report: &PdfValidationReport,
    prepared: &PreparedVerification,
) -> Result<(), PdfVerificationFailure> {
    if report.validated_at != prepared.validation_time
        || report.trust_store_digest != prepared.trust_digest
        || report.mode != "offline-strict-revocation"
        || report.engine != "ERPX Rust / OpenSSL / lopdf"
        || report.signatures.len() > 128
        || report.status != aggregate(report.signatures.iter().map(|signature| signature.status))
        || (report.status == ValidationState::Valid
            && report.signatures.iter().any(|signature| {
                [
                    signature.integrity,
                    signature.required_certificate_chain,
                    signature.certificate_validity,
                    signature.revocation,
                    signature.timestamp,
                    signature.policy,
                ]
                .iter()
                .any(|state| *state != ValidationState::Valid)
            }))
    {
        return Err(PdfVerificationFailure::WorkerProtocol);
    }
    Ok(())
}
