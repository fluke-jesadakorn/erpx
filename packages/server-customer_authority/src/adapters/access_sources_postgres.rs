//! PostgreSQL execution of the customer-access sources read.
//!
//! This is the only layer that prepares or executes SQL. The statement is the
//! executable `$n` form of the owner's `customer_authority_rd_access_sources`
//! read: every positional `?` became `$n`, the closed owner projections are
//! inlined verbatim, and every row is mapped into the owner domain type before
//! the application layer shapes the wire payload. No TypeScript prepares or
//! executes this statement and no plan-only reply is returned.
use crate::application::access_sources::{AccessSourcesSource, SourcesFuture};
use crate::domain::access::AccessSourceRow;
use crate::domain::access_sources::AccessSourcesQuery;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};

/// Exact executable successor of the retired TypeScript `access.ts` sources
/// read. The five positional binds mirror `args(a)`: company, branch twice and
/// department twice. Both closed owner projections are inlined byte-for-byte.
const SOURCES: &str = concat!(
    "SELECT o.id,o.number,o.version,p.name FROM (",
    "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",",
    "\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",",
    "\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",",
    "\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",",
    "\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders",
    ") o JOIN (",
    "SELECT \"id\",\"company_id\",\"kind\",\"code\",\"name\",\"currency\",\"tax_registration\",",
    "\"payment_terms_days\",\"credit_limit_minor\",\"status\",\"version\",\"created_at\",",
    "\"updated_at\",\"person_id\",\"notification_email\",\"notification_consent\",",
    "\"merged_into_id\",\"merged_at\",\"merged_by\",\"phone\",\"address_json\" FROM finance_parties",
    ") p ON p.id=o.party_id AND p.company_id=o.company_id ",
    "WHERE o.company_id=$1 AND ($2::text IS NULL OR o.branch_id=$3) ",
    "AND ($4::text IS NULL OR o.department_id=$5) ",
    "AND o.kind IN ('sales','quote') ",
    "AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled') ",
    "ORDER BY o.updated_at DESC LIMIT 250"
);

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CUSTOMER_ACCESS_SOURCES_UNAVAILABLE",
        "The approved customer document list is not available.",
    )
}

/// Caller-owned PostgreSQL source. Composition supplies the unit of work; this
/// adapter never opens a connection, commits a transaction or accepts SQL.
pub struct PgAccessSources<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgAccessSources<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    fn ready(&self) -> Result<(), BusinessError> {
        if !self.connection.is_in_transaction() {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "CUSTOMER_ACCESS_SOURCES_TRANSACTION_REQUIRED",
                "The customer-access source read requires the caller's transaction.",
            ));
        }
        Ok(())
    }

    /// Execute the SQL on the caller's open unit and map every row into the
    /// owner domain type. The complete payload is returned to the application
    /// layer for validation.
    pub async fn read(
        &mut self,
        query: &AccessSourcesQuery,
    ) -> Result<Vec<AccessSourceRow>, BusinessError> {
        self.ready()?;
        query.validate()?;
        let branch = query.branch_id.as_deref();
        let department = query.department_id.as_deref();
        let rows = sqlx::query(SOURCES)
            .bind(&query.company_id)
            .bind(branch)
            .bind(branch)
            .bind(department)
            .bind(department)
            .fetch_all(&mut *self.connection)
            .await
            .map_err(|_| unavailable())?;
        rows.into_iter()
            .map(|row| {
                Ok::<_, BusinessError>(AccessSourceRow {
                    id: row.try_get("id").map_err(|_| unavailable())?,
                    number: row.try_get("number").map_err(|_| unavailable())?,
                    name: row.try_get("name").map_err(|_| unavailable())?,
                    version: row.try_get("version").map_err(|_| unavailable())?,
                })
            })
            .collect()
    }
}

impl AccessSourcesSource for PgAccessSources<'_> {
    fn sources<'a>(
        &'a mut self,
        query: &'a AccessSourcesQuery,
    ) -> SourcesFuture<'a, Vec<AccessSourceRow>> {
        Box::pin(async move { self.read(query).await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_is_executable_placeholder_form_not_a_plan() {
        assert!(SOURCES.starts_with("SELECT o.id,o.number,o.version,p.name"));
        assert!(!SOURCES.contains('?'), "a bound statement uses $n, not ?: {SOURCES}");
        assert!(!SOURCES.contains("%PREDICATE%"));
        assert_eq!(SOURCES.matches("$1").count(), 1);
        assert_eq!(SOURCES.matches("$2").count(), 1);
        assert_eq!(SOURCES.matches("$3").count(), 1);
        assert_eq!(SOURCES.matches("$4").count(), 1);
        assert_eq!(SOURCES.matches("$5").count(), 1);
        assert!(SOURCES.contains("FROM commerce_orders"));
        assert!(SOURCES.contains("FROM finance_parties"));
        assert!(SOURCES.ends_with("ORDER BY o.updated_at DESC LIMIT 250"));
        assert!(!SOURCES.to_uppercase().contains("INSERT INTO"));
        assert!(!SOURCES.to_uppercase().contains("DELETE FROM"));
        assert!(!SOURCES.to_uppercase().contains("UPDATE "));
    }

    #[test]
    fn unavailable_is_a_bounded_owner_error() {
        assert_eq!(unavailable().code, "CUSTOMER_ACCESS_SOURCES_UNAVAILABLE");
        assert_eq!(unavailable().kind, FailureKind::Unavailable);
    }
}
