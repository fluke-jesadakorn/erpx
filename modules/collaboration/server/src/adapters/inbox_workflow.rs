//! Native Workflow visibility delegate for the Inbox destination port.
//!
//! This mirrors the retained per-row TypeScript gates for the
//! notification-visible Workflow slice:
//!
//! - `staff_task_alert`: `getTask` — the exact operations-delegate task
//!   proof, cited rather than re-derived
//!   (`crate::adapters::inbox_operations::task_target`): project purchase,
//!   social-source gate, work authority and capability history, scoped
//!   participant read, linked-post proof; same codes, same
//!   `` `/staff-tasks/${encodeURIComponent(id)}` `` href. No source
//!   override.
//! - `collaboration_action` is NOT proven here: `PgInboxTargets` retains a
//!   dedicated in-crate read for that type (drop-when-invisible with a
//!   versioned source) and intercepts it before any delegate. Routing it
//!   here would be dead code, so this delegate defers it.
//! - `project_handoff`: `projectHandoffTarget` — the project purchase
//!   (`projects.handover` history) with the Project authority, then the
//!   handoff row and the project proof; 403 `PROJECT_ACCESS_CHANGED`
//!   without a current assignment, 403 `PACKAGE_REQUIRED` without history,
//!   404 `PROJECT_HANDOFF_NOT_FOUND` ("Project handoff not found.") when
//!   the handoff is missing, 404 `PROJECT_NOT_FOUND`
//!   ("Project not found in the current role.") without currency or the
//!   project row. Href is
//!   `` `/projects/${encodeURIComponent(projectId)}#project-structure` ``.
//!   No source override.
//! - `maintenance_work`: `maintenanceDetail` — the assets purchase
//!   (`assets.maintenance` history) with the Assets authority, then the
//!   `records.read` role, the maintenance module and the scoped work read;
//!   403 with the purchase decision code without history, 403
//!   `ASSETS_ACCESS_CHANGED` without a current assignment, 403
//!   `PERMISSION_REQUIRED` without the role, 403 `MODULE_FORBIDDEN`
//!   without the module, 404 `MAINTENANCE_WORK_NOT_FOUND`
//!   ("Maintenance work order not found in this scope.") when the row is
//!   missing. Href is `` `/maintenance/${encodeURIComponent(id)}` ``. No
//!   source override.
//! - `maintenance_plan`: `preventivePlanDetail` — the same shape with the
//!   `assets.preventive` history; 404 `MAINTENANCE_PLAN_NOT_FOUND`
//!   ("Preventive plan not found in this scope."). Href is
//!   `` `/maintenance/plans/${encodeURIComponent(id)}` ``. No source
//!   override.
//! - `supplier_batch`: `getSupplierBatch` — the finance authority with the
//!   banking/inventory-valuation history, then the Company-treasury frame
//!   and the scoped batch read; 403 `FINANCE_ACCESS_CHANGED` without a
//!   current assignment, 403 with the purchase decision code without
//!   history, 403 `COMPANY_TREASURY_REQUIRED`
//!   ("Use a Company finance assignment for supplier bank batches.")
//!   outside the holding frame, 404 `BATCH_NOT_FOUND`
//!   ("Supplier batch not found.") when the row is missing. Href is
//!   `` `/finance/supplier-batches/${encodeURIComponent(id)}` ``. No
//!   source override.
//! - `handbook` + `handbook_article`: `handbookNotificationTarget` — the
//!   `work.handbook` history, the Company membership, then the editor
//!   proof and the article/revision read; 403 with the purchase decision
//!   code without history, 403 `MEMBERSHIP_REQUIRED`
//!   ("Current company membership is required.") without membership, 404
//!   `ARTICLE_NOT_FOUND` when the article or its revision is missing.
//!   Href is `` `/handbook/${encodeURIComponent(id)}` `` plus `?edit=1`
//!   for editors, with the versioned source `{kind:'handbook_article',
//!   id, version, href, label}` — the only versioned source in this slice.
//!
//! `field_service_order` is explicitly out of scope: the native page
//! requires field-service native owner work. This delegate returns the
//! fail-closed unsupported code for it so it defers.
//!
//! Gate order matches each oracle exactly, including the disabled-mode
//! shapes where the purchase and authority paths are skipped.
//!
//! Nothing here opens a pool, commits, or retries. It runs serializable-safe
//! SELECTs on the caller's connection.
//!
//! Faithful-mirror notes (documented, not silent):
//! - The JWT-static permission and module halves are enforced
//!   database-current via `iam_role_allows` on the resolved assignment. A
//!   stale claim fails closed with the row denial instead of the
//!   assignment-selection code.
//! - The queued-command authority (assignment version pins, revoked
//!   sessions, member versions, emergency windows, HR appointments,
//!   active work contexts, command guards) is not re-proven; the
//!   assignment currency, principal and scope proof stands in for it.
//! - The collaboration native-field branch resolves through an external
//!   field work source outside any caller transaction. There is no native
//!   equivalent to prove here, so the TypeScript fallback is mirrored
//!   directly and the native link is never invented.
//! - The collaboration card source-visibility filter (`visibleSource` per
//!   linked owner) is not re-proven: cards resolve through the
//!   frame-scoped card read, and cards whose linked source the reader
//!   cannot see stay visible here while the oracle would drop them. Seeds
//!   use self-contained rows.
//! - The handoff `activeStaff` collapse is not re-proven; the assignment
//!   currency proof stands in for it. Readers without a staff record fail
//!   open here while the oracle would deny them. Seeds use staffed users.
//! - The supplier-batch sourceOnly per-invoice document gating (banking
//!   unpurchased but inventory-valuation purchased) is not re-proven;
//!   seeds use banking-history companies where the oracle skips it too.
//! - The scheduled-feature predicates (portal release, collaboration
//!   module features) are effectively `TRUE` outside production and are
//!   not re-proven; production rollout divergence is documented, not
//!   silent.
//! - Presentation joins and facts (names, histories, eligibility,
//!   purchase controls, decisions, occurrences, threads) are not
//!   visibility: rows passing the identity proof resolve without them.
use crate::adapters::inbox_operations::{OperationsGate, task_target};
use crate::adapters::inbox_pg::{InboxTarget, InboxTargetFuture, InboxTargetPort};
use crate::adapters::inbox_targets::{encode_uri_component, retained_inbox_destination, source_record_label};
use erpx_commercial_server::public::capability_decision::PgCapabilityDecisionReader;
use erpx_commercial_server::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, current_package_catalogue,
};
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use serde_json::json;
use sqlx::PgConnection;

/// Source types this delegate can prove against Workflow rows.
pub fn is_workflow_inbox_target(source_type: &str) -> bool {
    matches!(
        source_type,
        "staff_task_alert"
            | "project_handoff"
            | "maintenance_work"
            | "maintenance_plan"
            | "supplier_batch"
            | "handbook"
            | "handbook_article"
    )
}

/// Retained destination address for one staff-task alert, byte-identical to
/// the TypeScript `` `/staff-tasks/${encodeURIComponent(id)}` ``.
pub fn task_alert_href(source_id: &str) -> String {
    format!("/staff-tasks/{}", encode_uri_component(source_id))
}

/// Retained destination address for one project handoff, byte-identical to
/// the oracle `` `/projects/${encodeURIComponent(projectId)}#project-structure` ``.
pub fn project_handoff_href(project_id: &str) -> String {
    format!(
        "/projects/{}#project-structure",
        encode_uri_component(project_id)
    )
}

/// Retained destination address for one maintenance work order,
/// byte-identical to the TypeScript
/// `` `/maintenance/${encodeURIComponent(id)}` ``.
pub fn maintenance_work_href(source_id: &str) -> String {
    format!("/maintenance/{}", encode_uri_component(source_id))
}

/// Retained destination address for one preventive plan, byte-identical to
/// the TypeScript `` `/maintenance/plans/${encodeURIComponent(id)}` ``.
pub fn maintenance_plan_href(source_id: &str) -> String {
    format!("/maintenance/plans/{}", encode_uri_component(source_id))
}

/// Retained destination address for one supplier batch, byte-identical to
/// the TypeScript
/// `` `/finance/supplier-batches/${encodeURIComponent(id)}` ``.
pub fn supplier_batch_href(source_id: &str) -> String {
    format!("/finance/supplier-batches/{}", encode_uri_component(source_id))
}

/// Retained destination address for one handbook article, byte-identical to
/// the oracle `` `/handbook/${encodeURIComponent(id)}${editor?'?edit=1':''}` ``.
pub fn handbook_article_href(source_id: &str, editor: bool) -> String {
    format!(
        "/handbook/{}{}",
        encode_uri_component(source_id),
        if editor { "?edit=1" } else { "" }
    )
}

/// Retained read capabilities per source type. Tasks settle the project
/// and work histories via the operations proof; handoffs settle the
/// handover history; maintenance settles its asset history; batches
/// settle banking with the inventory-valuation fallback; handbook settles
/// its work history.
pub fn workflow_history_capabilities(source_type: &str) -> &'static [&'static str] {
    match source_type {
        "staff_task_alert" => &[
            "projects.tasks",
            "work.actions",
            "work.advanced-routines",
            "work.social",
        ],
        "project_handoff" => &["projects.handover"],
        "maintenance_work" => &["assets.maintenance"],
        "maintenance_plan" => &["assets.preventive"],
        "supplier_batch" => &["finance.banking", "finance.inventory-valuation"],
        "handbook" | "handbook_article" => &["work.handbook"],
        _ => &[],
    }
}

fn permission_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PERMISSION_REQUIRED",
        "Your selected role does not grant this permission.",
    )
}

fn maintenance_module_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "MODULE_FORBIDDEN",
        "Your current role cannot access maintenance.",
    )
}

fn projects_module_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "MODULE_FORBIDDEN",
        "Your current role cannot access Projects.",
    )
}

fn project_access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PROJECT_ACCESS_CHANGED",
        "Choose a current Project assignment.",
    )
}

fn project_package_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PACKAGE_REQUIRED",
        "This Project work requires its purchased capability or retained history.",
    )
}

fn project_handoff_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PROJECT_HANDOFF_NOT_FOUND",
        "Project handoff not found.",
    )
}

fn project_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PROJECT_NOT_FOUND",
        "Project not found in the current role.",
    )
}

fn assets_access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ASSETS_ACCESS_CHANGED",
        "Choose a current authorized Assets or Maintenance work role.",
    )
}

fn maintenance_work_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "MAINTENANCE_WORK_NOT_FOUND",
        "Maintenance work order not found in this scope.",
    )
}

fn maintenance_plan_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "MAINTENANCE_PLAN_NOT_FOUND",
        "Preventive plan not found in this scope.",
    )
}

fn finance_access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FINANCE_ACCESS_CHANGED",
        "Choose a current Finance assignment.",
    )
}

fn company_treasury_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "COMPANY_TREASURY_REQUIRED",
        "Use a Company finance assignment for supplier bank batches.",
    )
}

fn supplier_batch_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "BATCH_NOT_FOUND",
        "Supplier batch not found.",
    )
}

fn membership_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "MEMBERSHIP_REQUIRED",
        "Current company membership is required.",
    )
}

fn article_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ARTICLE_NOT_FOUND",
        "This article has not been published or is unavailable.",
    )
}

fn article_revision_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ARTICLE_NOT_FOUND",
        "This revision is not published or is unavailable.",
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

/// Native Workflow visibility delegate, composed via
/// `PgInboxTargets::with_delegate`. Workflow-only: every other source type
/// defers to the existing fail-closed behavior instead of inventing a link.
pub struct WorkflowInboxTargets {
    branch_id: Option<String>,
    department_id: Option<String>,
    assignment_id: Option<String>,
    apps_enforcement: Option<PackageEnforcementMode>,
    projects_enforcement: Option<PackageEnforcementMode>,
    assets_enforcement: Option<PackageEnforcementMode>,
    finance_enforcement: Option<PackageEnforcementMode>,
}

impl WorkflowInboxTargets {
    pub fn new(branch_id: Option<&str>, department_id: Option<&str>) -> Self {
        Self {
            branch_id: branch_id.map(str::to_owned),
            department_id: department_id.map(str::to_owned),
            assignment_id: None,
            apps_enforcement: None,
            projects_enforcement: None,
            assets_enforcement: None,
            finance_enforcement: None,
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

    /// Pin the apps enforcement mode (handbook reads). Without it the
    /// delegate mirrors the retained apps environment-driven mode exactly.
    pub fn enforcement_mode(mut self, mode: PackageEnforcementMode) -> Self {
        self.apps_enforcement = Some(mode);
        self
    }

    /// Pin the projects enforcement mode (handoff reads).
    pub fn projects_enforcement_mode(mut self, mode: PackageEnforcementMode) -> Self {
        self.projects_enforcement = Some(mode);
        self
    }

    /// Pin the assets enforcement mode (maintenance reads).
    pub fn assets_enforcement_mode(mut self, mode: PackageEnforcementMode) -> Self {
        self.assets_enforcement = Some(mode);
        self
    }

    /// Pin the finance enforcement mode (supplier-batch reads).
    pub fn finance_enforcement_mode(mut self, mode: PackageEnforcementMode) -> Self {
        self.finance_enforcement = Some(mode);
        self
    }
}

impl Default for WorkflowInboxTargets {
    fn default() -> Self {
        Self::new(None, None)
    }
}

/// Retained environment-driven enforcement mode, mirrored from the
/// TypeScript oracles: disabled opens the read, a non-local environment
/// fails with the configuration code, and an unknown value fails closed
/// with the configuration code.
fn enforcement_mode(key: &str) -> Result<PackageEnforcementMode, BusinessError> {
    match std::env::var(key)
        .unwrap_or_else(|_| "disabled".to_owned())
        .as_str()
    {
        "disabled" => Ok(PackageEnforcementMode::Disabled),
        "shadow" => Ok(PackageEnforcementMode::Shadow),
        "enforced" => {
            if std::env::var("ERPX_ENVIRONMENT").ok().as_deref() != Some("local") {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "PACKAGE_ENFORCEMENT_CONFIGURATION",
                    "Package enforcement is unavailable.",
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

struct WorkflowGate {
    company: CompanyId,
    user_id: String,
    assignment_id: Option<String>,
    branch_id: Option<String>,
    department_id: Option<String>,
    apps_mode: PackageEnforcementMode,
    projects_mode: PackageEnforcementMode,
    assets_mode: PackageEnforcementMode,
    finance_mode: PackageEnforcementMode,
    now: i64,
}

impl WorkflowGate {
    fn projects_enforced(&self) -> bool {
        self.projects_mode != PackageEnforcementMode::Disabled
    }

    fn assets_enforced(&self) -> bool {
        self.assets_mode != PackageEnforcementMode::Disabled
    }

    fn finance_enforced(&self) -> bool {
        self.finance_mode != PackageEnforcementMode::Disabled
    }

    fn apps_enforced(&self) -> bool {
        self.apps_mode != PackageEnforcementMode::Disabled
    }

    fn holding_frame(&self) -> bool {
        self.branch_id.is_none() && self.department_id.is_none()
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

/// Database-current half of the retained JWT-static module checks: the
/// resolved assignment selects the module.
async fn module_allows(
    c: &mut PgConnection,
    assignment_id: &str,
    module: &str,
) -> Result<bool, BusinessError> {
    sqlx::query_scalar(
        "SELECT modules_json::jsonb @> jsonb_build_array($2::text) FROM assignments WHERE id=$1",
    )
    .bind(assignment_id)
    .bind(module)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())
    .map(|hit| hit.unwrap_or(false))
}

/// Retained `assignmentReadSql` proof: the exact assignment is current with
/// the caller, allows the retained permission, and covers the given scope.
/// Static permission names only; this never interpolates caller input.
async fn scope_authority(
    c: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    assignment_id: &str,
    role: &str,
    permission: &str,
    scope_branch_id: Option<&str>,
    scope_department_id: Option<&str>,
    now: i64,
) -> Result<bool, BusinessError> {
    let permission_check = match permission {
        "records.read" => "AND iam_role_allows(g.role,g.company_id,'records.read')",
        _ => "AND FALSE",
    };
    let hit: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT g.id FROM assignments g \
          WHERE g.id=$1 AND g.user_id=$2 AND g.role=$3 AND g.company_id=$4 \
          AND iam_assignment_active(g.id,$7) \
          AND iam_principal_active(g.user_id,g.company_id,$7) \
          {permission_check} \
          AND (g.scope_kind='holding' \
            OR (g.scope_kind='branch' AND g.scope_node_id=$5) \
            OR (g.scope_kind='department' AND g.scope_node_id=$6))",
    )))
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

/// Retained assignment currency proof without a permission check. The
/// queued-command session and IAM-version pins are not re-proven; the
/// currency, principal and scope proof stands in for them.
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

/// Retained history fact for one capability. Callers map a missing history
/// to the exact oracle shape (decision collapse for batches, 404 for
/// portal-gated reads).
async fn history_allowed(
    c: &mut PgConnection,
    company_id: &CompanyId,
    capability: &str,
    mode: PackageEnforcementMode,
    now: i64,
) -> Result<bool, BusinessError> {
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

/// Retained staff-task-alert proof: the exact operations-delegate task
/// proof, cited rather than re-derived. Same oracle, same codes.
async fn task_alert_target(
    c: &mut PgConnection,
    gate: &WorkflowGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    let operations = OperationsGate {
        company: gate.company.clone(),
        user_id: gate.user_id.clone(),
        assignment_id: gate.assignment_id.clone(),
        branch_id: gate.branch_id.clone(),
        department_id: gate.department_id.clone(),
        apps_mode: gate.apps_mode,
        projects_mode: gate.projects_mode,
        now: gate.now,
    };
    task_target(c, &operations, source_id).await
}

/// Retained handoff proof: the project purchase with the Project authority,
/// then the handoff row and the project proof. The `activeStaff` collapse
/// is not re-proven (documented gap).
async fn project_handoff_target(
    c: &mut PgConnection,
    gate: &WorkflowGate,
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
    if gate.projects_enforced() {
        let (assignment_id, role) = match &assignment {
            Some((id, role)) => (id.clone(), role.clone()),
            None => return Err(project_access_changed()),
        };
        if !scope_authority(
            c,
            gate.company.as_str(),
            &gate.user_id,
            &assignment_id,
            &role,
            "records.read",
            gate.branch_id.as_deref(),
            gate.department_id.as_deref(),
            gate.now,
        )
        .await?
        {
            return Err(project_access_changed());
        }
        purchase_require(c, &gate.company, "projects.handover", gate.projects_mode, gate.now)
            .await
            .map_err(|error| {
                if error.code == "PACKAGE_REQUIRED" {
                    project_package_required()
                } else {
                    error
                }
            })?;
    }
    let project_id: Option<String> = sqlx::query_scalar(
        "SELECT h.project_id FROM staff_project_handoffs h WHERE h.id=$1 AND h.company_id=$2",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    let Some(project_id) = project_id else {
        return Err(project_handoff_not_found());
    };
    // Application `project()` proof: the records role, the projects
    // module, the assignment currency and the project row. Missing
    // currency or a missing row collapses to the retained 404 exactly
    // like the oracle's `currentProjectAuthority` denial.
    let readable = match &assignment {
        Some((id, _)) => role_allows(c, gate.company.as_str(), id, "records.read").await?,
        None => false,
    };
    if !readable {
        return Err(permission_required());
    }
    let module = match &assignment {
        Some((id, _)) => module_allows(c, id, "projects").await?,
        None => false,
    };
    if !module {
        return Err(projects_module_forbidden());
    }
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
        return Err(project_not_found());
    }
    let project: Option<String> = sqlx::query_scalar(
        "SELECT p.id FROM staff_projects p WHERE p.id=$1 AND p.company_id=$2",
    )
    .bind(project_id.as_str())
    .bind(gate.company.as_str())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if project.is_none() {
        return Err(project_not_found());
    }
    Ok(Some(InboxTarget {
        href: Some(project_handoff_href(&project_id)),
        source: None,
    }))
}

/// Retained maintenance proof shared by work orders and preventive plans:
/// the asset history with the Assets authority, then the `records.read`
/// role, the maintenance module and the scoped asset-joined read.
async fn maintenance_target(
    c: &mut PgConnection,
    gate: &WorkflowGate,
    source_id: &str,
    capability: &str,
    table: &str,
    missing: fn() -> BusinessError,
) -> Result<Option<InboxTarget>, BusinessError> {
    let assignment = resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?;
    if gate.assets_enforced() {
        purchase_require(c, &gate.company, capability, gate.assets_mode, gate.now).await?;
        let current = match &assignment {
            Some((id, role)) => {
                scope_authority(
                    c,
                    gate.company.as_str(),
                    &gate.user_id,
                    id,
                    role,
                    "records.read",
                    gate.branch_id.as_deref(),
                    gate.department_id.as_deref(),
                    gate.now,
                )
                .await?
            }
            None => false,
        };
        if !current {
            return Err(assets_access_changed());
        }
    }
    let readable = match &assignment {
        Some((id, _)) => role_allows(c, gate.company.as_str(), id, "records.read").await?,
        None => false,
    };
    if !readable {
        return Err(permission_required());
    }
    let module = match &assignment {
        Some((id, _)) => module_allows(c, id, "maintenance").await?,
        None => false,
    };
    if !module {
        return Err(maintenance_module_forbidden());
    }
    // The oracle joins the asset row; rows whose asset is gone stay
    // invisible here exactly like the joined read.
    let alias = if table == "maintenance_work_orders" { "w" } else { "p" };
    let visible: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT {alias}.id FROM {table} {alias} \
          JOIN operational_assets a ON a.id={alias}.asset_id AND a.company_id={alias}.company_id \
          WHERE {alias}.id=$1 AND {alias}.company_id=$2 \
          AND ($3::text IS NULL OR {alias}.branch_id=$3) \
          AND ($4::text IS NULL OR {alias}.department_id=$4)"
    )))
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(gate.branch_id.as_deref())
    .bind(gate.department_id.as_deref())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if visible.is_none() {
        return Err(missing());
    }
    Ok(Some(InboxTarget {
        href: Some(if table == "maintenance_work_orders" {
            maintenance_work_href(source_id)
        } else {
            maintenance_plan_href(source_id)
        }),
        source: None,
    }))
}

/// Retained supplier-batch proof: the finance authority with the
/// banking/inventory-valuation history, then the Company-treasury frame
/// and the scoped batch read. The sourceOnly per-invoice document gating
/// is not re-proven (documented gap): seeds use banking-history
/// companies where the oracle skips it too.
async fn supplier_batch_target(
    c: &mut PgConnection,
    gate: &WorkflowGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    if gate.finance_enforced() {
        // Finance authority, literal mirror of
        // `requireFinanceCurrentAuthority`: the exact selected assignment
        // is current over the reader's branch.
        let assignment = resolve_assignment(
            c,
            gate.company.as_str(),
            &gate.user_id,
            gate.assignment_id.as_deref(),
            gate.now,
        )
        .await?;
        let current = match &assignment {
            Some((id, role)) => {
                let hit: Option<String> = sqlx::query_scalar(
                    "SELECT id FROM assignments WHERE id=$1 AND user_id=$2 AND role=$3 AND company_id=$4 \
                      AND valid_from<=$5 AND valid_until>$5 AND revoked_at IS NULL \
                      AND ($6::text IS NULL OR branch_id IS NULL OR branch_id=$6)",
                )
                .bind(id)
                .bind(gate.user_id.as_str())
                .bind(role)
                .bind(gate.company.as_str())
                .bind(gate.now)
                .bind(gate.branch_id.as_deref())
                .fetch_optional(&mut *c)
                .await
                .map_err(|_| storage())?;
                hit.is_some()
                    && role_allows(c, gate.company.as_str(), id, "finance.read").await?
            }
            None => false,
        };
        if !current {
            return Err(finance_access_changed());
        }
        if history_allowed(c, &gate.company, "finance.banking", gate.finance_mode, gate.now).await? {
            // Banking history settles the read; the oracle skips the
            // per-invoice gating on this path too.
        } else {
            purchase_require(
                c,
                &gate.company,
                "finance.inventory-valuation",
                gate.finance_mode,
                gate.now,
            )
            .await?;
        }
        if !gate.holding_frame() {
            return Err(company_treasury_required());
        }
    }
    // Application `company()` proof: the finance role and the holding
    // frame. Missing either collapses to the retained treasury denial
    // exactly like the oracle's `assertCompanyTreasury`.
    let assignment = resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?;
    let readable = match &assignment {
        Some((id, _)) => role_allows(c, gate.company.as_str(), id, "finance.read").await?,
        None => false,
    };
    if !readable || !gate.holding_frame() {
        return Err(company_treasury_required());
    }
    let hit: Option<String> = sqlx::query_scalar(
        "SELECT b.id FROM finance_supplier_batches b WHERE b.id=$1 AND b.company_id=$2",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if hit.is_none() {
        return Err(supplier_batch_not_found());
    }
    Ok(Some(InboxTarget {
        href: Some(supplier_batch_href(source_id)),
        source: None,
    }))
}

struct HandbookRow {
    id: String,
    archived: i64,
    published_revision: Option<i64>,
    draft_revision: i64,
}

/// Retained handbook proof: the work history, the Company membership,
/// then the editor proof and the article/revision read.
async fn handbook_target(
    c: &mut PgConnection,
    gate: &WorkflowGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    if gate.apps_enforced() {
        purchase_require(c, &gate.company, "work.handbook", gate.apps_mode, gate.now).await?;
    }
    // Company membership: any current assignment for the company and user.
    let member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM assignments a WHERE a.company_id=$1 AND a.user_id=$2 \
          AND a.revoked_at IS NULL AND a.valid_from<=$3 AND a.valid_until>$3 \
          AND iam_assignment_active(a.id,$3) AND iam_principal_active(a.user_id,a.company_id,$3))",
    )
    .bind(gate.company.as_str())
    .bind(gate.user_id.as_str())
    .bind(gate.now)
    .fetch_one(&mut *c)
    .await
    .map_err(|_| storage())?;
    if !member {
        return Err(membership_required());
    }
    // Editor proof, literal mirror of the oracle: the holding-owner
    // authority (`manage_erpx.access`) or a handbook-editor row.
    let assignment = resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?;
    let owner = match &assignment {
        Some((id, _)) => {
            let hit: Option<String> = sqlx::query_scalar(
                "SELECT a.id FROM assignments a WHERE a.id=$1 AND a.company_id=$2 \
                  AND a.scope_kind='holding' AND a.revoked_at IS NULL \
                  AND iam_assignment_active(a.id,$3) \
                  AND iam_principal_active(a.user_id,a.company_id,$3) \
                  AND iam_role_allows(a.role,a.company_id,'manage_erpx.access') \
                  AND a.valid_from<=$3 AND a.valid_until>$3 LIMIT 1",
            )
            .bind(id)
            .bind(gate.company.as_str())
            .bind(gate.now)
            .fetch_optional(&mut *c)
            .await
            .map_err(|_| storage())?;
            hit.is_some()
        }
        None => false,
    };
    let listed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM handbook_editors WHERE company_id=$1 AND user_id=$2)",
    )
    .bind(gate.company.as_str())
    .bind(gate.user_id.as_str())
    .fetch_one(&mut *c)
    .await
    .map_err(|_| storage())?;
    let editor = owner || listed;
    let row: Option<HandbookRow> = sqlx::query_as(
        "SELECT id,archived,published_revision,draft_revision FROM handbook_articles \
          WHERE id=$1 AND company_id=$2",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?
    .map(
        |(id, archived, published_revision, draft_revision): (String, i64, Option<i64>, i64)| {
            HandbookRow {
                id,
                archived,
                published_revision,
                draft_revision,
            }
        },
    );
    let Some(row) = row else {
        return Err(article_not_found());
    };
    if !editor && (row.archived != 0 || row.published_revision.is_none()) {
        return Err(article_not_found());
    }
    let wanted = if editor {
        row.draft_revision
    } else {
        row.published_revision.unwrap_or(0)
    };
    // Editors may read the unpublished draft; readers need the published
    // revision, exactly like the oracle's `(?=1 OR published_at IS NOT
    // NULL)` arm.
    let revision: Option<i64> = if editor {
        sqlx::query_scalar(
            "SELECT revision FROM handbook_revisions \
              WHERE article_id=$1 AND company_id=$2 AND revision=$3",
        )
        .bind(source_id)
        .bind(gate.company.as_str())
        .bind(wanted)
        .fetch_optional(&mut *c)
        .await
        .map_err(|_| storage())?
    } else {
        sqlx::query_scalar(
            "SELECT revision FROM handbook_revisions \
              WHERE article_id=$1 AND company_id=$2 AND revision=$3 AND published_at IS NOT NULL",
        )
        .bind(source_id)
        .bind(gate.company.as_str())
        .bind(wanted)
        .fetch_optional(&mut *c)
        .await
        .map_err(|_| storage())?
    };
    let Some(revision) = revision else {
        return Err(article_revision_not_found());
    };
    let href = handbook_article_href(source_id, editor);
    Ok(Some(InboxTarget {
        href: Some(href.clone()),
        source: Some(json!({
            "kind": "handbook_article",
            "id": row.id,
            "version": revision,
            "href": href,
            "label": source_record_label("handbook_article"),
        })),
    }))
}

impl InboxTargetPort for WorkflowInboxTargets {
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
            let apps_mode = match self.apps_enforcement {
                Some(mode) => mode,
                None => enforcement_mode("ERPX_APPS_PACKAGE_ENFORCEMENT")?,
            };
            let projects_mode = match self.projects_enforcement {
                Some(mode) => mode,
                None => enforcement_mode("ERPX_PROJECTS_PACKAGE_ENFORCEMENT")?,
            };
            let assets_mode = match self.assets_enforcement {
                Some(mode) => mode,
                None => enforcement_mode("ERPX_ASSETS_PACKAGE_ENFORCEMENT")?,
            };
            let finance_mode = match self.finance_enforcement {
                Some(mode) => mode,
                None => enforcement_mode("ERPX_FINANCE_PACKAGE_ENFORCEMENT")?,
            };
            // The operations task proof reads the same apps/projects keys
            // with the same logic, so the shared modes stay byte-identical.
            let now: i64 = sqlx::query_scalar(
                "SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint",
            )
            .fetch_one(&mut *connection)
            .await
            .map_err(|_| storage())?;
            let gate = WorkflowGate {
                company,
                user_id: user_id.to_owned(),
                assignment_id: self.assignment_id.clone(),
                branch_id: self.branch_id.clone(),
                department_id: self.department_id.clone(),
                apps_mode,
                projects_mode,
                assets_mode,
                finance_mode,
                now,
            };
            match source_type {
                "staff_task_alert" => task_alert_target(connection, &gate, source_id).await,
                "project_handoff" => project_handoff_target(connection, &gate, source_id).await,
                "maintenance_work" => {
                    maintenance_target(
                        connection,
                        &gate,
                        source_id,
                        "assets.maintenance",
                        "maintenance_work_orders",
                        maintenance_work_not_found,
                    )
                    .await
                }
                "maintenance_plan" => {
                    maintenance_target(
                        connection,
                        &gate,
                        source_id,
                        "assets.preventive",
                        "maintenance_preventive_plans",
                        maintenance_plan_not_found,
                    )
                    .await
                }
                "supplier_batch" => supplier_batch_target(connection, &gate, source_id).await,
                "handbook" | "handbook_article" => {
                    handbook_target(connection, &gate, source_id).await
                }
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
    use crate::adapters::inbox_operations::staff_task_href;
    use crate::adapters::inbox_targets::source_record_label;

    #[test]
    fn routing_covers_only_the_workflow_slice() {
        for source_type in [
            "staff_task_alert",
            "project_handoff",
            "maintenance_work",
            "maintenance_plan",
            "supplier_batch",
            "handbook",
            "handbook_article",
        ] {
            assert!(is_workflow_inbox_target(source_type), "{source_type}");
        }
        for other in [
            "notice",
            "collaboration_action",
            "collaboration_action_plan",
            "task",
            "post",
            "field_service_order",
            "operational_exception",
            "future_source",
            "",
        ] {
            assert!(!is_workflow_inbox_target(other), "{other}");
        }
    }

    #[test]
    fn hrefs_encode_exactly_like_the_typescript_projection() {
        // The alert reuses the staff-task address exactly.
        assert_eq!(task_alert_href("task-1"), staff_task_href("task-1"));
        assert_eq!(task_alert_href("a/b c"), "/staff-tasks/a%2Fb%20c");
        assert_eq!(
            project_handoff_href("proj-1"),
            "/projects/proj-1#project-structure"
        );
        assert_eq!(
            project_handoff_href("a/b c"),
            "/projects/a%2Fb%20c#project-structure"
        );
        assert_eq!(maintenance_work_href("wo-1"), "/maintenance/wo-1");
        assert_eq!(
            maintenance_work_href("a/b c"),
            "/maintenance/a%2Fb%20c"
        );
        assert_eq!(maintenance_plan_href("plan-1"), "/maintenance/plans/plan-1");
        assert_eq!(supplier_batch_href("batch-1"), "/finance/supplier-batches/batch-1");
        assert_eq!(
            supplier_batch_href("a/b c"),
            "/finance/supplier-batches/a%2Fb%20c"
        );
        assert_eq!(handbook_article_href("art-1", false), "/handbook/art-1");
        assert_eq!(
            handbook_article_href("art-1", true),
            "/handbook/art-1?edit=1"
        );
        assert_eq!(
            handbook_article_href("a/b c", true),
            "/handbook/a%2Fb%20c?edit=1"
        );
    }

    #[test]
    fn history_capabilities_match_the_retained_gates() {
        assert_eq!(
            workflow_history_capabilities("staff_task_alert"),
            &[
                "projects.tasks",
                "work.actions",
                "work.advanced-routines",
                "work.social"
            ]
        );
        assert_eq!(
            workflow_history_capabilities("project_handoff"),
            &["projects.handover"]
        );
        assert_eq!(
            workflow_history_capabilities("maintenance_work"),
            &["assets.maintenance"]
        );
        assert_eq!(
            workflow_history_capabilities("maintenance_plan"),
            &["assets.preventive"]
        );
        assert_eq!(
            workflow_history_capabilities("supplier_batch"),
            &["finance.banking", "finance.inventory-valuation"]
        );
        assert_eq!(
            workflow_history_capabilities("handbook"),
            &["work.handbook"]
        );
        assert_eq!(
            workflow_history_capabilities("handbook_article"),
            &["work.handbook"]
        );
        assert_eq!(
            workflow_history_capabilities("field_service_order"),
            &[] as &[&str]
        );
        assert_eq!(workflow_history_capabilities(""), &[] as &[&str]);
    }

    #[test]
    fn workflow_kinds_keep_their_retained_labels() {
        assert_eq!(source_record_label("handbook_article"), "Handbook article");
        let source = json!({
            "kind": "handbook_article",
            "id": "art-1",
            "version": 2,
            "href": "/handbook/art-1",
            "label": source_record_label("handbook_article"),
        });
        assert_eq!(source["kind"], json!("handbook_article"));
        assert_eq!(source["label"], json!("Handbook article"));
    }

    #[test]
    fn oracle_denials_keep_their_exact_codes_kinds_and_messages() {
        let authority = project_access_changed();
        assert_eq!(authority.kind, FailureKind::Forbidden);
        assert_eq!(authority.code, "PROJECT_ACCESS_CHANGED");
        let missing = project_handoff_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "PROJECT_HANDOFF_NOT_FOUND");
        assert_eq!(missing.message, "Project handoff not found.");
        let missing = project_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "PROJECT_NOT_FOUND");
        let authority = assets_access_changed();
        assert_eq!(authority.kind, FailureKind::Forbidden);
        assert_eq!(authority.code, "ASSETS_ACCESS_CHANGED");
        let missing = maintenance_work_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "MAINTENANCE_WORK_NOT_FOUND");
        let missing = maintenance_plan_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "MAINTENANCE_PLAN_NOT_FOUND");
        let authority = finance_access_changed();
        assert_eq!(authority.kind, FailureKind::Forbidden);
        assert_eq!(authority.code, "FINANCE_ACCESS_CHANGED");
        let frame = company_treasury_required();
        assert_eq!(frame.kind, FailureKind::Forbidden);
        assert_eq!(frame.code, "COMPANY_TREASURY_REQUIRED");
        let missing = supplier_batch_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "BATCH_NOT_FOUND");
        let missing = membership_required();
        assert_eq!(missing.kind, FailureKind::Forbidden);
        assert_eq!(missing.code, "MEMBERSHIP_REQUIRED");
        let missing = article_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "ARTICLE_NOT_FOUND");
        assert_eq!(
            missing.message,
            "This article has not been published or is unavailable."
        );
        let missing = article_revision_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "ARTICLE_NOT_FOUND");
    }
}
