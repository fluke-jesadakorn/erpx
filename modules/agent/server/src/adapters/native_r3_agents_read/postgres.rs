//! PostgreSQL execution for the native R3 Agents read. The nine statements are
//! byte-identical to the retained owner reader (`workspace_read_postgres.rs`);
//! only this module's call site and the mounted path are new. Rows are rendered
//! by PostgreSQL itself with `row_to_json`, so column names and JSON types stay
//! identical to the TypeScript reader. No write, advisory lock or cross-owner
//! projection is present.

use super::application::{AgentsRead, AgentsReadFuture, AgentsReadRequest, render_agents_payload};
use super::domain::{AgentsFacts, unavailable};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::Value;
use sqlx::Row;

const AGENTS: &str = "SELECT COALESCE(jsonb_agg(row_to_json(t) ORDER BY t.updated_at DESC), '[]'::jsonb) FROM (SELECT * FROM agent_catalog WHERE company_id=$1) t";
const VERSIONS: &str = "SELECT COALESCE(jsonb_agg(row_to_json(t) ORDER BY t.created_at DESC), '[]'::jsonb) FROM (SELECT v.*,a.name agent_name FROM agent_versions v JOIN agent_catalog a ON a.id=v.agent_id WHERE a.company_id=$1) t";
const POOLS: &str = "SELECT COALESCE(jsonb_agg(row_to_json(t) ORDER BY t.name), '[]'::jsonb) FROM (SELECT * FROM agent_pools WHERE company_id=$1) t";
const ASSIGNMENTS: &str = "SELECT COALESCE(jsonb_agg(row_to_json(t) ORDER BY t.updated_at DESC), '[]'::jsonb) FROM (SELECT x.*,v.version_number,a.name agent_name FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id JOIN agent_catalog a ON a.id=v.agent_id WHERE x.company_id=$1) t";
const INVOCATIONS: &str = "SELECT COALESCE(jsonb_agg(row_to_json(t) ORDER BY t.created_at DESC), '[]'::jsonb) FROM (SELECT * FROM ai_invocation_evidence WHERE company_id=$1 ORDER BY created_at DESC LIMIT 50) t";
const PEOPLE: &str = "SELECT COALESCE(jsonb_agg(row_to_json(t) ORDER BY t.name), '[]'::jsonb) FROM (SELECT DISTINCT x.user_id id,COALESCE(p.legal_name,x.user_id) name FROM assignments x LEFT JOIN hr_people p ON p.company_id=x.company_id AND p.user_id=x.user_id AND p.merged_into_id IS NULL WHERE x.company_id=$1 AND iam_assignment_active(x.id,$2) AND iam_principal_active(x.user_id,x.company_id,$3) ORDER BY 2) t";
const MEMBERS: &str = "SELECT COALESCE(jsonb_agg(row_to_json(t) ORDER BY t.pool_id,t.user_id), '[]'::jsonb) FROM (SELECT m.* FROM agent_pool_members m JOIN agent_pools p ON p.id=m.pool_id WHERE p.company_id=$1) t";
const POOL_HISTORY: &str = "SELECT COALESCE(jsonb_agg(row_to_json(t) ORDER BY t.created_at DESC), '[]'::jsonb) FROM (SELECT r.* FROM agent_pool_revisions r JOIN agent_pools p ON p.id=r.pool_id WHERE p.company_id=$1) t";
const SESSIONS: &str = "SELECT COALESCE(jsonb_agg(row_to_json(t) ORDER BY t.updated_at DESC), '[]'::jsonb) FROM (SELECT s.id,s.user_id,s.assignment_id,s.conversation_key,s.generation,s.created_at,c.* FROM ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id WHERE s.company_id=$1 AND (s.user_id=$2 OR $3) ORDER BY c.updated_at DESC LIMIT 100) t";

fn rows(value: Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items,
        _ => vec![],
    }
}

async fn aggregate(
    connection: &mut sqlx::PgConnection,
    sql: &'static str,
    binds: &[&str],
) -> Result<Vec<Value>, BusinessError> {
    let mut query = sqlx::query(sql);
    for bind in binds {
        query = query.bind(*bind);
    }
    let value = query
        .fetch_one(&mut *connection)
        .await
        .map_err(|_| unavailable())?
        .try_get::<Value, _>(0)
        .map_err(|_| unavailable())?;
    Ok(rows(value))
}

/// Executes one bounded read unit against the caller's injected pool.
pub struct PgAgentsRead {
    pool: DatabasePool,
}

impl PgAgentsRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl AgentsRead for PgAgentsRead {
    fn read<'a>(&'a self, request: AgentsReadRequest<'a>) -> AgentsReadFuture<'a, Value> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = request.company.to_owned();
        let user = request.user.to_owned();
        let now = request.now;
        let manage = request.manage;
        let evidence_view = request.evidence_view;
        Box::pin(async move {
            let result = unit
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let company = company.clone();
                    let user = user.clone();
                    Box::pin(async move {
                        let agents = aggregate(connection, AGENTS, &[&company]).await?;
                        let versions = aggregate(connection, VERSIONS, &[&company]).await?;
                        let pools = aggregate(connection, POOLS, &[&company]).await?;
                        let assignments = aggregate(connection, ASSIGNMENTS, &[&company]).await?;
                        let people = aggregate(connection, PEOPLE, &[&company]).await?;
                        let members = aggregate(connection, MEMBERS, &[&company]).await?;
                        let pool_history = aggregate(connection, POOL_HISTORY, &[&company]).await?;
                        let (invocations, sessions) = if evidence_view {
                            let invocations =
                                aggregate(connection, INVOCATIONS, &[&company]).await?;
                            let value = sqlx::query(SESSIONS)
                                .bind(&company)
                                .bind(&user)
                                .bind(manage)
                                .fetch_one(&mut *connection)
                                .await
                                .map_err(|_| unavailable())?
                                .try_get::<Value, _>(0)
                                .map_err(|_| unavailable())?;
                            (invocations, rows(value))
                        } else {
                            (vec![], vec![])
                        };
                        Ok(AgentsFacts {
                            agents,
                            versions,
                            pools,
                            assignments,
                            invocations,
                            people,
                            members,
                            pool_history,
                            sessions,
                        })
                    })
                })
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    PgTransactionError::Persistence(_) => unavailable(),
                })?;
            render_agents_payload(result, now, evidence_view)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statements_are_read_only_and_bind_the_retained_counts() {
        for (sql, count) in [
            (AGENTS, 1),
            (VERSIONS, 1),
            (POOLS, 1),
            (ASSIGNMENTS, 1),
            (INVOCATIONS, 1),
            (PEOPLE, 3),
            (MEMBERS, 1),
            (POOL_HISTORY, 1),
            (SESSIONS, 3),
        ] {
            assert!(sql.starts_with("SELECT "), "{sql}");
            assert_eq!(sql.matches("$1").count(), 1, "{sql}");
            assert!(!sql.contains('?'), "{sql}");
            assert!(!sql.contains("INSERT"), "{sql}");
            assert!(!sql.contains("UPDATE"), "{sql}");
            assert!(!sql.contains("DELETE"), "{sql}");
            assert!(!sql.contains("pg_advisory"), "{sql}");
            assert!(!sql.contains("FOR UPDATE"), "{sql}");
            if count > 1 {
                assert_eq!(sql.matches("$2").count(), 1, "{sql}");
            }
            if count > 2 {
                assert_eq!(sql.matches("$3").count(), 1, "{sql}");
            }
        }
    }
}
