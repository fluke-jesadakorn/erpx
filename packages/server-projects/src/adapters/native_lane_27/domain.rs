//! Pure response types, validation and invariants for the lane 27 Projects
//! actuals read. Nothing in this module touches SQL, HTTP or a database.

use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use serde::Serialize;

/// A registered Holding target. The host is the exact public origin host and
/// the slug is the exact path segment; both must be configured by the lead.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

/// Company/user/project identity captured from the request headers. All three
/// values are owner-bounded before any query is planned.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActualsScope {
    pub company: String,
    pub user: String,
    pub project: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkLogRow {
    pub id: String,
    pub project_id: String,
    pub user_id: String,
    pub work_date: String,
    pub minutes: i64,
    pub state: String,
    pub version: i64,
    pub reviewed_by: Option<String>,
    pub rejection_reason: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub is_actor: bool,
    pub can_review: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CostAllocationRow {
    pub id: String,
    pub project_id: String,
    pub journal_line_id: String,
    pub amount_minor: String,
    /// Exact decimal projection owned by Rust (the former TS minorToDecimal).
    pub amount: String,
    pub state: String,
    pub version: i64,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub rejection_reason: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub journal_id: String,
    pub journal_number: String,
    pub entry_date: String,
    pub currency: String,
    pub journal_status: String,
    pub account_code: String,
    pub account_name: String,
    pub can_review: bool,
    pub is_creator: bool,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PROJECT_ACTUALS_LANE27_REQUEST_INVALID",
        "Use one bounded Projects actuals read.",
    )
}

pub(crate) fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROJECT_ACTUALS_LANE27_UNAVAILABLE",
        "The native Projects actuals service is unavailable.",
    )
}

/// Bounded identity: no SQL metacharacters, path separators or control bytes
/// can reach a query or a route segment.
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && RecordId::new(value).is_ok()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte)
        })
}

pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
}

pub fn scope(company: &str, user: &str, project: &str) -> Result<ActualsScope, BusinessError> {
    if !valid_id(company) || !valid_id(user) || !valid_id(project) {
        return Err(invalid());
    }
    Ok(ActualsScope {
        company: company.to_owned(),
        user: user.to_owned(),
        project: project.to_owned(),
    })
}

fn parse_minor(value: &str) -> Option<i64> {
    if value.is_empty() || value.len() > 19 {
        return None;
    }
    value.parse::<i64>().ok()
}

/// Byte-for-byte parity with `minorToDecimal(value, 2)` in
/// `packages/feature-finance/src/lib/finance-money.ts`. Returning an error for
/// an unrepresentable stored value keeps the projection fail-closed.
pub fn minor_to_decimal(value: &str) -> Result<String, BusinessError> {
    let minor = parse_minor(value).ok_or_else(unavailable)?;
    let negative = minor < 0;
    let absolute = minor.unsigned_abs();
    Ok(format!(
        "{}{}.{:02}",
        if negative { "-" } else { "" },
        absolute / 100,
        absolute % 100
    ))
}

/// Fixed response invariants. A row with an unexpected shape is refused rather
/// than defaulted, so a schema change cannot leak an unprojected field.
pub fn check_work_log(row: &WorkLogRow) -> Result<(), BusinessError> {
    if !valid_id(&row.id)
        || !valid_id(&row.project_id)
        || !valid_id(&row.user_id)
        || row.work_date.len() != 10
        || row.minutes < 1
        || row.minutes > 1440
        || !matches!(row.state.as_str(), "submitted" | "approved" | "rejected")
        || row.version < 1
    {
        return Err(unavailable());
    }
    Ok(())
}

pub fn check_cost_allocation(row: &CostAllocationRow) -> Result<(), BusinessError> {
    if !valid_id(&row.id)
        || !valid_id(&row.project_id)
        || !valid_id(&row.journal_line_id)
        || parse_minor(&row.amount_minor).is_none()
        || minor_to_decimal(&row.amount_minor)? != row.amount
        || row.version < 1
        || !matches!(row.state.as_str(), "draft" | "approved" | "rejected")
    {
        return Err(unavailable());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minor_to_decimal_matches_ts_helper() {
        assert_eq!(minor_to_decimal("0").unwrap(), "0.00");
        assert_eq!(minor_to_decimal("7").unwrap(), "0.07");
        assert_eq!(minor_to_decimal("1250").unwrap(), "12.50");
        assert_eq!(minor_to_decimal("-1250").unwrap(), "-12.50");
        assert_eq!(minor_to_decimal("-7").unwrap(), "-0.07");
        assert!(minor_to_decimal("12.5").is_err());
        assert!(minor_to_decimal("").is_err());
    }

    #[test]
    fn scope_refuses_unbounded_identity() {
        assert!(scope("company-1", "user-1", "project-1").is_ok());
        assert!(scope("", "user-1", "project-1").is_err());
        assert!(scope("company-1", "user/1", "project-1").is_err());
        assert!(scope("company-1", "user-1", "project'1").is_err());
    }

    #[test]
    fn row_invariants_fail_closed() {
        let mut row = WorkLogRow {
            id: "log-1".into(),
            project_id: "project-1".into(),
            user_id: "user-1".into(),
            work_date: "2026-11-25".into(),
            minutes: 60,
            state: "submitted".into(),
            version: 1,
            reviewed_by: None,
            rejection_reason: String::new(),
            created_at: 1,
            updated_at: 1,
            is_actor: true,
            can_review: false,
        };
        assert!(check_work_log(&row).is_ok());
        row.minutes = 0;
        assert!(check_work_log(&row).is_err());
    }
}
