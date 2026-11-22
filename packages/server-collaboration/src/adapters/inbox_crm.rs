//! Native CRM visibility delegate for the Inbox destination port.
//!
//! This mirrors the retained TypeScript `getCRMLead` read gate in
//! `packages/server-crm/src/adapters/leads.ts` (`requireCRMPurchasedRead`
//! plus the scoped `records` read) as invoked from the staff-notification
//! projection for `crm_lead` / `crm_activity`:
//! a `crm_lead` item is visible only when the workspace-scoped lead source
//! exists (lead row in the selected Work frame backed by its CRM account and
//! contact), the reader holds current `commerce.read` authority over the
//! lead's scope, and the `sales.crm` purchase history gate passes. A
//! `crm_activity` item first resolves its lead through the retained
//! `activityLead` read; a missing link stays invisible without an error,
//! exactly like the TypeScript `continue`.
//!
//! Error-code contract (the retained per-row catch in
//! `visibleNotifications` swallows 403/404 per item): the oracle's 404/403
//! denials surface here with the exact TypeScript codes
//! (`CRM_SOURCE_NOT_FOUND`, `CRM_LEAD_NOT_FOUND`, `CRM_ACCESS_CHANGED`,
//! `PACKAGE_REQUIRED`) so composition can drop those rows exactly like the
//! retained projection. Anything else fails the Inbox page.
//!
//! Nothing here opens a pool, commits, or retries. It runs serializable-safe
//! SELECTs on the caller's connection.
//!
//! Faithful-mirror gaps (documented, not silent): the finance-customer row
//! match and the quote/sale/reservation validity inside the retained
//! opportunity-availability proof belong to the Finance/Commerce/Hotel
//! owners, so this delegate enforces the CRM-owned necessary conditions
//! (opportunity row implies reviewed Finance linkage on the lead's account)
//! and leaves the cross-owner row matches to CRM-owner composition. The
//! command-side guards (`workContextGuards`, `iamMutationGuards`,
//! `nativeClearCommandAssertions`) authorize commands, not reads, and are not
//! part of this read-only port.
use crate::adapters::inbox_pg::{InboxTarget, InboxTargetFuture, InboxTargetPort};
use crate::adapters::inbox_targets::{
    encode_uri_component, retained_inbox_destination, source_record_label,
};
use erpx_server_commercial::public::capability_decision::{
    PgCapabilityDecisionReader, PurchasedAccess,
};
use erpx_server_commercial::public::purchases::{
    PackageEnforcementMode, current_package_catalogue,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use serde_json::json;
use sqlx::{PgConnection, Row};

/// Source types this delegate can prove against CRM rows.
pub fn is_crm_inbox_target(source_type: &str) -> bool {
    matches!(source_type, "crm_lead" | "crm_activity")
}

/// Retained destination address for one lead, byte-identical to the
/// TypeScript `` `/crm/${encodeURIComponent(id)}` ``.
pub fn crm_lead_href(source_id: &str) -> String {
    format!("/crm/{}", encode_uri_component(source_id))
}

/// Retained destination address for one activity, byte-identical to the
/// TypeScript `` `/crm/${encodeURIComponent(lead_id)}#crm-activities` ``.
pub fn crm_activity_href(lead_id: &str) -> String {
    format!("/crm/{}#crm-activities", encode_uri_component(lead_id))
}

/// Retained purchase-history gate: `access && !access.history` denies with
/// the exact TypeScript `PACKAGE_REQUIRED` refusal.
pub fn require_purchase_history(access: &PurchasedAccess) -> Result<(), BusinessError> {
    if !access.history {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "PACKAGE_REQUIRED",
            "Sales CRM or retained history is required.",
        ));
    }
    Ok(())
}

fn source_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CRM_SOURCE_NOT_FOUND",
        "The original CRM source is unavailable in this workspace.",
    )
}

fn lead_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CRM_LEAD_NOT_FOUND",
        "Lead not found in this scope.",
    )
}

fn access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "CRM_ACCESS_CHANGED",
        "The original sales assignment is no longer current.",
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

/// Native CRM visibility delegate, composed via
/// `PgInboxTargets::with_delegate`. CRM-only: every other source type defers
/// to the existing fail-closed behavior instead of inventing a link.
///
/// `branch_id`/`department_id` carry the selected Work frame used by the
/// retained lead scope; without them the scope check is skipped exactly like
/// the TypeScript `(?::text IS NULL OR ...)` arms.
pub struct CrmInboxTargets {
    branch_id: Option<String>,
    department_id: Option<String>,
    assignment_id: Option<String>,
    enforcement: Option<PackageEnforcementMode>,
}

impl CrmInboxTargets {
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
    /// retained environment-driven mode exactly like composition. This seam
    /// exists so tests and composition can select a mode deterministically;
    /// it never changes what the mode means.
    pub fn enforcement_mode(mut self, mode: PackageEnforcementMode) -> Self {
        self.enforcement = Some(mode);
        self
    }
}

impl Default for CrmInboxTargets {
    fn default() -> Self {
        Self::new(None, None)
    }
}

/// Retained `commercePackageEnforcementMode`, mirrored from composition:
/// disabled opens the read, enforced requires the local environment, and an
/// unknown value fails closed instead of bypassing the paywall.
fn enforcement_mode() -> Result<PackageEnforcementMode, BusinessError> {
    let misconfigured = || {
        BusinessError::new(
            FailureKind::Unavailable,
            "PACKAGE_ENFORCEMENT_CONFIGURATION",
            "Package enforcement is unavailable.",
        )
    };
    match std::env::var("ERPX_COMMERCE_PACKAGE_ENFORCEMENT")
        .unwrap_or_else(|_| "disabled".to_owned())
        .as_str()
    {
        "disabled" => Ok(PackageEnforcementMode::Disabled),
        "shadow" => Ok(PackageEnforcementMode::Shadow),
        "enforced" => {
            if std::env::var("ERPX_ENVIRONMENT").ok().as_deref() != Some("local") {
                return Err(misconfigured());
            }
            Ok(PackageEnforcementMode::Enforced)
        }
        _ => Err(misconfigured()),
    }
}

/// Retained `activityLead` read: the lead behind one activity, if any.
async fn activity_lead(
    c: &mut PgConnection,
    company_id: &str,
    source_id: &str,
) -> Result<Option<String>, BusinessError> {
    sqlx::query_scalar("SELECT lead_id FROM crm_lead_activities WHERE id=$1 AND company_id=$2")
        .bind(source_id)
        .bind(company_id)
        .fetch_optional(&mut *c)
        .await
        .map_err(|_| storage())
}

/// Retained `source()` lead scope: the lead row in the selected Work frame
/// backed by its CRM account and contact. Missing rows fail with the exact
/// TypeScript `CRM_SOURCE_NOT_FOUND` refusal.
async fn scoped_lead(
    c: &mut PgConnection,
    company_id: &str,
    branch_id: Option<&str>,
    department_id: Option<&str>,
    lead_id: &str,
) -> Result<(String, String, Option<String>), BusinessError> {
    sqlx::query(
        "SELECT l.id,l.branch_id,l.department_id FROM crm_leads l \
          JOIN crm_accounts a ON a.id=l.account_id AND a.company_id=l.company_id \
          JOIN crm_contacts ct ON ct.id=l.contact_id AND ct.account_id=a.id \
          AND ct.company_id=l.company_id \
          WHERE l.id=$1 AND l.company_id=$2 \
          AND ($3::text IS NULL OR l.branch_id=$3) \
          AND ($4::text IS NULL OR l.department_id=$4)",
    )
    .bind(lead_id)
    .bind(company_id)
    .bind(branch_id)
    .bind(department_id)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?
    .map(|row| {
        let id: String = row.try_get("id").map_err(|_| storage())?;
        let branch: String = row.try_get("branch_id").map_err(|_| storage())?;
        let department: Option<String> =
            row.try_get("department_id").map_err(|_| storage())?;
        Ok((id, branch, department))
    })
    .transpose()?
    .ok_or_else(source_not_found)
}

/// Retained opportunity-availability rule, CRM-owned necessary conditions: a
/// lead carrying an opportunity must keep reviewed Finance linkage on its
/// account. The Finance-customer row match and the quote/sale/reservation
/// validity belong to their owners and stay unproven here (see module docs).
async fn opportunity_available(
    c: &mut PgConnection,
    company_id: &str,
    lead_id: &str,
) -> Result<(), BusinessError> {
    let has_opportunity: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM crm_opportunities o WHERE o.lead_id=$1)",
    )
    .bind(lead_id)
    .fetch_one(&mut *c)
    .await
    .map_err(|_| storage())?;
    if !has_opportunity {
        return Ok(());
    }
    let linked: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM crm_leads l \
          JOIN crm_accounts a ON a.id=l.account_id AND a.company_id=l.company_id \
          WHERE l.id=$1 AND l.company_id=$2 \
          AND a.finance_party_id IS NOT NULL AND a.reviewed_by IS NOT NULL)",
    )
    .bind(lead_id)
    .bind(company_id)
    .fetch_one(&mut *c)
    .await
    .map_err(|_| storage())?;
    if !linked {
        return Err(source_not_found());
    }
    Ok(())
}

/// Retained `requireCRMCurrentAuthority` for `commerce.read`: the reader's
/// current assignment must allow the permission over the lead's own scope.
/// This is the database-current half of the oracle; the static JWT
/// `hasPermission` check is subsumed by `iam_role_allows` on that assignment.
async fn current_authority(
    c: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    assignment_id: Option<&str>,
    now: i64,
    record_branch_id: &str,
    record_department_id: Option<&str>,
) -> Result<(), BusinessError> {
    let authorized: Option<String> = sqlx::query_scalar(
        "SELECT g.id FROM assignments g \
          WHERE g.company_id=$1 AND g.user_id=$2 AND ($3::text IS NULL OR g.id=$3) \
          AND g.revoked_at IS NULL \
          AND g.valid_from<=$4 AND g.valid_until>$4 \
          AND iam_assignment_active(g.id,$4) \
          AND iam_principal_active(g.user_id,g.company_id,$4) \
          AND iam_role_allows(g.role,g.company_id,'commerce.read') \
          AND (g.scope_kind='holding' \
            OR (g.scope_kind='branch' AND g.scope_node_id=$5) \
            OR (g.scope_kind='department' AND g.scope_node_id=$6)) \
          ORDER BY g.version DESC LIMIT 1",
    )
    .bind(company_id)
    .bind(user_id)
    .bind(assignment_id)
    .bind(now)
    .bind(record_branch_id)
    .bind(record_department_id)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if authorized.is_none() {
        return Err(access_changed());
    }
    Ok(())
}

/// Retained `requireCRMPurchasedRead` history gate through the native
/// Commercial capability port. Disabled enforcement opens the read.
async fn purchase_history(
    c: &mut PgConnection,
    company_id: &CompanyId,
    mode: PackageEnforcementMode,
    now: i64,
) -> Result<(), BusinessError> {
    if mode == PackageEnforcementMode::Disabled {
        return Ok(());
    }
    let catalogue = current_package_catalogue()?;
    let mut reader = PgCapabilityDecisionReader::new(company_id.clone(), c, &catalogue);
    let access = reader
        .access("sales.crm", mode, TimestampMillis::new(now))
        .await
        .map_err(pivot)?;
    require_purchase_history(&access)
}

/// Full retained `getCRMLead` proof for one lead: scoped source, opportunity
/// rule, current authority, purchase history, then the final identity read.
async fn prove_lead(
    c: &mut PgConnection,
    company: &CompanyId,
    user_id: &str,
    assignment_id: Option<&str>,
    branch_id: Option<&str>,
    department_id: Option<&str>,
    mode: PackageEnforcementMode,
    now: i64,
    lead_id: &str,
) -> Result<(), BusinessError> {
    let (_, record_branch, record_department) = scoped_lead(
        c,
        company.as_str(),
        branch_id,
        department_id,
        lead_id,
    )
    .await?;
    opportunity_available(c, company.as_str(), lead_id).await?;
    current_authority(
        c,
        company.as_str(),
        user_id,
        assignment_id,
        now,
        &record_branch,
        record_department.as_deref(),
    )
    .await?;
    purchase_history(c, company, mode, now).await?;
    // Retained `getLead` tail: the proven identity must still exist.
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM crm_leads WHERE id=$1 AND company_id=$2)",
    )
    .bind(lead_id)
    .bind(company.as_str())
    .fetch_one(&mut *c)
    .await
    .map_err(|_| storage())?;
    if !exists {
        return Err(lead_not_found());
    }
    Ok(())
}

impl InboxTargetPort for CrmInboxTargets {
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
            match source_type {
                "crm_lead" => {
                    prove_lead(
                        connection,
                        &company,
                        user_id,
                        self.assignment_id.as_deref(),
                        self.branch_id.as_deref(),
                        self.department_id.as_deref(),
                        mode,
                        now,
                        source_id,
                    )
                    .await?;
                    // The retained projection sets no version on CRM sources.
                    let href = crm_lead_href(source_id);
                    Ok(Some(InboxTarget {
                        href: Some(href.clone()),
                        source: Some(json!({
                            "kind": "crm_lead",
                            "id": source_id,
                            "href": href,
                            "label": source_record_label("crm_lead"),
                        })),
                    }))
                }
                "crm_activity" => {
                    let Some(lead_id) =
                        activity_lead(connection, company.as_str(), source_id).await?
                    else {
                        return Ok(None);
                    };
                    prove_lead(
                        connection,
                        &company,
                        user_id,
                        self.assignment_id.as_deref(),
                        self.branch_id.as_deref(),
                        self.department_id.as_deref(),
                        mode,
                        now,
                        &lead_id,
                    )
                    .await?;
                    let href = crm_activity_href(&lead_id);
                    Ok(Some(InboxTarget {
                        href: Some(href.clone()),
                        source: Some(json!({
                            "kind": "crm_activity",
                            "id": source_id,
                            "href": href,
                            "label": source_record_label("crm_activity"),
                        })),
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

    fn history_access(history: bool) -> PurchasedAccess {
        PurchasedAccess {
            new_work: false,
            settle_existing: false,
            history,
            revision: String::new(),
            codes: erpx_server_commercial::public::capability_decision::PurchasedAccessCodes {
                new_work:
                    erpx_server_commercial::public::purchases::PackageDecisionCode::PackageRequired,
                settle_existing:
                    erpx_server_commercial::public::purchases::PackageDecisionCode::PackageRequired,
                history:
                    erpx_server_commercial::public::purchases::PackageDecisionCode::PackageRequired,
            },
        }
    }

    #[test]
    fn routing_covers_only_the_crm_slice() {
        assert!(is_crm_inbox_target("crm_lead"));
        assert!(is_crm_inbox_target("crm_activity"));
        for other in [
            "notice",
            "collaboration_action",
            "calendar_appointment",
            "support_ticket",
            "crm",
            "future_source",
            "",
        ] {
            assert!(!is_crm_inbox_target(other), "{other}");
        }
    }

    #[test]
    fn hrefs_encode_exactly_like_the_typescript_projection() {
        assert_eq!(crm_lead_href("lead-1"), "/crm/lead-1");
        assert_eq!(crm_lead_href("seg/one two"), "/crm/seg%2Fone%20two");
        assert_eq!(crm_activity_href("lead-1"), "/crm/lead-1#crm-activities");
        assert_eq!(
            crm_activity_href("seg/one two"),
            "/crm/seg%2Fone%20two#crm-activities"
        );
        assert_eq!(crm_activity_href("😀"), "/crm/%F0%9F%98%80#crm-activities");
    }

    #[test]
    fn crm_kinds_keep_their_retained_labels() {
        assert_eq!(source_record_label("crm_lead"), "Crm lead");
        assert_eq!(source_record_label("crm_activity"), "Crm activity");
    }

    #[test]
    fn missing_purchase_history_denies_with_the_retained_code() {
        let denied = require_purchase_history(&history_access(false)).unwrap_err();
        assert_eq!(denied.kind, FailureKind::Forbidden);
        assert_eq!(denied.code, "PACKAGE_REQUIRED");
        assert!(require_purchase_history(&history_access(true)).is_ok());
    }

    #[test]
    fn oracle_denials_keep_their_exact_codes_and_kinds() {
        let not_found = source_not_found();
        assert_eq!(not_found.kind, FailureKind::NotFound);
        assert_eq!(not_found.code, "CRM_SOURCE_NOT_FOUND");
        let gone = lead_not_found();
        assert_eq!(gone.kind, FailureKind::NotFound);
        assert_eq!(gone.code, "CRM_LEAD_NOT_FOUND");
        let changed = access_changed();
        assert_eq!(changed.kind, FailureKind::Forbidden);
        assert_eq!(changed.code, "CRM_ACCESS_CHANGED");
    }
}
