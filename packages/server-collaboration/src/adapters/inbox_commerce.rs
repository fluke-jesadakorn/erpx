//! Native Commerce visibility delegate for the Inbox destination port.
//!
//! This mirrors the retained TypeScript `getOrder` read gate in
//! `packages/server-commerce/src/adapters/commerce-order-read.ts`
//! (`requireCommerceOrderRead` — scoped source, kind capability, current
//! authority — as invoked from the staff-notification projection for
//! `commerce_order`): a `commerce_order` item is visible only when the order
//! row exists in the selected Work frame, the `sales.quotes` / `sales.orders`
//! / `supply.purchasing` history gate for its kind passes, and the reader
//! holds current `commerce.read` authority over the order's scope. Missing or
//! out-of-frame orders fail with the exact TypeScript `ORDER_NOT_FOUND`
//! refusals; authority and purchase denials surface with the exact
//! TypeScript codes so composition can drop those rows exactly like the
//! retained per-row 403/404 catch.
//!
//! Nothing here opens a pool, commits, or retries. It runs serializable-safe
//! SELECTs on the caller's connection.
//!
//! Faithful-mirror notes (documented, not silent): the command-side guards
//! (`workContextGuards`, `iamMutationGuards`, `clearWorkflowCommandAssertions`)
//! authorize commands, not reads, and are not part of this read-only port;
//! the JWT-static `requirePermission` half of the application read is
//! subsumed by `iam_role_allows` on the reader's current assignment. The
//! read oracle has no ecommerce-specific capability branch — `capability()`
//! maps only the database-constrained `quote` / `sales` / `purchase` kinds,
//! and command-side ecommerce locks stay with Commerce composition.
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

/// Source types this delegate can prove against Commerce rows.
pub fn is_commerce_inbox_target(source_type: &str) -> bool {
    matches!(source_type, "commerce_order")
}

/// Retained destination address for one order, byte-identical to the
/// TypeScript `` `/commerce/orders/${encodeURIComponent(id)}` ``.
pub fn commerce_order_href(source_id: &str) -> String {
    format!("/commerce/orders/{}", encode_uri_component(source_id))
}

/// Retained kind-to-capability table from `purchase-boundary.ts`. `None` is
/// unreachable while the database constrains `kind` to the three retained
/// values; callers fail closed on it instead of guessing a capability.
pub fn commerce_order_capability(kind: &str) -> Option<&'static str> {
    match kind {
        "quote" => Some("sales.quotes"),
        "sales" => Some("sales.orders"),
        "purchase" => Some("supply.purchasing"),
        _ => None,
    }
}

fn order_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ORDER_NOT_FOUND",
        "Order not found.",
    )
}

fn order_not_found_in_workspace() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ORDER_NOT_FOUND",
        "Order not found in this workspace.",
    )
}

fn commerce_access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "COMMERCE_ACCESS_CHANGED",
        "Choose a current authorized Commerce work role.",
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

/// Native Commerce visibility delegate, composed via
/// `PgInboxTargets::with_delegate`. Commerce-only: every other source type
/// defers to the existing fail-closed behavior instead of inventing a link.
///
/// `branch_id`/`department_id` carry the selected Work frame used by the
/// retained order scope; without them the scope check is skipped exactly like
/// the TypeScript `actor.branchId && ...` arms.
pub struct CommerceInboxTargets {
    branch_id: Option<String>,
    department_id: Option<String>,
    assignment_id: Option<String>,
    enforcement: Option<PackageEnforcementMode>,
}

impl CommerceInboxTargets {
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

impl Default for CommerceInboxTargets {
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

/// Retained `source()` row for `type: "order"`: identity, scope and kind.
struct OrderRecord {
    company_id: String,
    branch_id: String,
    department_id: Option<String>,
    kind: String,
}

async fn order_record(
    c: &mut PgConnection,
    source_id: &str,
) -> Result<Option<OrderRecord>, BusinessError> {
    sqlx::query("SELECT company_id,branch_id,department_id,kind FROM commerce_orders WHERE id=$1")
    .bind(source_id)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?
    .map(|row| {
        let company_id: String = row.try_get("company_id").map_err(|_| storage())?;
        let branch_id: String = row.try_get("branch_id").map_err(|_| storage())?;
        let department_id: Option<String> =
            row.try_get("department_id").map_err(|_| storage())?;
        let kind: String = row.try_get("kind").map_err(|_| storage())?;
        Ok(OrderRecord {
            company_id,
            branch_id,
            department_id,
            kind,
        })
    })
    .transpose()
}

/// Retained `scoped()` frame check, evaluated in Rust exactly like the
/// TypeScript guard: the record must sit in the reader's Company and selected
/// Work frame.
fn scoped(
    record: &OrderRecord,
    company_id: &str,
    branch_id: Option<&str>,
    department_id: Option<&str>,
) -> Result<(), BusinessError> {
    if record.company_id != company_id
        || branch_id.is_some_and(|frame| record.branch_id != frame)
        || department_id.is_some_and(|frame| record.department_id.as_deref() != Some(frame))
    {
        return Err(order_not_found_in_workspace());
    }
    Ok(())
}

/// Retained `requireCurrent` for `commerce.read`: the reader's current
/// assignment must allow the permission over the order's own scope. This is
/// the database-current half of the oracle; the static JWT `requirePermission`
/// check is subsumed by `iam_role_allows` on that assignment.
async fn current_authority(
    c: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    assignment_id: Option<&str>,
    now: i64,
    record: &OrderRecord,
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
    .bind(&record.branch_id)
    .bind(record.department_id.as_deref())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if authorized.is_none() {
        return Err(commerce_access_changed());
    }
    Ok(())
}

/// Retained `checkPurchasedCapability` history gate for the order kind through
/// the native Commercial capability port. The refusal carries the exact
/// decision code and message from the TypeScript throw.
async fn purchase_history(
    c: &mut PgConnection,
    company_id: &CompanyId,
    capability: &str,
    mode: PackageEnforcementMode,
    now: i64,
) -> Result<(), BusinessError> {
    if mode == PackageEnforcementMode::Disabled {
        return Ok(());
    }
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

impl InboxTargetPort for CommerceInboxTargets {
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
                "commerce_order" => {
                    let Some(record) = order_record(connection, source_id).await? else {
                        return Err(order_not_found());
                    };
                    scoped(
                        &record,
                        company.as_str(),
                        self.branch_id.as_deref(),
                        self.department_id.as_deref(),
                    )?;
                    let Some(capability) = commerce_order_capability(&record.kind) else {
                        return Err(order_not_found());
                    };
                    purchase_history(connection, &company, capability, mode, now).await?;
                    current_authority(
                        connection,
                        company.as_str(),
                        user_id,
                        self.assignment_id.as_deref(),
                        now,
                        &record,
                    )
                    .await?;
                    // The retained projection sets no version on Commerce sources.
                    let href = commerce_order_href(source_id);
                    Ok(Some(InboxTarget {
                        href: Some(href.clone()),
                        source: Some(json!({
                            "kind": "commerce_order",
                            "id": source_id,
                            "href": href,
                            "label": source_record_label("commerce_order"),
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

    #[test]
    fn routing_covers_only_the_commerce_slice() {
        assert!(is_commerce_inbox_target("commerce_order"));
        for other in [
            "notice",
            "collaboration_action",
            "calendar_appointment",
            "crm_lead",
            "crm_activity",
            "support_ticket",
            "commerce",
            "future_source",
            "",
        ] {
            assert!(!is_commerce_inbox_target(other), "{other}");
        }
    }

    #[test]
    fn href_encodes_exactly_like_the_typescript_projection() {
        assert_eq!(commerce_order_href("ord-1"), "/commerce/orders/ord-1");
        assert_eq!(commerce_order_href("seg/one two"), "/commerce/orders/seg%2Fone%20two");
        assert_eq!(commerce_order_href("😀"), "/commerce/orders/%F0%9F%98%80");
    }

    #[test]
    fn kind_capability_table_matches_the_typescript_oracle() {
        assert_eq!(commerce_order_capability("quote"), Some("sales.quotes"));
        assert_eq!(commerce_order_capability("sales"), Some("sales.orders"));
        assert_eq!(
            commerce_order_capability("purchase"),
            Some("supply.purchasing")
        );
        // Unreachable while the database constrains kind; the delegate fails
        // closed on it instead of guessing a capability.
        assert_eq!(commerce_order_capability("ecommerce"), None);
        assert_eq!(commerce_order_capability(""), None);
    }

    #[test]
    fn commerce_kinds_keep_their_retained_labels() {
        assert_eq!(source_record_label("commerce_order"), "Commerce order");
    }

    #[test]
    fn oracle_denials_keep_their_exact_codes_kinds_and_messages() {
        let missing = order_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "ORDER_NOT_FOUND");
        assert_eq!(missing.message, "Order not found.");
        let scoped = order_not_found_in_workspace();
        assert_eq!(scoped.kind, FailureKind::NotFound);
        assert_eq!(scoped.code, "ORDER_NOT_FOUND");
        assert_eq!(scoped.message, "Order not found in this workspace.");
        let changed = commerce_access_changed();
        assert_eq!(changed.kind, FailureKind::Forbidden);
        assert_eq!(changed.code, "COMMERCE_ACCESS_CHANGED");
        assert_eq!(
            changed.message,
            "Choose a current authorized Commerce work role."
        );
    }
}
