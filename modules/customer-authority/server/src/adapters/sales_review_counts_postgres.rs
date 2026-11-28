//! PostgreSQL execution of the sales customer review counts read.
//!
//! This is the only layer that prepares or executes SQL. The statement is the
//! executable `$n` form of the owner's `customer_authority_rd_sales_review_counts`
//! plan: every positional `?` became `$n`, the closed commerce order projection
//! is inlined verbatim, and every row is mapped into the owner domain type. No
//! TypeScript prepares or executes this statement and no plan-only reply is
//! returned.
use crate::application::sales_review_counts::{
    ReviewCountsFuture, SalesReviewCountsSource,
};
use crate::domain::sales_review_counts::{ReviewCount, SalesReviewCountsQuery, unavailable};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};

/// Exact executable successor of the retired TypeScript `sales-review-counts`
/// read. The seven positional binds mirror `[company, company, company, branch,
/// branch, department, department]`.
const REVIEW_COUNTS: &str = concat!(
    "SELECT 'identity' kind,COUNT(*) count FROM customer_identity_changes WHERE company_id=$1 AND status='pending' ",
    "UNION ALL SELECT 'duplicates',COUNT(*) FROM customer_duplicate_reviews WHERE company_id=$2 AND status='pending' ",
    "UNION ALL SELECT 'access',COUNT(*) FROM customer_access_bindings b JOIN (",
    "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",",
    "\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",",
    "\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",",
    "\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders",
    ") o ON o.id=b.order_id AND o.company_id=b.company_id ",
    "WHERE o.company_id=$3 AND ($4::text IS NULL OR o.branch_id=$5) ",
    "AND ($6::text IS NULL OR o.department_id=$7) AND b.status='pending'",
);

/// Caller-owned PostgreSQL source. Composition supplies the unit of work; this
/// adapter never opens a connection, commits a transaction or accepts SQL.
pub struct PgSalesReviewCounts<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgSalesReviewCounts<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    fn ready(&self) -> Result<(), BusinessError> {
        if !self.connection.is_in_transaction() {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "CUSTOMER_SALES_REVIEW_COUNTS_TRANSACTION_REQUIRED",
                "The sales review counts read requires the caller's transaction.",
            ));
        }
        Ok(())
    }

    /// Execute the SQL on the caller's open unit and map every row into the
    /// owner domain type. The complete payload is returned to the application
    /// layer for validation.
    pub async fn read(
        &mut self,
        query: &SalesReviewCountsQuery,
    ) -> Result<Vec<ReviewCount>, BusinessError> {
        self.ready()?;
        query.validate()?;
        let branch = query.branch_id.as_deref();
        let department = query.department_id.as_deref();
        let rows = sqlx::query(REVIEW_COUNTS)
            .bind(&query.company_id)
            .bind(&query.company_id)
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
                Ok::<_, BusinessError>(ReviewCount {
                    kind: row.try_get("kind").map_err(|_| unavailable())?,
                    count: row.try_get("count").map_err(|_| unavailable())?,
                })
            })
            .collect()
    }
}

impl SalesReviewCountsSource for PgSalesReviewCounts<'_> {
    fn review_counts<'a>(
        &'a mut self,
        query: &'a SalesReviewCountsQuery,
    ) -> ReviewCountsFuture<'a, Vec<ReviewCount>> {
        Box::pin(async move { self.read(query).await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_is_executable_placeholder_form_not_a_plan() {
        assert!(REVIEW_COUNTS.starts_with("SELECT 'identity' kind,COUNT(*) count"));
        assert!(!REVIEW_COUNTS.contains('?'), "a bound statement uses $n, not ?: {REVIEW_COUNTS}");
        for n in 1..=7 {
            let placeholder = format!("${n}");
            assert_eq!(REVIEW_COUNTS.matches(placeholder.as_str()).count(), 1, "bind ${n}");
        }
        assert!(REVIEW_COUNTS.contains("FROM customer_identity_changes"));
        assert!(REVIEW_COUNTS.contains("FROM customer_duplicate_reviews"));
        assert!(REVIEW_COUNTS.contains("FROM customer_access_bindings"));
        assert!(REVIEW_COUNTS.contains("FROM commerce_orders"));
        assert!(REVIEW_COUNTS.contains("AND b.status='pending'"));
        for banned in ["INSERT", "DELETE", "UPDATE ", "LOCK TABLE", "FOR UPDATE", "pg_advisory"] {
            assert!(!REVIEW_COUNTS.contains(banned), "contains {banned}");
        }
    }

    #[test]
    fn unavailable_is_a_bounded_owner_error() {
        assert_eq!(unavailable().code, "CUSTOMER_SALES_REVIEW_COUNTS_UNAVAILABLE");
        assert_eq!(unavailable().kind, FailureKind::Unavailable);
    }
}
