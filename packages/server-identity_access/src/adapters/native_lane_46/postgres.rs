//! sqlx execution for the native directory "connection by id" read.
//!
//! This is the byte-equivalent successor of the statement prepared in
//! `adapters/directory-lookup.ts` (`connectionById`). Only the placeholder
//! syntax is translated (`?` to `$1`); the `SELECT *` projection and the
//! `company_id`/`id` predicate are unchanged. Every statement runs in one
//! owner Control unit and every persistence fault maps to 503.

use super::application::{ConnectionRead, ConnectionReadFuture};
use super::domain::{ConnectionFields, ConnectionRow, invalid_result, unavailable};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
use serde_json::Value;
use sqlx::Row;

/// Executed statement (`$1`, `$2` placeholders).
pub const SQL_CONNECTION_BY_ID: &str =
    "SELECT * FROM iam_connections WHERE company_id=$1 AND id=$2";

/// Pre-migration TypeScript statement (`?` placeholders), pinned only as the
/// parity oracle for the retired authority.
pub const TS_ORACLE_SQL: &str =
    "SELECT * FROM iam_connections WHERE company_id=? AND id=?";

/// The retired shared helper rendered this statement with its caller's
/// predicate. Recovering it from the two bound columns keeps the oracle exact.
pub fn connection_by_id_sql() -> &'static str {
    TS_ORACLE_SQL
}

pub struct PgConnectionRead {
    control: DatabasePool,
}

impl PgConnectionRead {
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

fn fields(row: &sqlx::postgres::PgRow) -> Result<ConnectionFields, BusinessError> {
    let version = row
        .try_get::<i32, _>("version")
        .map_err(|_| invalid_result())? as i64;
    let tested_version = row
        .try_get::<Option<i32>, _>("tested_version")
        .map_err(|_| invalid_result())?
        .map(|value| value as i64);
    Ok(ConnectionFields {
        id: row.try_get("id").map_err(|_| invalid_result())?,
        company_id: row.try_get("company_id").map_err(|_| invalid_result())?,
        name: row.try_get("name").map_err(|_| invalid_result())?,
        provider: row.try_get("provider").map_err(|_| invalid_result())?,
        config: row
            .try_get::<Value, _>("config")
            .map_err(|_| invalid_result())?,
        status: row.try_get("status").map_err(|_| invalid_result())?,
        version,
        tested_version,
        last_sync_at: row.try_get("last_sync_at").map_err(|_| invalid_result())?,
        health: row.try_get("health").map_err(|_| invalid_result())?,
        last_error: row.try_get("last_error").map_err(|_| invalid_result())?,
        created_by: row.try_get("created_by").map_err(|_| invalid_result())?,
        created_at: row.try_get("created_at").map_err(|_| invalid_result())?,
    })
}

impl ConnectionRead for PgConnectionRead {
    fn connection<'a>(
        &'a self,
        company_id: &'a str,
        id: &'a str,
    ) -> ConnectionReadFuture<'a, Option<ConnectionRow>> {
        let unit = PgUnitOfWork::new(self.control.clone());
        let company_id = company_id.to_owned();
        let id = id.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company_id = company_id.clone();
                let id = id.clone();
                Box::pin(async move {
                    let row = sqlx::query(SQL_CONNECTION_BY_ID)
                        .bind(company_id.as_str())
                        .bind(id.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    row.map(|row| fields(&row)?.into_row())
                        .transpose()
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
    fn executed_statement_is_the_legacy_statement_with_bound_placeholders() {
        // The only difference is `?` -> `$1`/`$2`; the `SELECT *` projection
        // and predicate are byte-identical to the retired TypeScript authority.
        assert_eq!(
            TS_ORACLE_SQL.replacen('?', "$1", 1).replacen('?', "$2", 1),
            SQL_CONNECTION_BY_ID
        );
        assert!(SQL_CONNECTION_BY_ID.starts_with("SELECT * FROM iam_connections"));
        assert!(SQL_CONNECTION_BY_ID.ends_with("company_id=$1 AND id=$2"));
        assert!(!SQL_CONNECTION_BY_ID.contains('?'));
        assert_eq!(connection_by_id_sql(), TS_ORACLE_SQL);
    }
}
