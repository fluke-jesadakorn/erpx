//! Native Sales confirmed sales-order cancellation rules.
//!
//! This packet owns the Sales selector/command surface for cancelling a
//! *confirmed* sales order that has no delivery. The retained TypeScript
//! boundary (`commerce.ts` `transitionOrder`) remains authoritative for draft
//! cancellation, quotations and the return-after-stock path.
//!
//! The command intentionally mirrors the Commerce order rules in
//! `server-commerce::domain::order` (independent canceller, fulfillment
//! reversal guard, exact-version CAS, scope and module checks) because the
//! Sales crate cannot add a dependency on the Commerce crate without a
//! lead-owned `Cargo.lock` update. Do not diverge from those rules; when the
//! lock is regenerated this module should delegate to the Commerce domain.
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub const OPERATION: &str = "commerce.order.cancel";

fn has(values: &[String], expected: &str) -> bool {
    values.iter().any(|value| value == expected)
}

#[derive(Clone, Debug)]
pub struct SalesCancellationActor {
    pub user_id: String,
    /// Retained principal aliases from the server-authorized Identity context.
    pub person_user_ids: Vec<String>,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub assignment_id: String,
    pub role: Option<String>,
    pub authority_revision: Option<i64>,
    pub assignment_version: Option<i64>,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub expires_at: i64,
}

impl SalesCancellationActor {
    fn same_person(&self, user_id: &str) -> bool {
        self.user_id == user_id || self.person_user_ids.iter().any(|alias| alias == user_id)
    }

    pub fn require_write(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !has(&self.modules, "sales") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "COMMERCE_MODULE_FORBIDDEN",
                "Your current role cannot access Sales and Commercial.",
            ));
        }
        if now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        if !has(&self.permissions, "commerce.sales") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "COMMERCE_PERMISSION_REQUIRED",
                "Your selected role does not grant the required Commercial permission.",
            ));
        }
        Ok(())
    }

    pub fn require_approve(&self) -> Result<(), BusinessError> {
        if !has(&self.permissions, "commerce.approve") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "COMMERCE_APPROVAL_REQUIRED",
                "Your selected role does not grant Commercial approval.",
            ));
        }
        Ok(())
    }

    /// Retention-compatible authority projection for audit/outbox readers. It
    /// records the selected assignment and scope; it grants no authority.
    pub fn authority_json(&self) -> String {
        json!({
            "assignmentId": self.assignment_id,
            "companyId": self.company_id.as_str(),
            "branchId": self.branch_id.as_ref().map(RecordId::as_str),
            "departmentId": self.department_id.as_ref().map(RecordId::as_str),
            "modules": self.modules,
            "permissions": self.permissions,
            "role": self.role,
            "revision": self.authority_revision,
            "assignmentVersion": self.assignment_version,
        })
        .to_string()
    }
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "SALES_CANCELLATION_REQUEST_INVALID",
        "Use one bounded confirmed sales-order cancellation request.",
    )
}

pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "SALES_CANCELLATION_SOURCE_INVALID",
        "The current sales order source is invalid.",
    )
}

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "SALES_ORDER_NOT_FOUND",
        "Sales order not found.",
    )
}

pub fn permission_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "COMMERCE_PERMISSION_REQUIRED",
        "Your selected role does not grant the required Commercial permission.",
    )
}

pub fn approval_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "COMMERCE_APPROVAL_REQUIRED",
        "Your selected role does not grant Commercial approval.",
    )
}

pub fn independent_canceller_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "INDEPENDENT_CANCELLER_REQUIRED",
        "A different manager must cancel an approved document.",
    )
}

pub fn order_reversal_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ORDER_REVERSAL_REQUIRED",
        "This order has stock movements. Use the return workflow instead of cancellation.",
    )
}

pub fn transition_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CONFLICT",
        "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This command key belongs to a different request.",
    )
}

pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "SALES_CANCELLATION_READBACK_INVALID",
        "The cancelled sales order could not be verified.",
    )
}

pub fn sale_required() -> BusinessError {
    BusinessError::invalid(
        "SALES_CANCELLATION_ORDER_REQUIRED",
        "Only a confirmed sales order can be cancelled here.",
    )
}

pub fn confirmed_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SALES_CANCELLATION_CONFIRMED_REQUIRED",
        "Only a confirmed sales order can be cancelled here. Use the draft order action for other states.",
    )
}

/// Canonical receipt shape accepted from the owner command. Cancellation never
/// mints a converted sales id.
pub fn validate_receipt_shape(id: &str, state: &str, version: i64, sales_id: Option<&str>) -> bool {
    !id.is_empty()
        && id.len() <= 200
        && state == "cancelled"
        && (2..=9_007_199_254_740_991).contains(&version)
        && sales_id.is_none()
}

/// Cancellation never carries a free-text reason in the retained Commerce
/// action; the exact body is `{expectedVersion}`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesCancellationCommand {
    pub key: String,
    pub order_id: String,
    pub expected_version: i64,
}

impl SalesCancellationCommand {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.key.is_empty()
            || self.key.len() > 200
            || self.order_id.is_empty()
            || self.order_id.len() > 200
            || !(1..=9_007_199_254_740_991).contains(&self.expected_version)
        {
            return Err(request_invalid());
        }
        Ok(())
    }

    /// Stable replay identity. Matches the retained Commerce transition
    /// identity so an existing receipt hashes to the same value.
    pub fn identity(&self, actor: &SalesCancellationActor) -> String {
        json!({
            "user": actor.user_id,
            "assignment": actor.assignment_id,
            "company": actor.company_id.as_str(),
            "branch": actor.branch_id.as_ref().map(RecordId::as_str),
            "department": actor.department_id.as_ref().map(RecordId::as_str),
            "operation": OPERATION,
            "input": {
                "id": self.order_id,
                "action": "cancel",
                "expectedVersion": self.expected_version,
                "reason": null,
            },
        })
        .to_string()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesCancellationRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub kind: String,
    pub state: String,
    pub version: i64,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub updated_at: i64,
}

impl SalesCancellationRow {
    pub fn validate_scope(&self, actor: &SalesCancellationActor) -> Result<(), BusinessError> {
        if self.company_id != actor.company_id.as_str()
            || actor
                .branch_id
                .as_ref()
                .is_some_and(|branch| self.branch_id.as_deref() != Some(branch.as_str()))
            || actor.department_id.as_ref().is_some_and(|department| {
                self.department_id.as_deref() != Some(department.as_str())
            })
        {
            return Err(source_invalid());
        }
        Ok(())
    }

    /// The owner-local state guard: a confirmed sales order only. Draft
    /// cancellation, quotations and post-delivery states stay with the retained
    /// Commerce/return paths.
    pub fn validate_confirmed_sale(&self) -> Result<(), BusinessError> {
        if self.kind != "sales" {
            return Err(sale_required());
        }
        if self.state != "confirmed" {
            return Err(confirmed_required());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SalesCancellationReceipt {
    pub id: String,
    pub state: String,
    pub version: i64,
    #[serde(rename = "salesId")]
    pub sales_id: Option<String>,
}

impl SalesCancellationReceipt {
    pub fn cancelled(order_id: &str, expected_version: i64) -> Self {
        Self {
            id: order_id.to_owned(),
            state: "cancelled".to_owned(),
            version: expected_version + 1,
            sales_id: None,
        }
    }

    pub fn to_wire(&self) -> serde_json::Value {
        json!({
            "id": self.id,
            "state": self.state,
            "version": self.version,
            "salesId": self.sales_id,
        })
    }

    pub fn validate(&self, order_id: &str, expected_version: i64) -> Result<(), BusinessError> {
        if self.id != order_id
            || self.state != "cancelled"
            || self.version != expected_version + 1
            || self.sales_id.is_some()
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct SalesCancellationPlan {
    pub order_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub expected_version: i64,
    pub next_state: String,
    pub actor_id: String,
    pub now: i64,
    pub receipt: SalesCancellationReceipt,
}

/// Authorize one exact-version confirmed sales cancellation. Performs no I/O;
/// the application has already locked the scoped order and counted
/// fulfillments on the caller's serializable transaction.
pub fn authorize_confirmed_cancellation(
    actor: &SalesCancellationActor,
    command: &SalesCancellationCommand,
    order: &SalesCancellationRow,
    fulfillment_count: i64,
    now: TimestampMillis,
) -> Result<SalesCancellationPlan, BusinessError> {
    actor.require_write(now)?;
    order.validate_scope(actor)?;
    order.validate_confirmed_sale()?;
    // Retained legacy order: the independent-canceller check runs before the
    // fulfillment reversal guard and before version/CAS.
    if !has(&actor.permissions, "commerce.approve") || actor.same_person(&order.created_by) {
        return Err(independent_canceller_required());
    }
    if fulfillment_count != 0 {
        return Err(order_reversal_required());
    }
    if order.version != command.expected_version {
        return Err(transition_conflict());
    }
    Ok(SalesCancellationPlan {
        order_id: order.id.clone(),
        company_id: order.company_id.clone(),
        branch_id: order.branch_id.clone(),
        department_id: order.department_id.clone(),
        expected_version: command.expected_version,
        next_state: "cancelled".to_owned(),
        actor_id: actor.user_id.clone(),
        now: now.get(),
        receipt: SalesCancellationReceipt::cancelled(&order.id, command.expected_version),
    })
}

pub fn validate_readback(
    plan: &SalesCancellationPlan,
    row: &SalesCancellationRow,
) -> Result<(), BusinessError> {
    if row.id != plan.order_id
        || row.company_id != plan.company_id
        || row.branch_id != plan.branch_id
        || row.department_id != plan.department_id
        || row.state != plan.next_state
        || row.version != plan.expected_version + 1
    {
        return Err(readback_invalid());
    }
    Ok(())
}

/// Bounded receipt record returned by the owner repository.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancellationReceiptRecord {
    pub request_hash: String,
    pub result_json: String,
    pub record_id: Option<String>,
}
