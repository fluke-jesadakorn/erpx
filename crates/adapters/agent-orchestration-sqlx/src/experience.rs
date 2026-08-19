use std::collections::BTreeSet;

use async_trait::async_trait;
use erpx_agent_orchestration::{
    AgentExperienceQueryPort, AgentExperienceRepository, AgentExperienceUnitOfWork, AgentPool,
    AgentPoolId, AgentSession, AgentSessionId, AgentVersionId, InvocationEvidenceCommit,
    InvocationEvidenceSummary, PoolCandidate, PoolCandidatePort, PoolCommit, PoolRoutingCommit,
    PoolRoutingDecision, PoolRoutingDecisionId, PoolRoutingSummary, PortError, PrincipalKind,
    ProviderInvocationEvidence, ProviderInvocationId, ProviderInvocationOutcome, SessionCommit,
    SessionContext, SessionMessage, SessionMessageId,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, AuthorityScope, DataClassification, RecordId,
};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{holding::HoldingBoundPool, normalized};

/// Holding-bound authoritative pool/session/provenance store. Every operation
/// starts through the verified RLS-binding transaction boundary.
#[derive(Debug, Clone)]
pub struct SqlxAgentExperienceStore {
    database: HoldingBoundPool,
}

/// Holding-bound CQRS query adapter over disposable projections only.
#[derive(Debug, Clone)]
pub struct SqlxAgentExperienceQuery {
    database: HoldingBoundPool,
}

impl SqlxAgentExperienceStore {
    pub fn try_new(pool: sqlx::PgPool, holding_id: Uuid) -> Result<Self, PortError> {
        Ok(Self {
            database: HoldingBoundPool::try_new(pool, holding_id)?,
        })
    }
}

impl SqlxAgentExperienceQuery {
    pub fn try_new(pool: sqlx::PgPool, holding_id: Uuid) -> Result<Self, PortError> {
        Ok(Self {
            database: HoldingBoundPool::try_new(pool, holding_id)?,
        })
    }
}

#[async_trait]
impl AgentExperienceRepository for SqlxAgentExperienceStore {
    async fn load_pool(
        &self,
        authority_scope: &AuthorityScope,
        id: AgentPoolId,
    ) -> Result<Option<AgentPool>, PortError> {
        let holding_id = self.database.holding_id();
        let mut tx = self.database.begin().await?;
        let row = sqlx::query(
            "SELECT active_scope,pool_name,pool_version,enabled,created_at,updated_at
             FROM erpx_agent_write.agent_pools
             WHERE holding_id=$1 AND agent_pool_id=$2",
        )
        .bind(holding_id)
        .bind(id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        let Some(row) = row else {
            tx.commit().await.map_err(|_| PortError::Persistence)?;
            return Ok(None);
        };
        let scope: AuthorityScope = serde_json::from_value(
            row.try_get("active_scope")
                .map_err(|_| PortError::Persistence)?,
        )
        .map_err(|_| PortError::Persistence)?;
        if !authority_scope.contains(&scope) {
            return Err(PortError::Denied);
        }
        let member_ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT agent_version_id FROM erpx_agent_write.agent_pool_members
             WHERE holding_id=$1 AND agent_pool_id=$2 ORDER BY agent_version_id",
        )
        .bind(holding_id)
        .bind(id.as_uuid())
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        let members = member_ids
            .into_iter()
            .map(AgentVersionId::try_from_uuid)
            .collect::<Result<BTreeSet<_>, _>>()
            .map_err(|_| PortError::Persistence)?;
        AgentPool::rehydrate(
            id,
            scope,
            row.try_get("pool_name")
                .map_err(|_| PortError::Persistence)?,
            members,
            row.try_get("enabled").map_err(|_| PortError::Persistence)?,
            positive_u64(
                row.try_get("pool_version")
                    .map_err(|_| PortError::Persistence)?,
            )?,
            row.try_get("created_at")
                .map_err(|_| PortError::Persistence)?,
            row.try_get("updated_at")
                .map_err(|_| PortError::Persistence)?,
        )
        .map(Some)
        .map_err(|_| PortError::Persistence)
    }

    async fn load_session(
        &self,
        authority_scope: &AuthorityScope,
        id: AgentSessionId,
    ) -> Result<Option<AgentSession>, PortError> {
        let holding_id = self.database.holding_id();
        let mut tx = self.database.begin().await?;
        let row = sqlx::query(
            "SELECT active_scope,owner_human_ref,record_id,descriptor_version,
                    source_event_position,expires_at,legal_hold,expired_at,
                    session_version,created_at,updated_at
             FROM erpx_agent_write.agent_sessions
             WHERE holding_id=$1 AND agent_session_id=$2",
        )
        .bind(holding_id)
        .bind(id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        let Some(row) = row else {
            tx.commit().await.map_err(|_| PortError::Persistence)?;
            return Ok(None);
        };
        let scope: AuthorityScope = serde_json::from_value(
            row.try_get("active_scope")
                .map_err(|_| PortError::Persistence)?,
        )
        .map_err(|_| PortError::Persistence)?;
        if !authority_scope.contains(&scope) {
            return Err(PortError::Denied);
        }
        let message_rows = sqlx::query(
            "SELECT session_message_id,author_kind,author_ref,encrypted_payload_ref,
                    content_digest,message_version,created_at,published_at,
                    publishing_approver_human_ref,publication_anchor_ref
             FROM erpx_agent_write.agent_session_messages
             WHERE holding_id=$1 AND agent_session_id=$2
             ORDER BY created_at,session_message_id",
        )
        .bind(holding_id)
        .bind(id.as_uuid())
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        let record_id: Option<Uuid> = row
            .try_get("record_id")
            .map_err(|_| PortError::Persistence)?;
        let context = record_id
            .map(|record_id| {
                Ok(SessionContext {
                    record_id: RecordId::try_from_uuid(record_id)
                        .map_err(|_| PortError::Persistence)?,
                    descriptor_version: positive_u64(
                        row.try_get("descriptor_version")
                            .map_err(|_| PortError::Persistence)?,
                    )?,
                    source_event_position: positive_u64(
                        row.try_get("source_event_position")
                            .map_err(|_| PortError::Persistence)?,
                    )?,
                })
            })
            .transpose()?;
        let messages = message_rows
            .into_iter()
            .map(|message| {
                Ok(SessionMessage {
                    id: SessionMessageId::try_from_uuid(
                        message
                            .try_get("session_message_id")
                            .map_err(|_| PortError::Persistence)?,
                    )
                    .map_err(|_| PortError::Persistence)?,
                    author_kind: parse_author_kind(
                        message
                            .try_get("author_kind")
                            .map_err(|_| PortError::Persistence)?,
                    )?,
                    author_ref: message
                        .try_get("author_ref")
                        .map_err(|_| PortError::Persistence)?,
                    encrypted_payload_ref: message
                        .try_get("encrypted_payload_ref")
                        .map_err(|_| PortError::Persistence)?,
                    content_digest: message
                        .try_get("content_digest")
                        .map_err(|_| PortError::Persistence)?,
                    version: positive_u64(
                        message
                            .try_get("message_version")
                            .map_err(|_| PortError::Persistence)?,
                    )?,
                    created_at: message
                        .try_get("created_at")
                        .map_err(|_| PortError::Persistence)?,
                    published_at: message
                        .try_get("published_at")
                        .map_err(|_| PortError::Persistence)?,
                    publishing_approver_human_ref: message
                        .try_get("publishing_approver_human_ref")
                        .map_err(|_| PortError::Persistence)?,
                    publication_anchor_ref: message
                        .try_get("publication_anchor_ref")
                        .map_err(|_| PortError::Persistence)?,
                })
            })
            .collect::<Result<Vec<_>, PortError>>()?;
        AgentSession::rehydrate(
            id,
            scope,
            row.try_get("owner_human_ref")
                .map_err(|_| PortError::Persistence)?,
            context,
            messages,
            row.try_get("expires_at")
                .map_err(|_| PortError::Persistence)?,
            row.try_get("legal_hold")
                .map_err(|_| PortError::Persistence)?,
            row.try_get("expired_at")
                .map_err(|_| PortError::Persistence)?,
            positive_u64(
                row.try_get("session_version")
                    .map_err(|_| PortError::Persistence)?,
            )?,
            row.try_get("created_at")
                .map_err(|_| PortError::Persistence)?,
            row.try_get("updated_at")
                .map_err(|_| PortError::Persistence)?,
        )
        .map(Some)
        .map_err(|_| PortError::Persistence)
    }
}

/// Expire exactly one due private session under the caller's already bound,
/// trusted maintenance transaction. The domain aggregate decides retention;
/// the adapter deletes only unpublished encrypted references and commits the
/// receipt, audit fact, and opaque projection event atomically.
pub(crate) async fn expire_one_due_session(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: Uuid,
    at: chrono::DateTime<chrono::Utc>,
) -> Result<bool, PortError> {
    let id: Option<Uuid> = sqlx::query_scalar(
        "SELECT agent_session_id FROM erpx_agent_write.agent_sessions
         WHERE holding_id=$1 AND expired_at IS NULL AND legal_hold=FALSE AND expires_at<=$2
         ORDER BY expires_at,agent_session_id LIMIT 1 FOR UPDATE SKIP LOCKED",
    )
    .bind(holding_id)
    .bind(at)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    let Some(id) = id else { return Ok(false) };
    let id = AgentSessionId::try_from_uuid(id).map_err(|_| PortError::Persistence)?;
    let mut session = load_session_in_transaction(tx, holding_id, id)
        .await?
        .ok_or(PortError::Persistence)?;
    let previous_version = session.version();
    session.expire(at).map_err(|_| PortError::VersionConflict)?;
    let updated = sqlx::query(
        "UPDATE erpx_agent_write.agent_sessions
         SET expired_at=$3,session_version=$4,updated_at=$3
         WHERE holding_id=$1 AND agent_session_id=$2 AND expired_at IS NULL
           AND legal_hold=FALSE AND session_version=$5",
    )
    .bind(holding_id)
    .bind(id.as_uuid())
    .bind(at)
    .bind(to_i64(session.version())?)
    .bind(to_i64(previous_version)?)
    .execute(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    if updated.rows_affected() != 1 {
        return Err(PortError::VersionConflict);
    }
    sqlx::query(
        "DELETE FROM erpx_agent_write.agent_session_messages
         WHERE holding_id=$1 AND agent_session_id=$2 AND published_at IS NULL",
    )
    .bind(holding_id)
    .bind(id.as_uuid())
    .execute(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    let idempotency_key = format!(
        "governance:expire-session:{}:{previous_version}",
        id.as_uuid()
    );
    let maintenance_actor = ActorAttribution {
        principal_ref: "system:agent-governance-maintenance".into(),
        kind: ActorKind::SystemWorker,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    };
    if let Some(draft) = crate::publication::published_session_directory_draft(&session)? {
        crate::publication::append_with_idempotency(
            tx,
            holding_id,
            &idempotency_key,
            serde_json::to_value(&maintenance_actor).map_err(|_| PortError::Persistence)?,
            at,
            &draft,
        )
        .await?;
    }
    finish_commit(
        tx,
        holding_id,
        &scope_key(session.scope()),
        &idempotency_key,
        &object_digest(&session)?,
        "agent_session",
        id.as_uuid(),
        session.version(),
        session.scope(),
        &maintenance_actor,
        &AuditContext {
            purpose: "expire-private-agent-session".into(),
            customer_visible: false,
            module: Some("agent-orchestration".into()),
        },
        "agent.session.changed",
        at,
    )
    .await?;
    Ok(true)
}

async fn load_session_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: Uuid,
    id: AgentSessionId,
) -> Result<Option<AgentSession>, PortError> {
    let row = sqlx::query(
        "SELECT active_scope,owner_human_ref,record_id,descriptor_version,
                source_event_position,expires_at,legal_hold,expired_at,
                session_version,created_at,updated_at
         FROM erpx_agent_write.agent_sessions
         WHERE holding_id=$1 AND agent_session_id=$2",
    )
    .bind(holding_id)
    .bind(id.as_uuid())
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    let Some(row) = row else { return Ok(None) };
    let message_rows = sqlx::query(
        "SELECT session_message_id,author_kind,author_ref,encrypted_payload_ref,
                content_digest,message_version,created_at,published_at,
                publishing_approver_human_ref,publication_anchor_ref
         FROM erpx_agent_write.agent_session_messages
         WHERE holding_id=$1 AND agent_session_id=$2 ORDER BY created_at,session_message_id",
    )
    .bind(holding_id)
    .bind(id.as_uuid())
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    let scope: AuthorityScope = serde_json::from_value(
        row.try_get("active_scope")
            .map_err(|_| PortError::Persistence)?,
    )
    .map_err(|_| PortError::Persistence)?;
    let record_id: Option<Uuid> = row
        .try_get("record_id")
        .map_err(|_| PortError::Persistence)?;
    let context = record_id
        .map(|record_id| {
            Ok(SessionContext {
                record_id: RecordId::try_from_uuid(record_id)
                    .map_err(|_| PortError::Persistence)?,
                descriptor_version: positive_u64(
                    row.try_get("descriptor_version")
                        .map_err(|_| PortError::Persistence)?,
                )?,
                source_event_position: positive_u64(
                    row.try_get("source_event_position")
                        .map_err(|_| PortError::Persistence)?,
                )?,
            })
        })
        .transpose()?;
    let messages = message_rows
        .into_iter()
        .map(|message| {
            Ok(SessionMessage {
                id: SessionMessageId::try_from_uuid(
                    message
                        .try_get("session_message_id")
                        .map_err(|_| PortError::Persistence)?,
                )
                .map_err(|_| PortError::Persistence)?,
                author_kind: parse_author_kind(
                    message
                        .try_get("author_kind")
                        .map_err(|_| PortError::Persistence)?,
                )?,
                author_ref: message
                    .try_get("author_ref")
                    .map_err(|_| PortError::Persistence)?,
                encrypted_payload_ref: message
                    .try_get("encrypted_payload_ref")
                    .map_err(|_| PortError::Persistence)?,
                content_digest: message
                    .try_get("content_digest")
                    .map_err(|_| PortError::Persistence)?,
                version: positive_u64(
                    message
                        .try_get("message_version")
                        .map_err(|_| PortError::Persistence)?,
                )?,
                created_at: message
                    .try_get("created_at")
                    .map_err(|_| PortError::Persistence)?,
                published_at: message
                    .try_get("published_at")
                    .map_err(|_| PortError::Persistence)?,
                publishing_approver_human_ref: message
                    .try_get("publishing_approver_human_ref")
                    .map_err(|_| PortError::Persistence)?,
                publication_anchor_ref: message
                    .try_get("publication_anchor_ref")
                    .map_err(|_| PortError::Persistence)?,
            })
        })
        .collect::<Result<Vec<_>, PortError>>()?;
    AgentSession::rehydrate(
        id,
        scope,
        row.try_get("owner_human_ref")
            .map_err(|_| PortError::Persistence)?,
        context,
        messages,
        row.try_get("expires_at")
            .map_err(|_| PortError::Persistence)?,
        row.try_get("legal_hold")
            .map_err(|_| PortError::Persistence)?,
        row.try_get("expired_at")
            .map_err(|_| PortError::Persistence)?,
        positive_u64(
            row.try_get("session_version")
                .map_err(|_| PortError::Persistence)?,
        )?,
        row.try_get("created_at")
            .map_err(|_| PortError::Persistence)?,
        row.try_get("updated_at")
            .map_err(|_| PortError::Persistence)?,
    )
    .map(Some)
    .map_err(|_| PortError::Persistence)
}

#[async_trait]
impl PoolCandidatePort for SqlxAgentExperienceStore {
    async fn eligible_candidates(
        &self,
        authority_scope: &AuthorityScope,
        pool: &AgentPool,
        capability: &str,
        requested_scope: &AuthorityScope,
    ) -> Result<Vec<PoolCandidate>, PortError> {
        if !authority_scope.contains(pool.scope())
            || !authority_scope.contains(requested_scope)
            || capability.trim().is_empty()
            || capability.len() > 128
        {
            return Err(PortError::Denied);
        }
        let holding_id = self.database.holding_id();
        let member_ids = pool
            .member_versions()
            .iter()
            .map(|id| id.as_uuid())
            .collect::<Vec<_>>();
        let mut tx = self.database.begin().await?;
        let rows = sqlx::query(
            "SELECT v.agent_definition_id,v.agent_version_id,
                    COUNT(a.assignment_id) FILTER (WHERE a.assignment_status IN
                        ('queued','running','waiting')) AS active_assignments
             FROM erpx_agent_write.agent_versions v
             JOIN erpx_agent_write.agent_definitions d
               ON d.holding_id=v.holding_id AND d.agent_definition_id=v.agent_definition_id
             LEFT JOIN erpx_agent_write.agent_assignments a
               ON a.holding_id=v.holding_id
              AND a.agent_definition_id=v.agent_definition_id
              AND a.agent_version_id=v.agent_version_id
             WHERE v.holding_id=$1 AND v.agent_version_id=ANY($2)
               AND v.version_status='approved' AND d.definition_status='approved'
             GROUP BY v.agent_definition_id,v.agent_version_id
             ORDER BY v.agent_definition_id,v.agent_version_id",
        )
        .bind(holding_id)
        .bind(member_ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        let mut candidates = Vec::with_capacity(rows.len());
        for row in rows {
            let definition_id: Uuid = row
                .try_get("agent_definition_id")
                .map_err(|_| PortError::Persistence)?;
            let version_id = AgentVersionId::try_from_uuid(
                row.try_get("agent_version_id")
                    .map_err(|_| PortError::Persistence)?,
            )
            .map_err(|_| PortError::Persistence)?;
            let definition = normalized::load_definition(
                &mut tx,
                holding_id,
                erpx_agent_orchestration::AgentDefinitionId::try_from_uuid(definition_id)
                    .map_err(|_| PortError::Persistence)?,
            )
            .await?
            .ok_or(PortError::Persistence)?;
            if definition.id().as_uuid() != definition_id
                || !pool.member_versions().contains(&version_id)
            {
                return Err(PortError::Persistence);
            }
            let pin = definition
                .pin(version_id)
                .map_err(|_| PortError::Persistence)?;
            let active: i64 = row
                .try_get("active_assignments")
                .map_err(|_| PortError::Persistence)?;
            candidates.push(PoolCandidate {
                pin,
                active_assignments: u32::try_from(active).map_err(|_| PortError::Persistence)?,
            });
        }
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        Ok(candidates)
    }
}

#[async_trait]
impl AgentExperienceUnitOfWork for SqlxAgentExperienceStore {
    async fn commit_pool(&self, commit: PoolCommit) -> Result<AgentPool, PortError> {
        if !commit.authority_scope.contains(commit.pool.scope()) {
            return Err(PortError::Denied);
        }
        let digest = object_digest(&commit.pool)?;
        let scope_key = scope_key(commit.pool.scope());
        let holding_id = self.database.holding_id();
        let mut tx = self.database.begin().await?;
        if replay_receipt(
            &mut tx,
            holding_id,
            &scope_key,
            &commit.idempotency_key,
            &digest,
            commit.pool.id().as_uuid(),
        )
        .await?
        {
            tx.commit().await.map_err(|_| PortError::Persistence)?;
            return Ok(commit.pool);
        }
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT pool_version FROM erpx_agent_write.agent_pools
             WHERE holding_id=$1 AND agent_pool_id=$2 FOR UPDATE",
        )
        .bind(holding_id)
        .bind(commit.pool.id().as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        validate_version(commit.expected_version, current, commit.pool.version())?;
        if current.is_none() {
            sqlx::query(
                "INSERT INTO erpx_agent_write.agent_pools
                    (holding_id,agent_pool_id,scope_key,active_scope,pool_name,pool_version,
                     enabled,created_at,updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
            )
            .bind(holding_id)
            .bind(commit.pool.id().as_uuid())
            .bind(&scope_key)
            .bind(serde_json::to_value(commit.pool.scope()).map_err(|_| PortError::Persistence)?)
            .bind(commit.pool.name())
            .bind(to_i64(commit.pool.version())?)
            .bind(commit.pool.enabled())
            .bind(commit.pool.created_at())
            .bind(commit.pool.updated_at())
            .execute(&mut *tx)
            .await
            .map_err(|_| PortError::Persistence)?;
        } else {
            let updated = sqlx::query(
                "UPDATE erpx_agent_write.agent_pools SET pool_name=$3,pool_version=$4,
                    enabled=$5,updated_at=$6
                 WHERE holding_id=$1 AND agent_pool_id=$2 AND pool_version=$7",
            )
            .bind(holding_id)
            .bind(commit.pool.id().as_uuid())
            .bind(commit.pool.name())
            .bind(to_i64(commit.pool.version())?)
            .bind(commit.pool.enabled())
            .bind(commit.pool.updated_at())
            .bind(to_i64(
                commit.expected_version.ok_or(PortError::VersionConflict)?,
            )?)
            .execute(&mut *tx)
            .await
            .map_err(|_| PortError::Persistence)?;
            if updated.rows_affected() != 1 {
                return Err(PortError::VersionConflict);
            }
            sqlx::query(
                "DELETE FROM erpx_agent_write.agent_pool_members
                 WHERE holding_id=$1 AND agent_pool_id=$2",
            )
            .bind(holding_id)
            .bind(commit.pool.id().as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(|_| PortError::Persistence)?;
        }
        for member in commit.pool.member_versions() {
            sqlx::query(
                "INSERT INTO erpx_agent_write.agent_pool_members
                    (holding_id,agent_pool_id,agent_version_id) VALUES ($1,$2,$3)",
            )
            .bind(holding_id)
            .bind(commit.pool.id().as_uuid())
            .bind(member.as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(|_| PortError::Persistence)?;
        }
        finish_commit(
            &mut tx,
            holding_id,
            &scope_key,
            &commit.idempotency_key,
            &digest,
            "agent_pool",
            commit.pool.id().as_uuid(),
            commit.pool.version(),
            commit.pool.scope(),
            &commit.actor,
            &commit.audit,
            "agent.pool.changed",
            commit.pool.updated_at(),
        )
        .await?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        Ok(commit.pool)
    }

    async fn commit_pool_routing(
        &self,
        commit: PoolRoutingCommit,
    ) -> Result<PoolRoutingDecision, PortError> {
        if !commit
            .authority_scope
            .contains(&commit.decision.requested_scope)
        {
            return Err(PortError::Denied);
        }
        let digest = object_digest(&commit.decision)?;
        let scope_key = scope_key(&commit.decision.requested_scope);
        let holding_id = self.database.holding_id();
        let mut tx = self.database.begin().await?;
        if replay_receipt(
            &mut tx,
            holding_id,
            &scope_key,
            &commit.idempotency_key,
            &digest,
            commit.decision.id.as_uuid(),
        )
        .await?
        {
            tx.commit().await.map_err(|_| PortError::Persistence)?;
            return Ok(commit.decision);
        }
        sqlx::query(
            "INSERT INTO erpx_agent_write.agent_pool_routing_decisions
                (holding_id,routing_decision_id,agent_pool_id,pool_version,root_action_ref,
                 capability,requested_scope,candidate_version_ids,selected_agent_version_id,
                 selected_agent_version_ids,decision_digest,compare_requested,
                 previewed_cost_micros,initiating_human_ref,decided_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
        )
        .bind(holding_id)
        .bind(commit.decision.id.as_uuid())
        .bind(commit.decision.pool_id.as_uuid())
        .bind(to_i64(commit.decision.pool_version)?)
        .bind(&commit.decision.root_action_ref)
        .bind(&commit.decision.capability)
        .bind(
            serde_json::to_value(&commit.decision.requested_scope)
                .map_err(|_| PortError::Persistence)?,
        )
        .bind(
            commit
                .decision
                .candidate_versions
                .iter()
                .map(|id| id.as_uuid())
                .collect::<Vec<_>>(),
        )
        .bind(commit.decision.selected.manifest.version_id.as_uuid())
        .bind(
            commit
                .decision
                .selected_versions
                .iter()
                .map(|id| id.as_uuid())
                .collect::<Vec<_>>(),
        )
        .bind(&commit.decision.decision_digest)
        .bind(commit.decision.compare_requested)
        .bind(
            commit
                .decision
                .previewed_cost_micros
                .map(to_i64)
                .transpose()?,
        )
        .bind(&commit.decision.initiating_human_ref)
        .bind(commit.decision.decided_at)
        .execute(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        finish_commit(
            &mut tx,
            holding_id,
            &scope_key,
            &commit.idempotency_key,
            &digest,
            "pool_routing",
            commit.decision.id.as_uuid(),
            1,
            &commit.decision.requested_scope,
            &commit.actor,
            &commit.audit,
            "agent.pool.routed",
            commit.decision.decided_at,
        )
        .await?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        Ok(commit.decision)
    }

    async fn commit_session(&self, commit: SessionCommit) -> Result<AgentSession, PortError> {
        if !commit.authority_scope.contains(commit.session.scope()) {
            return Err(PortError::Denied);
        }
        let digest = object_digest(&commit.session)?;
        let scope_key = scope_key(commit.session.scope());
        let holding_id = self.database.holding_id();
        let mut tx = self.database.begin().await?;
        if replay_receipt(
            &mut tx,
            holding_id,
            &scope_key,
            &commit.idempotency_key,
            &digest,
            commit.session.id().as_uuid(),
        )
        .await?
        {
            tx.commit().await.map_err(|_| PortError::Persistence)?;
            return Ok(commit.session);
        }
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT session_version FROM erpx_agent_write.agent_sessions
             WHERE holding_id=$1 AND agent_session_id=$2 FOR UPDATE",
        )
        .bind(holding_id)
        .bind(commit.session.id().as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        validate_version(commit.expected_version, current, commit.session.version())?;
        let context = commit.session.context();
        if current.is_none() {
            sqlx::query(
                "INSERT INTO erpx_agent_write.agent_sessions
                    (holding_id,agent_session_id,scope_key,active_scope,owner_human_ref,
                     record_id,descriptor_version,source_event_position,expires_at,legal_hold,
                     expired_at,session_version,created_at,updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
            )
            .bind(holding_id)
            .bind(commit.session.id().as_uuid())
            .bind(&scope_key)
            .bind(serde_json::to_value(commit.session.scope()).map_err(|_| PortError::Persistence)?)
            .bind(commit.session.owner_human_ref())
            .bind(context.map(|context| context.record_id.as_uuid()))
            .bind(
                context
                    .map(|context| to_i64(context.descriptor_version))
                    .transpose()?,
            )
            .bind(
                context
                    .map(|context| to_i64(context.source_event_position))
                    .transpose()?,
            )
            .bind(commit.session.expires_at())
            .bind(commit.session.legal_hold())
            .bind(commit.session.expired_at())
            .bind(to_i64(commit.session.version())?)
            .bind(commit.session.created_at())
            .bind(commit.session.updated_at())
            .execute(&mut *tx)
            .await
            .map_err(|_| PortError::Persistence)?;
        } else {
            let updated = sqlx::query(
                "UPDATE erpx_agent_write.agent_sessions SET legal_hold=$3,expired_at=$4,
                    session_version=$5,updated_at=$6
                 WHERE holding_id=$1 AND agent_session_id=$2 AND session_version=$7",
            )
            .bind(holding_id)
            .bind(commit.session.id().as_uuid())
            .bind(commit.session.legal_hold())
            .bind(commit.session.expired_at())
            .bind(to_i64(commit.session.version())?)
            .bind(commit.session.updated_at())
            .bind(to_i64(
                commit.expected_version.ok_or(PortError::VersionConflict)?,
            )?)
            .execute(&mut *tx)
            .await
            .map_err(|_| PortError::Persistence)?;
            if updated.rows_affected() != 1 {
                return Err(PortError::VersionConflict);
            }
            let retained_ids = commit
                .session
                .messages()
                .iter()
                .map(|message| message.id.as_uuid())
                .collect::<Vec<_>>();
            sqlx::query(
                "DELETE FROM erpx_agent_write.agent_session_messages
                 WHERE holding_id=$1 AND agent_session_id=$2 AND published_at IS NULL
                   AND NOT (session_message_id = ANY($3))",
            )
            .bind(holding_id)
            .bind(commit.session.id().as_uuid())
            .bind(retained_ids)
            .execute(&mut *tx)
            .await
            .map_err(|_| PortError::Persistence)?;
        }
        for message in commit.session.messages() {
            sqlx::query(
                "INSERT INTO erpx_agent_write.agent_session_messages
                    (holding_id,session_message_id,agent_session_id,author_kind,author_ref,
                     encrypted_payload_ref,content_digest,message_version,created_at,published_at,
                     publishing_approver_human_ref,publication_anchor_ref)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
                 ON CONFLICT (holding_id,session_message_id) DO UPDATE SET
                    message_version=EXCLUDED.message_version,published_at=EXCLUDED.published_at,
                    publishing_approver_human_ref=EXCLUDED.publishing_approver_human_ref,
                    publication_anchor_ref=EXCLUDED.publication_anchor_ref
                 WHERE erpx_agent_write.agent_session_messages.message_version + 1
                    = EXCLUDED.message_version",
            )
            .bind(holding_id)
            .bind(message.id.as_uuid())
            .bind(commit.session.id().as_uuid())
            .bind(author_kind(message.author_kind))
            .bind(&message.author_ref)
            .bind(&message.encrypted_payload_ref)
            .bind(&message.content_digest)
            .bind(to_i64(message.version)?)
            .bind(message.created_at)
            .bind(message.published_at)
            .bind(&message.publishing_approver_human_ref)
            .bind(message.publication_anchor_ref)
            .execute(&mut *tx)
            .await
            .map_err(|_| PortError::Persistence)?;
        }
        if let Some(draft) = crate::publication::published_session_directory_draft(&commit.session)?
        {
            crate::publication::append_with_idempotency(
                &mut tx,
                holding_id,
                &commit.idempotency_key,
                serde_json::to_value(&commit.actor).map_err(|_| PortError::Persistence)?,
                commit.session.updated_at(),
                &draft,
            )
            .await?;
        }
        finish_commit(
            &mut tx,
            holding_id,
            &scope_key,
            &commit.idempotency_key,
            &digest,
            "agent_session",
            commit.session.id().as_uuid(),
            commit.session.version(),
            commit.session.scope(),
            &commit.actor,
            &commit.audit,
            "agent.session.changed",
            commit.session.updated_at(),
        )
        .await?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        Ok(commit.session)
    }

    async fn commit_invocation_evidence(
        &self,
        commit: InvocationEvidenceCommit,
    ) -> Result<ProviderInvocationEvidence, PortError> {
        let digest = object_digest(&commit.evidence)?;
        let holding_id = self.database.holding_id();
        let mut tx = self.database.begin().await?;
        let assignment_scope: serde_json::Value = sqlx::query_scalar(
            "SELECT active_scope FROM erpx_agent_write.agent_assignments
             WHERE holding_id=$1 AND assignment_id=$2 FOR SHARE",
        )
        .bind(holding_id)
        .bind(commit.evidence.assignment_id().as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?
        .ok_or(PortError::NotFound)?;
        let scope: AuthorityScope =
            serde_json::from_value(assignment_scope).map_err(|_| PortError::Persistence)?;
        if !commit.authority_scope.contains(&scope) {
            return Err(PortError::Denied);
        }
        let scope_key = scope_key(&scope);
        if replay_receipt(
            &mut tx,
            holding_id,
            &scope_key,
            &commit.idempotency_key,
            &digest,
            commit.evidence.id().as_uuid(),
        )
        .await?
        {
            tx.commit().await.map_err(|_| PortError::Persistence)?;
            return Ok(commit.evidence);
        }
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT evidence_version FROM erpx_agent_write.provider_invocation_evidence
             WHERE holding_id=$1 AND invocation_id=$2 FOR UPDATE",
        )
        .bind(holding_id)
        .bind(commit.evidence.id().as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        validate_version(commit.expected_version, current, commit.evidence.version())?;
        if current.is_none() {
            sqlx::query(
                "INSERT INTO erpx_agent_write.provider_invocation_evidence
                    (holding_id,invocation_id,assignment_id,assignment_version,attempt,
                     agent_definition_id,agent_version_id,provider_ref,model_ref,policy_version,
                     payload_digest,policy_digest,field_refs,document_refs,classification,residency,
                     output_digest,provider_units,provider_cost_micros,evidence_version,invoked_at,
                     completed_at,outcome)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,
                         $17,$18,$19,$20,$21,$22,$23)",
            )
            .bind(holding_id)
            .bind(commit.evidence.id().as_uuid())
            .bind(commit.evidence.assignment_id().as_uuid())
            .bind(to_i64(commit.evidence.assignment_version())?)
            .bind(i32::try_from(commit.evidence.attempt()).map_err(|_| PortError::Persistence)?)
            .bind(commit.evidence.agent_definition_id().as_uuid())
            .bind(commit.evidence.agent_version_id().as_uuid())
            .bind(commit.evidence.provider_ref())
            .bind(commit.evidence.model_ref())
            .bind(to_i64(commit.evidence.policy_version())?)
            .bind(commit.evidence.payload_digest())
            .bind(commit.evidence.policy_digest())
            .bind(
                commit
                    .evidence
                    .field_refs()
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>(),
            )
            .bind(
                commit
                    .evidence
                    .document_refs()
                    .iter()
                    .map(|id| id.as_uuid())
                    .collect::<Vec<_>>(),
            )
            .bind(classification(commit.evidence.classification()))
            .bind(commit.evidence.residency())
            .bind(commit.evidence.output_digest())
            .bind(commit.evidence.provider_units().map(to_i64).transpose()?)
            .bind(
                commit
                    .evidence
                    .provider_cost_micros()
                    .map(to_i64)
                    .transpose()?,
            )
            .bind(to_i64(commit.evidence.version())?)
            .bind(commit.evidence.invoked_at())
            .bind(commit.evidence.completed_at())
            .bind(commit.evidence.outcome().map(outcome))
            .execute(&mut *tx)
            .await
            .map_err(|_| PortError::Persistence)?;
        } else {
            let updated = sqlx::query(
                "UPDATE erpx_agent_write.provider_invocation_evidence SET
                    output_digest=$3,provider_units=$4,provider_cost_micros=$5,
                    evidence_version=$6,completed_at=$7,outcome=$8
                 WHERE holding_id=$1 AND invocation_id=$2 AND evidence_version=$9",
            )
            .bind(holding_id)
            .bind(commit.evidence.id().as_uuid())
            .bind(commit.evidence.output_digest())
            .bind(commit.evidence.provider_units().map(to_i64).transpose()?)
            .bind(
                commit
                    .evidence
                    .provider_cost_micros()
                    .map(to_i64)
                    .transpose()?,
            )
            .bind(to_i64(commit.evidence.version())?)
            .bind(commit.evidence.completed_at())
            .bind(commit.evidence.outcome().map(outcome))
            .bind(to_i64(
                commit.expected_version.ok_or(PortError::VersionConflict)?,
            )?)
            .execute(&mut *tx)
            .await
            .map_err(|_| PortError::Persistence)?;
            if updated.rows_affected() != 1 {
                return Err(PortError::VersionConflict);
            }
        }
        finish_commit(
            &mut tx,
            holding_id,
            &scope_key,
            &commit.idempotency_key,
            &digest,
            "provider_invocation",
            commit.evidence.id().as_uuid(),
            commit.evidence.version(),
            &scope,
            &commit.actor,
            &commit.audit,
            "agent.provider_invocation.changed",
            commit
                .evidence
                .completed_at()
                .unwrap_or(commit.evidence.invoked_at()),
        )
        .await?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        Ok(commit.evidence)
    }
}

#[async_trait]
impl AgentExperienceQueryPort for SqlxAgentExperienceQuery {
    async fn routing_summary(
        &self,
        authority_scope: &AuthorityScope,
        id: PoolRoutingDecisionId,
    ) -> Result<Option<PoolRoutingSummary>, PortError> {
        let mut tx = self.database.begin().await?;
        let row = sqlx::query(
            "SELECT p.requested_scope,p.routing_decision_id,p.agent_pool_id,
                    p.selected_agent_version_ids,p.compare_requested,p.decision_digest,p.decided_at,
                    p.source_global_sequence,c.global_sequence AS checkpoint_position,
                    EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_failures f
                      WHERE f.holding_id=p.holding_id AND f.projection_name=p.projection_name
                        AND f.generation=p.generation AND f.partition_key=p.partition_key) AS has_failure
             FROM erpx_agent_read.pool_routing_projection p
             JOIN erpx_cqrs_read.projection_generations g
               ON g.holding_id=p.holding_id AND g.projection_name=p.projection_name
              AND g.generation=p.generation AND g.status='active'
             LEFT JOIN erpx_cqrs_read.projection_checkpoints c
               ON c.holding_id=p.holding_id AND c.projection_name=p.projection_name
              AND c.generation=p.generation AND c.partition_key=p.partition_key
             WHERE p.holding_id=$1 AND p.projection_name='agent_orchestration.pool_routing'
               AND p.routing_decision_id=$2",
        )
        .bind(self.database.holding_id())
        .bind(id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        let Some(row) = row else { return Ok(None) };
        ensure_projection_fresh(&row)?;
        ensure_scope(
            authority_scope,
            row.try_get("requested_scope")
                .map_err(|_| PortError::Persistence)?,
        )?;
        Ok(Some(PoolRoutingSummary {
            id,
            pool_id: AgentPoolId::try_from_uuid(
                row.try_get("agent_pool_id")
                    .map_err(|_| PortError::Persistence)?,
            )
            .map_err(|_| PortError::Persistence)?,
            selected_versions: row
                .try_get::<Vec<Uuid>, _>("selected_agent_version_ids")
                .map_err(|_| PortError::Persistence)?
                .into_iter()
                .map(AgentVersionId::try_from_uuid)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| PortError::Persistence)?,
            compare_requested: row
                .try_get("compare_requested")
                .map_err(|_| PortError::Persistence)?,
            decision_digest: row
                .try_get("decision_digest")
                .map_err(|_| PortError::Persistence)?,
            decided_at: row
                .try_get("decided_at")
                .map_err(|_| PortError::Persistence)?,
        }))
    }

    async fn invocation_summary(
        &self,
        authority_scope: &AuthorityScope,
        id: ProviderInvocationId,
    ) -> Result<Option<InvocationEvidenceSummary>, PortError> {
        let mut tx = self.database.begin().await?;
        let row = sqlx::query(
            "SELECT p.active_scope,p.assignment_id,p.attempt,p.provider_ref,p.model_ref,
                    p.policy_version,p.policy_digest,p.payload_digest,p.output_digest,
                    p.provider_units,p.provider_cost_micros,p.outcome,p.invoked_at,p.completed_at,
                    p.source_global_sequence,c.global_sequence AS checkpoint_position,
                    EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_failures f
                      WHERE f.holding_id=p.holding_id AND f.projection_name=p.projection_name
                        AND f.generation=p.generation AND f.partition_key=p.partition_key) AS has_failure
             FROM erpx_agent_read.provider_invocation_projection p
             JOIN erpx_cqrs_read.projection_generations g
               ON g.holding_id=p.holding_id AND g.projection_name=p.projection_name
              AND g.generation=p.generation AND g.status='active'
             LEFT JOIN erpx_cqrs_read.projection_checkpoints c
               ON c.holding_id=p.holding_id AND c.projection_name=p.projection_name
              AND c.generation=p.generation AND c.partition_key=p.partition_key
             WHERE p.holding_id=$1
               AND p.projection_name='agent_orchestration.provider_invocations'
               AND p.invocation_id=$2",
        )
        .bind(self.database.holding_id())
        .bind(id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        let Some(row) = row else { return Ok(None) };
        ensure_projection_fresh(&row)?;
        ensure_scope(
            authority_scope,
            row.try_get("active_scope")
                .map_err(|_| PortError::Persistence)?,
        )?;
        Ok(Some(InvocationEvidenceSummary {
            id,
            assignment_id: erpx_agent_orchestration::AssignmentId::try_from_uuid(
                row.try_get("assignment_id")
                    .map_err(|_| PortError::Persistence)?,
            )
            .map_err(|_| PortError::Persistence)?,
            attempt: positive_u32(row.try_get("attempt").map_err(|_| PortError::Persistence)?)?,
            provider_ref: row
                .try_get("provider_ref")
                .map_err(|_| PortError::Persistence)?,
            model_ref: row
                .try_get("model_ref")
                .map_err(|_| PortError::Persistence)?,
            policy_version: positive_u64(
                row.try_get("policy_version")
                    .map_err(|_| PortError::Persistence)?,
            )?,
            policy_digest: row
                .try_get("policy_digest")
                .map_err(|_| PortError::Persistence)?,
            payload_digest: row
                .try_get("payload_digest")
                .map_err(|_| PortError::Persistence)?,
            output_digest: row
                .try_get("output_digest")
                .map_err(|_| PortError::Persistence)?,
            provider_units: optional_u64(
                row.try_get("provider_units")
                    .map_err(|_| PortError::Persistence)?,
            )?,
            provider_cost_micros: optional_u64(
                row.try_get("provider_cost_micros")
                    .map_err(|_| PortError::Persistence)?,
            )?,
            outcome: row.try_get("outcome").map_err(|_| PortError::Persistence)?,
            invoked_at: row
                .try_get("invoked_at")
                .map_err(|_| PortError::Persistence)?,
            completed_at: row
                .try_get("completed_at")
                .map_err(|_| PortError::Persistence)?,
        }))
    }
}

fn ensure_projection_fresh(row: &sqlx::postgres::PgRow) -> Result<(), PortError> {
    let source = positive_u64(
        row.try_get("source_global_sequence")
            .map_err(|_| PortError::Persistence)?,
    )?;
    let checkpoint = row
        .try_get::<Option<i64>, _>("checkpoint_position")
        .map_err(|_| PortError::Persistence)?
        .ok_or(PortError::Persistence)
        .and_then(positive_u64)?;
    if row
        .try_get::<bool, _>("has_failure")
        .map_err(|_| PortError::Persistence)?
        || checkpoint < source
    {
        return Err(PortError::Persistence);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn finish_commit(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: Uuid,
    scope_key: &str,
    idempotency_key: &str,
    request_digest: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    scope: &AuthorityScope,
    actor: &ActorAttribution,
    audit: &AuditContext,
    event_type: &str,
    occurred_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), PortError> {
    let command_id = stable_id(
        holding_id,
        aggregate_id,
        idempotency_key.as_bytes(),
        b"command",
    );
    let audit_id = stable_id(
        holding_id,
        aggregate_id,
        &aggregate_version.to_be_bytes(),
        b"audit",
    );
    let event_id = stable_id(
        holding_id,
        aggregate_id,
        &aggregate_version.to_be_bytes(),
        b"event",
    );
    let correlation_id = stable_id(
        holding_id,
        aggregate_id,
        idempotency_key.as_bytes(),
        b"correlation",
    );
    sqlx::query(
        "INSERT INTO erpx_agent_write.agent_command_receipts
            (holding_id,scope_key,idempotency_key,request_digest,aggregate_type,
             aggregate_id,aggregate_version,committed_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(holding_id)
    .bind(scope_key)
    .bind(idempotency_key)
    .bind(request_digest)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(to_i64(aggregate_version)?)
    .bind(occurred_at)
    .execute(&mut **tx)
    .await
    .map_err(map_unique)?;
    sqlx::query(
        "INSERT INTO erpx_cqrs_write.audit_log
            (holding_id,audit_id,context_name,partition_key,command_id,correlation_id,
             active_scope,actor,audit_context,action,aggregate_type,aggregate_id,
             aggregate_version,details,occurred_at)
         VALUES ($1,$2,'agent_orchestration',$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
    )
    .bind(holding_id)
    .bind(audit_id)
    .bind(scope_key)
    .bind(command_id)
    .bind(correlation_id)
    .bind(serde_json::to_value(scope).map_err(|_| PortError::Persistence)?)
    .bind(serde_json::to_value(actor).map_err(|_| PortError::Persistence)?)
    .bind(serde_json::to_value(audit).map_err(|_| PortError::Persistence)?)
    .bind(event_type)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(to_i64(aggregate_version)?)
    .bind(json!({"opaque": true, "aggregate_version": aggregate_version}))
    .bind(occurred_at)
    .execute(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    sqlx::query(
        "INSERT INTO erpx_cqrs_write.outbox_events
            (holding_id,event_id,context_name,partition_key,command_id,event_type,event_version,
             aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,
             causation_id,payload,occurred_at,available_at)
         VALUES ($1,$2,'agent_orchestration',$3,$4,$5,1,$6,$7,$8,$9,$10,$11,NULL,$12,$13,$13)
         ON CONFLICT (holding_id,event_id) DO NOTHING",
    )
    .bind(holding_id)
    .bind(event_id)
    .bind(scope_key)
    .bind(command_id)
    .bind(event_type)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(to_i64(aggregate_version)?)
    .bind(serde_json::to_value(scope).map_err(|_| PortError::Persistence)?)
    .bind(serde_json::to_value(actor).map_err(|_| PortError::Persistence)?)
    .bind(correlation_id)
    .bind(json!({"aggregate_id": aggregate_id, "aggregate_version": aggregate_version}))
    .bind(occurred_at)
    .execute(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    Ok(())
}

async fn replay_receipt(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: Uuid,
    scope_key: &str,
    idempotency_key: &str,
    request_digest: &str,
    aggregate_id: Uuid,
) -> Result<bool, PortError> {
    let receipt: Option<(String, Uuid)> = sqlx::query_as(
        "SELECT request_digest,aggregate_id FROM erpx_agent_write.agent_command_receipts
         WHERE holding_id=$1 AND scope_key=$2 AND idempotency_key=$3 FOR UPDATE",
    )
    .bind(holding_id)
    .bind(scope_key)
    .bind(idempotency_key)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    match receipt {
        None => Ok(false),
        Some((digest, id)) if digest == request_digest && id == aggregate_id => Ok(true),
        Some(_) => Err(PortError::IdempotencyConflict),
    }
}

fn validate_version(
    expected: Option<u64>,
    current: Option<i64>,
    next: u64,
) -> Result<(), PortError> {
    match (expected, current) {
        (None, None) if next == 1 => Ok(()),
        (Some(expected), Some(current))
            if positive_u64(current)? == expected
                && next == expected.checked_add(1).ok_or(PortError::VersionConflict)? =>
        {
            Ok(())
        }
        _ => Err(PortError::VersionConflict),
    }
}

fn object_digest(value: &impl Serialize) -> Result<String, PortError> {
    let bytes = serde_json::to_vec(value).map_err(|_| PortError::Persistence)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn stable_id(holding_id: Uuid, aggregate_id: Uuid, identity: &[u8], purpose: &[u8]) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(b"erpx.agent.experience");
    digest.update(purpose);
    digest.update(holding_id.as_bytes());
    digest.update(aggregate_id.as_bytes());
    digest.update(identity);
    let bytes = digest.finalize();
    let mut uuid_bytes = [0_u8; 16];
    uuid_bytes.copy_from_slice(&bytes[..16]);
    uuid_bytes[6] = (uuid_bytes[6] & 0x0f) | 0x50;
    uuid_bytes[8] = (uuid_bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(uuid_bytes)
}

fn scope_key(scope: &AuthorityScope) -> String {
    match scope {
        AuthorityScope::Platform => "platform".into(),
        AuthorityScope::Holding { holding_id } => format!("h:{holding_id}"),
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => format!("h:{holding_id}/c:{company_id}"),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => format!("h:{holding_id}/c:{company_id}/b:{branch_id}"),
        AuthorityScope::Subtree {
            holding_id,
            organization_node_id,
        } => format!("h:{holding_id}/node:{organization_node_id}"),
    }
}

fn ensure_scope(
    authority_scope: &AuthorityScope,
    stored: serde_json::Value,
) -> Result<(), PortError> {
    let stored: AuthorityScope =
        serde_json::from_value(stored).map_err(|_| PortError::Persistence)?;
    if authority_scope.contains(&stored) {
        Ok(())
    } else {
        Err(PortError::Denied)
    }
}

fn author_kind(kind: PrincipalKind) -> &'static str {
    match kind {
        PrincipalKind::Human => "human",
        PrincipalKind::Agent => "agent",
        PrincipalKind::Service => "service",
    }
}
fn parse_author_kind(value: String) -> Result<PrincipalKind, PortError> {
    match value.as_str() {
        "human" => Ok(PrincipalKind::Human),
        "agent" => Ok(PrincipalKind::Agent),
        _ => Err(PortError::Persistence),
    }
}
fn classification(value: DataClassification) -> &'static str {
    match value {
        DataClassification::Public => "public",
        DataClassification::Internal => "internal",
        DataClassification::Confidential => "confidential",
        DataClassification::Restricted => "restricted",
    }
}
fn outcome(value: ProviderInvocationOutcome) -> &'static str {
    match value {
        ProviderInvocationOutcome::Completed => "completed",
        ProviderInvocationOutcome::Failed => "failed",
        ProviderInvocationOutcome::Cancelled => "cancelled",
    }
}
fn to_i64(value: u64) -> Result<i64, PortError> {
    i64::try_from(value).map_err(|_| PortError::Persistence)
}
fn positive_u64(value: i64) -> Result<u64, PortError> {
    if value <= 0 {
        Err(PortError::Persistence)
    } else {
        u64::try_from(value).map_err(|_| PortError::Persistence)
    }
}
fn positive_u32(value: i32) -> Result<u32, PortError> {
    if value <= 0 {
        Err(PortError::Persistence)
    } else {
        u32::try_from(value).map_err(|_| PortError::Persistence)
    }
}
fn optional_u64(value: Option<i64>) -> Result<Option<u64>, PortError> {
    value
        .map(|value| u64::try_from(value).map_err(|_| PortError::Persistence))
        .transpose()
}
fn map_unique(error: sqlx::Error) -> PortError {
    if error
        .as_database_error()
        .is_some_and(|error| error.is_unique_violation())
    {
        PortError::IdempotencyConflict
    } else {
        PortError::Persistence
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn adapter_source_enforces_holding_version_receipt_audit_and_outbox_fences() {
        let source = include_str!("experience.rs");
        assert!(source.contains("HoldingBoundPool"));
        assert!(source.contains("WHERE holding_id=$1"));
        assert!(source.contains("ON CONFLICT (holding_id,event_id)"));
        assert!(source.contains("erpx_cqrs_write.audit_log"));
        assert!(source.contains("agent_command_receipts"));
        assert!(source.contains("next == expected.checked_add(1)"));
        assert!(source.contains("v.version_status='approved'"));
        assert!(source.contains("d.definition_status='approved'"));
        assert!(source.contains("COUNT(a.assignment_id) FILTER"));
        assert!(!source.contains(concat!("prompt", "_material")));
        assert!(!source.contains(concat!("output", "_material")));
    }

    #[test]
    fn query_side_uses_disposable_projections_only() {
        let source = include_str!("experience.rs");
        assert!(source.contains("erpx_agent_read.pool_routing_projection"));
        assert!(source.contains("erpx_agent_read.provider_invocation_projection"));
        assert!(source.contains("g.status='active'"));
        assert!(source.contains("ensure_projection_fresh(&row)"));
        assert!(!source.contains(&["ORDER BY projected_at DESC", " LIMIT 1"].concat()));
    }
}
