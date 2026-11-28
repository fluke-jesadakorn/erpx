//! sqlx execution of the migrated active pay-group options read. This is the
//! only place the retired TypeScript statement now lives: it is executed here
//! on a Company-scoped `PgUnitOfWork`, with no TypeScript prepare/batch fallback.

use super::application::{OptionsFuture, PayGroupOptionsRead};
use super::domain::{
    MAX_GROUPS, PayGroupOption, invalid, unavailable, valid_id,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::BusinessError;
use sqlx::Row;

/// Byte-identical successor of the TypeScript literal behind
/// `PEOPLE_PAYROLL_RD_PAYROLL_OPTIONS_GROUPS` in `adapters/people-payroll.ts`.
/// Only `?` is renumbered to `$1` for sqlx; columns, order and bind order match.
pub const SQL_PAY_GROUP_OPTIONS: &str =
    "SELECT id,name,currency,frequency FROM hr_pay_groups WHERE company_id=$1 AND status='active' ORDER BY name";

/// SHA-256 of the executed statement, pinned in the TS transport oracle.
pub const SQL_ORACLE_SHA256: &str =
    "d53d068a7d9d7a7a5f1ccaf34026302bf253cfb15128adc8c7cd6d9aa5aeda43";

pub struct PgPayGroupOptionsRead {
    pool: DatabasePool,
}

impl PgPayGroupOptionsRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl PayGroupOptionsRead for PgPayGroupOptionsRead {
    fn options<'a>(&'a self, company: &'a str) -> OptionsFuture<'a, Vec<PayGroupOption>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        Box::pin(async move {
            let rows = unit
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let company = company.clone();
                    Box::pin(async move {
                        if !valid_id(&company) {
                            return Err(invalid().into());
                        }
                        let rows = sqlx::query(SQL_PAY_GROUP_OPTIONS)
                            .bind(company.as_str())
                            .fetch_all(&mut *connection)
                            .await
                            .map_err(|_| unavailable())?;
                        if rows.len() > MAX_GROUPS {
                            return Err(unavailable().into());
                        }
                        rows.into_iter()
                            .map(|row| {
                                Ok(PayGroupOption {
                                    id: row.try_get::<String, _>("id").map_err(|_| unavailable())?,
                                    name: row.try_get::<String, _>("name").map_err(|_| unavailable())?,
                                    currency: row.try_get::<String, _>("currency").map_err(|_| unavailable())?,
                                    frequency: row.try_get::<String, _>("frequency").map_err(|_| unavailable())?,
                                })
                            })
                            .collect::<Result<Vec<_>, BusinessError>>()
                            .map_err(Into::into)
                    })
                })
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    PgTransactionError::Persistence(_) => unavailable(),
                })?;
            Ok(rows)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;

    #[test]
    fn statement_keeps_owner_scope_and_shape() {
        assert!(SQL_PAY_GROUP_OPTIONS.contains("company_id=$1"));
        assert!(SQL_PAY_GROUP_OPTIONS.contains("status='active'"));
        assert!(SQL_PAY_GROUP_OPTIONS.contains("ORDER BY name"));
        assert!(SQL_PAY_GROUP_OPTIONS.contains("id,name,currency,frequency"));
        assert!(!SQL_PAY_GROUP_OPTIONS.contains('?'));
    }

    #[test]
    fn sql_oracle_matches_the_transport_constant() {
        let hex = sha2::Sha256::digest(SQL_PAY_GROUP_OPTIONS.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(hex, SQL_ORACLE_SHA256);
    }
}
