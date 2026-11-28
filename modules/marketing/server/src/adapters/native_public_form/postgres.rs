//! PostgreSQL execution for the native public Marketing form source read.
//!
//! `SOURCE_SQL` is the retired TypeScript `active-form-public` statement with
//! its `?` placeholders; execution uses a fixed typed projection of the same
//! table, predicates and single bind. This is the only layer that executes
//! SQL: no lock, advisory lock, mutation or cross-owner table is added.

use crate::adapters::native_public_form::application::{
    PublicFormFuture, PublicFormSource,
};
use crate::adapters::native_public_form::domain::{self, FormRow};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::BusinessError;
use sqlx::{Arguments, Row, postgres::PgArguments};

/// Exact retired TypeScript source statement (`active-form-public`).
pub const SOURCE_SQL: &str = "SELECT id,company_id,name,version,preview_json,configuration_json FROM marketing_assets WHERE id=? AND kind='form' AND status='active'";

/// The same statement with a uniform decode projection. `version` is cast to
/// bigint so the wire number matches the retired TypeScript JSON type.
const PROJECTION_SQL: &str = concat!(
    "SELECT id::text AS id,company_id::text AS company_id,name::text AS name,",
    "version::bigint AS version,preview_json::text AS preview_json,",
    "configuration_json::text AS configuration_json ",
    "FROM marketing_assets WHERE id=$1 AND kind='form' AND status='active'"
);

/// The executed statement, for parity checks and composition diagnostics.
pub fn rendered_sql() -> &'static str {
    PROJECTION_SQL
}

fn text(row: &sqlx::postgres::PgRow, key: &str) -> Result<String, BusinessError> {
    row.try_get::<Option<String>, _>(key)
        .map_err(|_| domain::unavailable())?
        .ok_or_else(domain::unavailable)
}

fn integer(row: &sqlx::postgres::PgRow, key: &str) -> Result<i64, BusinessError> {
    row.try_get::<Option<i64>, _>(key)
        .map_err(|_| domain::unavailable())?
        .ok_or_else(domain::unavailable)
}

fn projection(row: &sqlx::postgres::PgRow) -> Result<FormRow, BusinessError> {
    let row = FormRow {
        id: text(row, "id")?,
        company_id: text(row, "company_id")?,
        name: text(row, "name")?,
        version: integer(row, "version")?,
        preview_json: text(row, "preview_json")?,
        configuration_json: text(row, "configuration_json")?,
    };
    if !domain::valid_row(&row) {
        return Err(domain::unavailable());
    }
    Ok(row)
}

/// Pool-backed owner source. The router mounts one of these per registered
/// Holding target; the axum handler never opens a connection itself.
pub struct PgPublicFormSource {
    pool: DatabasePool,
}

impl PgPublicFormSource {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl PublicFormSource for PgPublicFormSource {
    fn form<'a>(&'a self, form_id: &'a str) -> PublicFormFuture<'a, Option<FormRow>> {
        Box::pin(async move {
            let mut arguments = PgArguments::default();
            arguments.add(form_id).map_err(|_| domain::unavailable())?;
            let row = sqlx::query_with(sqlx::AssertSqlSafe(PROJECTION_SQL), arguments)
                .fetch_optional(self.pool.raw())
                .await
                .map_err(|_| domain::unavailable())?;
            row.as_ref().map(projection).transpose()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_and_projection_keep_the_same_owner_predicates_and_bind() {
        assert_eq!(
            SOURCE_SQL,
            "SELECT id,company_id,name,version,preview_json,configuration_json FROM marketing_assets WHERE id=? AND kind='form' AND status='active'"
        );
        assert_eq!(SOURCE_SQL.matches('?').count(), 1);
        assert_eq!(PROJECTION_SQL.matches("$1").count(), 1);
        assert!(PROJECTION_SQL.contains("kind='form' AND status='active'"));
        for banned in [
            "INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE",
            "LOCK TABLE", "pg_advisory", "attachment_", "command_receipts", "finance_",
        ] {
            assert!(
                !PROJECTION_SQL.contains(banned),
                "public form source must stay owner-local and read-only: {banned}"
            );
        }
    }
}
