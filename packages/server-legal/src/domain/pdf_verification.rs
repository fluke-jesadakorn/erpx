use erpx_server_shared::public::{CompanyId, HoldingId, RecordId, UserId};

use super::pdf_validation::PdfValidationReport;
#[cfg(feature = "pdf-verifier")]
use super::processing::LegalProcessingLease;

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
    /// Internal dated computation provenance, never a current authority grant.
    pub computation: Option<PdfComputationReceipt>,
}

#[derive(Clone, Debug)]
pub struct PdfComputationReceipt {
    pub job_id: RecordId,
    pub attempt: u32,
    pub document_version_id: RecordId,
    pub document_version: u64,
    pub document_checksum_sha256: String,
    pub original_source_data_sha256: String,
    pub prepared_at: i64,
}

/// Owned computation input, not a database/authority witness. Constructors stay
/// inside Legal; no connection, opaque source token or file witness is carried.
#[cfg(feature = "pdf-verifier")]
pub struct PreparedLegalPdf {
    pub(crate) lease: LegalProcessingLease,
    pub(crate) document: LegalInspectionDocument,
    pub(crate) source_data_sha256: String,
    pub(crate) prepared_at: i64,
    pub(crate) snapshot: AuthorizedPdfSnapshot,
}
#[cfg(feature = "pdf-verifier")]
impl PreparedLegalPdf {
    pub fn lease(&self) -> &LegalProcessingLease {
        &self.lease
    }
    pub fn document(&self) -> &LegalInspectionDocument {
        &self.document
    }
    pub fn source_data_sha256(&self) -> &str {
        &self.source_data_sha256
    }
    pub fn prepared_at(&self) -> i64 {
        self.prepared_at
    }
}

/// Immutable, dated evidence from computation outside both transactions. It
/// grants no authority and final retention must remint all current witnesses.
#[derive(Clone)]
#[cfg(feature = "pdf-verifier")]
pub struct ComputedLegalPdf {
    pub(crate) lease: LegalProcessingLease,
    pub(crate) document: LegalInspectionDocument,
    pub(crate) source_data_sha256: String,
    pub(crate) prepared_at: i64,
    pub(crate) evidence: PdfVerificationEvidence,
}
#[cfg(feature = "pdf-verifier")]
impl ComputedLegalPdf {
    pub fn lease(&self) -> &LegalProcessingLease {
        &self.lease
    }
    pub fn document(&self) -> &LegalInspectionDocument {
        &self.document
    }
    pub fn source_data_sha256(&self) -> &str {
        &self.source_data_sha256
    }
    pub fn report(&self) -> &PdfValidationReport {
        &self.evidence.report
    }
    pub fn prepared_at(&self) -> i64 {
        self.prepared_at
    }
}

/// Legal-owned immutable metadata. Canonical Files access is supplied by an
/// owner port; a raw object key does not authorize an original read.
#[derive(Clone, Debug, Eq, PartialEq)]
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
    /// Stable source-data identity only, excluding transaction/current authority.
    /// Final retention remints an authorized read and compares this data digest.
    pub source_data_sha256: String,
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
