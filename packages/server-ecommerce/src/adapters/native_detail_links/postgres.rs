//! Native Ecommerce checkout detail-link postgres adapter.
//!
//! Both retired statements are rebuilt byte-for-byte with one positional bind
//! and executed on the owner pool. `to_jsonb(source)` keeps every selected
//! column, including the Commerce alias columns on return links. No statement
//! or relation name is accepted from the wire.

use crate::adapters::native_detail_links::application::{DetailLinkFuture, DetailLinkRead};
use crate::adapters::native_detail_links::domain::{DetailLinks, LinkRow, unavailable, validate_checkout};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::BusinessError;
use serde_json::Value;
use sqlx::{QueryBuilder, Row};

/// Retired TS literal: `ecommerceOrderedList('*', 'ecommerce_fulfillment_links', 'checkout_id=?')`.
pub const FULFILLMENT_LINKS_SQL: &str =
    "SELECT * FROM ecommerce_fulfillment_links WHERE checkout_id=?";

/// Closed Commerce-owned return projection, inlined byte-for-byte at the join.
pub const COMMERCE_RETURNS_SQL: &str = "SELECT \"id\",\"company_id\",\"branch_id\",\"order_id\",\"fulfillment_id\",\"invoice_id\",\"kind\",\"number\",\"return_date\",\"reason\",\"status\",\"subtotal_minor\",\"tax_minor\",\"total_minor\",\"stock_journal_id\",\"credit_journal_id\",\"version\",\"requested_by\",\"approved_by\",\"created_at\",\"updated_at\",\"credited_minor\",\"refund_minor\",\"base_credit_minor\",\"base_credited_minor\",\"base_refund_minor\",\"fx_journal_id\",\"goods_fx_journal_id\" FROM commerce_returns";

/// Retired TS literal: `ecommerceOrderedList('l.*,r.status current_return_status,r.version current_return_version', ...)`.
pub const RETURN_LINKS_SQL: &str = "SELECT l.*,r.status current_return_status,r.version current_return_version FROM ecommerce_return_links l JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"order_id\",\"fulfillment_id\",\"invoice_id\",\"kind\",\"number\",\"return_date\",\"reason\",\"status\",\"subtotal_minor\",\"tax_minor\",\"total_minor\",\"stock_journal_id\",\"credit_journal_id\",\"version\",\"requested_by\",\"approved_by\",\"created_at\",\"updated_at\",\"credited_minor\",\"refund_minor\",\"base_credit_minor\",\"base_credited_minor\",\"base_refund_minor\",\"fx_journal_id\",\"goods_fx_journal_id\" FROM commerce_returns) r ON r.id=l.commerce_return_id WHERE l.checkout_id=?";

pub struct PgDetailLinkRead {
    pool: DatabasePool,
}

impl PgDetailLinkRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

/// Execute one retired owner statement with its single checkout bind and keep
/// the full row as JSON. The statement text is never taken from the request.
async fn fetch_rows(
    pool: DatabasePool,
    statement: &str,
    checkout_id: &str,
) -> Result<Vec<LinkRow>, BusinessError> {
    let mut parts = statement.split('?');
    let mut query = QueryBuilder::new("SELECT to_jsonb(source) FROM (");
    query.push(parts.next().unwrap_or(""));
    for tail in parts {
        query.push_bind(checkout_id);
        query.push(tail);
    }
    query.push(") source");
    let rows = query
        .build()
        .fetch_all(pool.raw())
        .await
        .map_err(|_| unavailable())?;
    rows.into_iter()
        .map(|row| {
            row.try_get::<Value, _>(0)
                .map(LinkRow)
                .map_err(|_| unavailable())
        })
        .collect::<Result<Vec<_>, _>>()
}

impl DetailLinkRead for PgDetailLinkRead {
    fn links<'a>(&'a self, checkout_id: &'a str) -> DetailLinkFuture<'a, DetailLinks> {
        let pool = self.pool.clone();
        let checkout = checkout_id.to_owned();
        Box::pin(async move {
            validate_checkout(&checkout)?;
            let fulfillment = fetch_rows(pool.clone(), FULFILLMENT_LINKS_SQL, &checkout).await?;
            let returns = fetch_rows(pool, RETURN_LINKS_SQL, &checkout).await?;
            Ok(DetailLinks { fulfillment, returns })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retired_statements_are_byte_identical_and_single_bind() {
        assert_eq!(
            FULFILLMENT_LINKS_SQL,
            "SELECT * FROM ecommerce_fulfillment_links WHERE checkout_id=?"
        );
        assert_eq!(
            RETURN_LINKS_SQL,
            format!("SELECT l.*,r.status current_return_status,r.version current_return_version FROM ecommerce_return_links l JOIN ({COMMERCE_RETURNS_SQL}) r ON r.id=l.commerce_return_id WHERE l.checkout_id=?")
        );
        for sql in [FULFILLMENT_LINKS_SQL, RETURN_LINKS_SQL] {
            assert_eq!(sql.matches('?').count(), 1);
            assert!(sql.starts_with("SELECT "));
            assert!(!sql.contains(';'));
            let upper = sql.to_uppercase();
            assert!(!upper.contains("INSERT")
                && !upper.contains("UPDATE ")
                && !upper.contains("DELETE")
                && !upper.contains("FOR UPDATE")
                && !upper.contains("FOR SHARE")
                && !upper.contains("LOCK TABLE"));
        }
    }

    #[test]
    fn sqlx_builder_keeps_the_checkout_bind_in_order() {
        let mut parts = RETURN_LINKS_SQL.split('?');
        let mut query = QueryBuilder::new("SELECT to_jsonb(source) FROM (");
        query.push(parts.next().unwrap());
        for tail in parts {
            query.push_bind("checkout-1");
            query.push(tail);
        }
        query.push(") source");
        let rendered = query.into_sql();
        let rendered = rendered.as_str();
        assert_eq!(rendered.matches('$').count(), 1);
        assert!(rendered.contains("$1"));
        assert!(rendered.contains("current_return_version"));
        assert!(!rendered.contains('?'));
    }
}
