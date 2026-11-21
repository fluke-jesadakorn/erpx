//! Operations queue read facts. These projections are presentation facts only;
//! they are not a current authorization proof, source purchase or command
//! capability. Cross-owner source visibility and people labels arrive through
//! the public owner ports supplied by composition.
use crate::domain::field_exceptions::{Exception, ExceptionSummary, summarize};
use erpx_server_shared::public::{BusinessError, FailureKind};

pub use crate::domain::field_exceptions::{
    Exception as QueueException, summarize as summarize_queue,
};

pub const MAX_EXCEPTIONS: usize = 200;
pub const MAX_POLICIES: usize = 500;
pub const MAX_PEOPLE: usize = 1_000;
pub const MAX_HISTORY: usize = 500;

#[derive(Clone, Debug)]
pub struct OperationsReadActor {
    pub user_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    /// Epoch milliseconds at which the selected work context expires.
    pub expires_at: i64,
}
impl OperationsReadActor {
    pub fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }
    pub fn can_write(&self) -> bool {
        self.has("records.write")
    }
    pub fn can_review(&self) -> bool {
        self.has("records.review")
    }
    pub fn require_read(&self, now: i64) -> Result<(), BusinessError> {
        if !self.has("records.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission.",
            ));
        }
        if !self.modules.iter().any(|module| module == "operations") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "MODULE_FORBIDDEN",
                "Operations is unavailable to the selected role.",
            ));
        }
        if now >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        Ok(())
    }
}

/// Current owner-port projection of an authorized non-Field source record.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceView {
    pub kind: String,
    pub id: String,
    pub version: i64,
    pub title: String,
    pub href: String,
    pub label: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub status: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct QueuePolicyRow {
    pub id: String,
    pub company_id: String,
    pub area: String,
    pub name: String,
    pub revision: i64,
    pub source_kinds_json: String,
    pub target_minutes: i64,
    pub status: String,
    pub created_by: String,
    pub reviewed_by: Option<String>,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct OperationsEventRow {
    pub id: String,
    pub exception_id: String,
    pub version: i64,
    pub action: String,
    pub actor_id: String,
    pub reason: String,
    pub status: String,
    pub details_json: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct PersonOption {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ExceptionView {
    #[serde(flatten)]
    pub row: Exception,
    pub source: SourceView,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationsWorkspace {
    pub exceptions: Vec<ExceptionView>,
    pub policies: Vec<QueuePolicyRow>,
    pub people: Vec<PersonOption>,
    pub actor_id: String,
    pub can_write: bool,
    pub can_review: bool,
    pub summary: ExceptionSummary,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationsDetail {
    #[serde(flatten)]
    pub workspace: OperationsWorkspace,
    pub row: ExceptionView,
    pub history: Vec<OperationsEventRow>,
}

impl OperationsWorkspace {
    pub fn value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

impl OperationsDetail {
    pub fn value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct WorkspaceData {
    pub exceptions: Vec<Exception>,
    pub policies: Vec<QueuePolicyRow>,
}

pub fn validate_source(
    row: &Exception,
    source: &SourceView,
    actor: &OperationsReadActor,
) -> Result<(), BusinessError> {
    if source.kind != row.source_kind
        || source.id != row.source_id
        || source.company_id != actor.company_id
        || source.company_id != row.company_id
        || source.version < 1
        || source.title.chars().count() > 400
        || source.status.chars().count() > 200
        || source.href.len() > 2_048
        || source.label.chars().count() > 600
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "OPERATIONS_SOURCE_INVALID",
            "The source record no longer matches this Operations exception.",
        ));
    }
    Ok(())
}

pub fn summary(rows: &[ExceptionView], now: i64) -> ExceptionSummary {
    let rows: Vec<Exception> = rows.iter().map(|view| view.row.clone()).collect();
    summarize(&rows, now)
}

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "OPERATIONS_EXCEPTION_NOT_FOUND",
        "Exception or source is unavailable in this scope.",
    )
}

pub fn read_limit(message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, "OPERATIONS_READ_LIMIT", message)
}
