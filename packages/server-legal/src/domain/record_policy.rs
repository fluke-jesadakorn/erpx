//! Record policy consumes current owner facts, never a serialized IAM actor.
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use std::collections::BTreeSet;

/// Trusted composition projects these facts from a borrowed sealed context.
/// This constructible value is data only and never proves current IAM authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalRecordActor {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub user_id: UserId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub role: String,
    pub permissions: BTreeSet<String>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegalRecordOperation {
    Metadata,
    Document,
    Export,
    Edit,
    QueueProcessing,
    ProcessingReplay,
    Processing,
}
impl LegalRecordOperation {
    pub fn permission(self) -> &'static str {
        match self {
            Self::Export => "legal.export",
            Self::Edit => "legal.edit",
            _ => "legal.read",
        }
    }
    pub fn document(self) -> bool {
        !matches!(self, Self::Metadata | Self::Edit)
    }
    pub fn processing(self) -> bool {
        matches!(
            self,
            Self::QueueProcessing | Self::Processing | Self::ProcessingReplay
        )
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegalCommercialMode {
    Disabled,
    Shadow,
    Enforced,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum LegalPurchasePurpose {
    History,
    NewWork,
    SettleExisting,
}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct LegalPurchaseRequirement {
    pub capability: String,
    pub purpose: LegalPurchasePurpose,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalRecordRequest {
    pub item_id: RecordId,
    pub document_id: Option<RecordId>,
    pub operation: LegalRecordOperation,
    pub mode: LegalCommercialMode,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalRecordSummary {
    pub id: RecordId,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub kind: String,
    pub classification: String,
    pub created_by: UserId,
    pub state: String,
    pub version: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalRecordDocument {
    pub id: RecordId,
    pub item_id: RecordId,
    pub version: i64,
    pub checksum_sha256: String,
    pub media_type: String,
    pub bytes: i64,
    pub attachment_link_id: Option<RecordId>,
    pub attachment_source_id: Option<RecordId>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalReleaseObservation {
    pub allowed: bool,
    pub revision: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalPurchaseObservation {
    pub allowed: bool,
    pub revision: String,
}
pub struct LegalRecordFacts {
    pub item: LegalRecordSummary,
    pub member: bool,
    pub editor: bool,
    pub enabled: bool,
    pub ai_enabled: bool,
    /// Holds preserve content; they do not forbid an otherwise authorized read.
    pub held: bool,
    pub disposed: bool,
    pub destruction_executing: bool,
    pub destruction_completed: bool,
    pub document: Option<LegalRecordDocument>,
}
pub fn legal_kind_capability(kind: &str) -> Result<&'static str, BusinessError> {
    match kind {
        "contract" | "template" => Ok("legal.contracts"),
        "request" | "matter" | "counsel" | "ip" => Ok("legal.matters"),
        "obligation" | "compliance" | "license" | "corporate" | "privacy" | "rule" => {
            Ok("legal.obligations")
        }
        _ => Err(BusinessError::new(
            FailureKind::Unavailable,
            "LEGAL_SOURCE_INVALID",
            "The current Legal source is invalid.",
        )),
    }
}
pub(crate) fn policy_error(
    kind: FailureKind,
    code: &'static str,
    message: &'static str,
) -> BusinessError {
    BusinessError::new(kind, code, message)
}
pub(crate) fn not_found() -> BusinessError {
    policy_error(
        FailureKind::NotFound,
        "LEGAL_NOT_FOUND",
        "Legal record is unavailable in your current scope.",
    )
}
pub(crate) fn changed() -> BusinessError {
    policy_error(
        FailureKind::Conflict,
        "LEGAL_SOURCE_CHANGED",
        "The current Legal record policy changed.",
    )
}
