//! Native Operations visibility delegate for the Inbox destination port.
//!
//! This mirrors the retained per-row TypeScript gates for the
//! notification-visible Operations sources:
//!
//! - `operational_exception`: the fallback `operationsExceptionDetail` —
//!   the `records.read` role, the operations module, then the scoped
//!   candidate read excluding field-service rows; 403
//!   `PERMISSION_REQUIRED` without the role, 403 `MODULE_FORBIDDEN`
//!   without the module, 404 `OPERATIONS_EXCEPTION_NOT_FOUND` when the
//!   row is missing. Field-service rows fail closed here with the same
//!   404: the oracle would try its external native work source first,
//!   which has no caller-transaction equivalent, so the unprovable native
//!   link is never invented. No source override.
//! - `task`: `getTask` — the project purchase for project tasks, the
//!   social-source gate, the work authority and capability history for
//!   ordinary tasks, then the scoped task read (creator, assignee,
//!   delegate, project owner or member, approval participants) with the
//!   project and work scopes; a linked source post must itself resolve;
//!   403 `WORK_TASK_SOURCE_UNAVAILABLE` when a linked conversation
//!   source is gone, 403 `WORK_TASK_AUTHORITY_CHANGED` without a current
//!   assignment, 403 `PACKAGE_REQUIRED` without history, 404
//!   `TASK_NOT_FOUND` when the row is missing, 404 `POST_NOT_FOUND` when
//!   its source post is gone. No source override.
//! - `post`: the published-post read (company-scoped, published,
//!   audience-visible) with the work purchase scope; 404 `POST_NOT_FOUND`
//!   when the row is missing. No source override.
//!
//! Gate order matches each oracle exactly, including the disabled-mode
//! shapes where the purchase paths are skipped.
//!
//! Nothing here opens a pool, commits, or retries. It runs serializable-safe
//! SELECTs on the caller`s connection.
//!
//! Faithful-mirror notes (documented, not silent):
//! - The JWT-static `hasPermission` and module halves are enforced
//!   database-current via `iam_role_allows` on the exact (or highest
//!   active) assignment and its `modules_json`. A stale claim fails
//!   closed with the module code instead of the assignment-selection
//!   code.
//! - The native-first operational-exception branch resolves through an
//!   external field-service work source outside any caller transaction.
//!   There is no native equivalent to prove here, so field-service rows
//!   fail closed with the fallback 404 instead of inventing the link.
//! - The application-visible source check on exception candidates is not
//!   re-proven per linked owner: candidates resolve through the scoped
//!   candidate read, and rows whose linked source the reader cannot see
//!   stay visible here while the oracle would drop them. Seeds use
//!   self-contained rows.
//! - The task social scope is mirrored as published, open-audience,
//!   same-company origins: audience- or purchase-gated conversation
//!   sources fail closed instead of resolving, and the social history
//!   capability is required whenever a task carries conversation origins.
//!   The queued-command authority (assignment version pins, revoked
//!   sessions, member versions, emergency windows, HR appointments,
//!   active work contexts) and the scheduled-feature gates are not
//!   re-proven; the assignment currency, principal and scope proof
//!   stands in for them.
//! - The command-side guards (`workContextGuards`, `iamMutationGuards`,
//!   assertion clearing, purchase boundaries, receipts, locks, deferred
//!   finance effects) authorize commands, not reads, and are not part of
//!   this read-only port.
//! - Presentation joins and facts (project and person names, delegations,
//!   histories, proposals, offers, handoffs, reservations, events,
//!   postings, genealogy, costs, attachments, work states) are not
//!   visibility: rows passing the identity proof resolve without them.
use crate::adapters::inbox_pg::{InboxTarget, InboxTargetFuture, InboxTargetPort};
use crate::adapters::inbox_targets::{encode_uri_component, retained_inbox_destination};
use erpx_commercial_server::public::capability_decision::PgCapabilityDecisionReader;
use erpx_commercial_server::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, current_package_catalogue,
};
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use sqlx::PgConnection;

/// Source types this delegate can prove against Operations rows.
pub fn is_operations_inbox_target(source_type: &str) -> bool {
    matches!(
        source_type,
        "operational_exception" | "task" | "post"
    )
}

/// Retained destination address for one operational exception,
/// byte-identical to the TypeScript `` `/operations/${encodeURIComponent(id)}` ``.
pub fn operational_exception_href(source_id: &str) -> String {
    format!("/operations/{}", encode_uri_component(source_id))
}

/// Retained destination address for one staff task, byte-identical to the
/// TypeScript `` `/staff-tasks/${encodeURIComponent(id)}` ``.
pub fn staff_task_href(source_id: &str) -> String {
    format!("/staff-tasks/{}", encode_uri_component(source_id))
}

/// Retained destination address for one post, byte-identical to the
/// TypeScript `` `/posts/${encodeURIComponent(id)}` ``.
pub fn post_href(source_id: &str) -> String {
    format!("/posts/{}", encode_uri_component(source_id))
}

/// Retained read capabilities per source type. Tasks settle the project
/// capability for project tasks and the work capabilities (plus the
/// advanced-routines and social capabilities for templated or
/// conversation-linked tasks) otherwise; posts settle the social
/// capability; exceptions settle none.
pub fn operations_history_capabilities(source_type: &str) -> &'static [&'static str] {
    match source_type {
        "operational_exception" => &[],
        "task" => &[
            "projects.tasks",
            "work.actions",
            "work.advanced-routines",
            "work.social",
        ],
        "post" => &["work.social"],
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

fn operations_module_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "MODULE_FORBIDDEN",
        "Operations is unavailable to the selected role.",
    )
}

fn operations_exception_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "OPERATIONS_EXCEPTION_NOT_FOUND",
        "Exception or source is unavailable in this scope.",
    )
}

fn work_task_source_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "WORK_TASK_SOURCE_UNAVAILABLE",
        "The original conversation source is no longer available to this task account.",
    )
}

fn work_task_authority_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "WORK_TASK_AUTHORITY_CHANGED",
        "Choose the current task account and Company role.",
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

fn project_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PROJECT_NOT_FOUND",
        "Project not found in the current workspace.",
    )
}

fn work_task_package_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PACKAGE_REQUIRED",
        "This ordinary Work task requires its purchased capability or retained history.",
    )
}

fn task_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "TASK_NOT_FOUND",
        "Task not found.",
    )
}

fn post_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "POST_NOT_FOUND",
        "This post is unavailable.",
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

/// Native Operations visibility delegate, composed via
/// `PgInboxTargets::with_delegate`. Operations-only: every other source type
/// defers to the existing fail-closed behavior instead of inventing a link.
pub struct OperationsInboxTargets {
    branch_id: Option<String>,
    department_id: Option<String>,
    assignment_id: Option<String>,
    enforcement: Option<PackageEnforcementMode>,
    projects_enforcement: Option<PackageEnforcementMode>,
}

impl OperationsInboxTargets {
    pub fn new(branch_id: Option<&str>, department_id: Option<&str>) -> Self {
        Self {
            branch_id: branch_id.map(str::to_owned),
            department_id: department_id.map(str::to_owned),
            assignment_id: None,
            enforcement: None,
            projects_enforcement: None,
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
    /// retained owner environment-driven mode exactly (apps key for the
    /// work slice, projects key for the project slice). This seam exists so
    /// tests and composition can select a mode deterministically; it never
    /// changes what the mode means.
    pub fn enforcement_mode(mut self, mode: PackageEnforcementMode) -> Self {
        self.enforcement = Some(mode);
        self
    }

    /// Pin the projects enforcement mode separately. Tasks settle project
    /// and work capabilities under different owner keys.
    pub fn projects_enforcement_mode(mut self, mode: PackageEnforcementMode) -> Self {
        self.projects_enforcement = Some(mode);
        self
    }
}

impl Default for OperationsInboxTargets {
    fn default() -> Self {
        Self::new(None, None)
    }
}

/// Retained environment-driven enforcement mode, mirrored from the
/// TypeScript oracles: disabled opens the read, a non-local environment
/// fails with the environment code, and an unknown value fails closed with
/// the configuration code.
pub(crate) fn enforcement_mode(key: &str) -> Result<PackageEnforcementMode, BusinessError> {
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

pub(crate) struct OperationsGate {
    pub(crate) company: CompanyId,
    pub(crate) user_id: String,
    pub(crate) assignment_id: Option<String>,
    pub(crate) branch_id: Option<String>,
    pub(crate) department_id: Option<String>,
    pub(crate) apps_mode: PackageEnforcementMode,
    pub(crate) projects_mode: PackageEnforcementMode,
    pub(crate) now: i64,
}

impl OperationsGate {
    fn apps_enforced(&self) -> bool {
        self.apps_mode != PackageEnforcementMode::Disabled
    }

    fn projects_enforced(&self) -> bool {
        self.projects_mode != PackageEnforcementMode::Disabled
    }

    fn work_gated(&self) -> bool {
        self.apps_enforced() || self.projects_enforced()
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

/// Retained history fact for one capability. Callers map a missing history
/// to the exact oracle code.
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

/// Retained exception proof: the readable check, then the scoped candidate
/// read excluding field-service rows.
async fn exception_target(
    c: &mut PgConnection,
    gate: &OperationsGate,
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
    let readable = match &assignment {
        Some((id, _)) => role_allows(c, gate.company.as_str(), id, "records.read").await?,
        None => false,
    };
    if !readable {
        return Err(permission_required());
    }
    let module = match &assignment {
        Some((id, _)) => module_allows(c, id, "operations").await?,
        None => false,
    };
    if !module {
        return Err(operations_module_forbidden());
    }
    let visible: Option<String> = sqlx::query_scalar(
        "SELECT e.id FROM operations_exceptions e \
          WHERE e.id=$1 AND e.company_id=$2 \
          AND ($3::text IS NULL OR e.branch_id=$3) \
          AND ($4::text IS NULL OR e.department_id=$4) \
          AND e.source_kind<>'field_service_order'",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(gate.branch_id.as_deref())
    .bind(gate.department_id.as_deref())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if visible.is_none() {
        return Err(operations_exception_not_found());
    }
    Ok(Some(InboxTarget {
        href: Some(operational_exception_href(source_id)),
        source: None,
    }))
}

/// Retained assignment currency proof without a permission check, mirror
/// of the work-task authority shape: the exact assignment is current with
/// the caller and covers the given scope.
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

/// Retained task proof: the project purchase for project tasks, the
/// social-source gate, the work authority and capability history for
/// ordinary tasks, then the scoped task read; a linked source post must
/// itself resolve.
pub(crate) async fn task_target(
    c: &mut PgConnection,
    gate: &OperationsGate,
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
    // The oracle proves purchase authority before the participant-scoped
    // read, so existence is probed by id alone first: a missing row is
    // TASK_NOT_FOUND, while an existing-but-invisible row keeps falling
    // through to the branch authority below.
    let probe: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT t.project_id,t.template_id FROM staff_tasks t WHERE t.id=$1 AND t.company_id=$2",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    let Some((probe_project_id, probe_template_id)) = probe else {
        return Err(task_not_found());
    };
    let is_project = probe_project_id.is_some();
    if probe_project_id.is_some() && gate.projects_enforced() {
        // Project tasks settle the project purchase before the
        // participant-scoped read below, exactly like the oracle's project
        // purchased read running before the scoped application read.
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
        if !history_allowed(c, &gate.company, "projects.tasks", gate.projects_mode, gate.now).await?
        {
            return Err(project_package_required());
        }
        let project: Option<String> = sqlx::query_scalar(
            "SELECT p.id FROM staff_projects p WHERE p.id=$1 AND p.company_id=$2",
        )
        .bind(probe_project_id.as_deref())
        .bind(gate.company.as_str())
        .fetch_optional(&mut *c)
        .await
        .map_err(|_| storage())?;
        if project.is_none() {
            return Err(project_not_found());
        }
    }
    // The scoped task row is read first so the project and ordinary
    // branches below share one identity proof.
    let row: Option<Option<String>> = sqlx::query_scalar::<_, Option<String>>(
        "SELECT o.post_id FROM staff_tasks t \
          LEFT JOIN staff_projects p ON p.id=t.project_id AND p.company_id=t.company_id \
          LEFT JOIN social_task_origins o ON o.task_id=t.id \
          WHERE t.id=$1 AND t.company_id=$2 \
          AND (iam_same_person(t.creator_user_id,$3,t.company_id) \
          OR iam_same_person(t.assignee_user_id,$4,t.company_id) \
          OR EXISTS(SELECT 1 FROM staff_task_delegations sd WHERE sd.task_id=t.id \
            AND sd.company_id=t.company_id AND sd.status='active' \
            AND sd.starts_at<=$12 AND sd.expires_at>$12 \
            AND iam_same_person(sd.delegate_user_id,$5,t.company_id)) \
          OR iam_same_person(p.owner_user_id,$6,t.company_id) \
          OR EXISTS(SELECT 1 FROM staff_project_members pm WHERE pm.project_id=t.project_id \
            AND pm.user_id=$7) \
          OR EXISTS(SELECT 1 FROM erpx_json_each(t.start_approvers_json) WHERE value=$8) \
          OR EXISTS(SELECT 1 FROM erpx_json_each(t.completion_approvers_json) WHERE value=$9) \
          OR EXISTS(SELECT 1 FROM approval_instances ai JOIN approval_steps ast \
            ON ast.instance_id=ai.id WHERE ai.record_type='task' AND ai.record_id=t.id \
            AND ai.company_id=t.company_id \
            AND EXISTS(SELECT 1 FROM (SELECT ast.approver_user_id user_id \
              UNION SELECT value FROM erpx_json_each(COALESCE(ast.approver_pool_json,'[]'))) participant \
              WHERE iam_same_person(participant.user_id,$10,ai.company_id)) \
            AND (ast.state IN ('approved','rejected') \
              OR ast.stage='department_approval' \
                AND jsonb_exists(ast.route_snapshot_json::jsonb,'departmentPolicyVersion') \
              OR ast.stage='hr_approval' \
                AND ast.route_snapshot_json::jsonb->>'appointedPersonId' IS NOT NULL \
              OR EXISTS(SELECT 1 FROM assignments aa WHERE aa.id=$11 \
                AND iam_role_allows(aa.role,aa.company_id,'workflow.'||COALESCE(ast.required_role, \
                  CASE WHEN ast.authority IN ('named','department') THEN 'people_manager' \
                  ELSE ast.authority END)) \
                AND aa.company_id=ai.company_id \
                AND (aa.scope_kind='holding' \
                  OR aa.scope_kind='branch' AND aa.scope_node_id=ai.branch_id \
                  OR aa.scope_kind='department' AND aa.scope_node_id=ai.department_id)))))",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(&gate.user_id)
    .bind(&gate.user_id)
    .bind(&gate.user_id)
    .bind(&gate.user_id)
    .bind(&gate.user_id)
    .bind(&gate.user_id)
    .bind(&gate.user_id)
    .bind(&gate.user_id)
    .bind(
        assignment
            .as_ref()
            .map(|(id, _)| id.as_str()),
    )
    .bind(gate.now)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    let has_origins: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM social_task_origins WHERE task_id=$1)",
    )
    .bind(source_id)
    .fetch_one(&mut *c)
    .await
    .map_err(|_| storage())?;
    // Ordinary tasks settle the work gates before the participant-scoped
    // read below, exactly like the oracle proving purchase authority
    // before the scoped application read.
    if !is_project && gate.work_gated() {
        if has_origins && !social_origins_visible(c, gate, source_id).await? {
            return Err(work_task_source_unavailable());
        }
        if gate.apps_enforced() {
            let (assignment_id, role) = match &assignment {
                Some((id, role)) => (id.clone(), role.clone()),
                None => return Err(work_task_authority_changed()),
            };
            if !assignment_current(
                c,
                gate.company.as_str(),
                &gate.user_id,
                &assignment_id,
                &role,
                gate.branch_id.as_deref(),
                gate.department_id.as_deref(),
                gate.now,
            )
            .await?
            {
                return Err(work_task_authority_changed());
            }
            let mut capabilities = vec!["work.actions"];
            if probe_template_id.is_some() {
                capabilities.push("work.advanced-routines");
            }
            if has_origins {
                capabilities.push("work.social");
            }
            for capability in capabilities {
                if !history_allowed(c, &gate.company, capability, gate.apps_mode, gate.now).await? {
                    return Err(work_task_package_required());
                }
            }
        }
    }
    let Some(source_post_id) = row else {
        return Err(task_not_found());
    };
    if let Some(post_id) = source_post_id {
        post_visible(c, gate, &post_id).await?;
    }
    Ok(Some(InboxTarget {
        href: Some(staff_task_href(source_id)),
        source: None,
    }))
}

/// Retained conversation-source gate for tasks: linked origins must
/// resolve to published, open-audience, same-company posts.
async fn social_origins_visible(
    c: &mut PgConnection,
    gate: &OperationsGate,
    source_id: &str,
) -> Result<bool, BusinessError> {
    let blocked: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM social_task_origins origin \
          LEFT JOIN social_posts post ON post.id=origin.post_id AND post.company_id=$2 \
          WHERE origin.task_id=$1 \
          AND (post.id IS NULL OR post.state<>'published' \
            OR NOT (post.audience_kind IS NULL \
              OR post.audience_kind IN ('company','branch','department'))))",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .fetch_one(&mut *c)
    .await
    .map_err(|_| storage())?;
    Ok(!blocked)
}

/// Retained published-post proof shared by the post slice and the task
/// source-post arm: company-scoped, published, open-audience.
async fn post_visible(
    c: &mut PgConnection,
    gate: &OperationsGate,
    source_id: &str,
) -> Result<(), BusinessError> {
    let visible: Option<String> = sqlx::query_scalar(
        "SELECT p.id FROM social_posts p \
          WHERE p.id=$1 AND p.company_id=$2 AND p.state='published' \
          AND (p.audience_kind IS NULL \
            OR p.audience_kind IN ('company','branch','department'))",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if visible.is_none() {
        return Err(post_not_found());
    }
    Ok(())
}

/// Retained post proof: the published-post read with the work purchase
/// scope. Posts carry no frame of their own and no readable gate: in
/// enforced mode the reader's currency and the social history scope the
/// read, and every negative collapses to the retained 404 exactly like
/// the oracle's scope filtering.
async fn post_target(
    c: &mut PgConnection,
    gate: &OperationsGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    if gate.apps_enforced() {
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
        if !current
            || !history_allowed(c, &gate.company, "work.social", gate.apps_mode, gate.now).await?
        {
            return Err(post_not_found());
        }
    }
    post_visible(c, gate, source_id).await?;
    Ok(Some(InboxTarget {
        href: Some(post_href(source_id)),
        source: None,
    }))
}

impl InboxTargetPort for OperationsInboxTargets {
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
            let apps_mode = match self.enforcement {
                Some(mode) => mode,
                None => enforcement_mode("ERPX_APPS_PACKAGE_ENFORCEMENT")?,
            };
            let projects_mode = match self.projects_enforcement {
                Some(mode) => mode,
                None => enforcement_mode("ERPX_PROJECTS_PACKAGE_ENFORCEMENT")?,
            };
            let now: i64 = sqlx::query_scalar(
                "SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint",
            )
            .fetch_one(&mut *connection)
            .await
            .map_err(|_| storage())?;
            let gate = OperationsGate {
                company,
                user_id: user_id.to_owned(),
                assignment_id: self.assignment_id.clone(),
                branch_id: self.branch_id.clone(),
                department_id: self.department_id.clone(),
                apps_mode,
                projects_mode,
                now,
            };
            match source_type {
                "operational_exception" => exception_target(connection, &gate, source_id).await,
                "task" => task_target(connection, &gate, source_id).await,
                "post" => post_target(connection, &gate, source_id).await,
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
    fn routing_covers_only_the_operations_slice() {
        for source_type in ["operational_exception", "task", "post"] {
            assert!(is_operations_inbox_target(source_type), "{source_type}");
        }
        for other in [
            "notice",
            "collaboration_action",
            "calendar_appointment",
            "crm_lead",
            "commerce_order",
            "invoice",
            "support_ticket",
            "operations",
            "future_source",
            "",
        ] {
            assert!(!is_operations_inbox_target(other), "{other}");
        }
    }

    #[test]
    fn hrefs_encode_exactly_like_the_typescript_projection() {
        assert_eq!(
            operational_exception_href("exc-1"),
            "/operations/exc-1"
        );
        assert_eq!(
            operational_exception_href("a/b c"),
            "/operations/a%2Fb%20c"
        );
        assert_eq!(staff_task_href("task-1"), "/staff-tasks/task-1");
        assert_eq!(staff_task_href("a/b c"), "/staff-tasks/a%2Fb%20c");
        assert_eq!(post_href("post-1"), "/posts/post-1");
        assert_eq!(post_href("a/b c"), "/posts/a%2Fb%20c");
    }

    #[test]
    fn history_capabilities_match_the_retained_gates() {
        assert_eq!(operations_history_capabilities("operational_exception"), &[] as &[&str]);
        assert_eq!(
            operations_history_capabilities("task"),
            &[
                "projects.tasks",
                "work.actions",
                "work.advanced-routines",
                "work.social"
            ]
        );
        assert_eq!(
            operations_history_capabilities("post"),
            &["work.social"]
        );
        assert_eq!(operations_history_capabilities("operations"), &[] as &[&str]);
        assert_eq!(operations_history_capabilities(""), &[] as &[&str]);
    }

    #[test]
    fn operations_kinds_keep_their_retained_labels() {
        // The table keeps the housekeeping label for the raw task kind;
        // the notification final map surfaces staff tasks as "Task".
        assert_eq!(
            source_record_label("operational_exception"),
            "Operational exception"
        );
        assert_eq!(source_record_label("task"), "Housekeeping task");
        assert_eq!(source_record_label("post"), "Post");
    }

    #[test]
    fn oracle_denials_keep_their_exact_codes_kinds_and_messages() {
        let module = operations_module_forbidden();
        assert_eq!(module.kind, FailureKind::Forbidden);
        assert_eq!(module.code, "MODULE_FORBIDDEN");
        let missing = operations_exception_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "OPERATIONS_EXCEPTION_NOT_FOUND");
        assert_eq!(
            missing.message,
            "Exception or source is unavailable in this scope."
        );
        let source = work_task_source_unavailable();
        assert_eq!(source.kind, FailureKind::Forbidden);
        assert_eq!(source.code, "WORK_TASK_SOURCE_UNAVAILABLE");
        let authority = work_task_authority_changed();
        assert_eq!(authority.kind, FailureKind::Forbidden);
        assert_eq!(authority.code, "WORK_TASK_AUTHORITY_CHANGED");
        let package = work_task_package_required();
        assert_eq!(package.kind, FailureKind::Forbidden);
        assert_eq!(package.code, "PACKAGE_REQUIRED");
        let missing = task_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "TASK_NOT_FOUND");
        assert_eq!(missing.message, "Task not found.");
        let missing = post_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "POST_NOT_FOUND");
        assert_eq!(missing.message, "This post is unavailable.");
    }
}
