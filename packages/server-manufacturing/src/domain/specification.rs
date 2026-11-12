//! Portable Manufacturing specification command facts.
//!
//! Create, independent review and retire are expressed as validated inputs,
//! policy decisions and immutable plans. The application layer supplies the
//! current authority, active product/routing sources and persistence; nothing
//! here reaches a database, clock or HTTP request. The retained TypeScript
//! contract (`specSchema`/`reviewSchema`) is preserved: draft creation at
//! version 1, independent approve/reject, retire only from `approved`, and a
//! current version/status CAS.
use crate::domain::bom::{
    BomActor, MAX_SAFE_INTEGER, identifier, js_trim, nonnegative, positive, text_bare,
    text_trimmed, valid_mode,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const CREATE_OPERATION: &str = "manufacturing.specification.create";
pub const APPROVE_OPERATION: &str = "manufacturing.specification.approve";
pub const REJECT_OPERATION: &str = "manufacturing.specification.reject";
pub const RETIRE_OPERATION: &str = "manufacturing.specification.retire";
pub const MAX_REASON: usize = 3_000;
pub const MAX_LINES: usize = 100;
pub const MAX_OPERATIONS: usize = 50;
pub const MAX_SUBSTITUTION_GROUP: usize = 80;

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "MANUFACTURING_REQUEST_INVALID",
        "Review the specification details and try again.",
    )
}

pub fn invalid_lines() -> BusinessError {
    BusinessError::invalid(
        "INVALID_SPECIFICATION_LINES",
        "Materials must be unique and different from output.",
    )
}

pub fn specification_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "SPECIFICATION_NOT_FOUND",
        "Specification not found.",
    )
}

pub fn review_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CONFLICT",
        "Independent current review is required.",
    )
}

pub fn source_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "MANUFACTURING_SOURCE_CHANGED",
        "Choose a current active product or routing resource.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "MANUFACTURING_IDEMPOTENCY_CONFLICT",
        "This Idempotency-Key was already used for a different Manufacturing command.",
    )
}

pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MANUFACTURING_READBACK_INVALID",
        "The specification command was not persisted as submitted.",
    )
}

pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MANUFACTURING_SOURCE_INVALID",
        "The current manufacturing specification source is invalid.",
    )
}

/// Exact request digest snapshot stored with one receipt row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptSnapshot {
    pub request_hash: String,
    pub result_json: String,
    pub operation: String,
    pub actor_id: String,
    pub record_id: String,
    pub authority_json: String,
}

/// Current owner row facts used for the review CAS and independence decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpecificationRow {
    pub id: String,
    pub status: String,
    pub version: i64,
    pub created_by: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewAction {
    Approve,
    Reject,
    Retire,
}

impl ReviewAction {
    pub fn operation(self) -> &'static str {
        match self {
            Self::Approve => APPROVE_OPERATION,
            Self::Reject => REJECT_OPERATION,
            Self::Retire => RETIRE_OPERATION,
        }
    }

    pub fn status(self) -> &'static str {
        match self {
            Self::Approve => "approved",
            Self::Reject => "rejected",
            Self::Retire => "retired",
        }
    }

    /// Retire is only allowed from an approved specification; approve/reject
    /// are only allowed from a draft.
    pub fn expected_status(self) -> &'static str {
        match self {
            Self::Approve | Self::Reject => "draft",
            Self::Retire => "approved",
        }
    }

    /// The retained rule forbids self-approval and self-rejection. The creator
    /// may retire their own approved specification when they still hold
    /// `records.review`.
    pub fn requires_independence(self) -> bool {
        matches!(self, Self::Approve | Self::Reject)
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpecificationLineInput {
    pub product_id: String,
    pub quantity: String,
    #[serde(default)]
    pub substitution_group: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpecificationOperationInput {
    pub resource_id: String,
    pub name: String,
    pub setup_minutes: i64,
    pub run_minutes_per_batch: i64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateRequest {
    pub name: String,
    pub mode: String,
    pub output_product_id: String,
    pub batch_quantity: String,
    pub expected_yield_bps: i64,
    pub lines: Vec<SpecificationLineInput>,
    #[serde(default)]
    pub operations: Vec<SpecificationOperationInput>,
}

impl CreateRequest {
    pub fn validate(self) -> Result<ValidatedCreate, BusinessError> {
        if !text_trimmed(&self.name, 2, 160)
            || !valid_mode(&self.mode)
            || !identifier(&self.output_product_id)
            || !(1..=20_000).contains(&self.expected_yield_bps)
            || !(1..=MAX_LINES).contains(&self.lines.len())
            || self.operations.len() > MAX_OPERATIONS
        {
            return Err(request_invalid());
        }
        let batch_quantity_milli =
            parse_quantity_milli(&self.batch_quantity).ok_or_else(request_invalid)?;
        if !positive(batch_quantity_milli) {
            return Err(request_invalid());
        }
        let mut products = BTreeSet::new();
        let mut lines = Vec::with_capacity(self.lines.len());
        for line in self.lines {
            if !identifier(&line.product_id)
                || line.product_id == self.output_product_id
                || !products.insert(line.product_id.clone())
            {
                return Err(invalid_lines());
            }
            let quantity_milli =
                parse_quantity_milli(&line.quantity).ok_or_else(request_invalid)?;
            if !positive(quantity_milli) {
                return Err(request_invalid());
            }
            let substitution_group = match line.substitution_group {
                // The certified BOM read requires a non-empty, trimmed,
                // control-free group of at most 80 UTF-8 bytes.
                Some(value) if text_bare(&value, 1, MAX_SUBSTITUTION_GROUP) => Some(value),
                Some(_) => return Err(request_invalid()),
                None => None,
            };
            lines.push(ValidatedLine {
                product_id: line.product_id,
                quantity_milli,
                substitution_group,
            });
        }
        let mut operations = Vec::with_capacity(self.operations.len());
        for operation in self.operations {
            if !identifier(&operation.resource_id)
                || !text_trimmed(&operation.name, 2, 160)
                || !nonnegative(operation.setup_minutes)
                || !positive(operation.run_minutes_per_batch)
            {
                return Err(request_invalid());
            }
            operations.push(ValidatedOperation {
                resource_id: operation.resource_id,
                name: operation.name,
                setup_minutes: operation.setup_minutes,
                run_minutes_per_batch: operation.run_minutes_per_batch,
            });
        }
        Ok(ValidatedCreate {
            name: self.name,
            mode: self.mode,
            output_product_id: self.output_product_id,
            batch_quantity_milli,
            expected_yield_bps: self.expected_yield_bps,
            lines,
            operations,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ValidatedLine {
    pub product_id: String,
    pub quantity_milli: i64,
    pub substitution_group: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ValidatedOperation {
    pub resource_id: String,
    pub name: String,
    pub setup_minutes: i64,
    pub run_minutes_per_batch: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ValidatedCreate {
    pub name: String,
    pub mode: String,
    pub output_product_id: String,
    pub batch_quantity_milli: i64,
    pub expected_yield_bps: i64,
    pub lines: Vec<ValidatedLine>,
    pub operations: Vec<ValidatedOperation>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewRequest {
    pub action: ReviewAction,
    pub expected_version: i64,
    pub reason: String,
}

impl ReviewRequest {
    pub fn validate(self) -> Result<ValidatedReview, BusinessError> {
        if !positive(self.expected_version) {
            return Err(request_invalid());
        }
        Ok(ValidatedReview {
            action: self.action,
            expected_version: self.expected_version,
            reason: validated_reason(&self.reason)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ValidatedReview {
    pub action: ReviewAction,
    pub expected_version: i64,
    pub reason: String,
}

fn validated_reason(value: &str) -> Result<String, BusinessError> {
    let value = js_trim(value);
    let chars = value.chars().count();
    if chars < 3 || value.len() > MAX_REASON || value.chars().any(char::is_control) {
        return Err(request_invalid());
    }
    Ok(value.to_owned())
}

/// The retained review guard: current status, exact version and independence.
pub fn require_reviewable(
    actor: &BomActor,
    row: &SpecificationRow,
    expected_version: i64,
    action: ReviewAction,
) -> Result<(), BusinessError> {
    if row.status != action.expected_status()
        || row.version != expected_version
        || (action.requires_independence() && row.created_by == actor.user_id)
    {
        return Err(review_conflict());
    }
    Ok(())
}

/// Parse the retained `^\d+(\.\d{1,3})?$` decimal quantity into milli-units.
pub fn parse_quantity_milli(value: &str) -> Option<i64> {
    let (integer, fraction) = match value.split_once('.') {
        Some((integer, fraction)) => (integer, Some(fraction)),
        None => (value, None),
    };
    if integer.is_empty() || !integer.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let fraction = match fraction {
        Some(fraction) => {
            if fraction.is_empty()
                || fraction.len() > 3
                || !fraction.bytes().all(|byte| byte.is_ascii_digit())
            {
                return None;
            }
            let mut digits = fraction.to_owned();
            while digits.len() < 3 {
                digits.push('0');
            }
            digits.parse::<i64>().ok()?
        }
        None => 0,
    };
    let integer = integer.parse::<i64>().ok()?;
    let value = integer.checked_mul(1_000)?.checked_add(fraction)?;
    (0..=MAX_SAFE_INTEGER).contains(&value).then_some(value)
}

/// Receipt result of one create/review command. Field names match the retained
/// `{id,status,version}` reply byte-for-byte.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandResult {
    pub id: String,
    pub status: String,
    pub version: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CreateLinePlan {
    pub id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub substitution_group: Option<String>,
    pub ordinal: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CreateOperationPlan {
    pub id: String,
    pub resource_id: String,
    pub name: String,
    pub setup_minutes: i64,
    pub run_minutes_per_batch: i64,
    pub ordinal: i64,
}

/// Fully decided draft creation, written to the caller's transaction.
#[derive(Clone, Debug, PartialEq)]
pub struct CreatePlan {
    pub key: String,
    pub actor: BomActor,
    pub request_hash: String,
    pub id: String,
    pub name: String,
    pub mode: String,
    pub output_product_id: String,
    pub batch_quantity_milli: i64,
    pub expected_yield_bps: i64,
    pub lines: Vec<CreateLinePlan>,
    pub operations: Vec<CreateOperationPlan>,
    pub now: i64,
}

/// Fully decided review transition, written to the caller's transaction.
#[derive(Clone, Debug, PartialEq)]
pub struct ReviewPlan {
    pub key: String,
    pub actor: BomActor,
    pub request_hash: String,
    pub specification: SpecificationRow,
    pub expected_version: i64,
    pub action: ReviewAction,
    pub status: String,
    pub reason: String,
    pub now: i64,
}
