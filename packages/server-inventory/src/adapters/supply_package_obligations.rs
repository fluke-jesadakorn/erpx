//! Native Supply package-obligation read for the Inventory owner.
//!
//! The caller supplies the already-authorized Company scope and the owning
//! serializable transaction; authority is re-checked in the owner scope before
//! any read. Every fact comes from Inventory-owned relations plus the same
//! closed initial-marker, Quality-kind and terminal-receipt gates the TS read
//! uses. It never writes by itself.
use crate::domain::supply_package_obligations::{
    SupplyObligationFact, require_supply_inventory_scope, supply_obligation_outstanding,
    supply_obligation_revision,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn invalid(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::Unavailable, code, message).into()
}

async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("INVENTORY_OWNER_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("INVENTORY_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

/// Every source relation the projection reads, fenced for the duration of the
/// caller's serializable unit. Mirrors the TS `LOCK TABLE` list; Quality and
/// Files source locks stay with their own owners.
pub async fn lock_supply_inventory_package_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE commerce_stock,commerce_warehouses,inventory_lots,inventory_lot_balances, \
        inventory_counts,inventory_count_lines,inventory_count_lot_lines,inventory_count_decisions, \
        inventory_adjustment_moves,inventory_transfers,inventory_transfer_lines, \
        inventory_transfer_decisions,inventory_transfer_moves,inventory_transfer_lot_allocations,inventory_transit,inventory_transit_lines, \
        inventory_transit_events,inventory_transit_lots,inventory_transit_lot_events, \
        inventory_landed_costs,inventory_landed_cost_lines,inventory_landed_cost_lots, \
        inventory_landed_cost_events,inventory_landed_cost_moves,command_receipts IN SHARE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}

const INITIAL_MARKED_ROWS: &str = "SELECT record_id id,authority_json::jsonb->>'companyId' company_id FROM command_receipts WHERE operation='inventory.count.create' AND jsonb_exists(result_json::jsonb,'_initialPhysicalStockCount')";
const MANUFACTURING_MARKED_ROWS: &str = "SELECT record_id id,authority_json::jsonb->>'companyId' company_id FROM command_receipts WHERE operation='inventory.count.create' AND jsonb_exists(result_json::jsonb,'_manufacturingInitialPhysicalStockCount')";
const POS_MARKED_ROWS: &str = "SELECT record_id id,authority_json::jsonb->>'companyId' company_id FROM command_receipts WHERE operation='inventory.count.create' AND jsonb_exists(result_json::jsonb,'_posInitialPhysicalStockCount')";
const SERVICE_MARKED_ROWS: &str = "SELECT record_id id,authority_json::jsonb->>'companyId' company_id FROM command_receipts WHERE operation='inventory.count.create' AND jsonb_exists(result_json::jsonb,'_serviceInitialPhysicalStockCount')";

const RECORD_ATTACHMENT_STEPS: &str = "SELECT source.company_id,source.target_type,source.target_id, \
  source.expected_target_version previous_version,source.expected_target_version+1 next_version, \
  command.command_id,to_jsonb(event) event_image,to_jsonb(source) set_image \
  FROM attachment_sets source JOIN attachment_events event ON event.company_id=source.company_id AND event.set_id=source.id AND event.action='set.bound' \
  JOIN command_receipts command ON command.command_id=source.bound_command_id AND command.record_id=source.target_id AND command.actor_id=event.actor_id \
  AND command.authority_json::jsonb->>'companyId'=source.company_id \
  WHERE source.state='bound' AND event.details_json::jsonb->>'bindingId'=source.target_id AND \
  ((command.operation='attachment.record.bind' AND source.component_kind='record' AND event.details_json::jsonb->>'bindingType'='record_document' \
  AND command.result_json::jsonb->>'setId'=source.id AND command.result_json::jsonb->>'version'=(source.expected_target_version+1)::text) \
  OR(command.operation='attachment.compatibility.bind' AND event.details_json::jsonb->>'bindingType'='legacy_upload_adapter' \
  AND command.result_json::jsonb->>'attachmentSetId'=source.id AND command.result_json::jsonb->>'ownerVersion'=(source.expected_target_version+1)::text))";

const COMMERCE_RECEIPT_ORIGINS: &str = "SELECT 'receipt'::text kind,a.quality_hold_id hold_id,o.company_id,o.branch_id,o.warehouse_id,ol.product_id,a.lot_id,a.id source_id,o.id owner_id,a.quantity_milli,a.value_minor,NULL::text created_by \
  FROM inventory_fulfillment_lot_allocations a JOIN commerce_fulfillment_lines line ON line.id=a.fulfillment_line_id \
  JOIN commerce_order_lines ol ON ol.id=line.order_line_id AND ol.item_type<>'service' \
  JOIN commerce_fulfillments f ON f.id=line.fulfillment_id JOIN commerce_orders o ON o.id=f.order_id AND o.id=ol.order_id AND o.kind='purchase' \
  WHERE a.quality_hold_id IS NOT NULL AND a.disposition='quarantine'";

const COMMERCE_RETURNED_FROM: &str = "FROM commerce_returns r JOIN commerce_orders o ON o.id=r.order_id AND o.company_id=r.company_id AND o.kind='sales' \
  JOIN commerce_fulfillments f ON f.id=r.fulfillment_id AND f.order_id=o.id AND f.invoice_id=r.invoice_id \
  JOIN commerce_return_lines line ON line.return_id=r.id JOIN commerce_fulfillment_lines fl ON fl.id=line.fulfillment_line_id AND fl.fulfillment_id=f.id \
  JOIN commerce_order_lines ol ON ol.id=fl.order_line_id AND ol.order_id=o.id AND ol.product_id=line.product_id AND ol.item_type<>'service' \
  JOIN command_receipts receipt ON receipt.record_id=r.id AND receipt.operation='commerce.return.approve' AND receipt.actor_id=r.approved_by AND receipt.authority_json::jsonb->>'companyId'=r.company_id";

const QUALITY_LINEAGE_PARENTS: &str = "JOIN inventory_quality_holds parent ON parent.company_id=child.company_id AND parent.branch_id=child.branch_id AND parent.warehouse_id=child.warehouse_id AND parent.product_id=child.product_id AND parent.lot_id IS NOT DISTINCT FROM child.lot_id AND parent.created_by=child.created_by AND parent.spec_id IS NOT DISTINCT FROM child.spec_id AND parent.spec_version IS NOT DISTINCT FROM child.spec_version AND parent.source_type IS NOT DISTINCT FROM child.source_type AND parent.status='disposed' \
  JOIN inventory_counts c ON c.id=parent.disposition_count_id AND c.quality_hold_id=parent.id AND c.company_id=parent.company_id AND c.branch_id=parent.branch_id AND c.warehouse_id=parent.warehouse_id AND c.status='posted' \
  JOIN inventory_quality_disposition_events e ON e.hold_id=parent.id AND e.count_id=c.id AND e.action='post'";

fn commerce_quality_origin_rows() -> String {
    format!(
        r#"{receipt} UNION ALL SELECT 'customer-return'::text kind,'quality-return-'||receipt.command_id||'-'||line.ordinal::text hold_id,r.company_id,r.branch_id,o.warehouse_id,line.product_id,NULL::text lot_id,line.id source_id,r.id owner_id,line.quantity_milli,line.value_minor,r.approved_by created_by {returned} WHERE r.kind='customer_return' AND r.status IN ('approved','posted') AND NOT EXISTS(SELECT 1 FROM commerce_return_lot_allocations allocation WHERE allocation.return_line_id=line.id) UNION ALL SELECT 'customer-return'::text kind,'quality-return-'||receipt.command_id||'-'||line.ordinal::text||'-'||allocation.lot_ordinal::text hold_id,r.company_id,r.branch_id,o.warehouse_id,line.product_id,allocation.lot_id,allocation.id source_id,r.id owner_id,allocation.quantity_milli,allocation.value_minor,r.approved_by created_by {returned} JOIN (SELECT *,row_number() OVER (PARTITION BY return_line_id ORDER BY id)-1 lot_ordinal FROM commerce_return_lot_allocations) allocation ON allocation.return_line_id=line.id JOIN inventory_fulfillment_lot_allocations original ON original.id=allocation.fulfillment_allocation_id AND original.fulfillment_line_id=fl.id AND original.lot_id=allocation.lot_id WHERE r.kind='customer_return' AND r.status IN ('approved','posted') "#,
        receipt = COMMERCE_RECEIPT_ORIGINS,
        returned = COMMERCE_RETURNED_FROM,
    )
}

fn quality_purchase_source_rows() -> String {
    let origins = commerce_quality_origin_rows();
    format!(
        r#"WITH RECURSIVE lineage AS (
  SELECT id hold_id,id root_id,0 depth FROM inventory_quality_holds
  UNION ALL SELECT walk.hold_id,parent.id,walk.depth+1 FROM lineage walk JOIN inventory_quality_holds child ON child.id=walk.root_id {parents}
  WHERE walk.depth<64 AND child.id=parent.id||'-remaining-'||c.id AND child.source_id=parent.source_id||':remaining:'||c.id AND child.held_quantity_milli=parent.held_quantity_milli-parent.disposition_quantity_milli AND child.held_quantity_milli>0
  ), roots AS (SELECT DISTINCT ON(hold_id) hold_id,root_id FROM lineage ORDER BY hold_id,depth DESC)
  SELECT h.id,h.company_id,h.branch_id,h.warehouse_id,h.product_id,h.lot_id,root.id root_id,
  COALESCE(origin.kind,'advanced') kind,CASE WHEN origin.kind='customer-return' THEN 'inventory.stock-core' ELSE 'supply.stock' END stock_capability,
  CASE WHEN origin.kind='customer-return' THEN 'finance.sales-posting' ELSE 'finance.inventory-valuation' END finance_capability,
  jsonb_build_array(jsonb_build_object('id',h.id,'company',h.company_id,'branch',h.branch_id,'warehouse',h.warehouse_id,'product',h.product_id,'lot',h.lot_id,'sourceType',h.source_type,'sourceId',h.source_id,'spec',h.spec_id,'specVersion',h.spec_version,'creator',h.created_by,'quantity',h.held_quantity_milli),jsonb_build_object('id',root.id,'company',root.company_id,'branch',root.branch_id,'warehouse',root.warehouse_id,'product',root.product_id,'lot',root.lot_id,'sourceType',root.source_type,'sourceId',root.source_id,'spec',root.spec_id,'specVersion',root.spec_version,'creator',root.created_by,'quantity',root.held_quantity_milli),to_jsonb(origin)) witness
  FROM roots JOIN inventory_quality_holds h ON h.id=roots.hold_id JOIN inventory_quality_holds root ON root.id=roots.root_id
  LEFT JOIN ({origins}) origin ON root.spec_id IS NULL AND origin.hold_id=root.id AND origin.company_id=root.company_id AND origin.branch_id=root.branch_id AND origin.warehouse_id=root.warehouse_id AND origin.product_id=root.product_id AND origin.lot_id IS NOT DISTINCT FROM root.lot_id AND origin.quantity_milli=root.held_quantity_milli
  AND ((origin.kind='receipt' AND root.source_type='goods_receipt') OR (origin.kind='customer-return' AND root.source_type='customer_return' AND origin.source_id=root.source_id AND origin.created_by=root.created_by)) "#,
        parents = QUALITY_LINEAGE_PARENTS,
        origins = origins,
    )
}

fn supply_inventory_terminal_receipts() -> String {
    format!(
        r#"WITH terminals AS(
  SELECT 'count'::text kind,count.id,count.company_id,count.status,count.version,
  event.reviewed_version+1 terminal_version,command.command_id
  FROM inventory_counts count JOIN inventory_count_decisions event ON event.count_id=count.id
  JOIN command_receipts command ON command.command_id=event.command_id AND command.record_id=count.id AND command.actor_id=event.actor_id
  AND command.operation='inventory.count.'||event.action AND command.authority_json::jsonb->>'companyId'=count.company_id
  AND command.result_json::jsonb->>'status'=count.status AND command.result_json::jsonb->>'version'=(event.reviewed_version+1)::text
  WHERE count.status IN ('posted','cancelled','rejected') AND event.action=CASE count.status WHEN 'posted' THEN 'post' WHEN 'rejected' THEN 'reject' ELSE 'cancel' END
  UNION ALL SELECT 'landed-cost',cost.id,cost.company_id,cost.status,cost.version,event.reviewed_version+1,command.command_id
  FROM inventory_landed_costs cost JOIN inventory_landed_cost_events event ON event.landed_cost_id=cost.id
  JOIN command_receipts command ON command.command_id=event.command_id AND command.record_id=cost.id AND command.actor_id=event.actor_id
  AND command.operation='inventory.landed_cost.'||event.action AND command.authority_json::jsonb->>'companyId'=cost.company_id
  AND command.result_json::jsonb->>'status'=cost.status AND command.result_json::jsonb->>'version'=(event.reviewed_version+1)::text
  WHERE cost.status IN ('posted','cancelled','rejected') AND event.action=CASE cost.status WHEN 'posted' THEN 'post' WHEN 'rejected' THEN 'reject' ELSE 'cancel' END
  ) SELECT terminal.* FROM terminals terminal WHERE terminal.terminal_version<=terminal.version AND EXISTS(
  WITH RECURSIVE versions(version) AS(SELECT terminal.terminal_version UNION
  SELECT binding.next_version FROM versions JOIN ({steps}) binding ON binding.previous_version=versions.version
  AND binding.company_id=terminal.company_id AND binding.target_id=terminal.id
  AND (terminal.kind='count' AND binding.target_type IN ('stock_count','inventory_stock_count') OR terminal.kind='landed-cost' AND binding.target_type='landed_cost')
  WHERE binding.next_version<=terminal.version) SELECT 1 FROM versions WHERE version=terminal.version)"#,
        steps = RECORD_ATTACHMENT_STEPS,
    )
}

/// Authoritative Supply inventory obligation report. `package_version` is the
/// persisted grant version; `now` is the caller clock in millis.
#[derive(Clone, Debug)]
pub struct SupplyInventoryObligationReport {
    pub company_id: String,
    pub package_id: String,
    pub module_id: String,
    pub observed_at: i64,
    pub revision: String,
    pub outstanding: usize,
    pub facts: Vec<SupplyObligationFact>,
}

/// Read the current Supply inventory obligations on the caller's transaction.
/// Read-only; authority is re-checked in the owner scope before any snapshot.
pub async fn read_supply_inventory_package_obligations(
    c: &mut PgConnection,
    company: &str,
    package: &str,
    package_version: i64,
    module: &str,
    now: i64,
) -> Result<SupplyInventoryObligationReport, Error> {
    serializable(c).await?;
    require_supply_inventory_scope(company, package, package_version, module, now)
        .map_err(|e| -> Error { e.into() })?;
    let quality = quality_purchase_source_rows();
    let terminal = supply_inventory_terminal_receipts();
    let sql = format!(
        r#"WITH scope AS(SELECT $1::text company_id,$2::boolean full,$3::boolean quality_addon,$4::boolean advanced_addon),counts AS MATERIALIZED(
  SELECT count.* FROM inventory_counts count,scope WHERE count.company_id=scope.company_id AND NOT EXISTS(SELECT 1 FROM ({initial} UNION ALL {manufacturing} UNION ALL {pos} UNION ALL {service}) initial_marker WHERE initial_marker.id=count.id) AND (scope.full OR scope.quality_addon AND EXISTS(SELECT 1 FROM ({quality}) quality_source WHERE quality_source.id=count.quality_hold_id AND quality_source.company_id=count.company_id AND quality_source.kind='advanced')) AND NOT EXISTS(
  SELECT 1 FROM ({quality_again}) source WHERE source.id=count.quality_hold_id AND source.company_id=count.company_id AND source.kind='customer-return'))
  SELECT 'count' kind,c.id,jsonb_build_array(to_jsonb(c),
  (SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM inventory_count_lines line WHERE line.count_id=c.id),
  (SELECT jsonb_agg(to_jsonb(lot) ORDER BY lot.id) FROM inventory_count_lot_lines lot JOIN inventory_count_lines line ON line.id=lot.count_line_id WHERE line.count_id=c.id),
  (SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM inventory_count_decisions event WHERE event.count_id=c.id),
  (SELECT jsonb_agg(to_jsonb(move) ORDER BY move.id) FROM inventory_adjustment_moves move WHERE move.count_id=c.id)) fact,
  NOT COALESCE(EXISTS(SELECT 1 FROM ({terminal}) terminal WHERE terminal.kind='count' AND terminal.id=c.id AND terminal.company_id=c.company_id AND terminal.status=c.status AND terminal.version=c.version),false) pending
  FROM counts c
  UNION ALL SELECT 'transfer',t.id,jsonb_build_array(to_jsonb(t),
  (SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM inventory_transfer_lines line WHERE line.transfer_id=t.id),
  (SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM inventory_transfer_decisions event WHERE event.transfer_id=t.id),
  (SELECT jsonb_agg(to_jsonb(move) ORDER BY move.id) FROM inventory_transfer_moves move WHERE move.transfer_id=t.id)),
  NOT COALESCE(t.status IN ('posted','cancelled','rejected') AND EXISTS(SELECT 1 FROM inventory_transfer_decisions event JOIN command_receipts receipt
  ON receipt.command_id=event.command_id AND receipt.record_id=t.id AND receipt.actor_id=event.actor_id AND receipt.operation='inventory.transfer.'||event.action
  AND receipt.authority_json::jsonb->>'companyId'=t.company_id AND receipt.result_json::jsonb->>'status'=t.status
  WHERE event.transfer_id=t.id AND event.reviewed_version<t.version AND event.action=CASE t.status WHEN 'posted' THEN 'post' WHEN 'rejected' THEN 'reject' ELSE 'cancel' END)
  AND (t.status<>'posted' OR EXISTS(SELECT 1 FROM inventory_transfer_lines line WHERE line.transfer_id=t.id))
  AND (t.status<>'posted' OR t.delivery_mode<>'transit' OR EXISTS(SELECT 1 FROM inventory_transit transit WHERE transit.transfer_id=t.id AND transit.company_id=t.company_id)),false)
  FROM inventory_transfers t,scope WHERE scope.full AND t.company_id=scope.company_id
  UNION ALL SELECT 'transit',t.id,jsonb_build_array(to_jsonb(t),
  (SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM inventory_transit_lines line WHERE line.transit_id=t.id),
  (SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM inventory_transit_events event WHERE event.transit_id=t.id),
  (SELECT jsonb_agg(to_jsonb(lot) ORDER BY lot.id) FROM inventory_transit_lots lot JOIN inventory_transit_lines line ON line.id=lot.transit_line_id WHERE line.transit_id=t.id)),
  NOT COALESCE((t.status='cancelled' AND EXISTS(SELECT 1 FROM command_receipts receipt WHERE receipt.record_id=t.id AND receipt.operation='inventory.transit.cancel'
  AND receipt.authority_json::jsonb->>'companyId'=t.company_id AND receipt.result_json::jsonb->>'version'=t.version::text)
  AND EXISTS(SELECT 1 FROM inventory_transit_events event WHERE event.transit_id=t.id AND event.kind='cancel'))
  OR(t.status='completed' AND EXISTS(SELECT 1 FROM inventory_transit_lines line WHERE line.transit_id=t.id)
  AND NOT EXISTS(SELECT 1 FROM inventory_transit_lines line WHERE line.transit_id=t.id AND (line.received_milli+line.lost_milli<>line.quantity_milli OR line.settled_value_minor<>line.value_minor
  OR line.received_milli<>COALESCE((SELECT sum(event.quantity_milli) FROM inventory_transit_events event WHERE event.line_id=line.id AND event.kind='receipt'),0)
  OR line.lost_milli<>COALESCE((SELECT sum(event.quantity_milli) FROM inventory_transit_events event WHERE event.line_id=line.id AND event.kind='loss_approved'),0)))
  AND NOT EXISTS(SELECT 1 FROM inventory_transit_events proposal WHERE proposal.transit_id=t.id AND proposal.kind='loss_requested'
  AND NOT EXISTS(SELECT 1 FROM inventory_transit_events decision WHERE decision.proposal_id=proposal.id))),false)
  FROM inventory_transit t,scope WHERE scope.full AND t.company_id=scope.company_id
  UNION ALL SELECT 'landed-cost',cost.id,jsonb_build_array(to_jsonb(cost),
  (SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM inventory_landed_cost_lines line WHERE line.landed_cost_id=cost.id),
  (SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM inventory_landed_cost_events event WHERE event.landed_cost_id=cost.id),
  (SELECT jsonb_agg(to_jsonb(move) ORDER BY move.id) FROM inventory_landed_cost_moves move WHERE move.landed_cost_id=cost.id)),
  NOT COALESCE(EXISTS(SELECT 1 FROM ({terminal_again}) terminal WHERE terminal.kind='landed-cost' AND terminal.id=cost.id AND terminal.company_id=cost.company_id AND terminal.status=cost.status AND terminal.version=cost.version)
  AND (cost.status<>'posted' OR cost.amount_minor=COALESCE((SELECT sum(move.value_minor) FROM inventory_landed_cost_moves move WHERE move.landed_cost_id=cost.id),0)),false)
  FROM inventory_landed_costs cost,scope WHERE (scope.full OR scope.advanced_addon) AND cost.company_id=scope.company_id
  UNION ALL SELECT 'invalid-stock',concat(stock.warehouse_id,':',stock.product_id),to_jsonb(stock),stock.quantity_milli<0 OR stock.value_minor<0
  FROM commerce_stock stock JOIN commerce_warehouses warehouse ON warehouse.id=stock.warehouse_id CROSS JOIN scope WHERE warehouse.company_id=scope.company_id AND scope.full
  ORDER BY kind,id"#,
        initial = INITIAL_MARKED_ROWS,
        manufacturing = MANUFACTURING_MARKED_ROWS,
        pos = POS_MARKED_ROWS,
        service = SERVICE_MARKED_ROWS,
        quality = quality,
        quality_again = quality,
        terminal = terminal,
        terminal_again = terminal,
    );
    let full = package == "supply";
    let quality_addon = package == "supply-quality";
    let advanced_addon = package == "supply-advanced";
    let rows: Vec<(String, String, serde_json::Value, bool)> =
        sqlx::query_as(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(company)
            .bind(full)
            .bind(quality_addon)
            .bind(advanced_addon)
            .fetch_all(&mut *c)
            .await
            .map_err(|_| {
                invalid(
                    "SUPPLY_INVENTORY_PACKAGE_OBLIGATION_INVALID",
                    "The Supply inventory obligation source is unavailable.",
                )
            })?;
    let mut facts = Vec::with_capacity(rows.len());
    for (kind, id, fact, pending) in rows {
        if kind.is_empty() || kind.len() > 64 || id.is_empty() || id.len() > 400 {
            return Err(invalid(
                "SUPPLY_INVENTORY_PACKAGE_OBLIGATION_INVALID",
                "The Supply inventory obligation source is unavailable.",
            ));
        }
        let fact_json =
            serde_json::to_string(&fact).map_err(|_| {
                invalid(
                    "SUPPLY_INVENTORY_PACKAGE_OBLIGATION_INVALID",
                    "The Supply inventory obligation source is unavailable.",
                )
            })?;
        facts.push(SupplyObligationFact {
            kind,
            id,
            fact_json,
            pending,
        });
    }
    let outstanding = supply_obligation_outstanding(&facts);
    let revision = supply_obligation_revision(company, package_version, &facts);
    Ok(SupplyInventoryObligationReport {
        company_id: company.to_owned(),
        package_id: package.to_owned(),
        module_id: module.to_owned(),
        observed_at: now,
        revision,
        outstanding,
        facts,
    })
}

#[allow(dead_code)]
fn assert_fact_shape(row: &sqlx::postgres::PgRow) -> Result<(), Error> {
    let _: String = row.try_get("kind").map_err(|_| {
        invalid(
            "SUPPLY_INVENTORY_PACKAGE_OBLIGATION_INVALID",
            "The Supply inventory obligation source is unavailable.",
        )
    })?;
    Ok(())
}
