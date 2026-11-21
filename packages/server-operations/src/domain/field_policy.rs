//! Queue policy and opening decisions. Infrastructure and current authority are
//! supplied by the caller's retained owner ports, never inferred from a DTO.
use super::field_exceptions::{text, trimmed};
use erpx_server_shared::public::record_source_contract::SourceKind;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
fn invalid() -> BusinessError {
    BusinessError::invalid(
        "OPERATIONS_INPUT_INVALID",
        "Provide a valid Operations value.",
    )
}
pub fn positive(v: i64) -> Result<(), BusinessError> {
    if !(1..=9_007_199_254_740_991).contains(&v) {
        return Err(invalid());
    }
    Ok(())
}
pub fn identifier(v: &str) -> Result<(), BusinessError> {
    if v.is_empty() || v.chars().count() > 180 {
        return Err(invalid());
    }
    Ok(())
}
pub fn supported_source_kind(kind: SourceKind) -> bool {
    matches!(
        kind,
        SourceKind::CommerceOrder
            | SourceKind::SupportTicket
            | SourceKind::FieldServiceOrder
            | SourceKind::StaffTask
            | SourceKind::PosSale
            | SourceKind::PmsRecord
            | SourceKind::Asset
    )
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueuePolicy {
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
#[derive(Clone, Debug)]
pub struct DraftPolicy {
    pub area: String,
    pub name: String,
    pub target_minutes: i64,
    pub source_kinds: Vec<String>,
}
impl DraftPolicy {
    pub fn validate(&self) -> Result<(), BusinessError> {
        self.validate_native()?;
        if self.source_kinds.iter().any(|v| v != "field_service_order") {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "OPERATIONS_NATIVE_SOURCE_UNSUPPORTED",
                "This policy requires its other native owner capabilities.",
            ));
        }
        Ok(())
    }
    pub fn validate_native(&self) -> Result<(), BusinessError> {
        let area = trimmed(&self.area);
        let bytes = area.as_bytes();
        if bytes.len() < 2
            || bytes.len() > 40
            || !bytes[0].is_ascii_lowercase()
            || !bytes
                .iter()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'_' | b'-'))
        {
            return Err(invalid());
        }
        text(&self.name, 3, 160)?;
        if !(1..=10080).contains(&self.target_minutes)
            || self.source_kinds.is_empty()
            || self.source_kinds.len() > 7
        {
            return Err(invalid());
        }
        if self
            .source_kinds
            .iter()
            .any(|v| !SourceKind::parse(v).is_ok_and(supported_source_kind))
        {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "OPERATIONS_NATIVE_SOURCE_UNSUPPORTED",
                "This policy requires its other native owner capabilities.",
            ));
        }
        Ok(())
    }
    pub fn draft(
        &self,
        id: &str,
        company: &str,
        user: &str,
        revision: i64,
        now: i64,
    ) -> Result<QueuePolicy, BusinessError> {
        self.validate()?;
        self.draft_native(id, company, user, revision, now)
    }
    pub fn draft_native(
        &self,
        id: &str,
        company: &str,
        user: &str,
        revision: i64,
        now: i64,
    ) -> Result<QueuePolicy, BusinessError> {
        self.validate_native()?;
        identifier(id)?;
        positive(revision)?;
        let mut kinds = self.source_kinds.clone();
        kinds.sort();
        kinds.dedup();
        Ok(QueuePolicy {
            id: id.into(),
            company_id: company.into(),
            area: trimmed(&self.area).into(),
            name: trimmed(&self.name).into(),
            revision,
            source_kinds_json: serde_json::to_string(&kinds).map_err(|_| invalid())?,
            target_minutes: self.target_minutes,
            status: "draft".into(),
            created_by: user.into(),
            reviewed_by: None,
            version: 1,
            created_at: now,
            updated_at: now,
        })
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyAction {
    Publish,
    Reject,
    Retire,
}
impl PolicyAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Publish => "publish",
            Self::Reject => "reject",
            Self::Retire => "retire",
        }
    }
    pub fn status(self) -> &'static str {
        match self {
            Self::Publish => "published",
            Self::Reject => "rejected",
            Self::Retire => "retired",
        }
    }
}
pub fn review_policy(
    row: &QueuePolicy,
    action: PolicyAction,
    expected: i64,
    same_creator: bool,
    reason: &str,
) -> Result<i64, BusinessError> {
    positive(expected)?;
    text(reason, 3, 4000)?;
    if row.version != expected
        || row.status
            != if action == PolicyAction::Retire {
                "published"
            } else {
                "draft"
            }
        || action != PolicyAction::Retire && same_creator
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "CONFLICT",
            "Review the current policy and independent review authority.",
        ));
    }
    let v = expected.checked_add(1).ok_or_else(invalid)?;
    positive(v)?;
    Ok(v)
}
#[derive(Clone, Debug)]
pub struct OpenException {
    pub source_id: String,
    pub source_version: i64,
    pub policy_id: String,
    pub policy_version: i64,
    pub title: String,
    pub issue: String,
    pub priority: String,
    pub owner_id: String,
}
impl OpenException {
    pub fn validate(&self) -> Result<(), BusinessError> {
        identifier(&self.source_id)?;
        identifier(&self.policy_id)?;
        identifier(&self.owner_id)?;
        positive(self.source_version)?;
        positive(self.policy_version)?;
        text(&self.title, 3, 160)?;
        text(&self.issue, 3, 4000)?;
        if !matches!(self.priority.as_str(), "normal" | "high" | "critical") {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn due_at(&self, policy: &QueuePolicy, now: i64) -> Result<i64, BusinessError> {
        self.due_at_for(policy, now, SourceKind::FieldServiceOrder)
    }
    pub fn due_at_for(
        &self,
        policy: &QueuePolicy,
        now: i64,
        source_kind: SourceKind,
    ) -> Result<i64, BusinessError> {
        self.validate()?;
        let kinds: Vec<String> =
            serde_json::from_str(&policy.source_kinds_json).map_err(|_| invalid())?;
        if policy.id != self.policy_id
            || policy.version != self.policy_version
            || policy.status != "published"
            || !kinds.iter().any(|v| v == source_kind.as_str())
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "OPERATIONS_POLICY_UNAVAILABLE",
                "Choose a published policy that permits this source kind.",
            ));
        }
        if !(1..=10080).contains(&policy.target_minutes) {
            return Err(invalid());
        }
        let due = now
            .checked_add(policy.target_minutes * 60000)
            .ok_or_else(invalid)?;
        if due.abs() > 9_007_199_254_740_991 {
            return Err(invalid());
        }
        Ok(due)
    }
}
