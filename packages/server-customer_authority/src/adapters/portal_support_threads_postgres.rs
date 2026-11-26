//! PostgreSQL execution of the verified-customer portal support-thread read.
//!
//! This is the only layer that prepares or executes SQL. The statements are the
//! executable form of the registered `portal-support.*` read plans: `?` bind
//! positions became `$n`, and every row is mapped into the owner domain type
//! before the application layer shapes the wire payload.

use crate::application::portal_support_threads::{
    PortalThreadSource, ThreadsFuture, customer_support_threads,
};
use crate::domain::portal_support_threads::{Thread, ThreadsQuery, thread_from_columns};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};

const IDENTITY: &str =
    "SELECT EXISTS(SELECT 1 FROM customer_access_identities WHERE binding_id=$1 AND user_id=$2 AND email=$3)";
const REQUEST_ID: &str =
    "SELECT id FROM customer_portal_actions WHERE binding_id=$1 AND user_id=$2 AND action='request_support'";
const THREADS: &str = concat!(
    "SELECT request.id,request.created_at,request.note,response.id response_id,",
    "response.response_version,response.message,response.created_at published_at,",
    "ack.created_at acknowledged_at,closure.created_at closed_at ",
    "FROM customer_portal_actions request LEFT JOIN LATERAL(",
    "SELECT * FROM customer_portal_support_responses response WHERE response.request_id=request.id ",
    "ORDER BY response.response_version DESC LIMIT 1) response ON true ",
    "LEFT JOIN customer_portal_support_acknowledgements ack ON ack.request_id=request.id AND ",
    "ack.response_id=response.id LEFT JOIN customer_portal_support_closures closure ON ",
    "closure.request_id=request.id WHERE request.binding_id=$1 AND request.action='request_support' ",
    "AND ($2::text IS NULL OR request.user_id=$2) ORDER BY request.created_at,request.id"
);

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PORTAL_SUPPORT_THREADS_UNAVAILABLE",
        "The portal support thread read is unavailable.",
    )
}

/// Caller-owned PostgreSQL source. The composition layer supplies the unit of
/// work and the verified session facts; this adapter never opens a connection
/// or commits a transaction.
pub struct PgPortalSupportThreads<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgPortalSupportThreads<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    fn ready(&self) -> Result<(), BusinessError> {
        if !self.connection.is_in_transaction() {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "PORTAL_SUPPORT_THREADS_TRANSACTION_REQUIRED",
                "The portal support thread read requires the caller's transaction.",
            ));
        }
        Ok(())
    }

    /// Execute the full use case on this connection. Every SQL statement is
    /// executed here, not planned for a TypeScript caller.
    pub async fn read(&mut self, query: ThreadsQuery) -> Result<Vec<Thread>, BusinessError> {
        self.ready()?;
        customer_support_threads(self, query).await
    }
}

impl PortalThreadSource for PgPortalSupportThreads<'_> {
    fn identity_exists<'a>(
        &'a mut self,
        query: &'a ThreadsQuery,
    ) -> ThreadsFuture<'a, bool> {
        Box::pin(async move {
            let exists = sqlx::query_scalar::<_, bool>(IDENTITY)
                .bind(&query.binding_id)
                .bind(&query.user_id)
                .bind(&query.email)
                .fetch_one(&mut *self.connection)
                .await
                .map_err(|_| unavailable())?;
            Ok(exists)
        })
    }

    fn request_id<'a>(
        &'a mut self,
        query: &'a ThreadsQuery,
    ) -> ThreadsFuture<'a, Option<String>> {
        Box::pin(async move {
            let id = sqlx::query_scalar::<_, String>(REQUEST_ID)
                .bind(&query.binding_id)
                .bind(&query.user_id)
                .fetch_optional(&mut *self.connection)
                .await
                .map_err(|_| unavailable())?;
            Ok(id)
        })
    }

    fn threads<'a>(&'a mut self, query: &'a ThreadsQuery) -> ThreadsFuture<'a, Vec<Thread>> {
        Box::pin(async move {
            let rows = sqlx::query(THREADS)
                .bind(&query.binding_id)
                .bind(&query.user_id)
                .bind(&query.user_id)
                .fetch_all(&mut *self.connection)
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|row| {
                    Ok::<_, BusinessError>(thread_from_columns(
                        row.try_get("id").map_err(|_| unavailable())?,
                        row.try_get("created_at").map_err(|_| unavailable())?,
                        row.try_get("note").map_err(|_| unavailable())?,
                        row.try_get("response_id").map_err(|_| unavailable())?,
                        row.try_get("response_version").map_err(|_| unavailable())?,
                        row.try_get("message").map_err(|_| unavailable())?,
                        row.try_get("published_at").map_err(|_| unavailable())?,
                        row.try_get("acknowledged_at").map_err(|_| unavailable())?,
                        row.try_get("closed_at").map_err(|_| unavailable())?,
                    ))
                })
                .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_is_executable_placeholder_form_not_a_plan() {
        for sql in [IDENTITY, REQUEST_ID, THREADS] {
            assert!(sql.starts_with("SELECT "), "{sql}");
            assert!(!sql.contains('?'), "a bound statement uses $n, not ?: {sql}");
            assert!(!sql.contains("${"), "no unresolved source token: {sql}");
        }
        assert_eq!(THREADS.matches("$1").count(), 1);
        assert_eq!(THREADS.matches("$2").count(), 2);
        assert!(THREADS.contains("customer_portal_support_responses"));
        assert!(THREADS.contains("customer_portal_support_acknowledgements"));
        assert!(THREADS.contains("customer_portal_support_closures"));
    }

    #[test]
    fn unavailable_is_a_bounded_owner_error() {
        assert_eq!(unavailable().code, "PORTAL_SUPPORT_THREADS_UNAVAILABLE");
        assert_eq!(unavailable().kind, FailureKind::Unavailable);
    }
}
