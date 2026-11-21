//! Sales-delivery sources and Inventory-owned effects for the Commerce
//! fulfillment command. The caller already owns the sales order, fulfillment,
//! Finance journal, authorization and command receipt; this adapter only reads
//! current source facts and applies stock, lot/serial balances and Sales
//! reservation consumption inside that same transaction.
//!
//! Required order inside the caller's serializable unit:
//! 1. `lock_stock_issue_sources`
//! 2. `read_stock_issue_source`
//! 3. `prepare_stock_issue` (the returned values are authoritative)
//! 4. insert `commerce_fulfillments`/`commerce_fulfillment_lines` using the plan
//!    quantities and values
//! 5. `apply_stock_issue`
//! 6. update the Commerce order line/state, Finance and command-receipt records
//!
//! Step 5 re-reads and revalidates the order, stock, lot and reservation facts;
//! a caller cannot substitute an HTTP read or trust plan labels/values.
use crate::domain::stock_issue::{
    MAX_LINES, MAX_LOTS_PER_LINE, MAX_RESERVATIONS_PER_LINE, MAX_SAFE, StockIssueAppliedAllocation,
    StockIssueAppliedLine, StockIssueAppliedReceipt, StockIssueLine, StockIssueLinePlan,
    StockIssueLot, StockIssuePlan, StockIssueReservation, StockIssueReservationPlan,
    StockIssueSource, StockIssueStock, validate_stock_issue_plan,
};
use crate::domain::valuation::scaled_inventory_value;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "STOCK_ISSUE_SOURCE_CHANGED",
        "The sales delivery source changed. Reload it and try again.",
    )
    .into()
}

fn missing() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "ORDER_NOT_FOUND",
        "The sales order is not available in the selected Company.",
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

/// Fence every source relation the delivery reads or writes before the first
/// Holding data statement. Writers take the stronger intent; read-only sources
/// use the same statement so a concurrent delivery waits.
pub async fn lock_stock_issue_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        // commerce_stock version is incremented by every reservation-source
        // trigger, so all four reservation tables must be fenced with the
        // stock, lot and fulfillment evidence this command reads and writes.
        "LOCK TABLE commerce_orders,commerce_order_lines,commerce_fulfillments,\
         commerce_fulfillment_lines,commerce_stock,commerce_stock_moves,inventory_lots,\
         inventory_lot_balances,inventory_lot_moves,inventory_fulfillment_lot_allocations,\
         inventory_quality_holds,pos_reservations,inventory_order_reservations,\
         field_service_part_reservations,manufacturing_material_reservations \
         IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(&mut *c)
    .await?;
    sqlx::query("LOCK TABLE commerce_products,commerce_warehouses,finance_parties IN SHARE MODE")
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
    WHERE o.id=$1 AND o.company_id=$2 AND o.kind='sales' \
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

type ReservationSqlRow = (String, String, i64, i64, i64, i64, bool);

const RESERVATIONS: &str = "SELECT r.id,r.order_line_id,r.quantity_milli,r.consumed_milli,\
    r.released_milli,r.expires_at,\
    (r.expires_at > floor(extract(epoch FROM statement_timestamp())*1000)::bigint) active \
    FROM inventory_order_reservations r \
    JOIN commerce_order_lines l ON l.id=r.order_line_id \
    JOIN commerce_orders o ON o.id=l.order_id \
    WHERE r.order_line_id=$1 AND r.warehouse_id=$2 AND r.product_id=$3 \
      AND o.company_id=$4 \
    ORDER BY r.created_at,r.id LIMIT 201";

/// Read the current delivery source. `branch`/`department` are the already
/// resolved selection scope, never caller labels.
pub async fn read_stock_issue_source(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    department: Option<&str>,
    order_id: &str,
) -> Result<StockIssueSource, Error> {
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
            "STOCK_ISSUE_SOURCE_INVALID",
            "The sales order has too many delivery lines.",
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
        let mut reservations: Vec<ReservationSqlRow> = sqlx::query_as(RESERVATIONS)
            .bind(&line_id)
            .bind(&warehouse_id)
            .bind(&product_id)
            .bind(company)
            .fetch_all(&mut *c)
            .await?;
        let reservations_truncated = reservations.len() > MAX_RESERVATIONS_PER_LINE;
        reservations.truncate(MAX_RESERVATIONS_PER_LINE);
        result.push(StockIssueLine {
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
                    StockIssueStock {
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
                    )| StockIssueLot {
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
            reservations: reservations
                .into_iter()
                .map(
                    |(
                        id,
                        order_line_id,
                        quantity_milli,
                        consumed_milli,
                        released_milli,
                        expires_at,
                        active,
                    )| {
                        StockIssueReservation {
                            id,
                            order_line_id,
                            quantity_milli,
                            consumed_milli,
                            released_milli,
                            expires_at,
                            active,
                        }
                    },
                )
                .collect(),
            reservations_truncated,
        });
    }
    Ok(StockIssueSource {
        company_id: company.to_owned(),
        order: crate::domain::stock_issue::StockIssueOrder {
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

fn stock_move_id(fulfillment: &str, line: usize) -> String {
    format!("{fulfillment}-{line}")
}

fn fulfillment_line_id(fulfillment: &str, line: usize) -> String {
    format!("{fulfillment}-{line}")
}

fn allocation_id(fulfillment: &str, line: usize, allocation: usize) -> String {
    format!("lot-allocation-{fulfillment}-{line}-{allocation}")
}

fn lot_move_id(fulfillment: &str, line: usize, allocation: usize) -> String {
    format!("lot-move-{fulfillment}-{line}-{allocation}")
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
        "SELECT quantity_milli,value_minor,version FROM commerce_stock \
         WHERE warehouse_id=$1 AND product_id=$2",
    )
    .bind(warehouse)
    .bind(product)
    .fetch_optional(&mut *c)
    .await?)
}

struct CurrentLot {
    lot_code: Option<String>,
    serial_number: Option<String>,
    status: String,
    quantity_milli: i64,
    value_minor: i64,
    version: i64,
    quality_hold_milli: i64,
}

type CurrentLotSqlRow = (Option<String>, Option<String>, String, i64, i64, i64, i64);

async fn current_lot(
    c: &mut PgConnection,
    company: &str,
    warehouse: &str,
    product: &str,
    lot_id: &str,
) -> Result<Option<CurrentLot>, Error> {
    let row: Option<CurrentLotSqlRow> = sqlx::query_as(
        "SELECT l.lot_code,l.serial_number,l.status,b.quantity_milli,b.value_minor,b.version,\
             COALESCE((SELECT SUM(COALESCE(h.held_quantity_milli,b.quantity_milli)) \
               FROM inventory_quality_holds h WHERE h.lot_id=l.id \
                 AND h.warehouse_id=b.warehouse_id \
                 AND h.status NOT IN ('released','cancelled','disposed')),0)::bigint \
             FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id \
             WHERE l.id=$1 AND l.company_id=$2 AND l.product_id=$3 AND b.warehouse_id=$4",
    )
    .bind(lot_id)
    .bind(company)
    .bind(product)
    .bind(warehouse)
    .fetch_optional(&mut *c)
    .await?;
    Ok(row.map(
        |(
            lot_code,
            serial_number,
            status,
            quantity_milli,
            value_minor,
            version,
            quality_hold_milli,
        )| CurrentLot {
            lot_code,
            serial_number,
            status,
            quantity_milli,
            value_minor,
            version,
            quality_hold_milli,
        },
    ))
}

async fn current_reservations(
    c: &mut PgConnection,
    company: &str,
    warehouse: &str,
    product: &str,
    line_id: &str,
) -> Result<Vec<StockIssueReservationPlan>, Error> {
    let rows: Vec<ReservationSqlRow> = sqlx::query_as(RESERVATIONS)
        .bind(line_id)
        .bind(warehouse)
        .bind(product)
        .bind(company)
        .fetch_all(&mut *c)
        .await?;
    if rows.len() > MAX_RESERVATIONS_PER_LINE {
        return Err(conflict(
            "STOCK_ISSUE_SOURCE_INVALID",
            "The sales line has too many reservations.",
        ));
    }
    Ok(rows
        .into_iter()
        .filter(|(_, _, quantity, consumed, released, _, active)| {
            *active && quantity.saturating_sub(*consumed).saturating_sub(*released) > 0
        })
        .map(
            |(id, order_line_id, quantity_milli, consumed_milli, released_milli, expires_at, _)| {
                StockIssueReservationPlan {
                    id,
                    order_line_id,
                    quantity_milli,
                    consumed_milli,
                    released_milli,
                    expires_at,
                    remaining_milli: quantity_milli
                        .saturating_sub(consumed_milli)
                        .saturating_sub(released_milli),
                }
            },
        )
        .collect())
}

fn same_reservations(
    planned: &[StockIssueReservationPlan],
    current: &[StockIssueReservationPlan],
) -> bool {
    planned.len() == current.len()
        && planned.iter().zip(current).all(|(left, right)| {
            left.id == right.id
                && left.order_line_id == right.order_line_id
                && left.quantity_milli == right.quantity_milli
                && left.consumed_milli == right.consumed_milli
                && left.released_milli == right.released_milli
                && left.expires_at == right.expires_at
                && left.remaining_milli == right.remaining_milli
        })
}

async fn consume_reservations(
    c: &mut PgConnection,
    line: &StockIssueLinePlan,
) -> Result<i64, Error> {
    let mut remaining = line.quantity_milli;
    let mut consumed_total = 0_i64;
    for reservation in &line.reservations {
        if remaining == 0 {
            break;
        }
        let take = remaining.min(reservation.remaining_milli);
        if take <= 0 {
            continue;
        }
        let result = sqlx::query(
            "UPDATE inventory_order_reservations SET consumed_milli=consumed_milli+$1 \
             WHERE id=$2 AND order_line_id=$3 AND quantity_milli=$4 \
               AND consumed_milli=$5 AND released_milli=$6",
        )
        .bind(take)
        .bind(&reservation.id)
        .bind(&reservation.order_line_id)
        .bind(reservation.quantity_milli)
        .bind(reservation.consumed_milli)
        .bind(reservation.released_milli)
        .execute(&mut *c)
        .await?;
        if result.rows_affected() != 1 {
            return Err(changed());
        }
        remaining -= take;
        consumed_total += take;
    }
    Ok(consumed_total)
}

fn tracking_identity_valid(tracking: &str, lot: &CurrentLot) -> bool {
    match tracking {
        "lot" => lot.serial_number.is_none() && lot.lot_code.is_some(),
        "serial" => lot.lot_code.is_none() && lot.serial_number.is_some(),
        _ => false,
    }
}

async fn apply_lot_allocations(
    c: &mut PgConnection,
    plan: &StockIssuePlan,
    line: &StockIssueLinePlan,
    index: usize,
    fulfillment_id: &str,
    actor: &str,
    now: i64,
) -> Result<Vec<StockIssueAppliedAllocation>, Error> {
    let mut applied = Vec::with_capacity(line.allocations.len());
    for allocation in &line.allocations {
        let lot = current_lot(
            c,
            &plan.company_id,
            &plan.warehouse_id,
            &line.product_id,
            &allocation.lot_id,
        )
        .await?
        .ok_or_else(changed)?;
        if lot.status != "active"
            || lot.quality_hold_milli > 0
            || lot.version != allocation.expected_balance_version
            || lot.quantity_milli < allocation.quantity_milli
            || lot.value_minor < allocation.value_minor
            || !tracking_identity_valid(&line.tracking_mode, &lot)
        {
            return Err(conflict(
                "LOT_UNAVAILABLE",
                "The selected lot or serial changed, is quarantined, or has insufficient quantity.",
            ));
        }
        let expected_value_minor = if allocation.quantity_milli == lot.quantity_milli {
            lot.value_minor
        } else {
            scaled_inventory_value(
                lot.value_minor,
                allocation.quantity_milli,
                lot.quantity_milli,
            )
            .map_err(|_| {
                conflict(
                    "STOCK_ISSUE_VALUE_INVALID",
                    "The delivery value is outside the supported range.",
                )
            })?
        };
        if expected_value_minor != allocation.value_minor {
            return Err(conflict(
                "STOCK_ISSUE_VALUE_INVALID",
                "The delivery value does not match the current lot valuation.",
            ));
        }
        let result = sqlx::query(
            "UPDATE inventory_lot_balances SET quantity_milli=quantity_milli-$1,\
             value_minor=value_minor-$2,version=version+1 \
             WHERE lot_id=$3 AND warehouse_id=$4 AND version=$5 \
               AND quantity_milli>=$1 AND value_minor>=$2",
        )
        .bind(allocation.quantity_milli)
        .bind(allocation.value_minor)
        .bind(&allocation.lot_id)
        .bind(&plan.warehouse_id)
        .bind(allocation.expected_balance_version)
        .execute(&mut *c)
        .await?;
        if result.rows_affected() != 1 {
            return Err(changed());
        }
        sqlx::query(
            "UPDATE inventory_lots SET status=CASE WHEN NOT EXISTS(\
               SELECT 1 FROM inventory_lot_balances WHERE lot_id=$1 AND quantity_milli>0\
             ) THEN 'depleted' ELSE status END WHERE id=$1",
        )
        .bind(&allocation.lot_id)
        .execute(&mut *c)
        .await?;
        let id = allocation_id(fulfillment_id, index, allocation.allocation_index);
        sqlx::query(
            "INSERT INTO inventory_fulfillment_lot_allocations(id,fulfillment_line_id,\
             lot_id,quantity_milli,value_minor,disposition,quality_hold_id) \
             VALUES($1,$2,$3,$4,$5,'available',NULL)",
        )
        .bind(&id)
        .bind(fulfillment_line_id(fulfillment_id, index))
        .bind(&allocation.lot_id)
        .bind(allocation.quantity_milli)
        .bind(allocation.value_minor)
        .execute(&mut *c)
        .await?;
        sqlx::query(
            "INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,\
             quantity_milli,value_minor,movement_date,created_by,created_at) \
             VALUES($1,$2,$3,'delivery',$4,$5,$6,$7,$8,$9)",
        )
        .bind(lot_move_id(
            fulfillment_id,
            index,
            allocation.allocation_index,
        ))
        .bind(&allocation.lot_id)
        .bind(&plan.warehouse_id)
        .bind(fulfillment_line_id(fulfillment_id, index))
        .bind(-allocation.quantity_milli)
        .bind(-allocation.value_minor)
        .bind(&plan.date)
        .bind(actor)
        .bind(now)
        .execute(&mut *c)
        .await?;
        applied.push(StockIssueAppliedAllocation {
            id,
            lot_id: allocation.lot_id.clone(),
            lot_code: lot.lot_code,
            serial_number: lot.serial_number,
            quantity_milli: allocation.quantity_milli,
            value_minor: allocation.value_minor,
            balance_version: allocation.expected_balance_version + 1,
        });
    }
    Ok(applied)
}

/// Apply the Inventory half of a sales delivery. `fulfillment_id` must name the
/// matching Commerce fulfillment and fulfillment lines already inserted in
/// this same transaction. The caller must have inserted them using the plan
/// values returned by `prepare_stock_issue`.
pub async fn apply_stock_issue(
    c: &mut PgConnection,
    plan: &StockIssuePlan,
    fulfillment_id: &str,
    command_key: &str,
    actor: &str,
    now: i64,
) -> Result<StockIssueAppliedReceipt, Error> {
    serializable(c).await?;
    validate_stock_issue_plan(plan)?;
    if fulfillment_id.is_empty()
        || fulfillment_id.len() > 200
        || command_key.is_empty()
        || command_key.len() > 200
        || actor.is_empty()
        || actor.len() > 200
        || now < 0
    {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "STOCK_ISSUE_COMMAND_INVALID",
            "Use a current delivery command.",
        )
        .into());
    }
    let order: Option<(String, i64, String, String, String, String)> = sqlx::query_as(
        "SELECT kind,version,state,warehouse_id,branch_id,order_date FROM commerce_orders \
         WHERE id=$1 AND company_id=$2",
    )
    .bind(&plan.order_id)
    .bind(&plan.company_id)
    .fetch_optional(&mut *c)
    .await?;
    let Some((kind, order_version, order_state, order_warehouse, order_branch, order_date)) = order
    else {
        return Err(missing());
    };
    if kind != "sales" {
        return Err(conflict(
            "SALES_ORDER_REQUIRED",
            "Only sales orders issue stock.",
        ));
    }
    if order_warehouse != plan.warehouse_id || order_branch != plan.branch_id {
        return Err(conflict(
            "STOCK_ISSUE_SCOPE_CHANGED",
            "The delivery warehouse or branch changed. Reload its current sources.",
        ));
    }
    if plan.date < order_date {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "INVALID_COMPLETION_DATE",
            "The delivery date cannot precede the order date.",
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
            "STOCK_ISSUE_FULFILLMENT_REQUIRED",
            "The current Commerce fulfillment must exist in this transaction.",
        ));
    }
    let mut existing_moves = 0_usize;
    for (index, line) in plan.lines.iter().enumerate() {
        let move_values =
            existing_stock_move(c, &stock_move_id(fulfillment_id, index), fulfillment_id).await?;
        if let Some((quantity_milli, value_minor)) = move_values {
            if quantity_milli != -line.quantity_milli || value_minor != -line.stock_value_minor {
                return Err(conflict(
                    "STOCK_ISSUE_REPLAY_MISMATCH",
                    "This fulfillment was already delivered with different quantities or values.",
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
                "STOCK_ISSUE_FULFILLMENT_REQUIRED",
                "The matching Commerce fulfillment line must exist in this transaction.",
            ));
        };
        if fulfillment_quantity != line.quantity_milli {
            return Err(conflict(
                "STOCK_ISSUE_FULFILLMENT_REQUIRED",
                "The Commerce fulfillment line quantity does not match the delivery plan.",
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
                "STOCK_ISSUE_REPLAY_INCOMPLETE",
                "A partial earlier delivery cannot be replayed.",
            ));
        }
        if move_values.is_some() && existing_allocations as usize != line.allocations.len() {
            return Err(conflict(
                "STOCK_ISSUE_REPLAY_MISMATCH",
                "This fulfillment was already delivered with different lot allocations.",
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
                "STOCK_ISSUE_SOURCE_CHANGED",
                "The selected sales line is no longer a current stock line.",
            ));
        }
    }
    if existing_moves == plan.lines.len() {
        return replay(c, plan, fulfillment_id).await;
    }
    if existing_moves != 0 {
        return Err(conflict(
            "STOCK_ISSUE_REPLAY_INCOMPLETE",
            "A partial earlier delivery cannot be replayed.",
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
            || ordered_milli != line.ordered_milli
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
                "Quantity exceeds the remaining sales quantity.",
            )
            .into());
        }
        let stock_before = current_stock(c, &plan.warehouse_id, &line.product_id)
            .await?
            .ok_or_else(|| {
                conflict(
                    "INSUFFICIENT_STOCK",
                    "There is no stock position for this warehouse and product.",
                )
            })?;
        if stock_before.2 != line.expected_stock_version {
            return Err(changed());
        }
        if line.tracking_mode == "none" {
            let expected_value_minor = if line.quantity_milli == stock_before.0 {
                stock_before.1
            } else {
                scaled_inventory_value(stock_before.1, line.quantity_milli, stock_before.0)
                    .map_err(|_| {
                        conflict(
                            "STOCK_ISSUE_VALUE_INVALID",
                            "The delivery value is outside the supported range.",
                        )
                    })?
            };
            if expected_value_minor != line.stock_value_minor {
                return Err(conflict(
                    "STOCK_ISSUE_VALUE_INVALID",
                    "The delivery value does not match the current stock valuation.",
                ));
            }
        }
        let reservations = current_reservations(
            c,
            &plan.company_id,
            &plan.warehouse_id,
            &line.product_id,
            &line.line_id,
        )
        .await?;
        if !same_reservations(&line.reservations, &reservations) {
            return Err(changed());
        }
        let consumed = consume_reservations(c, line).await?;
        let after_reservations = current_stock(c, &plan.warehouse_id, &line.product_id)
            .await?
            .ok_or_else(changed)?;
        if after_reservations.0 != stock_before.0 || after_reservations.1 != stock_before.1 {
            return Err(changed());
        }
        let next_quantity = after_reservations
            .0
            .checked_sub(line.quantity_milli)
            .filter(|value| *value >= 0)
            .ok_or_else(|| {
                conflict(
                    "INSUFFICIENT_STOCK",
                    "There is not enough stock in this warehouse for the delivery.",
                )
            })?;
        let next_value = after_reservations
            .1
            .checked_sub(line.stock_value_minor)
            .filter(|value| *value >= 0)
            .ok_or_else(|| {
                conflict(
                    "STOCK_ISSUE_VALUE_INVALID",
                    "The delivery value exceeds the recorded stock value.",
                )
            })?;
        let result = sqlx::query(
            "UPDATE commerce_stock SET quantity_milli=$1,value_minor=$2,version=version+1 \
             WHERE warehouse_id=$3 AND product_id=$4 AND version=$5 \
               AND quantity_milli=$6 AND value_minor=$7",
        )
        .bind(next_quantity)
        .bind(next_value)
        .bind(&plan.warehouse_id)
        .bind(&line.product_id)
        .bind(after_reservations.2)
        .bind(after_reservations.0)
        .bind(after_reservations.1)
        .execute(&mut *c)
        .await?;
        if result.rows_affected() != 1 {
            return Err(changed());
        }
        let allocations =
            apply_lot_allocations(c, plan, line, index, fulfillment_id, actor, now).await?;
        sqlx::query(
            "INSERT INTO commerce_stock_moves(id,warehouse_id,product_id,fulfillment_id,\
             quantity_milli,value_minor,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(stock_move_id(fulfillment_id, index))
        .bind(&plan.warehouse_id)
        .bind(&line.product_id)
        .bind(fulfillment_id)
        .bind(-line.quantity_milli)
        .bind(-line.stock_value_minor)
        .bind(now)
        .execute(&mut *c)
        .await?;
        let stock_version: i64 = sqlx::query_scalar(
            "SELECT version FROM commerce_stock WHERE warehouse_id=$1 AND product_id=$2",
        )
        .bind(&plan.warehouse_id)
        .bind(&line.product_id)
        .fetch_one(&mut *c)
        .await?;
        applied.push(StockIssueAppliedLine {
            line_id: line.line_id.clone(),
            fulfillment_line_id: fulfillment_line_id(fulfillment_id, index),
            stock_move_id: stock_move_id(fulfillment_id, index),
            stock_version,
            quantity_milli: line.quantity_milli,
            value_minor: line.stock_value_minor,
            reservations_consumed_milli: consumed,
            allocations,
        });
    }
    Ok(StockIssueAppliedReceipt {
        fulfillment_id: fulfillment_id.to_owned(),
        order_id: plan.order_id.clone(),
        replayed: false,
        lines: applied,
    })
}

async fn replay(
    c: &mut PgConnection,
    plan: &StockIssuePlan,
    fulfillment_id: &str,
) -> Result<StockIssueAppliedReceipt, Error> {
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
                "STOCK_ISSUE_REPLAY_INCOMPLETE",
                "A partial earlier delivery cannot be replayed.",
            ));
        };
        if move_quantity != -line.quantity_milli || move_value != -line.stock_value_minor {
            return Err(conflict(
                "STOCK_ISSUE_REPLAY_MISMATCH",
                "This fulfillment was already delivered with different quantities or values.",
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
            let id = allocation_id(fulfillment_id, index, allocation.allocation_index);
            let row: Option<(String, i64, i64, String, Option<String>)> = sqlx::query_as(
                "SELECT lot_id,quantity_milli,value_minor,disposition,quality_hold_id \
                 FROM inventory_fulfillment_lot_allocations WHERE id=$1",
            )
            .bind(&id)
            .fetch_optional(&mut *c)
            .await?;
            let Some((persisted_lot_id, quantity_milli, value_minor, disposition, quality_hold_id)) =
                row
            else {
                return Err(conflict(
                    "STOCK_ISSUE_REPLAY_INCOMPLETE",
                    "A partial earlier delivery cannot be replayed.",
                ));
            };
            let balance_version: Option<i64> = sqlx::query_scalar(
                "SELECT version FROM inventory_lot_balances WHERE lot_id=$1 AND warehouse_id=$2",
            )
            .bind(&persisted_lot_id)
            .bind(&plan.warehouse_id)
            .fetch_optional(&mut *c)
            .await?;
            let lot: Option<(Option<String>, Option<String>)> = sqlx::query_as(
                "SELECT lot_code,serial_number FROM inventory_lots WHERE id=$1 AND company_id=$2",
            )
            .bind(&persisted_lot_id)
            .bind(&plan.company_id)
            .fetch_optional(&mut *c)
            .await?;
            let Some((lot_code, serial_number)) = lot else {
                return Err(changed());
            };
            if persisted_lot_id != allocation.lot_id
                || quantity_milli != allocation.quantity_milli
                || value_minor != allocation.value_minor
                || disposition != "available"
                || quality_hold_id.is_some()
            {
                return Err(conflict(
                    "STOCK_ISSUE_REPLAY_MISMATCH",
                    "This fulfillment was already delivered with different lot allocations.",
                ));
            }
            allocations.push(StockIssueAppliedAllocation {
                id,
                lot_id: persisted_lot_id,
                lot_code,
                serial_number,
                quantity_milli,
                value_minor,
                balance_version: balance_version.ok_or_else(changed)?,
            });
        }
        let reservations_consumed_milli =
            line.reservations.iter().fold(0_i64, |total, reservation| {
                total.saturating_add(reservation.remaining_milli)
            });
        applied.push(StockIssueAppliedLine {
            line_id: line.line_id.clone(),
            fulfillment_line_id: fulfillment_line_id(fulfillment_id, index),
            stock_move_id: stock_move_id(fulfillment_id, index),
            stock_version,
            quantity_milli: line.quantity_milli,
            value_minor: line.stock_value_minor,
            reservations_consumed_milli: reservations_consumed_milli.min(line.quantity_milli),
            allocations,
        });
    }
    Ok(StockIssueAppliedReceipt {
        fulfillment_id: fulfillment_id.to_owned(),
        order_id: plan.order_id.clone(),
        replayed: true,
        lines: applied,
    })
}
