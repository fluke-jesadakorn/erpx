//! Portable Inventory reorder-policy command rules.
//!
//! A policy proposes a reorder point for one warehouse product; an independent
//! Company inventory approver activates, rejects, withdraws or retires it. This
//! module plans outcomes from current warehouse/product/policy facts; IAM
//! authority and the Workflow ledger stay at the composition boundary.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;

pub const MAX_REASON: usize = 2_000;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReorderSaveInput {
    pub warehouse_id: String,
    pub product_id: String,
    pub reorder_point: String,
    pub reason: String,
    #[serde(default)]
    pub expected_version: Option<i64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReorderAction {
    Approve,
    Reject,
    Withdraw,
    Retire,
}

impl ReorderAction {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "approve" => Some(Self::Approve),
            "reject" => Some(Self::Reject),
            "withdraw" => Some(Self::Withdraw),
            "retire" => Some(Self::Retire),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Reject => "reject",
            Self::Withdraw => "withdraw",
            Self::Retire => "retire",
        }
    }

    pub fn operation(self) -> String {
        format!("inventory.reorder.{}", self.as_str())
    }

    pub fn review(self) -> bool {
        matches!(self, Self::Approve | Self::Reject | Self::Retire)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReorderActionInput {
    pub action: ReorderAction,
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReorderWarehouse {
    pub id: String,
    pub branch_id: String,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReorderProduct {
    pub id: String,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReorderPolicyRecord {
    pub id: String,
    pub company_id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub branch_id: String,
    pub current_reorder_point_milli: Option<i64>,
    pub proposed_reorder_point_milli: Option<i64>,
    pub requested_by: String,
    pub approved_by: Option<String>,
    pub retired_at: Option<i64>,
    pub version: i64,
}

#[derive(Clone, Debug)]
pub struct ReorderSaveSource {
    pub policy: Option<ReorderPolicyRecord>,
    pub warehouse: Option<ReorderWarehouse>,
    pub product: Option<ReorderProduct>,
}

#[derive(Clone, Debug)]
pub struct ReorderTransitionSource {
    pub policy: ReorderPolicyRecord,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReorderSavePlan {
    pub id: String,
    pub company_id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub branch_id: String,
    pub point_milli: i64,
    pub reason: String,
    pub version: i64,
    pub revise: bool,
    pub expected_version: Option<i64>,
    pub warehouse_version: i64,
    pub product_version: i64,
    pub current_reorder_point_milli: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReorderTransitionPlan {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub action: ReorderAction,
    pub expected_version: i64,
    pub version: i64,
    pub reason: String,
    pub proposed_reorder_point_milli: Option<i64>,
    pub current_reorder_point_milli: Option<i64>,
    pub status: &'static str,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn require_reason(value: &str) -> Result<String, BusinessError> {
    let trimmed = value.trim_matches(char::is_whitespace);
    let length = trimmed.chars().count();
    if length < 5 || length > MAX_REASON {
        return Err(invalid(
            "REORDER_REASON_REQUIRED",
            "Record a bounded reorder reason with the required detail.",
        ));
    }
    Ok(trimmed.to_owned())
}

/// Plan a reorder-point proposal/revision. `id` is the deterministic
/// `reorder-<hash>` record derived by composition from company/warehouse/product.
pub fn plan_save(
    source: &ReorderSaveSource,
    input: &ReorderSaveInput,
    company_id: &str,
    id: &str,
) -> Result<ReorderSavePlan, BusinessError> {
    let reason = require_reason(&input.reason)?;
    let point = crate::domain::valuation::reorder_point_milli(&input.reorder_point)?;
    let warehouse = source.warehouse.as_ref().ok_or_else(|| {
        BusinessError::new(
            FailureKind::NotFound,
            "WAREHOUSE_NOT_FOUND",
            "The active warehouse is unavailable in the selected scope.",
        )
    })?;
    let product = source.product.as_ref().ok_or_else(|| {
        BusinessError::new(
            FailureKind::NotFound,
            "PRODUCT_NOT_FOUND",
            "The active product is unavailable in the selected Company.",
        )
    })?;
    if warehouse.id != input.warehouse_id || product.id != input.product_id {
        return Err(conflict(
            "REORDER_POLICY_CHANGED",
            "The warehouse or product changed. Refresh the stock view.",
        ));
    }
    if let Some(policy) = &source.policy {
        if policy.proposed_reorder_point_milli.is_some() {
            return Err(conflict(
                "REORDER_PROPOSAL_PENDING",
                "A reorder point for this product and warehouse is already awaiting review.",
            ));
        }
        if input.expected_version != Some(policy.version) {
            return Err(conflict(
                "REORDER_POLICY_CHANGED",
                "The reorder policy changed. Refresh and review it before proposing another value.",
            ));
        }
    } else if input.expected_version.is_some() {
        return Err(conflict(
            "REORDER_POLICY_CHANGED",
            "This reorder policy does not exist. Refresh the stock view.",
        ));
    }
    Ok(ReorderSavePlan {
        id: id.to_owned(),
        company_id: company_id.to_owned(),
        warehouse_id: warehouse.id.clone(),
        product_id: product.id.clone(),
        branch_id: warehouse.branch_id.clone(),
        point_milli: point,
        reason,
        version: source.policy.as_ref().map_or(1, |policy| policy.version + 1),
        revise: source.policy.is_some(),
        expected_version: input.expected_version,
        warehouse_version: warehouse.version,
        product_version: product.version,
        current_reorder_point_milli: source
            .policy
            .as_ref()
            .and_then(|policy| policy.current_reorder_point_milli),
    })
}

/// Plan an approve/reject/withdraw/retire decision under independent Company
/// inventory authority. The actor's branch scope and identity are supplied by
/// the caller from current authority.
#[allow(clippy::too_many_arguments)]
pub fn plan_transition(
    source: &ReorderTransitionSource,
    input: &ReorderActionInput,
    actor: &str,
    actor_branch: Option<&str>,
    can_prepare: bool,
    can_approve: bool,
) -> Result<ReorderTransitionPlan, BusinessError> {
    let policy = &source.policy;
    let reason = require_reason(&input.reason)?;
    if input.expected_version <= 0 || input.expected_version != policy.version {
        return Err(conflict(
            "REORDER_POLICY_CHANGED",
            "The reorder policy changed. Refresh and review it before continuing.",
        ));
    }
    if input.action.review() && (!can_approve || actor_branch.is_some()) {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "INVENTORY_APPROVAL_FORBIDDEN",
            "An independent Company inventory approver must review this policy.",
        ));
    }
    if input.action == ReorderAction::Withdraw && !can_prepare {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "INVENTORY_PREPARE_FORBIDDEN",
            "The current role cannot withdraw a reorder proposal.",
        ));
    }
    match input.action {
        ReorderAction::Approve | ReorderAction::Reject => {
            if policy.proposed_reorder_point_milli.is_none() {
                return Err(conflict(
                    "REORDER_PROPOSAL_MISSING",
                    "There is no reorder proposal awaiting review.",
                ));
            }
            if policy.requested_by == actor {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "INDEPENDENT_APPROVER_REQUIRED",
                    "A different Company approver must review the proposed reorder point.",
                ));
            }
        }
        ReorderAction::Withdraw => {
            if policy.proposed_reorder_point_milli.is_none() || policy.requested_by != actor {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "REORDER_WITHDRAW_FORBIDDEN",
                    "Only the person who proposed a pending reorder point can withdraw it.",
                ));
            }
        }
        ReorderAction::Retire => {
            if policy.proposed_reorder_point_milli.is_some()
                || policy.current_reorder_point_milli.is_none()
                || policy.retired_at.is_some()
            {
                return Err(conflict(
                    "REORDER_POLICY_NOT_ACTIVE",
                    "Only an active approved reorder point without a pending proposal can be retired.",
                ));
            }
            if policy.approved_by.as_deref() == Some(actor) {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "INDEPENDENT_APPROVER_REQUIRED",
                    "A different Company approver must retire this reorder point.",
                ));
            }
        }
    }
    let status = match input.action {
        ReorderAction::Reject => {
            if policy.current_reorder_point_milli.is_some() {
                "approved"
            } else {
                "unconfigured"
            }
        }
        ReorderAction::Retire => "retired",
        ReorderAction::Withdraw => {
            if policy.current_reorder_point_milli.is_some() {
                "approved"
            } else {
                "unconfigured"
            }
        }
        ReorderAction::Approve => "approved",
    };
    Ok(ReorderTransitionPlan {
        id: policy.id.clone(),
        company_id: policy.company_id.clone(),
        branch_id: policy.branch_id.clone(),
        action: input.action,
        expected_version: policy.version,
        version: policy.version + 1,
        reason,
        proposed_reorder_point_milli: policy.proposed_reorder_point_milli,
        current_reorder_point_milli: policy.current_reorder_point_milli,
        status,
    })
}
