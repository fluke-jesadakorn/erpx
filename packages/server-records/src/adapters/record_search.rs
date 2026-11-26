//! PostgreSQL adapter for the native Records reference-search read.
//!
//! This is the only place in the migrated path that holds SQL text for the
//! search family. It selects Records-owned `workspace_records` rows on a pooled
//! Holding connection; Company/branch/department scope, the module allowlist and
//! the search term are always bind parameters. Cross-owner row admission and
//! the control-side principal lookup are composition-owned ports.

use crate::application::record_search::{RecordSearchSource, SearchFuture};
use crate::domain::record_search::{
    MAX_SCAN, RecordHit, SearchActor, SearchQuery, unavailable,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::BusinessError;
use sqlx::Row;

/// Holding-scope owner read. Mirrors the retained `module_id||'/'||id ILIKE`
/// and `title ILIKE` predicates with one bound term reused for both.
pub const HOLDING_SQL: &str = "SELECT module_id,id,title,status,version FROM workspace_records \
 WHERE archived_at IS NULL AND company_id=$1 AND module_id=ANY($2::text[]) \
 AND (module_id||'/'||id ILIKE $3 OR title ILIKE $3) ORDER BY updated_at DESC LIMIT $4";

/// Branch-scope owner read.
pub const BRANCH_SQL: &str = "SELECT module_id,id,title,status,version FROM workspace_records \
 WHERE archived_at IS NULL AND company_id=$1 AND branch_id=$2 AND module_id=ANY($3::text[]) \
 AND (module_id||'/'||id ILIKE $4 OR title ILIKE $4) ORDER BY updated_at DESC LIMIT $5";

/// Department-scope owner read.
pub const DEPARTMENT_SQL: &str = "SELECT module_id,id,title,status,version FROM workspace_records \
 WHERE archived_at IS NULL AND company_id=$1 AND department_id=$2 AND module_id=ANY($3::text[]) \
 AND (module_id||'/'||id ILIKE $4 OR title ILIKE $4) ORDER BY updated_at DESC LIMIT $5";

/// Postgres-backed owner reader for the Records search family.
pub struct PgRecordSearch {
    pool: DatabasePool,
}

impl PgRecordSearch {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl RecordSearchSource for PgRecordSearch {
    fn records<'a>(
        &'a self,
        actor: &'a SearchActor,
        query: &'a SearchQuery,
    ) -> SearchFuture<'a, Vec<RecordHit>> {
        let company = actor.company_id.clone();
        let branch = actor.branch_id.clone();
        let department = actor.department_id.clone();
        let modules = actor.modules.clone();
        let term = query.term();
        Box::pin(async move {
            let mut connection = self
                .pool
                .raw()
                .acquire()
                .await
                .map_err(|_| unavailable())?;
            let stmt = if let Some(department) = department {
                sqlx::query(DEPARTMENT_SQL)
                    .bind(company.as_str())
                    .bind(department.as_str())
                    .bind(&modules)
                    .bind(term.as_str())
                    .bind(MAX_SCAN as i64)
            } else if let Some(branch) = branch {
                sqlx::query(BRANCH_SQL)
                    .bind(company.as_str())
                    .bind(branch.as_str())
                    .bind(&modules)
                    .bind(term.as_str())
                    .bind(MAX_SCAN as i64)
            } else {
                sqlx::query(HOLDING_SQL)
                    .bind(company.as_str())
                    .bind(&modules)
                    .bind(term.as_str())
                    .bind(MAX_SCAN as i64)
            };
            let rows = stmt
                .fetch_all(&mut *connection)
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|row| {
                    Ok(RecordHit {
                        module_id: row.try_get("module_id").map_err(|_| unavailable())?,
                        id: row.try_get("id").map_err(|_| unavailable())?,
                        title: row.try_get("title").map_err(|_| unavailable())?,
                        status: row.try_get("status").map_err(|_| unavailable())?,
                        version: row.try_get("version").map_err(|_| unavailable())?,
                    })
                })
                .collect::<Result<Vec<_>, BusinessError>>()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executed_sql_is_owner_local_and_parameterised() {
        for sql in [HOLDING_SQL, BRANCH_SQL, DEPARTMENT_SQL] {
            assert!(sql.starts_with("SELECT module_id,id,title,status,version FROM workspace_records"));
            assert!(sql.contains("company_id=$1"));
            assert!(sql.contains("archived_at IS NULL"));
            assert!(sql.contains("module_id=ANY("));
            assert!(sql.contains("ILIKE $"));
            assert!(sql.contains("ORDER BY updated_at DESC"));
            assert!(sql.contains("LIMIT $"));
            assert!(!sql.contains("include_str!"));
        }
        assert!(BRANCH_SQL.contains("branch_id=$2"));
        assert!(DEPARTMENT_SQL.contains("department_id=$2"));
    }

    #[test]
    fn scan_bound_is_finite() {
        assert!(MAX_SCAN >= MAX_SCAN.min(1));
        assert!(MAX_SCAN <= 1_000);
    }
}
