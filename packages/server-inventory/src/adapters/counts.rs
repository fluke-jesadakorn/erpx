//! PostgreSQL generic stock-count source/effect seam.
//!
//! The caller (composition) owns current IAM/Commercial authority, the Workflow
//! ledger, Finance posting journals and Files evidence. These functions read
//! the current Count/warehouse/product/stock/lot facts and apply the planned
//! draft create or revision inside the caller's serializable unit.
use crate::domain::counts::{
    CountLot, CountLotEffect, CountProduct, CountRecord, CountSavePlan, CountSource, CountStock,
    CountTransitionEffects, CountTransitionLine, CountTransitionLot, CountWarehouse,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};
use std::collections::BTreeMap;

type Error = PgTransactionError<BusinessError>;

fn invalid(code: &'static str, message: &'static str) -> Error {
    BusinessError::invalid(code, message).into()
}

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "COUNT_VERSION_CHANGED",
        "The physical count changed.",
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

/// Lock the Count rows before the first read so a concurrent save or posting
/// waits. Lot and stock relations follow the existing Count lock order.
pub async fn lock_count_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE inventory_counts,inventory_count_lines,inventory_count_lot_lines,\
         commerce_warehouses,commerce_products,commerce_stock,inventory_lots,\
         inventory_lot_balances IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// Read the current draft/revision source for the selected Company scope.
/// `product_ids` are the products named by the command; they are never trusted
/// as labels, only used to bound the owner reads.
pub async fn read_count_save_source(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    current_id: Option<&str>,
    warehouse_id: &str,
    product_ids: &[String],
) -> Result<CountSource, Error> {
    serializable(c).await?;
    if company.is_empty() || warehouse_id.is_empty() {
        return Err(invalid(
            "COUNT_INVALID",
            "Use a current Count command in the selected Company.",
        ));
    }
    let current = match current_id {
        None => None,
        Some(id) => sqlx::query(
            "SELECT id,company_id,branch_id,warehouse_id,number,count_date,reason,status,version,\
             net_value_minor,created_by,prepared_by,approved_by,quality_hold_id \
             FROM inventory_counts WHERE id=$1 AND company_id=$2 \
             AND ($3::text IS NULL OR branch_id=$3)",
        )
        .bind(id)
        .bind(company)
        .bind(branch)
        .fetch_optional(&mut *c)
        .await?
        .map(|row| -> Result<CountRecord, sqlx::Error> {
            Ok(CountRecord {
                id: row.try_get("id")?,
                company_id: row.try_get("company_id")?,
                branch_id: row.try_get("branch_id")?,
                warehouse_id: row.try_get("warehouse_id")?,
                number: row.try_get("number")?,
                count_date: row.try_get("count_date")?,
                reason: row.try_get("reason")?,
                status: row.try_get("status")?,
                version: row.try_get("version")?,
                net_value_minor: row.try_get("net_value_minor")?,
                created_by: row.try_get("created_by")?,
                prepared_by: row.try_get("prepared_by")?,
                approved_by: row.try_get("approved_by")?,
                quality_hold_id: row.try_get("quality_hold_id")?,
            })
        })
        .transpose()?,
    };
    let warehouse = sqlx::query(
        "SELECT id,branch_id,version,status FROM commerce_warehouses \
         WHERE id=$1 AND company_id=$2 AND ($3::text IS NULL OR branch_id=$3) LIMIT 1",
    )
    .bind(warehouse_id)
    .bind(company)
    .bind(branch)
    .fetch_optional(&mut *c)
    .await?
    .map(|row| -> Result<CountWarehouse, sqlx::Error> {
        Ok(CountWarehouse {
            id: row.try_get("id")?,
            branch_id: row.try_get("branch_id")?,
            version: row.try_get("version")?,
            active: row.try_get::<String, _>("status")? == "active",
        })
    })
    .transpose()?;
    let products = sqlx::query(
        "SELECT id,name,unit,version,tracking_mode FROM commerce_products \
         WHERE company_id=$1 AND status='active' AND id=ANY($2)",
    )
    .bind(company)
    .bind(product_ids)
    .fetch_all(&mut *c)
    .await?;
    let mut product_map = BTreeMap::new();
    for row in products {
        let product = CountProduct {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            unit: row.try_get("unit")?,
            version: row.try_get("version")?,
            tracking_mode: row.try_get("tracking_mode")?,
        };
        product_map.insert(product.id.clone(), product);
    }
    let stocks = sqlx::query(
        "SELECT product_id,quantity_milli,value_minor,version FROM commerce_stock \
         WHERE warehouse_id=$1 AND product_id=ANY($2)",
    )
    .bind(warehouse_id)
    .bind(product_ids)
    .fetch_all(&mut *c)
    .await?;
    let mut stock_map = BTreeMap::new();
    for row in stocks {
        let product_id: String = row.try_get("product_id")?;
        stock_map.insert(
            product_id,
            CountStock {
                quantity_milli: row.try_get("quantity_milli")?,
                value_minor: row.try_get("value_minor")?,
                version: row.try_get("version")?,
            },
        );
    }
    let lots = sqlx::query(
        "SELECT l.id,l.product_id,l.tracking_mode,l.lot_code,l.serial_number,\
         l.manufactured_date,l.expiry_date,b.quantity_milli,b.value_minor,b.version balance_version \
         FROM inventory_lots l JOIN inventory_lot_balances b \
           ON b.lot_id=l.id AND b.warehouse_id=$2 \
         WHERE l.company_id=$1 AND b.quantity_milli>0 AND l.product_id=ANY($3) ORDER BY l.id",
    )
    .bind(company)
    .bind(warehouse_id)
    .bind(product_ids)
    .fetch_all(&mut *c)
    .await?;
    let mut lot_map: BTreeMap<String, Vec<CountLot>> = BTreeMap::new();
    for row in lots {
        let lot = CountLot {
            id: row.try_get("id")?,
            product_id: row.try_get("product_id")?,
            tracking_mode: row.try_get("tracking_mode")?,
            lot_code: row.try_get("lot_code")?,
            serial_number: row.try_get("serial_number")?,
            manufactured_date: row.try_get("manufactured_date")?,
            expiry_date: row.try_get("expiry_date")?,
            quantity_milli: row.try_get("quantity_milli")?,
            value_minor: row.try_get("value_minor")?,
            balance_version: row.try_get("balance_version")?,
        };
        lot_map.entry(lot.product_id.clone()).or_default().push(lot);
    }
    Ok(CountSource {
        current,
        warehouse,
        products: product_map,
        stocks: stock_map,
        lots: lot_map,
    })
}

/// Apply a planned draft create or revision. The plan was produced from the
/// same source and its version/scope are rechecked before the first write.
pub async fn apply_count_save(
    c: &mut PgConnection,
    plan: &CountSavePlan,
    expected_version: Option<i64>,
    actor: &str,
    now: i64,
) -> Result<(), Error> {
    serializable(c).await?;
    if plan.record_id.is_empty() || plan.company_id.is_empty() || actor.is_empty() {
        return Err(invalid(
            "COUNT_COMMAND_INVALID",
            "Use a current stock Count command.",
        ));
    }
    if plan.revision {
        let expected = expected_version.ok_or_else(changed)?;
        let updated = sqlx::query(
            "UPDATE inventory_counts SET count_date=$1,reason=$2,net_value_minor=$3,status='draft',\
             prepared_by=$4,review_reason=NULL,version=version+1,updated_at=$5 \
             WHERE id=$6 AND company_id=$7 AND version=$8 AND status IN ('draft','rejected')",
        )
        .bind(&plan.count_date)
        .bind(&plan.reason)
        .bind(plan.net_value_minor)
        .bind(actor)
        .bind(now)
        .bind(&plan.record_id)
        .bind(&plan.company_id)
        .bind(expected)
        .execute(&mut *c)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(changed());
        }
        sqlx::query(
            "DELETE FROM inventory_count_lot_lines WHERE count_line_id IN \
             (SELECT id FROM inventory_count_lines WHERE count_id=$1)",
        )
        .bind(&plan.record_id)
        .execute(&mut *c)
        .await?;
        sqlx::query("DELETE FROM inventory_count_lines WHERE count_id=$1")
            .bind(&plan.record_id)
            .execute(&mut *c)
            .await?;
    } else {
        sqlx::query(
            "INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,\
             reason,net_value_minor,created_by,prepared_by,created_at,updated_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$9,$10,$10)",
        )
        .bind(&plan.record_id)
        .bind(&plan.company_id)
        .bind(&plan.branch_id)
        .bind(&plan.warehouse_id)
        .bind(&plan.number)
        .bind(&plan.count_date)
        .bind(&plan.reason)
        .bind(plan.net_value_minor)
        .bind(actor)
        .bind(now)
        .execute(&mut *c)
        .await?;
    }
    for line in &plan.lines {
        sqlx::query(
            "INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,\
             unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,\
             counted_quantity_milli,counted_value_minor,unit_cost_minor,ordinal) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
        )
        .bind(&line.id)
        .bind(&plan.record_id)
        .bind(&line.product_id)
        .bind(line.product_version)
        .bind(&line.description)
        .bind(&line.unit)
        .bind(line.snapshot_quantity_milli)
        .bind(line.snapshot_value_minor)
        .bind(line.snapshot_version)
        .bind(line.counted_quantity_milli)
        .bind(line.counted_value_minor)
        .bind(line.unit_cost_minor)
        .bind(line.ordinal)
        .execute(&mut *c)
        .await?;
        for lot in &line.lots {
            sqlx::query(
                "INSERT INTO inventory_count_lot_lines(id,count_line_id,source_lot_id,result_lot_id,\
                 tracking_mode,lot_code,serial_number,manufactured_date,expiry_date,\
                 snapshot_quantity_milli,snapshot_value_minor,snapshot_version,\
                 counted_quantity_milli,counted_value_minor,unit_cost_minor) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
            )
            .bind(&lot.id)
            .bind(&lot.count_line_id)
            .bind(&lot.source_lot_id)
            .bind(&lot.result_lot_id)
            .bind(&lot.tracking_mode)
            .bind(&lot.lot_code)
            .bind(&lot.serial_number)
            .bind(&lot.manufactured_date)
            .bind(&lot.expiry_date)
            .bind(lot.snapshot_quantity_milli)
            .bind(lot.snapshot_value_minor)
            .bind(lot.snapshot_version)
            .bind(lot.counted_quantity_milli)
            .bind(lot.counted_value_minor)
            .bind(lot.unit_cost_minor)
            .execute(&mut *c)
            .await?;
        }
    }
    Ok(())
}

/// Read the current Count plus its saved lines and lot evidence for a
/// submit/review/cancel/post transition. The caller owns the current authority
/// and the Flow/Files/Finance witnesses; this only reads Inventory facts.
pub async fn read_count_transition_source(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    id: &str,
) -> Result<(CountRecord, Vec<CountTransitionLine>), Error> {
    serializable(c).await?;
    let record = sqlx::query(
        "SELECT id,company_id,branch_id,warehouse_id,number,count_date,reason,status,version,\
         net_value_minor,created_by,prepared_by,approved_by,quality_hold_id \
         FROM inventory_counts WHERE id=$1 AND company_id=$2 \
         AND ($3::text IS NULL OR branch_id=$3)",
    )
    .bind(id)
    .bind(company)
    .bind(branch)
    .fetch_optional(&mut *c)
    .await?
    .map(|row| -> Result<CountRecord, sqlx::Error> {
        Ok(CountRecord {
            id: row.try_get("id")?,
            company_id: row.try_get("company_id")?,
            branch_id: row.try_get("branch_id")?,
            warehouse_id: row.try_get("warehouse_id")?,
            number: row.try_get("number")?,
            count_date: row.try_get("count_date")?,
            reason: row.try_get("reason")?,
            status: row.try_get("status")?,
            version: row.try_get("version")?,
            net_value_minor: row.try_get("net_value_minor")?,
            created_by: row.try_get("created_by")?,
            prepared_by: row.try_get("prepared_by")?,
            approved_by: row.try_get("approved_by")?,
            quality_hold_id: row.try_get("quality_hold_id")?,
        })
    })
    .transpose()?
    .ok_or_else(|| {
        PgTransactionError::Business(BusinessError::new(
            FailureKind::NotFound,
            "COUNT_NOT_FOUND",
            "Stock count not found in your workspace.",
        ))
    })?;
    let rows = sqlx::query(
        "SELECT id,product_id,unit,tracking_mode,snapshot_quantity_milli,\
         snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor \
         FROM inventory_count_lines WHERE count_id=$1 ORDER BY ordinal",
    )
    .bind(id)
    .fetch_all(&mut *c)
    .await?;
    let mut lines = Vec::with_capacity(rows.len());
    for row in rows {
        let line_id: String = row.try_get("id")?;
        let lots = sqlx::query(
            "SELECT id,count_line_id,source_lot_id,result_lot_id,tracking_mode,lot_code,\
             serial_number,manufactured_date,expiry_date,snapshot_quantity_milli,\
             snapshot_value_minor,counted_quantity_milli,counted_value_minor,unit_cost_minor \
             FROM inventory_count_lot_lines WHERE count_line_id=$1 ORDER BY id",
        )
        .bind(&line_id)
        .fetch_all(&mut *c)
        .await?
        .into_iter()
        .map(|lot| -> Result<CountTransitionLot, sqlx::Error> {
            Ok(CountTransitionLot {
                id: lot.try_get("id")?,
                count_line_id: lot.try_get("count_line_id")?,
                source_lot_id: lot.try_get("source_lot_id")?,
                result_lot_id: lot.try_get("result_lot_id")?,
                tracking_mode: lot.try_get("tracking_mode")?,
                lot_code: lot.try_get("lot_code")?,
                serial_number: lot.try_get("serial_number")?,
                manufactured_date: lot.try_get("manufactured_date")?,
                expiry_date: lot.try_get("expiry_date")?,
                snapshot_quantity_milli: lot.try_get("snapshot_quantity_milli")?,
                snapshot_value_minor: lot.try_get("snapshot_value_minor")?,
                counted_quantity_milli: lot.try_get("counted_quantity_milli")?,
                counted_value_minor: lot.try_get("counted_value_minor")?,
                unit_cost_minor: lot.try_get("unit_cost_minor")?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
        lines.push(CountTransitionLine {
            id: line_id,
            product_id: row.try_get("product_id")?,
            unit: row.try_get("unit")?,
            tracking_mode: row.try_get("tracking_mode")?,
            snapshot_quantity_milli: row.try_get("snapshot_quantity_milli")?,
            snapshot_value_minor: row.try_get("snapshot_value_minor")?,
            snapshot_version: row.try_get("snapshot_version")?,
            counted_quantity_milli: row.try_get("counted_quantity_milli")?,
            counted_value_minor: row.try_get("counted_value_minor")?,
            lots,
        });
    }
    Ok((record, lines))
}

/// Apply a planned transition inside the caller's serializable unit. Stock and
/// lot effects are written only for posting; the decision row is always
/// written. `journal_id` is supplied by the composition boundary after it has
/// prepared the balanced Finance journal.
pub async fn apply_count_transition(
    c: &mut PgConnection,
    record: &CountRecord,
    effects: &CountTransitionEffects,
    expected_version: i64,
    actor: &str,
    journal_id: Option<&str>,
    now: i64,
) -> Result<(), Error> {
    serializable(c).await?;
    if record.id.is_empty() || record.company_id.is_empty() || actor.is_empty() {
        return Err(invalid(
            "COUNT_COMMAND_INVALID",
            "Use a current stock Count command.",
        ));
    }
    if record.version != expected_version {
        return Err(changed());
    }
    const DECISION: &str = "UPDATE inventory_counts SET status=$1,review_reason=$2,\
         approved_by=CASE WHEN $3='approve' THEN $4 ELSE approved_by END,\
         submitted_at=CASE WHEN $3='submit' THEN $5 ELSE submitted_at END,\
         posted_at=CASE WHEN $3='post' THEN $6 ELSE posted_at END,\
         journal_id=COALESCE($7,journal_id),version=version+1,updated_at=$8 \
         WHERE id=$9 AND company_id=$10 AND version=$11";
    const POST_DECISION: &str = "UPDATE inventory_counts SET status=$1,review_reason=$2,\
         approved_by=CASE WHEN $3='approve' THEN $4 ELSE approved_by END,\
         submitted_at=CASE WHEN $3='submit' THEN $5 ELSE submitted_at END,\
         posted_at=CASE WHEN $3='post' THEN $6 ELSE posted_at END,\
         journal_id=COALESCE($7,journal_id),version=version+1,updated_at=$8 \
         WHERE id=$9 AND company_id=$10 AND version=$11 AND status='approved'";
    let action = action_for_status(&effects.next);
    let updated = sqlx::query(if effects.posting { POST_DECISION } else { DECISION })
        .bind(&effects.next)
        .bind(&effects.reason)
        .bind(action)
        .bind(actor)
        .bind(now)
        .bind(now)
        .bind(journal_id)
        .bind(now)
        .bind(&record.id)
        .bind(&record.company_id)
        .bind(expected_version)
        .execute(&mut *c)
        .await?;
    if updated.rows_affected() != 1 {
        return Err(changed());
    }
    if !effects.posting {
        return Ok(());
    }
    for stock in &effects.stocks {
        if stock.stock_insert {
            sqlx::query(
                "INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) \
                 VALUES($1,$2,$3,$4) ON CONFLICT(warehouse_id,product_id) DO UPDATE SET \
                 quantity_milli=EXCLUDED.quantity_milli,value_minor=EXCLUDED.value_minor,\
                 version=commerce_stock.version+1",
            )
            .bind(&record.warehouse_id)
            .bind(&stock.product_id)
            .bind(stock.counted_quantity_milli)
            .bind(stock.counted_value_minor)
            .execute(&mut *c)
            .await?;
        } else {
            sqlx::query(
                "UPDATE commerce_stock SET quantity_milli=$1,value_minor=$2,version=version+1 \
                 WHERE warehouse_id=$3 AND product_id=$4",
            )
            .bind(stock.counted_quantity_milli)
            .bind(stock.counted_value_minor)
            .bind(&record.warehouse_id)
            .bind(&stock.product_id)
            .execute(&mut *c)
            .await?;
        }
        sqlx::query(
            "INSERT INTO inventory_adjustment_moves(id,count_id,warehouse_id,product_id,\
             quantity_milli,value_minor,movement_date,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(format!("{}-{}", record.id, stock.line_id))
        .bind(&record.id)
        .bind(&record.warehouse_id)
        .bind(&stock.product_id)
        .bind(stock.counted_quantity_milli - stock.current_quantity_milli)
        .bind(stock.counted_value_minor - stock.current_value_minor)
        .bind(&record.count_date)
        .bind(now)
        .execute(&mut *c)
        .await?;
    }
    for lot in &effects.lots {
        apply_lot(c, record, lot, actor, now).await?;
    }
    if let Some(hold) = &record.quality_hold_id {
        sqlx::query(
            "UPDATE inventory_lots SET status='scrapped' WHERE id=$1 \
             AND NOT EXISTS(SELECT 1 FROM inventory_lot_balances WHERE lot_id=$1 AND quantity_milli>0)",
        )
        .bind(hold)
        .execute(&mut *c)
        .await?;
    }
    Ok(())
}

fn action_for_status(status: &str) -> &'static str {
    match status {
        "submitted" => "submit",
        "approved" => "approve",
        "posted" => "post",
        "rejected" => "reject",
        "cancelled" => "cancel",
        _ => "submit",
    }
}

async fn apply_lot(
    c: &mut PgConnection,
    record: &CountRecord,
    lot: &CountLotEffect,
    actor: &str,
    now: i64,
) -> Result<(), Error> {
    if lot.source_lot_id.is_some() {
        sqlx::query(
            "UPDATE inventory_lot_balances SET quantity_milli=$1,value_minor=$2,\
             version=version+1 WHERE lot_id=$3 AND warehouse_id=$4",
        )
        .bind(lot.counted_quantity_milli)
        .bind(lot.counted_value_minor)
        .bind(&lot.result_lot_id)
        .bind(&record.warehouse_id)
        .execute(&mut *c)
        .await?;
        sqlx::query(
            "UPDATE inventory_lots SET status=CASE WHEN $1=0 THEN 'depleted' ELSE 'active' END \
             WHERE id=$2",
        )
        .bind(lot.counted_quantity_milli)
        .bind(&lot.result_lot_id)
        .execute(&mut *c)
        .await?;
    } else {
        sqlx::query(
            "INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,\
             serial_number,manufactured_date,expiry_date,status,created_by,created_at) \
             VALUES($1,$2,(SELECT product_id FROM inventory_count_lines WHERE id=$3),$4,$5,$6,$7,$8,\
             'active',$9,$10)",
        )
        .bind(&lot.result_lot_id)
        .bind(&record.company_id)
        .bind(&lot.count_line_id)
        .bind(&lot.tracking_mode)
        .bind(&lot.lot_code)
        .bind(&lot.serial_number)
        .bind(&lot.manufactured_date)
        .bind(&lot.expiry_date)
        .bind(actor)
        .bind(now)
        .execute(&mut *c)
        .await?;
        sqlx::query(
            "INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor) \
             VALUES($1,$2,$3,$4)",
        )
        .bind(&lot.result_lot_id)
        .bind(&record.warehouse_id)
        .bind(lot.counted_quantity_milli)
        .bind(lot.counted_value_minor)
        .execute(&mut *c)
        .await?;
    }
    sqlx::query(
        "INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,\
         quantity_milli,value_minor,movement_date,created_by,created_at) \
         VALUES($1,$2,$3,'stock_count',$4,$5,$6,$7,$8,$9)",
    )
    .bind(format!("{}-lot-{}", record.id, lot.lot_id))
    .bind(&lot.result_lot_id)
    .bind(&record.warehouse_id)
    .bind(&lot.lot_id)
    .bind(lot.counted_quantity_milli - lot.snapshot_quantity_milli)
    .bind(lot.counted_value_minor - lot.snapshot_value_minor)
    .bind(&record.count_date)
    .bind(actor)
    .bind(now)
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// Initial/manufacturing/POS/Service seeding kind. The four immutable source
/// tables share the count/company/warehouse/product/chart shape but name their
/// setup receipt column differently, so the table is selected by a closed enum
/// instead of caller text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CountSeedKind {
    Initial,
    Manufacturing,
    Pos,
    Service,
}

impl CountSeedKind {
    /// Closed wire name; never inferred from caller text beyond this table.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "initial" => Some(Self::Initial),
            "manufacturing" => Some(Self::Manufacturing),
            "pos" => Some(Self::Pos),
            "service" => Some(Self::Service),
            _ => None,
        }
    }

    /// Private receipt marker retained by the immutable source triggers.
    pub fn marker(self) -> &'static str {
        match self {
            Self::Initial => "_initialPhysicalStockCount",
            Self::Manufacturing => "_manufacturingInitialPhysicalStockCount",
            Self::Pos => "_posInitialPhysicalStockCount",
            Self::Service => "_serviceInitialPhysicalStockCount",
        }
    }

    pub fn proof_kind(self) -> &'static str {
        match self {
            Self::Initial => "initial-physical-stock-count",
            Self::Manufacturing => "manufacturing-initial-physical-stock-count",
            Self::Pos => "pos-initial-physical-stock-count",
            Self::Service => "service-initial-physical-stock-count",
        }
    }

    fn setup_keys(self) -> (&'static str, Option<&'static str>) {
        match self {
            Self::Initial => ("goodsSetupCommandId", Some("goodsSetupRequestHash")),
            Self::Manufacturing => (
                "manufacturingSetupCommandId",
                Some("manufacturingSetupRequestHash"),
            ),
            Self::Pos => ("posSetupCommandId", Some("posSetupRequestHash")),
            Self::Service => ("serviceSetupCommandId", Some("serviceSetupRequestHash")),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CountSeedSource<'a> {
    pub kind: CountSeedKind,
    pub count_id: &'a str,
    pub company_id: &'a str,
    pub branch_id: &'a str,
    pub warehouse_id: &'a str,
    pub product_id: &'a str,
    pub count_command_id: &'a str,
    pub setup_command_id: &'a str,
    pub chart_id: &'a str,
    pub inventory_account_id: &'a str,
    pub variance_account_id: &'a str,
    pub currency: &'a str,
    pub proof_json: &'a str,
    pub created_by: &'a str,
    pub created_at: i64,
    pub pos_bootstrap_receipt_id: Option<&'a str>,
    pub pos_approval_receipt_id: Option<&'a str>,
}

/// Validated immutable first-Count provenance. The proof keeps the retained
/// camelCase wire shape so the deferred source triggers can compare it with the
/// command receipt marker; identity fields are rechecked against the saved plan.
#[derive(Clone, Debug)]
pub struct CountSeedPlan {
    pub kind: CountSeedKind,
    pub proof: serde_json::Value,
    pub branch_id: String,
    pub product_id: String,
    pub setup_command_id: String,
    pub chart_id: String,
    pub inventory_account_id: String,
    pub variance_account_id: String,
    pub currency: String,
    pub pos_bootstrap_receipt_id: Option<String>,
    pub pos_approval_receipt_id: Option<String>,
}

fn seed_text<'a>(proof: &'a serde_json::Value, key: &'static str) -> Result<&'a str, Error> {
    proof
        .get(key)
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            invalid(
                "COUNT_SEED_PROOF_INVALID",
                "Use the captured first-Count source proof.",
            )
        })
}

/// Admit a first-Count seed envelope against the draft that will be written.
/// The caller passes the command-derived identity; the proof is never trusted as
/// a label. Source reciprocity stays with the immutable PostgreSQL triggers.
pub fn plan_count_seed(
    kind: CountSeedKind,
    proof: &serde_json::Value,
    count_id: &str,
    company_id: &str,
    warehouse_id: &str,
    branch_id: &str,
    product_id: &str,
    count_command_id: &str,
) -> Result<CountSeedPlan, Error> {
    if proof.as_object().is_none() {
        return Err(invalid(
            "COUNT_SEED_PROOF_INVALID",
            "Use the captured first-Count source proof.",
        ));
    }
    if seed_text(proof, "kind")? != kind.proof_kind()
        || seed_text(proof, "countId")? != count_id
        || seed_text(proof, "companyId")? != company_id
        || seed_text(proof, "warehouseId")? != warehouse_id
        || seed_text(proof, "branchId")? != branch_id
        || seed_text(proof, "productId")? != product_id
        || seed_text(proof, "countCommandId")? != count_command_id
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "COUNT_SEED_SOURCE_CHANGED",
            "The original first-Count source changed.",
        )
        .into());
    }
    let (setup_key, request_key) = kind.setup_keys();
    let setup_command_id = seed_text(proof, setup_key)?.to_owned();
    if let Some(key) = request_key {
        seed_text(proof, key)?;
    }
    let currency = seed_text(proof, "currency")?;
    if currency.len() != 3 || !currency.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return Err(invalid(
            "COUNT_SEED_PROOF_INVALID",
            "Use the captured first-Count source proof.",
        ));
    }
    let (pos_bootstrap_receipt_id, pos_approval_receipt_id) = if kind == CountSeedKind::Pos {
        seed_text(proof, "posBootstrapRequestHash")?;
        seed_text(proof, "posBootstrapAuditId")?;
        seed_text(proof, "posApprovalRequestHash")?;
        seed_text(proof, "posApprovalAuditId")?;
        seed_text(proof, "posAccountingImageHash")?;
        (
            Some(seed_text(proof, "posBootstrapReceiptId")?.to_owned()),
            Some(seed_text(proof, "posApprovalReceiptId")?.to_owned()),
        )
    } else {
        (None, None)
    };
    Ok(CountSeedPlan {
        kind,
        proof: proof.clone(),
        branch_id: branch_id.to_owned(),
        product_id: product_id.to_owned(),
        setup_command_id,
        chart_id: seed_text(proof, "chartId")?.to_owned(),
        inventory_account_id: seed_text(proof, "inventoryAccountId")?.to_owned(),
        variance_account_id: seed_text(proof, "varianceAccountId")?.to_owned(),
        currency: currency.to_owned(),
        pos_bootstrap_receipt_id,
        pos_approval_receipt_id,
    })
}

/// Insert the immutable first-Count provenance row that the seeding adapters
/// previously wrote in TypeScript. Runs only inside the caller's unit, after the
/// draft and its command receipt exist.
pub async fn insert_count_seed_source(
    c: &mut PgConnection,
    seed: &CountSeedSource<'_>,
) -> Result<(), Error> {
    serializable(c).await?;
    const INITIAL: &str = "INSERT INTO inventory_initial_physical_stock_sources(count_id,\
         company_id,branch_id,warehouse_id,product_id,count_command_id,goods_setup_command_id,\
         chart_id,inventory_account_id,variance_account_id,currency,proof_json,created_by,created_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12::jsonb,$13,$14)";
    const MANUFACTURING: &str = "INSERT INTO inventory_mfg_initial_physical_sources(count_id,\
         company_id,branch_id,warehouse_id,product_id,count_command_id,\
         manufacturing_setup_command_id,chart_id,inventory_account_id,variance_account_id,\
         currency,proof_json,created_by,created_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12::jsonb,$13,$14)";
    const POS: &str = "INSERT INTO inventory_pos_initial_physical_sources(count_id,company_id,\
         branch_id,warehouse_id,product_id,count_command_id,pos_setup_command_id,\
         pos_bootstrap_receipt_id,pos_approval_receipt_id,chart_id,\
         inventory_account_id,variance_account_id,currency,proof_json,created_by,created_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14::jsonb,$15,$16)";
    const SERVICE: &str = "INSERT INTO inventory_service_initial_physical_sources(count_id,\
         company_id,branch_id,warehouse_id,product_id,count_command_id,service_setup_command_id,\
         chart_id,inventory_account_id,variance_account_id,currency,proof_json,created_by,created_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12::jsonb,$13,$14)";
    let sql = match seed.kind {
        CountSeedKind::Initial => INITIAL,
        CountSeedKind::Manufacturing => MANUFACTURING,
        CountSeedKind::Pos => POS,
        CountSeedKind::Service => SERVICE,
    };
    let mut query = sqlx::query(sql)
        .bind(seed.count_id)
        .bind(seed.company_id)
        .bind(seed.branch_id)
        .bind(seed.warehouse_id)
        .bind(seed.product_id)
        .bind(seed.count_command_id)
        .bind(seed.setup_command_id);
    if seed.kind == CountSeedKind::Pos {
        query = query
            .bind(seed.pos_bootstrap_receipt_id.ok_or_else(|| {
                invalid(
                    "COUNT_SEED_PROOF_INVALID",
                    "Use the captured POS first-Count source proof.",
                )
            })?)
            .bind(seed.pos_approval_receipt_id.ok_or_else(|| {
                invalid(
                    "COUNT_SEED_PROOF_INVALID",
                    "Use the captured POS first-Count source proof.",
                )
            })?);
    }
    query
        .bind(seed.chart_id)
        .bind(seed.inventory_account_id)
        .bind(seed.variance_account_id)
        .bind(seed.currency)
        .bind(seed.proof_json)
        .bind(seed.created_by)
        .bind(seed.created_at)
        .execute(&mut *c)
        .await?;
    Ok(())
}
