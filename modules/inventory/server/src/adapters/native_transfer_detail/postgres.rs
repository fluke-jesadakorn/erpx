//! Postgres execution for the native stock-transfer detail read.
//!
//! `OWNER_SQL` is the byte-identical statement retired from the TypeScript
//! `getTransfer` adapter with the Commerce warehouse/settings and Finance chart
//! projection fragments inlined. Only this file numbers the `?` placeholders,
//! binds values and serializes the projected row to JSON. No predicate is
//! accepted from the caller and no SQL is prepared or executed in TypeScript
//! for this operation.

use crate::adapters::native_transfer_detail::application::{
    InventoryTransferDetailRead, TransferDetailFuture,
};
use crate::adapters::native_transfer_detail::domain::{TransferDetailRequest, VALUE_COUNT};
use erpx_platform_runtime_server::public::http_support::serde_json::Value;
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::postgres::PgArguments;
use sqlx::query::QueryScalar;
use sqlx::AssertSqlSafe;

type RowQuery<'q> = QueryScalar<'q, sqlx::Postgres, String, PgArguments>;

/// The exact retained `getTransfer` statement. `t.*` plus the three aliases is
/// the complete TypeScript `TransferRow` payload; the branch scope binds the
/// Company and branch in that order after the transfer id.
pub const OWNER_SQL: &str = r#"SELECT t.*,s.name source_warehouse_name,d.name target_warehouse_name,chart.base_currency currency FROM inventory_transfers t JOIN commerce_warehouses s ON s.id=t.source_warehouse_id JOIN commerce_warehouses d ON d.id=t.target_warehouse_id JOIN commerce_settings c ON c.company_id=t.company_id JOIN finance_charts chart ON chart.id=c.chart_id AND chart.company_id=c.company_id WHERE t.id=? AND t.company_id=? AND (?::text IS NULL OR t.source_branch_id=? OR t.target_branch_id=?)"#;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "INVENTORY_TRANSFER_DETAIL_UNAVAILABLE",
        "The native Inventory transfer reader is unavailable.",
    )
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "INVENTORY_TRANSFER_DETAIL_BIND_INVALID",
        "The transfer-detail scoped values must be bounded scalars.",
    )
}

/// Number the retained `?` placeholders in bind order.
fn numbered(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len() + 16);
    let mut index = 0usize;
    for ch in sql.chars() {
        if ch == '?' {
            index += 1;
            out.push('$');
            out.push_str(&index.to_string());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Build the owner statement. The inner SQL is unchanged apart from placeholder
/// numbering; the outer wrapper serializes the whole derived row so every
/// TypeScript `TransferRow` field survives.
pub fn statement() -> String {
    format!(
        "SELECT to_jsonb(__transfer_detail)::text FROM ({}) __transfer_detail",
        numbered(OWNER_SQL)
    )
}

fn bind_values<'q>(mut query: RowQuery<'q>, values: Vec<Value>) -> Result<RowQuery<'q>, BusinessError> {
    for value in values {
        query = match value {
            Value::Null => query.bind(Option::<&str>::None),
            Value::Bool(flag) => query.bind(flag),
            Value::Number(number) => match number.as_i64() {
                Some(integer) => query.bind(integer),
                None => return Err(invalid()),
            },
            Value::String(text) => query.bind(text),
            Value::Array(_) | Value::Object(_) => return Err(invalid()),
        };
    }
    Ok(query)
}

/// Pg-backed owner reader. It owns the transaction, the statement, the bind
/// order and the row projection; the router only transports the result.
pub struct PgInventoryTransferDetailRead {
    pool: DatabasePool,
}

impl PgInventoryTransferDetailRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl InventoryTransferDetailRead for PgInventoryTransferDetailRead {
    fn get<'a>(
        &'a self,
        request: &'a TransferDetailRequest,
    ) -> TransferDetailFuture<'a, Option<Value>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let request = request.clone();
        Box::pin(async move {
            let result = unit
                .run_once(TransactionOptions::default(), move |connection| {
                    let request = request.clone();
                    Box::pin(async move {
                        request.validate().map_err(PgTransactionError::Business)?;
                        let values = request.values();
                        if values.len() != VALUE_COUNT {
                            return Err(PgTransactionError::Business(invalid()));
                        }
                        let mut statement =
                            sqlx::query_scalar::<_, String>(AssertSqlSafe(statement()));
                        statement = bind_values(statement, values)
                            .map_err(PgTransactionError::Business)?;
                        let text = statement
                            .fetch_optional(&mut *connection)
                            .await
                            .map_err(|_| PgTransactionError::Business(unavailable()))?;
                        text.map(|text| {
                            serde_json::from_str(&text)
                                .map_err(|_| PgTransactionError::Business(unavailable()))
                        })
                        .transpose()
                    })
                })
                .await;
            match result {
                Ok(row) => Ok(row),
                Err(PgTransactionError::Business(error)) => Err(error),
                Err(PgTransactionError::Persistence(_)) => Err(unavailable()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_platform_runtime_server::public::http_support::serde_json::json;

    #[test]
    fn statement_numbers_all_five_retained_placeholders() {
        let sql = statement();
        assert!(sql.starts_with("SELECT to_jsonb(__transfer_detail)::text FROM (SELECT t.*"));
        assert!(sql.contains("WHERE t.id=$1 AND t.company_id=$2"));
        assert!(sql.contains("($3::text IS NULL OR t.source_branch_id=$4 OR t.target_branch_id=$5)"));
        assert!(!sql.contains('?'));
        assert_eq!(sql.match_indices('$').count(), VALUE_COUNT);
    }

    #[test]
    fn projection_preserves_every_typescript_transfer_row_field_group() {
        for field in [
            "source_warehouse_name",
            "target_warehouse_name",
            "currency",
        ] {
            assert!(OWNER_SQL.contains(field));
        }
        assert!(OWNER_SQL.contains("SELECT t.*,"));
        for fragment in [
            "JOIN commerce_warehouses s ON s.id=t.source_warehouse_id",
            "JOIN commerce_warehouses d ON d.id=t.target_warehouse_id",
            "JOIN commerce_settings c ON c.company_id=t.company_id",
            "JOIN finance_charts chart ON chart.id=c.chart_id AND chart.company_id=c.company_id",
        ] {
            assert!(OWNER_SQL.contains(fragment));
        }
    }

    #[test]
    fn numbered_keeps_bind_order_with_cast_suffixes() {
        assert_eq!(
            numbered("a=? AND b=?::text AND c=?"),
            "a=$1 AND b=$2::text AND c=$3"
        );
    }

    #[test]
    fn bind_rejects_non_scalar_values() {
        let query = sqlx::query_scalar::<_, String>(AssertSqlSafe(
            "SELECT to_jsonb(r)::text FROM (SELECT 1 AS n) r",
        ));
        assert!(bind_values(
            query,
            vec![
                json!("transfer-1"),
                json!("company-1"),
                json!(null),
                json!(null),
                json!(null)
            ]
        )
        .is_ok());
        let query = sqlx::query_scalar::<_, String>(AssertSqlSafe(
            "SELECT to_jsonb(r)::text FROM (SELECT 1 AS n) r",
        ));
        assert!(bind_values(query, vec![json!({"id": 1})]).is_err());
    }
}
