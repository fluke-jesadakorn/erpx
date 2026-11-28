//! Postgres execution for the R3 POS Initial Physical Stock detail read.
//!
//! Every statement below is executed here against the owner's registered
//! Holding pool. No statement is prepared or executed in TypeScript for this
//! operation, and no caller-supplied predicate reaches these constants.

use super::application::{DetailFuture, InitialStockDetailRead};
use super::domain::{
    CountRow, DecisionRow, DetailQuery, DetailRow, LineRow, LotRow,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{PgPool, Row};

pub const COUNT_SQL: &str = r#"SELECT c.id,c.number::text,c.status,c.version,c.count_date::text,c.reason,c.warehouse_id::text,COALESCE(w.name,'') warehouse_name,c.created_by,c.prepared_by,c.journal_id::text FROM inventory_counts c LEFT JOIN commerce_warehouses w ON w.id=c.warehouse_id WHERE c.id=$1 AND c.company_id=$2 AND ($3::text IS NULL OR c.branch_id=$3) AND EXISTS (SELECT 1 FROM command_receipts r WHERE r.operation='inventory.count.create' AND r.record_id=c.id AND jsonb_exists(r.result_json,'_posInitialPhysicalStockCount'))"#;

pub const LINE_SQL: &str = r#"SELECT l.product_id::text,p.sku,l.description,l.unit,COALESCE(p.tracking_mode,l.tracking_mode) tracking_mode,l.counted_quantity_milli,l.counted_value_minor,l.unit_cost_minor FROM inventory_count_lines l LEFT JOIN commerce_products p ON p.id=l.product_id WHERE l.count_id=$1 ORDER BY l.ordinal LIMIT 2"#;

pub const LOT_SQL: &str = r#"SELECT lot.lot_code::text,lot.serial_number::text,lot.manufactured_date::text,lot.expiry_date::text,lot.counted_quantity_milli,lot.unit_cost_minor FROM inventory_count_lot_lines lot JOIN inventory_count_lines l ON l.id=lot.count_line_id WHERE l.count_id=$1 ORDER BY lot.id"#;

pub const DECISION_SQL: &str = r#"SELECT d.action,d.reason::text,COALESCE(p.legal_name,d.actor_id::text) actor_name,d.created_at FROM inventory_count_decisions d LEFT JOIN hr_people p ON p.user_id=d.actor_id AND p.company_id=$2 AND p.merged_into_id IS NULL WHERE d.count_id=$1 ORDER BY d.created_at DESC"#;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "POS_INITIAL_STOCK_DETAIL_UNAVAILABLE",
        "The native POS Initial Physical Stock reader is unavailable.",
    )
}

pub struct PgInitialStockDetailRead {
    pool: PgPool,
}

impl PgInitialStockDetailRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool: pool.raw().clone(),
        }
    }
}

impl InitialStockDetailRead for PgInitialStockDetailRead {
    fn read<'a>(&'a self, query: &'a DetailQuery) -> DetailFuture<'a> {
        Box::pin(async move {
            let count = sqlx::query(COUNT_SQL)
                .bind(&query.count_id)
                .bind(&query.company_id)
                .bind(query.branch_id.as_deref())
                .fetch_optional(&self.pool)
                .await
                .map_err(|_| unavailable())?
                .ok_or_else(|| {
                    BusinessError::new(
                        FailureKind::NotFound,
                        "COUNT_NOT_FOUND",
                        "Stock count not found in your workspace.",
                    )
                })?;
            let count = CountRow {
                id: count.try_get("id").map_err(|_| unavailable())?,
                number: count.try_get("number").map_err(|_| unavailable())?,
                status: count.try_get("status").map_err(|_| unavailable())?,
                version: count.try_get("version").map_err(|_| unavailable())?,
                count_date: count.try_get("count_date").map_err(|_| unavailable())?,
                reason: count.try_get("reason").map_err(|_| unavailable())?,
                warehouse_id: count.try_get("warehouse_id").map_err(|_| unavailable())?,
                warehouse_name: count.try_get("warehouse_name").map_err(|_| unavailable())?,
                created_by: count.try_get("created_by").map_err(|_| unavailable())?,
                prepared_by: count.try_get("prepared_by").map_err(|_| unavailable())?,
                journal_id: count.try_get("journal_id").map_err(|_| unavailable())?,
            };
            let lines = sqlx::query(LINE_SQL)
                .bind(&query.count_id)
                .fetch_all(&self.pool)
                .await
                .map_err(|_| unavailable())?;
            if lines.len() > 1 {
                return Err(BusinessError::new(
                    FailureKind::NotFound,
                    "COUNT_NOT_FOUND",
                    "The original Initial Physical Stock Count source is unavailable.",
                ));
            }
            let line = lines
                .into_iter()
                .next()
                .map(|row| -> Result<LineRow, sqlx::Error> {
                    Ok(LineRow {
                        product_id: row.try_get("product_id")?,
                        sku: row.try_get("sku")?,
                        description: row.try_get("description")?,
                        unit: row.try_get("unit")?,
                        tracking_mode: row.try_get("tracking_mode")?,
                        counted_quantity_milli: row.try_get("counted_quantity_milli")?,
                        counted_value_minor: row.try_get("counted_value_minor")?,
                        unit_cost_minor: row.try_get("unit_cost_minor")?,
                    })
                })
                .transpose()
                .map_err(|_| unavailable())?;
            if line.is_none() {
                return Err(BusinessError::new(
                    FailureKind::NotFound,
                    "COUNT_NOT_FOUND",
                    "The original Initial Physical Stock Count source is unavailable.",
                ));
            }
            let lots = sqlx::query(LOT_SQL)
                .bind(&query.count_id)
                .fetch_all(&self.pool)
                .await
                .map_err(|_| unavailable())?
                .into_iter()
                .map(|row| -> Result<LotRow, sqlx::Error> {
                    Ok(LotRow {
                        lot_code: row.try_get("lot_code")?,
                        serial_number: row.try_get("serial_number")?,
                        manufactured_date: row.try_get("manufactured_date")?,
                        expiry_date: row.try_get("expiry_date")?,
                        counted_quantity_milli: row.try_get("counted_quantity_milli")?,
                        unit_cost_minor: row.try_get("unit_cost_minor")?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| unavailable())?;
            let decisions = sqlx::query(DECISION_SQL)
                .bind(&query.count_id)
                .bind(&query.company_id)
                .fetch_all(&self.pool)
                .await
                .map_err(|_| unavailable())?
                .into_iter()
                .map(|row| -> Result<DecisionRow, sqlx::Error> {
                    Ok(DecisionRow {
                        action: row.try_get("action")?,
                        reason: row.try_get("reason")?,
                        actor_name: row.try_get("actor_name")?,
                        created_at: row.try_get("created_at")?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| unavailable())?;
            Ok(Some(DetailRow {
                count,
                line,
                lots,
                decisions,
            }))
        })
    }
}
