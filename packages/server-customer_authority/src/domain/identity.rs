//! Portable Customer Authority customer identity-change policy.
//!
//! The proposal is an immutable before/after snapshot. Requesting is a Sales
//! action; apply/reject require an independent eligible Company sales manager
//! and recheck both the change version and the current customer master version.
use crate::domain::access::normalize_reason;
use erpx_server_shared::public::{
    BusinessError, FailureKind, RecordId,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const MAX_NAME: usize = 200;
pub const MAX_TAX_REGISTRATION: usize = 100;
pub const MAX_PHONE: usize = 50;
pub const MAX_EMAIL: usize = 320;
pub const MAX_ADDRESS_LINE: usize = 200;
pub const MAX_CITY: usize = 100;
pub const MAX_REGION: usize = 100;
pub const MAX_POSTAL_CODE: usize = 30;
pub const MAX_EVIDENCE: usize = 2_000;
pub const MAX_WORKSPACE_ROWS: usize = 200;
pub const MAX_HISTORY_ROWS: usize = 500;
pub const MAX_CANDIDATES: usize = 500;
pub const REQUEST_OPERATION: &str = "customer.identity.request";

fn not_found(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::NotFound, code, message)
}
fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}
fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}
pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_IDENTITY_CHANGE_INVALID",
        "Review the customer identity change.",
    )
}
pub fn customer_not_found() -> BusinessError {
    not_found("CUSTOMER_NOT_FOUND", "Active customer not found.")
}
pub fn change_not_found() -> BusinessError {
    not_found(
        "CUSTOMER_IDENTITY_CHANGE_NOT_FOUND",
        "Identity change not found in this Company.",
    )
}
pub fn change_changed() -> BusinessError {
    conflict(
        "CUSTOMER_IDENTITY_CHANGED",
        "The identity change changed. Reload and review the current decision.",
    )
}
pub fn customer_changed() -> BusinessError {
    conflict(
        "CUSTOMER_IDENTITY_SOURCE_CHANGED",
        "The customer master changed. Reload and review the current identity.",
    )
}
pub fn unchanged() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_IDENTITY_UNCHANGED",
        "Propose a different customer identity value.",
    )
}
pub fn independent_approver_required() -> BusinessError {
    forbidden(
        "INDEPENDENT_APPROVER_REQUIRED",
        "Another manager must review this identity change.",
    )
}
pub fn resubmit_forbidden() -> BusinessError {
    forbidden(
        "FORBIDDEN",
        "Only the requester or a sales manager may resubmit.",
    )
}
pub fn reviewer_unavailable() -> BusinessError {
    conflict(
        "CUSTOMER_IDENTITY_REVIEWER_UNAVAILABLE",
        "An eligible independent Company sales manager is required to review this identity change.",
    )
}
pub fn idempotency_conflict() -> BusinessError {
    conflict(
        "CUSTOMER_IDENTITY_IDEMPOTENCY_CONFLICT",
        "This Idempotency-Key was already used for a different customer identity command.",
    )
}
pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CUSTOMER_IDENTITY_READBACK_INVALID",
        "The customer identity change was not persisted as submitted.",
    )
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationConsent {
    #[default]
    Unknown,
    Granted,
    Revoked,
}

impl NotificationConsent {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Granted => "granted",
            Self::Revoked => "revoked",
        }
    }
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "unknown" => Ok(Self::Unknown),
            "granted" => Ok(Self::Granted),
            "revoked" => Ok(Self::Revoked),
            _ => Err(request_invalid()),
        }
    }
    pub fn from_database(value: Option<String>) -> Self {
        value
            .as_deref()
            .and_then(|value| Self::parse(value).ok())
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomerAddress {
    pub line1: String,
    #[serde(default)]
    pub line2: Option<String>,
    pub city: String,
    #[serde(default)]
    pub region: Option<String>,
    pub postal_code: String,
    pub country_code: String,
}

impl CustomerAddress {
    fn validate(mut self) -> Result<Self, BusinessError> {
        self.line1 = bounded(&self.line1, 1, MAX_ADDRESS_LINE)?;
        self.line2 = optional_bounded(self.line2, MAX_ADDRESS_LINE)?;
        self.city = bounded(&self.city, 1, MAX_CITY)?;
        self.region = optional_bounded(self.region, MAX_REGION)?;
        self.postal_code = bounded(&self.postal_code, 1, MAX_POSTAL_CODE)?;
        let country = self.country_code.trim().to_ascii_uppercase();
        if country.len() != 2 || !country.bytes().all(|byte| byte.is_ascii_alphabetic()) {
            return Err(request_invalid());
        }
        self.country_code = country;
        Ok(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomerIdentity {
    pub name: String,
    #[serde(default)]
    pub tax_registration: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub address: Option<CustomerAddress>,
    #[serde(default)]
    pub notification_email: Option<String>,
    #[serde(default)]
    pub notification_consent: NotificationConsent,
}

fn bounded(value: &str, min: usize, max: usize) -> Result<String, BusinessError> {
    let value = value.trim();
    let length = value.chars().count();
    if length < min || length > max || value.chars().any(char::is_control) {
        return Err(request_invalid());
    }
    Ok(value.to_owned())
}

fn optional_bounded(
    value: Option<String>,
    max: usize,
) -> Result<Option<String>, BusinessError> {
    match value {
        None => Ok(None),
        Some(value) => {
            let value = value.trim();
            if value.chars().count() > max || value.chars().any(char::is_control) {
                return Err(request_invalid());
            }
            Ok(Some(value.to_owned()))
        }
    }
}

fn normalize_email(value: Option<String>) -> Result<Option<String>, BusinessError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let Some((local, domain)) = value.split_once('@') else {
        return Err(request_invalid());
    };
    let valid = !local.is_empty()
        && !domain.is_empty()
        && !domain.contains('@')
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && value.len() <= MAX_EMAIL
        && !value.chars().any(char::is_whitespace)
        && !value.chars().any(char::is_control);
    if !valid {
        return Err(request_invalid());
    }
    Ok(Some(value.to_owned()))
}

impl CustomerIdentity {
    pub fn validate(mut self) -> Result<Self, BusinessError> {
        self.name = bounded(&self.name, 1, MAX_NAME)?;
        self.tax_registration = optional_bounded(self.tax_registration, MAX_TAX_REGISTRATION)?;
        self.phone = optional_bounded(self.phone, MAX_PHONE)?;
        self.address = match self.address {
            Some(address) => Some(address.validate()?),
            None => None,
        };
        self.notification_email = normalize_email(self.notification_email)?;
        Ok(self)
    }
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityRequest {
    pub customer_id: String,
    pub expected_customer_version: i64,
    pub proposed: CustomerIdentity,
    pub evidence: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedIdentityRequest {
    pub customer_id: String,
    pub expected_customer_version: i64,
    pub proposed: CustomerIdentity,
    pub evidence: String,
}

impl IdentityRequest {
    pub fn validate(&self) -> Result<ValidatedIdentityRequest, BusinessError> {
        if RecordId::new(self.customer_id.clone()).is_err()
            || self.expected_customer_version < 1
        {
            return Err(request_invalid());
        }
        Ok(ValidatedIdentityRequest {
            customer_id: self.customer_id.clone(),
            expected_customer_version: self.expected_customer_version,
            proposed: self.proposed.clone().validate()?,
            evidence: normalize_reason(&self.evidence).map_err(|_| request_invalid())?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IdentityChangeAction {
    Apply,
    Reject,
    Resubmit,
}

impl IdentityChangeAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Apply => "apply",
            Self::Reject => "reject",
            Self::Resubmit => "resubmit",
        }
    }
    pub fn operation(self) -> String {
        format!("customer.identity.{}", self.as_str())
    }
    pub fn status(self) -> &'static str {
        match self {
            Self::Apply => "applied",
            Self::Reject => "rejected",
            Self::Resubmit => "pending",
        }
    }
    pub fn is_resubmit(self) -> bool {
        matches!(self, Self::Resubmit)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityChange {
    pub action: IdentityChangeAction,
    pub expected_version: i64,
    #[serde(default)]
    pub expected_customer_version: Option<i64>,
    #[serde(default)]
    pub proposed: Option<CustomerIdentity>,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedIdentityChange {
    pub action: IdentityChangeAction,
    pub expected_version: i64,
    pub expected_customer_version: Option<i64>,
    pub proposed: Option<CustomerIdentity>,
    pub reason: String,
}

impl IdentityChange {
    pub fn validate(&self) -> Result<ValidatedIdentityChange, BusinessError> {
        if self.expected_version < 1 {
            return Err(request_invalid());
        }
        let (expected_customer_version, proposed) = if self.action.is_resubmit() {
            let version = self
                .expected_customer_version
                .filter(|value| *value >= 1)
                .ok_or_else(request_invalid)?;
            let proposed = self
                .proposed
                .clone()
                .ok_or_else(request_invalid)?
                .validate()?;
            (Some(version), Some(proposed))
        } else {
            (None, None)
        };
        Ok(ValidatedIdentityChange {
            action: self.action,
            expected_version: self.expected_version,
            expected_customer_version,
            proposed,
            reason: normalize_reason(&self.reason).map_err(|_| request_invalid())?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentityChangeRow {
    pub id: String,
    pub company_id: String,
    pub customer_id: String,
    pub customer_name: String,
    pub customer_code: String,
    pub current_customer_version: i64,
    pub expected_customer_version: i64,
    pub previous: Value,
    pub proposed: Value,
    pub status: String,
    pub evidence: String,
    pub created_by: String,
    pub reviewed_by: Option<String>,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub actor_eligible: bool,
}

impl IdentityChangeRow {
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        if self.id.is_empty()
            || self.company_id.is_empty()
            || self.customer_id.is_empty()
            || self.customer_name.is_empty()
            || self.current_customer_version < 1
            || self.expected_customer_version < 1
            || self.version < 1
            || !matches!(self.status.as_str(), "pending" | "rejected" | "applied")
            || !self.previous.is_object()
            || !self.proposed.is_object()
        {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "CUSTOMER_IDENTITY_SOURCE_INVALID",
                "The customer identity source is not available.",
            ));
        }
        Ok(())
    }
    pub fn can_review(&self, actor: &crate::domain::access::AccessActor) -> bool {
        self.status == "pending"
            && self.created_by != actor.user_id.as_str()
            && self.actor_eligible
            && actor.can_approve()
    }
    pub fn can_resubmit(&self, actor: &crate::domain::access::AccessActor) -> bool {
        self.status == "rejected"
            && (self.created_by == actor.user_id.as_str() || actor.can_approve())
    }
    pub fn to_json(&self, actor: &crate::domain::access::AccessActor) -> Value {
        json!({
            "id": self.id,
            "company_id": self.company_id,
            "customer_id": self.customer_id,
            "customer_name": self.customer_name,
            "customer_code": self.customer_code,
            "current_customer_version": self.current_customer_version,
            "expected_customer_version": self.expected_customer_version,
            "previous_json": self.previous,
            "proposed_json": self.proposed,
            "status": self.status,
            "evidence": self.evidence,
            "created_by": self.created_by,
            "reviewed_by": self.reviewed_by,
            "version": self.version,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
            "actor_eligible": self.actor_eligible,
            "canReview": self.can_review(actor),
            "canResubmit": self.can_resubmit(actor),
        })
    }
    pub fn work_item(&self, actor: &crate::domain::access::AccessActor) -> Value {
        let pending = self.status == "pending";
        let can_act = self.can_review(actor);
        json!({
            "id": self.id,
            "type": "customer_identity",
            "title": format!("Identity change · {}", self.customer_name),
            "detail": "Review the before/after identity snapshot before changing the customer master.",
            "href": format!("/customer-identity/{}", self.id),
            "status": self.status,
            "updatedAt": self.updated_at,
            "queue": if pending { if can_act { "todo" } else { "waiting" } } else { "completed" },
            "canAct": can_act,
            "responsibleQueue": "Independent Company sales manager",
            "nextAction": if pending { "Review the identity change" } else { "Read retained identity evidence" },
            "blocker": if pending && !can_act { Some("Awaiting an eligible independent Company sales manager") } else { None },
            "relationships": [if self.created_by == actor.user_id.as_str() { "requested" } else if can_act { "approval" } else { "team" }],
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentityEventRow {
    pub id: String,
    pub change_id: String,
    pub version: i64,
    pub action: String,
    pub actor_id: String,
    pub reason: String,
    pub customer_version: i64,
    pub previous: Value,
    pub proposed: Value,
    pub created_at: i64,
}

impl IdentityEventRow {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "change_id": self.change_id,
            "version": self.version,
            "action": self.action,
            "actor_id": self.actor_id,
            "reason": self.reason,
            "customer_version": self.customer_version,
            "previous_json": self.previous,
            "proposed_json": self.proposed,
            "created_at": self.created_at,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentityCustomer {
    pub id: String,
    pub name: String,
    pub code: String,
    pub tax_registration: Option<String>,
    pub phone: Option<String>,
    pub address: Value,
    pub version: i64,
}

impl IdentityCustomer {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "code": self.code,
            "tax_registration": self.tax_registration,
            "phone": self.phone,
            "address_json": self.address,
            "version": self.version,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomerSnapshot {
    pub identity: CustomerIdentity,
    pub version: i64,
    pub code: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestPlan {
    pub id: String,
    pub company_id: String,
    pub customer_id: String,
    pub expected_customer_version: i64,
    pub previous: CustomerIdentity,
    pub proposed: CustomerIdentity,
    pub evidence: String,
    pub created_by: String,
    pub event_id: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangePlan {
    pub id: String,
    pub company_id: String,
    pub customer_id: String,
    pub action: IdentityChangeAction,
    pub expected_version: i64,
    pub version: i64,
    pub expected_status: String,
    pub status: String,
    pub expected_customer_version: i64,
    pub previous: Value,
    pub proposed: CustomerIdentity,
    pub reviewed_by: Option<String>,
    pub reason: String,
    pub event_id: String,
    pub now: i64,
}

pub fn request_id(key: &str) -> String {
    format!("customer-identity-{key}")
}

pub fn request_identity(
    actor: &crate::domain::access::AccessActor,
    key: &str,
    id: &str,
    input: &ValidatedIdentityRequest,
) -> String {
    json!({
        "operation": REQUEST_OPERATION,
        "recordId": id,
        "actorId": actor.user_id.as_str(),
        "companyId": actor.company_id.as_str(),
        "idempotencyKey": key,
        "input": {
            "customerId": input.customer_id,
            "expectedCustomerVersion": input.expected_customer_version,
            "proposed": input.proposed,
            "evidence": input.evidence,
        },
    })
    .to_string()
}

pub fn request_plan(
    actor: &crate::domain::access::AccessActor,
    key: &str,
    input: &ValidatedIdentityRequest,
    previous: &CustomerIdentity,
    now: i64,
) -> RequestPlan {
    RequestPlan {
        id: request_id(key),
        company_id: actor.company_id.as_str().to_owned(),
        customer_id: input.customer_id.clone(),
        expected_customer_version: input.expected_customer_version,
        previous: previous.clone(),
        proposed: input.proposed.clone(),
        evidence: input.evidence.clone(),
        created_by: actor.user_id.as_str().to_owned(),
        event_id: format!("customer-identity-event-{key}"),
        now,
    }
}

pub fn change_identity(
    actor: &crate::domain::access::AccessActor,
    key: &str,
    id: &str,
    input: &ValidatedIdentityChange,
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
    if input.action.is_resubmit() {
        wire["input"]["expectedCustomerVersion"] = json!(input.expected_customer_version);
        wire["input"]["proposed"] = json!(input.proposed);
    }
    wire.to_string()
}

pub fn change_plan(
    actor: &crate::domain::access::AccessActor,
    row: &IdentityChangeRow,
    key: &str,
    input: &ValidatedIdentityChange,
    now: i64,
) -> Result<ChangePlan, BusinessError> {
    if input.expected_version != row.version {
        return Err(change_changed());
    }
    if input.action.is_resubmit() {
        if row.status != "rejected" {
            return Err(change_changed());
        }
    } else if row.status != "pending" {
        return Err(change_changed());
    }
    let proposed = if input.action.is_resubmit() {
        input
            .proposed
            .clone()
            .ok_or_else(request_invalid)?
    } else {
        serde_json::from_value(row.proposed.clone()).map_err(|_| request_invalid())?
    };
    let expected_customer_version = if input.action.is_resubmit() {
        input
            .expected_customer_version
            .ok_or_else(request_invalid)?
    } else {
        row.expected_customer_version
    };
    Ok(ChangePlan {
        id: row.id.clone(),
        company_id: row.company_id.clone(),
        customer_id: row.customer_id.clone(),
        action: input.action,
        expected_version: row.version,
        version: row.version + 1,
        expected_status: if input.action.is_resubmit() {
            "rejected".to_owned()
        } else {
            "pending".to_owned()
        },
        status: input.action.status().to_owned(),
        expected_customer_version,
        previous: row.previous.clone(),
        proposed,
        reviewed_by: if input.action.is_resubmit() {
            None
        } else {
            Some(actor.user_id.as_str().to_owned())
        },
        reason: input.reason.clone(),
        event_id: format!("customer-identity-event-{key}"),
        now,
    })
}
