use super::{
    document_extraction::{
        DocumentComputeBudget, DocumentExtractionFailure, DocumentExtractionInput,
        DocumentExtractionReport, EXTRACTION_RETENTION_HEADROOM_MILLIS,
    },
    pdf_verification::LegalInspectionDocument,
    processing::LegalProcessingLease,
};
use erpx_server_shared::public::{BusinessError, FailureKind};

/// Stable input data only. No transaction, Files witness, purchase witness or
/// permission-bearing context can escape the short preparation transaction.
pub struct PreparedLegalExtraction {
    pub(crate) lease: LegalProcessingLease,
    pub(crate) document: LegalInspectionDocument,
    pub(crate) source_data_sha256: String,
    pub(crate) prepared_at: i64,
    pub(crate) input: DocumentExtractionInput,
}
impl PreparedLegalExtraction {
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
#[derive(Clone)]
pub struct ComputedLegalExtraction {
    pub(crate) lease: LegalProcessingLease,
    pub(crate) document: LegalInspectionDocument,
    pub(crate) source_data_sha256: String,
    pub(crate) prepared_at: i64,
    pub(crate) computed_at: i64,
    pub(crate) outcome: Result<DocumentExtractionReport, DocumentExtractionFailure>,
}
impl ComputedLegalExtraction {
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
    pub fn report(&self) -> Option<&DocumentExtractionReport> {
        self.outcome.as_ref().ok()
    }
    pub fn failure(&self) -> Option<DocumentExtractionFailure> {
        self.outcome.as_ref().err().copied()
    }
    pub fn computed_at(&self) -> i64 {
        self.computed_at
    }
}
/// A retryable provider failure remains terminal for this job. Only interrupted
/// lease recovery requeues old work; this value grants no automatic retry.
pub enum LegalExtractionRetention {
    Completed(serde_json::Value),
    Failed { code: String, retryable: bool },
}
pub fn extraction_compute_budget(lease_until: i64, now: i64) -> Option<DocumentComputeBudget> {
    let remaining = lease_until
        .checked_sub(now)?
        .checked_sub(EXTRACTION_RETENTION_HEADROOM_MILLIS)?;
    let millis = u64::try_from(remaining)
        .ok()?
        .min(DocumentComputeBudget::maximum().millis());
    DocumentComputeBudget::new(millis).ok()
}
pub(crate) fn extraction_failure(error: DocumentExtractionFailure) -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        error.code(),
        "The bounded document extraction could not complete.",
    )
}
pub(crate) fn extraction_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "LEGAL_EXTRACTION_SOURCE_CHANGED",
        "The current Legal extraction source changed.",
    )
}
