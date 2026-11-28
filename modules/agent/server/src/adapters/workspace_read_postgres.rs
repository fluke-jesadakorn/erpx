//! PostgreSQL adapter for the native Agent workspace/evidence reads. Rows are
//! rendered by PostgreSQL itself with `row_to_json`, so column names and JSON
//! types stay identical to the retained reader; the application layer applies
//! privacy and performs the final validation. No write authority is present.
use crate::application::governance::AgentActor;
use crate::application::workspace_read::{AgentWorkspaceReadPorts, WorkspaceReadFuture};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;
use sqlx::{PgConnection, Row};

type Error = BusinessError;

const AGENTS: &str = "SELECT COALESCE(jsonb_agg(row_to_json(t) ORDER BY t.updated_at DESC), '[]'::jsonb) FROM (SELECT * FROM agent_catalog WHERE company_id=$1) t";
const VERSIONS: &str = "SELECT COALESCE(jsonb_agg(row_to_json(t) ORDER BY t.created_at DESC), '[]'::jsonb) FROM (SELECT v.*,a.name agent_name FROM agent_versions v JOIN agent_catalog a ON a.id=v.agent_id WHERE a.company_id=$1) t";
const POOLS: &str = "SELECT COALESCE(jsonb_agg(row_to_json(t) ORDER BY t.name), '[]'::jsonb) FROM (SELECT * FROM agent_pools WHERE company_id=$1) t";
const ASSIGNMENTS: &str = "SELECT COALESCE(jsonb_agg(row_to_json(t) ORDER BY t.updated_at DESC), '[]'::jsonb) FROM (SELECT x.*,v.version_number,a.name agent_name FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id JOIN agent_catalog a ON a.id=v.agent_id WHERE x.company_id=$1) t";
const INVOCATIONS: &str = "SELECT COALESCE(jsonb_agg(row_to_json(t) ORDER BY t.created_at DESC), '[]'::jsonb) FROM (SELECT * FROM ai_invocation_evidence WHERE company_id=$1 ORDER BY created_at DESC LIMIT 50) t";
const PEOPLE: &str = "SELECT COALESCE(jsonb_agg(row_to_json(t) ORDER BY t.name), '[]'::jsonb) FROM (SELECT DISTINCT x.user_id id,COALESCE(p.legal_name,x.user_id) name FROM assignments x LEFT JOIN hr_people p ON p.company_id=x.company_id AND p.user_id=x.user_id AND p.merged_into_id IS NULL WHERE x.company_id=$1 AND iam_assignment_active(x.id,$2) AND iam_principal_active(x.user_id,x.company_id,$3) ORDER BY 2) t";
const MEMBERS: &str = "SELECT COALESCE(jsonb_agg(row_to_json(t) ORDER BY t.pool_id,t.user_id), '[]'::jsonb) FROM (SELECT m.* FROM agent_pool_members m JOIN agent_pools p ON p.id=m.pool_id WHERE p.company_id=$1) t";
const POOL_HISTORY: &str = "SELECT COALESCE(jsonb_agg(row_to_json(t) ORDER BY t.created_at DESC), '[]'::jsonb) FROM (SELECT r.* FROM agent_pool_revisions r JOIN agent_pools p ON p.id=r.pool_id WHERE p.company_id=$1) t";
const SESSIONS: &str = "SELECT COALESCE(jsonb_agg(row_to_json(t) ORDER BY t.updated_at DESC), '[]'::jsonb) FROM (SELECT s.id,s.user_id,s.assignment_id,s.conversation_key,s.generation,s.created_at,c.* FROM ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id WHERE s.company_id=$1 AND (s.user_id=$2 OR $3) ORDER BY c.updated_at DESC LIMIT 100) t";
const INVOCATION: &str = "SELECT row_to_json(t) FROM (SELECT id,turn_id,company_id,user_id,agent_version_id,question_digest,sources_digest,outcome,input_tokens,output_tokens,error,created_at FROM ai_invocation_evidence WHERE company_id=$1 AND id=$2) t LIMIT 1";
const SESSION: &str = "SELECT row_to_json(t) FROM (SELECT s.id,s.user_id,s.assignment_id,s.conversation_key,s.generation,s.created_at,c.* FROM ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id WHERE s.company_id=$1 AND s.id=$2 AND (s.user_id=$3 OR $4)) t LIMIT 1";

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "AGENT_WORKSPACE_READ_UNAVAILABLE",
        "The Agent workspace read is unavailable.",
    )
}

fn rows(value: Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items,
        _ => vec![],
    }
}

/// One bounded read unit. Every read is bound to the actor's Company and user;
/// the caller supplies the connection and the committed clock.
pub struct PgAgentWorkspaceRead<'c> {
    connection: &'c mut PgConnection,
    actor: &'c AgentActor,
    now: i64,
}

impl<'c> PgAgentWorkspaceRead<'c> {
    pub fn new(connection: &'c mut PgConnection, actor: &'c AgentActor, now: i64) -> Self {
        Self {
            connection,
            actor,
            now,
        }
    }
}

impl AgentWorkspaceReadPorts for PgAgentWorkspaceRead<'_> {
    type Error = Error;

    fn actor(&self) -> &AgentActor {
        self.actor
    }
    fn now(&self) -> i64 {
        self.now
    }

    fn agents<'a>(&'a mut self, company: &'a str) -> WorkspaceReadFuture<'a, Vec<Value>, Error> {
        Box::pin(async move {
            let value = sqlx::query(AGENTS)
                .bind(company)
                .fetch_one(&mut *self.connection)
                .await
                .map_err(|_| unavailable())?
                .try_get::<Value, _>(0)
                .map_err(|_| unavailable())?;
            Ok(rows(value))
        })
    }

    fn versions<'a>(&'a mut self, company: &'a str) -> WorkspaceReadFuture<'a, Vec<Value>, Error> {
        Box::pin(async move {
            let value = sqlx::query(VERSIONS)
                .bind(company)
                .fetch_one(&mut *self.connection)
                .await
                .map_err(|_| unavailable())?
                .try_get::<Value, _>(0)
                .map_err(|_| unavailable())?;
            Ok(rows(value))
        })
    }

    fn pools<'a>(&'a mut self, company: &'a str) -> WorkspaceReadFuture<'a, Vec<Value>, Error> {
        Box::pin(async move {
            let value = sqlx::query(POOLS)
                .bind(company)
                .fetch_one(&mut *self.connection)
                .await
                .map_err(|_| unavailable())?
                .try_get::<Value, _>(0)
                .map_err(|_| unavailable())?;
            Ok(rows(value))
        })
    }

    fn assignments<'a>(
        &'a mut self,
        company: &'a str,
    ) -> WorkspaceReadFuture<'a, Vec<Value>, Error> {
        Box::pin(async move {
            let value = sqlx::query(ASSIGNMENTS)
                .bind(company)
                .fetch_one(&mut *self.connection)
                .await
                .map_err(|_| unavailable())?
                .try_get::<Value, _>(0)
                .map_err(|_| unavailable())?;
            Ok(rows(value))
        })
    }

    fn invocations<'a>(
        &'a mut self,
        company: &'a str,
    ) -> WorkspaceReadFuture<'a, Vec<Value>, Error> {
        Box::pin(async move {
            let value = sqlx::query(INVOCATIONS)
                .bind(company)
                .fetch_one(&mut *self.connection)
                .await
                .map_err(|_| unavailable())?
                .try_get::<Value, _>(0)
                .map_err(|_| unavailable())?;
            Ok(rows(value))
        })
    }

    fn people<'a>(
        &'a mut self,
        company: &'a str,
        now: i64,
    ) -> WorkspaceReadFuture<'a, Vec<Value>, Error> {
        Box::pin(async move {
            let value = sqlx::query(PEOPLE)
                .bind(company)
                .bind(now)
                .bind(now)
                .fetch_one(&mut *self.connection)
                .await
                .map_err(|_| unavailable())?
                .try_get::<Value, _>(0)
                .map_err(|_| unavailable())?;
            Ok(rows(value))
        })
    }

    fn members<'a>(&'a mut self, company: &'a str) -> WorkspaceReadFuture<'a, Vec<Value>, Error> {
        Box::pin(async move {
            let value = sqlx::query(MEMBERS)
                .bind(company)
                .fetch_one(&mut *self.connection)
                .await
                .map_err(|_| unavailable())?
                .try_get::<Value, _>(0)
                .map_err(|_| unavailable())?;
            Ok(rows(value))
        })
    }

    fn pool_history<'a>(
        &'a mut self,
        company: &'a str,
    ) -> WorkspaceReadFuture<'a, Vec<Value>, Error> {
        Box::pin(async move {
            let value = sqlx::query(POOL_HISTORY)
                .bind(company)
                .fetch_one(&mut *self.connection)
                .await
                .map_err(|_| unavailable())?
                .try_get::<Value, _>(0)
                .map_err(|_| unavailable())?;
            Ok(rows(value))
        })
    }

    fn sessions<'a>(
        &'a mut self,
        company: &'a str,
        user: &'a str,
        manage: bool,
    ) -> WorkspaceReadFuture<'a, Vec<Value>, Error> {
        Box::pin(async move {
            let value = sqlx::query(SESSIONS)
                .bind(company)
                .bind(user)
                .bind(manage)
                .fetch_one(&mut *self.connection)
                .await
                .map_err(|_| unavailable())?
                .try_get::<Value, _>(0)
                .map_err(|_| unavailable())?;
            Ok(rows(value))
        })
    }

    fn invocation<'a>(
        &'a mut self,
        company: &'a str,
        id: &'a str,
    ) -> WorkspaceReadFuture<'a, Option<Value>, Error> {
        Box::pin(async move {
            Ok(sqlx::query(INVOCATION)
                .bind(company)
                .bind(id)
                .fetch_optional(&mut *self.connection)
                .await
                .map_err(|_| unavailable())?
                .map(|row| row.try_get::<Value, _>(0))
                .transpose()
                .map_err(|_| unavailable())?)
        })
    }

    fn session<'a>(
        &'a mut self,
        company: &'a str,
        id: &'a str,
        user: &'a str,
        manage: bool,
    ) -> WorkspaceReadFuture<'a, Option<Value>, Error> {
        Box::pin(async move {
            Ok(sqlx::query(SESSION)
                .bind(company)
                .bind(id)
                .bind(user)
                .bind(manage)
                .fetch_optional(&mut *self.connection)
                .await
                .map_err(|_| unavailable())?
                .map(|row| row.try_get::<Value, _>(0))
                .transpose()
                .map_err(|_| unavailable())?)
        })
    }
}
