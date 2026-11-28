//! Native Customer visibility delegate for the Inbox destination port.
//!
//! This mirrors the retained per-row TypeScript gates for the
//! notification-visible Customer slice:
//!
//! - `customer_identity`: `customerIdentityDetail` — the `commerce.read`
//!   and `commerce.sales` roles, then the company row read (with its
//!   customer join); 403 `PERMISSION_REQUIRED` without a role, 404
//!   `CUSTOMER_IDENTITY_CHANGE_NOT_FOUND`
//!   ("Identity change not found in this Company.") when the row is
//!   missing. No source override.
//! - `customer_duplicate`: `customerDuplicateDetail` — the same two roles,
//!   then the company review read (with both customer joins); 403
//!   `PERMISSION_REQUIRED` without a role, 404
//!   `CUSTOMER_DUPLICATE_NOT_FOUND`
//!   ("Duplicate review not found in this Company.") when the row is
//!   missing. No source override.
//! - `customer_access`: `customerAccessBinding` — the same two roles, then
//!   the purchase path (queued authority, the scoped binding/order source,
//!   the `sales.portal` history), then the scoped binding read; 403
//!   `PERMISSION_REQUIRED` without a role, 403 with the purchase decision
//!   code without history, 404 `CUSTOMER_ACCESS_NOT_FOUND`
//!   ("Customer access request not found in this scope.") when the row is
//!   missing. No source override.
//!
//! Gate order matches each oracle exactly, including the disabled-mode
//! shapes where the purchase path is skipped.
//!
//! Nothing here opens a pool, commits, or retries. It runs serializable-safe
//! SELECTs on the caller's connection.
//!
//! Faithful-mirror notes (documented, not silent):
//! - The JWT-static `requirePermission` halves are enforced
//!   database-current via `iam_role_allows` on the resolved assignment. A
//!   stale claim fails closed with `PERMISSION_REQUIRED` before the oracle's
//!   later authority arms are reached.
//! - The queued-command authority (assignment version pins, revoked
//!   sessions, member versions, emergency windows, active work contexts) is
//!   not re-proven; the assignment currency, principal, role-active and
//!   scope proof stands in for it. The authority denial therefore surfaces
//!   as the retained `PERMISSION_REQUIRED` instead of the oracle's
//!   `QUEUED_AUTHORITY_CHANGED`: the DB-current permission proof subsumes
//!   the queued arm on this read-only port.
//! - The portal-release scheduled-feature predicate is effectively `TRUE`
//!   outside production (the oracle short-circuits there); the production
//!   rollout check is not re-proven. Seeds run with the predicate true.
//! - The identity/duplicate `actor_eligible` manager arms only feed
//!   `canReview` presentation mapping, never visibility: rows are found by
//!   id in the company rows, exactly like the oracle's `find`.
//! - Presentation joins and facts (customer names, histories, eligibility,
//!   purchase controls, threads, reads) are not visibility: rows passing
//!   the identity proof resolve without them.
use crate::adapters::inbox_pg::{InboxTarget, InboxTargetFuture, InboxTargetPort};
use crate::adapters::inbox_targets::{encode_uri_component, retained_inbox_destination};
use erpx_commercial_server::public::capability_decision::PgCapabilityDecisionReader;
use erpx_commercial_server::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, current_package_catalogue,
};
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use sqlx::PgConnection;

/// Source types this delegate can prove against Customer rows.
pub fn is_customer_inbox_target(source_type: &str) -> bool {
    matches!(
        source_type,
        "customer_identity" | "customer_duplicate" | "customer_access"
    )
}

/// Retained destination address for one identity change, byte-identical to
/// the TypeScript `` `/customer-identity/${encodeURIComponent(id)}` ``.
pub fn customer_identity_href(source_id: &str) -> String {
    format!("/customer-identity/{}", encode_uri_component(source_id))
}

/// Retained destination address for one duplicate review, byte-identical to
/// the TypeScript `` `/customer-duplicates/${encodeURIComponent(id)}` ``.
pub fn customer_duplicate_href(source_id: &str) -> String {
    format!("/customer-duplicates/{}", encode_uri_component(source_id))
}

/// Retained destination address for one access binding, byte-identical to
/// the TypeScript `` `/customer-access/${encodeURIComponent(id)}` ``.
pub fn customer_access_href(source_id: &str) -> String {
    format!("/customer-access/{}", encode_uri_component(source_id))
}

/// Retained read capabilities per source type. Identity and duplicate reads
/// settle no commercial capability (the oracles have no purchase gate);
/// access settles the portal history.
pub fn customer_history_capabilities(source_type: &str) -> &'static [&'static str] {
    match source_type {
        "customer_identity" | "customer_duplicate" => &[],
        "customer_access" => &["sales.portal"],
        _ => &[],
    }
}

/// Retained database-current permissions behind each customer read. Every
/// oracle requires both `commerce.read` and `commerce.sales` before its row
/// proof.
pub fn customer_required_permissions(source_type: &str) -> &'static [&'static str] {
    match source_type {
        "customer_identity" | "customer_duplicate" | "customer_access" => {
            &["commerce.read", "commerce.sales"]
        }
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

fn identity_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CUSTOMER_IDENTITY_CHANGE_NOT_FOUND",
        "Identity change not found in this Company.",
    )
}

fn duplicate_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CUSTOMER_DUPLICATE_NOT_FOUND",
        "Duplicate review not found in this Company.",
    )
}

fn access_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CUSTOMER_ACCESS_NOT_FOUND",
        "Customer access request not found in this scope.",
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

/// Native Customer visibility delegate, composed via
/// `PgInboxTargets::with_delegate`. Customer-only: every other source type
/// defers to the existing fail-closed behavior instead of inventing a link.
pub struct CustomerInboxTargets {
    branch_id: Option<String>,
    department_id: Option<String>,
    assignment_id: Option<String>,
    enforcement: Option<PackageEnforcementMode>,
}

impl CustomerInboxTargets {
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
    /// retained commerce environment-driven mode exactly. This seam exists
    /// so tests and composition can select a mode deterministically; it never
    /// changes what the mode means.
    pub fn enforcement_mode(mut self, mode: PackageEnforcementMode) -> Self {
        self.enforcement = Some(mode);
        self
    }
}

impl Default for CustomerInboxTargets {
    fn default() -> Self {
        Self::new(None, None)
    }
}

/// Retained environment-driven enforcement mode, mirrored from the
/// TypeScript commerce oracles: disabled opens the read, a non-local
/// environment fails with the configuration code, and an unknown value
/// fails closed with the configuration code.
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

struct CustomerGate {
    company: CompanyId,
    user_id: String,
    assignment_id: Option<String>,
    branch_id: Option<String>,
    department_id: Option<String>,
    mode: PackageEnforcementMode,
    now: i64,
}

impl CustomerGate {
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

/// Retained role proof: the resolved assignment allows every retained
/// permission. Static permission names only; this never interpolates caller
/// input.
async fn permissions_require(
    c: &mut PgConnection,
    gate: &CustomerGate,
    source_type: &str,
) -> Result<(), BusinessError> {
    let assignment = resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?;
    let assignment_id = match assignment {
        Some((id, _)) => id,
        None => return Err(permission_required()),
    };
    for permission in customer_required_permissions(source_type) {
        if !role_allows(c, gate.company.as_str(), &assignment_id, permission).await? {
            return Err(permission_required());
        }
    }
    Ok(())
}

/// Retained assignment currency without a permission check: the resolved
/// assignment is current with the caller and role-active. This stands in
/// for the queued-command authority (documented gap).
async fn assignment_current(
    c: &mut PgConnection,
    gate: &CustomerGate,
) -> Result<bool, BusinessError> {
    let hit: Option<String> = sqlx::query_scalar(
        "SELECT g.id FROM assignments g \
          WHERE g.company_id=$1 AND g.user_id=$2 AND ($3::text IS NULL OR g.id=$3) \
          AND g.revoked_at IS NULL \
          AND g.valid_from<=$4 AND g.valid_until>$4 \
          AND iam_assignment_active(g.id,$4) \
          AND iam_principal_active(g.user_id,g.company_id,$4) \
          AND EXISTS(SELECT 1 FROM iam_roles role WHERE role.id=g.role \
            AND role.company_id IN ('*',g.company_id) AND role.status='active') \
          ORDER BY g.version DESC LIMIT 1",
    )
    .bind(gate.company.as_str())
    .bind(gate.user_id.as_str())
    .bind(gate.assignment_id.as_deref())
    .bind(gate.now)
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

/// Retained identity proof: the commerce roles, then the company row read
/// with its customer join (the oracle's `find` by id in the company rows).
async fn identity_target(
    c: &mut PgConnection,
    gate: &CustomerGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    permissions_require(c, gate, "customer_identity").await?;
    let hit: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM customer_identity_changes c \
          JOIN finance_parties p ON p.id=c.customer_id AND p.company_id=c.company_id \
          WHERE c.id=$1 AND c.company_id=$2",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if hit.is_none() {
        return Err(identity_not_found());
    }
    Ok(Some(InboxTarget {
        href: Some(customer_identity_href(source_id)),
        source: None,
    }))
}

/// Retained duplicate proof: the commerce roles, then the company review
/// read with both customer joins.
async fn duplicate_target(
    c: &mut PgConnection,
    gate: &CustomerGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    permissions_require(c, gate, "customer_duplicate").await?;
    let hit: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM customer_duplicate_reviews r \
          JOIN finance_parties l ON l.id=r.left_customer_id AND l.company_id=r.company_id \
          JOIN finance_parties q ON q.id=r.right_customer_id AND q.company_id=r.company_id \
          WHERE r.id=$1 AND r.company_id=$2",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if hit.is_none() {
        return Err(duplicate_not_found());
    }
    Ok(Some(InboxTarget {
        href: Some(customer_duplicate_href(source_id)),
        source: None,
    }))
}

/// Retained access proof: the commerce roles, then the purchase path
/// (queued authority, the scoped binding/order source, the portal
/// history), then the scoped binding read. The portal-release feature
/// predicate is effectively true outside production (documented gap).
async fn access_target(
    c: &mut PgConnection,
    gate: &CustomerGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    permissions_require(c, gate, "customer_access").await?;
    if gate.enforced() {
        // Queued authority, stood in for by the assignment currency proof.
        if !assignment_current(c, gate).await? {
            return Err(permission_required());
        }
        // Purchase `source()`: the scoped binding/order image (quote/sales
        // kinds only). Missing rows deny here, exactly like the oracle.
        let sourced: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM customer_access_bindings b \
              JOIN commerce_orders o ON o.id=b.order_id AND o.company_id=b.company_id \
                AND o.party_id=b.customer_id AND o.kind IN ('quote','sales') \
              WHERE b.id=$1 AND b.company_id=$2 \
              AND ($3::text IS NULL OR o.branch_id=$3) \
              AND ($4::text IS NULL OR o.department_id=$4)",
        )
        .bind(source_id)
        .bind(gate.company.as_str())
        .bind(gate.branch_id.as_deref())
        .bind(gate.department_id.as_deref())
        .fetch_optional(&mut *c)
        .await
        .map_err(|_| storage())?;
        if sourced.is_none() {
            return Err(access_not_found());
        }
        purchase_require(c, &gate.company, "sales.portal", gate.mode, gate.now).await?;
    }
    // Application binding read: the scoped binding/order row. The oracle's
    // `binding(id)` carries no kind filter, mirrored exactly.
    let hit: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM customer_access_bindings b \
          JOIN commerce_orders o ON o.id=b.order_id AND o.company_id=b.company_id \
          WHERE b.id=$1 AND o.company_id=$2 \
          AND ($3::text IS NULL OR o.branch_id=$3) \
          AND ($4::text IS NULL OR o.department_id=$4)",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(gate.branch_id.as_deref())
    .bind(gate.department_id.as_deref())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if hit.is_none() {
        return Err(access_not_found());
    }
    Ok(Some(InboxTarget {
        href: Some(customer_access_href(source_id)),
        source: None,
    }))
}

impl InboxTargetPort for CustomerInboxTargets {
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
            let gate = CustomerGate {
                company,
                user_id: user_id.to_owned(),
                assignment_id: self.assignment_id.clone(),
                branch_id: self.branch_id.clone(),
                department_id: self.department_id.clone(),
                mode,
                now,
            };
            match source_type {
                "customer_identity" => identity_target(connection, &gate, source_id).await,
                "customer_duplicate" => duplicate_target(connection, &gate, source_id).await,
                "customer_access" => access_target(connection, &gate, source_id).await,
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
    fn routing_covers_only_the_customer_slice() {
        for source_type in [
            "customer_identity",
            "customer_duplicate",
            "customer_access",
        ] {
            assert!(is_customer_inbox_target(source_type), "{source_type}");
        }
        for other in [
            "notice",
            "collaboration_action",
            "leave",
            "expense",
            "request",
            "task",
            "post",
            "operational_exception",
            "organization_change",
            "future_source",
            "",
        ] {
            assert!(!is_customer_inbox_target(other), "{other}");
        }
    }

    #[test]
    fn hrefs_encode_exactly_like_the_typescript_projection() {
        assert_eq!(customer_identity_href("chg-1"), "/customer-identity/chg-1");
        assert_eq!(
            customer_identity_href("a/b c"),
            "/customer-identity/a%2Fb%20c"
        );
        assert_eq!(
            customer_duplicate_href("dup-1"),
            "/customer-duplicates/dup-1"
        );
        assert_eq!(
            customer_duplicate_href("a/b c"),
            "/customer-duplicates/a%2Fb%20c"
        );
        assert_eq!(customer_access_href("acc-1"), "/customer-access/acc-1");
        assert_eq!(
            customer_access_href("a/b c"),
            "/customer-access/a%2Fb%20c"
        );
    }

    #[test]
    fn capability_and_permission_tables_match_the_retained_gates() {
        assert_eq!(
            customer_history_capabilities("customer_identity"),
            &[] as &[&str]
        );
        assert_eq!(
            customer_history_capabilities("customer_duplicate"),
            &[] as &[&str]
        );
        assert_eq!(
            customer_history_capabilities("customer_access"),
            &["sales.portal"]
        );
        assert_eq!(
            customer_history_capabilities("customer"),
            &[] as &[&str]
        );
        assert_eq!(customer_history_capabilities(""), &[] as &[&str]);
        for source_type in [
            "customer_identity",
            "customer_duplicate",
            "customer_access",
        ] {
            assert_eq!(
                customer_required_permissions(source_type),
                &["commerce.read", "commerce.sales"],
                "{source_type}"
            );
        }
        assert_eq!(customer_required_permissions("customer"), &[] as &[&str]);
        assert_eq!(customer_required_permissions(""), &[] as &[&str]);
    }

    #[test]
    fn customer_kinds_keep_their_retained_labels() {
        assert_eq!(source_record_label("customer_identity"), "Customer identity");
        assert_eq!(source_record_label("customer_duplicate"), "Customer duplicate");
        assert_eq!(source_record_label("customer_access"), "Customer access");
    }

    #[test]
    fn oracle_denials_keep_their_exact_codes_kinds_and_messages() {
        let denied = permission_required();
        assert_eq!(denied.kind, FailureKind::Forbidden);
        assert_eq!(denied.code, "PERMISSION_REQUIRED");
        assert_eq!(
            denied.message,
            "Your selected role does not grant this permission."
        );
        let missing = identity_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "CUSTOMER_IDENTITY_CHANGE_NOT_FOUND");
        assert_eq!(
            missing.message,
            "Identity change not found in this Company."
        );
        let missing = duplicate_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "CUSTOMER_DUPLICATE_NOT_FOUND");
        assert_eq!(
            missing.message,
            "Duplicate review not found in this Company."
        );
        let missing = access_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "CUSTOMER_ACCESS_NOT_FOUND");
        assert_eq!(
            missing.message,
            "Customer access request not found in this scope."
        );
    }
}
