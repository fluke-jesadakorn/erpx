//! Agent-owned audit-read SQL. Only `ai_invocation_evidence`,
//! `ai_private_sessions`, `ai_private_session_controls` and their catalog
//! references are read here. Workflow, Files, Records, IAM and provider facts
//! stay behind their own public owner boundaries.
use crate::application::agent_evidence::{AgentEvidenceFuture, AgentEvidenceRepository};
use crate::domain::agent_evidence::{
    AgentInvocationEvidence, AgentPrivateSession, INVOCATION_LIMIT, SESSION_LIMIT,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection, Row, postgres::PgRow};

type Error = PgTransactionError<BusinessError>;

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("AGENT_EVIDENCE_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("AGENT_EVIDENCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub async fn lock_agent_evidence_sources(connection: &mut PgConnection) -> Result<(), Error> {
    serializable(connection).await?;
    sqlx::query(
        "LOCK TABLE ai_invocation_evidence,ai_private_sessions,ai_private_session_controls IN SHARE MODE",
    )
    .execute(connection)
    .await?;
    Ok(())
}

fn invocation_row(row: &PgRow) -> Result<AgentInvocationEvidence, Error> {
    Ok(AgentInvocationEvidence {
        id: row.try_get("id")?,
        session_id: row.try_get("session_id")?,
        turn_id: row.try_get("turn_id")?,
        company_id: row.try_get("company_id")?,
        user_id: row.try_get("user_id")?,
        agent_version_id: row.try_get("agent_version_id")?,
        question_digest: row.try_get("question_digest")?,
        sources_digest: row.try_get("sources_digest")?,
        outcome: row.try_get("outcome")?,
        input_tokens: row.try_get("input_tokens")?,
        output_tokens: row.try_get("output_tokens")?,
        error: row.try_get("error")?,
        created_at: row.try_get("created_at")?,
    })
}

fn session_row(row: &PgRow) -> Result<AgentPrivateSession, Error> {
    Ok(AgentPrivateSession {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        user_id: row.try_get("user_id")?,
        assignment_id: row.try_get("assignment_id")?,
        conversation_key: row.try_get("conversation_key")?,
        generation: row.try_get("generation")?,
        created_at: row.try_get("created_at")?,
        status: row.try_get("status")?,
        expires_at: row.try_get("expires_at")?,
        legal_hold: row.try_get("legal_hold")?,
        hold_reason: row.try_get("hold_reason")?,
        version: row.try_get("version")?,
        last_used_at: row.try_get("last_used_at")?,
        updated_at: row.try_get("updated_at")?,
        effective_status: String::new(),
    })
}

pub async fn read_invocations(
    connection: &mut PgConnection,
    company_id: &str,
) -> Result<Vec<AgentInvocationEvidence>, Error> {
    serializable(connection).await?;
    let rows = sqlx::query(
        "SELECT id,session_id,turn_id,company_id,user_id,agent_version_id,question_digest,\
         sources_digest,outcome,input_tokens,output_tokens,error,created_at \
         FROM ai_invocation_evidence WHERE company_id=$1 \
         ORDER BY created_at DESC,id ASC LIMIT $2",
    )
    .bind(company_id)
    .bind(INVOCATION_LIMIT as i64)
    .fetch_all(&mut *connection)
    .await?;
    rows.iter().map(invocation_row).collect()
}

pub async fn read_sessions(
    connection: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    manage: bool,
) -> Result<Vec<AgentPrivateSession>, Error> {
    serializable(connection).await?;
    let rows = sqlx::query(
        "SELECT s.id,s.company_id,s.user_id,s.assignment_id,s.conversation_key,s.generation,\
         s.created_at,c.status,c.expires_at,c.legal_hold,c.hold_reason,c.version,c.last_used_at,\
         c.updated_at \
         FROM ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id \
         WHERE s.company_id=$1 AND ($3 OR s.user_id=$2) \
         ORDER BY c.updated_at DESC,s.id ASC LIMIT $4",
    )
    .bind(company_id)
    .bind(user_id)
    .bind(manage)
    .bind(SESSION_LIMIT as i64)
    .fetch_all(&mut *connection)
    .await?;
    rows.iter().map(session_row).collect()
}

pub async fn read_invocation(
    connection: &mut PgConnection,
    company_id: &str,
    id: &str,
) -> Result<Option<AgentInvocationEvidence>, Error> {
    serializable(connection).await?;
    let row = sqlx::query(
        "SELECT id,session_id,turn_id,company_id,user_id,agent_version_id,question_digest,\
         sources_digest,outcome,input_tokens,output_tokens,error,created_at \
         FROM ai_invocation_evidence WHERE company_id=$1 AND id=$2",
    )
    .bind(company_id)
    .bind(id)
    .fetch_optional(&mut *connection)
    .await?;
    row.as_ref().map(invocation_row).transpose()
}

pub async fn read_session(
    connection: &mut PgConnection,
    company_id: &str,
    id: &str,
    user_id: &str,
    manage: bool,
) -> Result<Option<AgentPrivateSession>, Error> {
    serializable(connection).await?;
    let row = sqlx::query(
        "SELECT s.id,s.company_id,s.user_id,s.assignment_id,s.conversation_key,s.generation,\
         s.created_at,c.status,c.expires_at,c.legal_hold,c.hold_reason,c.version,c.last_used_at,\
         c.updated_at \
         FROM ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id \
         WHERE s.company_id=$1 AND s.id=$2 AND ($4 OR s.user_id=$3)",
    )
    .bind(company_id)
    .bind(id)
    .bind(user_id)
    .bind(manage)
    .fetch_optional(&mut *connection)
    .await?;
    row.as_ref().map(session_row).transpose()
}

/// SQL repository adapter bound to the caller's retained serializable unit.
pub struct PgAgentEvidenceRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgAgentEvidenceRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
    pub fn lock(&mut self) -> AgentEvidenceFuture<'_, (), Error> {
        Box::pin(async move { lock_agent_evidence_sources(self.connection).await })
    }
}

impl AgentEvidenceRepository for PgAgentEvidenceRead<'_> {
    type Error = Error;
    fn invocations<'a>(
        &'a mut self,
        company_id: &'a str,
    ) -> AgentEvidenceFuture<'a, Vec<AgentInvocationEvidence>, Self::Error> {
        Box::pin(async move { read_invocations(self.connection, company_id).await })
    }
    fn sessions<'a>(
        &'a mut self,
        company_id: &'a str,
        user_id: &'a str,
        manage: bool,
    ) -> AgentEvidenceFuture<'a, Vec<AgentPrivateSession>, Self::Error> {
        Box::pin(async move { read_sessions(self.connection, company_id, user_id, manage).await })
    }
    fn invocation<'a>(
        &'a mut self,
        company_id: &'a str,
        id: &'a str,
    ) -> AgentEvidenceFuture<'a, Option<AgentInvocationEvidence>, Self::Error> {
        Box::pin(async move { read_invocation(self.connection, company_id, id).await })
    }
    fn session<'a>(
        &'a mut self,
        company_id: &'a str,
        id: &'a str,
        user_id: &'a str,
        manage: bool,
    ) -> AgentEvidenceFuture<'a, Option<AgentPrivateSession>, Self::Error> {
        Box::pin(
            async move { read_session(self.connection, company_id, id, user_id, manage).await },
        )
    }
}
