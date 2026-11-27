//! PostgreSQL adapter for the native Records reference-search read.
//!
//! This is the only place in the migrated path that holds SQL text for the
//! search family. It selects Records-owned `workspace_records` rows on the selected
//! Holding transaction; Company/branch/department scope, the module allowlist and
//! the search term are always bind parameters. Cross-owner row admission and
//! the control-side principal lookup are composition-owned ports.

use crate::domain::record_search::{MAX_SCAN, RecordHit, SearchActor, SearchQuery, unavailable};
use erpx_server_shared::public::BusinessError;
use sqlx::{AssertSqlSafe, PgConnection, Row};

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

fn partitioned_candidates(statement: &'static str, field_probe: &'static str) -> String {
    let (condition, ordering) = statement
        .split_once(" ORDER BY ")
        .expect("fixed Records search statements include ordering");
    format!(
        "{condition} AND workspace_records.module_id<>'field_service' AND NOT EXISTS({field_probe}) ORDER BY {ordering}"
    )
}

/// Execute the Records-owned candidate read on the caller's Holding
/// connection. Scope, module allowlist and the single reused term stay bind
/// parameters; `LIMIT` remains the finite `MAX_SCAN` bind. The composition
/// supplies the fixed Field owner source probe before candidates are limited.
pub async fn read_search_candidates(
    connection: &mut PgConnection,
    actor: &SearchActor,
    query: &SearchQuery,
    field_probe: &'static str,
) -> Result<Vec<RecordHit>, BusinessError> {
    let (statement, node) = if let Some(department) = actor.department_id.as_deref() {
        (DEPARTMENT_SQL, Some(department))
    } else if let Some(branch) = actor.branch_id.as_deref() {
        (BRANCH_SQL, Some(branch))
    } else {
        (HOLDING_SQL, None)
    };
    let mut stmt = sqlx::query(AssertSqlSafe(partitioned_candidates(
        statement,
        field_probe,
    )))
    .bind(actor.company_id.as_str());
    if let Some(node) = node {
        stmt = stmt.bind(node);
    }
    let stmt = stmt
        .bind(&actor.modules)
        .bind(query.term())
        .bind(MAX_SCAN as i64);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executed_sql_is_owner_local_and_parameterised() {
        for sql in [HOLDING_SQL, BRANCH_SQL, DEPARTMENT_SQL] {
            assert!(
                sql.starts_with("SELECT module_id,id,title,status,version FROM workspace_records")
            );
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
    fn field_sources_are_excluded_before_the_candidate_limit_in_every_scope() {
        const PROBE: &str = "SELECT 1 WHERE workspace_records.id='field-source'";
        for statement in [HOLDING_SQL, BRANCH_SQL, DEPARTMENT_SQL] {
            let sql = partitioned_candidates(statement, PROBE);
            let predicate =
                format!("workspace_records.module_id<>'field_service' AND NOT EXISTS({PROBE})");
            assert!(sql.contains(&format!("{predicate} ORDER BY")));
            assert_eq!(sql.matches('$').count(), statement.matches('$').count());
        }
    }

    #[test]
    fn scan_bound_is_finite() {
        assert!(MAX_SCAN >= MAX_SCAN.min(1));
        assert!(MAX_SCAN <= 1_000);
    }

    #[test]
    fn candidate_bind_arity_and_term_reuse_are_exact() {
        assert!(HOLDING_SQL.ends_with("LIMIT $4"));
        assert_eq!(HOLDING_SQL.matches("ILIKE $3").count(), 2);
        assert!(!HOLDING_SQL.contains("$5"));
        for sql in [BRANCH_SQL, DEPARTMENT_SQL] {
            assert!(sql.ends_with("LIMIT $5"));
            assert_eq!(sql.matches("ILIKE $4").count(), 2);
            assert!(!sql.contains("$6"));
        }
    }
}
