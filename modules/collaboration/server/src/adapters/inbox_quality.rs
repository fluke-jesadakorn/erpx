//! Native Quality and Legal visibility delegate for the Inbox destination
//! port.
//!
//! This mirrors the retained per-row TypeScript gates for the
//! notification-visible Quality and Legal sources:
//!
//! - `quality_hold`: `getQualityHold` — the `quality.read` purchase gate
//!   (`requireQualityPurchasedRead`), then the scoped hold read with its
//!   product/warehouse/lot joins; 403 `QUALITY_ACCESS_CHANGED` without a
//!   current assignment, 404 `QUALITY_HOLD_NOT_FOUND` when the image or
//!   the row is missing. No source override.
//! - `quality_alert`: `getQualityAlert` — the scoped alert read behind its
//!   hold first, then the same purchase gate on the alert's hold; the href
//!   carries the hold id and the source is exactly `{kind:"quality_hold",
//!   id:hold_id, href, label}` with no version, byte-identical to the
//!   retained push.
//! - `legal`: `access` — the `legal.read` permission, the scoped item read
//!   (internal or member-visible, with the requester restriction), then the
//!   kind-capability purchase gate; 403 `PERMISSION_REQUIRED` without the
//!   role, 404 `LEGAL_NOT_FOUND` when the row is missing, 403
//!   `LEGAL_ACCESS_CHANGED` without a current assignment, 403
//!   `PACKAGE_REQUIRED` without history. No source override.
//!
//! Gate order matches each oracle exactly. Holds run authority, image,
//! purchase, read with enforcement on and the readable check with it off;
//! alerts run the readable check and the scoped alert read first and then
//! authority, image, purchase on the hold; legal runs permission, the
//! scoped read, authority, purchase.
//!
//! Nothing here opens a pool, commits, or retries. It runs serializable-safe
//! SELECTs on the caller's connection.
//!
//! Faithful-mirror notes (documented, not silent):
//! - The JWT-static `hasPermission` halves are enforced database-current via
//!   `iam_role_allows` on the exact (or highest active) assignment, literal
//!   mirror of the retained `assignmentReadSql` shape. Quality and transit
//!   scope that proof to the company/branch frame with a NULL department,
//!   exactly like their oracles; legal binds the full Work frame.
//! - Enforcement reads the retained owner key per source: commerce for the
//!   quality types, legal for the legal type.
//! - The command-side guards (`workContextGuards`, `iamMutationGuards`,
//!   assertion clearing, purchase boundaries, receipts, locks, deferred
//!   effects) authorize commands, not reads, and are not part of this
//!   read-only port.
//! - Presentation joins and facts (product/warehouse names, eligible and
//!   recipient counts, inspection evidence, legal members and documents)
//!   are not visibility: rows passing the identity proof resolve without
//!   them.
//! - Quality purchase capabilities resolve from the hold lineage root
//!   exactly like the oracle: a spec'd root settles `supply.quality`; a
//!   receipt origin settles receiving plus stock; a customer-return origin
//!   settles the order stream plus the stock core; anything else falls back
//!   to `supply.quality`. Marked initial-stock and quality-disposal
//!   variants keep their own owner purchase proofs in the oracle and fail
//!   closed here unless the mainstream history is also granted.
//! - The legal production-rollout release predicate is identically TRUE
//!   outside production, matching the oracle's development behavior; the
//!   enforced-mode `FEATURE_COMING-soon` refusal for non-rolled-out
//!   companies is not mirrored.
//! - Disabled-mode reads skip the purchase path exactly like the oracles;
//!   the legal assignment proof still runs database-current, so a stale
//!   legal assignment denies where the oracle would resolve.
use crate::adapters::inbox_pg::{InboxTarget, InboxTargetFuture, InboxTargetPort};
use crate::adapters::inbox_targets::{encode_uri_component, retained_inbox_destination};
use erpx_commercial_server::public::capability_decision::PgCapabilityDecisionReader;
use erpx_commercial_server::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, current_package_catalogue,
};
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use serde_json::json;
use sqlx::{PgConnection, Row};

/// Source types this delegate can prove against Quality and Legal rows.
pub fn is_quality_inbox_target(source_type: &str) -> bool {
    matches!(
        source_type,
        "quality_hold" | "quality_alert" | "legal"
    )
}

/// Retained destination address for one quality hold, byte-identical to the
/// TypeScript `` `/quality/${encodeURIComponent(id)}` ``.
pub fn quality_hold_href(source_id: &str) -> String {
    format!("/quality/{}", encode_uri_component(source_id))
}

/// Retained destination address for one quality alert: the hold's address,
/// byte-identical to the TypeScript `` `/quality/${encodeURIComponent(hold_id)}` ``.
pub fn quality_alert_href(hold_id: &str) -> String {
    quality_hold_href(hold_id)
}

/// Retained destination address for one legal record, byte-identical to the
/// TypeScript `` `/legal?record=${encodeURIComponent(id)}` ``.
pub fn legal_record_href(source_id: &str) -> String {
    format!("/legal?record={}", encode_uri_component(source_id))
}

/// Retained purchase-capability families per source type. Quality holds
/// resolve the exact settling set from their lineage root at read time;
/// the table names the full family the oracle draws from.
pub fn quality_history_capabilities(source_type: &str) -> &'static [&'static str] {
    match source_type {
        "quality_hold" | "quality_alert" => &[
            "supply.quality",
            "supply.receiving",
            "supply.stock",
            "sales.orders",
            "inventory.stock-core",
        ],
        "legal" => &[
            "legal.contracts",
            "legal.documents",
            "legal.matters",
            "legal.obligations",
            "legal.access",
        ],
        _ => &[],
    }
}

/// Retained legal kind-to-capability map, mirror of
/// `legalKindCapability`: contracts and templates settle contracts;
/// requests, matters, counsel and IP settle matters; everything else
/// settles obligations.
pub fn legal_kind_capability(kind: &str) -> &'static str {
    match kind {
        "contract" | "template" => "legal.contracts",
        "request" | "matter" | "counsel" | "ip" => "legal.matters",
        _ => "legal.obligations",
    }
}

fn permission_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PERMISSION_REQUIRED",
        "Your selected role does not grant this permission.",
    )
}

fn quality_scope_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "QUALITY_SCOPE_REQUIRED",
        "Use a Company or Branch quality assignment.",
    )
}

fn quality_access_changed(message: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "QUALITY_ACCESS_CHANGED",
        message,
    )
}

fn hold_workspace_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "QUALITY_HOLD_NOT_FOUND",
        "Quality hold not found in the current workspace.",
    )
}

fn hold_source_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "QUALITY_HOLD_NOT_FOUND",
        "The original Quality source is unavailable.",
    )
}

fn hold_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "QUALITY_HOLD_NOT_FOUND",
        "Quality hold not found.",
    )
}

fn quality_package_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PACKAGE_REQUIRED",
        "This Quality source requires its original purchase or retained history.",
    )
}

fn alert_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "QUALITY_ALERT_NOT_FOUND",
        "Quality escalation is not available in this scope.",
    )
}

fn legal_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "LEGAL_NOT_FOUND",
        "Legal record is unavailable in your current scope.",
    )
}

fn legal_access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "LEGAL_ACCESS_CHANGED",
        "The original Legal assignment is no longer current.",
    )
}

fn legal_package_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PACKAGE_REQUIRED",
        "This Legal source requires its purchased capability or retained history.",
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

/// Native Quality and Legal visibility delegate, composed via
/// `PgInboxTargets::with_delegate`. Quality- and Legal-only: every other
/// source type defers to the existing fail-closed behavior instead of
/// inventing a link.
pub struct QualityInboxTargets {
    branch_id: Option<String>,
    department_id: Option<String>,
    assignment_id: Option<String>,
    enforcement: Option<PackageEnforcementMode>,
}

impl QualityInboxTargets {
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
    /// retained owner environment-driven mode exactly (commerce for the
    /// quality types, legal for the legal type). This seam exists so tests
    /// and composition can select a mode deterministically; it never
    /// changes what the mode means.
    pub fn enforcement_mode(mut self, mode: PackageEnforcementMode) -> Self {
        self.enforcement = Some(mode);
        self
    }
}

impl Default for QualityInboxTargets {
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
        "legal" => "ERPX_LEGAL_PACKAGE_ENFORCEMENT",
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

struct QualityGate {
    company: CompanyId,
    user_id: String,
    assignment_id: Option<String>,
    branch_id: Option<String>,
    department_id: Option<String>,
    mode: PackageEnforcementMode,
    now: i64,
}

impl QualityGate {
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
        "quality.read" => "AND iam_role_allows(g.role,g.company_id,'quality.read')",
        "legal.read" => "AND iam_role_allows(g.role,g.company_id,'legal.read')",
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

/// Retained history proof for the given capabilities through the native
/// Commercial capability port. Every capability must allow history;
/// callers map the refusal to the exact oracle code.
async fn history_allowed(
    c: &mut PgConnection,
    company_id: &CompanyId,
    capabilities: &[&str],
    mode: PackageEnforcementMode,
    now: i64,
) -> Result<bool, BusinessError> {
    let catalogue = current_package_catalogue()?;
    for capability in capabilities {
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
        if !check.decision().is_some_and(|decision| decision.allowed) {
            return Ok(false);
        }
    }
    Ok(true)
}

struct HoldImage {
    branch_id: String,
    source_type: Option<String>,
    source_id: Option<String>,
    warehouse_id: String,
    product_id: String,
    lot_id: Option<String>,
    held_quantity_milli: Option<i64>,
    created_by: String,
}

/// Retained hold image: the scoped hold row plus its lineage root, exactly
/// like `qualityHoldPurchaseSource`. The root carries the settling
/// capabilities.
async fn hold_image(
    c: &mut PgConnection,
    gate: &QualityGate,
    hold_id: &str,
) -> Result<(HoldImage, Option<String>), BusinessError> {
    let row = sqlx::query(
        "SELECT branch_id,spec_id,source_type,source_id,warehouse_id,product_id,lot_id,held_quantity_milli,created_by \
          FROM inventory_quality_holds \
          WHERE id=$1 AND company_id=$2 \
          AND ($3::text IS NULL OR branch_id=$3)",
    )
    .bind(hold_id)
    .bind(gate.company.as_str())
    .bind(gate.branch_id.as_deref())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    let Some(row) = row else {
        return Err(hold_workspace_not_found());
    };
    let root_spec: Option<Option<String>> = sqlx::query_scalar(
        "WITH RECURSIVE lineage AS ( \
          SELECT h.*,0 depth FROM inventory_quality_holds h WHERE h.id=$1 AND h.company_id=$2 \
          UNION ALL SELECT parent.*,child.depth+1 FROM lineage child \
          JOIN inventory_quality_holds parent ON parent.company_id=child.company_id \
            AND parent.branch_id=child.branch_id AND parent.warehouse_id=child.warehouse_id \
            AND parent.product_id=child.product_id \
            AND parent.lot_id IS NOT DISTINCT FROM child.lot_id \
            AND parent.created_by=child.created_by \
            AND parent.spec_id IS NOT DISTINCT FROM child.spec_id \
            AND parent.spec_version IS NOT DISTINCT FROM child.spec_version \
            AND parent.source_type IS NOT DISTINCT FROM child.source_type \
            AND parent.status='disposed' \
          JOIN inventory_counts co ON co.id=parent.disposition_count_id \
            AND co.quality_hold_id=parent.id AND co.company_id=parent.company_id \
            AND co.branch_id=parent.branch_id AND co.warehouse_id=parent.warehouse_id \
            AND co.status='posted' \
          JOIN inventory_quality_disposition_events e ON e.hold_id=parent.id \
            AND e.count_id=co.id AND e.action='post' \
          WHERE child.depth<64 AND child.id=parent.id||'-remaining-'||co.id \
            AND child.source_id=parent.source_id||':remaining:'||co.id \
            AND child.held_quantity_milli=parent.held_quantity_milli-parent.disposition_quantity_milli \
            AND child.held_quantity_milli>0 \
        ) SELECT spec_id FROM lineage ORDER BY depth DESC LIMIT 1",
    )
    .bind(hold_id)
    .bind(gate.company.as_str())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;

    let root_spec: Option<String> = root_spec.flatten();
    let branch_id: String = row.try_get("branch_id").map_err(|_| storage())?;
    let source_type: Option<String> = row.try_get("source_type").map_err(|_| storage())?;
    let source_id_value: Option<String> = row.try_get("source_id").map_err(|_| storage())?;
    let warehouse_id: String = row.try_get("warehouse_id").map_err(|_| storage())?;
    let product_id: String = row.try_get("product_id").map_err(|_| storage())?;
    let lot_id: Option<String> = row.try_get("lot_id").map_err(|_| storage())?;
    let held_quantity_milli: Option<i64> = row.try_get("held_quantity_milli").map_err(|_| storage())?;
    let created_by: String = row.try_get("created_by").map_err(|_| storage())?;
    // The lineage root settles the capabilities; a spec'd root settles the
    // spec capability without consulting the receipt origins.
    Ok((
        HoldImage {
            branch_id,
            source_type,
            source_id: source_id_value,
            warehouse_id,
            product_id,
            lot_id,
            held_quantity_milli,
            created_by,
        },
        root_spec,
    ))
}

/// Retained origin capabilities for a lineage root, mirror of the
/// `commerceQualityHoldOrigin` mapping.
async fn origin_capabilities(
    c: &mut PgConnection,
    gate: &QualityGate,
    hold_id: &str,
    image: &HoldImage,
    root_spec_id: Option<&str>,
) -> Result<Vec<&'static str>, BusinessError> {
    if root_spec_id.is_some() {
        return Ok(vec!["supply.quality"]);
    }
    let kind = match image.source_type.as_deref() {
        Some("goods_receipt") => "receipt",
        Some("customer_return") => "customer-return",
        _ => return Ok(vec!["supply.quality"]),
    };
    let origin: Option<String> = sqlx::query_scalar(
        "SELECT kind FROM ( \
          SELECT 'receipt'::text kind,a.quality_hold_id hold_id,o.company_id,o.branch_id,o.warehouse_id,ol.product_id,a.lot_id,a.id source_id,o.id owner_id,a.quantity_milli,a.value_minor,NULL::text created_by \
          FROM inventory_fulfillment_lot_allocations a \
          JOIN commerce_fulfillment_lines line ON line.id=a.fulfillment_line_id \
          JOIN commerce_order_lines ol ON ol.id=line.order_line_id AND ol.item_type<>'service' \
          JOIN commerce_fulfillments f ON f.id=line.fulfillment_id \
          JOIN commerce_orders o ON o.id=f.order_id AND o.id=ol.order_id AND o.kind='purchase' \
          WHERE a.quality_hold_id IS NOT NULL AND a.disposition='quarantine' \
          UNION ALL \
          SELECT 'customer-return'::text kind,'quality-return-'||receipt.command_id||'-'||line.ordinal::text hold_id,r.company_id,r.branch_id,o.warehouse_id,line.product_id,NULL::text lot_id,line.id source_id,r.id owner_id,line.quantity_milli,line.value_minor,r.approved_by created_by \
          FROM commerce_returns r \
          JOIN commerce_orders o ON o.id=r.order_id AND o.company_id=r.company_id AND o.kind='sales' \
          JOIN commerce_fulfillments f ON f.id=r.fulfillment_id AND f.order_id=o.id AND f.invoice_id=r.invoice_id \
          JOIN commerce_return_lines line ON line.return_id=r.id \
          JOIN commerce_fulfillment_lines fl ON fl.id=line.fulfillment_line_id AND fl.fulfillment_id=f.id \
          JOIN commerce_order_lines ol ON ol.id=fl.order_line_id AND ol.order_id=o.id AND ol.product_id=line.product_id AND ol.item_type<>'service' \
          JOIN command_receipts receipt ON receipt.record_id=r.id AND receipt.operation='commerce.return.approve' AND receipt.actor_id=r.approved_by AND receipt.authority_json::jsonb->>'companyId'=r.company_id \
          WHERE r.kind='customer_return' AND r.status IN ('approved','posted') \
            AND NOT EXISTS(SELECT 1 FROM commerce_return_lot_allocations allocation WHERE allocation.return_line_id=line.id) \
          UNION ALL \
          SELECT 'customer-return'::text kind,'quality-return-'||receipt.command_id||'-'||line.ordinal::text||'-'||allocation.lot_ordinal::text hold_id,r.company_id,r.branch_id,o.warehouse_id,line.product_id,allocation.lot_id,allocation.id source_id,r.id owner_id,allocation.quantity_milli,allocation.value_minor,r.approved_by created_by \
          FROM commerce_returns r \
          JOIN commerce_orders o ON o.id=r.order_id AND o.company_id=r.company_id AND o.kind='sales' \
          JOIN commerce_fulfillments f ON f.id=r.fulfillment_id AND f.order_id=o.id AND f.invoice_id=r.invoice_id \
          JOIN commerce_return_lines line ON line.return_id=r.id \
          JOIN commerce_fulfillment_lines fl ON fl.id=line.fulfillment_line_id AND fl.fulfillment_id=f.id \
          JOIN commerce_order_lines ol ON ol.id=fl.order_line_id AND ol.order_id=o.id AND ol.product_id=line.product_id AND ol.item_type<>'service' \
          JOIN command_receipts receipt ON receipt.record_id=r.id AND receipt.operation='commerce.return.approve' AND receipt.actor_id=r.approved_by AND receipt.authority_json::jsonb->>'companyId'=r.company_id \
          JOIN (SELECT *,row_number() OVER (PARTITION BY return_line_id ORDER BY id)-1 lot_ordinal FROM commerce_return_lot_allocations) allocation ON allocation.return_line_id=line.id \
          JOIN inventory_fulfillment_lot_allocations original ON original.id=allocation.fulfillment_allocation_id AND original.fulfillment_line_id=fl.id AND original.lot_id=allocation.lot_id \
          WHERE r.kind='customer_return' AND r.status IN ('approved','posted') \
        ) origin WHERE kind=$1 AND hold_id=$2 AND company_id=$3 AND branch_id=$4 AND warehouse_id=$5 AND product_id=$6 \
          AND lot_id IS NOT DISTINCT FROM $7 AND quantity_milli=$8 \
          AND (kind='receipt' OR (source_id=$9 AND created_by=$10))",
    )
    .bind(kind)
    .bind(hold_id)
    .bind(gate.company.as_str())
    .bind(&image.branch_id)
    .bind(&image.warehouse_id)
    .bind(&image.product_id)
    .bind(image.lot_id.as_deref())
    .bind(image.held_quantity_milli)
    .bind(image.source_id.as_deref())
    .bind(&image.created_by)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    match origin.as_deref() {
        Some("receipt") => Ok(vec!["supply.receiving", "supply.stock"]),
        Some("customer-return") => Ok(vec!["sales.orders", "inventory.stock-core"]),
        _ => Ok(vec!["supply.quality"]),
    }
}

struct AlertRow {
    hold_id: String,
}

/// Retained alert read: the scoped alert behind its hold first, exactly
/// like the oracle reading the alert before the purchase gate.
async fn alert_row(
    c: &mut PgConnection,
    gate: &QualityGate,
    source_id: &str,
) -> Result<AlertRow, BusinessError> {
    let row = sqlx::query(
        "SELECT a.hold_id FROM inventory_quality_alerts a \
          JOIN inventory_quality_holds h ON h.id=a.hold_id \
          JOIN commerce_products p ON p.id=h.product_id \
          JOIN commerce_warehouses w ON w.id=h.warehouse_id \
          WHERE a.id=$1 AND h.company_id=$2 \
          AND ($3::text IS NULL OR h.branch_id=$3)",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(gate.branch_id.as_deref())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    let Some(row) = row else {
        return Err(alert_not_found());
    };
    let hold_id: String = row.try_get("hold_id").map_err(|_| storage())?;
    Ok(AlertRow { hold_id })
}

async fn quality_authority(
    c: &mut PgConnection,
    gate: &QualityGate,
) -> Result<(String, String), BusinessError> {
    if gate.department_id.is_some() {
        return Err(quality_access_changed(
            "Choose a current Company or Branch Quality assignment.",
        ));
    }
    let assignment = resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?;
    let readable = match &assignment {
        Some((id, _)) => role_allows(c, gate.company.as_str(), id, "quality.read").await?,
        None => false,
    };
    if !readable {
        return Err(quality_access_changed(
            "Choose a current authorized Quality assignment.",
        ));
    }
    let (assignment_id, role) = match assignment {
        Some((id, role)) => (id, role),
        None => {
            return Err(quality_access_changed(
                "The original Quality assignment is no longer current.",
            ));
        }
    };
    if !scope_authority(
        c,
        gate.company.as_str(),
        &gate.user_id,
        &assignment_id,
        &role,
        "quality.read",
        gate.branch_id.as_deref(),
        None,
        gate.now,
    )
    .await?
    {
        return Err(quality_access_changed(
            "The original Quality assignment is no longer current.",
        ));
    }
    Ok((assignment_id, role))
}

/// Retained hold proof: authority, the lineage image and the origin
/// purchase in enforced mode (exactly like `requireQualityPurchasedRead`
/// preceding the application read), the readable check otherwise, then the
/// scoped hold read.
async fn hold_target(
    c: &mut PgConnection,
    gate: &QualityGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    if gate.enforced() {

        quality_authority(c, gate).await?;

        let (image, root_spec) = hold_image(c, gate, source_id).await?;

        let capabilities =
            origin_capabilities(c, gate, source_id, &image, root_spec.as_deref()).await?;

        if !history_allowed(c, &gate.company, &capabilities, gate.mode, gate.now).await? {
            return Err(quality_package_required());
        }
    } else {
        // Enforcement off skips the purchase path entirely, exactly like
        // the oracle: the readable check authorizes the scoped read.
        let assignment = resolve_assignment(
            c,
            gate.company.as_str(),
            &gate.user_id,
            gate.assignment_id.as_deref(),
            gate.now,
        )
        .await?;
        let readable = match &assignment {
            Some((id, _)) => role_allows(c, gate.company.as_str(), id, "quality.read").await?,
            None => false,
        };
        if !readable {
            return Err(permission_required());
        }
        if gate.department_id.is_some() {
            return Err(quality_scope_required());
        }
    }
    let visible: Option<String> = sqlx::query_scalar(
        "SELECT h.id FROM inventory_quality_holds h \
          JOIN commerce_products p ON p.id=h.product_id \
          JOIN commerce_warehouses w ON w.id=h.warehouse_id \
          LEFT JOIN inventory_lots l ON l.id=h.lot_id \
          WHERE h.id=$1 AND h.company_id=$2 \
          AND ($3::text IS NULL OR h.branch_id=$3)",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(gate.branch_id.as_deref())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if visible.is_none() {
        return Err(hold_not_found());
    }
    Ok(Some(InboxTarget {
        href: Some(quality_hold_href(source_id)),
        source: None,
    }))
}

/// Retained alert proof: the readable check and the scoped alert read
/// first, then authority, the hold image and purchase on the alert's
/// hold in enforced mode. The href and source carry the hold id, exactly
/// like the retained push.
async fn alert_target(
    c: &mut PgConnection,
    gate: &QualityGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    // The application reads the alert before the purchase gate, so the
    // readable check precedes the scoped read in every mode.
    let assignment = resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?;
    let readable = match &assignment {
        Some((id, _)) => role_allows(c, gate.company.as_str(), id, "quality.read").await?,
        None => false,
    };
    if !readable {
        return Err(permission_required());
    }
    if gate.department_id.is_some() {
        return Err(quality_scope_required());
    }
    let alert = alert_row(c, gate, source_id).await?;
    if gate.enforced() {
        quality_authority(c, gate).await?;
        let (image, root_spec) = hold_image(c, gate, &alert.hold_id).await?;
        let capabilities =
            origin_capabilities(c, gate, &alert.hold_id, &image, root_spec.as_deref()).await?;
        if !history_allowed(c, &gate.company, &capabilities, gate.mode, gate.now).await? {
            return Err(quality_package_required());
        }
    }
    let href = quality_alert_href(&alert.hold_id);
    Ok(Some(InboxTarget {
        href: Some(href.clone()),
        source: Some(json!({
            "kind": "quality_hold",
            "id": alert.hold_id,
            "href": href,
            "label": "Quality hold",
        })),
    }))
}

/// Retained legal proof: the `legal.read` permission, the scoped item read
/// (internal or member-visible, with the requester restriction), the
/// current Legal assignment, then the kind-capability purchase.
async fn legal_target(
    c: &mut PgConnection,
    gate: &QualityGate,
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
        Some((id, _)) => role_allows(c, gate.company.as_str(), id, "legal.read").await?,
        None => false,
    };
    if !readable {
        return Err(permission_required());
    }
    let role = match &assignment {
        Some((_, role)) => role.clone(),
        None => return Err(legal_access_changed()),
    };
    let requester = role == "legal_requester";
    let row: Option<(String, String)> = if requester {
        sqlx::query_as(
            "SELECT i.id,i.kind FROM legal_items i \
              WHERE i.id=$1 AND i.company_id=$2 \
              AND ($3::text IS NULL OR i.branch_id=$3) \
              AND ($4::text IS NULL OR i.department_id=$4) \
              AND (i.classification='internal' \
                OR EXISTS(SELECT 1 FROM legal_members lm \
                  WHERE lm.item_id=i.id AND lm.user_id=$5)) \
              AND i.kind='request' AND i.created_by=$6",
        )
        .bind(source_id)
        .bind(gate.company.as_str())
        .bind(gate.branch_id.as_deref())
        .bind(gate.department_id.as_deref())
        .bind(&gate.user_id)
        .bind(&gate.user_id)
        .fetch_optional(&mut *c)
        .await
        .map_err(|_| storage())?
    } else {
        sqlx::query_as(
            "SELECT i.id,i.kind FROM legal_items i \
              WHERE i.id=$1 AND i.company_id=$2 \
              AND ($3::text IS NULL OR i.branch_id=$3) \
              AND ($4::text IS NULL OR i.department_id=$4) \
              AND (i.classification='internal' \
                OR EXISTS(SELECT 1 FROM legal_members lm \
                  WHERE lm.item_id=i.id AND lm.user_id=$5))",
        )
        .bind(source_id)
        .bind(gate.company.as_str())
        .bind(gate.branch_id.as_deref())
        .bind(gate.department_id.as_deref())
        .bind(&gate.user_id)
        .fetch_optional(&mut *c)
        .await
        .map_err(|_| storage())?
    };
    let Some((_, kind)) = row else {
        return Err(legal_not_found());
    };
    let (assignment_id, role) = match assignment {
        Some((id, role)) => (id, role),
        None => return Err(legal_access_changed()),
    };
    if !scope_authority(
        c,
        gate.company.as_str(),
        &gate.user_id,
        &assignment_id,
        &role,
        "legal.read",
        gate.branch_id.as_deref(),
        gate.department_id.as_deref(),
        gate.now,
    )
    .await?
    {
        return Err(legal_access_changed());
    }
    if gate.enforced() {
        let capability = legal_kind_capability(&kind);
        if !history_allowed(c, &gate.company, &[capability], gate.mode, gate.now).await? {
            return Err(legal_package_required());
        }
    }
    Ok(Some(InboxTarget {
        href: Some(legal_record_href(source_id)),
        source: None,
    }))
}

impl InboxTargetPort for QualityInboxTargets {
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
            let gate = QualityGate {
                company,
                user_id: user_id.to_owned(),
                assignment_id: self.assignment_id.clone(),
                branch_id: self.branch_id.clone(),
                department_id: self.department_id.clone(),
                mode,
                now,
            };
            match source_type {
                "quality_hold" => hold_target(connection, &gate, source_id).await,
                "quality_alert" => alert_target(connection, &gate, source_id).await,
                "legal" => legal_target(connection, &gate, source_id).await,
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
    fn routing_covers_only_the_quality_slice() {
        for source_type in ["quality_hold", "quality_alert", "legal"] {
            assert!(is_quality_inbox_target(source_type), "{source_type}");
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
            "quality",
            "future_source",
            "",
        ] {
            assert!(!is_quality_inbox_target(other), "{other}");
        }
    }

    #[test]
    fn hrefs_encode_exactly_like_the_typescript_projection() {
        assert_eq!(quality_hold_href("hold-1"), "/quality/hold-1");
        assert_eq!(quality_hold_href("a/b c"), "/quality/a%2Fb%20c");
        assert_eq!(quality_alert_href("hold-1"), "/quality/hold-1");
        assert_eq!(quality_alert_href("a/b c"), "/quality/a%2Fb%20c");
        assert_eq!(legal_record_href("leg-1"), "/legal?record=leg-1");
        assert_eq!(legal_record_href("a/b c"), "/legal?record=a%2Fb%20c");
    }

    #[test]
    fn history_capabilities_match_the_retained_gates() {
        assert_eq!(
            quality_history_capabilities("quality_hold"),
            &[
                "supply.quality",
                "supply.receiving",
                "supply.stock",
                "sales.orders",
                "inventory.stock-core"
            ]
        );
        assert_eq!(
            quality_history_capabilities("quality_alert"),
            &[
                "supply.quality",
                "supply.receiving",
                "supply.stock",
                "sales.orders",
                "inventory.stock-core"
            ]
        );
        assert_eq!(
            quality_history_capabilities("legal"),
            &[
                "legal.contracts",
                "legal.documents",
                "legal.matters",
                "legal.obligations",
                "legal.access"
            ]
        );
        assert_eq!(quality_history_capabilities("quality"), &[] as &[&str]);
        assert_eq!(quality_history_capabilities(""), &[] as &[&str]);
    }

    #[test]
    fn legal_kinds_map_to_their_retained_capabilities() {
        assert_eq!(legal_kind_capability("contract"), "legal.contracts");
        assert_eq!(legal_kind_capability("template"), "legal.contracts");
        assert_eq!(legal_kind_capability("request"), "legal.matters");
        assert_eq!(legal_kind_capability("matter"), "legal.matters");
        assert_eq!(legal_kind_capability("counsel"), "legal.matters");
        assert_eq!(legal_kind_capability("ip"), "legal.matters");
        assert_eq!(legal_kind_capability("obligation"), "legal.obligations");
        assert_eq!(legal_kind_capability("compliance"), "legal.obligations");
        assert_eq!(legal_kind_capability("rule"), "legal.obligations");
        assert_eq!(legal_kind_capability("unknown"), "legal.obligations");
    }

    #[test]
    fn quality_kinds_keep_their_retained_labels() {
        assert_eq!(source_record_label("quality_hold"), "Quality hold");
        assert_eq!(source_record_label("quality_alert"), "Quality follow-up");
        assert_eq!(source_record_label("legal"), "Legal");
    }

    #[test]
    fn oracle_denials_keep_their_exact_codes_kinds_and_messages() {
        let permission = permission_required();
        assert_eq!(permission.kind, FailureKind::Forbidden);
        assert_eq!(permission.code, "PERMISSION_REQUIRED");
        let scope = quality_scope_required();
        assert_eq!(scope.kind, FailureKind::Forbidden);
        assert_eq!(scope.code, "QUALITY_SCOPE_REQUIRED");
        assert_eq!(
            scope.message,
            "Use a Company or Branch quality assignment."
        );
        let changed = quality_access_changed("Choose a current authorized Quality assignment.");
        assert_eq!(changed.kind, FailureKind::Forbidden);
        assert_eq!(changed.code, "QUALITY_ACCESS_CHANGED");
        let missing = hold_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "QUALITY_HOLD_NOT_FOUND");
        assert_eq!(missing.message, "Quality hold not found.");
        let source = hold_source_not_found();
        assert_eq!(source.code, "QUALITY_HOLD_NOT_FOUND");
        assert_eq!(
            source.message,
            "The original Quality source is unavailable."
        );
        let missing = alert_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "QUALITY_ALERT_NOT_FOUND");
        assert_eq!(
            missing.message,
            "Quality escalation is not available in this scope."
        );
        let missing = legal_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "LEGAL_NOT_FOUND");
        let changed = legal_access_changed();
        assert_eq!(changed.kind, FailureKind::Forbidden);
        assert_eq!(changed.code, "LEGAL_ACCESS_CHANGED");
        let package = legal_package_required();
        assert_eq!(package.kind, FailureKind::Forbidden);
        assert_eq!(package.code, "PACKAGE_REQUIRED");
    }
}
