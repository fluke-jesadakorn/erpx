//! Postgres execution for the lane 17 landed-cost read family.
//!
//! The statements below are the byte-identical owner SQL retained from
//! `adapters/inventory-landed-costs.ts` (with the owner projection fragments
//! inlined). Only this file turns `?` into sqlx placeholders, wraps each result
//! in a `jsonb` projection and binds values. No predicate is accepted from a
//! caller and no transaction is opened by the HTTP layer.

use crate::adapters::native_lane_17::application::{InventoryLane17Read, LaneReadFuture};
use crate::adapters::native_lane_17::domain::{LaneReadKind, LaneReadRequest};
use erpx_server_runtime::public::http_support::serde_json::Value;
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::postgres::PgArguments;
use sqlx::query::QueryScalar;
use sqlx::AssertSqlSafe;

type RowQuery<'q> = QueryScalar<'q, sqlx::Postgres, String, PgArguments>;

/// `scopedCost` — one cost in the caller's company/branch.
const SQL_SCOPED: &str = r#"SELECT c.*,f.number fulfillment_number,o.id order_id,o.number order_number,o.warehouse_id,w.name warehouse_name FROM inventory_landed_costs c JOIN commerce_fulfillments f ON f.id=c.fulfillment_id JOIN commerce_orders o ON o.id=f.order_id JOIN commerce_warehouses w ON w.id=o.warehouse_id WHERE c.id=? AND c.company_id=? AND (?::text IS NULL OR c.branch_id=?)"#;

/// `landedCostsForOrder` — costs on a purchase order in the caller's scope.
const SQL_FOR_ORDER: &str = r#"SELECT c.* FROM inventory_landed_costs c JOIN commerce_fulfillments f ON f.id=c.fulfillment_id JOIN commerce_orders o ON o.id=f.order_id WHERE o.id=? AND o.company_id=? AND (?::text IS NULL OR o.branch_id=?) ORDER BY c.created_at DESC"#;

/// `landedCostOptions` moved-stock blocker probe.
const SQL_MOVED_STOCK: &str = r#"SELECT 1 FROM inventory_ledger m JOIN commerce_fulfillment_lines fl ON fl.fulfillment_id=? JOIN commerce_order_lines ol ON ol.id=fl.order_line_id AND ol.product_id=m.product_id WHERE m.warehouse_id=? AND m.quantity_milli<0 AND m.created_at>? LIMIT 1"#;

/// `landedCostWork` — open cost rows with the retained purchase-source witness.
const SQL_WORK_ROWS: &str = r#"SELECT c.*,f.number fulfillment_number,o.number order_number FROM inventory_landed_costs c JOIN commerce_fulfillments f ON f.id=c.fulfillment_id JOIN commerce_orders o ON o.id=f.order_id WHERE c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND c.status IN ('submitted','approved','rejected','cancelled','posted') AND EXISTS(SELECT 1 FROM (SELECT c.id,c.company_id,c.branch_id,receipt.warehouse_id,c.fulfillment_id,receipt.order_id,receipt.warehouse_active,
 jsonb_build_object('cost',to_jsonb(c)-'version'-'updated_at'-'status'-'approved_by'-'review_reason'-'journal_id','receipt',receipt.witness,'lines',COALESCE((SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM inventory_landed_cost_lines line WHERE line.landed_cost_id=c.id),'[]'::jsonb),'lots',COALESCE((SELECT jsonb_agg(to_jsonb(lot) ORDER BY lot.id) FROM inventory_landed_cost_lots lot JOIN inventory_landed_cost_lines line ON line.id=lot.landed_cost_line_id WHERE line.landed_cost_id=c.id),'[]'::jsonb)) witness
 FROM inventory_landed_costs c JOIN (SELECT f.id,o.id order_id,o.company_id,o.branch_id,o.warehouse_id,f.journal_id,w.status='active' warehouse_active,
 jsonb_build_object('receipt',jsonb_build_array(f.id,f.order_id,f.fulfillment_date,f.journal_id,f.created_at),'order',jsonb_build_array(o.id,o.company_id,o.branch_id,o.warehouse_id,o.kind,o.currency),'lines',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',fl.id,'orderLine',fl.order_line_id,'product',ol.product_id,'kind',ol.item_type,'quantity',fl.quantity_milli,'value',fl.value_minor,'lots',COALESCE((SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM inventory_fulfillment_lot_allocations a WHERE a.fulfillment_line_id=fl.id),'[]'::jsonb)) ORDER BY fl.id) FROM commerce_fulfillment_lines fl JOIN commerce_order_lines ol ON ol.id=fl.order_line_id AND ol.order_id=o.id WHERE fl.fulfillment_id=f.id),'[]'::jsonb)) witness
 FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id AND o.kind='purchase' JOIN commerce_warehouses w ON w.id=o.warehouse_id AND w.company_id=o.company_id AND w.branch_id=o.branch_id
 WHERE EXISTS(SELECT 1 FROM commerce_fulfillment_lines fl JOIN commerce_order_lines ol ON ol.id=fl.order_line_id AND ol.order_id=o.id AND ol.item_type<>'service' WHERE fl.fulfillment_id=f.id)) receipt ON receipt.id=c.fulfillment_id AND receipt.company_id=c.company_id AND receipt.branch_id=c.branch_id) actual_cost WHERE actual_cost.id=c.id AND actual_cost.company_id=c.company_id) ORDER BY c.updated_at DESC"#;

/// Company witness for the order-scoped moved-stock probe: the caller already
/// checked the receipt in its Holding unit, and the owner repeats that company
/// isolation instead of trusting an absent caller company value.
const COMPANY_WITNESS: &str =
    " AND EXISTS(SELECT 1 FROM commerce_fulfillments gf JOIN commerce_orders go ON go.id=gf.order_id WHERE gf.id=$1 AND go.company_id=$4)";

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "INVENTORY_LANE17_READ_UNAVAILABLE",
        "The native Inventory landed-cost reader is unavailable.",
    )
}

/// Byte-identical TS statement for one admitted kind.
pub fn owner_sql(kind: LaneReadKind) -> &'static str {
    match kind {
        LaneReadKind::LandedCostScoped => SQL_SCOPED,
        LaneReadKind::LandedCostsForOrder => SQL_FOR_ORDER,
        LaneReadKind::LandedCostMovedStock => SQL_MOVED_STOCK,
        LaneReadKind::LandedCostWorkRows => SQL_WORK_ROWS,
    }
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
/// numbering; the outer wrapper only serializes the projected row to JSON.
pub fn statement(kind: LaneReadKind) -> String {
    let mut sql = numbered(owner_sql(kind));
    if kind.needs_company_witness() {
        sql.push_str(COMPANY_WITNESS);
    }
    format!("SELECT to_jsonb(__lane17)::text FROM ({sql}) __lane17")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "INVENTORY_LANE17_BIND_INVALID",
        "The landed-cost read values must be bounded scalars.",
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
pub struct PgInventoryLane17Read {
    pool: DatabasePool,
}

impl PgInventoryLane17Read {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl InventoryLane17Read for PgInventoryLane17Read {
    fn read<'a>(&'a self, request: &'a LaneReadRequest) -> LaneReadFuture<'a, Vec<Value>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let request = request.clone();
        Box::pin(async move {
            let result = unit
                .run_once(TransactionOptions::default(), move |connection| {
                    let request = request.clone();
                    Box::pin(async move {
                        request.validate()?;
                        let mut values = request.values.clone();
                        if request.kind.needs_company_witness() {
                            values.push(Value::String(request.company.clone()));
                        }
                        let mut statement =
                            sqlx::query_scalar::<_, String>(AssertSqlSafe(statement(request.kind)));
                        statement = bind_values(statement, values)?;
                        let texts = statement
                            .fetch_all(&mut *connection)
                            .await
                            .map_err(|_| unavailable())?;
                        let rows = texts
                            .into_iter()
                            .map(|text| serde_json::from_str(&text).map_err(|_| unavailable()))
                            .collect::<Result<Vec<Value>, BusinessError>>()?;
                        Ok(rows)
                    })
                })
                .await;
            match result {
                Ok(rows) => Ok(rows),
                Err(PgTransactionError::Business(error)) => Err(error),
                Err(PgTransactionError::Persistence(_)) => Err(unavailable()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_runtime::public::http_support::serde_json::json;
    use sha2::{Digest, Sha256};

    fn hash(value: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn owner_sql_is_the_byte_identical_retained_statement() {
        // Oracle: SHA-256 of the exact SQL text composed by the TS adapter.
        assert_eq!(
            hash(owner_sql(LaneReadKind::LandedCostScoped)),
            "070942f7fbb48d6ae399623d3dd8c1d515b7505b8fce0dd426a15304a183621b"
        );
        assert_eq!(
            hash(owner_sql(LaneReadKind::LandedCostsForOrder)),
            "c1ba5b9e9acba1646f1d21f07eef84b67a315f5f5696586e9f22915aaae735f9"
        );
        assert_eq!(
            hash(owner_sql(LaneReadKind::LandedCostMovedStock)),
            "9abe06c83423ee185c42b6497e78c2fdb86b927bd7e1fc2239a158049dabcea4"
        );
        assert_eq!(
            hash(owner_sql(LaneReadKind::LandedCostWorkRows)),
            "41fb758c4c568aca7436e25c239e2d8c60b7fc999150af07479d5691858c448a"
        );
    }

    #[test]
    fn placeholder_numbering_preserves_bind_order() {
        let sql = numbered("a=? AND b=?::text AND c=?");
        assert_eq!(sql, "a=$1 AND b=$2::text AND c=$3");
        assert_eq!(sql.match_indices('$').count(), 3);
    }

    #[test]
    fn company_witness_is_only_added_to_the_order_scoped_probe() {
        let moved = statement(LaneReadKind::LandedCostMovedStock);
        assert!(moved.contains("go.company_id=$4"));
        assert!(moved.contains("SELECT to_jsonb(__lane17)::text FROM"));
        for kind in [
            LaneReadKind::LandedCostScoped,
            LaneReadKind::LandedCostsForOrder,
            LaneReadKind::LandedCostWorkRows,
        ] {
            assert!(!statement(kind).contains("go.company_id"));
        }
    }

    #[test]
    fn bind_rejects_non_scalar_values() {
        let query = sqlx::query_scalar::<_, String>(AssertSqlSafe(
            "SELECT to_jsonb(r)::text FROM (SELECT 1 AS n) r",
        ));
        assert!(bind_values(query, vec![json!("x"), json!(1), json!(null)]).is_ok());
        let query = sqlx::query_scalar::<_, String>(AssertSqlSafe(
            "SELECT to_jsonb(r)::text FROM (SELECT 1 AS n) r",
        ));
        assert!(bind_values(query, vec![json!({"n": 1})]).is_err());
    }
}
