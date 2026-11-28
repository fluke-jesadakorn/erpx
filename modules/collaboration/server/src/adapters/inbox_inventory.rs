//! Native Inventory workspace delegate for the Inbox destination port.
//!
//! This mirrors the retained per-row TypeScript gates for the
//! notification-visible Inventory and Sales sources:
//!
//! - `landed_cost`: `getLandedCost` — the readable check (`finance.read`,
//!   no department frame), the current Finance role, the landed-cost
//!   receipt-owner image, the advanced-inventory history decision, then the
//!   scoped cost read with its receipt/order/warehouse joins; 403
//!   `PERMISSION_REQUIRED` without the role, 403 `FINANCE_SCOPE_REQUIRED`
//!   on a department frame, 403 `LANDED_COST_ACCESS_CHANGED` without a
//!   current assignment, 404 `LANDED_COST_NOT_FOUND` when the image or the
//!   row is missing. The href carries the record's order id, exactly like
//!   the retained `` `/commerce/orders/${order_id}#landed-costs` ``.
//! - `stock_count`: `getCount` — the readable check (branch or holding
//!   inventory/finance assignment, no department frame), the current
//!   inventory role, the `supply.counts` history decision, the workspace
//!   image, then the scoped count read with its warehouse/settings/chart
//!   joins; 403 `INVENTORY_SCOPE_REQUIRED` without readable scope, 403
//!   `COUNT_ACCESS_CHANGED` without a current assignment, 404
//!   `COUNT_NOT_FOUND` when missing.
//! - `inventory_transit`: `getTransit` — the current inventory role, the
//!   `supply.transfers` history decision, the transfer-owner image, then
//!   the scoped transit read with its transfer/warehouse joins; 403
//!   `TRANSFER_ACCESS_CHANGED` without authority, 404
//!   `TRANSIT_NOT_FOUND` when the image or the row is missing.
//! - `customer_followup`: `getCustomerFollowup` — the quotation/sales
//!   source image, the current Sales role over the order scope, the
//!   order-kind history decision, then the scoped follow-up read with its
//!   order/customer joins; 404 `CUSTOMER_FOLLOWUP_SOURCE_NOT_FOUND` when
//!   the image is missing, 403 `FOLLOWUP_ACCESS_CHANGED` without
//!   authority, 403 `PACKAGE_REQUIRED` without history, 404
//!   `CUSTOMER_FOLLOWUP_NOT_FOUND` when the row is missing.
//!
//! None of the sources sets an override: the composition final map applies
//! the retained defaults.
//!
//! Nothing here opens a pool, commits, or retries. It runs serializable-safe
//! SELECTs on the caller's connection.
//!
//! Faithful-mirror notes (documented, not silent):
//! - Gate order matches each oracle exactly. Landed-cost runs authority,
//!   image, purchase, read; counts run authority, purchase, image, read;
//!   transits run authority, purchase, image, read; follow-ups run image,
//!   authority, purchase, read.
//! - The JWT-static `hasPermission` halves are enforced database-current via
//!   `iam_role_allows` on the exact (or highest active) assignment, literal
//!   mirror of the retained `assignmentReadSql` shape. Where the oracle
//!   rejects a department frame outright, the delegate fails with the same
//!   scope code before touching the database.
//! - Follow-up authority and purchase scope to the order behind the image
//!   (kind, branch, department), exactly like the oracle binding the
//!   record's scope. The enforced-mode rows projection re-proves the same
//!   facts, so it is subsumed by the image, authority and history proof.
//! - The command-side guards (`workContextGuards`, `iamMutationGuards`,
//!   assertion clearing, purchase boundaries, receipts, locks, deferred
//!   finance effects) authorize commands, not reads, and are not part of
//!   this read-only port.
//! - Presentation joins and facts (line/product detail, events, owner and
//!   candidate projections, loss and flow machinery) are not visibility:
//!   rows passing the identity proof resolve without them.
//! - Marked initial-physical-stock counts resolve their kind landing URL
//!   through the retained destination rewrite below once the mainstream
//!   `supply.counts` proof passes; quality-disposal counts keep their own
//!   owner purchase proofs and fail closed here unless the mainstream
//!   history is also granted.
//!
//! After the scoped count read resolves, the retained initial-stock
//! destination rewrite runs for that single id: a kind hit (sales,
//! manufacturing, POS, service) overrides BOTH the href and the source
//! href with the kind landing URL, exactly like
//! `projectInitialCountNotifications`. Ordinary counts keep the existing
//! `/stock-counts/{id}` href untouched.
//!
//! Faithful-mirror notes (documented, not silent):
//! - Disabled-mode JWT `PERMISSION_REQUIRED` denials are subsumed by the
//!   database-current authority codes, exactly like every other delegate:
//!   the role proof runs in every mode.
use crate::adapters::inbox_pg::{InboxTarget, InboxTargetFuture, InboxTargetPort};
use crate::adapters::inbox_targets::{encode_uri_component, retained_inbox_destination};
use erpx_commercial_server::public::capability_decision::PgCapabilityDecisionReader;
use erpx_commercial_server::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, current_package_catalogue,
};
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use sqlx::PgConnection;

/// Source types this delegate can prove against Inventory and Sales rows.
pub fn is_inventory_inbox_target(source_type: &str) -> bool {
    matches!(
        source_type,
        "landed_cost" | "stock_count" | "inventory_transit" | "customer_followup"
    )
}

/// Retained destination address for one landed cost: the record's order id
/// plus the exact `#landed-costs` fragment.
pub fn landed_cost_href(order_id: &str) -> String {
    format!(
        "/commerce/orders/{}#landed-costs",
        encode_uri_component(order_id)
    )
}

/// Retained destination address for one stock count, byte-identical to the
/// TypeScript `` `/stock-counts/${encodeURIComponent(id)}` ``.
pub fn stock_count_href(source_id: &str) -> String {
    format!("/stock-counts/{}", encode_uri_component(source_id))
}

/// Retained destination address for one transit, byte-identical to the
/// TypeScript `` `/stock-transit/${encodeURIComponent(id)}` ``.
pub fn inventory_transit_href(source_id: &str) -> String {
    format!("/stock-transit/{}", encode_uri_component(source_id))
}

/// Retained destination address for one follow-up, byte-identical to the
/// TypeScript `` `/customer-followups/${encodeURIComponent(id)}` ``.
pub fn customer_followup_href(source_id: &str) -> String {
    format!("/customer-followups/{}", encode_uri_component(source_id))
}

/// Retained initial-stock landing address per validated owner kind,
/// byte-identical to the TypeScript destinations map. `None` for unknown
/// kinds: ordinary counts keep their base href.
pub fn initial_count_landing(kind: &str, source_id: &str) -> Option<String> {
    let encoded = encode_uri_component(source_id);
    match kind {
        "sales" => Some(format!(
            "/customers-sales/sales/settings/initial-stock/{encoded}"
        )),
        "manufacturing" => Some(format!(
            "/production/manufacturing/settings/initial-stock/{encoded}"
        )),
        "pos" => Some(format!("/pos/settings/initial-stock/{encoded}")),
        "service" => Some(format!(
            "/services/field-service/settings/initial-stock/{encoded}"
        )),
        _ => None,
    }
}

/// Retained per-kind purchased capabilities for the initial-stock
/// destination proof, mirrored exactly from the oracle fragments.
pub fn initial_count_capabilities(kind: &str) -> &'static [&'static str] {
    match kind {
        "sales" => &[
            "sales.orders",
            "inventory.stock-core",
            "finance.sales-posting",
        ],
        "manufacturing" => &[
            "manufacturing.materials",
            "manufacturing.costs",
            "inventory.stock-core",
            "finance.production-costing",
        ],
        "pos" => &[
            "pos.sales",
            "inventory.stock-core",
            "finance.pos-posting",
        ],
        "service" => &[
            "service.orders",
            "service.parts-assets",
            "finance.service-invoice",
        ],
        _ => &[],
    }
}

/// Retained read capabilities per source type. Landed costs settle three
/// history capabilities together; follow-ups settle the order-kind
/// capability (quote or sales) resolved from the source image.
pub fn inventory_history_capabilities(source_type: &str) -> &'static [&'static str] {
    match source_type {
        "landed_cost" => &["supply.advanced-inventory", "supply.receiving", "supply.stock"],
        "stock_count" => &["supply.counts"],
        "inventory_transit" => &["supply.transfers"],
        "customer_followup" => &["sales.quotes", "sales.orders"],
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

fn finance_scope_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FINANCE_SCOPE_REQUIRED",
        "Use a Branch or Company Finance assignment.",
    )
}

fn landed_cost_access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "LANDED_COST_ACCESS_CHANGED",
        "The original landed-cost assignment is no longer current.",
    )
}

fn landed_cost_image_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "LANDED_COST_NOT_FOUND",
        "The original landed-cost receipt is unavailable in this workspace.",
    )
}

fn landed_cost_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "LANDED_COST_NOT_FOUND",
        "Landed cost record not found in your scope.",
    )
}

fn landed_cost_package_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PACKAGE_REQUIRED",
        "The original advanced inventory addon or retained history is required.",
    )
}

fn inventory_scope_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "INVENTORY_SCOPE_REQUIRED",
        "Use a branch or holding inventory or finance assignment.",
    )
}

fn count_access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "COUNT_ACCESS_CHANGED",
        "Choose a current inventory assignment.",
    )
}

fn count_image_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "COUNT_NOT_FOUND",
        "Stock count not found in this workspace.",
    )
}

fn count_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "COUNT_NOT_FOUND",
        "Stock count not found in your workspace.",
    )
}

fn count_package_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PACKAGE_REQUIRED",
        "Stock counts require the Supply purchase or retained history.",
    )
}

fn transfer_access_changed(message: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "TRANSFER_ACCESS_CHANGED",
        message,
    )
}

fn transit_image_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "TRANSIT_NOT_FOUND",
        "This warehouse movement is unavailable in the current workspace.",
    )
}

fn transit_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "TRANSIT_NOT_FOUND",
        "Transport record not found in this scope.",
    )
}

fn transit_package_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PACKAGE_REQUIRED",
        "Warehouse transfers require the Supply purchase or retained history.",
    )
}

fn followup_access_changed(message: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FOLLOWUP_ACCESS_CHANGED",
        message,
    )
}

fn followup_source_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CUSTOMER_FOLLOWUP_SOURCE_NOT_FOUND",
        "The actual customer quotation or Sales source is unavailable in this workspace.",
    )
}

fn followup_package_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PACKAGE_REQUIRED",
        "The actual quotation or Sales package history is required.",
    )
}

fn followup_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CUSTOMER_FOLLOWUP_NOT_FOUND",
        "Customer follow-up not found in this scope.",
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

/// Native Inventory visibility delegate, composed via
/// `PgInboxTargets::with_delegate`. Inventory-only: every other source type
/// defers to the existing fail-closed behavior instead of inventing a link.
pub struct InventoryInboxTargets {
    branch_id: Option<String>,
    department_id: Option<String>,
    assignment_id: Option<String>,
    enforcement: Option<PackageEnforcementMode>,
}

impl InventoryInboxTargets {
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

impl Default for InventoryInboxTargets {
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

struct InventoryGate {
    company: CompanyId,
    user_id: String,
    assignment_id: Option<String>,
    branch_id: Option<String>,
    department_id: Option<String>,
    mode: PackageEnforcementMode,
    now: i64,
}

impl InventoryGate {
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
        "finance.read" => "AND iam_role_allows(g.role,g.company_id,'finance.read')",
        "inventory.read" => "AND iam_role_allows(g.role,g.company_id,'inventory.read')",
        "commerce.read" => "AND iam_role_allows(g.role,g.company_id,'commerce.read')",
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

/// Retained history proof for one capability through the native Commercial
/// capability port. Shadow observes without blocking; enforced denies
/// without history. Callers map the refusal to the exact oracle code.
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

/// Retained landed-cost proof: readable role, no department frame, current
/// Finance assignment, the receipt-owner image, the advanced-inventory
/// history, then the scoped cost read. The href carries the record's order
/// id, exactly like the retained projection.
async fn landed_cost_target(
    c: &mut PgConnection,
    gate: &InventoryGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    let (assignment_id, role) = match resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?
    {
        Some((id, role)) => (id, role),
        None => return Err(permission_required()),
    };
    if !role_allows(c, gate.company.as_str(), &assignment_id, "finance.read").await? {
        return Err(permission_required());
    }
    if gate.department_id.is_some() {
        return Err(finance_scope_required());
    }
    if !scope_authority(
        c,
        gate.company.as_str(),
        &gate.user_id,
        &assignment_id,
        &role,
        "finance.read",
        gate.branch_id.as_deref(),
        None,
        gate.now,
    )
    .await?
    {
        return Err(landed_cost_access_changed());
    }
    if gate.enforced() {
        let image: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM inventory_landed_costs c \
              WHERE c.id=$1 AND c.company_id=$2 \
              AND ($3::text IS NULL OR c.branch_id=$3) \
              AND EXISTS( \
                SELECT 1 FROM commerce_fulfillments f \
                JOIN commerce_orders o ON o.id=f.order_id AND o.kind='purchase' \
                JOIN commerce_warehouses w ON w.id=o.warehouse_id \
                  AND w.company_id=o.company_id AND w.branch_id=o.branch_id \
                WHERE f.id=c.fulfillment_id \
                AND o.company_id=c.company_id AND o.branch_id=c.branch_id \
                AND EXISTS( \
                  SELECT 1 FROM commerce_fulfillment_lines fl \
                  JOIN commerce_order_lines ol ON ol.id=fl.order_line_id AND ol.order_id=o.id \
                    AND ol.item_type<>'service' \
                  WHERE fl.fulfillment_id=f.id))",
        )
        .bind(source_id)
        .bind(gate.company.as_str())
        .bind(gate.branch_id.as_deref())
        .fetch_optional(&mut *c)
        .await
        .map_err(|_| storage())?;
        if image.is_none() {
            return Err(landed_cost_image_not_found());
        }
        for capability in [
            "supply.advanced-inventory",
            "supply.receiving",
            "supply.stock",
        ] {
            if !history_allowed(c, &gate.company, capability, gate.mode, gate.now).await? {
                return Err(landed_cost_package_required());
            }
        }
    }
    let order_id: Option<String> = sqlx::query_scalar(
        "SELECT o.id FROM inventory_landed_costs c \
          JOIN commerce_fulfillments f ON f.id=c.fulfillment_id \
          JOIN commerce_orders o ON o.id=f.order_id \
          JOIN commerce_warehouses w ON w.id=o.warehouse_id \
          WHERE c.id=$1 AND c.company_id=$2 \
          AND ($3::text IS NULL OR c.branch_id=$3)",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(gate.branch_id.as_deref())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    let Some(order_id) = order_id else {
        return Err(landed_cost_not_found());
    };
    Ok(Some(InboxTarget {
        href: Some(landed_cost_href(&order_id)),
        source: None,
    }))
}

/// Retained initial-stock marker names, one per owner kind. The CASE
/// priority below (service, POS, manufacturing, sales) is literal mirror
/// of the oracle predicate.
const INITIAL_MARKERS: [(&str, &str); 4] = [
    ("service", "_serviceInitialPhysicalStockCount"),
    ("pos", "_posInitialPhysicalStockCount"),
    ("manufacturing", "_manufacturingInitialPhysicalStockCount"),
    ("sales", "_initialPhysicalStockCount"),
];

/// Retained sales initial-stock source rows, literal port of
/// `initialPhysicalStockCountSourceRows` with positional placeholders.
/// The fragment is fully self-contained (no caller input inside).
const INITIAL_SALES_SOURCE_ROWS: &str = r#"SELECT c.id,c.company_id,c.branch_id,c.warehouse_id,line.product_id,receipt.command_id source_command_id,receipt.result_json::jsonb->'_initialPhysicalStockCount' source_proof
 FROM inventory_counts c JOIN command_receipts receipt ON receipt.operation='inventory.count.create' AND receipt.record_id=c.id AND c.id='count-'||receipt.command_id AND receipt.actor_id=c.created_by AND receipt.authority_json::jsonb->>'companyId'=c.company_id
 JOIN inventory_count_lines line ON line.count_id=c.id
 JOIN command_receipts goods ON goods.command_id=receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'goodsSetupCommandId' AND goods.operation='commerce.sales-goods-posting-setup.initialize' AND goods.record_id=c.company_id AND goods.authority_json::jsonb->>'companyId'=c.company_id AND goods.result_json::jsonb->>'id'=c.company_id AND goods.result_json::jsonb->>'status'='configured' AND goods.result_json::jsonb->>'version'='1' AND goods.request_hash=receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'goodsSetupRequestHash'
 WHERE jsonb_exists(receipt.result_json::jsonb,'_initialPhysicalStockCount') AND receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'kind'='initial-physical-stock-count'
 AND receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'countId'=c.id AND receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'companyId'=c.company_id
 AND receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'branchId'=c.branch_id AND receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'warehouseId'=c.warehouse_id AND receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'productId'=line.product_id AND receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'countCommandId'=receipt.command_id
 AND receipt.result_json::jsonb->>'id'=c.id AND receipt.result_json::jsonb->>'status'='draft' AND receipt.result_json::jsonb->>'version'='1'
 AND line.snapshot_quantity_milli=0 AND line.snapshot_value_minor=0 AND line.snapshot_version=0 AND c.quality_hold_id IS NULL
 AND (SELECT count(*) FROM inventory_count_lines actual WHERE actual.count_id=c.id)=1"#;

/// Retained service initial-stock source rows, literal port of
/// `serviceInitialPhysicalStockCountSourceRows` (with its nested
/// service-parts-posting setup join and marker-exclusion clauses).
const INITIAL_SERVICE_SOURCE_ROWS: &str = r#"SELECT c.id,c.company_id,c.branch_id,c.warehouse_id,line.product_id,receipt.command_id source_command_id,receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount' source_proof
 FROM inventory_counts c JOIN command_receipts receipt ON receipt.operation='inventory.count.create' AND receipt.record_id=c.id AND c.id='count-'||receipt.command_id AND receipt.actor_id=c.created_by AND receipt.authority_json::jsonb->>'companyId'=c.company_id
 JOIN inventory_count_lines line ON line.count_id=c.id
 JOIN (SELECT receipt.command_id,receipt.record_id company_id,receipt.request_hash,receipt.result_json::jsonb->'_servicePartsPostingSetup' source_proof,receipt.result_json::jsonb->'_servicePartsPostingSetup'->'mapping' mapping
 FROM command_receipts receipt WHERE to_regclass('commerce_service_parts_posting_sources') IS NOT NULL
 AND receipt.operation IN ('commerce.service-parts-posting-setup.initialize','commerce.service-parts-posting-setup.confirm-existing')
 AND receipt.authority_json::jsonb->>'companyId'=receipt.record_id AND receipt.result_json::jsonb->>'id'=receipt.record_id AND receipt.result_json::jsonb->>'version'='1'
 AND receipt.result_json::jsonb->>'status'=CASE receipt.operation WHEN 'commerce.service-parts-posting-setup.initialize' THEN 'configured' ELSE 'confirmed-existing' END
 AND receipt.result_json::jsonb->'_servicePartsPostingSetup'->>'kind'='service-parts-posting-setup' AND receipt.result_json::jsonb->'_servicePartsPostingSetup'->>'companyId'=receipt.record_id AND receipt.result_json::jsonb->'_servicePartsPostingSetup'->>'commandId'=receipt.command_id AND receipt.result_json::jsonb->'_servicePartsPostingSetup'->'mapping'->>'company_id'=receipt.record_id) goods ON goods.command_id=receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'serviceSetupCommandId' AND goods.company_id=c.company_id AND goods.request_hash=receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'serviceSetupRequestHash'
 AND goods.mapping->>'chart_id'=receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'chartId' AND goods.mapping->>'inventory_account_id'=receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'inventoryAccountId' AND goods.mapping->>'currency'=receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'currency'
 WHERE to_regclass('inventory_service_initial_physical_sources') IS NOT NULL AND NOT jsonb_exists(receipt.result_json::jsonb,'_initialPhysicalStockCount') AND NOT jsonb_exists(receipt.result_json::jsonb,'_manufacturingInitialPhysicalStockCount') AND NOT jsonb_exists(receipt.result_json::jsonb,'_posInitialPhysicalStockCount') AND jsonb_exists(receipt.result_json::jsonb,'_serviceInitialPhysicalStockCount') AND receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'kind'='service-initial-physical-stock-count'
 AND receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'countId'=c.id AND receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'companyId'=c.company_id
 AND receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'branchId'=c.branch_id AND receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'warehouseId'=c.warehouse_id AND receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'productId'=line.product_id AND receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'countCommandId'=receipt.command_id
 AND receipt.result_json::jsonb->>'id'=c.id AND receipt.result_json::jsonb->>'status'='draft' AND receipt.result_json::jsonb->>'version'='1'
 AND line.snapshot_quantity_milli=0 AND line.snapshot_value_minor=0 AND line.snapshot_version=0 AND c.quality_hold_id IS NULL
 AND (SELECT count(*) FROM inventory_count_lines actual WHERE actual.count_id=c.id)=1"#;

/// Retained POS initial-stock source rows, literal port of
/// `posInitialPhysicalStockCountSourceRows` (with its nested POS setup,
/// bootstrap and approval receipt/audit joins).
const INITIAL_POS_SOURCE_ROWS: &str = r#"SELECT c.id,c.company_id,c.branch_id,c.warehouse_id,line.product_id,receipt.command_id source_command_id,receipt.result_json::jsonb->'_posInitialPhysicalStockCount' source_proof
 FROM inventory_counts c JOIN command_receipts receipt ON receipt.operation='inventory.count.create' AND receipt.record_id=c.id AND c.id='count-'||receipt.command_id AND receipt.actor_id=c.created_by AND receipt.authority_json::jsonb->>'companyId'=c.company_id
 JOIN inventory_count_lines line ON line.count_id=c.id
 JOIN command_receipts goods ON goods.command_id=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posSetupCommandId' AND goods.record_id=c.company_id AND goods.operation='commerce.pos-posting-setup.initialize' AND goods.authority_json::jsonb->>'companyId'=c.company_id AND goods.result_json::jsonb->>'id'=c.company_id AND goods.result_json::jsonb->>'status'='configured' AND goods.result_json::jsonb->>'version'='1' AND goods.request_hash=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posSetupRequestHash'
 JOIN pos_receipts bootstrap ON bootstrap.id=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posBootstrapReceiptId' AND bootstrap.company_id=c.company_id AND bootstrap.request_hash=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posBootstrapRequestHash'
 JOIN pos_audit bootstrap_audit ON bootstrap_audit.id=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posBootstrapAuditId' AND bootstrap_audit.company_id=c.company_id AND bootstrap_audit.record_id=c.company_id AND bootstrap_audit.operation='settings.bootstrap' AND bootstrap_audit.branch_id IS NULL AND bootstrap_audit.details_json::jsonb->'result'=bootstrap.result_json::jsonb AND bootstrap.result_json::jsonb->>'id'=c.company_id AND bootstrap.result_json::jsonb->>'warehouseId'=c.warehouse_id
 JOIN pos_receipts approval ON approval.id=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posApprovalReceiptId' AND approval.company_id=c.company_id AND approval.request_hash=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posApprovalRequestHash'
 JOIN pos_audit approval_audit ON approval_audit.id=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posApprovalAuditId' AND approval_audit.company_id=c.company_id AND approval_audit.record_id=c.company_id AND approval_audit.operation='accounting.approve' AND approval_audit.branch_id IS NULL AND approval_audit.actor_id<>bootstrap_audit.actor_id AND approval_audit.details_json::jsonb->'result'=approval.result_json::jsonb AND approval.result_json::jsonb->>'id'=c.company_id AND approval.result_json::jsonb->>'state'='approved'
 WHERE to_regclass('inventory_pos_initial_physical_sources') IS NOT NULL AND jsonb_exists(receipt.result_json::jsonb,'_posInitialPhysicalStockCount') AND NOT jsonb_exists(receipt.result_json::jsonb,'_initialPhysicalStockCount') AND NOT jsonb_exists(receipt.result_json::jsonb,'_manufacturingInitialPhysicalStockCount')
 AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'kind'='pos-initial-physical-stock-count' AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'countId'=c.id AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'companyId'=c.company_id AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'branchId'=c.branch_id AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'warehouseId'=c.warehouse_id AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'productId'=line.product_id AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'countCommandId'=receipt.command_id
 AND receipt.result_json::jsonb->>'id'=c.id AND receipt.result_json::jsonb->>'status'='draft' AND receipt.result_json::jsonb->>'version'='1' AND line.snapshot_quantity_milli=0 AND line.snapshot_value_minor=0 AND line.snapshot_version=0 AND c.quality_hold_id IS NULL AND (SELECT count(*) FROM inventory_count_lines actual WHERE actual.count_id=c.id)=1"#;

/// Retained manufacturing initial-stock source rows, literal port of
/// `manufacturingInitialPhysicalStockCountSourceRows` (with its nested
/// manufacturing-posting setup join covering both the initialize and
/// the existing-mapping receipt alternatives).
const INITIAL_MANUFACTURING_SOURCE_ROWS: &str = r#"SELECT c.id,c.company_id,c.branch_id,c.warehouse_id,line.product_id,receipt.command_id source_command_id,receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount' source_proof
 FROM inventory_counts c JOIN command_receipts receipt ON receipt.operation='inventory.count.create' AND receipt.record_id=c.id AND c.id='count-'||receipt.command_id AND receipt.actor_id=c.created_by AND receipt.authority_json::jsonb->>'companyId'=c.company_id
 JOIN inventory_count_lines line ON line.count_id=c.id
 JOIN (SELECT r.* FROM command_receipts r WHERE
 (r.operation='commerce.manufacturing-posting-setup.initialize' AND r.result_json::jsonb->>'id'=r.record_id AND r.result_json::jsonb->>'status'='configured' AND r.result_json::jsonb->>'version'='1' AND r.authority_json::jsonb->>'companyId'=r.record_id)
 OR (r.operation='commerce.manufacturing-posting-setup.confirm-existing' AND jsonb_exists(r.result_json::jsonb,'_manufacturingExistingPostingSetup')
 AND r.authority_json::jsonb->>'companyId'=r.record_id AND r.result_json::jsonb->>'id'=r.record_id AND r.result_json::jsonb->>'status'='confirmed-existing' AND r.result_json::jsonb->>'version'='1'
 AND r.result_json::jsonb->'_manufacturingExistingPostingSetup'->>'kind'='manufacturing-existing-posting-setup'
 AND r.result_json::jsonb->'_manufacturingExistingPostingSetup'->>'companyId'=r.record_id
 AND r.result_json::jsonb->'_manufacturingExistingPostingSetup'->>'confirmationCommandId'=r.command_id
 AND r.result_json::jsonb->'_manufacturingExistingPostingSetup'->'mapping'->>'company_id'=r.record_id)) goods ON goods.command_id=receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'manufacturingSetupCommandId' AND goods.record_id=c.company_id AND goods.authority_json::jsonb->>'companyId'=c.company_id AND goods.result_json::jsonb->>'id'=c.company_id AND goods.result_json::jsonb->>'version'='1' AND goods.request_hash=receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'manufacturingSetupRequestHash'
 AND (goods.operation='commerce.manufacturing-posting-setup.initialize' OR goods.result_json::jsonb->'_manufacturingExistingPostingSetup'->'mapping'->>'chart_id'=receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'chartId' AND goods.result_json::jsonb->'_manufacturingExistingPostingSetup'->'mapping'->>'inventory_account_id'=receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'inventoryAccountId')
 WHERE NOT jsonb_exists(receipt.result_json::jsonb,'_initialPhysicalStockCount') AND jsonb_exists(receipt.result_json::jsonb,'_manufacturingInitialPhysicalStockCount') AND receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'kind'='manufacturing-initial-physical-stock-count'
 AND receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'countId'=c.id AND receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'companyId'=c.company_id
 AND receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'branchId'=c.branch_id AND receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'warehouseId'=c.warehouse_id AND receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'productId'=line.product_id AND receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'countCommandId'=receipt.command_id
 AND receipt.result_json::jsonb->>'id'=c.id AND receipt.result_json::jsonb->>'status'='draft' AND receipt.result_json::jsonb->>'version'='1'
 AND line.snapshot_quantity_milli=0 AND line.snapshot_value_minor=0 AND line.snapshot_version=0 AND c.quality_hold_id IS NULL
 AND (SELECT count(*) FROM inventory_count_lines actual WHERE actual.count_id=c.id)=1"#;

fn initial_source_rows(kind: &str) -> &'static str {
    match kind {
        "sales" => INITIAL_SALES_SOURCE_ROWS,
        "manufacturing" => INITIAL_MANUFACTURING_SOURCE_ROWS,
        "pos" => INITIAL_POS_SOURCE_ROWS,
        "service" => INITIAL_SERVICE_SOURCE_ROWS,
        _ => "SELECT NULL::text id WHERE FALSE",
    }
}

/// Retained initial-stock destination proof for one already-visible count:
/// the department collapse, the `inventory.read` permission, the
/// queued-command authority stand-in, then the marker CASE, the source
/// proof and the per-kind purchased-capability history. Any miss collapses
/// to no override (the oracle returns `{}`), while storage failures throw.
///
/// The capability decisions are forced to enforced semantics: the oracle
/// predicate filters by grants/history with no mode switch, so disabled
/// mode must not open the override. The scheduled-feature predicates are
/// effectively `TRUE` outside production (same convention as the other
/// delegates) and are not re-proven.
async fn initial_count_destination(
    c: &mut PgConnection,
    gate: &InventoryGate,
    source_id: &str,
) -> Result<Option<String>, BusinessError> {
    if gate.department_id.is_some() {
        return Ok(None);
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
        Some((id, _)) => role_allows(c, gate.company.as_str(), id, "inventory.read").await?,
        None => false,
    };
    if !readable {
        return Ok(None);
    }
    let (assignment_id, role) = match assignment {
        Some((id, role)) => (id, role),
        None => return Ok(None),
    };
    if !scope_authority(
        c,
        gate.company.as_str(),
        &gate.user_id,
        &assignment_id,
        &role,
        "inventory.read",
        gate.branch_id.as_deref(),
        None,
        gate.now,
    )
    .await?
    {
        return Ok(None);
    }
    let mut kind: Option<&str> = None;
    for (candidate, marker) in INITIAL_MARKERS {
        let marked: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM command_receipts \
              WHERE operation='inventory.count.create' AND record_id=$1 \
              AND jsonb_exists(result_json::jsonb,$2)",
        )
        .bind(source_id)
        .bind(marker)
        .fetch_optional(&mut *c)
        .await
        .map_err(|_| storage())?;
        if marked.is_some() {
            kind = Some(candidate);
            break;
        }
    }
    let Some(kind) = kind else {
        return Ok(None);
    };
    let present: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT EXISTS(SELECT 1 FROM ({}) frag \
          WHERE frag.id=$1 AND frag.company_id=$2 \
          AND ($3::text IS NULL OR frag.branch_id=$3))",
        initial_source_rows(kind)
    )))
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(gate.branch_id.as_deref())
    .fetch_one(&mut *c)
    .await
    .map_err(|_| storage())?;
    if !present {
        return Ok(None);
    }
    for capability in initial_count_capabilities(kind) {
        if !history_allowed(
            c,
            &gate.company,
            capability,
            PackageEnforcementMode::Enforced,
            gate.now,
        )
        .await?
        {
            return Ok(None);
        }
    }
    Ok(initial_count_landing(kind, source_id))
}

/// Retained count proof: readable scope, current inventory assignment, the
/// `supply.counts` history, the workspace image, then the scoped count
/// read with its warehouse/settings/chart joins.
async fn count_target(
    c: &mut PgConnection,
    gate: &InventoryGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    if gate.department_id.is_some() {
        return Err(inventory_scope_required());
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
        Some((id, _)) => role_allows(c, gate.company.as_str(), id, "inventory.read").await?,
        None => false,
    };
    if !readable {
        return Err(inventory_scope_required());
    }
    let (assignment_id, role) = match assignment {
        Some((id, role)) => (id, role),
        None => return Err(count_access_changed()),
    };
    if !scope_authority(
        c,
        gate.company.as_str(),
        &gate.user_id,
        &assignment_id,
        &role,
        "inventory.read",
        gate.branch_id.as_deref(),
        None,
        gate.now,
    )
    .await?
    {
        return Err(count_access_changed());
    }
    if gate.enforced() {
        if !history_allowed(c, &gate.company, "supply.counts", gate.mode, gate.now).await? {
            return Err(count_package_required());
        }
        let image: Option<String> = sqlx::query_scalar(
            "SELECT id FROM inventory_counts \
              WHERE id=$1 AND company_id=$2 \
              AND ($3::text IS NULL OR branch_id=$3)",
        )
        .bind(source_id)
        .bind(gate.company.as_str())
        .bind(gate.branch_id.as_deref())
        .fetch_optional(&mut *c)
        .await
        .map_err(|_| storage())?;
        if image.is_none() {
            return Err(count_image_not_found());
        }
    }
    let visible: Option<String> = sqlx::query_scalar(
        "SELECT c.id FROM inventory_counts c \
          JOIN commerce_warehouses w ON w.id=c.warehouse_id \
          JOIN commerce_settings s ON s.company_id=c.company_id \
          JOIN finance_charts chart ON chart.id=s.chart_id AND chart.company_id=s.company_id \
          WHERE c.id=$1 AND c.company_id=$2 \
          AND ($3::text IS NULL OR c.branch_id=$3)",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(gate.branch_id.as_deref())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if visible.is_none() {
        return Err(count_not_found());
    }
    // Retained initial-stock rewrite, applied only after the scoped read
    // resolves: a kind hit overrides the href, exactly like
    // `projectInitialCountNotifications`. The source stays `None`, so a
    // present source href would be rewritten alongside (there is none).
    let href = match initial_count_destination(c, gate, source_id).await? {
        Some(landing) => landing,
        None => stock_count_href(source_id),
    };
    Ok(Some(InboxTarget {
        href: Some(href),
        source: None,
    }))
}

/// Retained transit proof: current inventory assignment, the
/// `supply.transfers` history, the transfer-owner image, then the scoped
/// transit read with its transfer/warehouse joins.
async fn transit_target(
    c: &mut PgConnection,
    gate: &InventoryGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    if gate.department_id.is_some() {
        return Err(transfer_access_changed(
            "Choose a current inventory assignment.",
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
        Some((id, _)) => role_allows(c, gate.company.as_str(), id, "inventory.read").await?,
        None => false,
    };
    if !readable {
        return Err(transfer_access_changed(
            "Choose a current authorized warehouse assignment.",
        ));
    }
    let (assignment_id, role) = match assignment {
        Some((id, role)) => (id, role),
        None => {
            return Err(transfer_access_changed(
                "The original warehouse assignment is no longer current.",
            ));
        }
    };
    if !scope_authority(
        c,
        gate.company.as_str(),
        &gate.user_id,
        &assignment_id,
        &role,
        "inventory.read",
        gate.branch_id.as_deref(),
        None,
        gate.now,
    )
    .await?
    {
        return Err(transfer_access_changed(
            "The original warehouse assignment is no longer current.",
        ));
    }
    if gate.enforced() {
        if !history_allowed(c, &gate.company, "supply.transfers", gate.mode, gate.now).await? {
            return Err(transit_package_required());
        }
        let image: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM inventory_transit d \
              JOIN inventory_transfers t ON t.id=d.transfer_id AND t.company_id=d.company_id \
              AND t.source_warehouse_id=d.source_warehouse_id \
              AND t.target_warehouse_id=d.target_warehouse_id \
              AND t.source_branch_id=d.source_branch_id \
              AND t.target_branch_id=d.target_branch_id \
              AND t.delivery_mode='transit' \
              WHERE d.id=$1 AND d.company_id=$2 \
              AND ($3::text IS NULL OR d.source_branch_id=$3 OR d.target_branch_id=$3)",
        )
        .bind(source_id)
        .bind(gate.company.as_str())
        .bind(gate.branch_id.as_deref())
        .fetch_optional(&mut *c)
        .await
        .map_err(|_| storage())?;
        if image.is_none() {
            return Err(transit_image_not_found());
        }
    }
    let visible: Option<String> = sqlx::query_scalar(
        "SELECT d.id FROM inventory_transit d \
          JOIN inventory_transfers t ON t.id=d.transfer_id \
          JOIN commerce_warehouses s ON s.id=d.source_warehouse_id \
          JOIN commerce_warehouses w ON w.id=d.target_warehouse_id \
          WHERE d.id=$1 AND d.company_id=$2 \
          AND ($3::text IS NULL OR d.source_branch_id=$3 OR d.target_branch_id=$3)",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(gate.branch_id.as_deref())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if visible.is_none() {
        return Err(transit_not_found());
    }
    Ok(Some(InboxTarget {
        href: Some(inventory_transit_href(source_id)),
        source: None,
    }))
}

/// Retained follow-up proof: the quotation/sales source image, the current
/// Sales role over the order scope, the order-kind history, then the scoped
/// follow-up read with its order/customer joins.
async fn followup_target(
    c: &mut PgConnection,
    gate: &InventoryGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    let image: Option<(Option<String>, Option<String>, String)> = if gate.enforced() {
        sqlx::query_as(
            "SELECT o.branch_id,o.department_id,o.kind FROM customer_followups f \
              JOIN commerce_orders o ON o.id=f.order_id AND o.company_id=f.company_id \
                AND o.kind IN ('quote','sales') \
              JOIN finance_parties p ON p.id=o.party_id AND p.company_id=o.company_id \
                AND p.kind='customer' \
              WHERE f.id=$1 AND f.company_id=$2 \
              AND ($3::text IS NULL OR o.branch_id=$3) \
              AND ($4::text IS NULL OR o.department_id=$4)",
        )
        .bind(source_id)
        .bind(gate.company.as_str())
        .bind(gate.branch_id.as_deref())
        .bind(gate.department_id.as_deref())
        .fetch_optional(&mut *c)
        .await
        .map_err(|_| storage())?
    } else {
        None
    };
    if gate.enforced() && image.is_none() {
        return Err(followup_source_not_found());
    }
    let (scope_branch, scope_department, order_kind) = match &image {
        Some((branch, department, kind)) => (
            branch.as_deref(),
            department.as_deref(),
            Some(kind.as_str()),
        ),
        None => (gate.branch_id.as_deref(), gate.department_id.as_deref(), None),
    };
    let assignment = resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?;
    let readable = match &assignment {
        Some((id, _)) => role_allows(c, gate.company.as_str(), id, "commerce.read").await?,
        None => false,
    };
    if !readable {
        return Err(followup_access_changed(
            "Choose a current authorized Sales role.",
        ));
    }
    let (assignment_id, role) = match assignment {
        Some((id, role)) => (id, role),
        None => {
            return Err(followup_access_changed(
                "The original customer follow-up assignment is no longer current.",
            ));
        }
    };
    if !scope_authority(
        c,
        gate.company.as_str(),
        &gate.user_id,
        &assignment_id,
        &role,
        "commerce.read",
        scope_branch,
        scope_department,
        gate.now,
    )
    .await?
    {
        return Err(followup_access_changed(
            "The original customer follow-up assignment is no longer current.",
        ));
    }
    if gate.enforced() {
        let capability = match order_kind {
            Some("quote") => "sales.quotes",
            _ => "sales.orders",
        };
        if !history_allowed(c, &gate.company, capability, gate.mode, gate.now).await? {
            return Err(followup_package_required());
        }
    }
    let visible: Option<String> = sqlx::query_scalar(
        "SELECT c.id FROM customer_followups c \
          JOIN commerce_orders o ON o.id=c.order_id AND o.company_id=c.company_id \
          JOIN finance_parties p ON p.id=o.party_id AND p.company_id=o.company_id \
          WHERE c.id=$1 AND o.company_id=$2 \
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
        return Err(followup_not_found());
    }
    Ok(Some(InboxTarget {
        href: Some(customer_followup_href(source_id)),
        source: None,
    }))
}

impl InboxTargetPort for InventoryInboxTargets {
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
            let gate = InventoryGate {
                company,
                user_id: user_id.to_owned(),
                assignment_id: self.assignment_id.clone(),
                branch_id: self.branch_id.clone(),
                department_id: self.department_id.clone(),
                mode,
                now,
            };
            match source_type {
                "landed_cost" => landed_cost_target(connection, &gate, source_id).await,
                "stock_count" => count_target(connection, &gate, source_id).await,
                "inventory_transit" => transit_target(connection, &gate, source_id).await,
                "customer_followup" => followup_target(connection, &gate, source_id).await,
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
    fn routing_covers_only_the_inventory_slice() {
        for source_type in [
            "landed_cost",
            "stock_count",
            "inventory_transit",
            "customer_followup",
        ] {
            assert!(is_inventory_inbox_target(source_type), "{source_type}");
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
            "supplier_bill",
            "inventory",
            "future_source",
            "",
        ] {
            assert!(!is_inventory_inbox_target(other), "{other}");
        }
    }

    #[test]
    fn hrefs_encode_exactly_like_the_typescript_projection() {
        assert_eq!(landed_cost_href("ord-1"), "/commerce/orders/ord-1#landed-costs");
        assert_eq!(
            landed_cost_href("a/b c"),
            "/commerce/orders/a%2Fb%20c#landed-costs"
        );
        assert_eq!(stock_count_href("cnt-1"), "/stock-counts/cnt-1");
        assert_eq!(stock_count_href("a/b c"), "/stock-counts/a%2Fb%20c");
        assert_eq!(inventory_transit_href("tr-1"), "/stock-transit/tr-1");
        assert_eq!(
            inventory_transit_href("a/b c"),
            "/stock-transit/a%2Fb%20c"
        );
        assert_eq!(customer_followup_href("fol-1"), "/customer-followups/fol-1");
        assert_eq!(
            customer_followup_href("a/b c"),
            "/customer-followups/a%2Fb%20c"
        );
    }

    #[test]
    fn history_capabilities_match_the_retained_gates() {
        assert_eq!(
            inventory_history_capabilities("landed_cost"),
            &["supply.advanced-inventory", "supply.receiving", "supply.stock"]
        );
        assert_eq!(
            inventory_history_capabilities("stock_count"),
            &["supply.counts"]
        );
        assert_eq!(
            inventory_history_capabilities("inventory_transit"),
            &["supply.transfers"]
        );
        assert_eq!(
            inventory_history_capabilities("customer_followup"),
            &["sales.quotes", "sales.orders"]
        );
        assert_eq!(inventory_history_capabilities("commerce_order"), &[] as &[&str]);
        assert_eq!(inventory_history_capabilities(""), &[] as &[&str]);
    }

    #[test]
    fn inventory_kinds_keep_their_retained_labels() {
        assert_eq!(source_record_label("landed_cost"), "Landed cost");
        assert_eq!(source_record_label("stock_count"), "Stock count");
        assert_eq!(source_record_label("inventory_transit"), "Inventory transit");
        assert_eq!(source_record_label("customer_followup"), "Customer followup");
    }

    #[test]
    fn initial_landings_encode_exactly_like_the_typescript_destinations() {
        assert_eq!(
            initial_count_landing("sales", "cnt-1"),
            Some("/customers-sales/sales/settings/initial-stock/cnt-1".to_owned())
        );
        assert_eq!(
            initial_count_landing("sales", "a/b c"),
            Some("/customers-sales/sales/settings/initial-stock/a%2Fb%20c".to_owned())
        );
        assert_eq!(
            initial_count_landing("manufacturing", "cnt-1"),
            Some("/production/manufacturing/settings/initial-stock/cnt-1".to_owned())
        );
        assert_eq!(
            initial_count_landing("pos", "cnt-1"),
            Some("/pos/settings/initial-stock/cnt-1".to_owned())
        );
        assert_eq!(
            initial_count_landing("service", "cnt-1"),
            Some("/services/field-service/settings/initial-stock/cnt-1".to_owned())
        );
        assert_eq!(initial_count_landing("ordinary", "cnt-1"), None);
        assert_eq!(initial_count_landing("", "cnt-1"), None);
    }

    #[test]
    fn initial_capabilities_match_the_retained_fragments() {
        assert_eq!(
            initial_count_capabilities("sales"),
            &[
                "sales.orders",
                "inventory.stock-core",
                "finance.sales-posting"
            ]
        );
        assert_eq!(
            initial_count_capabilities("manufacturing"),
            &[
                "manufacturing.materials",
                "manufacturing.costs",
                "inventory.stock-core",
                "finance.production-costing"
            ]
        );
        assert_eq!(
            initial_count_capabilities("pos"),
            &["pos.sales", "inventory.stock-core", "finance.pos-posting"]
        );
        assert_eq!(
            initial_count_capabilities("service"),
            &[
                "service.orders",
                "service.parts-assets",
                "finance.service-invoice"
            ]
        );
        assert_eq!(initial_count_capabilities("ordinary"), &[] as &[&str]);
        assert_eq!(initial_count_capabilities(""), &[] as &[&str]);
    }

    #[test]
    fn oracle_denials_keep_their_exact_codes_kinds_and_messages() {
        let permission = permission_required();
        assert_eq!(permission.kind, FailureKind::Forbidden);
        assert_eq!(permission.code, "PERMISSION_REQUIRED");
        let scope = finance_scope_required();
        assert_eq!(scope.kind, FailureKind::Forbidden);
        assert_eq!(scope.code, "FINANCE_SCOPE_REQUIRED");
        assert_eq!(
            scope.message,
            "Use a Branch or Company Finance assignment."
        );
        let changed = landed_cost_access_changed();
        assert_eq!(changed.kind, FailureKind::Forbidden);
        assert_eq!(changed.code, "LANDED_COST_ACCESS_CHANGED");
        let image = landed_cost_image_not_found();
        assert_eq!(image.kind, FailureKind::NotFound);
        assert_eq!(image.code, "LANDED_COST_NOT_FOUND");
        assert_eq!(
            image.message,
            "The original landed-cost receipt is unavailable in this workspace."
        );
        let missing = landed_cost_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "LANDED_COST_NOT_FOUND");
        assert_eq!(
            missing.message,
            "Landed cost record not found in your scope."
        );
        let package = landed_cost_package_required();
        assert_eq!(package.kind, FailureKind::Forbidden);
        assert_eq!(package.code, "PACKAGE_REQUIRED");
        let scope = inventory_scope_required();
        assert_eq!(scope.kind, FailureKind::Forbidden);
        assert_eq!(scope.code, "INVENTORY_SCOPE_REQUIRED");
        let changed = count_access_changed();
        assert_eq!(changed.kind, FailureKind::Forbidden);
        assert_eq!(changed.code, "COUNT_ACCESS_CHANGED");
        let image = count_image_not_found();
        assert_eq!(image.kind, FailureKind::NotFound);
        assert_eq!(image.code, "COUNT_NOT_FOUND");
        assert_eq!(
            image.message,
            "Stock count not found in this workspace."
        );
        let missing = count_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "COUNT_NOT_FOUND");
        assert_eq!(
            missing.message,
            "Stock count not found in your workspace."
        );
        let changed = transfer_access_changed("Choose a current inventory assignment.");
        assert_eq!(changed.kind, FailureKind::Forbidden);
        assert_eq!(changed.code, "TRANSFER_ACCESS_CHANGED");
        let image = transit_image_not_found();
        assert_eq!(image.kind, FailureKind::NotFound);
        assert_eq!(image.code, "TRANSIT_NOT_FOUND");
        assert_eq!(
            image.message,
            "This warehouse movement is unavailable in the current workspace."
        );
        let missing = transit_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "TRANSIT_NOT_FOUND");
        assert_eq!(
            missing.message,
            "Transport record not found in this scope."
        );
        let changed = followup_access_changed("Choose a current authorized Sales role.");
        assert_eq!(changed.kind, FailureKind::Forbidden);
        assert_eq!(changed.code, "FOLLOWUP_ACCESS_CHANGED");
        let source = followup_source_not_found();
        assert_eq!(source.kind, FailureKind::NotFound);
        assert_eq!(source.code, "CUSTOMER_FOLLOWUP_SOURCE_NOT_FOUND");
        let package = followup_package_required();
        assert_eq!(package.kind, FailureKind::Forbidden);
        assert_eq!(package.code, "PACKAGE_REQUIRED");
        assert_eq!(
            package.message,
            "The actual quotation or Sales package history is required."
        );
        let missing = followup_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "CUSTOMER_FOLLOWUP_NOT_FOUND");
        assert_eq!(
            missing.message,
            "Customer follow-up not found in this scope."
        );
    }
}
