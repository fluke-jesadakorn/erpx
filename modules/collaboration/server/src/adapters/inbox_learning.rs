//! Native Learning and Benefits visibility delegate for the Inbox
//! destination port.
//!
//! This mirrors the retained per-row TypeScript gates for the
//! notification-visible Learning and Benefits sources:
//!
//! - `learning_assignment`: the readable check (`records.read` plus the
//!   Learning module), the current Learning assignment, the `learn`
//!   history, then the scoped assignment read with its course join; with
//!   enforcement off the readable check authorizes the read directly;
//!   403 `PERMISSION_REQUIRED` without the role, 403 `MODULE_FORBIDDEN`
//!   without the module, 403 `LEARNING_ACCESS_CHANGED` without a current
//!   assignment, 403 `PACKAGE_REQUIRED` without history, 404
//!   `LEARNING_ASSIGNMENT_NOT_FOUND` when the row is missing. No source
//!   override.
//! - `benefit_election`: the Benefits module, the current People reader,
//!   the assignment module predicate, the `people.benefits` history, then
//!   the scoped election read behind the reader's person (or any election
//!   for a benefits reviewer); with enforcement off the readable check
//!   authorizes the read directly; 403 `MODULE_FORBIDDEN` without the
//!   module, 403 `PERMISSION_REQUIRED` without the role, 403
//!   `PEOPLE_ACCESS_CHANGED` without a current assignment, 403
//!   `BENEFIT_ACCESS_CHANGED` when the assignment drops Benefits, 403
//!   `PACKAGE_REQUIRED` without history, 404
//!   `BENEFIT_ELECTION_NOT_FOUND` when the row is missing. No source
//!   override.
//!
//! Gate order matches each oracle exactly, including the disabled-mode
//! shape where the purchase path is skipped.
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
//!   effects, live authority reloads) authorize commands, not reads, and
//!   are not part of this read-only port.
//! - Presentation masking and facts (result-gated columns, history,
//!   handoffs, certificates, payroll inputs, owners, eligibility) are not
//!   visibility: rows passing the identity proof resolve without them.
use crate::adapters::inbox_pg::{InboxTarget, InboxTargetFuture, InboxTargetPort};
use crate::adapters::inbox_targets::{encode_uri_component, retained_inbox_destination};
use erpx_commercial_server::public::capability_decision::PgCapabilityDecisionReader;
use erpx_commercial_server::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, current_package_catalogue,
};
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use sqlx::{PgConnection, Row};

/// Source types this delegate can prove against Learning and Benefits rows.
pub fn is_learning_inbox_target(source_type: &str) -> bool {
    matches!(source_type, "learning_assignment" | "benefit_election")
}

/// Retained destination address for one learning assignment,
/// byte-identical to the TypeScript `` `/learning/${encodeURIComponent(id)}` ``.
pub fn learning_assignment_href(source_id: &str) -> String {
    format!("/learning/{}", encode_uri_component(source_id))
}

/// Retained destination address for one benefit election, byte-identical to
/// the TypeScript `` `/benefits/${encodeURIComponent(id)}` ``.
pub fn benefit_election_href(source_id: &str) -> String {
    format!("/benefits/{}", encode_uri_component(source_id))
}

/// Retained read capabilities per source type.
pub fn learning_history_capabilities(source_type: &str) -> &'static [&'static str] {
    match source_type {
        "learning_assignment" => &["learn.assignments"],
        "benefit_election" => &["people.benefits"],
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

fn learning_module_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "MODULE_FORBIDDEN",
        "Your current role cannot access Learning.",
    )
}

fn learning_access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "LEARNING_ACCESS_CHANGED",
        "Choose a current Learning assignment.",
    )
}

fn learning_assignment_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "LEARNING_ASSIGNMENT_NOT_FOUND",
        "Learning assignment not found.",
    )
}

fn learning_package_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PACKAGE_REQUIRED",
        "This Learning work requires its purchased capability or retained history.",
    )
}

fn benefits_module_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "MODULE_FORBIDDEN",
        "Your current role cannot access Benefits.",
    )
}

fn people_access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PEOPLE_ACCESS_CHANGED",
        "Choose a current authorized People work role.",
    )
}

fn benefit_access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "BENEFIT_ACCESS_CHANGED",
        "The current assignment no longer includes Benefits.",
    )
}

fn benefit_package_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PACKAGE_REQUIRED",
        "This Benefits work requires its purchased capability or retained history.",
    )
}

fn benefit_election_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "BENEFIT_ELECTION_NOT_FOUND",
        "Election not found.",
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

/// Native Learning and Benefits visibility delegate, composed via
/// `PgInboxTargets::with_delegate`. Learning-only: every other source type
/// defers to the existing fail-closed behavior instead of inventing a link.
pub struct LearningInboxTargets {
    branch_id: Option<String>,
    department_id: Option<String>,
    assignment_id: Option<String>,
    enforcement: Option<PackageEnforcementMode>,
}

impl LearningInboxTargets {
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
    /// retained owner environment-driven mode exactly (learning and people
    /// keys per source). This seam exists so tests and composition can
    /// select a mode deterministically; it never changes what the mode
    /// means.
    pub fn enforcement_mode(mut self, mode: PackageEnforcementMode) -> Self {
        self.enforcement = Some(mode);
        self
    }
}

impl Default for LearningInboxTargets {
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
        "learning_assignment" => "ERPX_LEARNING_PACKAGE_ENFORCEMENT",
        "benefit_election" => "ERPX_PEOPLE_PACKAGE_ENFORCEMENT",
        _ => "ERPX_PEOPLE_PACKAGE_ENFORCEMENT",
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

struct LearningGate {
    company: CompanyId,
    user_id: String,
    assignment_id: Option<String>,
    branch_id: Option<String>,
    department_id: Option<String>,
    mode: PackageEnforcementMode,
    now: i64,
}

impl LearningGate {
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

/// Retained learning proof: the readable check, the current Learning
/// assignment with its module, the `learn` history, then the scoped
/// assignment read with its course join.
async fn learning_target(
    c: &mut PgConnection,
    gate: &LearningGate,
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
        let (assignment_id, role) = match &assignment {
            Some((id, role)) => (id.clone(), role.clone()),
            None => return Err(learning_access_changed()),
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
            return Err(learning_access_changed());
        }
        if !history_allowed(c, &gate.company, "learn.assignments", gate.mode, gate.now).await? {
            return Err(learning_package_required());
        }
        if !module_allows(c, &assignment_id, "learning").await? {
            return Err(learning_module_forbidden());
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
            Some((id, _)) => module_allows(c, id, "learning").await?,
            None => false,
        };
        if !module {
            return Err(learning_module_forbidden());
        }
    }
    let visible: Option<String> = sqlx::query_scalar(
        "SELECT a.id FROM learning_assignments a \
          JOIN learning_courses c ON c.id=a.course_id \
          WHERE a.id=$1 AND a.company_id=$2 \
          AND ($3::text IS NULL OR a.branch_id=$3) \
          AND ($4::text IS NULL OR a.department_id=$4)",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(gate.branch_id.as_deref())
    .bind(gate.department_id.as_deref())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if visible.is_none() {
        return Err(learning_assignment_not_found());
    }
    Ok(Some(InboxTarget {
        href: Some(learning_assignment_href(source_id)),
        source: None,
    }))
}

/// The reader's active benefit person, mirror of `activeBenefitPerson`.
async fn benefit_person(
    c: &mut PgConnection,
    company_id: &str,
    user_id: &str,
) -> Result<Option<String>, BusinessError> {
    sqlx::query_scalar(
        "SELECT p.id FROM hr_people p \
          JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
          WHERE p.company_id=$1 AND p.user_id=$2 \
          AND p.status='active' AND p.merged_into_id IS NULL \
          AND e.status='active' LIMIT 1",
    )
    .bind(company_id)
    .bind(user_id)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())
}

/// Retained assignment module predicate for Benefits, mirror of
/// `benefitAssignmentModulePredicate`: custom roles prove the module
/// permission, other roles prove an empty or Benefits-selecting module
/// set.
async fn benefit_module_selected(
    c: &mut PgConnection,
    assignment_id: &str,
) -> Result<bool, BusinessError> {
    sqlx::query_scalar(
        "SELECT ((selected.role LIKE 'custom_%' \
          AND iam_role_allows(selected.role,selected.company_id,'module.benefits')) \
        OR (selected.role NOT LIKE 'custom_%' \
          AND (jsonb_array_length(selected.modules_json::jsonb)=0 \
            OR selected.modules_json::jsonb @> jsonb_build_array('benefits'::text)))) \
        FROM assignments selected WHERE selected.id=$1",
    )
    .bind(assignment_id)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())
    .map(|hit| hit.unwrap_or(false))
}

/// Retained benefit proof: the Benefits module, the current People reader,
/// the assignment module predicate, the `people.benefits` history, then
/// the scoped election read behind the reader's person (or any election
/// for a benefits reviewer).
async fn benefit_target(
    c: &mut PgConnection,
    gate: &LearningGate,
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
            Some((id, _)) => module_allows(c, id, "benefits").await?,
            None => false,
        };
        if !module {
            return Err(benefits_module_forbidden());
        }
        let readable = match &assignment {
            Some((id, _)) => role_allows(c, gate.company.as_str(), id, "records.read").await?,
            None => false,
        };
        if !readable {
            return Err(permission_required());
        }
        let (assignment_id, role) = match &assignment {
            Some((id, role)) => (id.clone(), role.clone()),
            None => return Err(people_access_changed()),
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
            return Err(people_access_changed());
        }
        if !benefit_module_selected(c, &assignment_id).await? {
            return Err(benefit_access_changed());
        }
        if !history_allowed(c, &gate.company, "people.benefits", gate.mode, gate.now).await? {
            return Err(benefit_package_required());
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
            Some((id, _)) => module_allows(c, id, "benefits").await?,
            None => false,
        };
        if !module {
            return Err(benefits_module_forbidden());
        }
    }
    let reviewer = match &assignment {
        Some((id, _)) => role_allows(c, gate.company.as_str(), id, "benefits.review").await?,
        None => false,
    };
    let person_id = benefit_person(c, gate.company.as_str(), &gate.user_id).await?;
    let visible: Option<String> = sqlx::query_scalar(
        "SELECT e.id FROM benefit_elections e \
          JOIN benefit_plans p ON p.id=e.plan_id \
          WHERE e.id=$1 AND e.company_id=$2 \
          AND ($3::boolean OR e.person_id=$4)",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(reviewer)
    .bind(person_id.as_deref())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if visible.is_none() {
        return Err(benefit_election_not_found());
    }
    Ok(Some(InboxTarget {
        href: Some(benefit_election_href(source_id)),
        source: None,
    }))
}

impl InboxTargetPort for LearningInboxTargets {
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
            let gate = LearningGate {
                company,
                user_id: user_id.to_owned(),
                assignment_id: self.assignment_id.clone(),
                branch_id: self.branch_id.clone(),
                department_id: self.department_id.clone(),
                mode,
                now,
            };
            match source_type {
                "learning_assignment" => learning_target(connection, &gate, source_id).await,
                "benefit_election" => benefit_target(connection, &gate, source_id).await,
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
    fn routing_covers_only_the_learning_slice() {
        for source_type in ["learning_assignment", "benefit_election"] {
            assert!(is_learning_inbox_target(source_type), "{source_type}");
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
            "learning",
            "future_source",
            "",
        ] {
            assert!(!is_learning_inbox_target(other), "{other}");
        }
    }

    #[test]
    fn hrefs_encode_exactly_like_the_typescript_projection() {
        assert_eq!(learning_assignment_href("asg-1"), "/learning/asg-1");
        assert_eq!(learning_assignment_href("a/b c"), "/learning/a%2Fb%20c");
        assert_eq!(benefit_election_href("el-1"), "/benefits/el-1");
        assert_eq!(benefit_election_href("a/b c"), "/benefits/a%2Fb%20c");
    }

    #[test]
    fn history_capabilities_match_the_retained_gates() {
        assert_eq!(
            learning_history_capabilities("learning_assignment"),
            &["learn.assignments"]
        );
        assert_eq!(
            learning_history_capabilities("benefit_election"),
            &["people.benefits"]
        );
        assert_eq!(learning_history_capabilities("learning"), &[] as &[&str]);
        assert_eq!(learning_history_capabilities(""), &[] as &[&str]);
    }

    #[test]
    fn learning_kinds_keep_their_retained_labels() {
        assert_eq!(source_record_label("learning_assignment"), "Learning assignment");
        assert_eq!(source_record_label("benefit_election"), "Benefit election");
    }

    #[test]
    fn oracle_denials_keep_their_exact_codes_kinds_and_messages() {
        let scope = learning_access_changed();
        assert_eq!(scope.kind, FailureKind::Forbidden);
        assert_eq!(scope.code, "LEARNING_ACCESS_CHANGED");
        let missing = learning_assignment_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "LEARNING_ASSIGNMENT_NOT_FOUND");
        assert_eq!(missing.message, "Learning assignment not found.");
        let package = learning_package_required();
        assert_eq!(package.kind, FailureKind::Forbidden);
        assert_eq!(package.code, "PACKAGE_REQUIRED");
        let changed = people_access_changed();
        assert_eq!(changed.kind, FailureKind::Forbidden);
        assert_eq!(changed.code, "PEOPLE_ACCESS_CHANGED");
        let changed = benefit_access_changed();
        assert_eq!(changed.kind, FailureKind::Forbidden);
        assert_eq!(changed.code, "BENEFIT_ACCESS_CHANGED");
        let package = benefit_package_required();
        assert_eq!(package.kind, FailureKind::Forbidden);
        assert_eq!(package.code, "PACKAGE_REQUIRED");
        let missing = benefit_election_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "BENEFIT_ELECTION_NOT_FOUND");
        assert_eq!(missing.message, "Election not found.");
    }
}
