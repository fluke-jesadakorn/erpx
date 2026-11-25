//! sqlx execution for the native Commerce entrypoint read family.
//!
//! Each statement starts from the retired TypeScript text (only `?` binds are
//! translated to positional `$1`/`$2`). The Company predicate is bound by the
//! owner; no caller-supplied predicate or projection reaches this adapter.

use super::application::{CommerceEntrypointRead, EntrypointReadFuture};
use super::domain::{EntrypointFacts, EntrypointKind};
use erpx_server_runtime::public::postgres::{DatabasePool, PgUnitOfWork, TransactionOptions};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::Row;

const ORDER_KIND_SQL: &str =
    "SELECT kind FROM commerce_orders WHERE id=$1 AND company_id=$2";
const ORDER_FACTS_SQL: &str =
    "SELECT kind,state FROM commerce_orders WHERE id=$1 AND company_id=$2";
const FULFILLMENT_KIND_SQL: &str = "SELECT o.kind FROM commerce_fulfillments f \
     JOIN commerce_orders o ON o.id=f.order_id WHERE f.id=$1 AND o.company_id=$2";
const INVOICE_KIND_SQL: &str = "SELECT o.kind FROM commerce_fulfillments f \
     JOIN commerce_orders o ON o.id=f.order_id WHERE f.invoice_id=$1 AND o.company_id=$2";
const RETURN_KIND_SQL: &str =
    "SELECT o.kind FROM commerce_returns r JOIN commerce_orders o ON o.id=r.order_id \
     WHERE r.id=$1 AND o.company_id=$2";

/// Registered statement for a discriminator. Kept public so the contract test
/// can pin the rendered SQL/result oracle without a live database.
pub const fn statement_for(kind: EntrypointKind) -> &'static str {
    match kind {
        EntrypointKind::OrderKind => ORDER_KIND_SQL,
        EntrypointKind::OrderFacts => ORDER_FACTS_SQL,
        EntrypointKind::FulfillmentKind => FULFILLMENT_KIND_SQL,
        EntrypointKind::InvoiceKind => INVOICE_KIND_SQL,
        EntrypointKind::ReturnKind => RETURN_KIND_SQL,
    }
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMERCE_ENTRYPOINT_READ_UNAVAILABLE",
        "The native Commerce entrypoint read service is unavailable.",
    )
}

/// Postgres-backed owner reader. `commerce_orders`, `commerce_fulfillments`,
/// `commerce_returns` and their identity joins are Commerce-owned tables; this
/// read grants no module, permission or cross-owner authority.
pub struct PgCommerceEntrypointRead {
    pool: DatabasePool,
}

impl PgCommerceEntrypointRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl CommerceEntrypointRead for PgCommerceEntrypointRead {
    fn read<'a>(
        &'a self,
        company: &'a str,
        kind: EntrypointKind,
        id: &'a str,
    ) -> EntrypointReadFuture<'a, Option<EntrypointFacts>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let id = id.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let id = id.clone();
                Box::pin(async move {
                    let sql = statement_for(kind);
                    let row = sqlx::query(sql)
                        .bind(id.as_str())
                        .bind(company.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    row.map(|row| {
                        let value = row.try_get::<String, _>("kind").map_err(|_| unavailable())?;
                        let state = if kind.carries_state() {
                            Some(row.try_get::<String, _>("state").map_err(|_| unavailable())?)
                        } else {
                            None
                        };
                        Ok(EntrypointFacts { kind: value, state })
                    })
                    .transpose()
                })
            })
            .await
            .map_err(|error| match error {
                erpx_server_runtime::public::postgres::PgTransactionError::Business(error) => error,
                erpx_server_runtime::public::postgres::PgTransactionError::Persistence(_) => {
                    unavailable()
                }
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn sha(value: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn rendered_sql_oracle_is_stable() {
        let sql = [
            statement_for(EntrypointKind::OrderKind),
            statement_for(EntrypointKind::OrderFacts),
            statement_for(EntrypointKind::FulfillmentKind),
            statement_for(EntrypointKind::InvoiceKind),
            statement_for(EntrypointKind::ReturnKind),
        ];
        let joined = sql.join("\n");
        assert_eq!(
            sha(&joined),
            "b1dd9830f8cf1f007e2119df70f21ed6f53c7baa61d1b6ff15ad362376ee39b1"
        );
        for statement in sql {
            assert!(statement.starts_with("SELECT "));
            assert!(statement.contains("company_id=$2"));
            assert!(!statement.contains('?'));
            assert!(!statement.contains("FOR UPDATE"));
        }
    }

    #[test]
    fn state_column_is_selected_only_for_order_facts() {
        assert!(statement_for(EntrypointKind::OrderFacts).contains("kind,state"));
        for kind in [
            EntrypointKind::OrderKind,
            EntrypointKind::FulfillmentKind,
            EntrypointKind::InvoiceKind,
            EntrypointKind::ReturnKind,
        ] {
            assert!(!statement_for(kind).contains("state"));
        }
    }
}
