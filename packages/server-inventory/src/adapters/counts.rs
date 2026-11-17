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
         inventory_lot_balances IN SHARE ROW EXCLUSIVE MODE NOWAIT",
    )
    .execute(&mut *c)
    .await
    .map_err(PersistenceError::pre_snapshot_lock_conflict)?;
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
        "SELECT l.id,l.product_id,l.unit,p.tracking_mode,l.snapshot_quantity_milli,\
         l.snapshot_value_minor,l.snapshot_version,l.counted_quantity_milli,l.counted_value_minor \
         FROM inventory_count_lines l JOIN commerce_products p ON p.id=l.product_id \
         WHERE l.count_id=$1 ORDER BY l.ordinal",
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
    if record.quality_hold_id.is_some() {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "COUNT_QUALITY_DISPOSITION_REQUIRED",
            "Use the quality disposition command for a controlled stock count.",
        )
        .into());
    }
    apply_count_transition_inner(
        c,
        record,
        effects,
        expected_version,
        actor,
        journal_id,
        now,
        None,
    )
    .await
}

/// Apply a controlled Count only after Quality has staged its actual hold in
/// this same transaction. Identity/purchase/Finance/Workflow remain mandatory
/// composition witnesses; this source proves only the selected owner facts.
pub async fn apply_quality_count_effects(
    c: &mut PgConnection,
    expected_version: i64,
    quality: &erpx_server_quality::public::quality_commands::StagedQualityCountDisposition<
        CurrentInventoryQualityCount,
    >,
) -> Result<(), Error> {
    let source = quality.source();
    if expected_version != source.record().version
        || count_guard_unit(c).await? != source.guard.unit
    {
        return Err(changed());
    }
    quality.require_intermediate(c).await?;
    if source.effects().posting {
        apply_count_effects(
            c,
            source.record(),
            source.effects(),
            &quality.facts().actor_id,
            quality.facts().now,
            quality.lot_id(),
        )
        .await?;
    }
    quality.require_intermediate(c).await?;
    source.require_phase(c, false, false).await
}

pub async fn apply_quality_count_decision(
    c: &mut PgConnection,
    expected_version: i64,
    quality: &erpx_server_quality::public::quality_commands::StagedQualityCountDecision<
        CurrentInventoryQualityCount,
    >,
) -> Result<(), Error> {
    let source = quality.source();
    if expected_version != source.record().version
        || count_guard_unit(c).await? != source.guard.unit
    {
        return Err(changed());
    }
    quality.require_before_decision(c).await?;
    apply_count_decision(
        c,
        source.record(),
        source.effects(),
        expected_version,
        &quality.facts().actor_id,
        source.journal_id.as_deref(),
        quality.facts().now,
    )
    .await?;
    source
        .require_phase(c, quality.remaining_hold_id().is_some(), true)
        .await
}

async fn apply_count_transition_inner(
    c: &mut PgConnection,
    record: &CountRecord,
    effects: &CountTransitionEffects,
    expected_version: i64,
    actor: &str,
    journal_id: Option<&str>,
    now: i64,
    quality_lot_id: Option<&str>,
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
    apply_count_decision(c, record, effects, expected_version, actor, journal_id, now).await?;
    if effects.posting {
        apply_count_effects(c, record, effects, actor, now, quality_lot_id).await?;
    }
    Ok(())
}

async fn apply_count_decision(
    c: &mut PgConnection,
    record: &CountRecord,
    effects: &CountTransitionEffects,
    expected_version: i64,
    actor: &str,
    journal_id: Option<&str>,
    now: i64,
) -> Result<(), Error> {
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
    let updated = sqlx::query(if effects.posting {
        POST_DECISION
    } else {
        DECISION
    })
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
    Ok(())
}

async fn apply_count_effects(
    c: &mut PgConnection,
    record: &CountRecord,
    effects: &CountTransitionEffects,
    actor: &str,
    now: i64,
    quality_lot_id: Option<&str>,
) -> Result<(), Error> {
    for (line_index, stock) in effects.stocks.iter().enumerate() {
        if stock.stock_insert {
            sqlx::query("INSERT INTO commerce_stock(warehouse_id,product_id) VALUES($1,$2)")
                .bind(&record.warehouse_id)
                .bind(&stock.product_id)
                .execute(&mut *c)
                .await?;
        }
        sqlx::query("UPDATE commerce_stock SET quantity_milli=$1,value_minor=$2,version=version+1 WHERE warehouse_id=$3 AND product_id=$4")
            .bind(stock.counted_quantity_milli).bind(stock.counted_value_minor)
            .bind(&record.warehouse_id).bind(&stock.product_id).execute(&mut *c).await?;
        sqlx::query(
            "INSERT INTO inventory_adjustment_moves(id,count_id,warehouse_id,product_id,\
             quantity_milli,value_minor,movement_date,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(format!("{}-{}", record.id, line_index))
        .bind(&record.id)
        .bind(&record.warehouse_id)
        .bind(&stock.product_id)
        .bind(stock.counted_quantity_milli - stock.current_quantity_milli)
        .bind(stock.counted_value_minor - stock.current_value_minor)
        .bind(&record.count_date)
        .bind(now)
        .execute(&mut *c)
        .await?;
        for (lot_index, lot) in effects
            .lots
            .iter()
            .filter(|lot| lot.count_line_id == stock.line_id)
            .enumerate()
        {
            let move_id = format!("{}-{}-{}", record.id, line_index, lot_index);
            apply_lot(c, record, lot, actor, now, &move_id).await?;
        }
    }
    if effects.lots.iter().any(|lot| {
        !effects
            .stocks
            .iter()
            .any(|stock| stock.line_id == lot.count_line_id)
    }) {
        return Err(changed());
    }
    if let Some(lot_id) = quality_lot_id {
        sqlx::query(
            "UPDATE inventory_lots SET status='scrapped' WHERE id=$1 \
             AND NOT EXISTS(SELECT 1 FROM inventory_lot_balances WHERE lot_id=$1 AND quantity_milli>0)",
        )
        .bind(lot_id)
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
    move_id: &str,
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
    if lot.counted_quantity_milli == lot.snapshot_quantity_milli
        && lot.counted_value_minor == lot.snapshot_value_minor
    {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,\
         quantity_milli,value_minor,movement_date,created_by,created_at) \
         VALUES($1,$2,$3,'stock_count',$4,$5,$6,$7,$8,$9)",
    )
    .bind(move_id)
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

/// Inventory guard for the exact retained Count/stock/product/lot source. This
/// is not actor authority, purchase admission, Finance posting or a receipt.
/// The caller must acquire `lock_count_sources` and then this additional
/// ledger lock before the transaction's first source snapshot.
pub async fn lock_count_transition_guard_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query("LOCK TABLE inventory_ledger IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}

type CountGuardUnit = (String, i32, String, String);
async fn count_guard_unit(c: &mut PgConnection) -> Result<CountGuardUnit, Error> {
    serializable(c).await?;
    let held: bool = sqlx::query_scalar("SELECT count(DISTINCT relation)=9 FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND relation IN ('inventory_counts'::regclass,'inventory_count_lines'::regclass,'inventory_count_lot_lines'::regclass,'commerce_warehouses'::regclass,'commerce_products'::regclass,'commerce_stock'::regclass,'inventory_lots'::regclass,'inventory_lot_balances'::regclass,'inventory_ledger'::regclass) AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')")
        .fetch_one(&mut *c).await?;
    if !held {
        return Err(PersistenceError::new("INVENTORY_COUNT_GUARD_FENCE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}

pub struct CurrentCountTransitionGuard {
    unit: CountGuardUnit,
    record: CountRecord,
    lines: Vec<CountTransitionLine>,
    action: crate::domain::service_initial_physical_count::CountAction,
}
impl CurrentCountTransitionGuard {
    pub fn record(&self) -> &CountRecord {
        &self.record
    }
    pub fn lines(&self) -> &[CountTransitionLine] {
        &self.lines
    }
    /// Recheck before any effects. Trigger-induced stock changes after Quality
    /// stages a disposal require a separate retained after-image witness.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if count_guard_unit(c).await? != self.unit {
            return Err(changed());
        }
        let (record, lines) = read_count_transition_source(
            c,
            &self.record.company_id,
            Some(&self.record.branch_id),
            &self.record.id,
        )
        .await?;
        if record != self.record || lines != self.lines {
            return Err(changed());
        }
        check_count_transition_facts(c, &record, &lines, self.action).await
    }
}

/// Prove real current warehouse/product/stock/lot facts for a selected Count.
/// Does not validate current user, reviewer independence or package rights.
pub async fn inspect_count_transition_guard(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    id: &str,
    action: crate::domain::service_initial_physical_count::CountAction,
) -> Result<CurrentCountTransitionGuard, Error> {
    let unit = count_guard_unit(c).await?;
    let (record, lines) = read_count_transition_source(c, company, branch, id).await?;
    check_count_transition_facts(c, &record, &lines, action).await?;
    Ok(CurrentCountTransitionGuard {
        unit,
        record,
        lines,
        action,
    })
}

async fn check_count_transition_facts(
    c: &mut PgConnection,
    record: &CountRecord,
    lines: &[CountTransitionLine],
    action: crate::domain::service_initial_physical_count::CountAction,
) -> Result<(), Error> {
    use crate::domain::service_initial_physical_count::CountAction;
    if !matches!(
        action,
        CountAction::Submit | CountAction::Approve | CountAction::Post
    ) {
        return Ok(());
    }
    if lines.is_empty() {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "EMPTY_COUNT",
            "Enter the counted products first.",
        )
        .into());
    }
    let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM commerce_warehouses WHERE id=$1 AND company_id=$2 AND status='active')")
        .bind(&record.warehouse_id).bind(&record.company_id).fetch_one(&mut *c).await?;
    if !active {
        return Err(BusinessError::invalid(
            "WAREHOUSE_INVALID",
            "Choose an active warehouse in your scope.",
        )
        .into());
    }
    let invalid_lines: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inventory_count_lines l LEFT JOIN commerce_products p ON p.id=l.product_id LEFT JOIN commerce_stock s ON s.warehouse_id=$2 AND s.product_id=l.product_id WHERE l.count_id=$1 AND (p.id IS NULL OR p.version IS DISTINCT FROM l.product_version OR p.status<>'active' OR (l.snapshot_version=0 AND s.warehouse_id IS NOT NULL) OR (l.snapshot_version<>0 AND (s.warehouse_id IS NULL OR s.version IS DISTINCT FROM l.snapshot_version OR s.quantity_milli IS DISTINCT FROM l.snapshot_quantity_milli OR s.value_minor IS DISTINCT FROM l.snapshot_value_minor))))")
        .bind(&record.id).bind(&record.warehouse_id).fetch_one(&mut *c).await?;
    if invalid_lines {
        return Err(changed());
    }
    let invalid_lots: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inventory_count_lot_lines lot JOIN inventory_count_lines l ON l.id=lot.count_line_id LEFT JOIN inventory_lot_balances balance ON balance.lot_id=lot.source_lot_id AND balance.warehouse_id=$2 WHERE l.count_id=$1 AND ((lot.source_lot_id IS NOT NULL AND (balance.lot_id IS NULL OR balance.version IS DISTINCT FROM lot.snapshot_version OR balance.quantity_milli IS DISTINCT FROM lot.snapshot_quantity_milli OR balance.value_minor IS DISTINCT FROM lot.snapshot_value_minor)) OR (lot.source_lot_id IS NULL AND EXISTS(SELECT 1 FROM inventory_lots existing WHERE existing.id=lot.result_lot_id OR (existing.company_id=$3 AND existing.product_id=l.product_id AND ((existing.tracking_mode='lot' AND existing.lot_code=lot.lot_code) OR (existing.tracking_mode='serial' AND existing.serial_number=lot.serial_number)))))))")
        .bind(&record.id).bind(&record.warehouse_id).bind(&record.company_id).fetch_one(&mut *c).await?;
    if invalid_lots {
        return Err(changed());
    }
    if action == CountAction::Post {
        let moved: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inventory_ledger movement JOIN inventory_count_lines l ON l.product_id=movement.product_id WHERE l.count_id=$1 AND movement.warehouse_id=$2 AND movement.movement_date>$3)")
            .bind(&record.id).bind(&record.warehouse_id).bind(&record.count_date).fetch_one(&mut *c).await?;
        if moved {
            return Err(changed());
        }
    }
    Ok(())
}

/// Inventory-owned, transaction-bound Count witness for Quality disposition.
/// It proves owner facts/effects only; the caller must separately retain current
/// IAM/reviewer, purchase, Finance, Files, Workflow and Collaboration witnesses.
async fn count_row_image(
    c: &mut PgConnection,
    count_id: &str,
    decided: bool,
) -> Result<String, Error> {
    Ok(sqlx::query_scalar("SELECT jsonb_build_object('count', (SELECT CASE WHEN $2 THEN to_jsonb(c) - ARRAY['status','version','review_reason','updated_at','posted_at','journal_id']::text[] ELSE to_jsonb(c) END FROM inventory_counts c WHERE c.id=$1), 'lines', COALESCE((SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM inventory_count_lines l WHERE l.count_id=$1), '[]'::jsonb), 'lot_lines', COALESCE((SELECT jsonb_agg(to_jsonb(ll) ORDER BY ll.id) FROM inventory_count_lot_lines ll JOIN inventory_count_lines l ON l.id=ll.count_line_id WHERE l.count_id=$1), '[]'::jsonb))::text")
        .bind(count_id).bind(decided).fetch_one(c).await?)
}

async fn reject_inventory_image(
    c: &mut PgConnection,
    guard: &CurrentCountTransitionGuard,
    hold_lot_id: Option<&str>,
) -> Result<String, Error> {
    let products: Vec<&str> = guard
        .lines
        .iter()
        .map(|line| line.product_id.as_str())
        .collect();
    let mut lot_ids: Vec<&str> = guard
        .lines
        .iter()
        .flat_map(|line| line.lots.iter())
        .flat_map(|lot| {
            [
                lot.source_lot_id.as_deref(),
                Some(lot.result_lot_id.as_str()),
            ]
        })
        .flatten()
        .collect();
    if let Some(hold_lot_id) = hold_lot_id {
        lot_ids.push(hold_lot_id);
    }
    let lot_line_ids: Vec<&str> = guard
        .lines
        .iter()
        .flat_map(|line| line.lots.iter())
        .map(|lot| lot.id.as_str())
        .collect();
    Ok(sqlx::query_scalar("SELECT jsonb_build_object(        'count', (SELECT to_jsonb(c) - ARRAY['status','version','review_reason','updated_at']::text[] FROM inventory_counts c WHERE c.id=$4),        'lines', COALESCE((SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM inventory_count_lines l WHERE l.count_id=$4), '[]'::jsonb),        'lot_lines', COALESCE((SELECT jsonb_agg(to_jsonb(ll) ORDER BY ll.id) FROM inventory_count_lot_lines ll JOIN inventory_count_lines l ON l.id=ll.count_line_id WHERE l.count_id=$4), '[]'::jsonb),        'stock', COALESCE((SELECT jsonb_agg(to_jsonb(s) ORDER BY s.product_id) FROM commerce_stock s WHERE s.warehouse_id=$1 AND s.product_id=ANY($2)), '[]'::jsonb),        'lots', COALESCE((SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM inventory_lots l WHERE l.id=ANY($3)), '[]'::jsonb),        'balances', COALESCE((SELECT jsonb_agg(to_jsonb(b) ORDER BY b.lot_id,b.warehouse_id) FROM inventory_lot_balances b WHERE b.lot_id=ANY($3)), '[]'::jsonb),        'adjustments', COALESCE((SELECT jsonb_agg(to_jsonb(m) ORDER BY m.id) FROM inventory_adjustment_moves m WHERE m.count_id=$4), '[]'::jsonb),        'lot_moves', COALESCE((SELECT jsonb_agg(to_jsonb(m) ORDER BY m.id) FROM inventory_lot_moves m WHERE m.lot_id=ANY($3) OR (m.source_type='stock_count' AND m.source_id=ANY($5))), '[]'::jsonb))::text")
        .bind(&guard.record.warehouse_id).bind(&products).bind(&lot_ids).bind(&guard.record.id).bind(&lot_line_ids).fetch_one(c).await?)
}

pub struct CurrentInventoryQualityCount {
    guard: CurrentCountTransitionGuard,
    facts: erpx_server_quality::public::quality_commands::QualityCountDispositionFacts,
    effects: CountTransitionEffects,
    journal_id: Option<String>,
    lot_versions: BTreeMap<String, i64>,
    other_hold_lot_balances: Vec<(String, i64, i64, i64)>,
    reject_image: Option<String>,
    count_before_image: String,
    count_decided_baseline: String,
}
impl CurrentInventoryQualityCount {
    pub fn record(&self) -> &CountRecord {
        self.guard.record()
    }
    pub fn effects(&self) -> &CountTransitionEffects {
        &self.effects
    }
}

/// Mint only from actual fenced Inventory sources before Quality's first effect.
/// The supplied Quality facts do not grant actor or purchase authority; Quality
/// independently matches them to its locked hold before it stages disposal.
pub async fn prepare_inventory_quality_count(
    c: &mut PgConnection,
    guard: CurrentCountTransitionGuard,
    facts: erpx_server_quality::public::quality_commands::QualityCountDispositionFacts,
    effects: CountTransitionEffects,
    journal_id: Option<String>,
) -> Result<CurrentInventoryQualityCount, Error> {
    use crate::domain::service_initial_physical_count::{CountAction, CountStatus};
    use erpx_server_quality::public::quality_commands::QualityCountDispositionAction;
    guard.require_current(c).await?;
    let r = guard.record();
    let (action, status, posting) = match facts.action {
        QualityCountDispositionAction::Post => (CountAction::Post, CountStatus::Posted, true),
        QualityCountDispositionAction::Reject => {
            (CountAction::Reject, CountStatus::Rejected, false)
        }
    };
    if guard.action != action
        || r.id != facts.count_id
        || r.company_id != facts.company_id
        || r.branch_id != facts.branch_id
        || r.warehouse_id != facts.warehouse_id
        || r.quality_hold_id.as_deref() != Some(facts.hold_id.as_str())
        || !guard
            .lines
            .iter()
            .any(|line| line.product_id == facts.product_id)
    {
        return Err(changed());
    }
    let plan = crate::domain::counts::CountTransitionPlan {
        next: status,
        posting,
        reason: Some(facts.reason.clone()),
    };
    let expected = crate::domain::counts::plan_count_transition_effects(&plan, &guard.lines)?;
    if effects != expected {
        return Err(changed());
    }
    let original_journal: Option<String> =
        sqlx::query_scalar("SELECT journal_id FROM inventory_counts WHERE id=$1 AND company_id=$2")
            .bind(&r.id)
            .bind(&r.company_id)
            .fetch_one(&mut *c)
            .await?;
    let journal_id = journal_id.or(original_journal);
    let mut lot_versions = BTreeMap::new();
    for line in &guard.lines {
        for lot in &line.lots {
            let version: i64 = sqlx::query_scalar("SELECT snapshot_version FROM inventory_count_lot_lines WHERE id=$1 AND count_line_id=$2")
                .bind(&lot.id).bind(&line.id).fetch_one(&mut *c).await?;
            if lot_versions.insert(lot.id.clone(), version).is_some() {
                return Err(changed());
            }
        }
    }
    let other_hold_lot_balances = if let Some(lot_id) = &facts.lot_id {
        sqlx::query_as("SELECT warehouse_id,quantity_milli,value_minor,version FROM inventory_lot_balances WHERE lot_id=$1 AND warehouse_id<>$2 ORDER BY warehouse_id")
            .bind(lot_id).bind(&r.warehouse_id).fetch_all(&mut *c).await?
    } else {
        Vec::new()
    };
    let reject_image = if facts.action == QualityCountDispositionAction::Reject {
        Some(reject_inventory_image(c, &guard, facts.lot_id.as_deref()).await?)
    } else {
        None
    };
    let count_before_image = count_row_image(c, &guard.record.id, false).await?;
    let count_decided_baseline = count_row_image(c, &guard.record.id, true).await?;
    Ok(CurrentInventoryQualityCount {
        guard,
        facts,
        effects,
        journal_id,
        lot_versions,
        other_hold_lot_balances,
        reject_image,
        count_before_image,
        count_decided_baseline,
    })
}

impl erpx_server_quality::public::quality_commands::QualityCountDispositionSource
    for CurrentInventoryQualityCount
{
    fn facts(
        &self,
    ) -> &erpx_server_quality::public::quality_commands::QualityCountDispositionFacts {
        &self.facts
    }
    async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.guard.require_current(c).await?;
        if let Some(image) = &self.reject_image {
            if reject_inventory_image(c, &self.guard, self.facts.lot_id.as_deref()).await? != *image
            {
                return Err(changed());
            }
        }
        Ok(())
    }
    async fn require_count_before_decision(
        &self,
        c: &mut PgConnection,
        remaining_hold_inserted: bool,
    ) -> Result<(), Error> {
        self.require_phase(c, remaining_hold_inserted, false).await
    }
    async fn require_count_after(
        &self,
        c: &mut PgConnection,
        remaining_hold_inserted: bool,
    ) -> Result<(), Error> {
        self.require_phase(c, remaining_hold_inserted, true).await
    }
}
impl CurrentInventoryQualityCount {
    async fn require_phase(
        &self,
        c: &mut PgConnection,
        remaining_hold_inserted: bool,
        decided: bool,
    ) -> Result<(), Error> {
        use erpx_server_quality::public::quality_commands::QualityCountDispositionAction;
        if count_guard_unit(c).await? != self.guard.unit {
            return Err(changed());
        }
        let before = &self.guard.record;
        let expected_image = if decided {
            &self.count_decided_baseline
        } else {
            &self.count_before_image
        };
        if count_row_image(c, &before.id, decided).await? != *expected_image {
            return Err(changed());
        }
        let (actual, lines) = read_count_transition_source(
            c,
            &before.company_id,
            Some(&before.branch_id),
            &before.id,
        )
        .await?;
        let mut expected = before.clone();
        if decided {
            expected.status = self.effects.next.clone();
            expected.version = before.version.checked_add(1).ok_or_else(changed)?;
        }
        if actual != expected || lines != self.guard.lines {
            return Err(changed());
        }
        let detail = sqlx::query("SELECT review_reason,journal_id,posted_at,updated_at FROM inventory_counts WHERE id=$1 AND company_id=$2")
            .bind(&before.id).bind(&before.company_id).fetch_one(&mut *c).await?;
        let review: Option<String> = detail.try_get("review_reason")?;
        let journal: Option<String> = detail.try_get("journal_id")?;
        let posted_at: Option<i64> = detail.try_get("posted_at")?;
        let updated_at: i64 = detail.try_get("updated_at")?;
        if decided
            && (review.as_deref() != Some(self.facts.reason.as_str())
                || journal != self.journal_id
                || updated_at != self.facts.now
                || (self.facts.action == QualityCountDispositionAction::Post
                    && posted_at != Some(self.facts.now)))
        {
            return Err(changed());
        }
        if self.facts.action == QualityCountDispositionAction::Reject {
            if remaining_hold_inserted
                || self.reject_image.as_ref()
                    != Some(
                        &reject_inventory_image(c, &self.guard, self.facts.lot_id.as_deref())
                            .await?,
                    )
            {
                return Err(changed());
            }
            return Ok(());
        }
        for (index, stock) in self.effects.stocks.iter().enumerate() {
            let line = self.guard.lines.get(index).ok_or_else(changed)?;
            if line.id != stock.line_id || line.product_id != stock.product_id {
                return Err(changed());
            }
            let version = if stock.stock_insert {
                2
            } else {
                line.snapshot_version.checked_add(1).ok_or_else(changed)?
            };
            let version = if remaining_hold_inserted && stock.product_id == self.facts.product_id {
                version.checked_add(1).ok_or_else(changed)?
            } else {
                version
            };
            let current: Option<(i64,i64,i64)> = sqlx::query_as("SELECT quantity_milli,value_minor,version FROM commerce_stock WHERE warehouse_id=$1 AND product_id=$2")
                .bind(&before.warehouse_id).bind(&stock.product_id).fetch_optional(&mut *c).await?;
            if current
                != Some((
                    stock.counted_quantity_milli,
                    stock.counted_value_minor,
                    version,
                ))
            {
                return Err(changed());
            }
            let movement: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inventory_adjustment_moves WHERE id=$1 AND count_id=$2 AND warehouse_id=$3 AND product_id=$4 AND quantity_milli=$5 AND value_minor=$6 AND movement_date=$7 AND created_at=$8)")
                .bind(format!("{}-{}",before.id,index)).bind(&before.id).bind(&before.warehouse_id).bind(&stock.product_id)
                .bind(stock.counted_quantity_milli-stock.current_quantity_milli).bind(stock.counted_value_minor-stock.current_value_minor).bind(&before.count_date).bind(self.facts.now).fetch_one(&mut *c).await?;
            if !movement {
                return Err(changed());
            }
        }
        if let Some(lot_id) = &self.facts.lot_id {
            let balances: Vec<(String, i64, i64, i64)> = sqlx::query_as("SELECT warehouse_id,quantity_milli,value_minor,version FROM inventory_lot_balances WHERE lot_id=$1 AND warehouse_id<>$2 ORDER BY warehouse_id")
                .bind(lot_id).bind(&before.warehouse_id).fetch_all(&mut *c).await?;
            if balances != self.other_hold_lot_balances {
                return Err(changed());
            }
        }
        let mut lot_indices = BTreeMap::<&str, usize>::new();
        for lot in &self.effects.lots {
            let line_index = self
                .effects
                .stocks
                .iter()
                .position(|line| line.line_id == lot.count_line_id)
                .ok_or_else(changed)?;
            let index = lot_indices.entry(lot.count_line_id.as_str()).or_default();
            let move_id = format!("{}-{}-{}", before.id, line_index, *index);
            *index += 1;
            let version = if lot.source_lot_id.is_some() {
                self.lot_versions
                    .get(&lot.lot_id)
                    .copied()
                    .ok_or_else(changed)?
                    .checked_add(1)
                    .ok_or_else(changed)?
            } else {
                1
            };
            let current: Option<(i64,i64,i64,String)> = sqlx::query_as("SELECT b.quantity_milli,b.value_minor,b.version,l.status FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE b.lot_id=$1 AND b.warehouse_id=$2 AND l.company_id=$3")
                .bind(&lot.result_lot_id).bind(&before.warehouse_id).bind(&before.company_id).fetch_optional(&mut *c).await?;
            let status = if lot.source_lot_id.is_none() || lot.counted_quantity_milli > 0 {
                "active"
            } else if self.facts.lot_id.as_deref() == Some(lot.result_lot_id.as_str())
                && !self
                    .other_hold_lot_balances
                    .iter()
                    .any(|(_, quantity, _, _)| *quantity > 0)
            {
                "scrapped"
            } else {
                "depleted"
            };
            if current
                != Some((
                    lot.counted_quantity_milli,
                    lot.counted_value_minor,
                    version,
                    status.to_owned(),
                ))
            {
                return Err(changed());
            }
            let movement: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inventory_lot_moves WHERE id=$1 AND lot_id=$2 AND warehouse_id=$3 AND source_type='stock_count' AND source_id=$4 AND quantity_milli=$5 AND value_minor=$6 AND movement_date=$7 AND created_by=$8 AND created_at=$9)")
                .bind(move_id).bind(&lot.result_lot_id).bind(&before.warehouse_id).bind(&lot.lot_id).bind(lot.counted_quantity_milli-lot.snapshot_quantity_milli).bind(lot.counted_value_minor-lot.snapshot_value_minor).bind(&before.count_date).bind(&self.facts.actor_id).bind(self.facts.now).fetch_one(&mut *c).await?;
            let changed_lot = lot.counted_quantity_milli != lot.snapshot_quantity_milli
                || lot.counted_value_minor != lot.snapshot_value_minor;
            if movement != changed_lot {
                return Err(changed());
            }
        }
        let actual_moves: i64 =
            sqlx::query_scalar("SELECT count(*) FROM inventory_adjustment_moves WHERE count_id=$1")
                .bind(&before.id)
                .fetch_one(&mut *c)
                .await?;
        if actual_moves != self.effects.stocks.len() as i64 {
            return Err(changed());
        }
        let lot_line_ids: Vec<&str> = self
            .effects
            .lots
            .iter()
            .map(|lot| lot.lot_id.as_str())
            .collect();
        let actual_lot_moves: i64 = sqlx::query_scalar("SELECT count(*) FROM inventory_lot_moves WHERE source_type='stock_count' AND source_id=ANY($1)")
            .bind(&lot_line_ids).fetch_one(&mut *c).await?;
        let expected_lot_moves = self
            .effects
            .lots
            .iter()
            .filter(|lot| {
                lot.counted_quantity_milli != lot.snapshot_quantity_milli
                    || lot.counted_value_minor != lot.snapshot_value_minor
            })
            .count() as i64;
        if actual_lot_moves != expected_lot_moves {
            return Err(changed());
        }
        Ok(())
    }
}

/// Exact pre-action `countDetail` rows for Workflow decision digest input.
/// JSON is returned as ordered text because JS object property order is part of
/// `JSON.stringify`; converting through serde_json::Value would reorder keys.
pub struct CurrentQualityCountDecisionProjection {
    source: std::sync::Arc<CurrentInventoryQualityCount>,
    lines_json: String,
    lot_lines_json: String,
    product_image: String,
}
impl CurrentQualityCountDecisionProjection {
    pub fn lines_json(&self) -> &str {
        &self.lines_json
    }
    pub fn lot_lines_json(&self) -> &str {
        &self.lot_lines_json
    }
    pub async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.source.guard.require_current(c).await?;
        let (lines, lots, products) =
            read_quality_count_decision_projection(c, &self.source.guard.record).await?;
        if lines != self.lines_json || lots != self.lot_lines_json || products != self.product_image
        {
            return Err(changed());
        }
        Ok(())
    }
    /// After Count effects, the owner witness proves exact expected after-image;
    /// original ordered rows remain the digest input. A finalized Quality proof
    /// must separately prove whether a remaining child actually exists.
    pub async fn require_after(
        &self,
        c: &mut PgConnection,
        remaining_hold_inserted: bool,
    ) -> Result<(), Error> {
        self.source
            .require_phase(c, remaining_hold_inserted, true)
            .await?;
        if read_count_product_image(c, &self.source.guard.record).await? != self.product_image {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn inspect_quality_count_decision_projection(
    c: &mut PgConnection,
    source: std::sync::Arc<CurrentInventoryQualityCount>,
) -> Result<CurrentQualityCountDecisionProjection, Error> {
    source.guard.require_current(c).await?;
    let (lines_json, lot_lines_json, product_image) =
        read_quality_count_decision_projection(c, &source.guard.record).await?;
    source.guard.require_current(c).await?;
    Ok(CurrentQualityCountDecisionProjection {
        source,
        lines_json,
        lot_lines_json,
        product_image,
    })
}
async fn read_count_product_image(
    c: &mut PgConnection,
    record: &CountRecord,
) -> Result<String, Error> {
    let image: String = sqlx::query_scalar("SELECT COALESCE(jsonb_agg(to_jsonb(p) ORDER BY p.id),'[]'::jsonb)::text FROM commerce_products p WHERE p.id IN (SELECT l.product_id FROM inventory_count_lines l WHERE l.count_id=$1)")
        .bind(&record.id).fetch_one(c).await?;
    Ok(image)
}
async fn ordered_count_rows(
    c: &mut PgConnection,
    record: &CountRecord,
    lots: bool,
) -> Result<String, Error> {
    // row_to_json preserves SELECT column order, including all physical owner
    // columns before the Commerce/stock aliases used by the legacy query.
    let sql = if lots {
        "WITH rows AS MATERIALIZED (SELECT l.ordinal ord,cl.id lot_id,row_to_json(v)::text value FROM inventory_count_lot_lines cl JOIN inventory_count_lines l ON l.id=cl.count_line_id LEFT JOIN inventory_lot_balances b ON b.lot_id=cl.result_lot_id AND b.warehouse_id=$1 CROSS JOIN LATERAL (SELECT cl.*,b.quantity_milli current_quantity_milli,b.value_minor current_value_minor,COALESCE(b.version,0) current_version) v WHERE l.count_id=$2), bounded AS (SELECT ord,lot_id,value,octet_length(value) bytes FROM rows) SELECT CASE WHEN NOT EXISTS(SELECT 1 FROM jsonb_each(value::jsonb) j WHERE CASE WHEN jsonb_typeof(j.value)='number' THEN abs((j.value#>>'{}')::numeric)>9007199254740991 ELSE false END) THEN value ELSE NULL END FROM bounded ORDER BY ord,lot_id"
    } else {
        "WITH rows AS MATERIALIZED (SELECT l.ordinal ord,row_to_json(v)::text value FROM inventory_count_lines l JOIN commerce_products p ON p.id=l.product_id LEFT JOIN commerce_stock s ON s.product_id=l.product_id AND s.warehouse_id=$1 CROSS JOIN LATERAL (SELECT l.*,p.sku,p.tracking_mode,s.quantity_milli current_quantity_milli,s.value_minor current_value_minor,COALESCE(s.version,0) current_version) v WHERE l.count_id=$2), bounded AS (SELECT ord,value,octet_length(value) bytes FROM rows) SELECT CASE WHEN NOT EXISTS(SELECT 1 FROM jsonb_each(value::jsonb) j WHERE CASE WHEN jsonb_typeof(j.value)='number' THEN abs((j.value#>>'{}')::numeric)>9007199254740991 ELSE false END) THEN value ELSE NULL END FROM bounded ORDER BY ord"
    };
    let rows: Vec<Option<String>> = sqlx::query_scalar(sql)
        .bind(&record.warehouse_id)
        .bind(&record.id)
        .fetch_all(c)
        .await?;
    let rows = rows
        .into_iter()
        .map(|row| row.ok_or_else(changed))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(format!("[{}]", rows.join(",")))
}
async fn read_quality_count_decision_projection(
    c: &mut PgConnection,
    record: &CountRecord,
) -> Result<(String, String, String), Error> {
    let lines = ordered_count_rows(c, record, false).await?;
    let lots = ordered_count_rows(c, record, true).await?;
    let products = read_count_product_image(c, record).await?;
    Ok((lines, lots, products))
}

/// Direct controlled Count origin, not a receipt, entitlement, actor grant or
/// customer-return lineage. Workflow must separately prove the original scrap
/// command receipt for the exposed exact command/hold/actor/Count tuple, and
/// Quality/Commerce must prove the actual source and current purchase access.
pub struct CurrentDirectQualityCountOrigin {
    unit: CountGuardUnit,
    company_id: String,
    branch_id: String,
    count_id: String,
    hold_id: String,
    creator: String,
    source_command_id: String,
    image: Vec<Vec<u8>>,
}
impl CurrentDirectQualityCountOrigin {
    pub fn company_id(&self) -> &str {
        &self.company_id
    }
    pub fn branch_id(&self) -> &str {
        &self.branch_id
    }
    pub fn count_id(&self) -> &str {
        &self.count_id
    }
    pub fn hold_id(&self) -> &str {
        &self.hold_id
    }
    pub fn creator(&self) -> &str {
        &self.creator
    }
    pub fn source_command_id(&self) -> &str {
        &self.source_command_id
    }
    /// Rechecks actual scoped owner identity and immutable origin image. The
    /// allowed Count/hold transition fields may change as in legacy replay.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if direct_quality_count_unit(c).await? != self.unit {
            return Err(changed());
        }
        let found = read_direct_quality_count_origin(
            c,
            &self.company_id,
            Some(&self.branch_id),
            &self.count_id,
        )
        .await?;
        match found {
            Some((branch, hold, creator, command, image))
                if branch == self.branch_id
                    && hold == self.hold_id
                    && creator == self.creator
                    && command == self.source_command_id
                    && image == self.image =>
            {
                Ok(())
            }
            _ => Err(changed()),
        }
    }
}
async fn direct_quality_count_unit(c: &mut PgConnection) -> Result<CountGuardUnit, Error> {
    let unit = count_guard_unit(c).await?;
    let held:bool=sqlx::query_scalar("SELECT count(DISTINCT relation)=2 FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND relation IN ('inventory_quality_holds'::regclass,'inventory_quality_disposition_events'::regclass) AND mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')").fetch_one(&mut *c).await?;
    if !held {
        return Err(PersistenceError::new("QUALITY_DISPOSITION_SOURCE_FENCE_REQUIRED").into());
    }
    Ok(unit)
}
/// Call after `lock_count_sources`, `lock_count_transition_guard_sources`, and
/// Quality's `lock_quality_count_disposition_sources`, all before snapshot.
/// `None` means the scoped direct owner relation does not qualify.
pub async fn inspect_direct_quality_count_origin(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    count_id: &str,
) -> Result<Option<CurrentDirectQualityCountOrigin>, Error> {
    let unit = direct_quality_count_unit(c).await?;
    let found = read_direct_quality_count_origin(c, company, branch, count_id).await?;
    Ok(
        found.map(|(branch_id, hold_id, creator, source_command_id, image)| {
            CurrentDirectQualityCountOrigin {
                unit,
                company_id: company.to_owned(),
                branch_id,
                count_id: count_id.to_owned(),
                hold_id,
                creator,
                source_command_id,
                image,
            }
        }),
    )
}
async fn read_direct_quality_count_origin(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    count_id: &str,
) -> Result<Option<(String, String, String, String, Vec<Vec<u8>>)>, Error> {
    // Materialize the exact source once in PostgreSQL and transfer ordered
    // byte chunks. Splitting UTF-8 is safe because equality compares bytes.
    // Both PostgreSQL and this witness still retain the full source image.
    let rows = sqlx::query("WITH source AS MATERIALIZED (SELECT c.branch_id,h.id hold_id,c.created_by,substring(c.id FROM 15) command_id,convert_to(image.v,'UTF8') image FROM inventory_counts c JOIN inventory_quality_holds h ON h.id=c.quality_hold_id AND h.company_id=c.company_id AND h.branch_id=c.branch_id AND h.warehouse_id=c.warehouse_id JOIN inventory_quality_disposition_events e ON e.hold_id=h.id AND e.count_id=c.id AND e.action='request' AND e.actor_id=c.created_by AND e.id='quality-disposition-'||substring(c.id FROM 15) CROSS JOIN LATERAL (SELECT jsonb_build_object('count',to_jsonb(c)-ARRAY['status','review_reason','approved_by','submitted_at','posted_at','journal_id','version','updated_at']::text[], 'hold',to_jsonb(h)-ARRAY['status','version','updated_at','disposition_kind','disposition_reason','disposition_requested_by','disposition_count_id','disposition_quantity_milli','disposed_by','disposed_at']::text[], 'request',to_jsonb(e), 'lines',COALESCE((SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM inventory_count_lines l WHERE l.count_id=c.id),'[]'::jsonb))::text v) image WHERE c.id=$1 AND c.company_id=$2 AND ($3::text IS NULL OR c.branch_id=$3) AND left(c.id,14)='quality-scrap-' AND length(c.id)>14 AND c.created_by=c.prepared_by AND EXISTS(SELECT 1 FROM inventory_count_lines l WHERE l.count_id=c.id AND l.product_id=h.product_id AND l.snapshot_quantity_milli>l.counted_quantity_milli) AND NOT EXISTS(SELECT 1 FROM inventory_count_lines l WHERE l.count_id=c.id AND l.product_id<>h.product_id) AND (c.status IN ('rejected','cancelled') OR (h.disposition_count_id=c.id AND h.disposition_kind='scrap' AND h.disposition_quantity_milli>0 AND h.last_result='fail' AND h.status IN ('disposition_pending','disposed'))) LIMIT 2), sized AS MATERIALIZED (SELECT count(*) n FROM source) SELECT sized.n,source.branch_id,source.hold_id,source.created_by,source.command_id,substring(source.image FROM chunks.pos FOR 16384) image FROM sized LEFT JOIN source ON sized.n=1 LEFT JOIN LATERAL generate_series(1,octet_length(source.image),16384) AS chunks(pos) ON true ORDER BY chunks.pos")
        .bind(count_id).bind(company).bind(branch).fetch_all(c).await?;
    let first = rows.first().ok_or_else(changed)?;
    let count: i64 = first.try_get("n")?;
    if count == 0 { return Ok(None); }
    if count != 1 { return Err(changed()); }
    let branch = first.try_get("branch_id")?;
    let hold = first.try_get("hold_id")?;
    let creator = first.try_get("created_by")?;
    let command = first.try_get("command_id")?;
    let mut image = Vec::with_capacity(rows.len());
    for row in rows { image.push(row.try_get::<Vec<u8>, _>("image")?); }
    Ok(Some((branch, hold, creator, command, image)))
}
