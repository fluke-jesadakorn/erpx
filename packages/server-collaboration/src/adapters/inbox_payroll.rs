//! Native Payroll visibility delegate for the Inbox destination port.
//!
//! This mirrors the retained two-layer TypeScript oracle for the
//! notification-visible Payroll sources:
//!
//! - Layer 1 (`payrollNotificationTarget` in
//!   `packages/server-people_payroll/src/adapters/record-purchases.ts`):
//!   enforced package mode only. A correction resolves to its run when the
//!   correction row joins its run, the reader's frame is holding, a current
//!   `payroll.read` assignment exists, and the `people.payroll` history
//!   decision grants. An advance resolves to itself when the reader is the
//!   advance's person with a current assignment over the engagement scope, or
//!   a current holding `payroll.read` assignment exists, and the same history
//!   decision grants. Misses yield null, never an error.
//! - Layer 2 (the application `payrollCorrectionAlertTarget` /
//!   `payrollAdvanceAlertTarget` fallbacks, used exactly when layer 1 yields
//!   undefined, i.e. outside enforced mode): the correction resolves to its
//!   run for any current holding `payroll.read` assignment; the advance
//!   resolves for its person or any current holding `payroll.read`
//!   assignment. Misses yield null, never an error.
//!
//! The retained oracle defines no denial codes for these two source types:
//! every negative path is invisible (`Ok(None)`), and the per-row 403/404
//! catch never fires for them. Hrefs are byte-identical to the TypeScript
//! projection, including both encoded segments and the `#correction-` /
//! `#payroll-advance-` fragments. Neither branch sets a source override, so
//! the composition final map applies the retained defaults (`{kind, id,
//! href, label}` with `Payroll correction` / `Payroll advance` labels).
//!
//! Nothing here opens a pool, commits, or retries. It runs serializable-safe
//! SELECTs on the caller's connection.
//!
//! Faithful-mirror notes (documented, not silent): the JWT-static
//! `hasPermission` halves are enforced database-current via `iam_role_allows`
//! on the reader's assignment; purchase is evaluated through the native
//! Commercial decision port (`check`, never throwing — layer 1 maps a denied
//! decision to invisible exactly like the retained predicate); the
//! command-side guards authorize commands, not reads. `now` is the database
//! clock, matching the retained `packageClockSql` snapshots.
use crate::adapters::inbox_pg::{InboxTarget, InboxTargetFuture, InboxTargetPort};
use crate::adapters::inbox_targets::{
    encode_uri_component, retained_inbox_destination,
};
use erpx_server_commercial::public::capability_decision::PgCapabilityDecisionReader;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, current_package_catalogue,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use sqlx::{PgConnection, Row};

/// Source types this delegate can prove against Payroll rows.
pub fn is_payroll_inbox_target(source_type: &str) -> bool {
    matches!(source_type, "payroll_correction" | "payroll_advance")
}

/// Retained destination address for one correction, byte-identical to the
/// TypeScript
/// `` `/people/payroll/runs/${encodeURIComponent(runId)}#correction-${encodeURIComponent(id)}` ``.
pub fn payroll_correction_href(run_id: &str, source_id: &str) -> String {
    format!(
        "/people/payroll/runs/{}#correction-{}",
        encode_uri_component(run_id),
        encode_uri_component(source_id),
    )
}

/// Retained destination address for one advance, byte-identical to the
/// TypeScript
/// `` `/people/payroll/advances#payroll-advance-${encodeURIComponent(id)}` ``.
pub fn payroll_advance_href(advance_id: &str) -> String {
    format!(
        "/people/payroll/advances#payroll-advance-{}",
        encode_uri_component(advance_id),
    )
}

/// Layer selection: the retained port runs its scoped proof exactly in
/// enforced mode and defers to the application fallback otherwise (shadow
/// included, mirroring the `!=="enforced"` check).
pub fn uses_enforced_layer(mode: PackageEnforcementMode) -> bool {
    mode == PackageEnforcementMode::Enforced
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

/// Native Payroll visibility delegate, composed via
/// `PgInboxTargets::with_delegate`. Payroll-only: every other source type
/// defers to the existing fail-closed behavior instead of inventing a link.
///
/// `branch_id`/`department_id` carry the selected Work frame. The retained
/// correction proof requires a holding frame; the advance proof scopes the
/// frame against the engagement. Neither document read is record-scoped
/// beyond its Company.
pub struct PayrollInboxTargets {
    branch_id: Option<String>,
    department_id: Option<String>,
    assignment_id: Option<String>,
    enforcement: Option<PackageEnforcementMode>,
}

impl PayrollInboxTargets {
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

impl Default for PayrollInboxTargets {
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

/// Retained assignment half of the oracle: a current assignment for the
/// reader over the given scope. `permission` selects the `assignmentReadSql`
/// shape — `Some` checks the retained permission via `iam_role_allows`, `None`
/// checks bare assignment currency exactly like the permission-free arm.
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
            "AND iam_role_allows(g.role,g.company_id,{permission})",
            permission = sql_quote(permission),
        ),
        None => "AND EXISTS(SELECT 1 FROM iam_roles role \
          WHERE role.id=g.role AND role.company_id IN ('*',g.company_id) \
          AND role.status='active')"
            .to_owned(),
    };
    let authorized: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
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
    Ok(authorized.is_some())
}

/// Static permission names only; this never interpolates caller input.
fn sql_quote(value: &str) -> String {
    debug_assert!(value.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'.'));
    format!("'{value}'")
}

/// Retained purchase predicate: the `people.payroll` history decision. A
/// denied decision reads as invisible, exactly like the retained predicate
/// filtering the row out — layer 1 never throws.
async fn payroll_history(
    c: &mut PgConnection,
    company_id: &CompanyId,
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
            "people.payroll",
            CapabilityPurpose::History,
            mode,
            TimestampMillis::new(now),
        )
        .await
        .map_err(pivot)?;
    Ok(check.decision().is_some_and(|decision| decision.allowed))
}

/// Layer-1 correction proof: the correction row joins its run, the frame is
/// holding, a current `payroll.read` assignment exists, and history grants.
async fn layer1_correction(
    c: &mut PgConnection,
    company: &CompanyId,
    user_id: &str,
    assignment_id: Option<&str>,
    holding_frame: bool,
    mode: PackageEnforcementMode,
    now: i64,
    source_id: &str,
) -> Result<Option<String>, BusinessError> {
    let run_id: Option<String> = sqlx::query_scalar(
        "SELECT s.run_id FROM hr_payroll_corrections s \
          JOIN hr_payroll_runs r ON r.id=s.run_id AND r.company_id=s.company_id \
          WHERE s.id=$1 AND s.company_id=$2",
    )
    .bind(source_id)
    .bind(company.as_str())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    let Some(run_id) = run_id else {
        return Ok(None);
    };
    if !holding_frame {
        return Ok(None);
    }
    if !selected_assignment(
        c,
        company.as_str(),
        user_id,
        assignment_id,
        Some("payroll.read"),
        None,
        None,
        now,
    )
    .await?
    {
        return Ok(None);
    }
    if !payroll_history(c, company, mode, now).await? {
        return Ok(None);
    }
    Ok(Some(run_id))
}

/// Layer-1 advance proof: the person-or-privileged policy plus history.
async fn layer1_advance(
    c: &mut PgConnection,
    company: &CompanyId,
    user_id: &str,
    assignment_id: Option<&str>,
    branch_id: Option<&str>,
    department_id: Option<&str>,
    mode: PackageEnforcementMode,
    now: i64,
    source_id: &str,
) -> Result<Option<String>, BusinessError> {
    let row = sqlx::query(
        "SELECT s.id,payroll_engagement.branch_id,payroll_engagement.department_id,p.user_id \
          FROM hr_payroll_advances s \
          JOIN hr_engagements payroll_engagement \
            ON payroll_engagement.id=s.engagement_id \
            AND payroll_engagement.company_id=s.company_id \
          JOIN hr_people p ON p.id=s.person_id AND p.id=payroll_engagement.person_id \
            AND p.company_id=s.company_id \
          WHERE s.id=$1 AND s.company_id=$2",
    )
    .bind(source_id)
    .bind(company.as_str())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    let Some(row) = row else {
        return Ok(None);
    };
    let id: String = row.try_get("id").map_err(|_| storage())?;
    let engagement_branch: Option<String> =
        row.try_get("branch_id").map_err(|_| storage())?;
    let engagement_department: Option<String> =
        row.try_get("department_id").map_err(|_| storage())?;
    let person_user: Option<String> = row.try_get("user_id").map_err(|_| storage())?;
    let frame_matches = branch_id.is_none_or(|frame| {
        engagement_branch.as_deref().is_some_and(|branch| branch == frame)
    }) && department_id.is_none_or(|frame| {
        engagement_department
            .as_deref()
            .is_some_and(|department| department == frame)
    });
    // Self arm: the reader is the advance's person, the frame matches the
    // engagement scope, and a current assignment covers that scope.
    let is_self = person_user.as_deref() == Some(user_id);
    let self_visible = is_self
        && frame_matches
        && selected_assignment(
            c,
            company.as_str(),
            user_id,
            assignment_id,
            None,
            engagement_branch.as_deref(),
            engagement_department.as_deref(),
            now,
        )
        .await?;
    // Privileged arm: holding frame plus a current payroll.read assignment.
    let holding_frame = branch_id.is_none() && department_id.is_none();
    let privileged_visible = holding_frame
        && selected_assignment(
            c,
            company.as_str(),
            user_id,
            assignment_id,
            Some("payroll.read"),
            None,
            None,
            now,
        )
        .await?;
    if !(self_visible || privileged_visible) {
        return Ok(None);
    }
    if !payroll_history(c, company, mode, now).await? {
        return Ok(None);
    }
    Ok(Some(id))
}

/// Layer-2 correction fallback, literal mirror: any current holding
/// `payroll.read` assignment resolves the run. No principal check, exactly
/// like the retained query.
async fn layer2_correction(
    c: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    now: i64,
    source_id: &str,
) -> Result<Option<String>, BusinessError> {
    sqlx::query_scalar(
        "SELECT c.run_id FROM hr_payroll_corrections c \
          JOIN assignments a ON a.company_id=c.company_id AND a.user_id=$1 \
          WHERE c.id=$2 AND c.company_id=$3 \
          AND a.scope_kind='holding' AND a.revoked_at IS NULL \
          AND iam_assignment_active(a.id,$4) \
          AND iam_role_allows(a.role,a.company_id,'payroll.read') \
          LIMIT 1",
    )
    .bind(user_id)
    .bind(source_id)
    .bind(company_id)
    .bind(now)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())
}

/// Layer-2 advance fallback, literal mirror: the advance's person or any
/// current holding `payroll.read` assignment resolves the advance.
async fn layer2_advance(
    c: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    now: i64,
    source_id: &str,
) -> Result<Option<String>, BusinessError> {
    sqlx::query_scalar(
        "SELECT a.id FROM hr_payroll_advances a \
          JOIN hr_people p ON p.id=a.person_id \
          WHERE a.id=$1 AND a.company_id=$2 \
          AND (p.user_id=$3 OR EXISTS( \
            SELECT 1 FROM assignments x \
            WHERE x.company_id=a.company_id AND x.user_id=$4 \
            AND x.scope_kind='holding' AND x.revoked_at IS NULL \
            AND iam_assignment_active(x.id,$5) \
            AND iam_role_allows(x.role,x.company_id,'payroll.read'))) \
          LIMIT 1",
    )
    .bind(source_id)
    .bind(company_id)
    .bind(user_id)
    .bind(user_id)
    .bind(now)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())
}

impl InboxTargetPort for PayrollInboxTargets {
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
            let branch_id = self.branch_id.as_deref();
            let department_id = self.department_id.as_deref();
            let assignment_id = self.assignment_id.as_deref();
            match source_type {
                "payroll_correction" => {
                    let run_id = if uses_enforced_layer(mode) {
                        layer1_correction(
                            connection,
                            &company,
                            user_id,
                            assignment_id,
                            branch_id.is_none() && department_id.is_none(),
                            mode,
                            now,
                            source_id,
                        )
                        .await?
                    } else {
                        layer2_correction(connection, company.as_str(), user_id, now, source_id)
                            .await?
                    };
                    let Some(run_id) = run_id else {
                        return Ok(None);
                    };
                    // No source override: the composition final map applies
                    // the retained correction default.
                    Ok(Some(InboxTarget {
                        href: Some(payroll_correction_href(&run_id, source_id)),
                        source: None,
                    }))
                }
                "payroll_advance" => {
                    let advance_id = if uses_enforced_layer(mode) {
                        layer1_advance(
                            connection,
                            &company,
                            user_id,
                            assignment_id,
                            branch_id,
                            department_id,
                            mode,
                            now,
                            source_id,
                        )
                        .await?
                    } else {
                        layer2_advance(connection, company.as_str(), user_id, now, source_id)
                            .await?
                    };
                    let Some(advance_id) = advance_id else {
                        return Ok(None);
                    };
                    // No source override: the composition final map applies
                    // the retained advance default.
                    Ok(Some(InboxTarget {
                        href: Some(payroll_advance_href(&advance_id)),
                        source: None,
                    }))
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
    use crate::adapters::inbox_targets::source_record_label;

    #[test]
    fn routing_covers_only_the_payroll_slice() {
        assert!(is_payroll_inbox_target("payroll_correction"));
        assert!(is_payroll_inbox_target("payroll_advance"));
        for other in [
            "notice",
            "collaboration_action",
            "calendar_appointment",
            "crm_lead",
            "commerce_order",
            "invoice",
            "refund",
            "supplier_batch",
            "support_ticket",
            "payroll",
            "future_source",
            "",
        ] {
            assert!(!is_payroll_inbox_target(other), "{other}");
        }
    }

    #[test]
    fn hrefs_encode_both_segments_exactly_like_the_typescript_projection() {
        assert_eq!(
            payroll_correction_href("run-1", "cor-1"),
            "/people/payroll/runs/run-1#correction-cor-1"
        );
        assert_eq!(
            payroll_correction_href("seg/one two", "a b"),
            "/people/payroll/runs/seg%2Fone%20two#correction-a%20b"
        );
        assert_eq!(
            payroll_advance_href("adv-1"),
            "/people/payroll/advances#payroll-advance-adv-1"
        );
        assert_eq!(
            payroll_advance_href("seg/one two"),
            "/people/payroll/advances#payroll-advance-seg%2Fone%20two"
        );
        assert_eq!(
            payroll_advance_href("😀"),
            "/people/payroll/advances#payroll-advance-%F0%9F%98%80"
        );
    }

    #[test]
    fn enforced_mode_selects_the_scoped_layer() {
        assert!(uses_enforced_layer(PackageEnforcementMode::Enforced));
        assert!(!uses_enforced_layer(PackageEnforcementMode::Disabled));
        assert!(!uses_enforced_layer(PackageEnforcementMode::Shadow));
    }

    #[test]
    fn payroll_kinds_keep_their_retained_labels() {
        assert_eq!(source_record_label("payroll_correction"), "Payroll correction");
        assert_eq!(source_record_label("payroll_advance"), "Payroll advance");
    }
}
