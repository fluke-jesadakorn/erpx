//! PostgreSQL execution for the native Platform usage read.
//!
//! `EXECUTIVE_USAGE_SQL` is the executable form of the retired TypeScript
//! `recordsExecutiveUsageSql` projection with its `?` binds rendered as `$1`
//! and `$2`. It is executed here with sqlx on the owner's Control pool; no
//! TypeScript prepares or executes this statement for the native operation.

use crate::adapters::native_platform_usage::application::{
    ExecutiveUsageRead, SharedExecutiveUsageRead, UsageFuture,
};
use crate::adapters::native_platform_usage::domain::source_invalid;
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::BusinessError;

/// Executable owner SQL. The inner projection is byte-identical to the
/// `recordsExecutiveUsageSql` head owned by this crate.
pub const EXECUTIVE_USAGE_SQL: &str = "SELECT count(*)::bigint FROM (SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') AS month FROM (SELECT company_id,created_at FROM executive_generation_jobs) executive_generation_jobs) executive_usage WHERE company_id=$1 AND month=$2";

/// The exact `?`-placeholder rendering captured from the retired TypeScript
/// projection and its `recordsQuotaCountSql` wrapper.
pub fn rendered_usage_sql() -> String {
    EXECUTIVE_USAGE_SQL.replace("$1", "?").replace("$2", "?")
}

fn persistence(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => source_invalid(),
    }
}

pub struct PgExecutiveUsageRead {
    pool: DatabasePool,
}

impl PgExecutiveUsageRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl ExecutiveUsageRead for PgExecutiveUsageRead {
    fn generation_jobs<'a>(
        &'a self,
        company: &'a str,
        month: &'a str,
    ) -> UsageFuture<'a, i64> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let month = month.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let month = month.clone();
                Box::pin(async move {
                    let count = sqlx::query_scalar::<_, i64>(EXECUTIVE_USAGE_SQL)
                        .bind(company)
                        .bind(month)
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| PgTransactionError::Business(source_invalid()))?;
                    if !(0..=crate::adapters::native_platform_usage::domain::MAX_USAGE).contains(&count) {
                        return Err(PgTransactionError::Business(source_invalid()));
                    }
                    Ok(count)
                })
            })
            .await
            .map_err(persistence)
        })
    }
}

/// Convenience constructor used by the composition mount for one host.
pub fn shared_reader(pool: DatabasePool) -> SharedExecutiveUsageRead {
    std::sync::Arc::new(PgExecutiveUsageRead::new(pool))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_platform_usage::domain::MAX_USAGE;
    use sha2::{Digest, Sha256};

    fn hash(value: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    #[test]
    fn executable_sql_is_the_bound_render_of_the_retired_typescript() {
        let rendered = rendered_usage_sql();
        assert!(rendered.contains("SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') AS month FROM (SELECT company_id,created_at FROM executive_generation_jobs) executive_generation_jobs"));
        assert!(rendered.contains("WHERE company_id=? AND month=?"));
        assert_eq!(rendered.matches('?').count(), 2);
        let mut executable = rendered.clone();
        let first = executable.find("=?").unwrap();
        executable.replace_range(first..first + 2, "=$1");
        let second = executable.find("=?").unwrap();
        executable.replace_range(second..second + 2, "=$2");
        assert_eq!(executable, EXECUTIVE_USAGE_SQL);
        assert_eq!(hash(&EXECUTIVE_USAGE_SQL).len(), 64);
        assert!(!EXECUTIVE_USAGE_SQL.contains("INSERT")
            && !EXECUTIVE_USAGE_SQL.contains("UPDATE")
            && !EXECUTIVE_USAGE_SQL.contains("FOR UPDATE"));
        assert!(MAX_USAGE > 0);
    }
}
