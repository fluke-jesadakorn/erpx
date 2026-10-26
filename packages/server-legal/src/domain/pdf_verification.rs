use erpx_server_shared::public::{CompanyId, HoldingId, RecordId, UserId};

use super::pdf_validation::PdfValidationReport;

pub const PDF_BUSINESS_LIMIT: usize = 10 * 1024 * 1024;
pub const VERIFICATION_ADMISSION_LIMIT: usize = 32 * 1024 * 1024;
pub const VERIFICATION_OUTPUT_LIMIT: usize = 2 * 1024 * 1024;
pub const VERIFICATION_DEADLINE_SECONDS: u64 = 90;
pub const TRUST_ITEM_LIMIT: usize = 100;
pub const TRUST_ITEM_BYTES_LIMIT: usize = 4 * 1024 * 1024;

/// A durable job identifies records. It carries neither paths nor caller-selected trust.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PdfVerificationJob {
    pub job_id: RecordId,
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub document_version_id: RecordId,
    pub requested_by: UserId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrustMaterialKind {
    Root,
    Intermediate,
    Crl,
    Ocsp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrustMaterialState {
    Proposed,
    Approved,
    Retired,
}

/// Loaded by an authoritative repository, never deserialized from a public request.
/// `independent_person_review` requires the existing person identity check, not
/// merely two different user IDs that could belong to the same person.
#[derive(Clone, Debug)]
pub struct ApprovedCompanyTrustMaterial {
    pub id: RecordId,
    pub company_id: CompanyId,
    pub version: u64,
    pub kind: TrustMaterialKind,
    pub state: TrustMaterialState,
    pub created_by: UserId,
    pub approved_by: Option<UserId>,
    pub independent_person_review: bool,
    pub checksum_sha256: String,
    pub content: Vec<u8>,
}

/// Immutable bytes and authority revisions read together after authorization.
/// The repository bounds reads before allocating this snapshot.
#[derive(Clone, Debug)]
pub struct AuthorizedPdfSnapshot {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub document_version_id: RecordId,
    pub document_checksum_sha256: String,
    pub authorization_revision: String,
    pub trust_revision: String,
    pub pdf: Vec<u8>,
    pub trust: Vec<ApprovedCompanyTrustMaterial>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrustMaterialReceipt {
    pub id: RecordId,
    pub version: u64,
    pub kind: TrustMaterialKind,
    pub checksum_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PdfVerificationSnapshotStamp {
    pub document_checksum_sha256: String,
    pub authorization_revision: String,
    pub trust_revision: String,
    pub trust: Vec<TrustMaterialReceipt>,
}

#[derive(Clone, Debug)]
pub struct PdfVerificationEvidence {
    pub snapshot: PdfVerificationSnapshotStamp,
    pub report: PdfValidationReport,
    pub inspection_id: Option<RecordId>,
}

/// Legal-owned immutable metadata. Canonical Files access is supplied by an
/// owner port; a raw object key does not authorize an original read.
#[derive(Clone)]
pub struct LegalInspectionDocument {
    pub id: RecordId,
    pub item_id: RecordId,
    pub company_id: CompanyId,
    pub version: u64,
    pub checksum_sha256: String,
    pub bytes: u64,
    pub media_type: String,
    pub object_key: Option<String>,
    pub attachment_link_id: Option<RecordId>,
}

pub struct LegalOriginalRead {
    pub bytes: Vec<u8>,
    pub witness: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PdfVerificationFailure {
    Authorization,
    SnapshotChanged,
    InvalidScope,
    InvalidSnapshot,
    DocumentSizeLimit,
    AdmissionLimit,
    UnapprovedTrust,
    TrustChecksum,
    InvalidTrust,
    InvalidValidationTime,
    WorkerUnavailable,
    WorkerTimeout,
    WorkerOutputLimit,
    WorkerMemoryLimit,
    WorkerFailed,
    WorkerProtocol,
    WorkerCancelled,
}

impl PdfVerificationFailure {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Authorization => "PDF_VERIFICATION_UNAUTHORIZED",
            Self::SnapshotChanged => "PDF_VERIFICATION_SNAPSHOT_CHANGED",
            Self::InvalidScope => "PDF_VERIFICATION_SCOPE_MISMATCH",
            Self::InvalidSnapshot => "PDF_VERIFICATION_INVALID_SNAPSHOT",
            Self::DocumentSizeLimit => "PDF_SIZE_LIMIT",
            Self::AdmissionLimit => "VERIFIER_ADMISSION_LIMIT",
            Self::UnapprovedTrust => "TRUST_MATERIAL_NOT_INDEPENDENTLY_APPROVED",
            Self::TrustChecksum => "TRUST_MATERIAL_CHECKSUM_MISMATCH",
            Self::InvalidTrust => "TRUST_MATERIAL_INVALID",
            Self::InvalidValidationTime => "INVALID_VALIDATION_TIME",
            Self::WorkerUnavailable => "VERIFIER_UNAVAILABLE",
            Self::WorkerTimeout => "VERIFIER_DEADLINE_EXCEEDED",
            Self::WorkerOutputLimit => "VERIFIER_OUTPUT_LIMIT",
            Self::WorkerMemoryLimit => "VERIFIER_MEMORY_LIMIT",
            Self::WorkerFailed => "VERIFIER_PROCESS_FAILED",
            Self::WorkerProtocol => "VERIFIER_PROTOCOL_INVALID",
            Self::WorkerCancelled => "VERIFIER_CANCELLED",
        }
    }
}
