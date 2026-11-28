//! PostgreSQL adapter for the native Records reference-search read.
//!
//! This is the only place in the migrated path that holds SQL text for the
//! search family. It selects Records-owned `workspace_records` rows on the selected
//! Holding transaction; Company/branch/department scope, the module allowlist and
//! the search term are always bind parameters. Cross-owner row admission and
//! the control-side principal lookup are composition-owned ports.

use crate::domain::record_search::{MAX_SCAN, RecordHit, SearchActor, SearchQuery, unavailable};
use erpx_platform_shared_server::public::BusinessError;
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

/// Trusted owner SQL and selected actor facts assembled by composition.
pub struct RecordSearchGuards<'a> {
    pub field_probe: &'static str,
    pub finance_predicate: &'a str,
    pub assignment_role: &'a str,
    pub now_ms: i64,
}

fn partitioned_candidates(
    statement: &'static str,
    first_guard_bind: usize,
    guards: &RecordSearchGuards<'_>,
) -> String {
    let (condition, ordering) = statement
        .split_once(" ORDER BY ")
        .expect("fixed Records search statements include ordering");
    let (sources, predicates) = condition
        .split_once(" WHERE ")
        .expect("fixed Records search statements include predicates");
    let [assignment, user, role, clock] = std::array::from_fn(|offset| first_guard_bind + offset);
    format!(
        "WITH records_search_actor(assignment_id,user_id,role,now_ms) AS MATERIALIZED(\
         SELECT ${assignment}::text,${user}::text,${role}::text,${clock}::bigint) \
         {sources} CROSS JOIN records_search_actor WHERE {predicates} \
         AND workspace_records.module_id<>'field_service' AND NOT EXISTS({field_probe}) \
         AND ({finance}) ORDER BY {ordering}",
        field_probe = guards.field_probe,
        finance = guards.finance_predicate,
    )
}

/// Execute the Records-owned candidate read on the caller's Holding
/// connection. Scope, module allowlist and the single reused term stay bind
/// parameters; `LIMIT` remains the finite `MAX_SCAN` bind. The composition
/// supplies fixed owner predicates and selected actor binds before candidates are limited.
pub async fn read_search_candidates(
    connection: &mut PgConnection,
    actor: &SearchActor,
    query: &SearchQuery,
    guards: &RecordSearchGuards<'_>,
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
        if node.is_some() { 6 } else { 5 },
        guards,
    )))
    .bind(actor.company_id.as_str());
    if let Some(node) = node {
        stmt = stmt.bind(node);
    }
    let stmt = stmt
        .bind(&actor.modules)
        .bind(query.term())
        .bind(MAX_SCAN as i64)
        .bind(actor.assignment_id.as_str())
        .bind(actor.user_id.as_str())
        .bind(guards.assignment_role)
        .bind(guards.now_ms);
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
    fn owner_guards_are_applied_before_the_candidate_limit_in_every_scope() {
        const PROBE: &str = "SELECT 1 WHERE workspace_records.id='field-source'";
        let guards = RecordSearchGuards {
            field_probe: PROBE,
            finance_predicate: "workspace_records.module_id<>'finance'",
            assignment_role: "worker",
            now_ms: 1,
        };
        for (statement, first_guard_bind) in
            [(HOLDING_SQL, 5), (BRANCH_SQL, 6), (DEPARTMENT_SQL, 6)]
        {
            let sql = partitioned_candidates(statement, first_guard_bind, &guards);
            let filter = sql.split_once(" ORDER BY ").unwrap().0;
            assert!(
                filter.contains("FROM workspace_records CROSS JOIN records_search_actor WHERE")
            );
            assert!(filter.contains(&format!(
                "workspace_records.module_id<>'field_service' AND NOT EXISTS({PROBE})"
            )));
            assert!(filter.contains(guards.finance_predicate));
            assert!(filter.contains(&format!("SELECT ${first_guard_bind}::text,")));
            assert!(filter.contains(&format!("${}::bigint)", first_guard_bind + 3)));
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
