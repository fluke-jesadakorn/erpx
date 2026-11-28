//! sqlx execution for the lane-18 stock-availability read.
//!
//! `INNER_SQL` is the byte-identical statement retired from
//! `adapters/pos-facts.ts` with `?` placeholders mapped to `$1..$3` and the
//! closed owner projection fragments inlined. The outer wrapper only casts
//! numeric results so sqlx can decode them without a decimal dependency; all
//! predicates, aggregation and Company scope stay in the owner statement.

use super::application::{StockAvailabilityFuture, StockAvailabilityRead, unavailable};
use super::domain::{Scope, StockAvailabilityRow};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::BusinessError;
use sqlx::Row;

/// Byte-identical retired TypeScript statement (owner projections expanded).
/// `$2`/`$3` are the two branch binds of the original `all(db, sql, company,
/// branch, branch2)` call; lane 18 binds one resolved scope to both.
pub const INNER_SQL: &str = r#"SELECT s.warehouse_id,s.product_id,s.quantity_milli,COALESCE((SELECT SUM(r.quantity_milli) FROM inventory_active_reservations r WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0) reserved_milli,CASE WHEN EXISTS(SELECT 1 FROM (SELECT "id", "company_id", "branch_id", "warehouse_id", "product_id", "reason", "due_date", "status", "created_by", "last_inspector", "last_result", "released_by", "release_reason", "released_at", "version", "created_at", "updated_at", "cancel_requested_by", "cancel_reason", "cancelled_by", "cancelled_at", "lot_id", "held_quantity_milli", "source_type", "source_id", "disposition_kind", "disposition_reason", "disposition_requested_by", "disposition_count_id", "disposed_by", "disposed_at", "spec_id", "spec_version", "quantity_scope", "disposition_quantity_milli" FROM inventory_quality_holds) h WHERE h.warehouse_id=s.warehouse_id AND h.product_id=s.product_id AND h.status<>'released') THEN 1 ELSE 0 END quality_held FROM commerce_stock s JOIN (SELECT "id","company_id","branch_id","code","name","status","version" FROM commerce_warehouses) w ON w.id=s.warehouse_id WHERE w.company_id=$1 AND ($2::text IS NULL OR w.branch_id=$3)"#;

fn statement() -> String {
    format!(
        "SELECT warehouse_id::text warehouse_id,product_id::text product_id,\
         quantity_milli::bigint quantity_milli,reserved_milli::bigint reserved_milli,\
         quality_held::int2 quality_held FROM ({INNER_SQL}) lane18"
    )
}

fn unavailable_error() -> BusinessError {
    unavailable()
}

pub struct PgStockAvailabilityRead {
    unit: PgUnitOfWork,
}

impl PgStockAvailabilityRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            unit: PgUnitOfWork::new(pool),
        }
    }
}

impl StockAvailabilityRead for PgStockAvailabilityRead {
    fn rows<'a>(
        &'a self,
        scope: Scope<'a>,
    ) -> StockAvailabilityFuture<'a, Vec<StockAvailabilityRow>> {
        let unit = self.unit.clone();
        let company = scope.company.to_owned();
        let branch = scope.branch.map(str::to_owned);
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let branch = branch.clone();
                Box::pin(async move {
                    let rows = sqlx::query(sqlx::AssertSqlSafe(statement()))
                        .bind(company.as_str())
                        .bind(branch.as_deref())
                        .bind(branch.as_deref())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable_error())?;
                    rows.into_iter()
                        .map(|row| {
                            Ok(StockAvailabilityRow {
                                warehouse_id: row
                                    .try_get::<String, _>("warehouse_id")
                                    .map_err(|_| unavailable_error())?,
                                product_id: row
                                    .try_get::<String, _>("product_id")
                                    .map_err(|_| unavailable_error())?,
                                quantity_milli: row
                                    .try_get::<i64, _>("quantity_milli")
                                    .map_err(|_| unavailable_error())?,
                                reserved_milli: row
                                    .try_get::<i64, _>("reserved_milli")
                                    .map_err(|_| unavailable_error())?,
                                quality_held: row
                                    .try_get::<i16, _>("quality_held")
                                    .map_err(|_| unavailable_error())?,
                            })
                        })
                        .collect::<Result<Vec<_>, PgTransactionError<BusinessError>>>()
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable_error(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn hash(value: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    #[test]
    fn rendered_sql_pins_the_retired_statement_oracle() {
        assert_eq!(
            hash(INNER_SQL),
            "fbb602ffe25f6c0aafacba7fb88c8eb90dad2885f3aec75ccd7f43d6b7593b10",
            "update the SQL oracle only with a reviewed byte-identical statement change"
        );
        let sql = statement();
        assert!(sql.contains("w.company_id=$1"));
        assert!(sql.contains("w.branch_id=$3"));
        assert!(!sql.contains('?'));
    }
}
