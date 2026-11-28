//! sqlx/Postgres execution for the native Commerce return-line read.
//!
//! This is the only layer that executes SQL. The retired TypeScript statement
//! is kept verbatim as `SQL_RETURN_LINES_TS`; the executable form makes `fl.*`
//! an explicit allow-list and replaces the external purchase-boundary guard
//! with an owner-bound `commerce_orders.company_id=$2` predicate. The payload
//! columns and their order are unchanged.

use super::application::{CommerceReturnLineRead, ReturnLineFuture};
use super::domain::ReturnLine;
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::Row;

/// Retired TypeScript literal, pinned for the parity oracle.
pub const SQL_RETURN_LINES_TS: &str = "SELECT fl.*,ol.product_id,ol.item_type,ol.description,p.sku,COALESCE((SELECT SUM(rl.quantity_milli) FROM commerce_return_lines rl JOIN commerce_returns r ON r.id=rl.return_id WHERE rl.fulfillment_line_id=fl.id AND r.status!='rejected'),0) returned_milli FROM commerce_fulfillment_lines fl JOIN commerce_order_lines ol ON ol.id=fl.order_line_id JOIN commerce_products p ON p.id=ol.product_id JOIN commerce_fulfillments f ON f.id=fl.fulfillment_id WHERE f.order_id=? ORDER BY f.created_at,ol.ordinal";

/// Executable statement: explicit columns, `$1` order id, `$2` Company scope.
const SQL_RETURN_LINES: &str = "SELECT fl.id,fl.fulfillment_id,fl.order_line_id,fl.quantity_milli,fl.value_minor,fl.subtotal_minor,fl.tax_minor,ol.product_id,ol.item_type,ol.description,p.sku,COALESCE((SELECT SUM(rl.quantity_milli) FROM commerce_return_lines rl JOIN commerce_returns r ON r.id=rl.return_id WHERE rl.fulfillment_line_id=fl.id AND r.status!='rejected'),0) returned_milli FROM commerce_fulfillment_lines fl JOIN commerce_order_lines ol ON ol.id=fl.order_line_id JOIN commerce_products p ON p.id=ol.product_id JOIN commerce_fulfillments f ON f.id=fl.fulfillment_id JOIN commerce_orders o ON o.id=f.order_id WHERE f.order_id=$1 AND o.company_id=$2 ORDER BY f.created_at,ol.ordinal";

/// Registered executable statement for the owner route.
pub const fn statement() -> &'static str {
    SQL_RETURN_LINES
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMERCE_RETURN_LINE_READ_UNAVAILABLE",
        "The native Commerce return-line read service is unavailable.",
    )
}

/// Postgres-backed owner reader. Every referenced table is Commerce-owned;
/// this read grants no command, lock or cross-owner authority.
pub struct PgCommerceReturnLineRead {
    pool: DatabasePool,
}

impl PgCommerceReturnLineRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl CommerceReturnLineRead for PgCommerceReturnLineRead {
    fn read<'a>(
        &'a self,
        company: &'a str,
        order_id: &'a str,
    ) -> ReturnLineFuture<'a, Vec<ReturnLine>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let order_id = order_id.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let order_id = order_id.clone();
                Box::pin(async move {
                    let rows = sqlx::query(SQL_RETURN_LINES)
                        .bind(order_id.as_str())
                        .bind(company.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    rows.into_iter()
                        .map(|row| {
                            Ok(ReturnLine {
                                id: row.try_get("id").map_err(|_| unavailable())?,
                                fulfillment_id: row.try_get("fulfillment_id").map_err(|_| unavailable())?,
                                order_line_id: row.try_get("order_line_id").map_err(|_| unavailable())?,
                                quantity_milli: row.try_get("quantity_milli").map_err(|_| unavailable())?,
                                value_minor: row.try_get("value_minor").map_err(|_| unavailable())?,
                                subtotal_minor: row.try_get("subtotal_minor").map_err(|_| unavailable())?,
                                tax_minor: row.try_get("tax_minor").map_err(|_| unavailable())?,
                                product_id: row.try_get("product_id").map_err(|_| unavailable())?,
                                item_type: row.try_get("item_type").map_err(|_| unavailable())?,
                                description: row.try_get("description").map_err(|_| unavailable())?,
                                sku: row.try_get("sku").map_err(|_| unavailable())?,
                                returned_milli: row.try_get("returned_milli").map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()
                        .map_err(PgTransactionError::Business)
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_statement_is_owner_scoped_and_allow_listed() {
        assert!(SQL_RETURN_LINES_TS.contains('?'));
        assert!(!statement().contains('?'));
        assert!(statement().starts_with("SELECT fl.id,fl.fulfillment_id"));
        assert!(statement().contains("f.order_id=$1 AND o.company_id=$2"));
        assert!(statement().contains("returned_milli"));
        assert!(statement().contains("r.status!='rejected'"));
        assert!(!statement().contains("FOR UPDATE"));
        assert!(!statement().contains("LOCK TABLE"));
        assert!(!statement().contains("fl.*"));
    }

    #[test]
    fn payload_columns_match_the_retired_projection() {
        for column in [
            "fl.id",
            "fl.fulfillment_id",
            "fl.order_line_id",
            "fl.quantity_milli",
            "fl.value_minor",
            "fl.subtotal_minor",
            "fl.tax_minor",
            "ol.product_id",
            "ol.item_type",
            "ol.description",
            "p.sku",
            "returned_milli",
        ] {
            assert!(statement().contains(column), "missing {column}");
        }
    }
}
