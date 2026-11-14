//! PostgreSQL adapter for native Manufacturing production-lot posting.
//!
//! SQL is Manufacturing-owned (`manufacturing_material_lot_issues`,
//! `manufacturing_output_lots`, `manufacturing_lot_genealogy`) plus the shared
//! Inventory lot balance, lot status and lot movement rows the retained writer
//! produced. Inventory lot reads and writes are issued here on behalf of the
//! Manufacturing order command; a public Inventory consumption/lot port remains
//! the preferred future dependency. The caller owns one serializable Holding
//! transaction, the current authority and the command receipt; no commit,
//! receipt or audit write is performed here.
use crate::application::lots::{
    LotFuture, LotPosting, ManufacturingLotRepository,
};
use crate::domain::lots::{
    MaterialIssueRow, MaterialLotFacts, MaterialLotPlan, OutputLotPlan, lot_unavailable,
    material_ids,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

/// Quality-owned hold relation used by the retained Inventory lot guard.
const QUALITY_HOLD: &str = "FROM (SELECT warehouse_id,product_id,lot_id,status FROM inventory_quality_holds) h";

pub struct PgManufacturingLots<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgManufacturingLots<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("MANUFACTURING_LOT_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(
                PersistenceError::new("MANUFACTURING_LOT_SERIALIZABLE_REQUIRED").into(),
            );
        }
        Ok(())
    }

}


impl ManufacturingLotRepository for PgManufacturingLots<'_> {
    type Error = Error;

    fn material_lot<'a>(
        &'a mut self,
        company_id: &'a str,
        product_id: &'a str,
        warehouse_id: &'a str,
        lot_id: &'a str,
    ) -> LotFuture<'a, Option<MaterialLotFacts>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(AssertSqlSafe(format!(
                "SELECT l.status,l.expiry_date,l.tracking_mode,b.quantity_milli,b.value_minor,\
                 b.version balance_version,\
                 EXISTS(SELECT 1 {QUALITY_HOLD} WHERE h.warehouse_id=b.warehouse_id \
                   AND h.lot_id=l.id \
                   AND h.status NOT IN ('released','cancelled','disposed')) held \
                 FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id \
                 WHERE l.id=$1 AND l.company_id=$2 AND l.product_id=$3 AND b.warehouse_id=$4 \
                 FOR UPDATE OF b"
            )))
            .bind(lot_id)
            .bind(company_id)
            .bind(product_id)
            .bind(warehouse_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(MaterialLotFacts {
                    status: row.try_get("status")?,
                    held: row.try_get("held")?,
                    balance_version: row.try_get("balance_version")?,
                    quantity_milli: row.try_get("quantity_milli")?,
                    value_minor: row.try_get("value_minor")?,
                    expiry_date: row.try_get("expiry_date")?,
                })
            })
            .transpose()
        })
    }

    fn material_lot_issues<'a>(
        &'a mut self,
        order_id: &'a str,
    ) -> LotFuture<'a, Vec<MaterialIssueRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT id,quantity_milli FROM manufacturing_material_lot_issues \
                 WHERE order_id=$1 ORDER BY id",
            )
            .bind(order_id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok::<_, Error>(MaterialIssueRow {
                        id: row.try_get("id")?,
                        quantity_milli: row.try_get("quantity_milli")?,
                    })
                })
                .collect()
        })
    }

    fn commit_material_lots<'a>(
        &'a mut self,
        plan: &'a MaterialLotPlan,
        posting: &'a LotPosting<'a>,
    ) -> LotFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            // Validate the approved version/quantity/value under the held lock
            // before the first Inventory write.
            for issue in &plan.issues {
                let available: bool = sqlx::query_scalar(AssertSqlSafe(format!(
                    "SELECT EXISTS(SELECT 1 FROM inventory_lot_balances b \
                     JOIN inventory_lots l ON l.id=b.lot_id \
                     WHERE b.lot_id=$1 AND b.warehouse_id=$2 AND b.quantity_milli>=$3 \
                     AND b.value_minor>=$4 AND l.status='active' \
                     AND (l.expiry_date IS NULL OR l.expiry_date>=$5) \
                     AND NOT EXISTS(SELECT 1 {QUALITY_HOLD} WHERE h.warehouse_id=b.warehouse_id \
                       AND (h.lot_id=l.id OR (h.lot_id IS NULL AND h.product_id=l.product_id)) \
                       AND h.status NOT IN ('released','cancelled','disposed')))"
                )))
                .bind(&issue.lot_id)
                .bind(posting.warehouse_id)
                .bind(issue.quantity_milli)
                .bind(issue.value_minor)
                .bind(posting.entry_date)
                .fetch_one(&mut *self.connection)
                .await?;
                if !available {
                    return Err(lot_unavailable().into());
                }
                let changed = sqlx::query(
                    "UPDATE inventory_lot_balances SET quantity_milli=quantity_milli-$1,\
                     value_minor=value_minor-$2,version=version+1 \
                     WHERE lot_id=$3 AND warehouse_id=$4 AND quantity_milli>=$1 \
                     AND value_minor>=$2 AND version=$5",
                )
                .bind(issue.quantity_milli)
                .bind(issue.value_minor)
                .bind(&issue.lot_id)
                .bind(posting.warehouse_id)
                .bind(issue.balance_version)
                .execute(&mut *self.connection)
                .await?
                .rows_affected();
                if changed != 1 {
                    return Err(lot_unavailable().into());
                }
                sqlx::query(
                    "UPDATE inventory_lots SET status='depleted' WHERE id=$1 \
                     AND NOT EXISTS(SELECT 1 FROM inventory_lot_balances \
                       WHERE lot_id=$1 AND quantity_milli>0)",
                )
                .bind(&issue.lot_id)
                .execute(&mut *self.connection)
                .await?;
                let ids = material_ids(
                    posting.key,
                    posting.order_id,
                    posting.order_version,
                    posting.product_id,
                    issue.ordinal,
                );
                sqlx::query(
                    "INSERT INTO manufacturing_material_lot_issues(id,order_id,\
                     source_order_version,product_id,lot_id,quantity_milli,value_minor,\
                     created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
                )
                .bind(&ids.issue_id)
                .bind(posting.order_id)
                .bind(posting.order_version)
                .bind(posting.product_id)
                .bind(&issue.lot_id)
                .bind(issue.quantity_milli)
                .bind(issue.value_minor)
                .bind(posting.user_id)
                .bind(posting.now)
                .execute(&mut *self.connection)
                .await?;
                sqlx::query(
                    "INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,\
                     source_id,quantity_milli,value_minor,movement_date,created_by,created_at) \
                     VALUES($1,$2,$3,'manufacturing_issue',$4,$5,$6,$7,$8,$9)",
                )
                .bind(&ids.move_id)
                .bind(&issue.lot_id)
                .bind(posting.warehouse_id)
                .bind(&ids.move_source_id)
                .bind(-issue.quantity_milli)
                .bind(-issue.value_minor)
                .bind(posting.entry_date)
                .bind(posting.user_id)
                .bind(posting.now)
                .execute(&mut *self.connection)
                .await?;
            }
            Ok(())
        })
    }

    fn commit_output_lots<'a>(
        &'a mut self,
        plan: &'a OutputLotPlan,
        posting: &'a LotPosting<'a>,
    ) -> LotFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            for lot in &plan.lots {
                sqlx::query(
                    "INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,\
                     lot_code,serial_number,manufactured_date,expiry_date,created_by,created_at) \
                     VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
                )
                .bind(&lot.lot_id)
                .bind(posting.company_id)
                .bind(posting.product_id)
                .bind(if lot.serial_number.is_some() { "serial" } else { "lot" })
                .bind(&lot.lot_code)
                .bind(&lot.serial_number)
                .bind(posting.entry_date)
                .bind(&lot.expiry_date)
                .bind(posting.user_id)
                .bind(posting.now)
                .execute(&mut *self.connection)
                .await?;
                sqlx::query(
                    "INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,\
                     value_minor) VALUES($1,$2,$3,$4)",
                )
                .bind(&lot.lot_id)
                .bind(posting.warehouse_id)
                .bind(lot.quantity_milli)
                .bind(lot.value_minor)
                .execute(&mut *self.connection)
                .await?;
                sqlx::query(
                    "INSERT INTO manufacturing_output_lots(id,order_id,source_order_version,\
                     lot_id,quantity_milli,value_minor,created_by,created_at) \
                     VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
                )
                .bind(&lot.output_id)
                .bind(posting.order_id)
                .bind(posting.order_version)
                .bind(&lot.lot_id)
                .bind(lot.quantity_milli)
                .bind(lot.value_minor)
                .bind(posting.user_id)
                .bind(posting.now)
                .execute(&mut *self.connection)
                .await?;
                sqlx::query(
                    "INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,\
                     source_id,quantity_milli,value_minor,movement_date,created_by,created_at) \
                     VALUES($1,$2,$3,'manufacturing_output',$4,$5,$6,$7,$8,$9)",
                )
                .bind(&lot.move_id)
                .bind(&lot.lot_id)
                .bind(posting.warehouse_id)
                .bind(format!("{}:v{}", posting.order_id, posting.order_version))
                .bind(lot.quantity_milli)
                .bind(lot.value_minor)
                .bind(posting.entry_date)
                .bind(posting.user_id)
                .bind(posting.now)
                .execute(&mut *self.connection)
                .await?;
            }
            for row in &plan.genealogy {
                sqlx::query(
                    "INSERT INTO manufacturing_lot_genealogy(id,order_id,input_issue_id,\
                     output_lot_id,input_quantity_milli,source_order_version,created_at) \
                     VALUES($1,$2,$3,$4,$5,$6,$7)",
                )
                .bind(&row.id)
                .bind(posting.order_id)
                .bind(&row.issue_id)
                .bind(&row.output_id)
                .bind(row.input_quantity_milli)
                .bind(posting.order_version)
                .bind(posting.now)
                .execute(&mut *self.connection)
                .await?;
            }
            Ok(())
        })
    }
}
