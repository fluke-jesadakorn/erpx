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

/// Handoff identity captured from the request headers. Each handoff read uses a
/// bounded subset: list by project, work by user, point read by handoff id.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandoffScope {
    pub company: String,
    pub user: String,
    pub project: String,
    pub handoff: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HandoffRow {
    pub id: String,
    pub company_id: String,
    pub project_id: String,
    pub source_project_version: i64,
    pub recipient_id: String,
    pub status: String,
    pub version: i64,
    pub snapshot_json: String,
    pub snapshot_sha256: String,
    pub evidence: String,
    pub prepared_by: String,
    pub prepared_at: i64,
    pub decided_by: Option<String>,
    pub decided_at: Option<i64>,
    pub decision_reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HandoffProject {
    pub project_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HandoffWorkRow {
    pub id: String,
    pub version: i64,
    pub project_id: String,
    pub recipient_id: String,
    pub prepared_by: String,
    pub status: String,
    pub prepared_at: i64,
    pub decided_at: Option<i64>,
    pub title: String,
    pub lifecycle_status: String,
}

/// Owner-local Project row projected by the visible-row reads. The former TS
/// `SELECT p.*` projection is made explicit here so the response shape is owned
/// by Rust and cannot grow with unrelated schema columns.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProjectRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub title: String,
    pub description: String,
    pub owner_user_id: String,
    pub lifecycle_status: String,
    pub version: i64,
    pub updated_at: i64,
}

pub fn check_project(row: &ProjectRow) -> Result<(), BusinessError> {
    if !valid_id(&row.id)
        || !valid_id(&row.company_id)
        || !valid_id(&row.owner_user_id)
        || row.title.is_empty()
        || row.title.len() > 200
        || row.description.len() > 10_000
        || !matches!(row.lifecycle_status.as_str(), "active" | "handoff_pending" | "completed" | "cancelled")
        || row.version < 1
        || row.updated_at < 0
    {
        return Err(unavailable());
    }
    Ok(())
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

/// Company/user identity for list reads where no Project id is supplied. The
/// empty Project keeps the shared `ActualsScope` shape without weakening the
/// owner-or-member predicate executed by the visible-project query.
pub fn company_scope(company: &str, user: &str) -> Result<ActualsScope, BusinessError> {
    if !valid_id(company) || !valid_id(user) {
        return Err(invalid());
    }
    Ok(ActualsScope {
        company: company.to_owned(),
        user: user.to_owned(),
        project: String::new(),
    })
}

pub fn handoff_scope(
    company: &str,
    user: &str,
    project: &str,
    handoff: &str,
) -> Result<HandoffScope, BusinessError> {
    if !valid_id(company) || !valid_id(user) {
        return Err(invalid());
    }
    if !project.is_empty() && !valid_id(project) {
        return Err(invalid());
    }
    if !handoff.is_empty() && !valid_id(handoff) {
        return Err(invalid());
    }
    Ok(HandoffScope {
        company: company.to_owned(),
        user: user.to_owned(),
        project: project.to_owned(),
        handoff: handoff.to_owned(),
    })
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

pub fn check_handoff(row: &HandoffRow) -> Result<(), BusinessError> {
    if !valid_id(&row.id)
        || !valid_id(&row.company_id)
        || !valid_id(&row.project_id)
        || !valid_id(&row.recipient_id)
        || !valid_id(&row.prepared_by)
        || !matches!(row.status.as_str(), "pending" | "accepted" | "rejected" | "cancelled")
        || row.version < 1
        || row.source_project_version < 1
        || !valid_sha256(&row.snapshot_sha256)
        || row.prepared_at < 0
    {
        return Err(unavailable());
    }
    Ok(())
}

pub fn check_handoff_work(row: &HandoffWorkRow) -> Result<(), BusinessError> {
    if !valid_id(&row.id)
        || !valid_id(&row.project_id)
        || !valid_id(&row.recipient_id)
        || !valid_id(&row.prepared_by)
        || !matches!(row.status.as_str(), "pending" | "accepted" | "rejected" | "cancelled")
        || !matches!(row.lifecycle_status.as_str(), "active" | "handoff_pending" | "completed" | "cancelled")
        || row.version < 1
        || row.prepared_at < 0
    {
        return Err(unavailable());
    }
    Ok(())
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
    fn handoff_scope_is_bounded_per_read_kind() {
        assert!(handoff_scope("company-1", "user-1", "project-1", "").is_ok());
        assert!(handoff_scope("company-1", "user-1", "", "handoff-1").is_ok());
        assert!(handoff_scope("company-1", "user-1", "", "").is_ok());
        assert!(handoff_scope("", "user-1", "project-1", "handoff-1").is_err());
        assert!(handoff_scope("company-1", "user-1", "project-1", "bad/id").is_err());
        assert!(valid_sha256(&"a".repeat(64)));
        assert!(!valid_sha256(&"A".repeat(64)));
        assert!(!valid_sha256(&"a".repeat(63)));
    }

    #[test]
    fn company_scope_allows_list_reads_without_a_project() {
        let scope = company_scope("company-1", "user-1").unwrap();
        assert_eq!(scope.company, "company-1");
        assert_eq!(scope.user, "user-1");
        assert!(scope.project.is_empty());
        assert!(company_scope("", "user-1").is_err());
        assert!(company_scope("company-1", "bad/id").is_err());
    }

    #[test]
    fn visible_project_row_is_bounded() {
        let row = ProjectRow {
            id: "project-1".into(),
            company_id: "company-1".into(),
            branch_id: None,
            department_id: None,
            title: "Project 1".into(),
            description: String::new(),
            owner_user_id: "user-1".into(),
            lifecycle_status: "active".into(),
            version: 1,
            updated_at: 1,
        };
        assert!(check_project(&row).is_ok());
        let mut bad = row;
        bad.lifecycle_status = "unknown".into();
        assert!(check_project(&bad).is_err());
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
