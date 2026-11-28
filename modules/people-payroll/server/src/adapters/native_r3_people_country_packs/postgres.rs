//! sqlx execution of the migrated country-packs read. This is the only layer
//! that executes SQL for this operation: the `readListCountryPacksEffect`
//! statement from `adapters/people-payroll.ts` is rendered verbatim (no binds).
//! Columns, ordering and shape are unchanged. No IAM authority, commercial
//! package admission or command write happens here.

use super::application::{CountryPackFuture, CountryPacksRead};
use super::domain::{CountryPackRow, invalid, unavailable, valid_id};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::BusinessError;
use sqlx::Row;

/// Exact retained statement; `?` renumbering is unnecessary (there are no
/// binds). Company scope is enforced by the owning service target, not a
/// predicate, matching the TS reader where country packs are global reference
/// data.
pub const SQL_COUNTRY_PACKS: &str = r#"SELECT id,country_code,version,status,effective_from,effective_to,adapter_id,source_digest,record_version,export_formats_json,validated_by,updated_at FROM hr_country_packs ORDER BY country_code,version DESC"#;

/// SHA-256 of the executed statement, pinned in the owner contract test.
pub const SQL_ORACLE_SHA256: &str =
    "58051148dfbe30f2337fcaeb15edb453c21ba84a9d6fbff9fd5558090b156e3e";

fn text(row: &sqlx::postgres::PgRow, column: &str) -> Result<String, BusinessError> {
    row.try_get::<String, _>(column).map_err(|_| unavailable())
}

fn optional_text(row: &sqlx::postgres::PgRow, column: &str) -> Option<String> {
    row.try_get::<Option<String>, _>(column).ok().flatten()
}

fn integer(row: &sqlx::postgres::PgRow, column: &str) -> Result<i64, BusinessError> {
    row.try_get::<Option<i64>, _>(column)
        .ok()
        .flatten()
        .or_else(|| {
            row.try_get::<Option<i32>, _>(column)
                .ok()
                .flatten()
                .map(i64::from)
        })
        .ok_or_else(unavailable)
}

fn formats(row: &sqlx::postgres::PgRow) -> Result<serde_json::Value, BusinessError> {
    if let Ok(value) = row.try_get::<serde_json::Value, _>("export_formats_json") {
        return Ok(value);
    }
    match optional_text(row, "export_formats_json") {
        Some(raw) => serde_json::from_str(&raw).map_err(|_| unavailable()),
        None => Ok(serde_json::Value::Null),
    }
}

fn pack(row: &sqlx::postgres::PgRow) -> Result<CountryPackRow, BusinessError> {
    Ok(CountryPackRow {
        id: text(row, "id")?,
        country_code: text(row, "country_code")?,
        version: text(row, "version")?,
        status: text(row, "status")?,
        effective_from: optional_text(row, "effective_from"),
        effective_to: optional_text(row, "effective_to"),
        adapter_id: optional_text(row, "adapter_id"),
        source_digest: optional_text(row, "source_digest"),
        record_version: integer(row, "record_version")?,
        export_formats_json: formats(row)?,
        validated_by: optional_text(row, "validated_by"),
        updated_at: text(row, "updated_at")?,
    })
}

pub struct PgCountryPacksRead {
    pool: DatabasePool,
}

impl PgCountryPacksRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl CountryPacksRead for PgCountryPacksRead {
    fn packs<'a>(&'a self, company: &'a str) -> CountryPackFuture<'a, Vec<CountryPackRow>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                Box::pin(async move {
                    if !valid_id(&company) {
                        return Err(invalid().into());
                    }
                    let rows = sqlx::query(SQL_COUNTRY_PACKS)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    rows.iter()
                        .map(pack)
                        .collect::<Result<Vec<_>, BusinessError>>()
                        .map_err(Into::into)
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
    use sha2::Digest;

    #[test]
    fn statement_keeps_the_retained_projection_and_order() {
        assert!(SQL_COUNTRY_PACKS.starts_with("SELECT id,country_code,version,status,"));
        assert!(SQL_COUNTRY_PACKS.contains("export_formats_json,validated_by,updated_at"));
        assert!(SQL_COUNTRY_PACKS.contains("FROM hr_country_packs"));
        assert!(SQL_COUNTRY_PACKS.ends_with("ORDER BY country_code,version DESC"));
        assert!(!SQL_COUNTRY_PACKS.contains('?'));
        assert!(!SQL_COUNTRY_PACKS.contains("WHERE"));
    }

    #[test]
    fn sql_oracle_matches_the_owner_constant() {
        let hex = sha2::Sha256::digest(SQL_COUNTRY_PACKS.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(hex, SQL_ORACLE_SHA256);
    }
}
