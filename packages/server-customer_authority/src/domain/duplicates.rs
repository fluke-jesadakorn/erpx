//! Portable Customer Authority duplicate-review request policy.
//!
//! This native slice covers the initial duplicate review request and its bounded
//! workspace/detail read. Decision, reopen and merge remain in the retained
//! TypeScript owner until their native slices are implemented.
use crate::domain::access::{AccessActor, normalize_reason};
use erpx_server_shared::public::{
    BusinessError, FailureKind, RecordId,
};
use serde_json::{Value, json};

pub const MAX_EVIDENCE: usize = 2_000;
pub const MAX_WORKSPACE_ROWS: usize = 200;
pub const MAX_HISTORY_ROWS: usize = 500;
pub const REQUEST_OPERATION: &str = "customer.duplicate.request";

fn not_found(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::NotFound, code, message)
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

#[derive(Clone, Debug, Deserialize, Serialize)]
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DuplicateReviewRow {
    pub id: String,
    pub company_id: String,
    pub left_customer_id: String,
    pub right_customer_id: String,
    pub left_name: String,
    pub right_name: String,
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
            || !matches!(self.status.as_str(), "pending" | "distinct" | "duplicate")
        {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "CUSTOMER_DUPLICATE_SOURCE_INVALID",
                "The customer duplicate review source is not available.",
            ));
        }
        Ok(())
    }
    pub fn to_json(&self, actor: &AccessActor, now: i64) -> Value {
        let _ = (actor, now);
        json!({
            "id": self.id,
            "company_id": self.company_id,
            "left_customer_id": self.left_customer_id,
            "right_customer_id": self.right_customer_id,
            "left_name": self.left_name,
            "right_name": self.right_name,
            "status": self.status,
            "canonical_customer_id": self.canonical_customer_id,
            "evidence": self.evidence,
            "created_by": self.created_by,
            "reviewed_by": self.reviewed_by,
            "version": self.version,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
            "actor_eligible": self.actor_eligible,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DuplicateReviewEventRow {
    pub version: i64,
    pub action: String,
    pub actor_id: String,
    pub reason: String,
    pub canonical_customer_id: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DuplicateReviewCustomer {
    pub id: String,
    pub name: String,
    pub code: String,
}

impl DuplicateReviewCustomer {
    pub fn to_json(&self) -> Value {
        json!({"id": self.id, "name": self.name, "code": self.code})
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

// serde imports needed
use serde::{Deserialize, Serialize};
