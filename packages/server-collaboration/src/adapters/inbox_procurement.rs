//! Native Procurement visibility delegate for the Inbox destination port.
//!
//! This mirrors the retained per-row TypeScript gates for the
//! notification-visible Procurement sources:
//!
//! - `supplier_bill`: `getSupplierBill` — the `supply.purchasing` history
//!   gate (`withSupplierBillPurchasedRead`), the current `finance.read`
//!   role, the consolidated receipt-owner image proof, then the scoped bill
//!   read; 403 `SUPPLIER_BILL_ACCESS_CHANGED` without authority, 404
//!   `SUPPLIER_BILL_NOT_FOUND` when the image or the row is missing.
//! - `procurement_rfq`: `getRfq` — the `supply.purchasing` history gate
//!   (`requireProcurementPurchasedRead`), the current `commerce.read`
//!   role, then the scoped RFQ read; 403 `PROCUREMENT_ACCESS_CHANGED`
//!   without authority, 404 `RFQ_NOT_FOUND` when missing.
//! - `supplier_promise`: `getSupplierPromiseOrder` — the same purchase and
//!   `commerce.read` gates, then the scoped purchase-order read with the
//!   supplier join; 403 `PROCUREMENT_ACCESS_CHANGED` without authority, 404
//!   `PURCHASE_ORDER_NOT_FOUND` when missing.
//!
//! None of the sources sets an override: the composition final map applies
//! the retained defaults.
//!
//! Nothing here opens a pool, commits, or retries. It runs serializable-safe
//! SELECTs on the caller's connection.
//!
//! Faithful-mirror notes (documented, not silent):
//! - Gate order matches the oracles exactly: purchase decision, then
//!   authority, then the identity read. The bill image proof runs only when
//!   enforcement is enabled (any mode but disabled), exactly like
//!   `withSupplierBillPurchasedRead`: disabled mode proves the scoped bill
//!   row alone, so a bill failing the owner image stays visible there and
//!   denies elsewhere.
//! - The JWT-static `hasPermission` halves are enforced database-current via
//!   `iam_role_allows` on the exact (or highest active) assignment, literal
//!   mirror of the retained `assignmentReadSql` shape.
//! - The command-side guards (`workContextGuards`, `iamMutationGuards`,
//!   assertion clearing, purchase boundaries, receipts, locks, deferred
//!   finance effects) authorize commands, not reads, and are not part of
//!   this read-only port. The native command stagers (bill matching,
//!   RFQ/promise writers) own writes, not reads.
//! - Presentation joins (supplier/order/fulfillment/invoice facts, receipt
//!   counts, RFQ offers and lines, promise schedules, flow and candidate
//!   facts) are not visibility: rows passing the identity proof resolve
//!   without them.
//! - `inventory_transit` is inventory-owned and out of scope for this
//!   delegate; it defers like any other foreign source.
use crate::adapters::inbox_pg::{InboxTarget, InboxTargetFuture, InboxTargetPort};
use crate::adapters::inbox_targets::{encode_uri_component, retained_inbox_destination};
use erpx_server_commercial::public::capability_decision::PgCapabilityDecisionReader;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, current_package_catalogue,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use sqlx::PgConnection;

/// Source types this delegate can prove against Procurement rows.
pub fn is_procurement_inbox_target(source_type: &str) -> bool {
    matches!(
        source_type,
        "supplier_bill" | "procurement_rfq" | "supplier_promise"
    )
}

/// Retained destination address for one supplier bill, byte-identical to the
/// TypeScript `` `/supplier-bills/${encodeURIComponent(id)}` ``.
pub fn supplier_bill_href(source_id: &str) -> String {
    format!("/supplier-bills/{}", encode_uri_component(source_id))
}

/// Retained destination address for one RFQ, byte-identical to the
/// TypeScript `` `/procurement-rfqs/${encodeURIComponent(id)}` ``.
pub fn procurement_rfq_href(source_id: &str) -> String {
    format!("/procurement-rfqs/{}", encode_uri_component(source_id))
}

/// Retained destination address for one supplier promise: the encoded order
/// plus the exact `#supplier-promises` fragment.
pub fn supplier_promise_href(source_id: &str) -> String {
    format!(
        "/commerce/orders/{}#supplier-promises",
        encode_uri_component(source_id)
    )
}

/// Retained read capability per source type. All three notification reads
/// settle through the same purchasing history gate.
pub fn procurement_history_capability(source_type: &str) -> Option<&'static str> {
    match source_type {
        "supplier_bill" | "procurement_rfq" | "supplier_promise" => Some("supply.purchasing"),
        _ => None,
    }
}

fn bill_access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "SUPPLIER_BILL_ACCESS_CHANGED",
        "Choose a current authorized supplier-invoice work role.",
    )
}

fn bill_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "SUPPLIER_BILL_NOT_FOUND",
        "Supplier invoice review not found in this scope.",
    )
}

fn procurement_access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PROCUREMENT_ACCESS_CHANGED",
        "Choose a current authorized purchasing work role.",
    )
}

fn rfq_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "RFQ_NOT_FOUND",
        "Quotation request not found in this scope.",
    )
}

fn promise_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PURCHASE_ORDER_NOT_FOUND",
        "Purchase order not found in this scope.",
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

/// Native Procurement visibility delegate, composed via
/// `PgInboxTargets::with_delegate`. Procurement-only: every other source type
/// defers to the existing fail-closed behavior instead of inventing a link.
pub struct ProcurementInboxTargets {
    branch_id: Option<String>,
    department_id: Option<String>,
    assignment_id: Option<String>,
    enforcement: Option<PackageEnforcementMode>,
}

impl ProcurementInboxTargets {
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
    /// retained commerce environment-driven mode exactly. This seam exists so
    /// tests and composition can select a mode deterministically; it never
    /// changes what the mode means.
    pub fn enforcement_mode(mut self, mode: PackageEnforcementMode) -> Self {
        self.enforcement = Some(mode);
        self
    }
}

impl Default for ProcurementInboxTargets {
    fn default() -> Self {
        Self::new(None, None)
    }
}

/// Retained `commercePackageEnforcementMode`, mirrored from the TypeScript
/// oracle: disabled opens the read, a non-local environment fails with the
/// environment code, and an unknown value fails closed with the
/// configuration code.
fn enforcement_mode() -> Result<PackageEnforcementMode, BusinessError> {
    match std::env::var("ERPX_COMMERCE_PACKAGE_ENFORCEMENT")
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

/// Retained purchase-history gate through the native Commercial capability
/// port. The refusal carries the exact decision code and message from the
/// TypeScript throw.
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

/// Retained authority proof, literal mirror of the `assignmentReadSql`
/// shape: the exact (or highest active) assignment is current with the
/// caller, allows the retained permission, and covers the selected Work
/// frame.
async fn current_authority(
    c: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    assignment_id: Option<&str>,
    permission: &str,
    branch_id: Option<&str>,
    department_id: Option<&str>,
    now: i64,
) -> Result<(), BusinessError> {
    let permission_check = match permission {
        "finance.read" => "AND iam_role_allows(g.role,g.company_id,'finance.read')",
        "commerce.read" => "AND iam_role_allows(g.role,g.company_id,'commerce.read')",
        _ => "AND FALSE",
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
    .bind(branch_id)
    .bind(department_id)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if authorized.is_none() {
        return Err(match permission {
            "finance.read" => bill_access_changed(),
            _ => procurement_access_changed(),
        });
    }
    Ok(())
}

struct ProcurementGate {
    company: CompanyId,
    user_id: String,
    assignment_id: Option<String>,
    branch_id: Option<String>,
    department_id: Option<String>,
    mode: PackageEnforcementMode,
    now: i64,
}

impl ProcurementGate {
    fn enforced(&self) -> bool {
        self.mode != PackageEnforcementMode::Disabled
    }
}

/// Retained bill image proof: the consolidated purchase-receipt owner, byte
/// for byte the `image()` gate in `bill-purchases.ts`. A missing or
/// foreign-sourced bill denies with the retained 404.
async fn bill_image(
    c: &mut PgConnection,
    gate: &ProcurementGate,
    source_id: &str,
) -> Result<(), BusinessError> {
    let hit: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM procurement_bill_matches m \
          JOIN commerce_fulfillments primary_receipt \
            ON primary_receipt.id=m.fulfillment_id AND primary_receipt.order_id=m.order_id \
          JOIN commerce_orders primary_order \
            ON primary_order.id=m.order_id AND primary_order.company_id=m.company_id \
            AND primary_order.kind='purchase' \
          WHERE m.id=$1 AND m.company_id=$2 \
          AND ($3::text IS NULL OR m.branch_id=$3) \
          AND ($4::text IS NULL OR m.department_id=$4) \
          AND EXISTS(SELECT 1 FROM procurement_bill_match_receipts br \
            WHERE br.match_id=m.id AND br.fulfillment_id=m.fulfillment_id) \
          AND NOT EXISTS(SELECT 1 FROM procurement_bill_match_receipts br \
            LEFT JOIN commerce_fulfillments f ON f.id=br.fulfillment_id \
            LEFT JOIN commerce_orders o ON o.id=f.order_id \
            WHERE br.match_id=m.id \
            AND (o.id IS NULL OR o.kind<>'purchase' OR o.company_id<>m.company_id \
              OR o.party_id<>m.supplier_id OR o.currency<>m.currency))",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(gate.branch_id.as_deref())
    .bind(gate.department_id.as_deref())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if hit.is_none() {
        return Err(bill_not_found());
    }
    Ok(())
}

/// Retained bill proof: purchase gate, current `finance.read` role, the
/// owner image, then the scoped bill read.
async fn bill_target(
    c: &mut PgConnection,
    gate: &ProcurementGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    purchase_history(
        c,
        &gate.company,
        "supply.purchasing",
        gate.mode,
        gate.now,
    )
    .await?;
    current_authority(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        "finance.read",
        gate.branch_id.as_deref(),
        gate.department_id.as_deref(),
        gate.now,
    )
    .await?;
    if gate.enforced() {
        bill_image(c, gate, source_id).await?;
    }
    let visible: Option<String> = sqlx::query_scalar(
        "SELECT m.id FROM procurement_bill_matches m \
          WHERE m.id=$1 AND m.company_id=$2 \
          AND ($3::text IS NULL OR m.branch_id=$3) \
          AND ($4::text IS NULL OR m.department_id=$4)",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(gate.branch_id.as_deref())
    .bind(gate.department_id.as_deref())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if visible.is_none() {
        return Err(bill_not_found());
    }
    Ok(Some(InboxTarget {
        href: Some(supplier_bill_href(source_id)),
        source: None,
    }))
}

/// Retained RFQ proof: purchase gate, current `commerce.read` role, then the
/// scoped RFQ read.
async fn rfq_target(
    c: &mut PgConnection,
    gate: &ProcurementGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    purchase_history(
        c,
        &gate.company,
        "supply.purchasing",
        gate.mode,
        gate.now,
    )
    .await?;
    current_authority(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        "commerce.read",
        gate.branch_id.as_deref(),
        gate.department_id.as_deref(),
        gate.now,
    )
    .await?;
    let visible: Option<String> = sqlx::query_scalar(
        "SELECT r.id FROM procurement_rfqs r \
          WHERE r.id=$1 AND r.company_id=$2 \
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
        return Err(rfq_not_found());
    }
    Ok(Some(InboxTarget {
        href: Some(procurement_rfq_href(source_id)),
        source: None,
    }))
}

/// Retained supplier-promise proof: purchase gate, current `commerce.read`
/// role, then the scoped purchase-order read with the supplier join.
async fn promise_target(
    c: &mut PgConnection,
    gate: &ProcurementGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    purchase_history(
        c,
        &gate.company,
        "supply.purchasing",
        gate.mode,
        gate.now,
    )
    .await?;
    current_authority(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        "commerce.read",
        gate.branch_id.as_deref(),
        gate.department_id.as_deref(),
        gate.now,
    )
    .await?;
    let visible: Option<String> = sqlx::query_scalar(
        "SELECT o.id FROM commerce_orders o \
          JOIN finance_parties p ON p.id=o.party_id \
          WHERE o.id=$1 AND o.company_id=$2 AND o.kind='purchase' \
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
    if visible.is_none() {
        return Err(promise_not_found());
    }
    Ok(Some(InboxTarget {
        href: Some(supplier_promise_href(source_id)),
        source: None,
    }))
}

impl InboxTargetPort for ProcurementInboxTargets {
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
            let gate = ProcurementGate {
                company,
                user_id: user_id.to_owned(),
                assignment_id: self.assignment_id.clone(),
                branch_id: self.branch_id.clone(),
                department_id: self.department_id.clone(),
                mode,
                now,
            };
            match source_type {
                "supplier_bill" => bill_target(connection, &gate, source_id).await,
                "procurement_rfq" => rfq_target(connection, &gate, source_id).await,
                "supplier_promise" => promise_target(connection, &gate, source_id).await,
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
    fn routing_covers_only_the_procurement_slice() {
        for source_type in ["supplier_bill", "procurement_rfq", "supplier_promise"] {
            assert!(is_procurement_inbox_target(source_type), "{source_type}");
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
            "support",
            "inventory_transit",
            "future_source",
            "",
        ] {
            assert!(!is_procurement_inbox_target(other), "{other}");
        }
    }

    #[test]
    fn hrefs_encode_exactly_like_the_typescript_projection() {
        assert_eq!(supplier_bill_href("bill-1"), "/supplier-bills/bill-1");
        assert_eq!(supplier_bill_href("a/b c"), "/supplier-bills/a%2Fb%20c");
        assert_eq!(procurement_rfq_href("rfq-1"), "/procurement-rfqs/rfq-1");
        assert_eq!(procurement_rfq_href("a/b c"), "/procurement-rfqs/a%2Fb%20c");
        assert_eq!(
            supplier_promise_href("ord-1"),
            "/commerce/orders/ord-1#supplier-promises"
        );
        assert_eq!(
            supplier_promise_href("a/b c"),
            "/commerce/orders/a%2Fb%20c#supplier-promises"
        );
    }

    #[test]
    fn history_capabilities_match_the_retained_gates() {
        assert_eq!(
            procurement_history_capability("supplier_bill"),
            Some("supply.purchasing")
        );
        assert_eq!(
            procurement_history_capability("procurement_rfq"),
            Some("supply.purchasing")
        );
        assert_eq!(
            procurement_history_capability("supplier_promise"),
            Some("supply.purchasing")
        );
        assert_eq!(procurement_history_capability("inventory_transit"), None);
        assert_eq!(procurement_history_capability(""), None);
    }

    #[test]
    fn procurement_kinds_keep_their_retained_labels() {
        assert_eq!(source_record_label("supplier_bill"), "Supplier bill");
        assert_eq!(source_record_label("procurement_rfq"), "Procurement rfq");
        assert_eq!(source_record_label("supplier_promise"), "Supplier promise");
    }

    #[test]
    fn oracle_denials_keep_their_exact_codes_kinds_and_messages() {
        let changed = bill_access_changed();
        assert_eq!(changed.kind, FailureKind::Forbidden);
        assert_eq!(changed.code, "SUPPLIER_BILL_ACCESS_CHANGED");
        assert_eq!(
            changed.message,
            "Choose a current authorized supplier-invoice work role."
        );
        let missing = bill_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "SUPPLIER_BILL_NOT_FOUND");
        assert_eq!(
            missing.message,
            "Supplier invoice review not found in this scope."
        );
        let changed = procurement_access_changed();
        assert_eq!(changed.kind, FailureKind::Forbidden);
        assert_eq!(changed.code, "PROCUREMENT_ACCESS_CHANGED");
        assert_eq!(
            changed.message,
            "Choose a current authorized purchasing work role."
        );
        let missing = rfq_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "RFQ_NOT_FOUND");
        assert_eq!(missing.message, "Quotation request not found in this scope.");
        let missing = promise_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "PURCHASE_ORDER_NOT_FOUND");
        assert_eq!(missing.message, "Purchase order not found in this scope.");
    }
}
