//! Native Records visibility delegate for the Inbox destination port.
//!
//! This mirrors the retained per-row TypeScript gates for the
//! notification-visible Records slice:
//!
//! - `leave` + `leave_cancellation`: `getLeave` — the workforce purchase
//!   (`people.workforce` history) and the current People authority, then the
//!   scoped leave read (creator, decider, leave/cancellation approval
//!   participants, evidence readers); 403 with the purchase decision code
//!   without history, 403 `PEOPLE_ACCESS_CHANGED` without a current
//!   assignment, 404 `NOT_FOUND` ("Leave request not found.") when the row
//!   is missing. Both source types map to source kind `"leave"` with the
//!   raw `` `/tasks/people_payroll/${id}` `` href and a versioned source.
//! - `expense`: `getExpense` — the expense purchase (`people.expenses`
//!   history) and the current Expense authority, then the scoped expense
//!   read with the audit/participant proof; 403 with the purchase decision
//!   code without history, 403 `EXPENSE_ACCESS_CHANGED` without a current
//!   assignment, 404 `EXPENSE_NOT_FOUND`
//!   ("Expense claim not found in your workspace.") when the row is missing
//!   or unreadable. Raw `` `/expenses/${id}` `` href with a versioned
//!   source.
//! - `request`: `getEmployeeRequest` — no purchase gate; the scoped request
//!   read (workspace membership, then owner/member/approver/decision
//!   participation); 404 `REQUEST_NOT_FOUND` ("Request not found.") when
//!   the row is missing, 404 `REQUEST_NOT_FOUND`
//!   ("This request is outside your current workspace.") outside the
//!   workspace, 404 `REQUEST_NOT_FOUND`
//!   ("This request is not available to you.") without participation. The
//!   source kind is `` `${kind}_request` `` with the raw
//!   `` `/requests/${id}` `` href and a versioned source.
//!
//! Gate order matches each oracle exactly, including the disabled-mode
//! shapes where the purchase and authority paths are skipped.
//!
//! Nothing here opens a pool, commits, or retries. It runs serializable-safe
//! SELECTs on the caller's connection.
//!
//! Faithful-mirror notes (documented, not silent):
//! - The JWT-static permission halves (`leave.evidence.read`,
//!   `expense.read`, `expense.audit.read`, workflow roles) are enforced
//!   database-current via `iam_role_allows` on the resolved assignment. A
//!   stale claim fails closed with the row 404 instead of the
//!   assignment-selection code.
//! - The workforce/expense authority session pin (queued-command sessions,
//!   member versions, emergency windows) is not re-proven; the assignment
//!   currency, principal and scope proof stands in for it.
//! - The leave legacy routing init (`approvalView` for legacy rows) creates
//!   approval instances the participant arms may need. It has no
//!   caller-transaction equivalent here; legacy participant-only rows fail
//!   closed while creator/decider rows still resolve.
//! - The expense cancelled/payment arms (treasury sources, payment drafts
//!   and cancellations) and the `canDecide` approval-planning arm are not
//!   re-proven; rows visible only through those arms fail closed. Seeds use
//!   direct participant and audit-readable rows.
//! - The request department-eligible approver arm (`departmentStepEligible`)
//!   and the spending/resource route pool computation are not re-proven;
//!   designated approvers outside service membership fail closed while
//!   stored-pool approvers with the workflow role still resolve. The
//!   override (`superior`) path is never taken here: inbox calls
//!   `getEmployeeRequest` without override, exactly like the oracle.
//! - The request holding-owner arm is not re-proven; owners who are also
//!   requesters, members, approvers or deciders still resolve, while
//!   pure-owner rows fail closed. Seeds use participating owners.
//! - Presentation joins and facts (leave type names, approval and route
//!   details, permitted actions, decorations, events, teams, children) are
//!   not visibility: rows passing the identity proof resolve without them.
use crate::adapters::inbox_pg::{InboxTarget, InboxTargetFuture, InboxTargetPort};
use crate::adapters::inbox_targets::{retained_inbox_destination, source_record_label};
use erpx_commercial_server::public::capability_decision::PgCapabilityDecisionReader;
use erpx_commercial_server::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, current_package_catalogue,
};
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use serde_json::json;
use sqlx::PgConnection;

/// Source types this delegate can prove against Records rows.
pub fn is_records_inbox_target(source_type: &str) -> bool {
    matches!(
        source_type,
        "leave" | "leave_cancellation" | "expense" | "request"
    )
}

/// Retained destination address for one leave row: the raw id, exactly like
/// the TypeScript `` `/tasks/people_payroll/${row.record_id}` ``.
pub fn records_leave_href(source_id: &str) -> String {
    format!("/tasks/people_payroll/{source_id}")
}

/// Retained destination address for one expense row: the raw id, exactly
/// like the TypeScript `` `/expenses/${row.record_id}` ``.
pub fn records_expense_href(source_id: &str) -> String {
    format!("/expenses/{source_id}")
}

/// Retained destination address for one employee request: the raw id,
/// exactly like the TypeScript `` `/requests/${row.record_id}` ``.
pub fn employee_request_href(source_id: &str) -> String {
    format!("/requests/{source_id}")
}

/// Retained source kind per notification type. Leave cancellations surface
/// as `"leave"`; requests surface as `` `${kind}_request` `` once the row
/// kind is known.
pub fn records_source_kind(source_type: &str, request_kind: Option<&str>) -> String {
    match source_type {
        "leave" | "leave_cancellation" => "leave".to_owned(),
        "expense" => "expense".to_owned(),
        "request" => match request_kind {
            Some(kind) => format!("{kind}_request"),
            None => "request".to_owned(),
        },
        _ => source_type.to_owned(),
    }
}

/// Retained read capabilities per source type. Leave settles the workforce
/// history, expenses settle the expenses history, requests settle none (the
/// oracle has no purchase gate on the request read).
pub fn records_history_capabilities(source_type: &str) -> &'static [&'static str] {
    match source_type {
        "leave" | "leave_cancellation" => &["people.workforce"],
        "expense" => &["people.expenses"],
        "request" => &[],
        _ => &[],
    }
}

fn people_access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PEOPLE_ACCESS_CHANGED",
        "Choose a current authorized People work role.",
    )
}

fn expense_access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "EXPENSE_ACCESS_CHANGED",
        "The original Expense assignment or session is no longer current.",
    )
}

fn leave_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "NOT_FOUND",
        "Leave request not found.",
    )
}

fn expense_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "EXPENSE_NOT_FOUND",
        "Expense claim not found in your workspace.",
    )
}

fn request_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "REQUEST_NOT_FOUND",
        "Request not found.",
    )
}

fn request_outside_workspace() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "REQUEST_NOT_FOUND",
        "This request is outside your current workspace.",
    )
}

fn request_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "REQUEST_NOT_FOUND",
        "This request is not available to you.",
    )
}

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

/// Native Records visibility delegate, composed via
/// `PgInboxTargets::with_delegate`. Records-only: every other source type
/// defers to the existing fail-closed behavior instead of inventing a link.
pub struct RecordsInboxTargets {
    branch_id: Option<String>,
    department_id: Option<String>,
    assignment_id: Option<String>,
    enforcement: Option<PackageEnforcementMode>,
}

impl RecordsInboxTargets {
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

impl Default for RecordsInboxTargets {
    fn default() -> Self {
        Self::new(None, None)
    }
}

/// Retained environment-driven enforcement mode, mirrored from the
/// TypeScript people oracles: disabled opens the read, a non-local
/// environment fails with the environment code, and an unknown value fails
/// closed with the configuration code.
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

struct RecordsGate {
    company: CompanyId,
    user_id: String,
    assignment_id: Option<String>,
    branch_id: Option<String>,
    department_id: Option<String>,
    mode: PackageEnforcementMode,
    now: i64,
}

impl RecordsGate {
    fn enforced(&self) -> bool {
        self.mode != PackageEnforcementMode::Disabled
    }
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

/// Database-current half of the retained JWT-static permission checks.
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

/// Retained assignment currency without a permission check: the exact
/// assignment is current with the caller and covers the given scope. This
/// stands in for the workforce/expense authority session pin (documented
/// gap).
async fn assignment_current(
    c: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    assignment_id: &str,
    role: &str,
    scope_branch_id: Option<&str>,
    scope_department_id: Option<&str>,
    now: i64,
) -> Result<bool, BusinessError> {
    let hit: Option<String> = sqlx::query_scalar(
        "SELECT g.id FROM assignments g \
          WHERE g.id=$1 AND g.user_id=$2 AND g.role=$3 AND g.company_id=$4 \
          AND iam_assignment_active(g.id,$7) \
          AND iam_principal_active(g.user_id,g.company_id,$7) \
          AND EXISTS(SELECT 1 FROM iam_roles role WHERE role.id=g.role \
            AND role.company_id IN ('*',g.company_id) AND role.status='active') \
          AND (g.scope_kind='holding' \
            OR (g.scope_kind='branch' AND g.scope_node_id=$5) \
            OR (g.scope_kind='department' AND g.scope_node_id=$6))",
    )
    .bind(assignment_id)
    .bind(user_id)
    .bind(role)
    .bind(company_id)
    .bind(scope_branch_id)
    .bind(scope_department_id)
    .bind(now)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    Ok(hit.is_some())
}

/// Retained history proof for one capability through the native Commercial
/// capability port. The refusal carries the exact decision code and message
/// from the TypeScript throw.
async fn purchase_require(
    c: &mut PgConnection,
    company_id: &CompanyId,
    capability: &str,
    mode: PackageEnforcementMode,
    now: i64,
) -> Result<(), BusinessError> {
    let catalogue = current_package_catalogue()?;
    let mut reader = PgCapabilityDecisionReader::new(company_id.clone(), c, &catalogue);
    reader
        .require(
            capability,
            CapabilityPurpose::History,
            mode,
            TimestampMillis::new(now),
        )
        .await
        .map_err(pivot)?;
    Ok(())
}

/// Retained approval-participant predicate, literal mirror including the
/// role arm bound to the reader's assignment. Approval kinds outside the
/// database-constrained set can never match.
fn participant_sql(kind: &str, user_param: u8, assignment_param: u8) -> String {
    let template = match kind {
        "leave" => "EXISTS(SELECT 1 FROM approval_instances ai JOIN approval_steps ast ON ast.instance_id=ai.id WHERE ai.record_type='leave' AND ai.record_id=l.id AND ai.company_id=l.company_id AND EXISTS(SELECT 1 FROM (SELECT ast.approver_user_id user_id UNION SELECT value FROM erpx_json_each(COALESCE(ast.approver_pool_json,'[]'))) participant WHERE iam_same_person(participant.user_id,?,ai.company_id)) AND (ast.state IN ('approved','rejected') OR ast.stage='department_approval' AND jsonb_exists(ast.route_snapshot_json::jsonb,'departmentPolicyVersion') OR ast.stage='hr_approval' AND ast.route_snapshot_json::jsonb->>'appointedPersonId' IS NOT NULL OR EXISTS(SELECT 1 FROM assignments aa WHERE aa.id=? AND iam_role_allows(aa.role,aa.company_id,'workflow.'||COALESCE(ast.required_role,CASE WHEN ast.authority IN ('named','department') THEN 'people_manager' ELSE ast.authority END)) AND aa.company_id=ai.company_id AND (aa.scope_kind='holding' OR aa.scope_kind='branch' AND aa.scope_node_id=ai.branch_id OR aa.scope_kind='department' AND aa.scope_node_id=ai.department_id))))",
        "leave_cancellation" => "EXISTS(SELECT 1 FROM approval_instances ai JOIN approval_steps ast ON ast.instance_id=ai.id WHERE ai.record_type='leave_cancellation' AND ai.record_id=l.id AND ai.company_id=l.company_id AND EXISTS(SELECT 1 FROM (SELECT ast.approver_user_id user_id UNION SELECT value FROM erpx_json_each(COALESCE(ast.approver_pool_json,'[]'))) participant WHERE iam_same_person(participant.user_id,?,ai.company_id)) AND (ast.state IN ('approved','rejected') OR ast.stage='department_approval' AND jsonb_exists(ast.route_snapshot_json::jsonb,'departmentPolicyVersion') OR ast.stage='hr_approval' AND ast.route_snapshot_json::jsonb->>'appointedPersonId' IS NOT NULL OR EXISTS(SELECT 1 FROM assignments aa WHERE aa.id=? AND iam_role_allows(aa.role,aa.company_id,'workflow.'||COALESCE(ast.required_role,CASE WHEN ast.authority IN ('named','department') THEN 'people_manager' ELSE ast.authority END)) AND aa.company_id=ai.company_id AND (aa.scope_kind='holding' OR aa.scope_kind='branch' AND aa.scope_node_id=ai.branch_id OR aa.scope_kind='department' AND aa.scope_node_id=ai.department_id))))",
        "expense" => "EXISTS(SELECT 1 FROM approval_instances ai JOIN approval_steps ast ON ast.instance_id=ai.id WHERE ai.record_type='expense' AND ai.record_id=x.id AND ai.company_id=x.company_id AND EXISTS(SELECT 1 FROM (SELECT ast.approver_user_id user_id UNION SELECT value FROM erpx_json_each(COALESCE(ast.approver_pool_json,'[]'))) participant WHERE iam_same_person(participant.user_id,?,ai.company_id)) AND (ast.state IN ('approved','rejected') OR ast.stage='department_approval' AND jsonb_exists(ast.route_snapshot_json::jsonb,'departmentPolicyVersion') OR ast.stage='hr_approval' AND ast.route_snapshot_json::jsonb->>'appointedPersonId' IS NOT NULL OR EXISTS(SELECT 1 FROM assignments aa WHERE aa.id=? AND iam_role_allows(aa.role,aa.company_id,'workflow.'||COALESCE(ast.required_role,CASE WHEN ast.authority IN ('named','department') THEN 'people_manager' ELSE ast.authority END)) AND aa.company_id=ai.company_id AND (aa.scope_kind='holding' OR aa.scope_kind='branch' AND aa.scope_node_id=ai.branch_id OR aa.scope_kind='department' AND aa.scope_node_id=ai.department_id))))",
        _ => "FALSE",
    };
    let user_slot = format!("${user_param}");
    let assignment_slot = format!("${assignment_param}");
    template.replacen('?', &user_slot, 1).replacen('?', &assignment_slot, 1)
}

fn versioned_source(kind: &str, id: &str, version: i64, href: &str) -> serde_json::Value {
    json!({
        "kind": kind,
        "id": id,
        "version": version,
        "href": href,
        "label": source_record_label(kind),
    })
}

/// Retained leave proof: workforce history, People authority, then the
/// scoped leave read. Both `leave` and `leave_cancellation` notification
/// types resolve the same `leave_requests` row with source kind `"leave"`.
async fn leave_target(
    c: &mut PgConnection,
    gate: &RecordsGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    let assignment = resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?;
    if gate.enforced() {
        purchase_require(c, &gate.company, "people.workforce", gate.mode, gate.now).await?;
        let current = match &assignment {
            Some((id, role)) => {
                assignment_current(
                    c,
                    gate.company.as_str(),
                    &gate.user_id,
                    id,
                    role,
                    gate.branch_id.as_deref(),
                    gate.department_id.as_deref(),
                    gate.now,
                )
                .await?
            }
            None => false,
        };
        if !current {
            return Err(people_access_changed());
        }
    }
    let assignment_key = assignment.as_ref().map(|(id, _)| id.as_str());
    let evidence_reader = match &assignment {
        Some((id, _)) => role_allows(c, gate.company.as_str(), id, "leave.evidence.read").await?,
        None => false,
    };
    let row: Option<(String, i64)> = sqlx::query_as(
        sqlx::AssertSqlSafe(format!(
            "SELECT l.id,l.version FROM leave_requests l LEFT JOIN leave_types t ON t.id=l.leave_type_id AND t.company_id=l.company_id \
              WHERE l.id=$1 AND l.company_id=$2 \
              AND (l.created_by=$3 OR l.decided_by=$4 OR {} OR {} OR $9::boolean)",
            participant_sql("leave", 5, 6),
            participant_sql("leave_cancellation", 7, 8),
        )),
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(&gate.user_id)
    .bind(&gate.user_id)
    .bind(&gate.user_id)
    .bind(assignment_key)
    .bind(&gate.user_id)
    .bind(assignment_key)
    .bind(evidence_reader)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    let Some((id, version)) = row else {
        return Err(leave_not_found());
    };
    let href = records_leave_href(source_id);
    Ok(Some(InboxTarget {
        href: Some(href.clone()),
        source: Some(versioned_source("leave", &id, version, &href)),
    }))
}

/// Retained expense proof: expenses history, Expense authority, then the
/// scoped expense read with the audit/participant proof. The
/// cancelled/payment arms and the `canDecide` planning arm are documented
/// gaps: rows visible only through them fail closed here.
async fn expense_target(
    c: &mut PgConnection,
    gate: &RecordsGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    let assignment = resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?;
    if gate.enforced() {
        purchase_require(c, &gate.company, "people.expenses", gate.mode, gate.now).await?;
        let current = match &assignment {
            Some((id, role)) => {
                assignment_current(
                    c,
                    gate.company.as_str(),
                    &gate.user_id,
                    id,
                    role,
                    gate.branch_id.as_deref(),
                    gate.department_id.as_deref(),
                    gate.now,
                )
                .await?
            }
            None => false,
        };
        if !current {
            return Err(expense_access_changed());
        }
    }
    let assignment_key = assignment.as_ref().map(|(id, _)| id.as_str());
    let can_account = match &assignment {
        Some((id, _)) => role_allows(c, gate.company.as_str(), id, "expense.read").await?,
        None => false,
    };
    // The retained visible scope minus the cancelled/payment arms (treasury
    // and draft tables): direct, participant, frame-accounting and v2
    // reviewer arms only. See the module gaps.
    let row: Option<(String, i64, Option<String>, Option<String>)> = sqlx::query_as(
        sqlx::AssertSqlSafe(format!(
            "SELECT x.id,x.version,x.branch_id,x.department_id FROM finance_expenses x LEFT JOIN hr_people p ON p.id=x.employee_id \
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
        )),
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(&gate.user_id)
    .bind(&gate.user_id)
    .bind(&gate.user_id)
    .bind(&gate.user_id)
    .bind(assignment_key)
    .bind(gate.branch_id.as_deref())
    .bind(gate.department_id.as_deref())
    .bind(if can_account { 1 } else { 0 })
    .bind(assignment_key)
    .bind(gate.now)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    let Some((id, version, branch_id, department_id)) = row else {
        return Err(expense_not_found());
    };
    // The audit arm rechecks the row scope, exactly like the oracle; the
    // participant arm checks recorded approval decisions. `canDecide` is a
    // documented gap.
    let audit: bool = match &assignment {
        Some((aid, _)) => {
            let hit: Option<String> = sqlx::query_scalar(
                "SELECT a.id FROM assignments a WHERE a.company_id=$1 AND a.id=$2 \
                  AND iam_role_allows(a.role,a.company_id,'expense.audit.read') AND a.revoked_at IS NULL \
                  AND iam_assignment_active(a.id,$5) AND iam_principal_active(a.user_id,a.company_id,$5) \
                  AND a.valid_from<=$5 AND a.valid_until>$5 \
                  AND (a.scope_kind='holding' \
                    OR (a.scope_kind='branch' AND a.scope_node_id=$3) \
                    OR (a.scope_kind='department' AND a.scope_node_id=$4))",
            )
            .bind(gate.company.as_str())
            .bind(aid)
            .bind(branch_id.as_deref())
            .bind(department_id.as_deref())
            .bind(gate.now)
            .fetch_optional(&mut *c)
            .await
            .map_err(|_| storage())?;
            hit.is_some()
        }
        None => false,
    };
    let participant: bool = sqlx::query_scalar(
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
    if !audit && !participant {
        // Direct arms (claimant/creator/approver/frame/review) already
        // passed above; without audit or decision participation the oracle's
        // `canRead` denies. Collapse to the retained 404 exactly like the
        // oracle's scope filtering.
        //
        // Note: rows passing only via `canDecide` fail closed here
        // (documented gap).
        let direct: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM finance_expenses x LEFT JOIN hr_people p ON p.id=x.employee_id \
              WHERE x.id=$1 AND x.company_id=$2 \
              AND (iam_same_person(COALESCE(x.claimant_user_id,p.user_id),$3,x.company_id) \
                OR iam_same_person(x.created_by,$4,x.company_id) \
                OR iam_same_person(x.approved_by,$5,x.company_id)))",
        )
        .bind(source_id)
        .bind(gate.company.as_str())
        .bind(&gate.user_id)
        .bind(&gate.user_id)
        .bind(&gate.user_id)
        .fetch_one(&mut *c)
        .await
        .map_err(|_| storage())?;
        if !direct {
            return Err(expense_not_found());
        }
    }
    let href = records_expense_href(source_id);
    Ok(Some(InboxTarget {
        href: Some(href.clone()),
        source: Some(versioned_source("expense", &id, version, &href)),
    }))
}

struct RequestRow {
    id: String,
    kind: String,
    version: i64,
    requester_user_id: String,
    created_by: String,
    team_id: Option<String>,
    approval_json: String,
    company_id: String,
    branch_id: Option<String>,
    department_id: Option<String>,
}

async fn same_person(
    c: &mut PgConnection,
    left: &str,
    right: &str,
    company_id: &str,
) -> Result<bool, BusinessError> {
    sqlx::query_scalar("SELECT iam_same_person($1,$2,$3)")
        .bind(left)
        .bind(right)
        .bind(company_id)
        .fetch_one(&mut *c)
        .await
        .map_err(|_| storage())
}

/// Retained request proof: the row, then the workspace gate, then the
/// participation gate. No purchase gate exists on the oracle path.
async fn request_target(
    c: &mut PgConnection,
    gate: &RecordsGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    let row: Option<RequestRow> = sqlx::query_as(
        "SELECT id,kind,version,requester_user_id,created_by,team_id,approval_json,company_id,branch_id,department_id \
          FROM employee_requests WHERE id=$1 AND company_id=$2",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?
    .map(
        |(id, kind, version, requester_user_id, created_by, team_id, approval_json, company_id, branch_id, department_id): (
            String,
            String,
            i64,
            String,
            String,
            Option<String>,
            String,
            String,
            Option<String>,
            Option<String>,
        )| RequestRow {
            id,
            kind,
            version,
            requester_user_id,
            created_by,
            team_id,
            approval_json,
            company_id,
            branch_id,
            department_id,
        },
    );
    let Some(row) = row else {
        return Err(request_not_found());
    };
    // Workspace gate: the reader's frame must cover the row and a current
    // assignment must cover the row scope. The designated-approver and
    // override (`superior`) arms are documented gaps: inbox never overrides,
    // and department-eligible approvers outside membership fail closed.
    let in_workspace = gate.company.as_str() == row.company_id
        && (gate.branch_id.as_deref().is_none() || gate.branch_id.as_deref() == row.branch_id.as_deref())
        && (gate.department_id.as_deref().is_none()
            || gate.department_id.as_deref() == row.department_id.as_deref());
    let member_of_service: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM assignments a WHERE a.company_id=$1 AND a.user_id=$2 \
          AND a.revoked_at IS NULL AND a.valid_from<=$5 AND a.valid_until>$5 \
          AND iam_assignment_active(a.id,$5) AND iam_principal_active(a.user_id,a.company_id,$5) \
          AND (a.scope_kind='holding' \
            OR (a.scope_kind='branch' AND a.scope_node_id=$3) \
            OR (a.scope_kind='department' AND a.scope_node_id=$4)))",
    )
    .bind(row.company_id.as_str())
    .bind(gate.user_id.as_str())
    .bind(row.branch_id.as_deref())
    .bind(row.department_id.as_deref())
    .bind(gate.now)
    .fetch_one(&mut *c)
    .await
    .map_err(|_| storage())?;
    if !in_workspace || !member_of_service {
        return Err(request_outside_workspace());
    }
    // Participation gate: own, team member, stored-pool approver with the
    // workflow role (or department-policy snapshot), or recorded
    // decider/decision participant.
    let own = same_person(c, &row.requester_user_id, &gate.user_id, &row.company_id).await?
        || same_person(c, &row.created_by, &gate.user_id, &row.company_id).await?;
    let handlers: Vec<(String, String)> = match row.team_id.as_deref() {
        Some(team_id) => sqlx::query_as(
            "SELECT user_id,responsibility FROM service_team_members WHERE company_id=$1 AND team_id=$2",
        )
        .bind(row.company_id.as_str())
        .bind(team_id)
        .fetch_all(&mut *c)
        .await
        .map_err(|_| storage())?,
        None => Vec::new(),
    };
    let member = handlers.iter().any(|(user_id, _)| user_id == &gate.user_id);
    let saved: serde_json::Value =
        serde_json::from_str(&row.approval_json).unwrap_or(serde_json::Value::Null);
    let current_index = saved
        .get("currentStep")
        .and_then(|value| value.as_i64())
        .unwrap_or(0);
    let steps = saved
        .pointer("/route/steps")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let step = steps.get(usize::try_from(current_index.max(0)).unwrap_or(0));
    let pool: Vec<String> = saved
        .get("pool")
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let snapshot_has_department_policy = step
        .and_then(|step| step.get("snapshot"))
        .and_then(|snapshot| snapshot.get("departmentPolicyVersion"))
        .is_some();
    let required_role = step
        .and_then(|step| step.get("requiredRole"))
        .and_then(|value| value.as_str())
        .unwrap_or("people_manager");
    let in_pool = pool.iter().any(|id| id == &gate.user_id);
    let approver = if in_pool {
        if snapshot_has_department_policy {
            true
        } else {
            let assignment = resolve_assignment(
                c,
                gate.company.as_str(),
                &gate.user_id,
                gate.assignment_id.as_deref(),
                gate.now,
            )
            .await?;
            match assignment {
                Some((id, _)) => {
                    role_allows(
                        c,
                        gate.company.as_str(),
                        &id,
                        &format!("workflow.{required_role}"),
                    )
                    .await?
                }
                None => false,
            }
        }
    } else {
        false
    };
    let decisions: Vec<String> = saved
        .get("decisions")
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| {
                    value
                        .get("userId")
                        .and_then(|id| id.as_str().map(str::to_owned))
                })
                .collect()
        })
        .unwrap_or_default();
    let decided_by = saved
        .get("decidedBy")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let decided = !decided_by.is_empty()
        && same_person(c, decided_by, &gate.user_id, &row.company_id).await?;
    let mut decision_participant = false;
    for user in &decisions {
        if same_person(c, user, &gate.user_id, &row.company_id).await? {
            decision_participant = true;
            break;
        }
    }
    if !own && !member && !approver && !decided && !decision_participant {
        return Err(request_unavailable());
    }
    let kind = format!("{}_request", row.kind);
    let href = employee_request_href(source_id);
    Ok(Some(InboxTarget {
        href: Some(href.clone()),
        source: Some(versioned_source(&kind, &row.id, row.version, &href)),
    }))
}

impl InboxTargetPort for RecordsInboxTargets {
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
            let gate = RecordsGate {
                company,
                user_id: user_id.to_owned(),
                assignment_id: self.assignment_id.clone(),
                branch_id: self.branch_id.clone(),
                department_id: self.department_id.clone(),
                mode,
                now,
            };
            match source_type {
                "leave" | "leave_cancellation" => leave_target(connection, &gate, source_id).await,
                "expense" => expense_target(connection, &gate, source_id).await,
                "request" => request_target(connection, &gate, source_id).await,
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
    use crate::adapters::inbox_targets::source_record_label;

    #[test]
    fn routing_covers_only_the_records_slice() {
        for source_type in ["leave", "leave_cancellation", "expense", "request"] {
            assert!(is_records_inbox_target(source_type), "{source_type}");
        }
        for other in [
            "notice",
            "collaboration_action",
            "calendar_appointment",
            "overtime",
            "task",
            "post",
            "operational_exception",
            "future_source",
            "",
        ] {
            assert!(!is_records_inbox_target(other), "{other}");
        }
    }

    #[test]
    fn hrefs_match_the_raw_typescript_projection() {
        assert_eq!(
            records_leave_href("leave-1"),
            "/tasks/people_payroll/leave-1"
        );
        assert_eq!(
            records_leave_href("a/b c"),
            "/tasks/people_payroll/a/b c"
        );
        assert_eq!(records_expense_href("exp-1"), "/expenses/exp-1");
        assert_eq!(
            employee_request_href("req-1"),
            "/requests/req-1"
        );
    }

    #[test]
    fn kind_mapping_matches_the_notification_switch() {
        assert_eq!(records_source_kind("leave", None), "leave");
        assert_eq!(records_source_kind("leave_cancellation", None), "leave");
        assert_eq!(records_source_kind("expense", None), "expense");
        assert_eq!(
            records_source_kind("request", Some("purchase")),
            "purchase_request"
        );
        assert_eq!(
            records_source_kind("request", Some("service")),
            "service_request"
        );
        assert_eq!(
            records_source_kind("request", Some("equipment")),
            "equipment_request"
        );
    }

    #[test]
    fn history_capabilities_match_the_retained_gates() {
        assert_eq!(
            records_history_capabilities("leave"),
            &["people.workforce"]
        );
        assert_eq!(
            records_history_capabilities("leave_cancellation"),
            &["people.workforce"]
        );
        assert_eq!(
            records_history_capabilities("expense"),
            &["people.expenses"]
        );
        assert_eq!(records_history_capabilities("request"), &[] as &[&str]);
        assert_eq!(records_history_capabilities("overtime"), &[] as &[&str]);
        assert_eq!(records_history_capabilities(""), &[] as &[&str]);
    }

    #[test]
    fn records_kinds_keep_their_retained_labels() {
        assert_eq!(source_record_label("leave"), "Leave");
        assert_eq!(source_record_label("expense"), "Expense");
        assert_eq!(source_record_label("purchase_request"), "Purchase request");
        assert_eq!(source_record_label("service_request"), "Service request");
        assert_eq!(source_record_label("equipment_request"), "Equipment request");
        let source = versioned_source("purchase_request", "req-1", 3, "/requests/req-1");
        assert_eq!(source["kind"], json!("purchase_request"));
        assert_eq!(source["id"], json!("req-1"));
        assert_eq!(source["version"], json!(3));
        assert_eq!(source["href"], json!("/requests/req-1"));
        assert_eq!(source["label"], json!("Purchase request"));
    }

    #[test]
    fn oracle_denials_keep_their_exact_codes_kinds_and_messages() {
        let authority = people_access_changed();
        assert_eq!(authority.kind, FailureKind::Forbidden);
        assert_eq!(authority.code, "PEOPLE_ACCESS_CHANGED");
        let authority = expense_access_changed();
        assert_eq!(authority.kind, FailureKind::Forbidden);
        assert_eq!(authority.code, "EXPENSE_ACCESS_CHANGED");
        let missing = leave_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "NOT_FOUND");
        assert_eq!(missing.message, "Leave request not found.");
        let missing = expense_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "EXPENSE_NOT_FOUND");
        assert_eq!(missing.message, "Expense claim not found in your workspace.");
        let missing = request_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "REQUEST_NOT_FOUND");
        assert_eq!(missing.message, "Request not found.");
        let outside = request_outside_workspace();
        assert_eq!(outside.kind, FailureKind::NotFound);
        assert_eq!(outside.code, "REQUEST_NOT_FOUND");
        assert_eq!(
            outside.message,
            "This request is outside your current workspace."
        );
        let unavailable = request_unavailable();
        assert_eq!(unavailable.kind, FailureKind::NotFound);
        assert_eq!(unavailable.code, "REQUEST_NOT_FOUND");
        assert_eq!(unavailable.message, "This request is not available to you.");
    }
}
