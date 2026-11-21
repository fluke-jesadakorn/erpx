//! PostgreSQL landed-cost source/effect seam for the native landed-cost command.
//!
//! The caller (composition) owns current IAM/Commercial authority, the Workflow
//! ledger and the Finance journal. These functions read current receipt, stock
//! and lot facts, revalidate the published allocation and apply only Inventory
//! cost/stock/lot effects inside the caller's serializable unit.
use crate::domain::landed_costs::{
    CostLineRecord, CostLotRecord, LandedCostCreatePlan, LandedCostCreateSource,
    LandedCostRecord, LandedCostTransitionPlan, LandedCostTransitionSource, ReceiptLine,
    ReceiptLotBalance, ReceiptRecord, ReceivingStock,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "LANDED_COST_STATUS_INVALID",
        "The landed cost changed. Reload it before reviewing.",
    )
    .into()
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

/// Lock landed-cost, stock and lot relations before the first owner read.
pub async fn lock_landed_cost_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE inventory_landed_costs,inventory_landed_cost_lines,\
         inventory_landed_cost_lots,inventory_landed_cost_events,inventory_landed_cost_moves,\
         commerce_stock,commerce_fulfillments,commerce_fulfillment_lines,commerce_orders,\
         commerce_products,finance_accounts,finance_periods,inventory_lot_balances,\
         inventory_lot_moves,inventory_ledger IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// Read the posted purchase receipt and its retained quantities, stock and lots.
pub async fn read_landed_cost_create_source(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    fulfillment_id: &str,
) -> Result<LandedCostCreateSource, Error> {
    serializable(c).await?;
    let row = sqlx::query(
        "SELECT f.id,f.number,f.fulfillment_date,f.created_at,o.number order_number,\
         o.branch_id,o.warehouse_id,o.kind order_kind,j.status journal_status,\
         ch.base_currency currency,s.chart_id,s.inventory_account_id,f.version \
         FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id \
         JOIN commerce_settings s ON s.company_id=o.company_id \
         JOIN finance_charts ch ON ch.id=s.chart_id AND ch.company_id=s.company_id \
         JOIN finance_journals j ON j.id=f.journal_id \
         WHERE f.id=$1 AND o.company_id=$2 AND o.kind='purchase' \
           AND j.status='posted' AND ($3::text IS NULL OR o.branch_id=$3)",
    )
    .bind(fulfillment_id)
    .bind(company)
    .bind(branch)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| {
        BusinessError::new(
            FailureKind::NotFound,
            "POSTED_PURCHASE_RECEIPT_REQUIRED",
            "Choose a posted purchase receipt in your Finance scope.",
        )
    })?;
    let receipt = ReceiptRecord {
        id: row.try_get("id")?,
        number: row.try_get("number")?,
        fulfillment_date: row.try_get("fulfillment_date")?,
        created_at: row.try_get("created_at")?,
        order_number: row.try_get("order_number")?,
        branch_id: row.try_get("branch_id")?,
        warehouse_id: row.try_get("warehouse_id")?,
        order_kind: row.try_get("order_kind")?,
        journal_status: row.try_get("journal_status")?,
        currency: row.try_get("currency")?,
        chart_id: row.try_get("chart_id")?,
        inventory_account_id: row.try_get("inventory_account_id")?,
        version: row.try_get("version")?,
    };
    let line_rows = sqlx::query(
        "SELECT fl.id,ol.product_id,fl.quantity_milli,fl.value_minor,p.tracking_mode \
         FROM commerce_fulfillment_lines fl JOIN commerce_order_lines ol ON ol.id=fl.order_line_id \
         JOIN commerce_products p ON p.id=ol.product_id \
         WHERE fl.fulfillment_id=$1 ORDER BY ol.ordinal",
    )
    .bind(fulfillment_id)
    .fetch_all(&mut *c)
    .await?;
    let lines: Vec<ReceiptLine> = line_rows
        .into_iter()
        .map(|row| {
            Ok::<_, sqlx::Error>(ReceiptLine {
                id: row.try_get("id")?,
                product_id: row.try_get("product_id")?,
                quantity_milli: row.try_get("quantity_milli")?,
                value_minor: row.try_get("value_minor")?,
                tracking_mode: row.try_get("tracking_mode")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let mut stock = Vec::with_capacity(lines.len());
    let mut lots = Vec::with_capacity(lines.len());
    for line in &lines {
        let stock_row = sqlx::query(
            "SELECT quantity_milli,value_minor,version FROM commerce_stock \
             WHERE warehouse_id=$1 AND product_id=$2",
        )
        .bind(&receipt.warehouse_id)
        .bind(&line.product_id)
        .fetch_optional(&mut *c)
        .await?;
        stock.push(match stock_row {
            Some(row) => ReceivingStock {
                quantity_milli: row.try_get("quantity_milli")?,
                value_minor: row.try_get("value_minor")?,
                version: row.try_get("version")?,
            },
            None => ReceivingStock {
                quantity_milli: 0,
                value_minor: 0,
                version: 0,
            },
        });
        let lot_rows = sqlx::query(
            "SELECT a.id,a.lot_id,a.quantity_milli,a.value_minor,b.quantity_milli balance_quantity_milli,\
             b.value_minor balance_value_minor,b.version balance_version \
             FROM inventory_fulfillment_lot_allocations a \
             JOIN inventory_lot_balances b ON b.lot_id=a.lot_id AND b.warehouse_id=$1 \
             WHERE a.fulfillment_line_id=$2 ORDER BY a.id",
        )
        .bind(&receipt.warehouse_id)
        .bind(&line.id)
        .fetch_all(&mut *c)
        .await?;
        lots.push(
            lot_rows
                .into_iter()
                .map(|row| {
                    Ok::<_, sqlx::Error>(ReceiptLotBalance {
                        id: row.try_get("id")?,
                        lot_id: row.try_get("lot_id")?,
                        quantity_milli: row.try_get("quantity_milli")?,
                        value_minor: row.try_get("value_minor")?,
                        balance_quantity_milli: row.try_get("balance_quantity_milli")?,
                        balance_value_minor: row.try_get("balance_value_minor")?,
                        balance_version: row.try_get("balance_version")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()?,
        );
    }
    Ok(LandedCostCreateSource {
        receipt,
        lines,
        stock,
        lots,
        credit_account_exists: false,
    })
}

/// Read the current landed-cost record, its published lines/lots and the open
/// period covering the cost date.
pub async fn read_landed_cost_transition_source(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    id: &str,
) -> Result<LandedCostTransitionSource, Error> {
    serializable(c).await?;
    let row = sqlx::query(
        "SELECT c.id,c.company_id,c.branch_id,c.fulfillment_id,c.number,c.cost_date,\
         c.description,c.allocation_method,c.amount_minor,c.currency,c.credit_account_id,\
         s.inventory_account_id,o.warehouse_id,c.status,c.version,c.created_by,c.journal_id,\
         s.chart_id \
         FROM inventory_landed_costs c JOIN commerce_fulfillments f ON f.id=c.fulfillment_id \
         JOIN commerce_orders o ON o.id=f.order_id \
         JOIN commerce_settings s ON s.company_id=c.company_id \
         WHERE c.id=$1 AND c.company_id=$2 AND ($3::text IS NULL OR c.branch_id=$3)",
    )
    .bind(id)
    .bind(company)
    .bind(branch)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| {
        BusinessError::new(
            FailureKind::NotFound,
            "LANDED_COST_NOT_FOUND",
            "Landed cost record not found in your scope.",
        )
    })?;
    let cost = LandedCostRecord {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        fulfillment_id: row.try_get("fulfillment_id")?,
        number: row.try_get("number")?,
        cost_date: row.try_get("cost_date")?,
        description: row.try_get("description")?,
        allocation_method: row.try_get("allocation_method")?,
        amount_minor: row.try_get("amount_minor")?,
        currency: row.try_get("currency")?,
        chart_id: row.try_get("chart_id")?,
        credit_account_id: row.try_get("credit_account_id")?,
        inventory_account_id: row.try_get("inventory_account_id")?,
        warehouse_id: row.try_get("warehouse_id")?,
        status: row.try_get("status")?,
        version: row.try_get("version")?,
        created_by: row.try_get("created_by")?,
        journal_id: row.try_get("journal_id")?,
    };
    let line_rows = sqlx::query(
        "SELECT id,fulfillment_line_id,product_id,quantity_milli,receipt_value_minor,\
         allocated_minor,stock_quantity_milli,stock_value_minor,stock_version \
         FROM inventory_landed_cost_lines WHERE landed_cost_id=$1 ORDER BY ordinal",
    )
    .bind(id)
    .fetch_all(&mut *c)
    .await?;
    let lines: Vec<CostLineRecord> = line_rows
        .into_iter()
        .map(|row| {
            Ok::<_, sqlx::Error>(CostLineRecord {
                id: row.try_get("id")?,
                fulfillment_line_id: row.try_get("fulfillment_line_id")?,
                product_id: row.try_get("product_id")?,
                quantity_milli: row.try_get("quantity_milli")?,
                receipt_value_minor: row.try_get("receipt_value_minor")?,
                allocated_minor: row.try_get("allocated_minor")?,
                stock_quantity_milli: row.try_get("stock_quantity_milli")?,
                stock_value_minor: row.try_get("stock_value_minor")?,
                stock_version: row.try_get("stock_version")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let lot_rows = sqlx::query(
        "SELECT x.id,x.lot_id,x.landed_cost_line_id,x.quantity_milli,x.receipt_value_minor,\
         x.allocated_minor,x.balance_quantity_milli,x.balance_value_minor,x.balance_version,\
         l.product_id line_product_id \
         FROM inventory_landed_cost_lots x \
         JOIN inventory_landed_cost_lines l ON l.id=x.landed_cost_line_id \
         WHERE l.landed_cost_id=$1 ORDER BY l.ordinal,x.id",
    )
    .bind(id)
    .fetch_all(&mut *c)
    .await?;
    let lots: Vec<CostLotRecord> = lot_rows
        .into_iter()
        .map(|row| {
            Ok::<_, sqlx::Error>(CostLotRecord {
                id: row.try_get("id")?,
                lot_id: row.try_get("lot_id")?,
                cost_line_id: row.try_get("landed_cost_line_id")?,
                quantity_milli: row.try_get("quantity_milli")?,
                receipt_value_minor: row.try_get("receipt_value_minor")?,
                allocated_minor: row.try_get("allocated_minor")?,
                balance_quantity_milli: row.try_get("balance_quantity_milli")?,
                balance_value_minor: row.try_get("balance_value_minor")?,
                balance_version: row.try_get("balance_version")?,
                line_product_id: row.try_get("line_product_id")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let periods: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM finance_periods WHERE company_id=$1 AND state='open' \
         AND start_date<=$2 AND end_date>=$2",
    )
    .bind(company)
    .bind(&cost.cost_date)
    .fetch_all(&mut *c)
    .await?;
    Ok(LandedCostTransitionSource {
        cost,
        lines,
        lots,
        periods,
    })
}

/// Verify the selected credit account is active on the receipt chart.
pub async fn landed_cost_credit_account(
    c: &mut PgConnection,
    company: &str,
    chart_id: &str,
    account_id: &str,
) -> Result<bool, Error> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_accounts WHERE id=$1 AND company_id=$2 \
         AND chart_id=$3 AND status='active' AND category IN ('expense','liability'))",
    )
    .bind(account_id)
    .bind(company)
    .bind(chart_id)
    .fetch_one(&mut *c)
    .await?)
}

/// Apply a landed-cost create: header, published allocation, retained lots and
/// the immutable submission event.
pub async fn apply_landed_cost_create(
    c: &mut PgConnection,
    plan: &LandedCostCreatePlan,
    key: &str,
    actor: &str,
    now: i64,
) -> Result<(), Error> {
    let posted: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM commerce_fulfillments f \
         JOIN commerce_orders o ON o.id=f.order_id JOIN finance_journals j ON j.id=f.journal_id \
         WHERE f.id=$1 AND o.company_id=$2 AND o.kind='purchase' AND j.status='posted')",
    )
    .bind(&plan.fulfillment_id)
    .bind(&plan.company_id)
    .fetch_one(&mut *c)
    .await?;
    if !posted {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "POSTED_PURCHASE_RECEIPT_REQUIRED",
            "Allocate landed cost only to a posted purchase receipt.",
        )
        .into());
    }
    sqlx::query(
        "INSERT INTO inventory_landed_costs(id,company_id,branch_id,fulfillment_id,number,\
         cost_date,description,allocation_method,amount_minor,currency,credit_account_id,\
         created_by,created_at,updated_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$13)",
    )
    .bind(&plan.id)
    .bind(&plan.company_id)
    .bind(&plan.branch_id)
    .bind(&plan.fulfillment_id)
    .bind(&plan.number)
    .bind(&plan.cost_date)
    .bind(&plan.description)
    .bind(&plan.allocation_method)
    .bind(plan.amount_minor)
    .bind(&plan.currency)
    .bind(&plan.credit_account_id)
    .bind(actor)
    .bind(now)
    .execute(&mut *c)
    .await?;
    for line in &plan.lines {
        sqlx::query(
            "INSERT INTO inventory_landed_cost_lines(id,landed_cost_id,fulfillment_line_id,\
             product_id,quantity_milli,receipt_value_minor,allocated_minor,stock_quantity_milli,\
             stock_value_minor,stock_version,ordinal) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        )
        .bind(&line.id)
        .bind(&plan.id)
        .bind(&line.fulfillment_line_id)
        .bind(&line.product_id)
        .bind(line.quantity_milli)
        .bind(line.receipt_value_minor)
        .bind(line.allocated_minor)
        .bind(line.stock_quantity_milli)
        .bind(line.stock_value_minor)
        .bind(line.stock_version)
        .bind(line.ordinal)
        .execute(&mut *c)
        .await?;
        for lot in &line.lots {
            sqlx::query(
                "INSERT INTO inventory_landed_cost_lots(id,landed_cost_line_id,lot_id,\
                 quantity_milli,receipt_value_minor,allocated_minor,balance_quantity_milli,\
                 balance_value_minor,balance_version) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
            )
            .bind(&lot.id)
            .bind(&line.id)
            .bind(&lot.lot_id)
            .bind(lot.quantity_milli)
            .bind(lot.receipt_value_minor)
            .bind(lot.allocated_minor)
            .bind(lot.balance_quantity_milli)
            .bind(lot.balance_value_minor)
            .bind(lot.balance_version)
            .execute(&mut *c)
            .await?;
        }
    }
    sqlx::query(
        "INSERT INTO inventory_landed_cost_events(id,landed_cost_id,command_id,action,actor_id,\
         reviewed_version,reason,created_at) VALUES($1,$2,$3,'submitted',$4,0,$5,$6)",
    )
    .bind(format!("{}-created", plan.id))
    .bind(&plan.id)
    .bind(key)
    .bind(actor)
    .bind(&plan.description)
    .bind(now)
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// Apply an approve/reject/cancel/post transition with the original snapshot
/// guards and stock/lot revaluation effects.
pub async fn apply_landed_cost_transition(
    c: &mut PgConnection,
    plan: &LandedCostTransitionPlan,
    key: &str,
    actor: &str,
    now: i64,
    journal_id: Option<&str>,
) -> Result<(), Error> {
    if plan.action.revalue() {
        for line in &plan.lines {
            let snapshot: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM commerce_stock WHERE warehouse_id=$1 \
                 AND product_id=$2 AND version=$3 AND quantity_milli=$4 AND value_minor=$5)",
            )
            .bind(&plan.warehouse_id)
            .bind(&line.product_id)
            .bind(line.stock_version)
            .bind(line.stock_quantity_milli)
            .bind(line.stock_value_minor)
            .fetch_one(&mut *c)
            .await?;
            if !snapshot {
                return Err(changed());
            }
            let moved: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM inventory_ledger m \
                 JOIN commerce_fulfillment_lines source_line ON source_line.id=$1 \
                 JOIN commerce_fulfillments source ON source.id=source_line.fulfillment_id \
                 WHERE m.warehouse_id=$2 AND m.product_id=$3 AND m.quantity_milli<0 \
                   AND m.created_at>source.created_at)",
            )
            .bind(&line.fulfillment_line_id)
            .bind(&plan.warehouse_id)
            .bind(&line.product_id)
            .fetch_one(&mut *c)
            .await?;
            if moved {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "LANDED_COST_STOCK_MOVED",
                    "Landed cost cannot be added after stock from this receipt may have left the warehouse.",
                )
                .into());
            }
        }
        for lot in &plan.lots {
            let snapshot: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM inventory_lot_balances WHERE lot_id=$1 \
                 AND warehouse_id=$2 AND version=$3 AND quantity_milli=$4 AND value_minor=$5)",
            )
            .bind(&lot.lot_id)
            .bind(&plan.warehouse_id)
            .bind(lot.balance_version)
            .bind(lot.balance_quantity_milli)
            .bind(lot.balance_value_minor)
            .fetch_one(&mut *c)
            .await?;
            if !snapshot {
                return Err(changed());
            }
        }
    }
    let expected = if plan.action == crate::domain::landed_costs::LandedCostAction::Post {
        "approved"
    } else {
        "submitted"
    };
    let updated = sqlx::query(
        "UPDATE inventory_landed_costs SET status=$1,\
         approved_by=CASE WHEN $2='approve' THEN $3 ELSE approved_by END,\
         journal_id=COALESCE($4,journal_id),review_reason=$5,version=version+1,updated_at=$6 \
         WHERE id=$7 AND company_id=$8 AND status=$9 AND version=$10",
    )
    .bind(plan.status)
    .bind(plan.action.as_str())
    .bind(actor)
    .bind(journal_id)
    .bind(&plan.reason)
    .bind(now)
    .bind(&plan.id)
    .bind(&plan.company_id)
    .bind(expected)
    .bind(plan.expected_version)
    .execute(&mut *c)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(changed());
    }
    if plan.action == crate::domain::landed_costs::LandedCostAction::Post {
        for line in &plan.lines {
            sqlx::query(
                "UPDATE commerce_stock SET value_minor=value_minor+$1,version=version+1 \
                 WHERE warehouse_id=$2 AND product_id=$3",
            )
            .bind(line.allocated_minor)
            .bind(&plan.warehouse_id)
            .bind(&line.product_id)
            .execute(&mut *c)
            .await?;
            sqlx::query(
                "INSERT INTO inventory_landed_cost_moves(id,landed_cost_id,warehouse_id,\
                 product_id,value_minor,movement_date,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(format!("{}-{}", plan.id, line.id))
            .bind(&plan.id)
            .bind(&plan.warehouse_id)
            .bind(&line.product_id)
            .bind(line.allocated_minor)
            .bind(&plan.cost_date)
            .bind(now)
            .execute(&mut *c)
            .await?;
        }
        for lot in &plan.lots {
            sqlx::query(
                "UPDATE inventory_lot_balances SET value_minor=value_minor+$1,version=version+1 \
                 WHERE lot_id=$2 AND warehouse_id=$3",
            )
            .bind(lot.allocated_minor)
            .bind(&lot.lot_id)
            .bind(&plan.warehouse_id)
            .execute(&mut *c)
            .await?;
            sqlx::query(
                "INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,\
                 quantity_milli,value_minor,movement_date,created_by,created_at) \
                 VALUES($1,$2,$3,'landed_cost',$4,0,$5,$6,$7,$8)",
            )
            .bind(format!("landed-lot-{}-{}", plan.id, lot.id))
            .bind(&lot.lot_id)
            .bind(&plan.warehouse_id)
            .bind(&lot.id)
            .bind(lot.allocated_minor)
            .bind(&plan.cost_date)
            .bind(actor)
            .bind(now)
            .execute(&mut *c)
            .await?;
        }
    }
    sqlx::query(
        "INSERT INTO inventory_landed_cost_events(id,landed_cost_id,command_id,action,actor_id,\
         reviewed_version,reason,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(format!("{}-{}", plan.id, key))
    .bind(&plan.id)
    .bind(key)
    .bind(plan.action.as_str())
    .bind(actor)
    .bind(plan.expected_version)
    .bind(&plan.reason)
    .bind(now)
    .execute(&mut *c)
    .await?;
    Ok(())
}
