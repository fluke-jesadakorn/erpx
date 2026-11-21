use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use serde::{Deserialize, Serialize};

pub const PROCESSING_LEASE_MILLIS: i64 = 600_000;
pub const PROCESSING_ATTEMPTS: u32 = 3;
pub const PROCESSING_SCAN_LIMIT: usize = 64;
pub const PROCESSING_ACTOR_LIMIT: usize = 1024 * 1024;
pub const PROCESSING_RESULT_LIMIT: usize = 3 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LegalProcessingKind {
    Ocr,
    Verify,
}
impl LegalProcessingKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ocr => "ocr",
            Self::Verify => "verify",
        }
    }
    pub const fn operation(self) -> &'static str {
        match self {
            Self::Ocr => "legal.processing.ocr",
            Self::Verify => "legal.processing.verify",
        }
    }
}

/// Selection facts only. Current permission, assignment, purchase and readiness
/// must be proved by the retained owner bridge before replay and completion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalProcessingActor {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub user_id: UserId,
    pub selection_revision: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalProcessingDocument {
    pub item_id: RecordId,
    pub document_id: RecordId,
    pub checksum_sha256: String,
    pub item_version: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalProcessingQueueAuthority {
    pub revision: String,
    pub actor_snapshot_json: String,
}

pub struct PreparedLegalProcessingQueue {
    pub document: LegalProcessingDocument,
    pub authority: LegalProcessingQueueAuthority,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalProcessingJob {
    pub id: RecordId,
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub document: LegalProcessingDocument,
    pub kind: LegalProcessingKind,
    /// Stored selection is untrusted historical input, never cached authority.
    pub actor_snapshot_json: String,
    pub created_by: UserId,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalProcessingLease {
    pub job: LegalProcessingJob,
    pub token: RecordId,
    pub until: i64,
    pub attempts: u32,
}
impl LegalProcessingLease {
    pub fn require_live(
        &self,
        holding: &HoldingId,
        company: &CompanyId,
        now: i64,
    ) -> Result<(), BusinessError> {
        if &self.job.holding_id != holding || &self.job.company_id != company {
            return Err(scope_error());
        }
        if self.until < now || self.attempts == 0 || self.attempts > PROCESSING_ATTEMPTS {
            return Err(lease_changed());
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct LegalProcessingCursor {
    pub created_at: i64,
    pub id: RecordId,
}
#[derive(Clone, Debug)]
pub struct LegalProcessingScan {
    pub lease: Option<LegalProcessingLease>,
    /// A worker must continue from this cursor after skipped candidates and
    /// reset it after an empty page. It must not repeatedly scan only the head.
    pub next_cursor: Option<LegalProcessingCursor>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegalProcessingReceipt {
    pub id: RecordId,
    pub job_id: RecordId,
    pub state: String,
}

#[derive(Clone, Debug)]
pub struct LegalProcessingStatus {
    pub id: RecordId,
    pub item_id: RecordId,
    pub document_id: RecordId,
    pub state: String,
    pub attempts: u32,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

pub(crate) fn validate_authority(value: &str) -> Result<(), BusinessError> {
    if value.is_empty() || value.len() > 200 {
        return Err(scope_error());
    }
    Ok(())
}
pub(crate) fn validate_checksum(value: &str) -> Result<(), BusinessError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(BusinessError::invalid(
            "LEGAL_PROCESSING_SOURCE_INVALID",
            "Invalid processing source.",
        ));
    }
    Ok(())
}
pub(crate) fn lease_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "LEGAL_PROCESSING_LEASE_CHANGED",
        "The processing lease changed or expired.",
    )
}
pub(crate) fn scope_error() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "LEGAL_PROCESSING_SCOPE_MISMATCH",
        "The Legal processing scope changed.",
    )
}
