//! Goods-receipt sources and Inventory-owned effects for the Commerce receipt
//! command. The caller already owns the purchase order, fulfillment, Finance
//! journal, authorization and command receipt; this adapter only reads current
//! source facts and applies stock/lot evidence inside that same transaction.
use crate::domain::stock_receipt::{
    MAX_LINES, MAX_LOTS_PER_LINE, MAX_SAFE, ReceiptDisposition, StockReceiptAppliedAllocation,
    StockReceiptAppliedLine, StockReceiptAppliedReceipt, StockReceiptLine, StockReceiptLot,
    StockReceiptPlan, StockReceiptQualityRequest, StockReceiptSource, StockReceiptStock,
    quality_request, validate_stock_receipt_plan,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

/// Quality-owned hold creation called between the Inventory lot insert and the
/// final allocation evidence. The implementation borrows the caller's existing
/// transaction; it cannot commit or obtain a second connection.
pub trait StockReceiptQualityPort {
    fn create_quality_hold<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        request: StockReceiptQualityRequest,
    ) -> PgWorkFuture<'a, String, BusinessError>;
}

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "STOCK_RECEIPT_SOURCE_CHANGED",
        "The purchase receipt source changed. Reload it and try again.",
    )
    .into()
}

fn missing() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "ORDER_NOT_FOUND",
        "The purchase order is not available in the selected Company.",
    )
    .into()
}

fn conflict(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::Conflict, code, message).into()
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

/// Fence every source relation the receipt reads or writes before the first
/// Holding data statement. Writers take the stronger intent; read-only sources
/// use the same statement so a concurrent receipt waits.
pub async fn lock_stock_receipt_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        // The receipt source's stock row derives reserved quantity from
        // inventory_active_reservations, which unions POS, Sales, Field Service
        // and Manufacturing reservations. Fence all four before the first read
        // so the displayed availability cannot change under the receipt unit.
        "LOCK TABLE commerce_orders,commerce_order_lines,commerce_fulfillments,\
         commerce_fulfillment_lines,commerce_stock,commerce_stock_moves,inventory_lots,\
         inventory_lot_balances,inventory_lot_moves,inventory_fulfillment_lot_allocations,\
         inventory_quality_holds,pos_reservations,inventory_order_reservations,\
         field_service_part_reservations,manufacturing_material_reservations \
         IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(&mut *c)
    .await?;
    sqlx::query(
        "LOCK TABLE commerce_products,commerce_warehouses,commerce_settings,finance_charts,         finance_parties IN SHARE MODE",
    )
    .execute(&mut *c)
    .await?;
    Ok(())
}

type OrderSqlRow = (
    String,
    String,
    String,
    String,
    i64,
    String,
    String,
    Option<String>,
    String,
    String,
    String,
);

const ORDER: &str = "SELECT o.id,o.number,o.kind,o.state,o.version,o.warehouse_id,w.branch_id,\
    o.department_id,o.order_date,o.currency,p.name party_name \
    FROM commerce_orders o JOIN commerce_warehouses w ON w.id=o.warehouse_id \
    JOIN finance_parties p ON p.id=o.party_id \
    WHERE o.id=$1 AND o.company_id=$2 AND o.kind='purchase' \
      AND ($3::text IS NULL OR w.branch_id=$3) \
      AND ($4::text IS NULL OR o.department_id=$4)";

type LineSqlRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    i64,
    i64,
    i64,
    i64,
    i64,
);

const LINES: &str = "SELECT l.id,l.product_id,p.sku,l.description,p.unit,p.tracking_mode,\
    l.quantity_milli,l.fulfilled_milli,l.price_minor,l.tax_bps,l.ordinal \
    FROM commerce_order_lines l JOIN commerce_products p ON p.id=l.product_id \
    WHERE l.order_id=$1 AND l.item_type='stock' ORDER BY l.ordinal,l.id LIMIT 201";

const STOCK: &str = "SELECT s.quantity_milli,s.value_minor,s.version,\
    COALESCE((SELECT SUM(r.quantity_milli)::bigint FROM inventory_active_reservations r \
      WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0) reserved_milli,\
    inventory_quarantined_quantity(s.warehouse_id,s.product_id) quarantined_milli \
    FROM commerce_stock s WHERE s.warehouse_id=$1 AND s.product_id=$2";

type LotSqlRow = (
    String,
    Option<String>,
    Option<String>,
    String,
    i64,
    i64,
    i64,
    i64,
    Option<String>,
);

const LOTS: &str = "SELECT l.id,l.lot_code,l.serial_number,l.status,b.quantity_milli,\
    b.value_minor,b.version,\
    LEAST(b.quantity_milli,COALESCE((SELECT SUM(COALESCE(h.held_quantity_milli,b.quantity_milli)) \
      FROM inventory_quality_holds h WHERE h.lot_id=l.id AND h.warehouse_id=b.warehouse_id \
        AND h.status NOT IN ('released','cancelled','disposed')),0))::bigint quality_hold_milli,\
    (SELECT h.status FROM inventory_quality_holds h WHERE h.lot_id=l.id \
      AND h.warehouse_id=b.warehouse_id AND h.status NOT IN ('released','cancelled','disposed') \
      ORDER BY h.created_at DESC,h.id DESC LIMIT 1) quality_status \
    FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id \
    WHERE l.company_id=$1 AND l.product_id=$2 AND b.warehouse_id=$3 \
    ORDER BY l.lot_code NULLS LAST,l.serial_number NULLS LAST,l.id LIMIT 201";

/// Read the current receipt source. `branch`/`department` are the already
/// resolved selection scope, never caller labels.
pub async fn read_stock_receipt_source(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    department: Option<&str>,
    order_id: &str,
) -> Result<StockReceiptSource, Error> {
    serializable(c).await?;
    if company.is_empty() || company.len() > 200 || order_id.is_empty() || order_id.len() > 200 {
        return Err(missing());
    }
    let row: Option<OrderSqlRow> = sqlx::query_as(ORDER)
        .bind(order_id)
        .bind(company)
        .bind(branch)
        .bind(department)
        .fetch_optional(&mut *c)
        .await?;
    let Some((
        id,
        number,
        kind,
        state,
        version,
        warehouse_id,
        branch_id,
        department_id,
        order_date,
        currency,
        party_name,
    )) = row
    else {
        return Err(missing());
    };
    let lines: Vec<LineSqlRow> = sqlx::query_as(LINES).bind(&id).fetch_all(&mut *c).await?;
    if lines.len() > MAX_LINES {
        return Err(conflict(
            "STOCK_RECEIPT_SOURCE_INVALID",
            "The purchase order has too many receipt lines.",
        ));
    }
    let mut result = Vec::with_capacity(lines.len());
    for (
        line_id,
        product_id,
        sku,
        description,
        unit,
        tracking_mode,
        quantity_milli,
        fulfilled_milli,
        price_minor,
        tax_bps,
        ordinal,
    ) in lines
    {
        let stock: Option<(i64, i64, i64, i64, i64)> = sqlx::query_as(STOCK)
            .bind(&warehouse_id)
            .bind(&product_id)
            .fetch_optional(&mut *c)
            .await?;
        let mut lots: Vec<LotSqlRow> = sqlx::query_as(LOTS)
            .bind(company)
            .bind(&product_id)
            .bind(&warehouse_id)
            .fetch_all(&mut *c)
            .await?;
        let lots_truncated = lots.len() > MAX_LOTS_PER_LINE;
        lots.truncate(MAX_LOTS_PER_LINE);
        result.push(StockReceiptLine {
            id: line_id,
            product_id,
            sku,
            description,
            unit,
            tracking_mode,
            quantity_milli,
            fulfilled_milli,
            remaining_milli: quantity_milli.saturating_sub(fulfilled_milli),
            price_minor,
            tax_bps,
            ordinal,
            stock: stock.map(
                |(quantity_milli, value_minor, version, reserved_milli, quarantined_milli)| {
                    StockReceiptStock {
                        quantity_milli,
                        value_minor,
                        version,
                        reserved_milli,
                        quarantined_milli,
                    }
                },
            ),
            lots: lots
                .into_iter()
                .map(
                    |(
                        id,
                        lot_code,
                        serial_number,
                        status,
                        quantity_milli,
                        value_minor,
                        version,
                        quality_hold_milli,
                        quality_status,
                    )| StockReceiptLot {
                        id,
                        lot_code,
                        serial_number,
                        status,
                        quantity_milli,
                        value_minor,
                        version,
                        quality_hold_milli,
                        quality_status,
                    },
                )
                .collect(),
            lots_truncated,
        });
    }
    Ok(StockReceiptSource {
        company_id: company.to_owned(),
        order: crate::domain::stock_receipt::StockReceiptOrder {
            id,
            number,
            kind,
            state,
            version,
            warehouse_id,
            branch_id,
            department_id,
            order_date,
            currency,
            party_name,
        },
        lines: result,
    })
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

fn lot_id(company: &str, product: &str, tracking: &str, identity: &str) -> String {
    let digest = Sha256::digest(format!("{company}:{product}:{tracking}:{identity}").as_bytes());
    format!("lot-{}", &hex(&digest)[..48])
}

fn allocation_id(fulfillment: &str, line: usize, allocation: usize) -> String {
    format!("lot-allocation-{fulfillment}-{line}-{allocation}")
}

fn lot_move_id(fulfillment: &str, line: usize, allocation: usize) -> String {
    format!("lot-move-{fulfillment}-{line}-{allocation}")
}

fn stock_move_id(fulfillment: &str, line: usize) -> String {
    format!("{fulfillment}-{line}")
}

fn fulfillment_line_id(fulfillment: &str, line: usize) -> String {
    format!("{fulfillment}-{line}")
}

async fn existing_stock_move(
    c: &mut PgConnection,
    id: &str,
    fulfillment_id: &str,
) -> Result<Option<(i64, i64)>, Error> {
    Ok(sqlx::query_as(
        "SELECT quantity_milli,value_minor FROM commerce_stock_moves \
         WHERE id=$1 AND fulfillment_id=$2",
    )
    .bind(id)
    .bind(fulfillment_id)
    .fetch_optional(&mut *c)
    .await?)
}

async fn current_stock(
    c: &mut PgConnection,
    warehouse: &str,
    product: &str,
) -> Result<Option<(i64, i64, i64)>, Error> {
    Ok(sqlx::query_as(
        "SELECT quantity_milli,value_minor,version FROM commerce_stock WHERE warehouse_id=$1 AND product_id=$2",
    )
    .bind(warehouse)
    .bind(product)
    .fetch_optional(&mut *c)
    .await?)
}

async fn apply_stock(
    c: &mut PgConnection,
    plan: &StockReceiptPlan,
    line: &crate::domain::stock_receipt::StockReceiptLinePlan,
) -> Result<i64, Error> {
    let current = current_stock(c, &plan.warehouse_id, &line.product_id).await?;
    let next_quantity = current
        .as_ref()
        .map_or(line.quantity_milli, |(quantity, _, _)| {
            quantity.saturating_add(line.quantity_milli)
        });
    let next_value = current
        .as_ref()
        .map_or(line.stock_value_minor, |(_, value, _)| {
            value.saturating_add(line.stock_value_minor)
        });
    if next_quantity > MAX_SAFE || next_value > MAX_SAFE {
        return Err(conflict(
            "STOCK_RECEIPT_VALUE_INVALID",
            "The received stock would exceed supported ledger precision.",
        ));
    }
    match (current, line.expected_stock_version) {
        (Some((_, _, version)), Some(expected)) if version == expected => {
            let result = sqlx::query(
                "UPDATE commerce_stock SET quantity_milli=quantity_milli+$1,\
                 value_minor=value_minor+$2,version=version+1 \
                 WHERE warehouse_id=$3 AND product_id=$4 AND version=$5",
            )
            .bind(line.quantity_milli)
            .bind(line.stock_value_minor)
            .bind(&plan.warehouse_id)
            .bind(&line.product_id)
            .bind(expected)
            .execute(&mut *c)
            .await?;
            if result.rows_affected() != 1 {
                return Err(changed());
            }
            Ok(expected + 1)
        }
        (Some(_), _) => Err(changed()),
        (None, None) => {
            let result = sqlx::query(
                "INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) \
                 VALUES($1,$2,$3,$4) ON CONFLICT(warehouse_id,product_id) DO NOTHING",
            )
            .bind(&plan.warehouse_id)
            .bind(&line.product_id)
            .bind(line.quantity_milli)
            .bind(line.stock_value_minor)
            .execute(&mut *c)
            .await?;
            if result.rows_affected() != 1 {
                return Err(changed());
            }
            Ok(1)
        }
        (None, Some(_)) => Err(changed()),
    }
}

#[derive(Clone, Debug)]
struct ExistingLot {
    id: String,
    status: String,
    balance_version: Option<i64>,
}

async fn find_lot(
    c: &mut PgConnection,
    company: &str,
    warehouse: &str,
    product: &str,
    tracking: &str,
    identity: &str,
) -> Result<Option<ExistingLot>, Error> {
    let row: Option<(String, String, Option<i64>)> = sqlx::query_as(
        "SELECT l.id,l.status,b.version FROM inventory_lots l \
         LEFT JOIN inventory_lot_balances b ON b.lot_id=l.id AND b.warehouse_id=$4 \
         WHERE l.company_id=$1 AND l.product_id=$2 AND l.tracking_mode=$3 \
           AND ((l.tracking_mode='lot' AND l.lot_code=$5) \
             OR (l.tracking_mode='serial' AND l.serial_number=$5)) LIMIT 1",
    )
    .bind(company)
    .bind(product)
    .bind(tracking)
    .bind(warehouse)
    .bind(identity)
    .fetch_optional(&mut *c)
    .await?;
    Ok(row.map(|(id, status, balance_version)| ExistingLot {
        id,
        status,
        balance_version,
    }))
}

async fn has_quality_hold(
    c: &mut PgConnection,
    lot_id: &str,
    warehouse: &str,
) -> Result<bool, Error> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM inventory_quality_holds WHERE lot_id=$1 AND warehouse_id=$2 \
         AND status NOT IN ('released','cancelled','disposed'))",
    )
    .bind(lot_id)
    .bind(warehouse)
    .fetch_one(&mut *c)
    .await?)
}

async fn insert_or_activate_lot(
    c: &mut PgConnection,
    plan: &StockReceiptPlan,
    line: &crate::domain::stock_receipt::StockReceiptLinePlan,
    existing: Option<&ExistingLot>,
    allocation: &crate::domain::stock_receipt::StockReceiptAllocationPlan,
    actor: &str,
    now: i64,
) -> Result<String, Error> {
    if let Some(existing) = existing {
        if allocation.expected_balance_version != existing.balance_version {
            return Err(changed());
        }
        if !matches!(existing.status.as_str(), "active" | "depleted") {
            return Err(conflict(
                "LOT_UNAVAILABLE",
                "This lot is no longer active. Receive a new lot or complete the retained disposition.",
            ));
        }
        if existing.status == "depleted" {
            sqlx::query(
                "UPDATE inventory_lots SET status='active' WHERE id=$1 AND status='depleted'",
            )
            .bind(&existing.id)
            .execute(&mut *c)
            .await?;
        }
        return Ok(existing.id.clone());
    }
    let identity = allocation
        .serial_number
        .as_deref()
        .or(allocation.lot_code.as_deref())
        .unwrap_or_default();
    let id = lot_id(
        &plan.company_id,
        &line.product_id,
        &line.tracking_mode,
        identity,
    );
    let result = sqlx::query(
        "INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,\
         serial_number,manufactured_date,expiry_date,created_by,created_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
    )
    .bind(&id)
    .bind(&plan.company_id)
    .bind(&line.product_id)
    .bind(&line.tracking_mode)
    .bind(&allocation.lot_code)
    .bind(&allocation.serial_number)
    .bind(&allocation.manufactured_date)
    .bind(&allocation.expiry_date)
    .bind(actor)
    .bind(now)
    .execute(&mut *c)
    .await;
    match result {
        Ok(_) => Ok(id),
        Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("23505") => {
            if line.tracking_mode == "serial" {
                Err(conflict(
                    "SERIAL_ALREADY_RECEIVED",
                    "This serial number already exists for the product.",
                ))
            } else {
                Err(conflict(
                    "LOT_ALLOCATION_DUPLICATE",
                    "This lot code already exists for the product.",
                ))
            }
        }
        Err(error) => Err(error.into()),
    }
}

async fn add_lot_balance(
    c: &mut PgConnection,
    plan: &StockReceiptPlan,
    lot_id: &str,
    allocation: &crate::domain::stock_receipt::StockReceiptAllocationPlan,
) -> Result<i64, Error> {
    if let Some(expected) = allocation.expected_balance_version {
        let result = sqlx::query(
            "UPDATE inventory_lot_balances SET quantity_milli=quantity_milli+$1,\
             value_minor=value_minor+$2,version=version+1 \
             WHERE lot_id=$3 AND warehouse_id=$4 AND version=$5",
        )
        .bind(allocation.quantity_milli)
        .bind(allocation.value_minor)
        .bind(lot_id)
        .bind(&plan.warehouse_id)
        .bind(expected)
        .execute(&mut *c)
        .await?;
        if result.rows_affected() != 1 {
            return Err(changed());
        }
        Ok(expected + 1)
    } else {
        let result = sqlx::query(
            "INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor) \
             VALUES($1,$2,$3,$4) ON CONFLICT(lot_id,warehouse_id) DO NOTHING",
        )
        .bind(lot_id)
        .bind(&plan.warehouse_id)
        .bind(allocation.quantity_milli)
        .bind(allocation.value_minor)
        .execute(&mut *c)
        .await?;
        if result.rows_affected() != 1 {
            return Err(changed());
        }
        Ok(1)
    }
}

/// Apply the Inventory half of a goods receipt. `quality` owns the Quality
/// hold write; the caller must have inserted the matching Commerce fulfillment
/// and fulfillment lines in this same transaction.
pub async fn apply_stock_receipt<Q: StockReceiptQualityPort>(
    c: &mut PgConnection,
    plan: &StockReceiptPlan,
    fulfillment_id: &str,
    command_key: &str,
    actor: &str,
    now: i64,
    quality: &mut Q,
) -> Result<StockReceiptAppliedReceipt, Error> {
    serializable(c).await?;
    validate_stock_receipt_plan(plan)?;
    if fulfillment_id.is_empty() || command_key.is_empty() || actor.is_empty() || now < 0 {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "STOCK_RECEIPT_COMMAND_INVALID",
            "Use a current receipt command.",
        )
        .into());
    }
    let order: Option<(i64, String, String, String, String)> = sqlx::query_as(
        "SELECT version,state,warehouse_id,branch_id,order_date FROM commerce_orders \
         WHERE id=$1 AND company_id=$2",
    )
    .bind(&plan.order_id)
    .bind(&plan.company_id)
    .fetch_optional(&mut *c)
    .await?;
    let Some((order_version, order_state, order_warehouse, order_branch, order_date)) = order
    else {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "ORDER_NOT_FOUND",
            "The purchase order is not available in the selected Company.",
        )
        .into());
    };
    if order_warehouse != plan.warehouse_id || order_branch != plan.branch_id {
        return Err(conflict(
            "STOCK_RECEIPT_SCOPE_CHANGED",
            "The receipt warehouse or branch changed. Reload its current sources.",
        ));
    }
    if plan.date < order_date {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "INVALID_COMPLETION_DATE",
            "The receipt date cannot precede the order date.",
        )
        .into());
    }
    let fulfillment: Option<String> = sqlx::query_scalar(
        "SELECT order_id FROM commerce_fulfillments WHERE id=$1 AND fulfillment_date=$2",
    )
    .bind(fulfillment_id)
    .bind(&plan.date)
    .fetch_optional(&mut *c)
    .await?;
    if fulfillment.as_deref() != Some(plan.order_id.as_str()) {
        return Err(conflict(
            "STOCK_RECEIPT_FULFILLMENT_REQUIRED",
            "The current Commerce fulfillment must exist in this transaction.",
        ));
    }
    let mut existing_moves = 0_usize;
    for (index, line) in plan.lines.iter().enumerate() {
        let move_values =
            existing_stock_move(c, &stock_move_id(fulfillment_id, index), fulfillment_id).await?;
        if let Some((quantity_milli, value_minor)) = move_values {
            if quantity_milli != line.quantity_milli || value_minor != line.stock_value_minor {
                return Err(conflict(
                    "STOCK_RECEIPT_REPLAY_MISMATCH",
                    "This fulfillment was already received with different quantities or values.",
                ));
            }
            existing_moves += 1;
        }
        let fulfillment_line: Option<(String, i64)> = sqlx::query_as(
            "SELECT id,quantity_milli FROM commerce_fulfillment_lines \
             WHERE id=$1 AND fulfillment_id=$2 AND order_line_id=$3",
        )
        .bind(fulfillment_line_id(fulfillment_id, index))
        .bind(fulfillment_id)
        .bind(&line.line_id)
        .fetch_optional(&mut *c)
        .await?;
        let Some((_, fulfillment_quantity)) = fulfillment_line else {
            return Err(conflict(
                "STOCK_RECEIPT_FULFILLMENT_REQUIRED",
                "The matching Commerce fulfillment line must exist in this transaction.",
            ));
        };
        if fulfillment_quantity != line.quantity_milli {
            return Err(conflict(
                "STOCK_RECEIPT_FULFILLMENT_REQUIRED",
                "The Commerce fulfillment line quantity does not match the receipt plan.",
            ));
        }
        let existing_allocations: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM inventory_fulfillment_lot_allocations \
             WHERE fulfillment_line_id=$1",
        )
        .bind(fulfillment_line_id(fulfillment_id, index))
        .fetch_one(&mut *c)
        .await?;
        if existing_allocations > 0 && move_values.is_none() {
            return Err(conflict(
                "STOCK_RECEIPT_REPLAY_INCOMPLETE",
                "A partial earlier receipt cannot be replayed.",
            ));
        }
        if move_values.is_some() && existing_allocations as usize != line.allocations.len() {
            return Err(conflict(
                "STOCK_RECEIPT_REPLAY_MISMATCH",
                "This fulfillment was already received with different lot allocations.",
            ));
        }
        let current_line: Option<(String, String, i64, i64)> = sqlx::query_as(
            "SELECT product_id,item_type,quantity_milli,fulfilled_milli \
             FROM commerce_order_lines WHERE id=$1 AND order_id=$2",
        )
        .bind(&line.line_id)
        .bind(&plan.order_id)
        .fetch_optional(&mut *c)
        .await?;
        let Some((product_id, item_type, _, _)) = current_line else {
            return Err(changed());
        };
        if product_id != line.product_id || item_type != "stock" {
            return Err(conflict(
                "STOCK_RECEIPT_SOURCE_CHANGED",
                "The selected purchase line is no longer a current stock line.",
            ));
        }
    }
    if existing_moves == plan.lines.len() {
        return replay(c, plan, fulfillment_id, existing_moves).await;
    }
    if existing_moves != 0 {
        return Err(conflict(
            "STOCK_RECEIPT_REPLAY_INCOMPLETE",
            "A partial earlier receipt cannot be replayed.",
        ));
    }
    if order_version != plan.order_version
        || !matches!(order_state.as_str(), "confirmed" | "partially_fulfilled")
    {
        return Err(changed());
    }
    let mut applied = Vec::with_capacity(plan.lines.len());
    for (index, line) in plan.lines.iter().enumerate() {
        let current: Option<(String, String, i64, i64)> = sqlx::query_as(
            "SELECT product_id,item_type,quantity_milli,fulfilled_milli \
             FROM commerce_order_lines WHERE id=$1 AND order_id=$2",
        )
        .bind(&line.line_id)
        .bind(&plan.order_id)
        .fetch_optional(&mut *c)
        .await?;
        let Some((product_id, item_type, ordered_milli, current_fulfilled)) = current else {
            return Err(changed());
        };
        if product_id != line.product_id
            || item_type != "stock"
            || current_fulfilled != line.expected_fulfilled_milli
        {
            return Err(changed());
        }
        if current_fulfilled
            .checked_add(line.quantity_milli)
            .is_none_or(|next| next > ordered_milli || next > MAX_SAFE)
        {
            return Err(BusinessError::new(
                FailureKind::InvalidInput,
                "OVER_FULFILLMENT",
                "Quantity exceeds the remaining purchase quantity.",
            )
            .into());
        }
        let stock_version = apply_stock(c, plan, line).await?;
        let mut allocations = Vec::with_capacity(line.allocations.len());
        for allocation in &line.allocations {
            let identity = allocation
                .serial_number
                .as_deref()
                .or(allocation.lot_code.as_deref())
                .unwrap_or_default();
            let existing = find_lot(
                c,
                &plan.company_id,
                &plan.warehouse_id,
                &line.product_id,
                &line.tracking_mode,
                identity,
            )
            .await?;
            if line.tracking_mode == "serial" && existing.is_some() {
                return Err(conflict(
                    "SERIAL_ALREADY_RECEIVED",
                    "This serial number already exists for the product.",
                ));
            }
            if let Some(existing) = existing.as_ref() {
                if allocation.expected_balance_version != existing.balance_version {
                    return Err(changed());
                }
                if allocation.disposition == ReceiptDisposition::Available
                    && has_quality_hold(c, &existing.id, &plan.warehouse_id).await?
                {
                    return Err(conflict(
                        "LOT_UNAVAILABLE",
                        "This lot is quarantined. Receive it as quarantine or complete Quality release first.",
                    ));
                }
            }
            let lot_id =
                insert_or_activate_lot(c, plan, line, existing.as_ref(), allocation, actor, now)
                    .await?;
            let balance_version = add_lot_balance(c, plan, &lot_id, allocation).await?;
            let allocation_id = allocation_id(fulfillment_id, index, allocation.allocation_index);
            let hold_id = if allocation.disposition == ReceiptDisposition::Quarantine {
                let request = quality_request(
                    plan,
                    line,
                    allocation,
                    allocation_id.clone(),
                    format!("{fulfillment_id}-{index}-{}", allocation.allocation_index),
                    lot_id.clone(),
                )
                .expect("quarantine allocation has a quality request");
                Some(quality.create_quality_hold(&mut *c, request).await?)
            } else {
                None
            };
            sqlx::query(
                "INSERT INTO inventory_fulfillment_lot_allocations(id,fulfillment_line_id,\
                 lot_id,quantity_milli,value_minor,disposition,quality_hold_id) \
                 VALUES($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(&allocation_id)
            .bind(fulfillment_line_id(fulfillment_id, index))
            .bind(&lot_id)
            .bind(allocation.quantity_milli)
            .bind(allocation.value_minor)
            .bind(match allocation.disposition {
                ReceiptDisposition::Available => "available",
                ReceiptDisposition::Quarantine => "quarantine",
            })
            .bind(&hold_id)
            .execute(&mut *c)
            .await?;
            let move_id = lot_move_id(fulfillment_id, index, allocation.allocation_index);
            sqlx::query(
                "INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,\
                 quantity_milli,value_minor,movement_date,created_by,created_at) \
                 VALUES($1,$2,$3,'goods_receipt',$4,$5,$6,$7,$8,$9)",
            )
            .bind(&move_id)
            .bind(&lot_id)
            .bind(&plan.warehouse_id)
            .bind(fulfillment_line_id(fulfillment_id, index))
            .bind(allocation.quantity_milli)
            .bind(allocation.value_minor)
            .bind(&plan.date)
            .bind(actor)
            .bind(now)
            .execute(&mut *c)
            .await?;
            allocations.push(StockReceiptAppliedAllocation {
                id: allocation_id,
                lot_id,
                lot_code: allocation.lot_code.clone(),
                serial_number: allocation.serial_number.clone(),
                quantity_milli: allocation.quantity_milli,
                value_minor: allocation.value_minor,
                disposition: match allocation.disposition {
                    ReceiptDisposition::Available => "available",
                    ReceiptDisposition::Quarantine => "quarantine",
                }
                .into(),
                quality_hold_id: hold_id,
                balance_version,
            });
        }
        let move_id = stock_move_id(fulfillment_id, index);
        sqlx::query(
            "INSERT INTO commerce_stock_moves(id,warehouse_id,product_id,fulfillment_id,\
             quantity_milli,value_minor,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(&move_id)
        .bind(&plan.warehouse_id)
        .bind(&line.product_id)
        .bind(fulfillment_id)
        .bind(line.quantity_milli)
        .bind(line.stock_value_minor)
        .bind(now)
        .execute(&mut *c)
        .await?;
        applied.push(StockReceiptAppliedLine {
            line_id: line.line_id.clone(),
            fulfillment_line_id: fulfillment_line_id(fulfillment_id, index),
            stock_move_id: move_id,
            stock_version,
            quantity_milli: line.quantity_milli,
            value_minor: line.stock_value_minor,
            allocations,
        });
    }
    Ok(StockReceiptAppliedReceipt {
        fulfillment_id: fulfillment_id.to_owned(),
        order_id: plan.order_id.clone(),
        replayed: false,
        lines: applied,
    })
}

async fn replay(
    c: &mut PgConnection,
    plan: &StockReceiptPlan,
    fulfillment_id: &str,
    _lines: usize,
) -> Result<StockReceiptAppliedReceipt, Error> {
    let mut applied = Vec::with_capacity(plan.lines.len());
    for (index, line) in plan.lines.iter().enumerate() {
        let move_values: Option<(i64, i64)> = sqlx::query_as(
            "SELECT quantity_milli,value_minor FROM commerce_stock_moves \
             WHERE id=$1 AND fulfillment_id=$2",
        )
        .bind(stock_move_id(fulfillment_id, index))
        .bind(fulfillment_id)
        .fetch_optional(&mut *c)
        .await?;
        let Some((move_quantity, move_value)) = move_values else {
            return Err(conflict(
                "STOCK_RECEIPT_REPLAY_INCOMPLETE",
                "A partial earlier receipt cannot be replayed.",
            ));
        };
        if move_quantity != line.quantity_milli || move_value != line.stock_value_minor {
            return Err(conflict(
                "STOCK_RECEIPT_REPLAY_MISMATCH",
                "This fulfillment was already received with different quantities or values.",
            ));
        }
        let stock_version: i64 = sqlx::query_scalar(
            "SELECT version FROM commerce_stock WHERE warehouse_id=$1 AND product_id=$2",
        )
        .bind(&plan.warehouse_id)
        .bind(&line.product_id)
        .fetch_optional(&mut *c)
        .await?
        .ok_or_else(changed)?;
        let mut allocations = Vec::with_capacity(line.allocations.len());
        for allocation in &line.allocations {
            let allocation_id = allocation_id(fulfillment_id, index, allocation.allocation_index);
            let row: Option<(String, Option<String>, i64, i64)> = sqlx::query_as(
                "SELECT lot_id,quality_hold_id,value_minor,quantity_milli \
                 FROM inventory_fulfillment_lot_allocations WHERE id=$1",
            )
            .bind(&allocation_id)
            .fetch_optional(&mut *c)
            .await?;
            let Some((persisted_lot_id, quality_hold_id, value_minor, quantity_milli)) = row else {
                return Err(conflict(
                    "STOCK_RECEIPT_REPLAY_INCOMPLETE",
                    "A partial earlier receipt cannot be replayed.",
                ));
            };
            let identity = allocation
                .serial_number
                .as_deref()
                .or(allocation.lot_code.as_deref())
                .unwrap_or_default();
            let expected_lot_id = allocation.existing_lot_id.clone().unwrap_or_else(|| {
                lot_id(
                    &plan.company_id,
                    &line.product_id,
                    &line.tracking_mode,
                    identity,
                )
            });
            let quarantine = allocation.disposition == ReceiptDisposition::Quarantine;
            if quantity_milli != allocation.quantity_milli
                || value_minor != allocation.value_minor
                || persisted_lot_id != expected_lot_id
                || quality_hold_id.is_some() != quarantine
            {
                return Err(conflict(
                    "STOCK_RECEIPT_REPLAY_MISMATCH",
                    "This fulfillment was already received with different lot allocations.",
                ));
            }
            let balance_version: Option<i64> = sqlx::query_scalar(
                "SELECT version FROM inventory_lot_balances WHERE lot_id=$1 AND warehouse_id=$2",
            )
            .bind(&persisted_lot_id)
            .bind(&plan.warehouse_id)
            .fetch_optional(&mut *c)
            .await?;
            allocations.push(StockReceiptAppliedAllocation {
                id: allocation_id,
                lot_id: persisted_lot_id,
                lot_code: allocation.lot_code.clone(),
                serial_number: allocation.serial_number.clone(),
                quantity_milli,
                value_minor,
                disposition: match allocation.disposition {
                    ReceiptDisposition::Available => "available",
                    ReceiptDisposition::Quarantine => "quarantine",
                }
                .into(),
                quality_hold_id,
                balance_version: balance_version.ok_or_else(changed)?,
            });
        }
        applied.push(StockReceiptAppliedLine {
            line_id: line.line_id.clone(),
            fulfillment_line_id: fulfillment_line_id(fulfillment_id, index),
            stock_move_id: stock_move_id(fulfillment_id, index),
            stock_version,
            quantity_milli: move_quantity,
            value_minor: move_value,
            allocations,
        });
    }
    Ok(StockReceiptAppliedReceipt {
        fulfillment_id: fulfillment_id.to_owned(),
        order_id: plan.order_id.clone(),
        replayed: true,
        lines: applied,
    })
}

/// Verify the exact Inventory evidence retained by [`apply_stock_receipt`] in
/// the caller's transaction. The Commerce command calls this after updating
/// its order and before returning; a missing or changed stock move, lot
/// balance, allocation or quality hold fails the whole unit.
pub async fn require_stock_receipt_applied(
    c: &mut PgConnection,
    plan: &StockReceiptPlan,
    applied: &StockReceiptAppliedReceipt,
) -> Result<(), Error> {
    serializable(c).await?;
    if applied.fulfillment_id.is_empty()
        || applied.order_id != plan.order_id
        || applied.lines.len() != plan.lines.len()
    {
        return Err(changed());
    }
    for (index, line) in plan.lines.iter().enumerate() {
        let applied_line = &applied.lines[index];
        if applied_line.line_id != line.line_id
            || applied_line.quantity_milli != line.quantity_milli
            || applied_line.value_minor != line.stock_value_minor
            || applied_line.allocations.len() != line.allocations.len()
        {
            return Err(changed());
        }
        let move_id = stock_move_id(&applied.fulfillment_id, index);
        let move_row: Option<(String, String, i64, i64)> = sqlx::query_as(
            "SELECT warehouse_id,product_id,quantity_milli,value_minor \
             FROM commerce_stock_moves WHERE id=$1 AND fulfillment_id=$2",
        )
        .bind(&move_id)
        .bind(&applied.fulfillment_id)
        .fetch_optional(&mut *c)
        .await?;
        let Some((warehouse_id, product_id, quantity_milli, value_minor)) = move_row else {
            return Err(changed());
        };
        if warehouse_id != plan.warehouse_id
            || product_id != line.product_id
            || quantity_milli != line.quantity_milli
            || value_minor != line.stock_value_minor
        {
            return Err(changed());
        }
        let stock: Option<(i64, i64, i64)> =
            current_stock(c, &plan.warehouse_id, &line.product_id).await?;
        let Some((_, _, stock_version)) = stock else {
            return Err(changed());
        };
        if stock_version < applied_line.stock_version {
            return Err(changed());
        }
        for (allocation_index, allocation) in line.allocations.iter().enumerate() {
            let applied_allocation = &applied_line.allocations[allocation_index];
            let allocation_id =
                allocation_id(&applied.fulfillment_id, index, allocation.allocation_index);
            let fulfillment_line_id = fulfillment_line_id(&applied.fulfillment_id, index);
            if applied_allocation.id != allocation_id {
                return Err(changed());
            }
            let row: Option<(String, i64, i64, String, Option<String>)> = sqlx::query_as(
                "SELECT lot_id,quantity_milli,value_minor,disposition,quality_hold_id \
                 FROM inventory_fulfillment_lot_allocations \
                 WHERE id=$1 AND fulfillment_line_id=$2",
            )
            .bind(&allocation_id)
            .bind(&fulfillment_line_id)
            .fetch_optional(&mut *c)
            .await?;
            let Some((lot_id, quantity_milli, value_minor, disposition, quality_hold_id)) = row
            else {
                return Err(changed());
            };
            let expected_disposition = match allocation.disposition {
                ReceiptDisposition::Available => "available",
                ReceiptDisposition::Quarantine => "quarantine",
            };
            if lot_id != applied_allocation.lot_id
                || quantity_milli != allocation.quantity_milli
                || value_minor != allocation.value_minor
                || disposition != expected_disposition
            {
                return Err(changed());
            }
            let lot_move_id =
                lot_move_id(&applied.fulfillment_id, index, allocation.allocation_index);
            let lot_move: Option<(String, i64, i64, String)> = sqlx::query_as(
                "SELECT lot_id,quantity_milli,value_minor,source_id FROM inventory_lot_moves \
                 WHERE id=$1 AND warehouse_id=$2 AND source_type='goods_receipt'",
            )
            .bind(&lot_move_id)
            .bind(&plan.warehouse_id)
            .fetch_optional(&mut *c)
            .await?;
            let Some((move_lot_id, move_quantity, move_value, source_id)) = lot_move else {
                return Err(changed());
            };
            if move_lot_id != lot_id
                || move_quantity != allocation.quantity_milli
                || move_value != allocation.value_minor
                || source_id != fulfillment_line_id
            {
                return Err(changed());
            }
            let lot: Option<(Option<String>, Option<String>, String)> = sqlx::query_as(
                "SELECT lot_code,serial_number,status FROM inventory_lots \
                 WHERE id=$1 AND company_id=$2 AND product_id=$3 AND tracking_mode=$4",
            )
            .bind(&lot_id)
            .bind(&plan.company_id)
            .bind(&line.product_id)
            .bind(&line.tracking_mode)
            .fetch_optional(&mut *c)
            .await?;
            let Some((lot_code, serial_number, status)) = lot else {
                return Err(changed());
            };
            if status != "active"
                || lot_code.as_deref() != allocation.lot_code.as_deref()
                || serial_number.as_deref() != allocation.serial_number.as_deref()
            {
                return Err(changed());
            }
            let balance: Option<(i64, i64, i64)> = sqlx::query_as(
                "SELECT quantity_milli,value_minor,version FROM inventory_lot_balances \
                 WHERE lot_id=$1 AND warehouse_id=$2",
            )
            .bind(&lot_id)
            .bind(&plan.warehouse_id)
            .fetch_optional(&mut *c)
            .await?;
            let Some((balance_quantity, balance_value, balance_version)) = balance else {
                return Err(changed());
            };
            if balance_quantity < allocation.quantity_milli
                || balance_value < allocation.value_minor
                || balance_version < applied_allocation.balance_version
            {
                return Err(changed());
            }
            if allocation.disposition == ReceiptDisposition::Quarantine {
                let hold_id = quality_hold_id.ok_or_else(changed)?;
                let hold: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM inventory_quality_holds \
                     WHERE id=$1 AND lot_id=$2 AND warehouse_id=$3 \
                     AND source_type='goods_receipt' AND source_id=$4 \
                     AND status NOT IN ('released','cancelled','disposed') \
                     AND held_quantity_milli>=$5)",
                )
                .bind(&hold_id)
                .bind(&lot_id)
                .bind(&plan.warehouse_id)
                .bind(format!("{}-{index}-{}", applied.fulfillment_id, allocation.allocation_index))
                .bind(allocation.quantity_milli)
                .fetch_one(&mut *c)
                .await?;
                if !hold {
                    return Err(changed());
                }
            } else if quality_hold_id.is_some() {
                return Err(changed());
            }
        }
    }
    Ok(())
}
