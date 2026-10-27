//! Three short owner phases. No database connection or transaction-bound proof
//! may be stored in Prepared; it contains only owned lease/input data. Neither
//! computation data nor a provider report is a permission to publish or read.
use crate::domain::attachment_scan::{AttachmentScanScope, changed, remaining_compute_millis};
use erpx_server_shared::public::{BusinessError, Clock};
use std::future::Future;

/// Opaque data returned only by the Files PostgreSQL claim implementation. Its
/// lease identity survives compute, but its current authority is reminted by
/// every subsequent short transaction. No Clone/Deserialize authority DTO.
pub struct AttachmentScanClaim {
    pub(crate) scope: AttachmentScanScope,
    pub(crate) database: String,
    pub(crate) database_oid: i64,
    pub(crate) postmaster: String,
    pub(crate) job_id: String,
    pub(crate) source_id: String,
    pub(crate) token: String,
    pub(crate) attempt: i64,
    pub(crate) claimed_at: i64,
    pub(crate) claim_until: i64,
}
impl AttachmentScanClaim {
    pub(crate) fn copy_binding_data(&self) -> Self {
        Self {
            scope: self.scope.clone(),
            database: self.database.clone(),
            database_oid: self.database_oid,
            postmaster: self.postmaster.clone(),
            job_id: self.job_id.clone(),
            source_id: self.source_id.clone(),
            token: self.token.clone(),
            attempt: self.attempt,
            claimed_at: self.claimed_at,
            claim_until: self.claim_until,
        }
    }
    pub fn source_id(&self) -> &str {
        &self.source_id
    }
    pub fn attempt(&self) -> i64 {
        self.attempt
    }
    pub fn claim_until(&self) -> i64 {
        self.claim_until
    }
}

/// Constructible data identity is intentionally not an authority witness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentScanInputIdentity {
    pub source_sha256: String,
    pub checksum_sha256: String,
    pub media_type: String,
    pub bytes: usize,
}
pub struct PreparedAttachmentScan {
    pub(crate) claim: AttachmentScanClaim,
    pub(crate) identity: AttachmentScanInputIdentity,
    pub(crate) original: QuarantinedOriginalInput,
}
impl PreparedAttachmentScan {
    pub fn claim(&self) -> &AttachmentScanClaim {
        &self.claim
    }
    pub fn identity(&self) -> &AttachmentScanInputIdentity {
        &self.identity
    }
    pub fn original(&self) -> &QuarantinedOriginalInput {
        &self.original
    }
    pub fn remaining_compute_millis(&self, clock: &impl Clock) -> Result<u64, BusinessError> {
        remaining_compute_millis(self.claim.claim_until, clock.now().get())
    }
}

/// Files-owned prepared source data. A caller path/URL cannot construct this
/// input; the current scan source reader mints it before outside-DB I/O.
pub struct QuarantinedOriginalInput {
    pub(crate) key: String,
    pub(crate) bytes: usize,
    pub(crate) checksum_sha256: String,
}
impl QuarantinedOriginalInput {
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    pub fn checksum_sha256(&self) -> &str {
        &self.checksum_sha256
    }
}
pub enum QuarantinedReadFailure {
    Missing,
    Oversize,
    Metadata,
    Checksum,
    Unavailable,
}
impl QuarantinedReadFailure {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Missing => "ATTACHMENT_OBJECT_MISSING",
            Self::Oversize => "ATTACHMENT_SCAN_OVERSIZE",
            Self::Metadata => "ATTACHMENT_OBJECT_METADATA",
            Self::Checksum => "ATTACHMENT_CHECKSUM_MISMATCH",
            Self::Unavailable => "ATTACHMENT_OBJECT_UNAVAILABLE",
        }
    }
}
pub trait QuarantinedOriginalStore: Send + Sync {
    fn read(
        &self,
        input: &QuarantinedOriginalInput,
    ) -> impl Future<Output = Result<Vec<u8>, QuarantinedReadFailure>> + Send;
}

pub struct PreviewPublication {
    pub(crate) key: String,
    pub(crate) source_sha256: String,
    pub(crate) artifact: std::sync::Arc<crate::domain::preview::PreviewArtifact>,
    pub(crate) uploaded_at: i64,
}
impl PreviewPublication {
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }
    pub fn artifact(&self) -> &crate::domain::preview::PreviewArtifact {
        &self.artifact
    }
}
pub trait ImmutablePreviewStore: Send + Sync {
    fn publish_immutable(
        &self,
        publication: PreviewPublication,
    ) -> impl Future<Output = Result<(), BusinessError>> + Send;
}

/// Evidence-only successful provider outcome; the fresh final transaction
/// validates the lease, the exact original source identity and any preview.
pub enum AttachmentScanEvidence {
    Clean {
        engine: String,
        definition_updated_at: i64,
        preview: Option<StagedAttachmentPreview>,
    },
    Infected {
        engine: String,
        signature: String,
        definition_updated_at: i64,
    },
}
/// Opaque durable staging data. It is deliberately not a transaction proof or
/// a read/publication grant, and never deserializes from a request.
pub struct StagedAttachmentPreview {
    pub(crate) key: String,
    pub(crate) source_sha256: String,
    pub(crate) source_data_sha256: String,
    pub(crate) checksum_sha256: String,
    pub(crate) media_type: String,
    pub(crate) bytes: i64,
    pub(crate) width: u32,
    pub(crate) height: u32,
}
pub async fn stage_attachment_preview<S: ImmutablePreviewStore, C: Clock>(
    prepared: &PreparedAttachmentScan,
    artifact: crate::domain::preview::PreviewArtifact,
    store: &S,
    clock: &C,
) -> Result<StagedAttachmentPreview, BusinessError> {
    prepared.remaining_compute_millis(clock)?;
    let source_media =
        crate::domain::preview::PreviewSourceMedia::try_from(prepared.identity.media_type.as_str())
            .map_err(|_| changed())?;
    use crate::domain::preview::{PreviewMedia, PreviewSourceMedia};
    if !matches!(
        (source_media, artifact.media()),
        (PreviewSourceMedia::Pdf, PreviewMedia::Jpeg)
            | (
                PreviewSourceMedia::Png
                    | PreviewSourceMedia::Jpeg
                    | PreviewSourceMedia::Webp
                    | PreviewSourceMedia::Heic
                    | PreviewSourceMedia::Heif,
                PreviewMedia::Webp
            )
    ) {
        return Err(changed());
    }
    let key = crate::domain::attachment_scan::preview_object_key(
        &prepared.original.key,
        artifact.checksum_sha256(),
        artifact.media().as_mime(),
    )?;
    let artifact = std::sync::Arc::new(artifact);
    store
        .publish_immutable(PreviewPublication {
            key: key.clone(),
            source_sha256: prepared.identity.checksum_sha256.clone(),
            artifact: artifact.clone(),
            uploaded_at: clock.now().get(),
        })
        .await?;
    prepared.remaining_compute_millis(clock)?;
    Ok(StagedAttachmentPreview {
        key,
        source_sha256: prepared.identity.checksum_sha256.clone(),
        source_data_sha256: prepared.identity.source_sha256.clone(),
        checksum_sha256: artifact.checksum_sha256().into(),
        media_type: artifact.media().as_mime().into(),
        bytes: i64::try_from(artifact.bytes().len()).map_err(|_| changed())?,
        width: artifact.width(),
        height: artifact.height(),
    })
}

/// A bounded static native failure code, with no decoder stderr/path, actor,
/// original data or environment disclosure. Cancellation is never a failure
/// report: it leaves the same attempt recoverable after lease expiry.
pub struct AttachmentScanFailure {
    pub(crate) code: String,
}
impl AttachmentScanFailure {
    pub fn code(&self) -> &str {
        &self.code
    }
    pub fn from_business(error: &BusinessError) -> Result<Self, BusinessError> {
        let code = error.code;
        if code.is_empty()
            || code.len() > 160
            || !code
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        {
            return Err(changed());
        }
        Ok(Self {
            code: code.to_owned(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentScanSettlement {
    Available,
    MalwareRejected,
    Error { next_attempt: Option<i64> },
}
pub enum AttachmentScanComputeError {
    Failed(AttachmentScanFailure),
    Interrupted,
}
impl From<AttachmentScanFailure> for AttachmentScanComputeError {
    fn from(value: AttachmentScanFailure) -> Self {
        Self::Failed(value)
    }
}
pub enum AttachmentScanRunOutcome {
    NoWork,
    Settled(AttachmentScanSettlement),
    Interrupted,
}

/// Actual implementation probes both configured malware and preview providers.
/// A forged HTTP readiness DTO cannot select this internal trusted port.
pub trait AttachmentSanitizationProvider: Send + Sync {
    fn require_ready(
        &self,
        clock: &(impl Clock + Sync),
    ) -> impl Future<Output = Result<(), BusinessError>> + Send;
    fn compute(
        &self,
        input: &PreparedAttachmentScan,
        clock: &(impl Clock + Sync),
    ) -> impl Future<Output = Result<AttachmentScanEvidence, AttachmentScanComputeError>> + Send;
}

/// Composition starts a NEW Holding SERIALIZABLE unit for each call. The owner
/// repository borrows its connection; it never begins/commits/retries/closes it.
///
/// claim_and_prepare commits one actual 60s claim plus owned source data.
/// retain_current/fail_current freshly acquire the same job/token/source and
/// reject stale workers before *any* source/upload/event/ledger side effect.
/// Mutable draft/bound/withdrawn state is not an invented sanitization denial.
pub trait AttachmentScanTransactions: Send + Sync {
    type Error: From<BusinessError> + Send;
    fn claim_and_prepare(
        &self,
        source_filter: Option<&str>,
    ) -> impl Future<Output = Result<Option<PreparedAttachmentScan>, Self::Error>> + Send;
    fn retain_current(
        &self,
        prepared: PreparedAttachmentScan,
        evidence: AttachmentScanEvidence,
    ) -> impl Future<Output = Result<AttachmentScanSettlement, Self::Error>> + Send;
    fn fail_current(
        &self,
        prepared: PreparedAttachmentScan,
        failure: AttachmentScanFailure,
    ) -> impl Future<Output = Result<AttachmentScanSettlement, Self::Error>> + Send;
}

pub async fn run_next_attachment_scan<T, P, C>(
    transactions: &T,
    provider: &P,
    clock: &C,
    source_filter: Option<&str>,
) -> Result<AttachmentScanRunOutcome, T::Error>
where
    T: AttachmentScanTransactions,
    P: AttachmentSanitizationProvider,
    C: Clock + Sync,
{
    provider.require_ready(clock).await?;
    let Some(prepared) = transactions.claim_and_prepare(source_filter).await? else {
        return Ok(AttachmentScanRunOutcome::NoWork);
    };
    // This await owns only bytes/data; no Control/Holding connection or witness
    // crosses into a decoder process, socket scan or cancellation cleanup.
    match provider.compute(&prepared, clock).await {
        Ok(evidence) => transactions
            .retain_current(prepared, evidence)
            .await
            .map(AttachmentScanRunOutcome::Settled),
        Err(AttachmentScanComputeError::Failed(failure)) => transactions
            .fail_current(prepared, failure)
            .await
            .map(AttachmentScanRunOutcome::Settled),
        Err(AttachmentScanComputeError::Interrupted) => Ok(AttachmentScanRunOutcome::Interrupted),
    }
    // A retention/SQL error is returned, not converted into scanner evidence or
    // another write. Outer UoW alone sees 40001/40P01 and retries whole phases.
}
