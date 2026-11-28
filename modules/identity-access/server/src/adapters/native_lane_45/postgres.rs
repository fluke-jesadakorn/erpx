//! sqlx execution for the native IAM "sessions by user" read.
//!
//! This is the byte-equivalent successor of the TypeScript statement prepared
//! in `adapters/iam-service-reads.ts` (`sessions` port). Only the placeholder
//! syntax is translated (`?` to `$1`); the projection, predicate and fixed
//! `ORDER BY "createdAt" DESC` suffix are unchanged. Every statement runs in
//! one owner Control transaction and every persistence fault maps to 503.

use super::application::{SessionListFuture, SessionListRead};
use super::domain::{SessionFields, SessionRow, invalid_result, unavailable};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::BusinessError;
use sqlx::Row;

/// Executed statement (`$1` placeholder).
pub const SQL_SESSION_LIST: &str = "SELECT id,\"createdAt\",\"expiresAt\",\"userAgent\" FROM session WHERE \"userId\"=$1 ORDER BY \"createdAt\" DESC";

/// Pre-migration TypeScript statement (`?` placeholder), pinned only as the
/// parity oracle for the retired authority.
pub const TS_ORACLE_SQL: &str = "SELECT id,\"createdAt\",\"expiresAt\",\"userAgent\" FROM session WHERE \"userId\"=? ORDER BY \"createdAt\" DESC";

pub struct PgSessionListRead {
    control: DatabasePool,
}

impl PgSessionListRead {
    pub fn new(control: DatabasePool) -> Self {
        Self { control }
    }
}

fn persistence(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

impl SessionListRead for PgSessionListRead {
    fn list_sessions<'a>(&'a self, user_id: &'a str) -> SessionListFuture<'a, Vec<SessionRow>> {
        let unit = PgUnitOfWork::new(self.control.clone());
        let user_id = user_id.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let user_id = user_id.clone();
                Box::pin(async move {
                    let rows = sqlx::query(SQL_SESSION_LIST)
                        .bind(user_id.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    rows.into_iter()
                        .map(|row| {
                            SessionFields {
                                id: row.try_get("id").map_err(|_| invalid_result())?,
                                created_at: row.try_get("createdAt").map_err(|_| invalid_result())?,
                                expires_at: row.try_get("expiresAt").map_err(|_| invalid_result())?,
                                user_agent: row.try_get("userAgent").map_err(|_| invalid_result())?,
                            }
                            .into_row()
                        })
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(PgTransactionError::Business)
                })
            })
            .await
            .map_err(persistence)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executed_statement_is_the_legacy_statement_with_bound_placeholder() {
        // The only difference is `?` -> `$1`; projection, predicate and ORDER
        // BY suffix are byte-identical to the retired TypeScript authority.
        assert_eq!(TS_ORACLE_SQL.replacen('?', "$1", 1), SQL_SESSION_LIST);
        assert!(SQL_SESSION_LIST.contains("ORDER BY \"createdAt\" DESC"));
        assert!(!SQL_SESSION_LIST.contains('?'));
    }
}
