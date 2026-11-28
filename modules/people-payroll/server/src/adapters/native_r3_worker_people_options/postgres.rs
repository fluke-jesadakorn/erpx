//! sqlx execution of the migrated project-worker-startup people-options read.
//! This is the only layer that executes SQL for this operation: the retired
//! `peopleOptions` statement from `adapters/worker-startup.ts` is rendered
//! verbatim, with only `?` renumbered to `$1..$3`. Columns, joins, predicates,
//! bind order and ordering are unchanged. No IAM authority, commercial package
//! admission, source witness or command write happens here.

use super::application::{PeopleFuture, WorkerPeopleOptionsRead};
use super::domain::{
    MAX_PEOPLE, WorkerPersonOption, invalid, unavailable, valid_id, valid_query,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::BusinessError;
use sqlx::Row;

/// Exact retained statement. Bind order is `[company, query, query]`, matching
/// the TypeScript `.bind(companyId,query,query)` call: `$1` Company scope,
/// `$2` the empty-query branch and `$3` the `position(lower(...))` substring.
pub const SQL_WORKER_PEOPLE_OPTIONS: &str = r#"SELECT p.id,p.legal_name,p.version,e.id engagement_id,e.version engagement_version FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id AND e.worker_type='employee' AND e.status IN ('onboarding','active') WHERE p.company_id=$1 AND p.status='active' AND p.merged_into_id IS NULL AND p.user_id IS NULL AND ($2='' OR position(lower($3) in lower(p.legal_name))>0) AND (SELECT count(*) FROM hr_engagements current WHERE current.person_id=p.id AND current.company_id=p.company_id AND current.status!='ended')=1 AND NOT EXISTS(SELECT 1 FROM command_receipts receipt WHERE receipt.record_id=p.id AND (jsonb_exists(receipt.result_json::jsonb,'_projectWorkerStartup') OR jsonb_exists(receipt.result_json::jsonb,'_learningWorkerStartup'))) ORDER BY lower(p.legal_name),p.id LIMIT 50"#;

/// SHA-256 of the executed statement, pinned in the owner contract test.
pub const SQL_ORACLE_SHA256: &str =
    "f593d187e962d6d4d708ccbae5b29822cbd4579aeaceac0c0fb6b931f67aa5af";

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

fn pack(row: &sqlx::postgres::PgRow) -> Result<WorkerPersonOption, BusinessError> {
    Ok(WorkerPersonOption {
        id: row.try_get::<String, _>("id").map_err(|_| unavailable())?,
        legal_name: row
            .try_get::<String, _>("legal_name")
            .map_err(|_| unavailable())?,
        version: integer(row, "version")?,
        engagement_id: row
            .try_get::<String, _>("engagement_id")
            .map_err(|_| unavailable())?,
        engagement_version: integer(row, "engagement_version")?,
    })
}

pub struct PgWorkerPeopleOptionsRead {
    pool: DatabasePool,
}

impl PgWorkerPeopleOptionsRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl WorkerPeopleOptionsRead for PgWorkerPeopleOptionsRead {
    fn people<'a>(
        &'a self,
        company: &'a str,
        query: &'a str,
    ) -> PeopleFuture<'a, Vec<WorkerPersonOption>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let query = query.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let query = query.clone();
                Box::pin(async move {
                    if !valid_id(&company) || !valid_query(&query) {
                        return Err(invalid().into());
                    }
                    let rows = sqlx::query(SQL_WORKER_PEOPLE_OPTIONS)
                        .bind(company.as_str())
                        .bind(query.as_str())
                        .bind(query.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    if rows.len() > MAX_PEOPLE {
                        return Err(unavailable().into());
                    }
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
    fn statement_keeps_owner_scope_and_shape() {
        assert!(SQL_WORKER_PEOPLE_OPTIONS.contains("p.company_id=$1"));
        assert!(SQL_WORKER_PEOPLE_OPTIONS.contains("($2='' OR position(lower($3) in lower(p.legal_name))>0)"));
        assert!(SQL_WORKER_PEOPLE_OPTIONS.contains("e.worker_type='employee'"));
        assert!(SQL_WORKER_PEOPLE_OPTIONS.contains("e.status IN ('onboarding','active')"));
        assert!(SQL_WORKER_PEOPLE_OPTIONS.contains("_projectWorkerStartup"));
        assert!(SQL_WORKER_PEOPLE_OPTIONS.contains("_learningWorkerStartup"));
        assert!(SQL_WORKER_PEOPLE_OPTIONS.contains("ORDER BY lower(p.legal_name),p.id LIMIT 50"));
        assert!(!SQL_WORKER_PEOPLE_OPTIONS.contains('?'));
    }

    #[test]
    fn sql_oracle_matches_the_owner_constant() {
        let hex = sha2::Sha256::digest(SQL_WORKER_PEOPLE_OPTIONS.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(hex, SQL_ORACLE_SHA256);
    }
}
