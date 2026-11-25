//! Pg execution for the native Sales price-list workspace list reads.
//!
//! Starts from the byte-identical rendered TypeScript statements (retained as
//! oracles in `pricing_domain.rs`) and moves the projection into Rust: explicit
//! columns, company-scoped physical joins, and typed `PriceListRow` values.

use super::pricing_application::{PriceListRead, PriceListReadFuture};
use super::pricing_domain::{PriceListQuery, PriceListItemRow, PriceListRow, PriceListWorkspace};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::Row;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "SALES_PRICE_LIST_WORKSPACE_UNAVAILABLE",
        "The native Sales price-list workspace read is unavailable.",
    )
}

const SQL_LISTS: &str = concat!(
    "SELECT l.id,l.company_id,l.name,l.currency,l.customer_id,l.valid_from,l.valid_until,",
    "l.max_auto_discount_bps,l.status,l.version,l.created_by,l.approved_by,l.approval_reason,",
    "l.activated_at,l.created_at,l.updated_at,p.name customer_name",
    " FROM sales_price_lists l LEFT JOIN finance_parties p ON p.id=l.customer_id AND p.company_id=l.company_id",
    " WHERE l.company_id=$1 ORDER BY l.created_at DESC"
);

const SQL_ITEMS: &str = concat!(
    "SELECT i.id,i.price_list_id,i.product_id,i.price_minor,i.tax_bps,i.ordinal,",
    "p.sku,p.name product_name",
    " FROM sales_price_list_items i JOIN sales_price_lists l ON l.id=i.price_list_id",
    " JOIN commerce_products p ON p.id=i.product_id AND p.company_id=l.company_id",
    " WHERE l.company_id=$1 ORDER BY i.price_list_id,i.ordinal"
);

fn list(row: &sqlx::postgres::PgRow) -> Result<PriceListRow, BusinessError> {
    Ok(PriceListRow {
        id: row.try_get("id").map_err(|_| unavailable())?,
        company_id: row.try_get("company_id").map_err(|_| unavailable())?,
        name: row.try_get("name").map_err(|_| unavailable())?,
        currency: row.try_get("currency").map_err(|_| unavailable())?,
        customer_id: row.try_get("customer_id").map_err(|_| unavailable())?,
        valid_from: row.try_get("valid_from").map_err(|_| unavailable())?,
        valid_until: row.try_get("valid_until").map_err(|_| unavailable())?,
        max_auto_discount_bps: row.try_get("max_auto_discount_bps").map_err(|_| unavailable())?,
        status: row.try_get("status").map_err(|_| unavailable())?,
        version: row.try_get("version").map_err(|_| unavailable())?,
        created_by: row.try_get("created_by").map_err(|_| unavailable())?,
        approved_by: row.try_get("approved_by").map_err(|_| unavailable())?,
        approval_reason: row.try_get("approval_reason").map_err(|_| unavailable())?,
        activated_at: row.try_get("activated_at").map_err(|_| unavailable())?,
        created_at: row.try_get("created_at").map_err(|_| unavailable())?,
        updated_at: row.try_get("updated_at").map_err(|_| unavailable())?,
        customer_name: row.try_get("customer_name").map_err(|_| unavailable())?,
    })
}

fn item(row: &sqlx::postgres::PgRow) -> Result<PriceListItemRow, BusinessError> {
    Ok(PriceListItemRow {
        id: row.try_get("id").map_err(|_| unavailable())?,
        price_list_id: row.try_get("price_list_id").map_err(|_| unavailable())?,
        product_id: row.try_get("product_id").map_err(|_| unavailable())?,
        price_minor: row.try_get("price_minor").map_err(|_| unavailable())?,
        tax_bps: row.try_get("tax_bps").map_err(|_| unavailable())?,
        ordinal: row.try_get("ordinal").map_err(|_| unavailable())?,
        sku: row.try_get("sku").map_err(|_| unavailable())?,
        product_name: row.try_get("product_name").map_err(|_| unavailable())?,
    })
}

pub struct PgPriceListRead {
    pool: DatabasePool,
}

impl PgPriceListRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl PriceListRead for PgPriceListRead {
    fn workspace<'a>(&'a self, query: &'a PriceListQuery) -> PriceListReadFuture<'a, PriceListWorkspace> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = query.company.clone();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                Box::pin(async move {
                    let lists = sqlx::query(SQL_LISTS)
                        .bind(company.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .iter()
                        .map(list)
                        .collect::<Result<Vec<_>, _>>()?;
                    let items = sqlx::query(SQL_ITEMS)
                        .bind(company.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .iter()
                        .map(item)
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(PriceListWorkspace { lists, items })
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
    fn sql_is_read_only_company_scoped_and_projection_is_explicit() {
        for sql in [SQL_LISTS, SQL_ITEMS] {
            let upper = sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"));
            let words: Vec<&str> = upper.split(|c: char| !c.is_ascii_alphanumeric()).collect();
            assert!(!words.contains(&"INSERT"));
            assert!(!words.contains(&"UPDATE"));
            assert!(!words.contains(&"DELETE"));
            assert!(upper.contains("COMPANY_ID"));
            assert_eq!(sql.matches("$1").count(), 1);
            assert!(!sql.contains(".*"));
        }
    }
}
