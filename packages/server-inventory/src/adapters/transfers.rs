//! PostgreSQL transfer source/effect seam for the native transfer command.
//!
//! The caller (composition) owns current IAM/Commercial authority, the Workflow
//! command ledger and Files evidence. These functions read current owner facts,
//! revalidate the planned outcome against the same locked rows and apply only
//! the Inventory stock/lot/move/decision effects inside the caller's
//! serializable unit.
use crate::domain::transfers::{
    LotBalance, LotSnapshot, StockSnapshot, TransferAction, TransferAllocationRecord,
    TransferLineRecord, TransferRecord, TransferSavePlan, TransferSaveSource, TransitPostingConfig,
    TransferTransitionPlan, TransferTransitionSource, Warehouse,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "TRANSFER_SOURCE_CHANGED",
        "The transfer changed. Reload its current state before submitting.",
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

/// Lock every relation the transfer command reads or writes before the first
/// owner read. Transit rows are included because a posted transit transfer
/// creates its carrier record in the same unit.
pub async fn lock_transfer_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE inventory_transfers,inventory_transfer_lines,\
         inventory_transfer_lot_allocations,inventory_transfer_moves,\
         inventory_transfer_decisions,inventory_transit,inventory_transit_lines,\
         inventory_transit_lots,commerce_stock,commerce_warehouses,commerce_products,\
         inventory_lot_balances,inventory_lot_moves \
         IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(&mut *c)
    .await?;
    Ok(())
}

fn warehouse(row: &sqlx::postgres::PgRow) -> Result<Warehouse, sqlx::Error> {
    Ok(Warehouse {
        id: row.try_get("id")?,
        branch_id: row.try_get("branch_id")?,
        version: row.try_get("version")?,
    })
}

async fn read_warehouse(
    c: &mut PgConnection,
    company: &str,
    id: &str,
) -> Result<Option<Warehouse>, Error> {
    let row = sqlx::query(
        "SELECT id,branch_id,version FROM commerce_warehouses \
         WHERE id=$1 AND company_id=$2 AND status='active'",
    )
    .bind(id)
    .bind(company)
    .fetch_optional(&mut *c)
    .await?;
    row.as_ref().map(warehouse).transpose().map_err(Into::into)
}

async fn read_stock(
    c: &mut PgConnection,
    company: &str,
    warehouse: &str,
    product: &str,
) -> Result<Option<StockSnapshot>, Error> {
    let row = sqlx::query(
        "SELECT s.product_id,s.quantity_milli,s.value_minor,s.version,\
         p.version product_version,p.name,p.unit,p.tracking_mode,\
         COALESCE((SELECT SUM(r.quantity_milli) FROM inventory_active_reservations r \
           WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0) reserved_milli,\
         inventory_quarantined_quantity(s.warehouse_id,s.product_id) quarantined_milli \
         FROM commerce_stock s JOIN commerce_products p ON p.id=s.product_id \
         WHERE s.warehouse_id=$1 AND s.product_id=$2 AND p.company_id=$3 AND p.status='active'",
    )
    .bind(warehouse)
    .bind(product)
    .bind(company)
    .fetch_optional(&mut *c)
    .await?;
    row.map(|row| {
        Ok::<_, sqlx::Error>(StockSnapshot {
            product_id: row.try_get("product_id")?,
            name: row.try_get("name")?,
            unit: row.try_get("unit")?,
            tracking_mode: row.try_get("tracking_mode")?,
            quantity_milli: row.try_get("quantity_milli")?,
            value_minor: row.try_get("value_minor")?,
            reserved_milli: row.try_get("reserved_milli")?,
            quarantined_milli: row.try_get("quarantined_milli")?,
            version: row.try_get("version")?,
            product_version: row.try_get("product_version")?,
        })
    })
    .transpose()
    .map_err(Into::into)
}

async fn read_lot(
    c: &mut PgConnection,
    company: &str,
    product: &str,
    warehouse: &str,
    lot: &str,
) -> Result<Option<LotSnapshot>, Error> {
    let row = sqlx::query(
        "SELECT l.id lot_id,b.quantity_milli,b.value_minor,b.version balance_version,\
         CASE WHEN EXISTS(SELECT 1 FROM inventory_quality_holds h \
           WHERE h.lot_id=l.id AND h.warehouse_id=b.warehouse_id \
             AND h.status NOT IN ('released','cancelled','disposed')) THEN 1 ELSE 0 END quality_held \
         FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id \
         WHERE l.id=$1 AND l.company_id=$2 AND l.product_id=$3 AND b.warehouse_id=$4",
    )
    .bind(lot)
    .bind(company)
    .bind(product)
    .bind(warehouse)
    .fetch_optional(&mut *c)
    .await?;
    row.map(|row| {
        let held: i32 = row.try_get("quality_held")?;
        Ok::<_, sqlx::Error>(LotSnapshot {
            lot_id: row.try_get("lot_id")?,
            quantity_milli: row.try_get("quantity_milli")?,
            value_minor: row.try_get("value_minor")?,
            balance_version: row.try_get("balance_version")?,
            quality_held: held != 0,
        })
    })
    .transpose()
    .map_err(Into::into)
}

async fn read_transfer(
    c: &mut PgConnection,
    company: &str,
    id: &str,
) -> Result<Option<TransferRecord>, Error> {
    let row = sqlx::query(
        "SELECT id,company_id,source_warehouse_id,target_warehouse_id,source_branch_id,\
         target_branch_id,source_warehouse_version,target_warehouse_version,status,version,\
         created_by,prepared_by,delivery_mode,arrival_due_date,transfer_date \
         FROM inventory_transfers WHERE id=$1 AND company_id=$2",
    )
    .bind(id)
    .bind(company)
    .fetch_optional(&mut *c)
    .await?;
    row.map(|row| {
        Ok::<_, sqlx::Error>(TransferRecord {
            id: row.try_get("id")?,
            company_id: row.try_get("company_id")?,
            source_warehouse_id: row.try_get("source_warehouse_id")?,
            target_warehouse_id: row.try_get("target_warehouse_id")?,
            source_branch_id: row.try_get("source_branch_id")?,
            target_branch_id: row.try_get("target_branch_id")?,
            source_warehouse_version: row.try_get("source_warehouse_version")?,
            target_warehouse_version: row.try_get("target_warehouse_version")?,
            status: row.try_get("status")?,
            version: row.try_get("version")?,
            created_by: row.try_get("created_by")?,
            prepared_by: row.try_get("prepared_by")?,
            delivery_mode: row.try_get("delivery_mode")?,
            arrival_due_date: row.try_get("arrival_due_date")?,
            transfer_date: row.try_get("transfer_date")?,
        })
    })
    .transpose()
    .map_err(Into::into)
}

/// Read every fact `plan_save` needs for the submitted route and lines.
pub async fn read_transfer_save_source(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    id: Option<&str>,
    input: &crate::domain::transfers::TransferSaveInput,
) -> Result<TransferSaveSource, Error> {
    serializable(c).await?;
    let current = match id {
        Some(id) => {
            let record = read_transfer(c, company, id).await?.ok_or_else(|| {
                BusinessError::new(
                    FailureKind::NotFound,
                    "TRANSFER_NOT_FOUND",
                    "Transfer not found in this workspace.",
                )
            })?;
            if let Some(branch) = branch
                && record.source_branch_id != branch
                && record.target_branch_id != branch
            {
                return Err(BusinessError::new(
                    FailureKind::NotFound,
                    "TRANSFER_NOT_FOUND",
                    "Transfer not found in this workspace.",
                )
                .into());
            }
            Some(record)
        }
        None => None,
    };
    let source = read_warehouse(c, company, &input.source_warehouse_id)
        .await?
        .ok_or_else(|| {
            BusinessError::invalid(
                "WAREHOUSE_INVALID",
                "Choose an authorized source and a receiving warehouse in this company.",
            )
        })?;
    let target = read_warehouse(c, company, &input.target_warehouse_id)
        .await?
        .ok_or_else(|| {
            BusinessError::invalid(
                "WAREHOUSE_INVALID",
                "Choose an authorized source and a receiving warehouse in this company.",
            )
        })?;
    if let Some(branch) = branch
        && source.branch_id != branch
    {
        return Err(BusinessError::invalid(
            "WAREHOUSE_INVALID",
            "Choose an authorized source and a receiving warehouse in this company.",
        )
        .into());
    }
    let mut lines = Vec::with_capacity(input.lines.len());
    for line in &input.lines {
        let stock = read_stock(c, company, &source.id, &line.product_id)
            .await?
            .ok_or_else(|| {
                BusinessError::invalid(
                    "INSUFFICIENT_STOCK",
                    "Transfer quantity must be positive and within available source stock.",
                )
            })?;
        let mut lots = Vec::with_capacity(line.allocations.len());
        for allocation in &line.allocations {
            if let Some(lot) = read_lot(c, company, &line.product_id, &source.id, &allocation.lot_id)
                .await?
            {
                lots.push(lot);
            }
        }
        lines.push(crate::domain::transfers::TransferLineFacts { stock, lots });
    }
    Ok(TransferSaveSource {
        current,
        source,
        target,
        lines,
    })
}

async fn read_lines(
    c: &mut PgConnection,
    id: &str,
) -> Result<Vec<TransferLineRecord>, Error> {
    let rows = sqlx::query(
        "SELECT l.id,l.product_id,l.product_version,l.description,l.unit,\
         p.tracking_mode,l.quantity_milli,l.value_minor,l.source_quantity_milli,\
         l.source_value_minor,l.source_version,l.ordinal \
         FROM inventory_transfer_lines l JOIN commerce_products p ON p.id=l.product_id \
         WHERE l.transfer_id=$1 ORDER BY l.ordinal",
    )
    .bind(id)
    .fetch_all(&mut *c)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(TransferLineRecord {
                id: row.try_get("id")?,
                product_id: row.try_get("product_id")?,
                product_version: row.try_get("product_version")?,
                description: row.try_get("description")?,
                unit: row.try_get("unit")?,
                tracking_mode: row.try_get("tracking_mode")?,
                quantity_milli: row.try_get("quantity_milli")?,
                value_minor: row.try_get("value_minor")?,
                source_quantity_milli: row.try_get("source_quantity_milli")?,
                source_value_minor: row.try_get("source_value_minor")?,
                source_version: row.try_get("source_version")?,
                ordinal: row.try_get("ordinal")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(Into::into)
}

async fn read_allocations(
    c: &mut PgConnection,
    id: &str,
) -> Result<Vec<TransferAllocationRecord>, Error> {
    let rows = sqlx::query(
        "SELECT a.id,a.transfer_line_id,a.lot_id,a.quantity_milli,a.value_minor,\
         a.source_balance_version FROM inventory_transfer_lot_allocations a \
         JOIN inventory_transfer_lines l ON l.id=a.transfer_line_id \
         WHERE l.transfer_id=$1 ORDER BY l.ordinal,a.id",
    )
    .bind(id)
    .fetch_all(&mut *c)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(TransferAllocationRecord {
                id: row.try_get("id")?,
                transfer_line_id: row.try_get("transfer_line_id")?,
                lot_id: row.try_get("lot_id")?,
                quantity_milli: row.try_get("quantity_milli")?,
                value_minor: row.try_get("value_minor")?,
                source_balance_version: row.try_get("source_balance_version")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(Into::into)
}

async fn read_periods(c: &mut PgConnection, company: &str, date: &str) -> Result<Vec<String>, Error> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM finance_periods WHERE company_id=$1 AND state='open' \
         AND start_date<=$2 AND end_date>=$2",
    )
    .bind(company)
    .bind(date)
    .fetch_all(&mut *c)
    .await?;
    Ok(rows)
}

async fn read_transit_config(
    c: &mut PgConnection,
    company: &str,
) -> Result<Option<TransitPostingConfig>, Error> {
    let row = sqlx::query(
        "SELECT s.chart_id,s.inventory_account_id,c.base_currency currency \
         FROM commerce_settings s JOIN finance_charts c \
           ON c.id=s.chart_id AND c.company_id=s.company_id WHERE s.company_id=$1",
    )
    .bind(company)
    .fetch_optional(&mut *c)
    .await?;
    row.map(|row| {
        Ok::<_, sqlx::Error>(TransitPostingConfig {
            chart_id: row.try_get("chart_id")?,
            inventory_account_id: row.try_get("inventory_account_id")?,
            currency: row.try_get("currency")?,
        })
    })
    .transpose()
    .map_err(Into::into)
}

/// Read every current fact a submit/post/reject/cancel transition needs.
pub async fn read_transfer_transition_source(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    id: &str,
    action: TransferAction,
) -> Result<TransferTransitionSource, Error> {
    serializable(c).await?;
    let transfer = read_transfer(c, company, id).await?.ok_or_else(|| {
        BusinessError::new(
            FailureKind::NotFound,
            "TRANSFER_NOT_FOUND",
            "Transfer not found in this workspace.",
        )
    })?;
    if let Some(branch) = branch
        && transfer.source_branch_id != branch
        && transfer.target_branch_id != branch
    {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "TRANSFER_NOT_FOUND",
            "Transfer not found in this workspace.",
        )
        .into());
    }
    let lines = read_lines(c, id).await?;
    let allocations = read_allocations(c, id).await?;
    let source_warehouse = read_warehouse(c, company, &transfer.source_warehouse_id)
        .await?
        .ok_or_else(changed)?;
    let target_warehouse = read_warehouse(c, company, &transfer.target_warehouse_id)
        .await?
        .ok_or_else(changed)?;
    let mut source_stock = Vec::with_capacity(lines.len());
    let mut target_stock = Vec::with_capacity(lines.len());
    let mut target_lots = Vec::with_capacity(lines.len());
    for line in &lines {
        source_stock.push(
            read_stock(c, company, &transfer.source_warehouse_id, &line.product_id)
                .await?
                .ok_or_else(changed)?,
        );
        target_stock.push(
            read_stock(c, company, &transfer.target_warehouse_id, &line.product_id).await?,
        );
        let mut lots = Vec::new();
        for allocation in allocations
            .iter()
            .filter(|allocation| allocation.transfer_line_id == line.id)
        {
            let row = sqlx::query(
                "SELECT version,quantity_milli,value_minor FROM inventory_lot_balances \
                 WHERE lot_id=$1 AND warehouse_id=$2",
            )
            .bind(&allocation.lot_id)
            .bind(&transfer.target_warehouse_id)
            .fetch_optional(&mut *c)
            .await?;
            lots.push(
                row.map(|row| {
                    Ok::<_, sqlx::Error>(LotBalance {
                        version: row.try_get("version")?,
                        quantity_milli: row.try_get("quantity_milli")?,
                        value_minor: row.try_get("value_minor")?,
                    })
                })
                .transpose()?,
            );
        }
        target_lots.push(lots);
    }
    let periods = if action == TransferAction::Post && transfer.delivery_mode != "transit" {
        read_periods(c, company, &transfer.transfer_date).await?
    } else {
        Vec::new()
    };
    let transit = if action == TransferAction::Post && transfer.delivery_mode == "transit" {
        read_transit_config(c, company).await?
    } else {
        None
    };
    Ok(TransferTransitionSource {
        transfer,
        lines,
        allocations,
        source_warehouse,
        target_warehouse,
        source_stock,
        target_stock,
        target_lots,
        periods,
        transit,
    })
}

/// Apply a prepared transfer create/revision. Lines and lot allocations are
/// replaced atomically for a revision; a new record inserts once.
pub async fn apply_transfer_save(
    c: &mut PgConnection,
    plan: &TransferSavePlan,
    key: &str,
    actor: &str,
    now: i64,
    existing: bool,
) -> Result<(), Error> {
    if existing {
        let updated = sqlx::query(
            "UPDATE inventory_transfers SET source_branch_id=$1,target_branch_id=$2,\
             source_warehouse_version=$3,target_warehouse_version=$4,transfer_date=$5,\
             reason=$6,total_value_minor=$7,delivery_mode=$8,arrival_due_date=$9,\
             status='draft',version=version+1,updated_at=$10 WHERE id=$11 AND company_id=$12",
        )
        .bind(&plan.source_branch_id)
        .bind(&plan.target_branch_id)
        .bind(plan.source_warehouse_version)
        .bind(plan.target_warehouse_version)
        .bind(&plan.transfer_date)
        .bind(&plan.reason)
        .bind(plan.total_value_minor)
        .bind(&plan.delivery_mode)
        .bind(&plan.arrival_due_date)
        .bind(now)
        .bind(&plan.id)
        .bind(&plan.company_id)
        .execute(&mut *c)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(changed());
        }
        sqlx::query(
            "DELETE FROM inventory_transfer_lot_allocations WHERE transfer_line_id IN \
             (SELECT id FROM inventory_transfer_lines WHERE transfer_id=$1)",
        )
        .bind(&plan.id)
        .execute(&mut *c)
        .await?;
        sqlx::query("DELETE FROM inventory_transfer_lines WHERE transfer_id=$1")
            .bind(&plan.id)
            .execute(&mut *c)
            .await?;
    } else {
        sqlx::query(
            "INSERT INTO inventory_transfers(id,company_id,source_warehouse_id,target_warehouse_id,\
             source_branch_id,target_branch_id,source_warehouse_version,target_warehouse_version,\
             number,transfer_date,reason,total_value_minor,created_by,prepared_by,delivery_mode,\
             arrival_due_date,created_at,updated_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$17)",
        )
        .bind(&plan.id)
        .bind(&plan.company_id)
        .bind(&plan.source_warehouse_id)
        .bind(&plan.target_warehouse_id)
        .bind(&plan.source_branch_id)
        .bind(&plan.target_branch_id)
        .bind(plan.source_warehouse_version)
        .bind(plan.target_warehouse_version)
        .bind(&plan.number)
        .bind(&plan.transfer_date)
        .bind(&plan.reason)
        .bind(plan.total_value_minor)
        .bind(actor)
        .bind(actor)
        .bind(&plan.delivery_mode)
        .bind(&plan.arrival_due_date)
        .bind(now)
        .execute(&mut *c)
        .await?;
    }
    for line in &plan.lines {
        sqlx::query(
            "INSERT INTO inventory_transfer_lines(id,transfer_id,product_id,product_version,\
             description,unit,quantity_milli,value_minor,source_quantity_milli,\
             source_value_minor,source_version,ordinal) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
        )
        .bind(&line.id)
        .bind(&plan.id)
        .bind(&line.product_id)
        .bind(line.product_version)
        .bind(&line.description)
        .bind(&line.unit)
        .bind(line.quantity_milli)
        .bind(line.value_minor)
        .bind(line.source_quantity_milli)
        .bind(line.source_value_minor)
        .bind(line.source_version)
        .bind(line.ordinal)
        .execute(&mut *c)
        .await?;
        for allocation in &line.allocations {
            sqlx::query(
                "INSERT INTO inventory_transfer_lot_allocations(id,transfer_line_id,lot_id,\
                 quantity_milli,value_minor,source_balance_version) VALUES($1,$2,$3,$4,$5,$6)",
            )
            .bind(&allocation.id)
            .bind(&line.id)
            .bind(&allocation.lot_id)
            .bind(allocation.quantity_milli)
            .bind(allocation.value_minor)
            .bind(allocation.source_balance_version)
            .execute(&mut *c)
            .await?;
        }
    }
    let _ = key;
    Ok(())
}

/// Apply a prepared transition. Every stock/lot write is guarded by the source
/// version captured by the plan; a mismatch aborts the whole unit.
pub async fn apply_transfer_transition(
    c: &mut PgConnection,
    plan: &TransferTransitionPlan,
    key: &str,
    actor: &str,
    now: i64,
) -> Result<(), Error> {
    let updated = sqlx::query(
        "UPDATE inventory_transfers SET status=$1,\
         approved_by=CASE WHEN $2='post' THEN $3 ELSE approved_by END,\
         submitted_at=CASE WHEN $2='submit' THEN $4 ELSE submitted_at END,\
         posted_at=CASE WHEN $2='post' THEN $4 ELSE posted_at END,\
         version=version+1,updated_at=$4 WHERE id=$5 AND version=$6",
    )
    .bind(plan.status)
    .bind(plan.action.as_str())
    .bind(actor)
    .bind(now)
    .bind(&plan.id)
    .bind(plan.expected_version)
    .execute(&mut *c)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(changed());
    }
    if plan.action == TransferAction::Post && !plan.moves.is_empty() {
        for (index, movement) in plan.moves.iter().enumerate() {
            let source = sqlx::query(
                "UPDATE commerce_stock SET quantity_milli=quantity_milli-$1,\
                 value_minor=value_minor-$2,version=version+1 \
                 WHERE warehouse_id=$3 AND product_id=$4 AND version=$5 \
                   AND quantity_milli>=$1 AND value_minor>=$2",
            )
            .bind(movement.quantity_milli)
            .bind(movement.value_minor)
            .bind(&plan.source_warehouse_id)
            .bind(&movement.product_id)
            .bind(movement.source_version)
            .execute(&mut *c)
            .await?;
            if source.rows_affected() != 1 {
                return Err(changed());
            }
            if !movement.target_present {
                sqlx::query(
                    "INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) \
                     VALUES($1,$2,0,0)",
                )
                .bind(&plan.target_warehouse_id)
                .bind(&movement.product_id)
                .execute(&mut *c)
                .await?;
            }
            let target = sqlx::query(
                "UPDATE commerce_stock SET quantity_milli=$1,value_minor=$2,version=version+1 \
                 WHERE warehouse_id=$3 AND product_id=$4",
            )
            .bind(movement.target_quantity_milli)
            .bind(movement.target_value_minor)
            .bind(&plan.target_warehouse_id)
            .bind(&movement.product_id)
            .execute(&mut *c)
            .await?;
            if target.rows_affected() != 1 {
                return Err(changed());
            }
            for (lot_index, lot) in movement.lots.iter().enumerate() {
                let lot_source = sqlx::query(
                    "UPDATE inventory_lot_balances SET quantity_milli=quantity_milli-$1,\
                     value_minor=value_minor-$2,version=version+1 \
                     WHERE lot_id=$3 AND warehouse_id=$4 AND version=$5 \
                       AND quantity_milli>=$1 AND value_minor>=$2",
                )
                .bind(lot.quantity_milli)
                .bind(lot.value_minor)
                .bind(&lot.lot_id)
                .bind(&plan.source_warehouse_id)
                .bind(lot.source_balance_version)
                .execute(&mut *c)
                .await?;
                if lot_source.rows_affected() != 1 {
                    return Err(changed());
                }
                if lot.target_balance.is_none() {
                    sqlx::query(
                        "INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor) \
                         VALUES($1,$2,0,0)",
                    )
                    .bind(&lot.lot_id)
                    .bind(&plan.target_warehouse_id)
                    .execute(&mut *c)
                    .await?;
                }
                sqlx::query(
                    "UPDATE inventory_lot_balances SET quantity_milli=quantity_milli+$1,\
                     value_minor=value_minor+$2,version=version+1 \
                     WHERE lot_id=$3 AND warehouse_id=$4",
                )
                .bind(lot.quantity_milli)
                .bind(lot.value_minor)
                .bind(&lot.lot_id)
                .bind(&plan.target_warehouse_id)
                .execute(&mut *c)
                .await?;
                sqlx::query(
                    "INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,\
                     quantity_milli,value_minor,movement_date,created_by,created_at) \
                     VALUES($1,$2,$3,'transfer',$4,$5,$6,$7,$8,$9)",
                )
                .bind(format!("lot-move-{}-{}-{lot_index}-out", plan.id, index))
                .bind(&lot.lot_id)
                .bind(&plan.source_warehouse_id)
                .bind(format!("{}-{index}-out", plan.id))
                .bind(-lot.quantity_milli)
                .bind(-lot.value_minor)
                .bind(plan.transfer_date.clone())
                .bind(actor)
                .bind(now)
                .execute(&mut *c)
                .await?;
                sqlx::query(
                    "INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,\
                     quantity_milli,value_minor,movement_date,created_by,created_at) \
                     VALUES($1,$2,$3,'transfer',$4,$5,$6,$7,$8,$9)",
                )
                .bind(format!("lot-move-{}-{}-{lot_index}-in", plan.id, index))
                .bind(&lot.lot_id)
                .bind(&plan.target_warehouse_id)
                .bind(format!("{}-{index}-in", plan.id))
                .bind(lot.quantity_milli)
                .bind(lot.value_minor)
                .bind(plan.transfer_date.clone())
                .bind(actor)
                .bind(now)
                .execute(&mut *c)
                .await?;
            }
            for (side, direction) in [("out", -1_i64), ("in", 1_i64)] {
                sqlx::query(
                    "INSERT INTO inventory_transfer_moves(id,transfer_id,warehouse_id,product_id,\
                     quantity_milli,value_minor,movement_date,created_at) \
                     VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
                )
                .bind(format!("{}-{index}-{side}", plan.id))
                .bind(&plan.id)
                .bind(if direction < 0 {
                    plan.source_warehouse_id.clone()
                } else {
                    plan.target_warehouse_id.clone()
                })
                .bind(&movement.product_id)
                .bind(direction * movement.quantity_milli)
                .bind(direction * movement.value_minor)
                .bind(plan.transfer_date.clone())
                .bind(now)
                .execute(&mut *c)
                .await?;
            }
        }
    }
    if let Some(transit) = &plan.transit {
        sqlx::query(
            "INSERT INTO inventory_transit(id,transfer_id,company_id,source_warehouse_id,\
             target_warehouse_id,source_branch_id,target_branch_id,due_date,currency,chart_id,\
             inventory_account_id,created_by,approved_by,created_at,updated_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$14)",
        )
        .bind(&transit.id)
        .bind(&plan.id)
        .bind(&plan.company_id)
        .bind(&plan.source_warehouse_id)
        .bind(&plan.target_warehouse_id)
        .bind(&plan.source_branch_id)
        .bind(&plan.target_branch_id)
        .bind(&transit.due_date)
        .bind(&transit.currency)
        .bind(&transit.chart_id)
        .bind(&transit.inventory_account_id)
        .bind(actor)
        .bind(actor)
        .bind(now)
        .execute(&mut *c)
        .await?;
        for line in &transit.lines {
            sqlx::query(
                "INSERT INTO inventory_transit_lines(id,transit_id,source_line_id,product_id,\
                 quantity_milli,value_minor) VALUES($1,$2,$3,$4,$5,$6)",
            )
            .bind(&line.id)
            .bind(&transit.id)
            .bind(&line.source_line_id)
            .bind(&line.product_id)
            .bind(line.quantity_milli)
            .bind(line.value_minor)
            .execute(&mut *c)
            .await?;
            for lot in &line.lots {
                sqlx::query(
                    "INSERT INTO inventory_transit_lots(id,transit_line_id,lot_id,quantity_milli,\
                     value_minor) VALUES($1,$2,$3,$4,$5)",
                )
                .bind(&lot.id)
                .bind(&line.id)
                .bind(&lot.lot_id)
                .bind(lot.quantity_milli)
                .bind(lot.value_minor)
                .execute(&mut *c)
                .await?;
            }
        }
    }
    if plan.action != TransferAction::Submit {
        sqlx::query(
            "INSERT INTO inventory_transfer_decisions(id,transfer_id,command_id,action,actor_id,\
             reviewed_version,reason,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(format!("{}-{}", plan.id, key))
        .bind(&plan.id)
        .bind(key)
        .bind(plan.action.as_str())
        .bind(actor)
        .bind(plan.expected_version)
        .bind(plan.reason.as_deref().unwrap_or_default())
        .bind(now)
        .execute(&mut *c)
        .await?;
    }
    Ok(())
}
