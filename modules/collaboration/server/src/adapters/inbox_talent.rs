//! Native Talent, Marketing and Manufacturing visibility delegate for the
//! Inbox destination port.
//!
//! This mirrors the retained per-row TypeScript gates for the
//! notification-visible Talent, Marketing and Manufacturing sources:
//!
//! - `talent_application`: `talentDetail` — the current Recruitment role
//!   (`records.read` plus `records.review`), the `recruit.candidates`,
//!   `recruit.applications` and `recruit.offers` history, the Talent module,
//!   then the scoped application read behind its requisition and candidate;
//!   403 `TALENT_ACCESS_CHANGED` without authority, 403
//!   `MODULE_FORBIDDEN` without the module, 404
//!   `TALENT_APPLICATION_NOT_FOUND` when missing. No source override.
//! - `marketing_candidate`: `marketingDetail` — the Marketing module, the
//!   current `records.read` role, the consented-candidate image, the
//!   `sales.marketing` history, then the scoped candidate read with its
//!   form join; 403 `MODULE_FORBIDDEN` without the module, 403
//!   `MARKETING_ACCESS_CHANGED` without authority, 404
//!   `MARKETING_SOURCE_NOT_FOUND` when the image is missing, 403
//!   `PACKAGE_REQUIRED` without history, 404
//!   `MARKETING_CANDIDATE_NOT_FOUND` when the row is missing. No source
//!   override.
//! - `manufacturing_order`: `productionDetail` — the Manufacturing module,
//!   the current `records.read` role, the order-chain image (recommendation,
//!   scenario, specification and warehouse), the execution history plus the
//!   advanced-MRP history for advanced plans, then the scoped order read
//!   with its specification and product joins; 403
//!   `MANUFACTURING_ACCESS_CHANGED` without authority, 404
//!   `PRODUCTION_SOURCE_NOT_FOUND` when the image is missing, 403
//!   `PACKAGE_REQUIRED` without history, 404
//!   `MANUFACTURING_ORDER_NOT_FOUND` when the row is missing. No source
//!   override.
//!
//! Gate order matches each oracle exactly, including the disabled-mode
//! shape: with enforcement off the purchase path is skipped and the
//! readable check authorizes the scoped read directly.
//!
//! Nothing here opens a pool, commits, or retries. It runs serializable-safe
//! SELECTs on the caller`s connection.
//!
//! Faithful-mirror notes (documented, not silent):
//! - The JWT-static `hasPermission` and module halves are enforced
//!   database-current via `iam_role_allows` on the exact (or highest
//!   active) assignment and its `modules_json`, literal mirror of the
//!   retained `assignmentReadSql` shape. The JWT module claim is subsumed
//!   by the assignment module proof; a stale claim fails closed with the
//!   module code instead of the assignment-selection code.
//! - The command-side guards (`workContextGuards`, `iamMutationGuards`,
//!   assertion clearing, purchase boundaries, receipts, locks, deferred
//!   finance effects) authorize commands, not reads, and are not part of
//!   this read-only port.
//! - Presentation joins and facts (owner eligibility, product and warehouse
//!   names, history, offers, handoffs, reservations, events, postings,
//!   genealogy, costs) are not visibility: rows passing the identity proof
//!   resolve without them.
//! - The manufacturing image replays the full recommendation chain
//!   (scenario comparison logic included) and resolves the `advanced`
//!   flag from the scenario, exactly like the oracle; advanced plans
//!   settle the advanced-MRP history on top of execution.
use crate::adapters::inbox_pg::{InboxTarget, InboxTargetFuture, InboxTargetPort};
use crate::adapters::inbox_targets::{encode_uri_component, retained_inbox_destination};
use erpx_commercial_server::public::capability_decision::PgCapabilityDecisionReader;
use erpx_commercial_server::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, current_package_catalogue,
};
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use sqlx::{PgConnection, Row};

/// Source types this delegate can prove against Talent, Marketing and
/// Manufacturing rows.
pub fn is_talent_inbox_target(source_type: &str) -> bool {
    matches!(
        source_type,
        "talent_application" | "marketing_candidate" | "manufacturing_order"
    )
}

/// Retained destination address for one talent application, byte-identical
/// to the TypeScript `` `/talent/${encodeURIComponent(id)}` ``.
pub fn talent_application_href(source_id: &str) -> String {
    format!("/talent/{}", encode_uri_component(source_id))
}

/// Retained destination address for one marketing candidate, byte-identical
/// to the TypeScript `` `/marketing/${encodeURIComponent(id)}` ``.
pub fn marketing_candidate_href(source_id: &str) -> String {
    format!("/marketing/{}", encode_uri_component(source_id))
}

/// Retained destination address for one manufacturing order, byte-identical
/// to the TypeScript `` `/manufacturing/${encodeURIComponent(id)}` ``.
pub fn manufacturing_order_href(source_id: &str) -> String {
    format!("/manufacturing/{}", encode_uri_component(source_id))
}

/// Retained read capabilities per source type. Talent settles three
/// recruit histories; the marketing candidate settles the marketing
/// history; manufacturing orders settle execution plus the advanced-MRP
/// history for advanced plans.
pub fn talent_history_capabilities(source_type: &str) -> &'static [&'static str] {
    match source_type {
        "talent_application" => &[
            "recruit.candidates",
            "recruit.applications",
            "recruit.offers",
        ],
        "marketing_candidate" => &["sales.marketing"],
        "manufacturing_order" => &["manufacturing.execution", "planning.advanced-mrp"],
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

fn talent_module_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "MODULE_FORBIDDEN",
        "Your current role cannot access Talent.",
    )
}

fn talent_privacy_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "TALENT_PRIVACY_FORBIDDEN",
        "Candidate data requires Talent reviewer access.",
    )
}

fn talent_access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "TALENT_ACCESS_CHANGED",
        "Choose a current authorized Recruitment work role.",
    )
}

fn talent_application_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "TALENT_APPLICATION_NOT_FOUND",
        "Application not found.",
    )
}

fn marketing_module_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "MODULE_FORBIDDEN",
        "Your current role cannot access Marketing.",
    )
}

fn marketing_access_changed(message: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "MARKETING_ACCESS_CHANGED",
        message,
    )
}

fn marketing_source_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "MARKETING_SOURCE_NOT_FOUND",
        "The original Marketing source is unavailable in this workspace.",
    )
}

fn marketing_package_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PACKAGE_REQUIRED",
        "The Marketing addon or retained history is required.",
    )
}

fn marketing_candidate_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "MARKETING_CANDIDATE_NOT_FOUND",
        "Lead candidate not found.",
    )
}

fn manufacturing_module_message() -> &'static str {
    "Choose a current original production assignment."
}

fn manufacturing_access_changed(message: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "MANUFACTURING_ACCESS_CHANGED",
        message,
    )
}

fn manufacturing_module_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "MODULE_FORBIDDEN",
        "Your current role cannot access manufacturing.",
    )
}

fn production_source_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PRODUCTION_SOURCE_NOT_FOUND",
        "The original production source is unavailable in this workspace.",
    )
}

fn manufacturing_package_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PACKAGE_REQUIRED",
        "The original production capability or retained history is required.",
    )
}

fn manufacturing_order_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "MANUFACTURING_ORDER_NOT_FOUND",
        "Order not found.",
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
        PgTransactionError::Persistence(error) => {

            storage()
        }
    }
}

/// Native Talent, Marketing and Manufacturing visibility delegate, composed
/// via `PgInboxTargets::with_delegate`. Talent-only: every other source type
/// defers to the existing fail-closed behavior instead of inventing a link.
pub struct TalentInboxTargets {
    branch_id: Option<String>,
    department_id: Option<String>,
    assignment_id: Option<String>,
    enforcement: Option<PackageEnforcementMode>,
}

impl TalentInboxTargets {
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
    /// retained owner environment-driven mode exactly (talent, commerce and
    /// manufacturing keys per source). This seam exists so tests and
    /// composition can select a mode deterministically; it never changes
    /// what the mode means.
    pub fn enforcement_mode(mut self, mode: PackageEnforcementMode) -> Self {
        self.enforcement = Some(mode);
        self
    }
}

impl Default for TalentInboxTargets {
    fn default() -> Self {
        Self::new(None, None)
    }
}

/// Retained environment-driven enforcement mode, mirrored from the
/// TypeScript oracles: disabled opens the read, a non-local environment
/// fails with the environment code, and an unknown value fails closed with
/// the configuration code.
fn enforcement_mode(source_type: &str) -> Result<PackageEnforcementMode, BusinessError> {
    let key = match source_type {
        "talent_application" => "ERPX_TALENT_PACKAGE_ENFORCEMENT",
        "marketing_candidate" => "ERPX_COMMERCE_PACKAGE_ENFORCEMENT",
        "manufacturing_order" => "ERPX_MANUFACTURING_PACKAGE_ENFORCEMENT",
        _ => "ERPX_COMMERCE_PACKAGE_ENFORCEMENT",
    };
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

struct TalentGate {
    company: CompanyId,
    user_id: String,
    assignment_id: Option<String>,
    branch_id: Option<String>,
    department_id: Option<String>,
    mode: PackageEnforcementMode,
    now: i64,
}

impl TalentGate {
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
        "records.review" => "AND iam_role_allows(g.role,g.company_id,'records.review')",
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

/// Retained purchase-history proof for the given capabilities through the
/// native Commercial capability port. The refusal carries the exact
/// decision code and message from the TypeScript throw.
async fn purchase_require(
    c: &mut PgConnection,
    company_id: &CompanyId,
    capabilities: &[&str],
    mode: PackageEnforcementMode,
    now: i64,
) -> Result<(), BusinessError> {
    let catalogue = current_package_catalogue()?;
    for capability in capabilities {
        let mut reader = PgCapabilityDecisionReader::new(company_id.clone(), c, &catalogue);
        reader
            .require(
                capability,
                CapabilityPurpose::History,
                mode,
                TimestampMillis::new(now),
            )
            .await
            .map_err(|e| {

                pivot(e)
            })?;
    }
    Ok(())
}

/// Retained purchase-history fact for one capability. Callers map a
/// missing history to the exact oracle code.
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

/// Retained talent proof: the Recruitment role, the Talent module, the
/// recruit history, then the scoped application read behind its
/// requisition and candidate.
async fn talent_target(
    c: &mut PgConnection,
    gate: &TalentGate,
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

        let (assignment_id, role) = match assignment {
            Some((id, role)) => (id, role),
            None => return Err(talent_access_changed()),
        };
        for permission in ["records.read", "records.review"] {
            if !scope_authority(
                c,
                gate.company.as_str(),
                &gate.user_id,
                &assignment_id,
                &role,
                permission,
                gate.branch_id.as_deref(),
                gate.department_id.as_deref(),
                gate.now,
            )
            .await?
            {
                return Err(talent_access_changed());
            }
        }
        if !module_allows(c, &assignment_id, "talent").await? {
            return Err(talent_module_forbidden());
        }

        purchase_require(
            c,
            &gate.company,
            &[
                "recruit.candidates",
                "recruit.applications",
                "recruit.offers",
            ],
            gate.mode,
            gate.now,
        )
        .await?;
    } else {
        let readable = match &assignment {
            Some((id, _)) => role_allows(c, gate.company.as_str(), id, "records.read").await?,
            None => false,
        };
        if !readable {
            return Err(permission_required());
        }
        let module = match &assignment {
            Some((id, _)) => module_allows(c, id, "talent").await?,
            None => false,
        };
        if !module {
            return Err(talent_module_forbidden());
        }
        let reviewer = match &assignment {
            Some((id, _)) => role_allows(c, gate.company.as_str(), id, "records.review").await?,
            None => false,
        };
        if !reviewer {
            return Err(talent_privacy_forbidden());
        }
    }
    let visible: Option<String> = sqlx::query_scalar(
        "SELECT a.id FROM talent_applications a \
          JOIN talent_requisitions r ON r.id=a.requisition_id \
          JOIN talent_candidates cand ON cand.id=a.candidate_id \
          WHERE a.id=$1 AND a.company_id=$2 \
          AND ($3::text IS NULL OR r.branch_id=$3) \
          AND ($4::text IS NULL OR r.department_id=$4)",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(gate.branch_id.as_deref())
    .bind(gate.department_id.as_deref())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if visible.is_none() {
        return Err(talent_application_not_found());
    }
    Ok(Some(InboxTarget {
        href: Some(talent_application_href(source_id)),
        source: None,
    }))
}

/// Retained marketing proof: the Marketing module, the current
/// `records.read` role, the consented-candidate image, the marketing
/// history, then the scoped candidate read with its form join.
async fn marketing_target(
    c: &mut PgConnection,
    gate: &TalentGate,
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
        let module = match &assignment {
            Some((id, _)) => module_allows(c, id, "marketing").await?,
            None => false,
        };
        if !module {
            return Err(marketing_module_forbidden());
        }
        let (assignment_id, role) = match assignment {
            Some((id, role)) => (id, role),
            None => {
                return Err(marketing_access_changed(
                    "Choose a current Marketing work role.",
                ));
            }
        };
        if !role_allows(c, gate.company.as_str(), &assignment_id, "records.read").await? {
            return Err(marketing_access_changed(
                "Choose a current Marketing work role.",
            ));
        }
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
            return Err(marketing_access_changed(
                "The original Marketing assignment is no longer current.",
            ));
        }
        let image: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM ( \
              SELECT candidate.* FROM marketing_lead_candidates candidate \
              JOIN marketing_assets form ON form.id=candidate.form_id \
                AND form.company_id=candidate.company_id AND form.kind='form' \
                AND form.version>=candidate.form_version \
              JOIN marketing_asset_events captured ON captured.asset_id=form.id \
                AND captured.version=candidate.form_version AND captured.action='activate' \
                AND captured.status='active' \
              JOIN marketing_engagement_events submission ON submission.candidate_id=candidate.id \
                AND submission.version=1 AND submission.event_type='form_submit' \
              WHERE length(trim(candidate.consent_evidence))>=3 \
            ) candidate \
            WHERE id=$1 AND company_id=$2 \
            AND ($3::text IS NULL OR branch_id IS NULL OR branch_id=$3) \
            AND ($4::text IS NULL OR department_id IS NULL OR department_id=$4)",
        )
        .bind(source_id)
        .bind(gate.company.as_str())
        .bind(gate.branch_id.as_deref())
        .bind(gate.department_id.as_deref())
        .fetch_optional(&mut *c)
        .await
        .map_err(|_| storage())?;
        if image.is_none() {
            return Err(marketing_source_not_found());
        }
        if !history_allowed(c, &gate.company, "sales.marketing", gate.mode, gate.now).await? {
            return Err(marketing_package_required());
        }
    } else {
        let readable = match &assignment {
            Some((id, _)) => role_allows(c, gate.company.as_str(), id, "records.read").await?,
            None => false,
        };
        if !readable {
            return Err(permission_required());
        }
        let module = match &assignment {
            Some((id, _)) => module_allows(c, id, "marketing").await?,
            None => false,
        };
        if !module {
            return Err(marketing_module_forbidden());
        }
    }
    let visible: Option<String> = sqlx::query_scalar(
        "SELECT c.id FROM marketing_lead_candidates c \
          JOIN marketing_assets a ON a.id=c.form_id \
          WHERE c.id=$1 AND c.company_id=$2 \
          AND ($3::text IS NULL OR c.branch_id=$3) \
          AND ($4::text IS NULL OR c.department_id=$4)",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(gate.branch_id.as_deref())
    .bind(gate.department_id.as_deref())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if visible.is_none() {
        return Err(marketing_candidate_not_found());
    }
    Ok(Some(InboxTarget {
        href: Some(marketing_candidate_href(source_id)),
        source: None,
    }))
}

/// Retained manufacturing proof: the Manufacturing module, the current
/// `records.read` role, the order-chain image with its advanced flag, the
/// execution (plus advanced-MRP) history, then the scoped order read.
async fn manufacturing_target(
    c: &mut PgConnection,
    gate: &TalentGate,
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
        let module = match &assignment {
            Some((id, _)) => module_allows(c, id, "manufacturing").await?,
            None => false,
        };
        if !module {
            return Err(manufacturing_access_changed(
                manufacturing_module_message(),
            ));
        }
        let (assignment_id, role) = match assignment {
            Some((id, role)) => (id, role),
            None => {
                return Err(manufacturing_access_changed(
                    manufacturing_module_message(),
                ));
            }
        };
        if !role_allows(c, gate.company.as_str(), &assignment_id, "records.read").await? {
            return Err(manufacturing_access_changed(
                manufacturing_module_message(),
            ));
        }
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
            return Err(manufacturing_access_changed(
                "The original production assignment is no longer current.",
            ));
        }
        let advanced: Option<bool> = sqlx::query_scalar(
            "SELECT scenario.advanced FROM manufacturing_orders o \
              JOIN planning_recommendations r ON r.id=o.recommendation_id \
              JOIN (SELECT s.id,s.company_id,s.branch_id,s.warehouse_id,s.specification_id,s.specification_version, \
                (COALESCE(s.context_json::jsonb->>'comparisonScenarioId','')<>'' \
                  OR jsonb_exists(s.context_json::jsonb,'comparison')) advanced \
                FROM planning_scenarios s \
                LEFT JOIN planning_scenarios comparison \
                  ON comparison.id=s.context_json::jsonb->>'comparisonScenarioId' \
                  AND comparison.id=s.context_json::jsonb->'comparison'->>'scenarioId' \
                  AND comparison.company_id=s.company_id AND comparison.branch_id=s.branch_id \
                  AND comparison.specification_id=s.specification_id \
                  AND comparison.version>=(s.context_json::jsonb->'comparison'->>'sourceVersion')::bigint \
                WHERE (COALESCE(s.context_json::jsonb->>'comparisonScenarioId','')='' \
                  AND NOT(jsonb_exists(s.context_json::jsonb,'comparison'))) \
                  OR comparison.id IS NOT NULL) scenario ON scenario.id=r.scenario_id \
              JOIN manufacturing_specifications spec ON spec.id=o.specification_id \
                AND spec.company_id=o.company_id \
              JOIN commerce_warehouses w ON w.id=o.warehouse_id \
                AND w.company_id=o.company_id AND w.branch_id=o.branch_id \
              WHERE o.id=$1 AND o.company_id=$2 \
              AND ($3::text IS NULL OR o.branch_id=$3) \
              AND scenario.company_id=o.company_id AND scenario.branch_id=o.branch_id \
              AND scenario.warehouse_id=o.warehouse_id \
              AND scenario.specification_id=o.specification_id \
              AND scenario.specification_version=o.specification_version",
        )
        .bind(source_id)
        .bind(gate.company.as_str())
        .bind(gate.branch_id.as_deref())
        .fetch_optional(&mut *c)
        .await
        .map_err(|_| storage())?;
        let Some(advanced) = advanced else {
            return Err(production_source_not_found());
        };
        let mut capabilities = vec!["manufacturing.execution"];
        if advanced {
            capabilities.push("planning.advanced-mrp");
        }
        for capability in capabilities {
            if !history_allowed(c, &gate.company, capability, gate.mode, gate.now).await? {
                return Err(manufacturing_package_required());
            }
        }
    } else {
        let readable = match &assignment {
            Some((id, _)) => role_allows(c, gate.company.as_str(), id, "records.read").await?,
            None => false,
        };
        if !readable {
            return Err(permission_required());
        }
        let module = match &assignment {
            Some((id, _)) => module_allows(c, id, "manufacturing").await?,
            None => false,
        };
        if !module {
            return Err(manufacturing_module_forbidden());
        }
    }
    let visible: Option<String> = sqlx::query_scalar(
        "SELECT o.id FROM manufacturing_orders o \
          JOIN manufacturing_specifications s ON s.id=o.specification_id \
          JOIN commerce_products p ON p.id=s.output_product_id \
          WHERE o.id=$1 AND o.company_id=$2 \
          AND ($3::text IS NULL OR o.branch_id=$3)",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(gate.branch_id.as_deref())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if visible.is_none() {
        return Err(manufacturing_order_not_found());
    }
    Ok(Some(InboxTarget {
        href: Some(manufacturing_order_href(source_id)),
        source: None,
    }))
}

impl InboxTargetPort for TalentInboxTargets {
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
                None => enforcement_mode(source_type)?,
            };
            let now: i64 = sqlx::query_scalar(
                "SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint",
            )
            .fetch_one(&mut *connection)
            .await
            .map_err(|_| storage())?;
            let gate = TalentGate {
                company,
                user_id: user_id.to_owned(),
                assignment_id: self.assignment_id.clone(),
                branch_id: self.branch_id.clone(),
                department_id: self.department_id.clone(),
                mode,
                now,
            };
            match source_type {
                "talent_application" => talent_target(connection, &gate, source_id).await,
                "marketing_candidate" => marketing_target(connection, &gate, source_id).await,
                "manufacturing_order" => manufacturing_target(connection, &gate, source_id).await,
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
    fn routing_covers_only_the_talent_slice() {
        for source_type in [
            "talent_application",
            "marketing_candidate",
            "manufacturing_order",
        ] {
            assert!(is_talent_inbox_target(source_type), "{source_type}");
        }
        for other in [
            "notice",
            "collaboration_action",
            "calendar_appointment",
            "crm_lead",
            "commerce_order",
            "invoice",
            "payroll_correction",
            "roster_week",
            "support_ticket",
            "talent",
            "future_source",
            "",
        ] {
            assert!(!is_talent_inbox_target(other), "{other}");
        }
    }

    #[test]
    fn hrefs_encode_exactly_like_the_typescript_projection() {
        assert_eq!(talent_application_href("app-1"), "/talent/app-1");
        assert_eq!(talent_application_href("a/b c"), "/talent/a%2Fb%20c");
        assert_eq!(marketing_candidate_href("cand-1"), "/marketing/cand-1");
        assert_eq!(marketing_candidate_href("a/b c"), "/marketing/a%2Fb%20c");
        assert_eq!(manufacturing_order_href("ord-1"), "/manufacturing/ord-1");
        assert_eq!(manufacturing_order_href("a/b c"), "/manufacturing/a%2Fb%20c");
    }

    #[test]
    fn history_capabilities_match_the_retained_gates() {
        assert_eq!(
            talent_history_capabilities("talent_application"),
            &["recruit.candidates", "recruit.applications", "recruit.offers"]
        );
        assert_eq!(
            talent_history_capabilities("marketing_candidate"),
            &["sales.marketing"]
        );
        assert_eq!(
            talent_history_capabilities("manufacturing_order"),
            &["manufacturing.execution", "planning.advanced-mrp"]
        );
        assert_eq!(talent_history_capabilities("talent"), &[] as &[&str]);
        assert_eq!(talent_history_capabilities(""), &[] as &[&str]);
    }

    #[test]
    fn talent_kinds_keep_their_retained_labels() {
        assert_eq!(source_record_label("talent_application"), "Talent application");
        assert_eq!(
            source_record_label("marketing_candidate"),
            "Marketing candidate"
        );
        assert_eq!(
            source_record_label("manufacturing_order"),
            "Manufacturing order"
        );
    }

    #[test]
    fn oracle_denials_keep_their_exact_codes_kinds_and_messages() {
        let privacy = talent_privacy_forbidden();
        assert_eq!(privacy.kind, FailureKind::Forbidden);
        assert_eq!(privacy.code, "TALENT_PRIVACY_FORBIDDEN");
        assert_eq!(
            privacy.message,
            "Candidate data requires Talent reviewer access."
        );
        let changed = talent_access_changed();
        assert_eq!(changed.kind, FailureKind::Forbidden);
        assert_eq!(changed.code, "TALENT_ACCESS_CHANGED");
        let missing = talent_application_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "TALENT_APPLICATION_NOT_FOUND");
        assert_eq!(missing.message, "Application not found.");
        let changed =
            marketing_access_changed("Choose a current Marketing work role.");
        assert_eq!(changed.kind, FailureKind::Forbidden);
        assert_eq!(changed.code, "MARKETING_ACCESS_CHANGED");
        let source = marketing_source_not_found();
        assert_eq!(source.kind, FailureKind::NotFound);
        assert_eq!(source.code, "MARKETING_SOURCE_NOT_FOUND");
        assert_eq!(
            source.message,
            "The original Marketing source is unavailable in this workspace."
        );
        let package = marketing_package_required();
        assert_eq!(package.kind, FailureKind::Forbidden);
        assert_eq!(package.code, "PACKAGE_REQUIRED");
        assert_eq!(
            package.message,
            "The Marketing addon or retained history is required."
        );
        let missing = marketing_candidate_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "MARKETING_CANDIDATE_NOT_FOUND");
        assert_eq!(missing.message, "Lead candidate not found.");
        let module = manufacturing_module_forbidden();
        assert_eq!(module.kind, FailureKind::Forbidden);
        assert_eq!(module.code, "MODULE_FORBIDDEN");
        let source = production_source_not_found();
        assert_eq!(source.kind, FailureKind::NotFound);
        assert_eq!(source.code, "PRODUCTION_SOURCE_NOT_FOUND");
        let package = manufacturing_package_required();
        assert_eq!(package.kind, FailureKind::Forbidden);
        assert_eq!(package.code, "PACKAGE_REQUIRED");
        let missing = manufacturing_order_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "MANUFACTURING_ORDER_NOT_FOUND");
        assert_eq!(missing.message, "Order not found.");
    }
}
