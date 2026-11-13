//! PostgreSQL generic stock-count source/effect seam.
//!
//! The caller (composition) owns current IAM/Commercial authority, the Workflow
//! ledger, Finance posting journals and Files evidence. These functions read
//! the current Count/warehouse/product/stock/lot facts and apply the planned
//! draft create or revision inside the caller's serializable unit.
use crate::domain::counts::{
    CountLot, CountProduct, CountRecord, CountSavePlan, CountSource, CountStock, CountWarehouse,
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
