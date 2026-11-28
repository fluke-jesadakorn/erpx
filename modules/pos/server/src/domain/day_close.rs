//! Decisions for POS day-close preparation and independent confirmation.
//!
//! The report and same-person witness must be obtained under the same owner
//! transaction as the eventual write. This module does not claim the report,
//! its JavaScript-compatible digest, or the persistence adapter are native.
use crate::domain::commands::PosActor;
use erpx_platform_shared_server::public::{BusinessError, FailureKind, TimestampMillis};

#[derive(Clone, Debug)]
pub struct DayReportFacts {
    pub company_id: String,
    pub branch_id: String,
    pub business_date: String,
    pub digest: String,
    pub blockers: Vec<String>,
    pub shift_count: usize,
}

#[derive(Clone, Debug)]
pub struct PreparedDayClose {
    pub company_id: String,
    pub branch_id: String,
    pub business_date: String,
    pub state: String,
    pub version: i64,
    pub source_digest: String,
    pub prepared_by: String,
}

fn valid_date(value: &str) -> bool {
    let b = value.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return false;
    }
    let number = |slice: &[u8]| {
        slice
            .iter()
            .fold(0_u32, |n, c| n * 10 + u32::from(c - b'0'))
    };
    let year = number(&b[0..4]);
    let month = number(&b[5..7]);
    let day = number(&b[8..10]);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    day > 0 && day <= max
}

fn error(kind: FailureKind, code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(kind, code, message)
}
fn require(actor: &PosActor, now: TimestampMillis, permission: &str) -> Result<(), BusinessError> {
    if !actor.permissions.iter().any(|item| item == permission)
        || !actor.modules.iter().any(|item| item == "pos")
    {
        return Err(error(
            FailureKind::Forbidden,
            "POS_PERMISSION_REQUIRED",
            "Your selected role does not grant this POS command.",
        ));
    }
    if now.get() >= actor.expires_at {
        return Err(error(
            FailureKind::Conflict,
            "WORK_CONTEXT_CHANGED",
            "Your selected work context changed.",
        ));
    }
    Ok(())
}
fn scope(actor: &PosActor, company: &str, branch: &str) -> Result<(), BusinessError> {
    if actor.department_id.is_some()
        || actor.company_id.as_str() != company
        || actor
            .branch_id
            .as_ref()
            .is_some_and(|id| id.as_str() != branch)
    {
        return Err(error(
            FailureKind::NotFound,
            "POS_NOT_FOUND",
            "Branch is outside the current workspace.",
        ));
    }
    Ok(())
}

/// Call after verifying the branch exists and the report was generated for the
/// exact selected company, branch and date while holding the day-close lock.
pub fn validate_prepare(
    actor: &PosActor,
    now: TimestampMillis,
    branch: &str,
    date: &str,
    today: &str,
    already_closed: bool,
    report: &DayReportFacts,
) -> Result<(), BusinessError> {
    require(actor, now, "pos.approve")?;
    require(actor, now, "pos.reports")?;
    if branch.is_empty() || !valid_date(date) || !valid_date(today) {
        return Err(BusinessError::invalid(
            "POS_REQUEST_INVALID",
            "A branch and valid business date are required.",
        ));
    }
    if date > today {
        return Err(BusinessError::invalid(
            "POS_FUTURE_DATE",
            "Close only the current or an earlier business date.",
        ));
    }
    if already_closed {
        return Err(error(
            FailureKind::Conflict,
            "POS_DAY_CLOSED",
            "This business day is already prepared.",
        ));
    }
    scope(actor, &report.company_id, branch)?;
    if report.branch_id != branch || report.business_date != date || report.digest.is_empty() {
        return Err(error(
            FailureKind::Unavailable,
            "POS_SOURCE_INVALID",
            "The current POS source could not be verified.",
        ));
    }
    if !report.blockers.is_empty() {
        return Err(error(
            FailureKind::Conflict,
            "POS_DAY_BLOCKED",
            "Resolve the report blockers before preparing the day close.",
        ));
    }
    if report.shift_count == 0 {
        return Err(BusinessError::invalid(
            "POS_DAY_EMPTY",
            "No cash sessions exist for this business date.",
        ));
    }
    Ok(())
}

/// `same_person` must be the IAM identity witness, not a user-id comparison.
/// The caller locks the close and re-reads the report in the write transaction.
pub fn validate_confirm(
    actor: &PosActor,
    now: TimestampMillis,
    close: &PreparedDayClose,
    expected_version: i64,
    same_person: bool,
    report: &DayReportFacts,
) -> Result<(), BusinessError> {
    require(actor, now, "finance.approve")?;
    require(actor, now, "pos.reports")?;
    scope(actor, &close.company_id, &close.branch_id)?;
    if expected_version < 1 {
        return Err(BusinessError::invalid(
            "POS_REQUEST_INVALID",
            "A positive expected version is required.",
        ));
    }
    if close.version != expected_version {
        return Err(error(
            FailureKind::Conflict,
            "POS_VERSION_CONFLICT",
            "Reload the current close.",
        ));
    }
    if close.state != "prepared" || same_person || actor.user_id.as_str() == close.prepared_by {
        return Err(error(
            FailureKind::Conflict,
            "POS_INDEPENDENT_REVIEW",
            "Finance must independently confirm the day close.",
        ));
    }
    if report.company_id != close.company_id
        || report.branch_id != close.branch_id
        || report.business_date != close.business_date
        || report.digest.is_empty()
    {
        return Err(error(
            FailureKind::Unavailable,
            "POS_SOURCE_INVALID",
            "The current POS source could not be verified.",
        ));
    }
    if !report.blockers.is_empty() || report.digest != close.source_digest {
        return Err(error(
            FailureKind::Conflict,
            "POS_DAY_CHANGED",
            "Source records changed after preparation.",
        ));
    }
    Ok(())
}
