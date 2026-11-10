//! Portable Customer Authority duplicate-review policy.
//!
//! This native slice owns the initial duplicate review request, the independent
//! change/decision/reopen/merge commands and the bounded workspace/detail read.
//! The retained TypeScript owner keeps only the Sales transport and its page
//! projection until the lead binds the native HTTP router.
use crate::domain::access::{AccessActor, normalize_reason};
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const MAX_EVIDENCE: usize = 2_000;
pub const MAX_WORKSPACE_ROWS: usize = 200;
pub const MAX_HISTORY_ROWS: usize = 500;
pub const REQUEST_OPERATION: &str = "customer.duplicate.request";

fn not_found(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::NotFound, code, message)
}
fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}
fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}
pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_DUPLICATE_REQUEST_INVALID",
        "Review the customer duplicate request.",
    )
}
pub fn change_invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_DUPLICATE_CHANGE_INVALID",
        "Review the customer duplicate decision.",
    )
}
pub fn source_not_found() -> BusinessError {
    not_found(
        "CUSTOMER_DUPLICATE_SOURCE_NOT_FOUND",
        "The customer record is not available in this scope.",
    )
}
pub fn review_not_found() -> BusinessError {
    not_found(
        "CUSTOMER_DUPLICATE_NOT_FOUND",
        "Duplicate review not found in this Company.",
    )
}
pub fn review_changed() -> BusinessError {
    conflict(
        "CUSTOMER_DUPLICATE_CHANGED",
        "The duplicate review changed. Reload and review the current decision.",
    )
}
pub fn idempotency_conflict() -> BusinessError {
    conflict(
        "CUSTOMER_DUPLICATE_IDEMPOTENCY_CONFLICT",
        "This Idempotency-Key was already used for a different customer duplicate command.",
    )
}
pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CUSTOMER_DUPLICATE_READBACK_INVALID",
        "The duplicate review decision was not persisted as submitted.",
    )
}
pub fn independent_approver_required() -> BusinessError {
    forbidden(
        "INDEPENDENT_APPROVER_REQUIRED",
        "Another manager must decide whether these customers are duplicates.",
    )
}
pub fn reviewer_unavailable() -> BusinessError {
    conflict(
        "CUSTOMER_DUPLICATE_REVIEWER_UNAVAILABLE",
        "An eligible independent Company sales manager is required to review this duplicate.",
    )
}
pub fn merge_requires_duplicate() -> BusinessError {
    conflict(
        "CUSTOMER_MERGE_REQUIRES_DUPLICATE",
        "Confirm the duplicate decision before merging customer records.",
    )
}
pub fn merge_blocked(message: &'static str) -> BusinessError {
    conflict("CUSTOMER_MERGE_BLOCKED", message)
}
pub fn merge_source_changed() -> BusinessError {
    conflict(
        "CUSTOMER_MERGE_SOURCE_CHANGED",
        "Both active customer records must remain in the same company and currency before merge.",
    )
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DuplicateReviewRequest {
    pub left_customer_id: String,
    pub right_customer_id: String,
    pub evidence: String,
}

impl DuplicateReviewRequest {
    pub fn validate(&self) -> Result<ValidatedDuplicateReviewRequest, BusinessError> {
        if RecordId::new(self.left_customer_id.clone()).is_err()
            || RecordId::new(self.right_customer_id.clone()).is_err()
            || self.left_customer_id == self.right_customer_id
        {
            return Err(request_invalid());
        }
        Ok(ValidatedDuplicateReviewRequest {
            left_customer_id: self.left_customer_id.clone(),
            right_customer_id: self.right_customer_id.clone(),
            evidence: normalize_reason(&self.evidence)?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedDuplicateReviewRequest {
    pub left_customer_id: String,
    pub right_customer_id: String,
    pub evidence: String,
}

impl ValidatedDuplicateReviewRequest {
    pub fn sorted_pair(&self) -> (String, String) {
        if self.left_customer_id <= self.right_customer_id {
            (self.left_customer_id.clone(), self.right_customer_id.clone())
        } else {
            (self.right_customer_id.clone(), self.left_customer_id.clone())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DuplicateAction {
    Distinct,
    Duplicate,
    Reopen,
    Merge,
}

impl DuplicateAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Distinct => "distinct",
            Self::Duplicate => "duplicate",
            Self::Reopen => "reopen",
            Self::Merge => "merge",
        }
    }
    pub fn operation(self) -> String {
        format!("customer.duplicate.{}", self.as_str())
    }
    pub fn status(self) -> &'static str {
        match self {
            Self::Distinct => "distinct",
            Self::Duplicate => "duplicate",
            Self::Reopen => "pending",
            Self::Merge => "merged",
        }
    }
    pub fn is_reopen(self) -> bool {
        matches!(self, Self::Reopen)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DuplicateChange {
    pub action: DuplicateAction,
    pub expected_version: i64,
    #[serde(default)]
    pub canonical_customer_id: Option<String>,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedDuplicateChange {
    pub action: DuplicateAction,
    pub expected_version: i64,
    pub canonical_customer_id: Option<String>,
    pub reason: String,
}

impl DuplicateChange {
    pub fn validate(&self) -> Result<ValidatedDuplicateChange, BusinessError> {
        if self.expected_version < 1 {
            return Err(change_invalid());
        }
        let canonical = match self.action {
            DuplicateAction::Duplicate => {
                let canonical = self
                    .canonical_customer_id
                    .clone()
                    .filter(|value| RecordId::new(value.clone()).is_ok())
                    .ok_or_else(change_invalid)?;
                Some(canonical)
            }
            _ => None,
        };
        Ok(ValidatedDuplicateChange {
            action: self.action,
            expected_version: self.expected_version,
            canonical_customer_id: canonical,
            reason: normalize_reason(&self.reason).map_err(|_| change_invalid())?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DuplicateReviewRow {
    pub id: String,
    pub company_id: String,
    pub left_customer_id: String,
    pub right_customer_id: String,
    pub left_name: String,
    pub right_name: String,
    pub left_code: String,
    pub right_code: String,
    pub left_tax_registration: Option<String>,
    pub right_tax_registration: Option<String>,
    pub left_status: String,
    pub right_status: String,
    pub left_merged_into_id: Option<String>,
    pub right_merged_into_id: Option<String>,
    pub status: String,
    pub canonical_customer_id: Option<String>,
    pub evidence: String,
    pub created_by: String,
    pub reviewed_by: Option<String>,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub actor_eligible: bool,
}

impl DuplicateReviewRow {
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        if self.id.is_empty()
            || self.company_id.is_empty()
            || self.left_customer_id.is_empty()
            || self.right_customer_id.is_empty()
            || self.left_name.is_empty()
            || self.right_name.is_empty()
            || self.left_customer_id == self.right_customer_id
            || self.version < 1
            || !matches!(
                self.status.as_str(),
                "pending" | "distinct" | "duplicate" | "merged"
            )
            || !matches!(self.left_status.as_str(), "active" | "blocked")
            || !matches!(self.right_status.as_str(), "active" | "blocked")
        {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "CUSTOMER_DUPLICATE_SOURCE_INVALID",
                "The customer duplicate review source is not available.",
            ));
        }
        Ok(())
    }
    pub fn pair_active(&self) -> bool {
        self.left_status == "active"
            && self.right_status == "active"
            && self.left_merged_into_id.is_none()
            && self.right_merged_into_id.is_none()
    }
    pub fn can_review(&self, actor: &AccessActor) -> bool {
        self.status == "pending"
            && self.created_by != actor.user_id.as_str()
            && self.actor_eligible
            && actor.can_approve()
    }
    pub fn can_reopen(&self, actor: &AccessActor) -> bool {
        self.status != "pending"
            && self.status != "merged"
            && self.pair_active()
            && self.actor_eligible
            && actor.can_approve()
    }
    pub fn can_merge(&self, actor: &AccessActor) -> bool {
        self.status == "duplicate"
            && self.pair_active()
            && self.created_by != actor.user_id.as_str()
            && self.actor_eligible
            && actor.can_approve()
    }
    pub fn to_json(&self, actor: &AccessActor, now: i64) -> Value {
        let _ = now;
        json!({
            "id": self.id,
            "company_id": self.company_id,
            "left_customer_id": self.left_customer_id,
            "right_customer_id": self.right_customer_id,
            "left_name": self.left_name,
            "right_name": self.right_name,
            "left_code": self.left_code,
            "right_code": self.right_code,
            "left_tax_registration": self.left_tax_registration,
            "right_tax_registration": self.right_tax_registration,
            "left_status": self.left_status,
            "right_status": self.right_status,
            "left_merged_into_id": self.left_merged_into_id,
            "right_merged_into_id": self.right_merged_into_id,
            "status": self.status,
            "canonical_customer_id": self.canonical_customer_id,
            "evidence": self.evidence,
            "created_by": self.created_by,
            "reviewed_by": self.reviewed_by,
            "version": self.version,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
            "actor_eligible": self.actor_eligible,
            "canReview": self.can_review(actor),
            "canReopen": self.can_reopen(actor),
            "canMerge": self.can_merge(actor),
        })
    }
    pub fn work_item(&self, actor: &AccessActor, now: i64) -> Value {
        let _ = now;
        let can_act = self.can_review(actor) || self.can_merge(actor);
        let pending = self.status == "pending";
        json!({
            "id": self.id,
            "type": "customer_duplicate",
            "actionKind": if self.can_review(actor) { Some("approval") } else { Some("work") },
            "title": format!("Duplicate review · {} / {}", self.left_name, self.right_name),
            "detail": "Compare the customer records and record an explainable decision.",
            "href": format!("/customer-duplicates/{}", self.id),
            "status": self.status,
            "updatedAt": self.updated_at,
            "queue": if can_act { "todo" } else if pending { "waiting" } else { "completed" },
            "canAct": can_act,
            "responsibleQueue": "Independent Company sales manager",
            "nextAction": if pending { "Decide duplicate or distinct" } else if self.can_merge(actor) { "Merge eligible source records into the canonical customer" } else { "Read retained identity decision" },
            "blocker": if pending && !self.can_review(actor) { Some("Awaiting an eligible independent Company sales manager") } else { None },
            "relationships": [if self.created_by == actor.user_id.as_str() { "requested" } else if can_act { "approval" } else { "team" }],
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DuplicateReviewEventRow {
    pub id: String,
    pub version: i64,
    pub action: String,
    pub actor_id: String,
    pub reason: String,
    pub canonical_customer_id: Option<String>,
    pub created_at: i64,
}

impl DuplicateReviewEventRow {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "version": self.version,
            "action": self.action,
            "actor_id": self.actor_id,
            "reason": self.reason,
            "canonical_customer_id": self.canonical_customer_id,
            "created_at": self.created_at,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DuplicateReviewCustomer {
    pub id: String,
    pub name: String,
    pub code: String,
    pub tax_registration: Option<String>,
    pub version: i64,
}

impl DuplicateReviewCustomer {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "code": self.code,
            "tax_registration": self.tax_registration,
            "version": self.version,
        })
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MergeBlockers {
    pub notifications: bool,
    pub grants: bool,
    pub agreements: bool,
    pub billing: bool,
    pub order_evidence: bool,
}

impl MergeBlockers {
    pub fn message(&self) -> Option<&'static str> {
        if self.notifications {
            Some("Customer notifications retain immutable recipient snapshots.")
        } else if self.grants {
            Some("Customer access bindings retain immutable source scope.")
        } else if self.agreements {
            Some("Field Service agreements retain immutable customer scope.")
        } else if self.billing {
            Some("Field Service billing releases retain immutable customer scope.")
        } else if self.order_evidence {
            Some("Customer orders with source evidence must be corrected by their owner workflow first.")
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MergePartyFacts {
    pub canonical_status: String,
    pub merged_status: String,
    pub canonical_merged_into_id: Option<String>,
    pub merged_merged_into_id: Option<String>,
    pub canonical_currency: String,
    pub merged_currency: String,
}

impl MergePartyFacts {
    pub fn eligible(&self) -> bool {
        self.canonical_status == "active"
            && self.merged_status == "active"
            && self.canonical_merged_into_id.is_none()
            && self.merged_merged_into_id.is_none()
            && self.canonical_currency == self.merged_currency
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestPlan {
    pub id: String,
    pub company_id: String,
    pub left_customer_id: String,
    pub right_customer_id: String,
    pub evidence: String,
    pub created_by: String,
    pub event_id: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangePlan {
    pub id: String,
    pub company_id: String,
    pub action: DuplicateAction,
    pub expected_version: i64,
    pub version: i64,
    pub expected_status: String,
    pub status: String,
    pub canonical_customer_id: Option<String>,
    pub reviewed_by: Option<String>,
    pub reason: String,
    pub event_id: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MergePlan {
    pub id: String,
    pub company_id: String,
    pub expected_version: i64,
    pub version: i64,
    pub canonical_customer_id: String,
    pub merged_customer_id: String,
    pub reviewed_by: String,
    pub reason: String,
    pub event_id: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateReceipt {
    pub id: String,
    pub status: String,
    pub version: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_customer_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_customer_id: Option<String>,
}

impl DuplicateReceipt {
    pub fn new(id: String, status: &str, version: i64) -> Self {
        Self {
            id,
            status: status.to_owned(),
            version,
            canonical_customer_id: None,
            merged_customer_id: None,
        }
    }
    pub fn merged(
        id: String,
        version: i64,
        canonical_customer_id: String,
        merged_customer_id: String,
    ) -> Self {
        Self {
            id,
            status: "merged".to_owned(),
            version,
            canonical_customer_id: Some(canonical_customer_id),
            merged_customer_id: Some(merged_customer_id),
        }
    }
    pub fn to_wire(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
    pub fn valid(&self) -> bool {
        !self.id.is_empty()
            && self.version >= 1
            && matches!(
                self.status.as_str(),
                "pending" | "distinct" | "duplicate" | "merged"
            )
    }
}

pub fn request_id(key: &str) -> String {
    format!("customer-duplicate-{key}")
}

pub fn request_identity(
    actor: &AccessActor,
    key: &str,
    id: &str,
    input: &ValidatedDuplicateReviewRequest,
) -> String {
    let (left, right) = input.sorted_pair();
    json!({
        "operation": REQUEST_OPERATION,
        "recordId": id,
        "actorId": actor.user_id.as_str(),
        "companyId": actor.company_id.as_str(),
        "idempotencyKey": key,
        "input": {
            "leftCustomerId": left,
            "rightCustomerId": right,
            "evidence": input.evidence,
        },
    })
    .to_string()
}

pub fn request_plan(
    actor: &AccessActor,
    key: &str,
    input: &ValidatedDuplicateReviewRequest,
    now: i64,
) -> RequestPlan {
    let (left, right) = input.sorted_pair();
    RequestPlan {
        id: request_id(key),
        company_id: actor.company_id.as_str().to_owned(),
        left_customer_id: left,
        right_customer_id: right,
        evidence: input.evidence.clone(),
        created_by: actor.user_id.as_str().to_owned(),
        event_id: format!("customer-duplicate-event-{key}"),
        now,
    }
}

pub fn change_identity(
    actor: &AccessActor,
    key: &str,
    id: &str,
    input: &ValidatedDuplicateChange,
) -> String {
    let mut wire = json!({
        "operation": input.action.operation(),
        "recordId": id,
        "actorId": actor.user_id.as_str(),
        "companyId": actor.company_id.as_str(),
        "idempotencyKey": key,
        "input": {
            "action": input.action.as_str(),
            "expectedVersion": input.expected_version,
            "reason": input.reason,
        },
    });
    if input.action == DuplicateAction::Duplicate {
        wire["input"]["canonicalCustomerId"] = json!(input.canonical_customer_id);
    }
    wire.to_string()
}

pub fn change_plan(
    actor: &AccessActor,
    row: &DuplicateReviewRow,
    key: &str,
    input: &ValidatedDuplicateChange,
    now: i64,
) -> Result<ChangePlan, BusinessError> {
    if input.expected_version != row.version {
        return Err(review_changed());
    }
    let expected_status = if input.action.is_reopen() {
        row.status.clone()
    } else {
        "pending".to_owned()
    };
    if !input.action.is_reopen() && row.status != "pending" {
        return Err(review_changed());
    }
    if input.action == DuplicateAction::Duplicate {
        let Some(canonical) = input.canonical_customer_id.as_deref() else {
            return Err(change_invalid());
        };
        if canonical != row.left_customer_id && canonical != row.right_customer_id {
            return Err(review_changed());
        }
    }
    Ok(ChangePlan {
        id: row.id.clone(),
        company_id: row.company_id.clone(),
        action: input.action,
        expected_version: row.version,
        version: row.version + 1,
        expected_status,
        status: input.action.status().to_owned(),
        canonical_customer_id: input.canonical_customer_id.clone(),
        reviewed_by: if input.action.is_reopen() {
            None
        } else {
            Some(actor.user_id.as_str().to_owned())
        },
        reason: input.reason.clone(),
        event_id: format!("customer-duplicate-event-{key}"),
        now,
    })
}

pub fn merge_plan(
    actor: &AccessActor,
    row: &DuplicateReviewRow,
    key: &str,
    input: &ValidatedDuplicateChange,
    now: i64,
) -> Result<MergePlan, BusinessError> {
    if input.expected_version != row.version {
        return Err(review_changed());
    }
    if row.status != "duplicate" {
        return Err(merge_requires_duplicate());
    }
    let Some(canonical) = row.canonical_customer_id.clone() else {
        return Err(merge_requires_duplicate());
    };
    let merged = if canonical == row.left_customer_id {
        row.right_customer_id.clone()
    } else if canonical == row.right_customer_id {
        row.left_customer_id.clone()
    } else {
        return Err(merge_source_changed());
    };
    Ok(MergePlan {
        id: row.id.clone(),
        company_id: row.company_id.clone(),
        expected_version: row.version,
        version: row.version + 1,
        canonical_customer_id: canonical,
        merged_customer_id: merged,
        reviewed_by: actor.user_id.as_str().to_owned(),
        reason: input.reason.clone(),
        event_id: format!("customer-duplicate-event-{key}"),
        now,
    })
}
