//! Native POS cash-movement command facts.
//!
//! The qualified slice records one cash-in/cash-out movement against an open
//! register session. It does not close a session, approve a movement, post a
//! Finance journal, touch stock or replace the retained POS sale/refund paths.
//! Callers retain the normal Account/Holding session, purchase admission and
//! current assignment authority; this owner policy keeps the current Company,
//! optional branch, work-context expiry, `pos.sell` permission, `pos` module
//! and exact command shape.
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, FailureKind, HoldingId, RecordId,
    TimestampMillis, UserId,
};

pub const OPERATION: &str = "cash.create";
const AMOUNT_LIMIT: i64 = 99_999_999_999;
const REASON_MIN: usize = 3;
const REASON_MAX: usize = 1_000;

#[derive(Clone, Debug)]
pub struct CashMovementActor {
    pub user_id: UserId,
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub assignment_id: AssignmentId,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub expires_at: i64,
}

impl CashMovementActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn has_module(&self, module: &str) -> bool {
        self.modules.iter().any(|value| value == module)
    }

    pub fn require_cash(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has("pos.sell") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "POS_PERMISSION_REQUIRED",
                "Your selected role does not grant POS selling access.",
            ));
        }
        if !self.has_module("pos") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "POS_MODULE_FORBIDDEN",
                "Your current role cannot access POS.",
            ));
        }
        if now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        if self.department_id.is_some() {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "POS_BRANCH_REQUIRED",
                "Select a Company or branch for POS.",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct CreateCashMovement {
    pub key: CommandKey,
    pub shift_id: RecordId,
    pub expected_version: i64,
    pub amount_minor: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedCashMovement {
    pub shift_id: String,
    pub expected_version: i64,
    pub amount_minor: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CashShift {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub business_date: String,
    pub state: String,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CashReceipt {
    pub request_hash: String,
    pub result_json: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CashMovementReadback {
    pub id: String,
    pub shift_id: String,
    pub amount_minor: i64,
    pub reason: String,
    pub approved_by: Option<String>,
    pub created_at: i64,
    pub shift_version: i64,
    pub shift_state: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CashMovementReceipt {
    pub id: String,
}

impl CashMovementReceipt {
    pub fn to_wire(&self) -> serde_json::Value {
        serde_json::json!({ "id": self.id })
    }
}

pub(crate) fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "POS_CASH_REQUEST_INVALID",
        "Enter a valid cash amount and reason for an open session.",
    )
}

pub(crate) fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "POS_CASH_SOURCE_INVALID",
        "The current POS cash source is invalid.",
    )
}

pub(crate) fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "POS_CASH_SOURCE_INVALID",
        "The recorded cash movement could not be read back.",
    )
}

pub(crate) fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "POS_NOT_FOUND",
        "Record not found in this workspace.",
    )
}

pub(crate) fn version_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "POS_VERSION_CONFLICT",
        "Record changed. Load the latest data and try again.",
    )
}

pub(crate) fn day_closed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "POS_DAY_CLOSED",
        "This business day is preparing to close. Use a current-day adjustment.",
    )
}

pub(crate) fn setup_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "POS_SETUP_REQUIRED",
        "Configure POS accounts and sales points before use.",
    )
}

pub(crate) fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "Command number is already used for other data.",
    )
}

impl CreateCashMovement {
    pub fn validate(&self) -> Result<ValidatedCashMovement, BusinessError> {
        if self.expected_version < 1
            || self.amount_minor == 0
            || !(-AMOUNT_LIMIT..=AMOUNT_LIMIT).contains(&self.amount_minor)
        {
            return Err(request_invalid());
        }
        let reason = self.reason.trim();
        let reason_len = reason.chars().count();
        if !(REASON_MIN..=REASON_MAX).contains(&reason_len) {
            return Err(request_invalid());
        }
        Ok(ValidatedCashMovement {
            shift_id: self.shift_id.as_str().to_owned(),
            expected_version: self.expected_version,
            amount_minor: self.amount_minor,
            reason: reason.to_owned(),
        })
    }

    pub fn key(&self) -> &CommandKey {
        &self.key
    }
}

impl ValidatedCashMovement {
    pub fn validate_shift(
        &self,
        actor: &CashMovementActor,
        shift: &CashShift,
    ) -> Result<(), BusinessError> {
        if shift.id != self.shift_id
            || shift.company_id != actor.company_id.as_str()
            || shift.branch_id.is_empty()
            || actor
                .branch_id
                .as_ref()
                .is_some_and(|branch| shift.branch_id != branch.as_str())
            || !valid_date(&shift.business_date)
        {
            return Err(not_found());
        }
        if shift.version != self.expected_version {
            return Err(version_conflict());
        }
        if shift.state != "open" {
            return Err(request_invalid());
        }
        Ok(())
    }

    pub fn validate_readback(&self, movement: &CashMovementReadback) -> Result<(), BusinessError> {
        if movement.id.is_empty()
            || !valid_uuid(&movement.id)
            || movement.shift_id != self.shift_id
            || movement.amount_minor != self.amount_minor
            || movement.reason != self.reason
            || movement.approved_by.is_some()
            || movement.created_at < 0
            || movement.shift_state != "open"
            || movement.shift_version != self.expected_version + 1
        {
            return Err(readback_invalid());
        }
        Ok(())
    }
}

pub fn valid_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 8 | 13 | 18 | 23) {
            if *byte != b'-' {
                return false;
            }
        } else if !byte.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).expect("a Rust string always serializes as JSON")
}

/// Canonical receipt identity. The top-level field order matches the retained
/// command identity; the bounded input object is normalised after validation.
pub fn receipt_identity(actor: &CashMovementActor, movement: &ValidatedCashMovement) -> String {
    let branch = actor
        .branch_id
        .as_ref()
        .map_or_else(|| "null".to_owned(), |value| json_string(value.as_str()));
    let input = format!(
        "{{\"id\":{},\"expectedVersion\":{},\"amountMinor\":{},\"reason\":{}}}",
        json_string(&movement.shift_id),
        movement.expected_version,
        movement.amount_minor,
        json_string(&movement.reason),
    );
    format!(
        "{{\"user\":{},\"assignment\":{},\"company\":{},\"branch\":{},\"operation\":{},\"input\":{}}}",
        json_string(actor.user_id.as_str()),
        json_string(actor.assignment_id.as_str()),
        json_string(actor.company_id.as_str()),
        branch,
        json_string(OPERATION),
        input,
    )
}

pub fn parse_receipt_result(value: &str) -> Result<CashMovementReceipt, BusinessError> {
    // The stored result is owner-written. Reject anything that is not exactly
    // one nonempty UUID id so a corrupt receipt cannot be presented as success.
    if value.len() > 4_096 {
        return Err(source_invalid());
    }
    let parsed: serde_json::Value = serde_json::from_str(value).map_err(|_| source_invalid())?;
    let Some(object) = parsed.as_object() else {
        return Err(source_invalid());
    };
    if object.len() != 1 {
        return Err(source_invalid());
    }
    let id = object
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(source_invalid)?;
    if !valid_uuid(id) {
        return Err(source_invalid());
    }
    Ok(CashMovementReceipt { id: id.to_owned() })
}
