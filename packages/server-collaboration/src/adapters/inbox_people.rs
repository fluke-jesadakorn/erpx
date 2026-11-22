//! Native People visibility delegate for the Inbox destination port.
//!
//! This mirrors the retained per-row TypeScript gates for the
//! notification-visible People sources:
//!
//! - `roster_week` / `roster_request`: `rosterNotificationLink` in
//!   `packages/server-people_payroll/src/application/people-roster.ts`
//!   (people.workforce history gate, current roster assignment, identity
//!   reads, manager-or-participant visibility, department access) with the
//!   exact href shapes including the request query string.
//! - `roster_coverage`: the `rosterData` coverage branch — the record id
//!   parses as `department:week:person`, the reader can manage rosters, the
//!   department is in access, the week is a valid Monday, and a coverage gap
//!   for the current or next Bangkok week matches. A non-matching id stays
//!   invisible without an error, exactly like the retained `continue`.
//! - `overtime`: the `getOvertime` identity read (creator or approval
//!   participant); href carries the raw id, no source override.
//! - `leave` / `leave_cancellation`: the `getLeave` identity read
//!   (creator, decider, approval participants, or evidence reader); source
//!   carries the numeric version; href carries the raw id.
//! - `expense`: the `getExpense` visibility read (claimant/creator/approver,
//!   approval participants, company-scoped reviewer arms); href carries the
//!   raw id, no source override.
//! - `retirement_clearance`: the task resolves from the frame-scoped
//!   clearance list for the assignee or an authorized role, with the
//!   enforced-mode lifecycle history proof; href is the task's clearance
//!   path, no source override.
//!
//! The retained per-row catch swallows 403/404 for every branch here, so no
//! denial codes exist for these source types: every negative path is
//! invisible (`Ok(None)`). The only error beyond storage and fail-closed is
//! the retained 422 week validation on the coverage branch.
//!
//! Nothing here opens a pool, commits, or retries. It runs serializable-safe
//! SELECTs on the caller's connection.
//!
//! Faithful-mirror notes (documented, not silent):
//! - JWT-static `hasPermission` halves are enforced database-current via
//!   `iam_role_allows` on the reader's resolved assignment.
//! - The command-side guards (`workContextGuards`, `iamMutationGuards`,
//!   assertion clearing, purchase boundaries, receipts) authorize commands,
//!   not reads.
//! - Leave legacy routing (`readGetLeaveLegacy` plus `approvalView`) and
//!   expense decoration/approval (`approvalFields`, `canDecide`) are workflow
//!   presentation machinery, not visibility: rows passing the identity proof
//!   resolve without them.
//! - Expense treasury/cancelled duty arms are not mirrored; those rows fail
//!   closed here while the oracle could show them through the settlement
//!   paths. Expense `canDecide` approvers without a deciding record, audit
//!   path, or reviewer grant rely on live workflow evaluation and are not
//!   proven here either.
//! - `approval_instances.record_type` is database-constrained to
//!   `expense|leave|task`, so the overtime and leave_cancellation
//!   participant arms can never match; the predicate is still evaluated
//!   literally.
//! - Active-staff resolution collapses to exactly one active
//!   person/engagement row, mirroring the retained `STAFF_LINK_REQUIRED`
//!   collapse (which the application maps to no staff).
use crate::adapters::inbox_pg::{InboxTarget, InboxTargetFuture, InboxTargetPort};
use crate::adapters::inbox_targets::{
    encode_uri_component, retained_inbox_destination, source_record_label,
};
use erpx_server_commercial::public::capability_decision::PgCapabilityDecisionReader;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, current_package_catalogue,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use serde_json::json;
use sqlx::{PgConnection, Row};

/// Source types this delegate can prove against People rows.
pub fn is_people_inbox_target(source_type: &str) -> bool {
    matches!(
        source_type,
        "roster_week"
            | "roster_request"
            | "roster_coverage"
            | "overtime"
            | "leave"
            | "leave_cancellation"
            | "expense"
            | "retirement_clearance"
    )
}

/// Retained destination address for one roster week, byte-identical to the
/// TypeScript `` `/people/roster?department=${encodeURIComponent(department)}&week=${week}` ``.
pub fn roster_week_href(department_id: &str, week_start: &str) -> String {
    format!(
        "/people/roster?department={}&week={}",
        encode_uri_component(department_id),
        week_start,
    )
}

/// Retained destination address for one roster request: the week address
/// plus the exact `view=requests&request=` suffix.
pub fn roster_request_href(department_id: &str, week_start: &str, request_id: &str) -> String {
    format!(
        "{}&view=requests&request={}",
        roster_week_href(department_id, week_start),
        encode_uri_component(request_id),
    )
}

/// Retained destination address for one overtime row: the raw id, exactly
/// like the TypeScript `` `/overtime/${row.record_id}` ``.
pub fn overtime_href(source_id: &str) -> String {
    format!("/overtime/{source_id}")
}

/// Retained destination address for one leave row: the raw id, exactly like
/// the TypeScript `` `/tasks/people_payroll/${row.record_id}` ``.
pub fn leave_href(source_id: &str) -> String {
    format!("/tasks/people_payroll/{source_id}")
}

/// Retained destination address for one expense row: the raw id, exactly
/// like the TypeScript `` `/expenses/${row.record_id}` ``.
pub fn expense_href(source_id: &str) -> String {
    format!("/expenses/{source_id}")
}

/// Retained destination address for one clearance task: the task's own
/// clearance path, exactly like the retained `task.href`.
pub fn retirement_case_href(case_id: &str) -> String {
    format!("/people/clearance/{case_id}")
}

/// Coverage record shape: `department:week_start:person_id` with a real
/// calendar date and a colon-free person segment, byte-identical to the
/// retained `/^(.*):(\d{4}-\d{2}-\d{2}):([^:]+)$/` match.
pub fn parse_roster_coverage_id(source_id: &str) -> Option<(&str, &str, &str)> {
    let mut parts = source_id.rsplitn(3, ':');
    let person_id = parts.next()?;
    let week_start = parts.next()?;
    let department_id = parts.next()?;
    if parts.next().is_some() || person_id.is_empty() || !is_iso_date(week_start) {
        return None;
    }
    Some((department_id, week_start, person_id))
}

fn is_iso_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let digits = |range: std::ops::Range<usize>| {
        value[range].parse::<i64>().ok().filter(|_| {
            value.as_bytes()[0..10]
                .iter()
                .enumerate()
                .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
        })
    };
    let (Some(year), Some(month), Some(day)) = (digits(0..4), digits(5..7), digits(8..10)) else {
        return false;
    };
    civil_day_number(year, month, day).is_some()
}

/// Proleptic Gregorian day number; `None` for impossible calendar dates.
fn civil_day_number(year: i64, month: i64, day: i64) -> Option<i64> {
    if !(1..=12).contains(&month) || day < 1 {
        return None;
    }
    let leap = month == 2 && year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if day > max_day {
        return None;
    }
    let adjusted_year = if month <= 2 { year - 1 } else { year };
    let era = adjusted_year.div_euclid(400);
    let year_of_era = adjusted_year - era * 400;
    let month_prime = (month + 9) % 12;
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146097 + day_of_era - 719468)
}

fn civil_from_day_number(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719468;
    let era = shifted.div_euclid(146097);
    let day_of_era = shifted - era * 146097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    (
        if month <= 2 { year + 1 } else { year },
        month,
        day,
    )
}

fn format_iso_date(year: i64, month: i64, day: i64) -> String {
    format!("{year:04}-{month:02}-{day:02}")
}

/// Monday (00:00 Asia/Bangkok has no daylight saving) of the current and next
/// coverage weeks for one clock reading, mirroring the retained
/// `rosterMonday(today)` plus seven days.
pub fn bangkok_coverage_weeks(now_ms: i64) -> Option<(String, String)> {
    let days = (now_ms + 7 * 3_600_000).div_euclid(86_400_000);
    // 1970-01-01 was a Thursday; Monday is weekday zero.
    let monday = days - (days + 3).rem_euclid(7);
    let (year, month, day) = civil_from_day_number(monday);
    let (next_year, next_month, next_day) = civil_from_day_number(monday + 7);
    if !(1..=12).contains(&month) || !(1..=12).contains(&next_month) {
        return None;
    }
    Some((
        format_iso_date(year, month, day),
        format_iso_date(next_year, next_month, next_day),
    ))
}

/// Retained Monday validation for coverage weeks: a real date falling on a
/// Monday, mirroring `z.iso.date` plus `rosterMonday(start) !== start`.
pub fn is_roster_monday(week_start: &str) -> bool {
    let mut parts = week_start.split('-');
    let (Some(year), Some(month), Some(day)) = (
        parts.next().and_then(|value| value.parse::<i64>().ok()),
        parts.next().and_then(|value| value.parse::<i64>().ok()),
        parts.next().and_then(|value| value.parse::<i64>().ok()),
    ) else {
        return false;
    };
    if parts.next().is_some() {
        return false;
    }
    civil_day_number(year, month, day).is_some_and(|days| (days + 3).rem_euclid(7) == 0)
}

/// Layer selection shares the retained enforced-mode check; see
/// [`crate::adapters::inbox_payroll::uses_enforced_layer`].
use crate::adapters::inbox_payroll::uses_enforced_layer;

fn unsupported() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "INBOX_TARGET_SOURCE_UNSUPPORTED",
        "This Inbox source is not part of the retained projection.",
    )
}

fn owner_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "INBOX_TARGET_PORT_UNAVAILABLE",
        "This Inbox destination requires its owning workspace.",
    )
}

fn storage() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "INBOX_TARGET_QUERY_FAILED",
        "The Inbox destination is unavailable.",
    )
}

fn pivot(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => storage(),
    }
}

/// Native People visibility delegate, composed via
/// `PgInboxTargets::with_delegate`. People-only: every other source type
/// defers to the existing fail-closed behavior instead of inventing a link.
pub struct PeopleInboxTargets {
    branch_id: Option<String>,
    department_id: Option<String>,
    assignment_id: Option<String>,
    enforcement: Option<PackageEnforcementMode>,
}

impl PeopleInboxTargets {
    pub fn new(branch_id: Option<&str>, department_id: Option<&str>) -> Self {
        Self {
            branch_id: branch_id.map(str::to_owned),
            department_id: department_id.map(str::to_owned),
            assignment_id: None,
            enforcement: None,
        }
    }

    /// Bind the exact selected assignment. Without it the unit resolves the
    /// highest active assignment for the current user and Company.
    pub fn assignment(mut self, assignment_id: Option<&str>) -> Self {
        self.assignment_id = assignment_id
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        self
    }

    /// Pin the package-enforcement mode. Without it the delegate mirrors the
    /// retained people environment-driven mode exactly. This seam exists so
    /// tests and composition can select a mode deterministically; it never
    /// changes what the mode means.
    pub fn enforcement_mode(mut self, mode: PackageEnforcementMode) -> Self {
        self.enforcement = Some(mode);
        self
    }
}

impl Default for PeopleInboxTargets {
    fn default() -> Self {
        Self::new(None, None)
    }
}

/// Retained `peoplePackageEnforcementMode`, mirrored from the TypeScript
/// oracle: disabled opens the read, a non-local environment fails with the
/// environment code, and an unknown value fails closed with the
/// configuration code.
fn enforcement_mode() -> Result<PackageEnforcementMode, BusinessError> {
    match std::env::var("ERPX_PEOPLE_PACKAGE_ENFORCEMENT")
        .unwrap_or_else(|_| "disabled".to_owned())
        .as_str()
    {
        "disabled" => Ok(PackageEnforcementMode::Disabled),
        "shadow" => Ok(PackageEnforcementMode::Shadow),
        "enforced" => {
            if std::env::var("ERPX_ENVIRONMENT").ok().as_deref() != Some("local") {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "PACKAGE_ENFORCEMENT_ENVIRONMENT",
                    "Package enforcement is unavailable in this environment.",
                ));
            }
            Ok(PackageEnforcementMode::Enforced)
        }
        _ => Err(BusinessError::new(
            FailureKind::Unavailable,
            "PACKAGE_ENFORCEMENT_CONFIGURATION",
            "Package enforcement is unavailable.",
        )),
    }
}

/// Retained approval-participant predicate, literal mirror including the
/// role arm bound to the reader's assignment. Approval kinds outside the
/// database-constrained set can never match.
fn participant_sql(kind: &str, user_param: u8, assignment_param: u8) -> String {
    let template = match kind {
        "overtime" => "EXISTS(SELECT 1 FROM approval_instances ai JOIN approval_steps ast ON ast.instance_id=ai.id WHERE ai.record_type='overtime' AND ai.record_id=o.id AND ai.company_id=o.company_id AND EXISTS(SELECT 1 FROM (SELECT ast.approver_user_id user_id UNION SELECT value FROM erpx_json_each(COALESCE(ast.approver_pool_json,'[]'))) participant WHERE iam_same_person(participant.user_id,?,ai.company_id)) AND (ast.state IN ('approved','rejected') OR ast.stage='department_approval' AND jsonb_exists(ast.route_snapshot_json::jsonb,'departmentPolicyVersion') OR ast.stage='hr_approval' AND ast.route_snapshot_json::jsonb->>'appointedPersonId' IS NOT NULL OR EXISTS(SELECT 1 FROM assignments aa WHERE aa.id=? AND iam_role_allows(aa.role,aa.company_id,'workflow.'||COALESCE(ast.required_role,CASE WHEN ast.authority IN ('named','department') THEN 'people_manager' ELSE ast.authority END)) AND aa.company_id=ai.company_id AND (aa.scope_kind='holding' OR aa.scope_kind='branch' AND aa.scope_node_id=ai.branch_id OR aa.scope_kind='department' AND aa.scope_node_id=ai.department_id))))",
        "leave" => "EXISTS(SELECT 1 FROM approval_instances ai JOIN approval_steps ast ON ast.instance_id=ai.id WHERE ai.record_type='leave' AND ai.record_id=l.id AND ai.company_id=l.company_id AND EXISTS(SELECT 1 FROM (SELECT ast.approver_user_id user_id UNION SELECT value FROM erpx_json_each(COALESCE(ast.approver_pool_json,'[]'))) participant WHERE iam_same_person(participant.user_id,?,ai.company_id)) AND (ast.state IN ('approved','rejected') OR ast.stage='department_approval' AND jsonb_exists(ast.route_snapshot_json::jsonb,'departmentPolicyVersion') OR ast.stage='hr_approval' AND ast.route_snapshot_json::jsonb->>'appointedPersonId' IS NOT NULL OR EXISTS(SELECT 1 FROM assignments aa WHERE aa.id=? AND iam_role_allows(aa.role,aa.company_id,'workflow.'||COALESCE(ast.required_role,CASE WHEN ast.authority IN ('named','department') THEN 'people_manager' ELSE ast.authority END)) AND aa.company_id=ai.company_id AND (aa.scope_kind='holding' OR aa.scope_kind='branch' AND aa.scope_node_id=ai.branch_id OR aa.scope_kind='department' AND aa.scope_node_id=ai.department_id))))",
        "leave_cancellation" => "EXISTS(SELECT 1 FROM approval_instances ai JOIN approval_steps ast ON ast.instance_id=ai.id WHERE ai.record_type='leave_cancellation' AND ai.record_id=l.id AND ai.company_id=l.company_id AND EXISTS(SELECT 1 FROM (SELECT ast.approver_user_id user_id UNION SELECT value FROM erpx_json_each(COALESCE(ast.approver_pool_json,'[]'))) participant WHERE iam_same_person(participant.user_id,?,ai.company_id)) AND (ast.state IN ('approved','rejected') OR ast.stage='department_approval' AND jsonb_exists(ast.route_snapshot_json::jsonb,'departmentPolicyVersion') OR ast.stage='hr_approval' AND ast.route_snapshot_json::jsonb->>'appointedPersonId' IS NOT NULL OR EXISTS(SELECT 1 FROM assignments aa WHERE aa.id=? AND iam_role_allows(aa.role,aa.company_id,'workflow.'||COALESCE(ast.required_role,CASE WHEN ast.authority IN ('named','department') THEN 'people_manager' ELSE ast.authority END)) AND aa.company_id=ai.company_id AND (aa.scope_kind='holding' OR aa.scope_kind='branch' AND aa.scope_node_id=ai.branch_id OR aa.scope_kind='department' AND aa.scope_node_id=ai.department_id))))",
        "expense" => "EXISTS(SELECT 1 FROM approval_instances ai JOIN approval_steps ast ON ast.instance_id=ai.id WHERE ai.record_type='expense' AND ai.record_id=x.id AND ai.company_id=x.company_id AND EXISTS(SELECT 1 FROM (SELECT ast.approver_user_id user_id UNION SELECT value FROM erpx_json_each(COALESCE(ast.approver_pool_json,'[]'))) participant WHERE iam_same_person(participant.user_id,?,ai.company_id)) AND (ast.state IN ('approved','rejected') OR ast.stage='department_approval' AND jsonb_exists(ast.route_snapshot_json::jsonb,'departmentPolicyVersion') OR ast.stage='hr_approval' AND ast.route_snapshot_json::jsonb->>'appointedPersonId' IS NOT NULL OR EXISTS(SELECT 1 FROM assignments aa WHERE aa.id=? AND iam_role_allows(aa.role,aa.company_id,'workflow.'||COALESCE(ast.required_role,CASE WHEN ast.authority IN ('named','department') THEN 'people_manager' ELSE ast.authority END)) AND aa.company_id=ai.company_id AND (aa.scope_kind='holding' OR aa.scope_kind='branch' AND aa.scope_node_id=ai.branch_id OR aa.scope_kind='department' AND aa.scope_node_id=ai.department_id))))",
        _ => "FALSE",
    };
    // The retained oracle binds `?` positionally (MySQL-style). The native
    // port numbers the same two slots per call site: the participant user
    // first, the assignment id second.
    let user_slot = format!("${user_param}");
    let assignment_slot = format!("${assignment_param}");
    template.replacen('?', &user_slot, 1).replacen('?', &assignment_slot, 1)
}

/// Resolve the reader's assignment: the bound selection, else the highest
/// active assignment for the current user and Company.
async fn resolve_assignment(
    c: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    assignment_id: Option<&str>,
    now: i64,
) -> Result<Option<(String, String)>, BusinessError> {
    sqlx::query_as(
        "SELECT g.id,g.role FROM assignments g \
          WHERE g.company_id=$1 AND g.user_id=$2 AND ($3::text IS NULL OR g.id=$3) \
          AND g.revoked_at IS NULL \
          AND g.valid_from<=$4 AND g.valid_until>$4 \
          AND iam_assignment_active(g.id,$4) \
          AND iam_principal_active(g.user_id,g.company_id,$4) \
          ORDER BY g.version DESC LIMIT 1",
    )
    .bind(company_id)
    .bind(user_id)
    .bind(assignment_id)
    .bind(now)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())
}

/// Current role permission on the resolved assignment, the database-current
/// half of the retained JWT-static permission checks.
async fn role_allows(
    c: &mut PgConnection,
    company_id: &str,
    assignment_id: &str,
    permission: &str,
) -> Result<bool, BusinessError> {
    sqlx::query_scalar(
        "SELECT iam_role_allows((SELECT role FROM assignments WHERE id=$1), $2, $3)",
    )
    .bind(assignment_id)
    .bind(company_id)
    .bind(permission)
    .fetch_one(&mut *c)
    .await
    .map_err(|_| storage())
}

/// Retained roster assignment currency, literal mirror of the oracle
/// predicate: the exact selected (or highest active) assignment is current
/// with a holding or frame-matching branch. No permission or IAM-function
/// checks — the oracle has none on this path.
async fn roster_assignment_current(
    c: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    assignment_id: Option<&str>,
    branch_id: Option<&str>,
    now: i64,
) -> Result<bool, BusinessError> {
    let current: Option<String> = sqlx::query_scalar(
        "SELECT g.id FROM assignments g \
          WHERE g.company_id=$1 AND g.user_id=$2 AND ($3::text IS NULL OR g.id=$3) \
          AND g.valid_from<=$4 AND g.valid_until>$4 AND g.revoked_at IS NULL \
          AND ($5::text IS NULL OR g.branch_id IS NULL OR g.branch_id=$5) \
          ORDER BY g.version DESC LIMIT 1",
    )
    .bind(company_id)
    .bind(user_id)
    .bind(assignment_id)
    .bind(now)
    .bind(branch_id)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    Ok(current.is_some())
}

/// Retained workforce authority (overtime/leave/expense gate): a current
/// assignment over the selected frame. No permission check — the oracle's
/// no-permission branch only proves assignment currency.
async fn workforce_assignment_current(
    c: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    assignment_id: Option<&str>,
    branch_id: Option<&str>,
    department_id: Option<&str>,
    now: i64,
) -> Result<bool, BusinessError> {
    selected_assignment(
        c,
        company_id,
        user_id,
        assignment_id,
        None,
        branch_id,
        department_id,
        now,
    )
    .await
}

/// Assignment currency with an optional retained permission, mirroring the
/// `assignmentReadSql` shape: current, principal-active, role-allowing, and
/// scoped. `None` permission checks bare assignment existence exactly like
/// the retained no-permission branch.
async fn selected_assignment(
    c: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    assignment_id: Option<&str>,
    permission: Option<&str>,
    scope_branch_id: Option<&str>,
    scope_department_id: Option<&str>,
    now: i64,
) -> Result<bool, BusinessError> {
    let permission_check = match permission {
        Some(permission) => format!(
            "AND iam_role_allows(g.role,g.company_id,{})",
            sql_quote(permission)
        ),
        None => "AND EXISTS(SELECT 1 FROM iam_roles role \
          WHERE role.id=g.role AND role.company_id IN ('*',g.company_id) \
          AND role.status='active')"
            .to_owned(),
    };
    let current: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT g.id FROM assignments g \
          WHERE g.company_id=$1 AND g.user_id=$2 AND ($3::text IS NULL OR g.id=$3) \
          AND g.revoked_at IS NULL \
          AND g.valid_from<=$4 AND g.valid_until>$4 \
          AND iam_assignment_active(g.id,$4) \
          AND iam_principal_active(g.user_id,g.company_id,$4) \
          {permission_check} \
          AND (g.scope_kind='holding' \
            OR (g.scope_kind='branch' AND g.scope_node_id=$5) \
            OR (g.scope_kind='department' AND g.scope_node_id=$6)) \
          ORDER BY g.version DESC LIMIT 1",
    )))
    .bind(company_id)
    .bind(user_id)
    .bind(assignment_id)
    .bind(now)
    .bind(scope_branch_id)
    .bind(scope_department_id)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    Ok(current.is_some())
}

/// Static permission names only; this never interpolates caller input.
fn sql_quote(value: &str) -> String {
    debug_assert!(value.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'.'));
    format!("'{value}'")
}

/// Retained purchase-history decision: granted means visible, denied means
/// invisible. Layer 1 never throws, so the check (not require) is the exact
/// mirror — enforced denials read as invisible, exactly like the retained
/// predicate filtering the row out.
async fn people_history(
    c: &mut PgConnection,
    company_id: &CompanyId,
    capability: &str,
    mode: PackageEnforcementMode,
    now: i64,
) -> Result<bool, BusinessError> {
    if mode == PackageEnforcementMode::Disabled {
        return Ok(true);
    }
    let catalogue = current_package_catalogue()?;
    let mut reader = PgCapabilityDecisionReader::new(company_id.clone(), c, &catalogue);
    let check = reader
        .check(
            capability,
            CapabilityPurpose::History,
            mode,
            TimestampMillis::new(now),
        )
        .await
        .map_err(pivot)?;
    Ok(check.decision().is_some_and(|decision| decision.allowed))
}

/// Active staff person for department access, mirroring the retained
/// `activeStaff` collapse: exactly one active person/engagement row, else no
/// staff (the retained `STAFF_LINK_REQUIRED` maps to null in `self()`).
async fn active_staff(
    c: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    now: i64,
) -> Result<Option<(String, Option<String>)>, BusinessError> {
    let rows = sqlx::query(
        "SELECT p.id AS person_id,e.department_id AS department_id FROM hr_people p \
          JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
          WHERE p.company_id=$1 AND p.user_id=$2 AND p.status='active' AND p.merged_into_id IS NULL \
          AND e.status='active' \
          AND e.start_date<=to_char((to_timestamp($3/1000.0) AT TIME ZONE 'UTC'),'YYYY-MM-DD') \
          AND (e.end_date IS NULL OR e.end_date>=to_char((to_timestamp($3/1000.0) AT TIME ZONE 'UTC'),'YYYY-MM-DD')) \
          AND EXISTS(SELECT 1 FROM assignments a \
            WHERE a.user_id=p.user_id AND a.company_id=p.company_id AND a.revoked_at IS NULL \
            AND a.valid_from<=$3 AND a.valid_until>$3 \
            AND iam_assignment_active(a.id,$3) AND iam_principal_active(a.user_id,a.company_id,$3))",
    )
    .bind(company_id)
    .bind(user_id)
    .bind(now)
    .fetch_all(&mut *c)
    .await
    .map_err(|_| storage())?;
    if rows.len() != 1 {
        return Ok(None);
    }
    let person_id: String = rows[0].try_get("person_id").map_err(|_| storage())?;
    let department_id: Option<String> = rows[0].try_get("department_id").map_err(|_| storage())?;
    Ok(Some((person_id, department_id)))
}

/// Retained department access: the department resolves inside the reader's
/// accessible set, literal mirror of the organization-nodes query.
async fn roster_departments(
    c: &mut PgConnection,
    company_id: &str,
    branch_id: Option<&str>,
    department_id: Option<&str>,
    manager: bool,
    staff_department_id: Option<&str>,
) -> Result<Vec<String>, BusinessError> {
    sqlx::query_scalar(
        "WITH RECURSIVE subtree AS ( \
          SELECT id FROM organization_nodes WHERE id=$1 AND company_id=$2 \
          UNION ALL SELECT n.id FROM organization_nodes n JOIN subtree p ON n.parent_id=p.id \
          WHERE n.company_id=$3 AND n.status='active') \
        SELECT d.id FROM organization_nodes d \
        LEFT JOIN organization_nodes b ON b.id=d.branch_id \
        WHERE d.company_id=$4 AND d.kind='department' AND d.status='active' \
        AND ($5::text IS NULL OR d.branch_id=$6) AND ($7::text IS NULL OR d.id IN (SELECT id FROM subtree)) \
        AND ($8 OR d.id=$9)",
    )
    .bind(department_id)
    .bind(company_id)
    .bind(company_id)
    .bind(company_id)
    .bind(branch_id)
    .bind(branch_id)
    .bind(department_id)
    .bind(manager)
    .bind(staff_department_id)
    .fetch_all(&mut *c)
    .await
    .map_err(|_| storage())
}

async fn department_visible(
    c: &mut PgConnection,
    company_id: &str,
    branch_id: Option<&str>,
    department_id: Option<&str>,
    manager: bool,
    staff_department_id: Option<&str>,
    department: &str,
) -> Result<bool, BusinessError> {
    Ok(roster_departments(
        c,
        company_id,
        branch_id,
        department_id,
        manager,
        staff_department_id,
    )
    .await?
    .iter()
    .any(|id| id == department))
}

/// One manager flag for the three roster read permissions, enforced
/// database-current on the resolved assignment.
async fn roster_manager(
    c: &mut PgConnection,
    company_id: &str,
    assignment_id: Option<&str>,
) -> Result<bool, BusinessError> {
    let Some(assignment_id) = assignment_id else {
        return Ok(false);
    };
    for permission in ["roster.read", "roster.manage", "roster.approve"] {
        if role_allows(c, company_id, assignment_id, permission).await? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Retained coverage gaps for one department and week, literal mirror of the
/// notification gap query over base tables (the platform projections are
/// plain column-subset selects of these tables).
async fn coverage_gaps(
    c: &mut PgConnection,
    company_id: &str,
    department_id: &str,
    week_start: &str,
) -> Result<Vec<(String, String)>, BusinessError> {
    sqlx::query(
        "SELECT p.id person_id,p.legal_name,e.id engagement_id,$1::date::text week_start \
          FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
          JOIN organization_positions pos ON pos.id=e.position_id AND pos.company_id=e.company_id AND pos.active=1 AND pos.roster_required=1 \
          WHERE p.company_id=$2 AND e.department_id=$3 AND p.status='active' AND p.merged_into_id IS NULL \
          AND e.status='active' AND e.start_date::date<=($4::date+6) AND (e.end_date IS NULL OR e.end_date::date>=$4::date) \
          AND EXISTS (SELECT 1 FROM generate_series(0,6) AS day_offset(offset_days) \
            CROSS JOIN LATERAL (SELECT $5::date + day_offset.offset_days AS calendar_day) day \
            WHERE (extract(isodow FROM day.calendar_day)<=5 OR NOT EXISTS ( \
              SELECT 1 FROM generate_series(0,6) AS weekday_offset(offset_days) \
              CROSS JOIN LATERAL (SELECT $6::date + weekday_offset.offset_days AS calendar_day) weekday \
              WHERE extract(isodow FROM weekday.calendar_day)<=5 AND weekday.calendar_day>=e.start_date::date \
              AND (e.end_date IS NULL OR weekday.calendar_day<=e.end_date::date))) \
            AND day.calendar_day>=e.start_date::date AND (e.end_date IS NULL OR day.calendar_day<=e.end_date::date) \
            AND NOT EXISTS (SELECT 1 FROM leave_requests l LEFT JOIN employees emp ON emp.id=l.employee_id AND emp.company_id=l.company_id \
              WHERE l.company_id=e.company_id AND l.state='approved' AND coalesce(l.person_id,emp.person_id)=p.id \
              AND (EXISTS (SELECT 1 FROM leave_day_segments ds WHERE ds.request_id=l.id AND ds.company_id=l.company_id AND ds.leave_date::date=day.calendar_day AND ds.segment='full') \
                OR NOT EXISTS (SELECT 1 FROM leave_day_segments ds WHERE ds.request_id=l.id) AND day.calendar_day BETWEEN l.start_date::date AND coalesce(l.end_date::date,l.start_date::date+l.days::int-1) \
                  AND (day.calendar_day<>l.start_date::date OR l.start_segment='full') AND (l.end_date IS NULL OR day.calendar_day<>l.end_date::date OR l.end_segment='full'))) \
            AND NOT EXISTS (SELECT 1 FROM people_roster_assignments a JOIN people_roster_shifts s ON s.id=a.shift_id AND s.state='active' \
              JOIN people_roster_weeks w ON w.id=s.week_id WHERE w.company_id=e.company_id AND w.department_id=e.department_id AND w.week_start=$7::date \
              AND a.engagement_id=e.id AND a.state='assigned')) ORDER BY p.legal_name,p.id",
    )
    .bind(week_start)
    .bind(company_id)
    .bind(department_id)
    .bind(week_start)
    .bind(week_start)
    .bind(week_start)
    .bind(week_start)
    .fetch_all(&mut *c)
    .await
    .map_err(|_| storage())?
    .into_iter()
    .map(|row| {
        let week: String = row.try_get("week_start").map_err(|_| storage())?;
        let person: String = row.try_get("person_id").map_err(|_| storage())?;
        Ok((week, person))
    })
    .collect()
}

struct RosterWeekRow {
    department_id: String,
    week_start: String,
    state: String,
}

async fn read_week(
    c: &mut PgConnection,
    company_id: &str,
    source_id: &str,
) -> Result<Option<RosterWeekRow>, BusinessError> {
    sqlx::query("SELECT department_id,week_start::text AS week_start,state FROM people_roster_weeks WHERE id=$1 AND company_id=$2")
        .bind(source_id)
        .bind(company_id)
        .fetch_optional(&mut *c)
        .await
        .map_err(|_| storage())?
        .map(|row| {
            let department_id: String = row.try_get("department_id").map_err(|_| storage())?;
            let week_start: String = row.try_get("week_start").map_err(|_| storage())?;
            let state: String = row.try_get("state").map_err(|_| storage())?;
            Ok(RosterWeekRow {
                department_id,
                week_start,
                state,
            })
        })
        .transpose()
}

struct RosterShiftRow {
    week_id: String,
}

async fn read_shift(
    c: &mut PgConnection,
    company_id: &str,
    source_id: &str,
) -> Result<Option<RosterShiftRow>, BusinessError> {
    sqlx::query(
        "SELECT s.week_id FROM people_roster_shifts s JOIN people_roster_weeks w ON w.id=s.week_id \
          WHERE s.id=$1 AND w.company_id=$2",
    )
    .bind(source_id)
    .bind(company_id)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?
    .map(|row| {
        let week_id: String = row.try_get("week_id").map_err(|_| storage())?;
        Ok(RosterShiftRow { week_id })
    })
    .transpose()
}

struct RosterRequestRow {
    requester_person_id: String,
    coworker_person_id: Option<String>,
    target_shift_id: String,
}

async fn read_request(
    c: &mut PgConnection,
    company_id: &str,
    source_id: &str,
) -> Result<Option<RosterRequestRow>, BusinessError> {
    sqlx::query(
        "SELECT requester_person_id,coworker_person_id,target_shift_id \
          FROM people_roster_requests WHERE id=$1 AND company_id=$2",
    )
    .bind(source_id)
    .bind(company_id)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?
    .map(|row| {
        let requester_person_id: String =
            row.try_get("requester_person_id").map_err(|_| storage())?;
        let coworker_person_id: Option<String> =
            row.try_get("coworker_person_id").map_err(|_| storage())?;
        let target_shift_id: String = row.try_get("target_shift_id").map_err(|_| storage())?;
        Ok(RosterRequestRow {
            requester_person_id,
            coworker_person_id,
            target_shift_id,
        })
    })
    .transpose()
}

async fn request_participant(
    c: &mut PgConnection,
    company_id: &str,
    person_id: Option<&str>,
    user_id: &str,
) -> Result<bool, BusinessError> {
    let Some(person_id) = person_id else {
        return Ok(false);
    };
    let hit: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM hr_people WHERE id=$1 AND company_id=$2 AND iam_same_person(user_id,$3,company_id)",
    )
    .bind(person_id)
    .bind(company_id)
    .bind(user_id)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    Ok(hit.is_some())
}

struct PeopleGate {
    company: CompanyId,
    user_id: String,
    assignment_id: Option<String>,
    branch_id: Option<String>,
    department_id: Option<String>,
    mode: PackageEnforcementMode,
    now: i64,
}

/// Shared enforced gate for roster/overtime/leave reads: history decision,
/// then current roster assignment. Both denials read as invisible, exactly
/// like the retained per-row catch.
async fn roster_gate(
    c: &mut PgConnection,
    gate: &PeopleGate,
    capability: &str,
) -> Result<bool, BusinessError> {
    if !people_history(c, &gate.company, capability, gate.mode, gate.now).await? {
        return Ok(false);
    }
    roster_assignment_current(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.branch_id.as_deref(),
        gate.now,
    )
    .await
}

/// Shared enforced gate for workforce reads (overtime/leave/expense):
/// history decision, then current assignment over the selected frame.
async fn workforce_gate(
    c: &mut PgConnection,
    gate: &PeopleGate,
    capability: &str,
) -> Result<bool, BusinessError> {
    if !people_history(c, &gate.company, capability, gate.mode, gate.now).await? {
        return Ok(false);
    }
    workforce_assignment_current(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.branch_id.as_deref(),
        gate.department_id.as_deref(),
        gate.now,
    )
    .await
}

async fn roster_week_target(
    c: &mut PgConnection,
    gate: &PeopleGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    if uses_enforced_layer(gate.mode) && !roster_gate(c, gate, "people.workforce").await? {
        return Ok(None);
    }
    let Some(week) = read_week(c, gate.company.as_str(), source_id).await? else {
        return Ok(None);
    };
    let assignment = resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?;
    let manager = match &assignment {
        Some((id, _)) => roster_manager(c, gate.company.as_str(), Some(id)).await?,
        None => false,
    };
    if !manager && week.state != "published" {
        return Ok(None);
    }
    let staff = active_staff(c, gate.company.as_str(), &gate.user_id, gate.now).await?;
    if !department_visible(
        c,
        gate.company.as_str(),
        gate.branch_id.as_deref(),
        gate.department_id.as_deref(),
        manager,
        staff.as_ref().and_then(|(_, department)| department.as_deref()),
        &week.department_id,
    )
    .await?
    {
        return Ok(None);
    }
    Ok(Some(InboxTarget {
        href: Some(roster_week_href(&week.department_id, &week.week_start)),
        source: None,
    }))
}

async fn roster_request_target(
    c: &mut PgConnection,
    gate: &PeopleGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    if uses_enforced_layer(gate.mode) && !roster_gate(c, gate, "people.workforce").await? {
        return Ok(None);
    }
    let Some(request) = read_request(c, gate.company.as_str(), source_id).await? else {
        return Ok(None);
    };
    let assignment = resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?;
    let manager = match &assignment {
        Some((id, _)) => roster_manager(c, gate.company.as_str(), Some(id)).await?,
        None => false,
    };
    let own = request_participant(
        c,
        gate.company.as_str(),
        Some(&request.requester_person_id),
        &gate.user_id,
    )
    .await?;
    let coworker = request_participant(
        c,
        gate.company.as_str(),
        request.coworker_person_id.as_deref(),
        &gate.user_id,
    )
    .await?;
    if !manager && !own && !coworker {
        return Ok(None);
    }
    let Some(shift) = read_shift(c, gate.company.as_str(), &request.target_shift_id).await? else {
        return Ok(None);
    };
    let Some(week) = read_week(c, gate.company.as_str(), &shift.week_id).await? else {
        return Ok(None);
    };
    let staff = active_staff(c, gate.company.as_str(), &gate.user_id, gate.now).await?;
    if !department_visible(
        c,
        gate.company.as_str(),
        gate.branch_id.as_deref(),
        gate.department_id.as_deref(),
        manager,
        staff.as_ref().and_then(|(_, department)| department.as_deref()),
        &week.department_id,
    )
    .await?
    {
        return Ok(None);
    }
    Ok(Some(InboxTarget {
        href: Some(roster_request_href(
            &week.department_id,
            &week.week_start,
            source_id,
        )),
        source: None,
    }))
}

async fn roster_coverage_target(
    c: &mut PgConnection,
    gate: &PeopleGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    let Some((department_id, week_start, person_id)) = parse_roster_coverage_id(source_id) else {
        return Ok(None);
    };
    if uses_enforced_layer(gate.mode) && !roster_gate(c, gate, "people.workforce").await? {
        return Ok(None);
    }
    let assignment = resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?;
    let can_manage = match &assignment {
        Some((id, _)) => role_allows(c, gate.company.as_str(), id, "roster.manage").await?,
        None => false,
    };
    if !can_manage {
        return Ok(None);
    }
    let staff = active_staff(c, gate.company.as_str(), &gate.user_id, gate.now).await?;
    if !department_visible(
        c,
        gate.company.as_str(),
        gate.branch_id.as_deref(),
        gate.department_id.as_deref(),
        can_manage,
        staff.as_ref().and_then(|(_, department)| department.as_deref()),
        department_id,
    )
    .await?
    {
        return Ok(None);
    }
    if !is_roster_monday(week_start) {
        return Err(BusinessError::invalid(
            "ROSTER_WEEK",
            "Choose a valid Monday.",
        ));
    }
    let Some((current, next)) = bangkok_coverage_weeks(gate.now) else {
        return Err(storage());
    };
    for week in [current, next] {
        for (gap_week, gap_person) in
            coverage_gaps(c, gate.company.as_str(), department_id, &week).await?
        {
            if gap_week == week_start && gap_person == person_id {
                return Ok(Some(InboxTarget {
                    href: Some(roster_week_href(department_id, week_start)),
                    source: None,
                }));
            }
        }
    }
    Ok(None)
}

async fn overtime_target(
    c: &mut PgConnection,
    gate: &PeopleGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    if uses_enforced_layer(gate.mode) && !workforce_gate(c, gate, "people.workforce").await? {
        return Ok(None);
    }
    let assignment_key = resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?
    .map(|(id, _)| id);
    let visible: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT o.id FROM overtime_requests o WHERE o.id=$1 AND o.company_id=$2 \
          AND (o.created_by=$3 OR {} )",
        participant_sql("overtime", 4, 5),
    )))
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(&gate.user_id)
    .bind(&gate.user_id)
    .bind(assignment_key.as_deref())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if visible.is_none() {
        return Ok(None);
    }
    Ok(Some(InboxTarget {
        href: Some(overtime_href(source_id)),
        source: None,
    }))
}

async fn leave_target(
    c: &mut PgConnection,
    gate: &PeopleGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    if uses_enforced_layer(gate.mode) && !workforce_gate(c, gate, "people.workforce").await? {
        return Ok(None);
    }
    let assignment_key = resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?
    .map(|(id, _)| id);
    let evidence_reader = match &assignment_key {
        Some(id) => role_allows(c, gate.company.as_str(), id, "leave.evidence.read").await?,
        None => false,
    };
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT l.id,l.version FROM leave_requests l LEFT JOIN leave_types t ON t.id=l.leave_type_id AND t.company_id=l.company_id \
          WHERE l.id=$1 AND l.company_id=$2 \
          AND (l.created_by=$3 OR l.decided_by=$4 OR {} OR {} OR $9::boolean)",
        participant_sql("leave", 5, 6),
        participant_sql("leave_cancellation", 7, 8),
    )))
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(&gate.user_id)
    .bind(&gate.user_id)
    .bind(&gate.user_id)
    .bind(assignment_key.as_deref())
    .bind(&gate.user_id)
    .bind(assignment_key.as_deref())
    .bind(evidence_reader)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    let Some(row) = row else {
        return Ok(None);
    };
    let id: String = row.try_get("id").map_err(|_| storage())?;
    let version: i64 = row.try_get("version").map_err(|_| storage())?;
    let href = leave_href(source_id);
    Ok(Some(InboxTarget {
        href: Some(href.clone()),
        source: Some(json!({
            "kind": "leave",
            "id": id,
            "version": version,
            "href": href,
            "label": source_record_label("leave"),
        })),
    }))
}

async fn expense_target(
    c: &mut PgConnection,
    gate: &PeopleGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    if uses_enforced_layer(gate.mode) && !workforce_gate(c, gate, "people.expenses").await? {
        return Ok(None);
    }
    let assignment_key = resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?
    .map(|(id, _)| id);
    let can_account = match &assignment_key {
        Some(id) => role_allows(c, gate.company.as_str(), id, "expense.read").await?,
        None => false,
    };
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT x.id,x.branch_id,x.department_id FROM finance_expenses x LEFT JOIN hr_people p ON p.id=x.employee_id \
          WHERE x.id=$1 AND x.company_id=$2 \
          AND (iam_same_person(COALESCE(x.claimant_user_id,p.user_id),$3,x.company_id) \
            OR iam_same_person(x.created_by,$4,x.company_id) \
            OR iam_same_person(x.approved_by,$5,x.company_id) \
            OR {} \
            OR (($8::text IS NULL OR x.branch_id=$8) AND ($9::text IS NULL OR x.department_id=$9) AND $10=1 AND x.status IN ('in_review','approved','posted','reimbursed')) \
            OR (x.workflow_version=2 AND x.status IN ('in_review','approved','posted','reimbursed') AND EXISTS( \
              SELECT 1 FROM assignments access WHERE access.id=$11 AND access.company_id=x.company_id \
              AND iam_role_allows(access.role,access.company_id,'expense.review.read') AND access.revoked_at IS NULL \
              AND iam_assignment_active(access.id,$12) AND iam_principal_active(access.user_id,access.company_id,$12) \
              AND access.valid_from<=$12 AND access.valid_until>$12 \
              AND (access.scope_kind='holding' OR (access.scope_kind='branch' AND access.scope_node_id=x.branch_id) OR (access.scope_kind='department' AND access.scope_node_id=x.department_id)))))",
        participant_sql("expense", 6, 7),
    )))
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(&gate.user_id)
    .bind(&gate.user_id)
    .bind(&gate.user_id)
    .bind(&gate.user_id)
    .bind(assignment_key.as_deref())
    .bind(gate.branch_id.as_deref())
    .bind(gate.department_id.as_deref())
    .bind(if can_account { 1 } else { 0 })
    .bind(assignment_key.as_deref())
    .bind(gate.now)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    let Some(row) = row else {
        return Ok(None);
    };
    let branch_id: Option<String> = row.try_get("branch_id").map_err(|_| storage())?;
    let department_id: Option<String> = row.try_get("department_id").map_err(|_| storage())?;
    // The audit arm rechecks the row scope, exactly like the oracle.
    let can_read = selected_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        assignment_key.as_deref(),
        Some("expense.audit.read"),
        branch_id.as_deref(),
        department_id.as_deref(),
        gate.now,
    )
    .await?
        || sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM approval_decisions d JOIN approval_instances i ON i.id=d.instance_id \
          WHERE i.record_type='expense' AND i.record_id=$1 AND i.company_id=$2 \
          AND d.actor_user_id=$3 AND d.action IN ('approve','reject'))",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(&gate.user_id)
    .fetch_one(&mut *c)
    .await
    .map_err(|_| storage())?;
    if !can_read {
        return Ok(None);
    }
    Ok(Some(InboxTarget {
        href: Some(expense_href(source_id)),
        source: None,
    }))
}

fn clearance_permission(kind: &str) -> Option<&'static str> {
    match kind {
        "hr" => Some("people.manage"),
        "handover" | "equipment" => None,
        "directory" => Some("iam.directory.manage"),
        "payroll" => Some("payroll.prepare"),
        "finance" => Some("finance.write"),
        _ => None,
    }
}

fn clearance_kind_known(kind: &str) -> bool {
    matches!(
        kind,
        "hr" | "handover" | "equipment" | "directory" | "payroll" | "finance"
    )
}

struct ClearanceTask {
    case_id: String,
    clearance_type: String,
    assignee_id: Option<String>,
    branch_id: Option<String>,
    department_id: Option<String>,
}

async fn clearance_task(
    c: &mut PgConnection,
    company_id: &str,
    branch_id: Option<&str>,
    department_id: Option<&str>,
    source_id: &str,
) -> Result<Option<ClearanceTask>, BusinessError> {
    sqlx::query(
        "SELECT t.case_id,t.clearance_type,t.assignee_id,e.branch_id,e.department_id \
          FROM people_preparation_tasks t \
          JOIN people_lifecycle_cases c ON c.id=t.case_id AND c.company_id=t.company_id \
          JOIN hr_engagements e ON e.id=c.engagement_id AND e.company_id=c.company_id \
          JOIN hr_people p ON p.id=c.person_id AND p.company_id=c.company_id \
          WHERE t.id=$1 AND t.company_id=$2 AND t.clearance_type IS NOT NULL \
          AND ($3::text IS NULL OR e.branch_id=$3) \
          AND ($4::text IS NULL OR e.department_id=$4)",
    )
    .bind(source_id)
    .bind(company_id)
    .bind(branch_id)
    .bind(department_id)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?
    .map(|row| {
        let case_id: String = row.try_get("case_id").map_err(|_| storage())?;
        let clearance_type: String = row.try_get("clearance_type").map_err(|_| storage())?;
        let assignee_id: Option<String> = row.try_get("assignee_id").map_err(|_| storage())?;
        let branch_id: Option<String> = row.try_get("branch_id").map_err(|_| storage())?;
        let department_id: Option<String> =
            row.try_get("department_id").map_err(|_| storage())?;
        Ok(ClearanceTask {
            case_id,
            clearance_type,
            assignee_id,
            branch_id,
            department_id,
        })
    })
    .transpose()
}

async fn retirement_target(
    c: &mut PgConnection,
    gate: &PeopleGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    let Some(task) = clearance_task(
        c,
        gate.company.as_str(),
        gate.branch_id.as_deref(),
        gate.department_id.as_deref(),
        source_id,
    )
    .await?
    else {
        return Ok(None);
    };
    let assignment = resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?;
    let is_assignee = task.assignee_id.as_deref() == Some(gate.user_id.as_str());
    let hr = match &assignment {
        Some((id, _)) => role_allows(c, gate.company.as_str(), id, "people.manage").await?,
        None => false,
    };
    let kind_permission = clearance_permission(&task.clearance_type);
    let mut kind_allowed = false;
    if let Some(permission) = kind_permission {
        kind_allowed = match &assignment {
            Some((id, _)) => role_allows(c, gate.company.as_str(), id, permission).await?,
            None => false,
        };
    }
    if !(is_assignee || hr || kind_allowed) {
        return Ok(None);
    }
    if uses_enforced_layer(gate.mode) {
        let frame_set = gate.branch_id.is_some() || gate.department_id.is_some();
        let frame_blocks_kind = ["payroll", "finance", "directory"].contains(&task.clearance_type.as_str()) && frame_set;
        let kind_access = if !clearance_kind_known(&task.clearance_type) {
            false
        } else if frame_blocks_kind {
            false
        } else {
            selected_assignment(
                c,
                gate.company.as_str(),
                &gate.user_id,
                gate.assignment_id.as_deref(),
                kind_permission,
                task.branch_id.as_deref(),
                task.department_id.as_deref(),
                gate.now,
            )
            .await?
        };
        let policy = (is_assignee && kind_access)
            || hr
            || (kind_permission.is_some() && kind_access);
        if !policy {
            return Ok(None);
        }
        if !people_history(c, &gate.company, "people.lifecycle", gate.mode, gate.now).await? {
            return Ok(None);
        }
    }
    Ok(Some(InboxTarget {
        href: Some(retirement_case_href(&task.case_id)),
        source: None,
    }))
}

impl InboxTargetPort for PeopleInboxTargets {
    fn target<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        company_id: &'a str,
        user_id: &'a str,
        source_type: &'a str,
        source_id: &'a str,
        _source_version: i64,
    ) -> InboxTargetFuture<'a> {
        Box::pin(async move {
            let company = CompanyId::new(company_id).map_err(|_| storage())?;
            let mode = match self.enforcement {
                Some(mode) => mode,
                None => enforcement_mode()?,
            };
            let now: i64 = sqlx::query_scalar(
                "SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint",
            )
            .fetch_one(&mut *connection)
            .await
            .map_err(|_| storage())?;
            let gate = PeopleGate {
                company,
                user_id: user_id.to_owned(),
                assignment_id: self.assignment_id.clone(),
                branch_id: self.branch_id.clone(),
                department_id: self.department_id.clone(),
                mode,
                now,
            };
            match source_type {
                "roster_week" => roster_week_target(connection, &gate, source_id).await,
                "roster_request" => roster_request_target(connection, &gate, source_id).await,
                "roster_coverage" => roster_coverage_target(connection, &gate, source_id).await,
                "overtime" => overtime_target(connection, &gate, source_id).await,
                "leave" | "leave_cancellation" => {
                    leave_target(connection, &gate, source_id).await
                }
                "expense" => expense_target(connection, &gate, source_id).await,
                "retirement_clearance" => retirement_target(connection, &gate, source_id).await,
                _ if retained_inbox_destination(source_type, source_id).is_some() => {
                    Err(owner_unavailable())
                }
                _ => Err(unsupported()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_covers_only_the_people_slice() {
        for source_type in [
            "roster_week",
            "roster_request",
            "roster_coverage",
            "overtime",
            "leave",
            "leave_cancellation",
            "expense",
            "retirement_clearance",
        ] {
            assert!(is_people_inbox_target(source_type), "{source_type}");
        }
        for other in [
            "notice",
            "collaboration_action",
            "calendar_appointment",
            "crm_lead",
            "commerce_order",
            "invoice",
            "refund",
            "payroll_correction",
            "request",
            "task",
            "post",
            "support_ticket",
            "people",
            "future_source",
            "",
        ] {
            assert!(!is_people_inbox_target(other), "{other}");
        }
    }

    #[test]
    fn hrefs_encode_exactly_like_the_typescript_projection() {
        assert_eq!(
            roster_week_href("dep-1", "2026-11-16"),
            "/people/roster?department=dep-1&week=2026-11-16"
        );
        assert_eq!(
            roster_week_href("a/b c", "2026-11-16"),
            "/people/roster?department=a%2Fb%20c&week=2026-11-16"
        );
        assert_eq!(
            roster_request_href("dep-1", "2026-11-16", "req-1"),
            "/people/roster?department=dep-1&week=2026-11-16&view=requests&request=req-1"
        );
        assert_eq!(
            roster_request_href("dep-1", "2026-11-16", "a/b"),
            "/people/roster?department=dep-1&week=2026-11-16&view=requests&request=a%2Fb"
        );
        assert_eq!(overtime_href("ovt-1/2"), "/overtime/ovt-1/2");
        assert_eq!(leave_href("lea-1/2"), "/tasks/people_payroll/lea-1/2");
        assert_eq!(expense_href("exp-1/2"), "/expenses/exp-1/2");
        assert_eq!(
            retirement_case_href("case-1"),
            "/people/clearance/case-1"
        );
    }

    #[test]
    fn coverage_ids_parse_byte_identically_to_the_typescript_regex() {
        assert_eq!(
            parse_roster_coverage_id("dep-1:2026-11-16:person-1"),
            Some(("dep-1", "2026-11-16", "person-1"))
        );
        // Greedy department: colons stay in the first segment.
        assert_eq!(
            parse_roster_coverage_id("a:b:2026-11-16:p1"),
            Some(("a:b", "2026-11-16", "p1"))
        );
        // Empty department still matches, exactly like `(.*)`.
        assert_eq!(
            parse_roster_coverage_id(":2026-11-16:p1"),
            Some(("", "2026-11-16", "p1"))
        );
        for invalid in [
            "dep-1:2026-11-16:",
            "dep-1:2026-11-16",
            "dep-1:not-a-date:p1",
            "dep-1:2026-13-01:p1",
            "dep-1:2026-02-29:p1",
            "dep-1:2026-11-16:two:parts:extra",
            "",
        ] {
            assert_eq!(parse_roster_coverage_id(invalid), None, "{invalid}");
        }
        // Leap years validate as real dates.
        assert_eq!(
            parse_roster_coverage_id("dep-1:2024-02-29:p1"),
            Some(("dep-1", "2024-02-29", "p1"))
        );
    }

    #[test]
    fn monday_validation_matches_the_retained_week_rule() {
        assert!(is_roster_monday("2026-11-16"));
        assert!(is_roster_monday("2026-11-23"));
        for not_monday in ["2026-11-17", "2026-11-22", "2026-02-29", "not-a-date", ""] {
            assert!(!is_roster_monday(not_monday), "{not_monday}");
        }
    }

    #[test]
    fn bangkok_coverage_weeks_track_monday_boundaries() {
        // 2026-11-22T12:00:00+07:00: Sunday Bangkok -> Monday 2026-11-16.
        let sunday_noon = 1_795_323_600_000_i64;
        assert_eq!(
            bangkok_coverage_weeks(sunday_noon),
            Some(("2026-11-16".into(), "2026-11-23".into()))
        );
        // Monday 2026-11-16T00:00:00+07:00 stays on the same week.
        let monday_midnight = 1_794_762_000_000_i64;
        assert_eq!(
            bangkok_coverage_weeks(monday_midnight),
            Some(("2026-11-16".into(), "2026-11-23".into()))
        );
    }

    #[test]
    fn enforced_mode_selects_the_scoped_layer() {
        assert!(uses_enforced_layer(PackageEnforcementMode::Enforced));
        assert!(!uses_enforced_layer(PackageEnforcementMode::Disabled));
        assert!(!uses_enforced_layer(PackageEnforcementMode::Shadow));
    }

    #[test]
    fn people_kinds_keep_their_retained_labels() {
        assert_eq!(source_record_label("leave"), "Leave");
        assert_eq!(source_record_label("expense"), "Expense");
        assert_eq!(source_record_label("overtime"), "Overtime");
        assert_eq!(source_record_label("roster_week"), "Roster week");
        assert_eq!(source_record_label("retirement_clearance"), "Retirement clearance");
    }
}
