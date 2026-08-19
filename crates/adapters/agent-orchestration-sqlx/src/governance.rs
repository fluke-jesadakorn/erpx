use crate::holding::HoldingBoundPool;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_agent_orchestration::{
    AgentGovernanceQueryPort, AgentGovernanceRecord, AgentGovernanceRepository,
    GovernanceRecordCommit, GovernanceRecordProjection, GovernanceRecordReceipt, PortError,
};
use erpx_cqrs_sqlx::ReadPool;
use erpx_kernel::HoldingId;
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SqlxAgentGovernanceRepository {
    pool: HoldingBoundPool,
}

#[derive(Debug, Clone)]
pub struct SqlxAgentGovernanceQuery {
    pool: ReadPool,
    holding_id: HoldingId,
}

impl SqlxAgentGovernanceQuery {
    pub fn try_new(pool: sqlx::PgPool, holding_id: HoldingId) -> Result<Self, PortError> {
        let pool = ReadPool::for_holding(pool, holding_id.as_uuid())
            .map_err(|_| PortError::Persistence)?;
        Ok(Self { pool, holding_id })
    }
}

#[async_trait]
impl AgentGovernanceQueryPort for SqlxAgentGovernanceQuery {
    async fn get(
        &self,
        authority_scope: &erpx_kernel::AuthorityScope,
        id: Uuid,
    ) -> Result<Option<GovernanceRecordProjection>, PortError> {
        authority_scope.validate().map_err(|_| PortError::Denied)?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| PortError::Persistence)?;
        let generation: Option<Uuid> = sqlx::query_scalar(
            "SELECT generation FROM erpx_cqrs_read.projection_generations
             WHERE holding_id=$1 AND projection_name='agent_orchestration.governance'
               AND status='active'",
        )
        .bind(self.holding_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        let generation = generation.ok_or(PortError::Persistence)?;
        let row = sqlx::query(
            "SELECT p.active_scope,p.record_id,p.record_kind,p.subject_record_id,
                    p.policy_digest,p.lifecycle_state,p.source_aggregate_version,
                    p.source_global_sequence,p.updated_at,p.projected_at,
                    c.global_sequence AS checkpoint_position,
                    EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_failures f
                      WHERE f.holding_id=p.holding_id AND f.projection_name=p.projection_name
                        AND f.generation=p.generation AND f.partition_key=p.partition_key) AS has_failure
             FROM erpx_agent_read.governance_projection p
             LEFT JOIN erpx_cqrs_read.projection_checkpoints c
               ON c.holding_id=p.holding_id AND c.projection_name=p.projection_name
              AND c.generation=p.generation AND c.partition_key=p.partition_key
             WHERE p.holding_id=$1 AND p.projection_name='agent_orchestration.governance'
               AND p.generation=$2 AND p.record_id=$3",
        )
        .bind(self.holding_id.as_uuid())
        .bind(generation)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        let Some(row) = row else { return Ok(None) };
        let scope: erpx_kernel::AuthorityScope = serde_json::from_value(
            row.try_get("active_scope")
                .map_err(|_| PortError::Persistence)?,
        )
        .map_err(|_| PortError::Persistence)?;
        if !authority_scope.contains(&scope) {
            return Err(PortError::Denied);
        }
        let source_position = positive(
            row.try_get("source_global_sequence")
                .map_err(|_| PortError::Persistence)?,
        )?;
        let checkpoint_position = positive(
            row.try_get::<Option<i64>, _>("checkpoint_position")
                .map_err(|_| PortError::Persistence)?
                .ok_or(PortError::Persistence)?,
        )?;
        if row
            .try_get::<bool, _>("has_failure")
            .map_err(|_| PortError::Persistence)?
            || checkpoint_position < source_position
        {
            return Err(PortError::Persistence);
        }
        let record = serde_json::from_value(serde_json::json!({
            "id": row.try_get::<Uuid, _>("record_id").map_err(|_| PortError::Persistence)?,
            "version": row.try_get::<i64, _>("source_aggregate_version").map_err(|_| PortError::Persistence)?,
            "scope": scope,
            "kind": row.try_get::<String, _>("record_kind").map_err(|_| PortError::Persistence)?,
            "subject_record_id": row.try_get::<Uuid, _>("subject_record_id").map_err(|_| PortError::Persistence)?,
            "policy_digest": row.try_get::<String, _>("policy_digest").map_err(|_| PortError::Persistence)?,
            "status": row.try_get::<String, _>("lifecycle_state").map_err(|_| PortError::Persistence)?,
            "updated_at": row.try_get::<DateTime<Utc>, _>("updated_at").map_err(|_| PortError::Persistence)?,
        }))
        .map_err(|_| PortError::Persistence)?;
        Ok(Some(GovernanceRecordProjection {
            record,
            generation,
            source_position,
            checkpoint_position,
            projected_at: row
                .try_get("projected_at")
                .map_err(|_| PortError::Persistence)?,
        }))
    }
}
impl SqlxAgentGovernanceRepository {
    pub fn try_new(pool: sqlx::PgPool, holding_id: Uuid) -> Result<Self, PortError> {
        Ok(Self {
            pool: HoldingBoundPool::try_new(pool, holding_id)?,
        })
    }
}

#[async_trait]
impl AgentGovernanceRepository for SqlxAgentGovernanceRepository {
    async fn load(
        &self,
        authority_scope: &erpx_kernel::AuthorityScope,
        id: Uuid,
    ) -> Result<Option<AgentGovernanceRecord>, PortError> {
        let mut tx = self.pool.begin().await?;
        let row=sqlx::query("SELECT active_scope,record_kind,subject_record_id,policy_digest,lifecycle_state,aggregate_version,updated_at FROM erpx_agent_write.agent_governance_records WHERE holding_id=$1 AND record_id=$2").bind(self.pool.holding_id()).bind(id).fetch_optional(&mut *tx).await.map_err(|_|PortError::Persistence)?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        let Some(row) = row else { return Ok(None) };
        let scope: erpx_kernel::AuthorityScope = serde_json::from_value(
            row.try_get("active_scope")
                .map_err(|_| PortError::Persistence)?,
        )
        .map_err(|_| PortError::Persistence)?;
        if !authority_scope.contains(&scope) {
            return Err(PortError::Denied);
        }
        let value = serde_json::json!({"id":id,"version":row.try_get::<i64,_>("aggregate_version").map_err(|_|PortError::Persistence)?,"scope":scope,"kind":row.try_get::<String,_>("record_kind").map_err(|_|PortError::Persistence)?,"subject_record_id":row.try_get::<Uuid,_>("subject_record_id").map_err(|_|PortError::Persistence)?,"policy_digest":row.try_get::<String,_>("policy_digest").map_err(|_|PortError::Persistence)?,"status":row.try_get::<String,_>("lifecycle_state").map_err(|_|PortError::Persistence)?,"updated_at":row.try_get::<DateTime<Utc>,_>("updated_at").map_err(|_|PortError::Persistence)?});
        serde_json::from_value(value)
            .map(Some)
            .map_err(|_| PortError::Persistence)
    }
    async fn commit(
        &self,
        c: GovernanceRecordCommit,
    ) -> Result<GovernanceRecordReceipt, PortError> {
        if c.holding_id != self.pool.holding_id()
            || !c.authority_scope.contains(c.record.scope())
            || c.idempotency_key.trim().is_empty()
            || c.idempotency_key.len() > 128
            || !digest(&c.request_digest)
        {
            return Err(PortError::Denied);
        }
        c.record.validate().map_err(|_| PortError::Persistence)?;
        let scope_key = scope_key(c.record.scope())?;
        let mut tx = self.pool.begin().await?;
        if let Some(row)=sqlx::query("SELECT request_digest,aggregate_id,aggregate_version,committed_at FROM erpx_agent_write.agent_command_receipts WHERE holding_id=$1 AND scope_key=$2 AND idempotency_key=$3 FOR UPDATE").bind(c.holding_id).bind(&scope_key).bind(&c.idempotency_key).fetch_optional(&mut *tx).await.map_err(|_|PortError::Persistence)?{let same=row.try_get::<String,_>("request_digest").ok().as_deref()==Some(&c.request_digest)&&row.try_get::<Uuid,_>("aggregate_id").ok()==Some(c.record.id())&&row.try_get::<i64,_>("aggregate_version").ok().and_then(|v|u64::try_from(v).ok())==Some(c.record.version());if !same{return Err(PortError::IdempotencyConflict)}let at=row.try_get("committed_at").map_err(|_|PortError::Persistence)?;tx.commit().await.map_err(|_|PortError::Persistence)?;return Ok(GovernanceRecordReceipt{record_id:c.record.id(),version:c.record.version(),replayed:true,committed_at:at})}
        let current:Option<i64>=sqlx::query_scalar("SELECT aggregate_version FROM erpx_agent_write.agent_governance_records WHERE holding_id=$1 AND record_id=$2 FOR UPDATE").bind(c.holding_id).bind(c.record.id()).fetch_optional(&mut *tx).await.map_err(|_|PortError::Persistence)?;
        match (c.expected_version, current) {
            (None, None) if c.record.version() == 1 => {}
            (Some(e), Some(v))
                if u64::try_from(v).ok() == Some(e)
                    && c.record.version()
                        == e.checked_add(1).ok_or(PortError::VersionConflict)? => {}
            _ => return Err(PortError::VersionConflict),
        }
        sqlx::query("INSERT INTO erpx_agent_write.agent_governance_records(holding_id,record_id,scope_key,active_scope,record_kind,subject_record_id,policy_digest,lifecycle_state,aggregate_version,updated_at)VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)ON CONFLICT(holding_id,record_id)DO UPDATE SET policy_digest=EXCLUDED.policy_digest,lifecycle_state=EXCLUDED.lifecycle_state,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at WHERE erpx_agent_write.agent_governance_records.aggregate_version+1=EXCLUDED.aggregate_version").bind(c.holding_id).bind(c.record.id()).bind(&scope_key).bind(serde_json::to_value(c.record.scope()).map_err(|_|PortError::Persistence)?).bind(c.record.kind().token()).bind(c.record.subject_record_id().as_uuid()).bind(c.record.policy_digest()).bind(token(c.record.status())?).bind(i64::try_from(c.record.version()).map_err(|_|PortError::Persistence)?).bind(c.record.updated_at()).execute(&mut *tx).await.map_err(|_|PortError::Persistence)?;
        let command_id = stable(
            c.holding_id,
            c.record.id(),
            c.idempotency_key.as_bytes(),
            b"command",
        );
        let audit_id = stable(
            c.holding_id,
            c.record.id(),
            &c.record.version().to_be_bytes(),
            b"audit",
        );
        let event_id = stable(
            c.holding_id,
            c.record.id(),
            &c.record.version().to_be_bytes(),
            b"event",
        );
        let actor = serde_json::to_value(&c.actor).map_err(|_| PortError::Persistence)?;
        let partition = format!("holding:{}", c.holding_id);
        let kind = c.record.kind().token();
        sqlx::query("INSERT INTO erpx_cqrs_write.audit_log(holding_id,audit_id,context_name,partition_key,command_id,correlation_id,active_scope,actor,audit_context,action,aggregate_type,aggregate_id,aggregate_version,details,occurred_at)VALUES($1,$2,'agent_orchestration',$3,$4,$5,$6,$7,$8,'agent.governance.changed',$9,$10,$11,$12,$13)").bind(c.holding_id).bind(audit_id).bind(&partition).bind(command_id).bind(c.correlation_id).bind(serde_json::to_value(c.record.scope()).map_err(|_|PortError::Persistence)?).bind(actor.clone()).bind(serde_json::json!({"purpose":"agent-governance","customer_visible":false})).bind(kind).bind(c.record.id()).bind(i64::try_from(c.record.version()).map_err(|_|PortError::Persistence)?).bind(serde_json::json!({"opaque":true,"state":token(c.record.status())?})).bind(c.record.updated_at()).execute(&mut *tx).await.map_err(|_|PortError::Persistence)?;
        sqlx::query("INSERT INTO erpx_cqrs_write.outbox_events(holding_id,event_id,context_name,partition_key,command_id,event_type,event_version,aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,causation_id,payload,occurred_at,available_at)VALUES($1,$2,'agent_orchestration',$3,$4,'agent.governance.changed',1,$5,$6,$7,$8,$9,$10,NULL,$11,$12,$12)").bind(c.holding_id).bind(event_id).bind(&partition).bind(command_id).bind(kind).bind(c.record.id()).bind(i64::try_from(c.record.version()).map_err(|_|PortError::Persistence)?).bind(serde_json::to_value(c.record.scope()).map_err(|_|PortError::Persistence)?).bind(actor.clone()).bind(c.correlation_id).bind(serde_json::json!({"record_id":c.record.id(),"kind":kind,"state":token(c.record.status())?,"policy_digest":c.record.policy_digest()})).bind(c.record.updated_at()).execute(&mut *tx).await.map_err(|_|PortError::Persistence)?;
        let draft = crate::publication::governance_directory_draft(&c.record)?;
        crate::publication::append_with_idempotency(
            &mut tx,
            c.holding_id,
            &format!(
                "governance-directory:{}:{}",
                c.record.id(),
                c.record.version()
            ),
            actor,
            c.record.updated_at(),
            &draft,
        )
        .await?;
        let committed_at = Utc::now();
        sqlx::query("INSERT INTO erpx_agent_write.agent_command_receipts(holding_id,scope_key,idempotency_key,request_digest,aggregate_type,aggregate_id,aggregate_version,committed_at)VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(c.holding_id).bind(scope_key).bind(c.idempotency_key).bind(c.request_digest).bind(kind).bind(c.record.id()).bind(i64::try_from(c.record.version()).map_err(|_|PortError::Persistence)?).bind(committed_at).execute(&mut *tx).await.map_err(|_|PortError::Persistence)?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        Ok(GovernanceRecordReceipt {
            record_id: c.record.id(),
            version: c.record.version(),
            replayed: false,
            committed_at,
        })
    }
}

fn digest(v: &str) -> bool {
    v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit())
}
fn positive(value: i64) -> Result<u64, PortError> {
    let value = u64::try_from(value).map_err(|_| PortError::Persistence)?;
    if value == 0 {
        return Err(PortError::Persistence);
    }
    Ok(value)
}
fn token(v: impl serde::Serialize) -> Result<String, PortError> {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .ok_or(PortError::Persistence)
}
fn scope_key(v: &erpx_kernel::AuthorityScope) -> Result<String, PortError> {
    let encoded = serde_json::to_vec(v).map_err(|_| PortError::Persistence)?;
    Ok(format!("scope:{:x}", Sha256::digest(encoded)))
}
fn stable(t: Uuid, id: Uuid, v: &[u8], p: &[u8]) -> Uuid {
    let n = Uuid::new_v5(&t, id.as_bytes());
    let mut b = Vec::from(v);
    b.extend_from_slice(p);
    Uuid::new_v5(&n, &b)
}

#[cfg(test)]
mod tests {
    #[test]
    fn repository_is_holding_bound_atomic_and_has_no_trigger_or_material() {
        let s = include_str!("governance.rs");
        assert!(s.contains("HoldingBoundPool"));
        assert!(s.contains("agent_command_receipts"));
        assert!(s.contains("audit_log"));
        assert!(s.contains("outbox_events"));
        assert!(s.contains("governance_directory_draft"));
        for x in [
            concat!("prompt", "_ref"),
            concat!("output", "_ref"),
            concat!("proactive", "_trigger"),
        ] {
            assert!(!s.contains(x))
        }
    }

    #[test]
    fn query_reads_only_the_exact_active_generation_and_fails_on_lag() {
        let source = include_str!("governance.rs");
        let query = source
            .split("impl AgentGovernanceQueryPort")
            .nth(1)
            .expect("query implementation")
            .split("impl SqlxAgentGovernanceRepository")
            .next()
            .expect("query boundary");
        assert!(query.contains("status='active'"));
        assert!(query.contains("p.generation=$2"));
        assert!(query.contains("checkpoint_position < source_position"));
        assert!(!query.contains("erpx_agent_write"));
    }
}
