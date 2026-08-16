use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs_sqlx::ReadPool;
use erpx_kernel::{TenantId, TenantScope};
use erpx_talent::{
    AggregateRepository, TalentAggregate, TalentCommit, TalentCommitReceipt, TalentPortError,
    TalentProjection, TalentReadRepository, TalentRealm,
};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SqlxTalentWriteRepository {
    pool: PgPool,
}
impl SqlxTalentWriteRepository {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
#[derive(Debug, Clone)]
pub struct SqlxTalentReadRepository {
    pool: ReadPool,
    tenant_id: TenantId,
}
impl SqlxTalentReadRepository {
    pub fn new(pool: PgPool, tenant_id: TenantId) -> Result<Self, TalentPortError> {
        let pool = ReadPool::for_tenant(pool, tenant_id.as_uuid())
            .map_err(|_| TalentPortError::Unavailable)?;
        Ok(Self { pool, tenant_id })
    }
}

#[async_trait]
impl AggregateRepository for SqlxTalentWriteRepository {
    async fn replay(
        &self,
        tenant_id: TenantId,
        identity: &erpx_talent::MutationIdentity,
        aggregate_type: &str,
        aggregate_id: Uuid,
    ) -> Result<Option<TalentCommitReceipt>, TalentPortError> {
        identity.validate()?;
        let mut tx = self.pool.begin().await.map_err(map)?;
        set_tenant(&mut tx, tenant_id).await?;
        let receipt = lookup_receipt(&mut tx, tenant_id, &identity.idempotency_key).await?;
        let Some((stored_command_id, request_digest, stored_type, stored_id, receipt)) = receipt
        else {
            tx.commit().await.map_err(map)?;
            return Ok(None);
        };
        if stored_command_id != identity.command_id.as_uuid()
            || request_digest != identity.request_digest
            || stored_type != aggregate_type
            || stored_id != aggregate_id
        {
            return Err(TalentPortError::IdempotencyConflict);
        }
        tx.commit().await.map_err(map)?;
        Ok(Some(receipt))
    }

    async fn load(
        &self,
        tenant_id: TenantId,
        authority: &TenantScope,
        realm: TalentRealm,
        aggregate_type: &str,
        id: Uuid,
    ) -> Result<Option<TalentAggregate>, TalentPortError> {
        let mut tx = self.pool.begin().await.map_err(map)?;
        set_tenant(&mut tx, tenant_id).await?;
        let document: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT aggregate_document FROM erpx_talent_write.aggregate_rehydration
             WHERE tenant_id=$1 AND aggregate_type=$2 AND aggregate_id=$3",
        )
        .bind(tenant_id.as_uuid())
        .bind(aggregate_type)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map)?;
        tx.commit().await.map_err(map)?;
        let Some(document) = document else {
            return Ok(None);
        };
        let aggregate = decode(aggregate_type, document)?;
        if aggregate.id_uuid() != id
            || aggregate.authority_realm() != realm
            || !authority.contains(aggregate.scope())
        {
            return Err(TalentPortError::Denied);
        }
        Ok(Some(aggregate))
    }

    async fn commit(&self, command: TalentCommit) -> Result<TalentCommitReceipt, TalentPortError> {
        command.identity.validate()?;
        if !command.event_type.starts_with("talent.")
            || !command
                .actor
                .authority_scope
                .contains(command.aggregate.scope())
        {
            return Err(TalentPortError::Invalid);
        }
        let tenant = command.actor.tenant_id;
        let kind = command.aggregate.aggregate_type();
        let id = command.aggregate.id_uuid();
        let version = command.aggregate.version();
        if command.actor.principal.realm == TalentRealm::Candidate
            && (command.aggregate.authority_realm() != TalentRealm::Candidate
                || command.aggregate.candidate_principal_id()
                    != Some(command.actor.principal.principal_id))
        {
            return Err(TalentPortError::RealmMismatch);
        }
        let mut tx = self.pool.begin().await.map_err(map)?;
        set_tenant(&mut tx, tenant).await?;
        if let Some(receipt) = replay_command(&mut tx, tenant, &command).await? {
            tx.commit().await.map_err(map)?;
            return Ok(receipt);
        }
        let current = lock_version(&mut tx, tenant, kind, id).await?;
        match (command.expected_version, current) {
            (None, None) if version == 1 => {}
            (Some(expected), Some(actual))
                if expected == actual
                    && version == expected.checked_add(1).ok_or(TalentPortError::Conflict)? => {}
            _ => return Err(TalentPortError::Conflict),
        }
        let state = serde_json::to_value(&command.aggregate)
            .map_err(|_| TalentPortError::Invalid)?
            .get("state")
            .cloned()
            .ok_or(TalentPortError::Invalid)?;
        let projection_payload =
            serde_json::to_value(&command.aggregate).map_err(|_| TalentPortError::Invalid)?;
        sqlx::query("SELECT erpx_talent_write.persist_normalized($1,$2,$3,$4,$5,$6)")
            .bind(tenant.as_uuid())
            .bind(kind)
            .bind(id)
            .bind(scope_key(command.aggregate.scope())?)
            .bind(
                serde_json::to_value(command.aggregate.scope())
                    .map_err(|_| TalentPortError::Invalid)?,
            )
            .bind(state)
            .execute(&mut *tx)
            .await
            .map_err(map)?;
        let action = enum_name(command.action)?;
        let realm = enum_name(command.actor.principal.realm)?;
        sqlx::query("INSERT INTO erpx_talent_write.audit_log
            (tenant_id,audit_id,command_id,actor_principal_id,actor_realm,action,aggregate_type,aggregate_id,
             aggregate_version,scope_key,correlation_id,evidence_digest,occurred_at)
            VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)")
            .bind(tenant.as_uuid()).bind(derived_id(command.identity.command_id.as_uuid(),b"audit"))
            .bind(command.identity.command_id.as_uuid()).bind(command.actor.principal.principal_id).bind(realm).bind(action)
            .bind(kind).bind(id).bind(to_i64(version)?).bind(scope_key(command.aggregate.scope())?)
            .bind(command.identity.correlation_id.as_uuid()).bind(&command.identity.request_digest)
            .bind(command.aggregate.updated_at()).execute(&mut *tx).await.map_err(map)?;
        let projection_event = match command.aggregate.authority_realm() {
            TalentRealm::Candidate => crate::CANDIDATE_SNAPSHOT_EVENT,
            TalentRealm::Workforce => crate::WORKFORCE_SNAPSHOT_EVENT,
        };
        let actor = serde_json::json!({"principal_ref":format!("principal:{}",command.actor.principal.principal_id),"kind":"tenant_user","session_id":null,"represented_principal_ref":null,"support_capability_id":null});
        let projection_event_id = derived_id(command.identity.command_id.as_uuid(), b"projection");
        let global_sequence: i64 = sqlx::query_scalar("INSERT INTO erpx_cqrs_write.outbox_events (tenant_id,event_id,context_name,partition_key,command_id,event_type,event_version,aggregate_type,aggregate_id,aggregate_version,tenant_scope,actor,correlation_id,causation_id,payload,occurred_at,available_at) VALUES ($1,$2,'talent',$3,$4,$5,1,$6,$7,$8,$9,$10,$11,NULL,$12,$13,$13) RETURNING global_sequence")
            .bind(tenant.as_uuid()).bind(projection_event_id).bind(crate::aggregate_partition(kind,id)).bind(command.identity.command_id.as_uuid()).bind(projection_event).bind(kind).bind(id).bind(to_i64(version)?).bind(serde_json::to_value(command.aggregate.scope()).map_err(|_|TalentPortError::Invalid)?).bind(actor).bind(command.identity.correlation_id.as_uuid()).bind(serde_json::json!({"aggregate":projection_payload})).bind(command.aggregate.updated_at()).fetch_one(&mut *tx).await.map_err(map)?;
        if let Some(draft) = crate::directory::talent_directory_draft(&command.aggregate)? {
            let actor = serde_json::json!({"principal_ref":format!("principal:{}",command.actor.principal.principal_id),"kind":"tenant_user","session_id":null,"represented_principal_ref":null,"support_capability_id":null});
            crate::directory::append_directory_draft(
                &mut tx,
                tenant,
                command.identity.command_id,
                command.identity.correlation_id,
                actor,
                command.aggregate.updated_at(),
                &draft,
            )
            .await?;
        }
        sqlx::query("INSERT INTO erpx_talent_write.outbox
            (tenant_id,event_id,aggregate_type,aggregate_id,aggregate_version,event_type,correlation_id,payload,occurred_at)
            VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(tenant.as_uuid()).bind(derived_id(command.identity.command_id.as_uuid(),b"event"))
            .bind(kind).bind(id).bind(to_i64(version)?).bind(&command.event_type)
            .bind(command.identity.correlation_id.as_uuid()).bind(command.event_payload)
            .bind(command.aggregate.updated_at()).execute(&mut *tx).await.map_err(map)?;
        let committed_at:DateTime<Utc>=sqlx::query_scalar("INSERT INTO erpx_talent_write.command_receipts
            (tenant_id,idempotency_key,command_id,request_digest,aggregate_type,aggregate_id,aggregate_version)
            VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING committed_at")
            .bind(tenant.as_uuid()).bind(command.identity.idempotency_key).bind(command.identity.command_id.as_uuid())
            .bind(command.identity.request_digest).bind(kind).bind(id).bind(to_i64(version)?)
            .fetch_one(&mut *tx).await.map_err(map)?;
        tx.commit().await.map_err(map)?;
        Ok(TalentCommitReceipt {
            aggregate_id: id,
            aggregate_version: version,
            event_id: projection_event_id,
            global_sequence: u64::try_from(global_sequence)
                .map_err(|_| TalentPortError::Internal)?,
            committed_at,
            replayed: false,
        })
    }
}

#[async_trait]
impl TalentReadRepository for SqlxTalentReadRepository {
    async fn get(
        &self,
        tenant_id: TenantId,
        realm: TalentRealm,
        aggregate_type: &str,
        id: Uuid,
    ) -> Result<Option<TalentProjection>, TalentPortError> {
        if tenant_id != self.tenant_id {
            return Err(TalentPortError::Denied);
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| TalentPortError::Unavailable)?;
        let projection_name = match realm {
            TalentRealm::Candidate => crate::CANDIDATE_PROJECTION,
            TalentRealm::Workforce => crate::WORKFORCE_PROJECTION,
        };
        let generation:Option<Uuid>=sqlx::query_scalar("SELECT generation FROM erpx_cqrs_read.projection_generations WHERE tenant_id=$1 AND projection_name=$2 AND status='active'").bind(self.tenant_id.as_uuid()).bind(projection_name).fetch_optional(&mut *tx).await.map_err(map)?;
        let generation = generation.ok_or(TalentPortError::Unavailable)?;
        let row=match realm{
            TalentRealm::Candidate=>sqlx::query("SELECT tenant_scope,authority_realm,candidate_principal_id,lifecycle_status,
                source_aggregate_version,source_global_sequence,projected_at,generation,c.global_sequence AS checkpoint_position,
                EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_failures f WHERE f.tenant_id=p.tenant_id AND f.projection_name=p.projection_name AND f.generation=p.generation AND f.partition_key=p.partition_key) AS has_failure
                FROM erpx_talent_read.candidate_projection p LEFT JOIN erpx_cqrs_read.projection_checkpoints c ON c.tenant_id=p.tenant_id AND c.projection_name=p.projection_name AND c.generation=p.generation AND c.partition_key=p.partition_key
                WHERE p.tenant_id=$1 AND p.projection_name=$2 AND p.generation=$3 AND record_id=$4 AND record_type=$5")
                .bind(self.tenant_id.as_uuid()).bind(projection_name).bind(generation).bind(id).bind(variant(aggregate_type)?)
                .fetch_optional(&mut *tx).await.map_err(map)?,
            TalentRealm::Workforce=>sqlx::query("SELECT tenant_scope,authority_realm,NULL::uuid candidate_principal_id,lifecycle_status,
                source_aggregate_version,source_global_sequence,projected_at,generation,c.global_sequence AS checkpoint_position,
                EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_failures f WHERE f.tenant_id=p.tenant_id AND f.projection_name=p.projection_name AND f.generation=p.generation AND f.partition_key=p.partition_key) AS has_failure
                FROM erpx_talent_read.workforce_projection p LEFT JOIN erpx_cqrs_read.projection_checkpoints c ON c.tenant_id=p.tenant_id AND c.projection_name=p.projection_name AND c.generation=p.generation AND c.partition_key=p.partition_key
                WHERE p.tenant_id=$1 AND p.projection_name=$2 AND p.generation=$3 AND record_id=$4 AND record_type=$5")
                .bind(self.tenant_id.as_uuid()).bind(projection_name).bind(generation).bind(id).bind(variant(aggregate_type)?)
                .fetch_optional(&mut *tx).await.map_err(map)?,
        };
        tx.commit().await.map_err(map)?;
        if let Some(ref row) = row {
            let position = row
                .try_get::<i64, _>("source_global_sequence")
                .map_err(|_| TalentPortError::Internal)?;
            let checkpoint = row
                .try_get::<Option<i64>, _>("checkpoint_position")
                .map_err(|_| TalentPortError::Internal)?;
            if row
                .try_get::<bool, _>("has_failure")
                .map_err(|_| TalentPortError::Internal)?
                || checkpoint.is_none_or(|value| value < position)
            {
                return Err(TalentPortError::Stale);
            }
        }
        row.map(|row| projection(aggregate_type, id, row))
            .transpose()
    }
}

async fn set_tenant(
    tx: &mut Transaction<'_, Postgres>,
    tenant: TenantId,
) -> Result<(), TalentPortError> {
    sqlx::query("SELECT set_config('app.tenant_id',$1,true)")
        .bind(tenant.to_string())
        .execute(&mut **tx)
        .await
        .map_err(map)?;
    Ok(())
}
async fn replay_command(
    tx: &mut Transaction<'_, Postgres>,
    tenant: TenantId,
    command: &TalentCommit,
) -> Result<Option<TalentCommitReceipt>, TalentPortError> {
    let row=sqlx::query("SELECT command_id,request_digest,aggregate_type,aggregate_id,aggregate_version,committed_at
        FROM erpx_talent_write.command_receipts WHERE tenant_id=$1 AND idempotency_key=$2 FOR UPDATE")
        .bind(tenant.as_uuid()).bind(&command.identity.idempotency_key).fetch_optional(&mut **tx).await.map_err(map)?;
    let Some(row) = row else { return Ok(None) };
    let stored: i64 = row
        .try_get("aggregate_version")
        .map_err(|_| TalentPortError::Internal)?;
    if row.try_get::<Uuid, _>("command_id").ok() != Some(command.identity.command_id.as_uuid())
        || row.try_get::<String, _>("request_digest").ok().as_deref()
            != Some(&command.identity.request_digest)
        || row.try_get::<String, _>("aggregate_type").ok().as_deref()
            != Some(command.aggregate.aggregate_type())
        || row.try_get::<Uuid, _>("aggregate_id").ok() != Some(command.aggregate.id_uuid())
        || u64::try_from(stored).ok() != Some(command.aggregate.version())
    {
        return Err(TalentPortError::IdempotencyConflict);
    }
    let event_id = derived_id(command.identity.command_id.as_uuid(), b"projection");
    let global_sequence: i64 = sqlx::query_scalar(
        "SELECT global_sequence FROM erpx_cqrs_write.outbox_events
         WHERE tenant_id=$1 AND event_id=$2",
    )
    .bind(tenant.as_uuid())
    .bind(event_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(map)?
    .ok_or(TalentPortError::Internal)?;
    Ok(Some(TalentCommitReceipt {
        aggregate_id: command.aggregate.id_uuid(),
        aggregate_version: command.aggregate.version(),
        event_id,
        global_sequence: u64::try_from(global_sequence).map_err(|_| TalentPortError::Internal)?,
        committed_at: row
            .try_get("committed_at")
            .map_err(|_| TalentPortError::Internal)?,
        replayed: true,
    }))
}

async fn lookup_receipt(
    tx: &mut Transaction<'_, Postgres>,
    tenant: TenantId,
    idempotency_key: &str,
) -> Result<Option<(Uuid, String, String, Uuid, TalentCommitReceipt)>, TalentPortError> {
    let row = sqlx::query(
        "SELECT command_id,request_digest,aggregate_type,aggregate_id,aggregate_version,committed_at
         FROM erpx_talent_write.command_receipts
         WHERE tenant_id=$1 AND idempotency_key=$2 FOR UPDATE",
    )
    .bind(tenant.as_uuid())
    .bind(idempotency_key)
    .fetch_optional(&mut **tx)
    .await
    .map_err(map)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let command_id: Uuid = row
        .try_get("command_id")
        .map_err(|_| TalentPortError::Internal)?;
    let request_digest: String = row
        .try_get("request_digest")
        .map_err(|_| TalentPortError::Internal)?;
    let aggregate_type: String = row
        .try_get("aggregate_type")
        .map_err(|_| TalentPortError::Internal)?;
    let aggregate_id: Uuid = row
        .try_get("aggregate_id")
        .map_err(|_| TalentPortError::Internal)?;
    let aggregate_version: i64 = row
        .try_get("aggregate_version")
        .map_err(|_| TalentPortError::Internal)?;
    let event_id = derived_id(command_id, b"projection");
    let global_sequence: i64 = sqlx::query_scalar(
        "SELECT global_sequence FROM erpx_cqrs_write.outbox_events
         WHERE tenant_id=$1 AND event_id=$2",
    )
    .bind(tenant.as_uuid())
    .bind(event_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(map)?
    .ok_or(TalentPortError::Internal)?;
    Ok(Some((
        command_id,
        request_digest,
        aggregate_type,
        aggregate_id,
        TalentCommitReceipt {
            aggregate_id,
            aggregate_version: u64::try_from(aggregate_version)
                .map_err(|_| TalentPortError::Internal)?,
            event_id,
            global_sequence: u64::try_from(global_sequence)
                .map_err(|_| TalentPortError::Internal)?,
            committed_at: row
                .try_get("committed_at")
                .map_err(|_| TalentPortError::Internal)?,
            replayed: true,
        },
    )))
}
async fn lock_version(
    tx: &mut Transaction<'_, Postgres>,
    tenant: TenantId,
    kind: &str,
    id: Uuid,
) -> Result<Option<u64>, TalentPortError> {
    let (table, column) = table(kind)?;
    let query = format!(
        "SELECT aggregate_version FROM erpx_talent_write.{table} WHERE tenant_id=$1 AND {column}=$2 FOR UPDATE"
    );
    let value: Option<i64> = sqlx::query_scalar(&query)
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(map)?;
    value
        .map(|v| u64::try_from(v).map_err(|_| TalentPortError::Internal))
        .transpose()
}
fn decode(kind: &str, document: serde_json::Value) -> Result<TalentAggregate, TalentPortError> {
    serde_json::from_value(serde_json::json!({"aggregate":variant(kind)?,"state":document}))
        .map_err(|_| TalentPortError::Internal)
}
fn table(kind: &str) -> Result<(&'static str, &'static str), TalentPortError> {
    match kind {
        "talent_requisition" => Ok(("requisitions", "requisition_id")),
        "talent_candidate" => Ok(("candidates", "candidate_id")),
        "talent_application" => Ok(("applications", "application_id")),
        "talent_offer" => Ok(("offers", "offer_id")),
        "talent_hire_handoff" => Ok(("hire_handoffs", "handoff_id")),
        "talent_proposal" => Ok(("proposals", "proposal_id")),
        "talent_interview" | "talent_review_cycle" => Ok(("governed_records", "record_id")),
        _ => Err(TalentPortError::Invalid),
    }
}
fn variant(kind: &str) -> Result<&'static str, TalentPortError> {
    match kind {
        "talent_requisition" => Ok("requisition"),
        "talent_candidate" => Ok("candidate"),
        "talent_application" => Ok("application"),
        "talent_offer" => Ok("offer"),
        "talent_hire_handoff" => Ok("hire_handoff"),
        "talent_proposal" => Ok("proposal"),
        "talent_interview" | "talent_review_cycle" => Ok("record"),
        _ => Err(TalentPortError::Invalid),
    }
}
fn scope_key(scope: &TenantScope) -> Result<String, TalentPortError> {
    match scope {
        TenantScope::Company {
            holding_id,
            company_id,
        } => Ok(format!("h:{holding_id}/c:{company_id}")),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok(format!("h:{holding_id}/c:{company_id}/b:{branch_id}")),
        _ => Err(TalentPortError::Invalid),
    }
}
fn enum_name<T: serde::Serialize>(value: T) -> Result<String, TalentPortError> {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .ok_or(TalentPortError::Invalid)
}
fn projection(
    kind: &str,
    id: Uuid,
    row: sqlx::postgres::PgRow,
) -> Result<TalentProjection, TalentPortError> {
    let version: i64 = row
        .try_get("source_aggregate_version")
        .map_err(|_| TalentPortError::Internal)?;
    let position: i64 = row
        .try_get("source_global_sequence")
        .map_err(|_| TalentPortError::Internal)?;
    let realm: String = row
        .try_get("authority_realm")
        .map_err(|_| TalentPortError::Internal)?;
    Ok(TalentProjection {
        aggregate_type: kind.into(),
        aggregate_id: id,
        scope: serde_json::from_value(
            row.try_get("tenant_scope")
                .map_err(|_| TalentPortError::Internal)?,
        )
        .map_err(|_| TalentPortError::Internal)?,
        authority_realm: serde_json::from_value(realm.into())
            .map_err(|_| TalentPortError::Internal)?,
        candidate_principal_id: row
            .try_get("candidate_principal_id")
            .map_err(|_| TalentPortError::Internal)?,
        lifecycle_status: row
            .try_get("lifecycle_status")
            .map_err(|_| TalentPortError::Internal)?,
        generation: row
            .try_get("generation")
            .map_err(|_| TalentPortError::Internal)?,
        aggregate_version: u64::try_from(version).map_err(|_| TalentPortError::Internal)?,
        source_position: u64::try_from(position).map_err(|_| TalentPortError::Internal)?,
        checkpoint_position: u64::try_from(
            row.try_get::<i64, _>("checkpoint_position")
                .map_err(|_| TalentPortError::Internal)?,
        )
        .map_err(|_| TalentPortError::Internal)?,
        projected_at: row
            .try_get("projected_at")
            .map_err(|_| TalentPortError::Internal)?,
    })
}
fn derived_id(seed: Uuid, label: &[u8]) -> Uuid {
    let mut hash = Sha256::new();
    hash.update(seed.as_bytes());
    hash.update(label);
    let mut bytes: [u8; 16] = hash.finalize()[..16].try_into().expect("digest");
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}
fn to_i64(v: u64) -> Result<i64, TalentPortError> {
    i64::try_from(v).map_err(|_| TalentPortError::Invalid)
}
fn map(_: sqlx::Error) -> TalentPortError {
    TalentPortError::Internal
}

#[cfg(test)]
mod tests {
    #[test]
    fn command_path_is_tenant_bound_atomic_and_normalized() {
        let source = include_str!("repository.rs");
        for marker in [
            "set_config('app.tenant_id'",
            "FOR UPDATE",
            "audit_log",
            "command_receipts",
            "outbox",
            "persist_normalized",
        ] {
            assert!(source.contains(marker));
        }
    }
}
