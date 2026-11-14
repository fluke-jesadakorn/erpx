//! Portable Customer Authority customer-access grant policy.
//!
//! The access binding is explicitly source-bound to one approved Sales
//! quotation/order. Nothing here infers access from following, mentions or an
//! unverified email. No database, HTTP, clock or notification effect is
//! reachable from this module.
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const MAX_REASON: usize = 2_000;
pub const MAX_EMAIL: usize = 254;
pub const ACCESS_WINDOW_MILLIS: i64 = 90 * 24 * 60 * 60 * 1_000;
pub const MAX_WORKSPACE_ROWS: usize = 200;
pub const MAX_SOURCE_ROWS: usize = 250;
pub const MAX_EVENT_ROWS: usize = 500;
pub const MAX_READ_ROWS: usize = 100;
pub const REQUEST_OPERATION: &str = "customer.access.request";

fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}
fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}
fn not_found(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::NotFound, code, message)
}
pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_ACCESS_REQUEST_INVALID",
        "Review the customer access request.",
    )
}
pub fn source_not_found() -> BusinessError {
    not_found(
        "CUSTOMER_ACCESS_SOURCE_NOT_FOUND",
        "The approved customer document is not available in this scope.",
    )
}
pub fn source_changed() -> BusinessError {
    conflict(
        "CUSTOMER_ACCESS_SOURCE_CHANGED",
        "The customer document changed. Reload and review the current source.",
    )
}
pub fn binding_not_found() -> BusinessError {
    not_found(
        "CUSTOMER_ACCESS_NOT_FOUND",
        "Customer access request not found in this scope.",
    )
}
pub fn binding_changed() -> BusinessError {
    conflict(
        "CUSTOMER_ACCESS_CHANGED",
        "The customer access request changed. Reload and review the current decision.",
    )
}
pub fn idempotency_conflict() -> BusinessError {
    conflict(
        "CUSTOMER_ACCESS_IDEMPOTENCY_CONFLICT",
        "This Idempotency-Key was already used for a different customer access command.",
    )
}
pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CUSTOMER_ACCESS_READBACK_INVALID",
        "The customer access decision was not persisted as submitted.",
    )
}
pub fn reviewer_unavailable() -> BusinessError {
    conflict(
        "CUSTOMER_ACCESS_REVIEWER_UNAVAILABLE",
        "An eligible independent sales manager is required to review this access request.",
    )
}

#[derive(Clone, Debug)]
pub struct AccessActor {
    pub user_id: UserId,
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub assignment_id: AssignmentId,
    pub role: Option<String>,
    pub authority_revision: Option<i64>,
    pub assignment_version: Option<i64>,
    pub permissions: Vec<String>,
    pub expires_at: i64,
}

impl AccessActor {
    pub fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn require_context(&self, now: i64) -> Result<(), BusinessError> {
        if now >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        Ok(())
    }

    pub fn require_read(&self, now: i64) -> Result<(), BusinessError> {
        self.require_context(now)?;
        if !self.has("commerce.read") {
            return Err(forbidden(
                "CUSTOMER_ACCESS_PERMISSION_REQUIRED",
                "Your selected role does not grant customer access read authority.",
            ));
        }
        Ok(())
    }

    pub fn require_sales(&self, now: i64) -> Result<(), BusinessError> {
        self.require_context(now)?;
        if !self.has("commerce.sales") {
            return Err(forbidden(
                "CUSTOMER_ACCESS_PERMISSION_REQUIRED",
                "Your selected role does not grant customer access sales authority.",
            ));
        }
        Ok(())
    }

    pub fn require_approve(&self, now: i64) -> Result<(), BusinessError> {
        self.require_context(now)?;
        if !self.has("commerce.approve") {
            return Err(forbidden(
                "CUSTOMER_ACCESS_APPROVAL_REQUIRED",
                "Your selected role does not grant independent customer access approval.",
            ));
        }
        Ok(())
    }

    pub fn can_approve(&self) -> bool {
        self.has("commerce.approve")
    }

    pub fn in_scope(
        &self,
        company_id: &str,
        branch_id: Option<&str>,
        department_id: Option<&str>,
    ) -> bool {
        company_id == self.company_id.as_str()
            && self
                .branch_id
                .as_ref()
                .is_none_or(|value| branch_id == Some(value.as_str()))
            && self
                .department_id
                .as_ref()
                .is_none_or(|value| department_id == Some(value.as_str()))
    }

    pub fn authority_json(&self) -> String {
        json!({
            "assignmentId": self.assignment_id.as_str(),
            "companyId": self.company_id.as_str(),
            "branchId": self.branch_id.as_ref().map(RecordId::as_str),
            "departmentId": self.department_id.as_ref().map(RecordId::as_str),
            "role": self.role,
            "permissions": self.permissions,
            "assignmentVersion": self.assignment_version,
            "revision": self.authority_revision,
        })
        .to_string()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessAction {
    Approve,
    Reject,
    Revoke,
}

impl AccessAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Reject => "reject",
            Self::Revoke => "revoke",
        }
    }
    pub fn operation(self) -> String {
        format!("customer.access.{}", self.as_str())
    }
    pub fn is_review(self) -> bool {
        !matches!(self, Self::Revoke)
    }
    pub fn status(self) -> &'static str {
        match self {
            Self::Approve => "approved",
            Self::Reject => "rejected",
            Self::Revoke => "revoked",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccessRequest {
    pub order_id: String,
    pub source_version: i64,
    pub email: String,
    pub expires_at: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccessChange {
    pub action: AccessAction,
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedRequest {
    pub order_id: String,
    pub source_version: i64,
    pub email: String,
    pub expires_at: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedChange {
    pub action: AccessAction,
    pub expected_version: i64,
    pub reason: String,
}

fn valid_text(value: &str, max: usize) -> bool {
    value.len() <= max && !value.chars().any(char::is_control)
}

pub fn normalize_reason(value: &str) -> Result<String, BusinessError> {
    let reason = value.trim();
    let length = reason.chars().count();
    if !(3..=MAX_REASON).contains(&length) || !valid_text(reason, MAX_REASON) {
        return Err(request_invalid());
    }
    Ok(reason.to_owned())
}

pub fn normalize_email(value: &str) -> Result<String, BusinessError> {
    let email = value.trim().to_ascii_lowercase();
    let Some((local, domain)) = email.split_once('@') else {
        return Err(request_invalid());
    };
    let valid = !local.is_empty()
        && !domain.is_empty()
        && !domain.contains('@')
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && email.len() <= MAX_EMAIL
        && !email.chars().any(char::is_whitespace)
        && !email.chars().any(char::is_control);
    if !valid {
        return Err(request_invalid());
    }
    Ok(email)
}

impl AccessRequest {
    pub fn validate_schema(&self) -> Result<ValidatedRequest, BusinessError> {
        if RecordId::new(self.order_id.clone()).is_err()
            || self.source_version < 1 || self.expires_at < 1
        {
            return Err(request_invalid());
        }
        Ok(ValidatedRequest {
            order_id: self.order_id.clone(),
            source_version: self.source_version,
            email: normalize_email(&self.email)?,
            expires_at: self.expires_at,
            reason: normalize_reason(&self.reason)?,
        })
    }

    pub fn validate(&self, now: i64) -> Result<ValidatedRequest, BusinessError> {
        let input = self.validate_schema()?;
        input.validate_expiry(now)?;
        Ok(input)
    }
}

impl ValidatedRequest {
    pub fn validate_expiry(&self, now: i64) -> Result<(), BusinessError> {
        if self.expires_at <= now || self.expires_at > now.saturating_add(ACCESS_WINDOW_MILLIS) {
            return Err(BusinessError::invalid(
                "CUSTOMER_ACCESS_EXPIRY",
                "Choose an expiry within the next 90 days.",
            ));
        }
        Ok(())
    }
}

impl AccessChange {
    pub fn validate(&self) -> Result<ValidatedChange, BusinessError> {
        if self.expected_version < 1 {
            return Err(request_invalid());
        }
        Ok(ValidatedChange {
            action: self.action,
            expected_version: self.expected_version,
            reason: normalize_reason(&self.reason)?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccessOrder {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub customer_id: String,
    pub number: String,
    pub kind: String,
    pub state: String,
    pub version: i64,
}

impl AccessOrder {
    pub fn source_eligible(&self) -> bool {
        matches!(self.kind.as_str(), "sales" | "quote")
            && matches!(
                self.state.as_str(),
                "confirmed" | "converted" | "partially_fulfilled" | "fulfilled"
            )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccessBindingRow {
    pub id: String,
    pub company_id: String,
    pub order_id: String,
    pub customer_id: String,
    pub email: String,
    pub expires_at: i64,
    pub status: String,
    pub reason: String,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub reviewed_at: Option<i64>,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub order_number: String,
    pub order_state: String,
    pub order_kind: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub actor_eligible: bool,
    pub reviewers: i64,
    pub identity_exists: bool,
}

impl AccessBindingRow {
    pub fn can_review(&self, actor: &AccessActor) -> bool {
        self.status == "pending" && self.actor_eligible && actor.can_approve()
    }
    pub fn can_reject(&self, actor: &AccessActor) -> bool {
        self.can_review(actor)
    }
    pub fn can_revoke(&self, actor: &AccessActor) -> bool {
        matches!(self.status.as_str(), "pending" | "approved")
            && (self.created_by == actor.user_id.as_str() || actor.can_approve())
    }
    pub fn can_invite(&self, actor: &AccessActor, now: i64) -> bool {
        self.status == "approved"
            && self.expires_at > now
            && matches!(self.order_kind.as_str(), "sales" | "quote")
            && matches!(
                self.order_state.as_str(),
                "confirmed" | "converted" | "partially_fulfilled" | "fulfilled"
            )
            && !self.identity_exists
            && (self.created_by == actor.user_id.as_str() || actor.can_approve())
    }
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        if self.order_id.is_empty()
            || self.company_id.is_empty()
            || self.customer_id.is_empty()
            || self.order_number.is_empty()
            || self.version < 1
            || self.reviewers < 0
            || !matches!(
                self.status.as_str(),
                "pending" | "approved" | "rejected" | "revoked"
            )
        {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "CUSTOMER_ACCESS_SOURCE_INVALID",
                "The customer access source is not available.",
            ));
        }
        Ok(())
    }
    pub fn to_json(&self, actor: &AccessActor, now: i64) -> Value {
        json!({
            "id": self.id,
            "company_id": self.company_id,
            "order_id": self.order_id,
            "customer_id": self.customer_id,
            "email": self.email,
            "expires_at": self.expires_at,
            "status": self.status,
            "reason": self.reason,
            "created_by": self.created_by,
            "approved_by": self.approved_by,
            "reviewed_at": self.reviewed_at,
            "version": self.version,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
            "number": self.order_number,
            "order_state": self.order_state,
            "branch_id": self.branch_id,
            "department_id": self.department_id,
            "reviewers": self.reviewers,
            "canReview": self.can_review(actor),
            "canReject": self.can_reject(actor),
            "canRevoke": self.can_revoke(actor),
            "canInvite": self.can_invite(actor, now),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccessSourceRow {
    pub id: String,
    pub number: String,
    pub name: String,
    pub version: i64,
}

impl AccessSourceRow {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "number": self.number,
            "name": self.name,
            "version": self.version,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccessEventRow {
    pub version: i64,
    pub action: String,
    pub actor_id: String,
    pub reason: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccessReadRow {
    pub binding_version: i64,
    pub user_id: String,
    pub read_at: i64,
    pub projection_version: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestPlan {
    pub id: String,
    pub order_id: String,
    pub customer_id: String,
    pub email: String,
    pub expires_at: i64,
    pub reason: String,
    pub created_by: String,
    pub event_id: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangePlan {
    pub id: String,
    pub order_id: String,
    pub expected_version: i64,
    pub version: i64,
    pub expected_status: String,
    pub status: String,
    pub approved_by: Option<String>,
    pub reviewed_at: Option<i64>,
    pub reason: String,
    pub action: AccessAction,
    pub event_id: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccessReceipt {
    pub id: String,
    pub status: String,
    pub version: i64,
}

impl AccessReceipt {
    pub fn new(id: String, status: &str, version: i64) -> Self {
        Self {
            id,
            status: status.to_owned(),
            version,
        }
    }
    pub fn to_wire(&self) -> Value {
        json!({"id": self.id, "status": self.status, "version": self.version})
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptSnapshot {
    pub request_hash: String,
    pub result_json: String,
    pub operation: String,
    pub actor_id: String,
    pub record_id: String,
    pub authority_json: String,
}

pub fn request_id(key: &str) -> String {
    format!("customer-access-{key}")
}

pub fn request_identity(
    actor: &AccessActor,
    key: &str,
    id: &str,
    input: &ValidatedRequest,
) -> String {
    json!({
        "operation": REQUEST_OPERATION,
        "recordId": id,
        "actorId": actor.user_id.as_str(),
        "companyId": actor.company_id.as_str(),
        "idempotencyKey": key,
        "input": {
            "orderId": input.order_id,
            "sourceVersion": input.source_version,
            "email": input.email,
            "expiresAt": input.expires_at,
            "reason": input.reason,
        },
    })
    .to_string()
}

pub fn change_identity(
    actor: &AccessActor,
    key: &str,
    id: &str,
    input: &ValidatedChange,
) -> String {
    json!({
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
    })
    .to_string()
}

pub fn request_plan(
    actor: &AccessActor,
    key: &str,
    input: &ValidatedRequest,
    order: &AccessOrder,
    now: i64,
) -> RequestPlan {
    RequestPlan {
        id: request_id(key),
        order_id: order.id.clone(),
        customer_id: order.customer_id.clone(),
        email: input.email.clone(),
        expires_at: input.expires_at,
        reason: input.reason.clone(),
        created_by: actor.user_id.as_str().to_owned(),
        event_id: format!("customer-access-event-{key}"),
        now,
    }
}

fn require_source_scope(actor: &AccessActor, order: &AccessOrder) -> Result<(), BusinessError> {
    if !actor.in_scope(
        &order.company_id,
        order.branch_id.as_deref(),
        order.department_id.as_deref(),
    ) {
        return Err(source_not_found());
    }
    Ok(())
}

pub fn validate_request_source(
    actor: &AccessActor,
    input: &ValidatedRequest,
    order: Option<AccessOrder>,
) -> Result<AccessOrder, BusinessError> {
    let order = order.ok_or_else(source_not_found)?;
    require_source_scope(actor, &order)?;
    if order.id != input.order_id || order.version != input.source_version {
        return Err(source_changed());
    }
    if !order.source_eligible() {
        return Err(source_changed());
    }
    Ok(order)
}

pub fn validate_binding_source(
    actor: &AccessActor,
    binding: AccessBindingRow,
) -> Result<(AccessBindingRow, AccessOrder), BusinessError> {
    binding.validate_source()?;
    if !actor.in_scope(
        &binding.company_id,
        binding.branch_id.as_deref(),
        binding.department_id.as_deref(),
    ) {
        return Err(binding_not_found());
    }
    let order = AccessOrder {
        id: binding.order_id.clone(),
        company_id: binding.company_id.clone(),
        branch_id: binding.branch_id.clone(),
        department_id: binding.department_id.clone(),
        customer_id: binding.customer_id.clone(),
        number: binding.order_number.clone(),
        kind: binding.order_kind.clone(),
        state: binding.order_state.clone(),
        version: 0,
    };
    Ok((binding, order))
}

pub fn change_plan(
    actor: &AccessActor,
    binding: &AccessBindingRow,
    order: &AccessOrder,
    input: &ValidatedChange,
    key: &str,
    now: i64,
) -> Result<ChangePlan, BusinessError> {
    if input.expected_version != binding.version {
        return Err(binding_changed());
    }
    if input.action.is_review() {
        actor.require_approve(now)?;
        if binding.created_by == actor.user_id.as_str() {
            return Err(forbidden(
                "INDEPENDENT_APPROVER_REQUIRED",
                "Another authorized sales manager must review customer access.",
            ));
        }
        if binding.status != "pending" || binding.expires_at <= now {
            return Err(binding_changed());
        }
        if input.action == AccessAction::Approve && !order.source_eligible() {
            return Err(source_changed());
        }
    } else {
        if !matches!(binding.status.as_str(), "pending" | "approved") {
            return Err(binding_changed());
        }
        if binding.created_by != actor.user_id.as_str() && !actor.can_approve() {
            return Err(forbidden(
                "CUSTOMER_ACCESS_PERMISSION_REQUIRED",
                "Only the requester or a sales manager may revoke this access.",
            ));
        }
    }
    Ok(ChangePlan {
        id: binding.id.clone(),
        order_id: binding.order_id.clone(),
        expected_version: binding.version,
        version: binding.version + 1,
        expected_status: binding.status.clone(),
        status: input.action.status().to_owned(),
        approved_by: if input.action == AccessAction::Approve {
            Some(actor.user_id.as_str().to_owned())
        } else {
            binding.approved_by.clone()
        },
        reviewed_at: if input.action.is_review() {
            Some(now)
        } else {
            binding.reviewed_at
        },
        reason: input.reason.clone(),
        action: input.action,
        event_id: format!("customer-access-event-{key}"),
        now,
    })
}
