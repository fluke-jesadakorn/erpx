#![doc = "PostgreSQL CQRS adapters for Field Service."]
#![forbid(unsafe_code)]

mod directory;
mod projection;
pub use projection::{FieldServiceOrderSnapshot, FieldServiceProjector};
pub const PROJECTION_NAME: &str = "field_service.orders";
pub const SNAPSHOT_EVENT_TYPE: &str = "field_service.order.snapshot.v1";
pub const RECORD_SNAPSHOT_EVENT_TYPE: &str = "field_service.record.snapshot.v1";

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs_sqlx::ReadPool;
use erpx_field_service::{
    AuthorizedFieldServiceAccess, CommitReceipt, FieldServiceAggregate, FieldServiceCommit,
    FieldServiceReadRepository, FieldServiceRecord, FieldServiceWriteRepository, PortError,
    ServiceAgreement, ServiceAgreementId, ServiceOrder, ServiceOrderId, ServiceOrderProjection,
};
use sqlx::{PgPool, Postgres, Row, Transaction};

#[derive(Debug, Clone)]
pub struct SqlxFieldServiceWriteRepository {
    pool: PgPool,
}
impl SqlxFieldServiceWriteRepository {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
#[async_trait]
impl FieldServiceWriteRepository for SqlxFieldServiceWriteRepository {
    async fn load_agreement(
        &self,
        a: &AuthorizedFieldServiceAccess,
        id: ServiceAgreementId,
    ) -> Result<Option<ServiceAgreement>, PortError> {
        load(&self.pool, a, "agreement", id.as_uuid()).await
    }
    async fn load_order(
        &self,
        a: &AuthorizedFieldServiceAccess,
        id: ServiceOrderId,
    ) -> Result<Option<ServiceOrder>, PortError> {
        load(&self.pool, a, "order", id.as_uuid()).await
    }
    async fn load_record(
        &self,
        a: &AuthorizedFieldServiceAccess,
        id: uuid::Uuid,
    ) -> Result<Option<FieldServiceRecord>, PortError> {
        load_any(&self.pool, a, id).await
    }
    async fn commit(&self, r: FieldServiceCommit) -> Result<CommitReceipt, PortError> {
        r.identity.validate()?;
        valid(&r.authority, r.aggregate.scope())?;
        let holding = r.authority.actor.holding_id;
        let key = r.aggregate.key();
        let version = r.aggregate.version();
        let (kind, id, document) = match &r.aggregate {
            FieldServiceAggregate::Agreement(v) => {
                ("agreement", v.id().as_uuid(), serde_json::to_value(v))
            }
            FieldServiceAggregate::Order(v) => ("order", v.id().as_uuid(), serde_json::to_value(v)),
            FieldServiceAggregate::Record(v) => (v.kind().token(), v.id(), serde_json::to_value(v)),
        };
        let document = document.map_err(|_| PortError::InvalidState)?;
        let mut tx = self.pool.begin().await.map_err(map)?;
        set_holding(&mut tx, holding.as_uuid()).await?;
        if let Some(row) = sqlx::query("SELECT command_id,request_digest,aggregate_key,aggregate_version,committed_at FROM erpx_field_service_write.command_receipts WHERE holding_id=$1 AND idempotency_key=$2")
            .bind(holding.as_uuid())
            .bind(&r.identity.idempotency_key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map)?
        {
            let command: uuid::Uuid = row.try_get("command_id").map_err(|_| PortError::Internal)?;
            let digest: String = row.try_get("request_digest").map_err(|_| PortError::Internal)?;
            let stored: String = row.try_get("aggregate_key").map_err(|_| PortError::Internal)?;
            let stored_version: i64 = row.try_get("aggregate_version").map_err(|_| PortError::Internal)?;
            if command != r.identity.command_id.as_uuid()
                || digest != r.identity.request_digest
                || stored != key
                || u64::try_from(stored_version).ok() != Some(version)
            {
                return Err(PortError::Conflict);
            }
            let committed_at = row.try_get("committed_at").map_err(|_| PortError::Internal)?;
            let (event_id, global_sequence) = command_event(&mut tx, holding.as_uuid(), command).await?;
            tx.commit().await.map_err(map)?;
            return Ok(CommitReceipt {
                aggregate_key: stored,
                version,
                committed_at,
                replayed: true,
                event_id,
                global_sequence,
            });
        }
        let current=sqlx::query_scalar::<_,i64>("SELECT aggregate_version FROM erpx_field_service_write.aggregate_versions WHERE holding_id=$1 AND aggregate_key=$2 FOR UPDATE").bind(holding.as_uuid()).bind(&key).fetch_optional(&mut *tx).await.map_err(map)?;
        match (r.expected_version, current) {
            (None, None) if version == 1 => {}
            (Some(e), Some(v))
                if i64::try_from(e).ok() == Some(v)
                    && version == e.checked_add(1).ok_or(PortError::Conflict)? => {}
            _ => return Err(PortError::Conflict),
        }
        let scope =
            serde_json::to_value(r.aggregate.scope()).map_err(|_| PortError::InvalidState)?;
        let projection_document = document.clone();
        sqlx::query("SELECT erpx_field_service_write.persist_normalized($1,$2,$3,$4,$5,$6,$7)")
            .bind(holding.as_uuid())
            .bind(&key)
            .bind(kind)
            .bind(id)
            .bind(&scope)
            .bind(i64::try_from(version).map_err(|_| PortError::InvalidState)?)
            .bind(document)
            .execute(&mut *tx)
            .await
            .map_err(map)?;
        sqlx::query("INSERT INTO erpx_field_service_write.audit_log (holding_id,command_id,actor_principal_id,action,authority_scope,aggregate_key,aggregate_version,correlation_id,evidence_digest) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(holding.as_uuid()).bind(r.identity.command_id.as_uuid()).bind(r.authority.actor.principal_id.as_uuid()).bind(&r.event_type).bind(scope).bind(&key).bind(i64::try_from(version).map_err(|_|PortError::InvalidState)?).bind(r.identity.correlation_id.as_uuid()).bind(&r.identity.request_digest).execute(&mut *tx).await.map_err(map)?;
        sqlx::query("INSERT INTO erpx_field_service_write.outbox (holding_id,aggregate_key,aggregate_version,event_type,correlation_id,payload) VALUES ($1,$2,$3,$4,$5,$6)").bind(holding.as_uuid()).bind(&key).bind(i64::try_from(version).map_err(|_|PortError::InvalidState)?).bind(r.event_type).bind(r.identity.correlation_id.as_uuid()).bind(r.event_payload).execute(&mut *tx).await.map_err(map)?;
        if kind == "order" {
            let actor = serde_json::json!({"principal_ref":format!("principal:{}",r.authority.actor.principal_id.as_uuid()),"kind":"holding_user","session_id":null,"represented_principal_ref":null,"support_capability_id":null});
            sqlx::query("INSERT INTO erpx_cqrs_write.outbox_events (holding_id,event_id,context_name,partition_key,command_id,event_type,event_version,aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,causation_id,payload,occurred_at,available_at) VALUES ($1,$2,'field_service',$3,$2,$4,1,'order',$5,$6,$7,$8,$9,NULL,$10,clock_timestamp(),clock_timestamp())")
                .bind(holding.as_uuid()).bind(r.identity.command_id.as_uuid()).bind(format!("field_service.order:{}",id.simple())).bind(SNAPSHOT_EVENT_TYPE).bind(id).bind(i64::try_from(version).map_err(|_|PortError::InvalidState)?).bind(serde_json::to_value(&r.authority.target_scope).map_err(|_|PortError::InvalidState)?).bind(actor).bind(r.identity.correlation_id.as_uuid()).bind(serde_json::json!({"order":projection_document})).execute(&mut *tx).await.map_err(map)?;
        } else if matches!(kind, "request" | "dispatch" | "visit" | "handoff") {
            let actor = serde_json::json!({"principal_ref":format!("principal:{}",r.authority.actor.principal_id.as_uuid()),"kind":"holding_user","session_id":null,"represented_principal_ref":null,"support_capability_id":null});
            sqlx::query("INSERT INTO erpx_cqrs_write.outbox_events (holding_id,event_id,context_name,partition_key,command_id,event_type,event_version,aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,causation_id,payload,occurred_at,available_at) VALUES ($1,$2,'field_service',$3,$2,$4,1,$5,$6,$7,$8,$9,$10,NULL,$11,clock_timestamp(),clock_timestamp())")
                .bind(holding.as_uuid()).bind(r.identity.command_id.as_uuid()).bind(format!("field_service.{kind}:{}",id.simple())).bind(RECORD_SNAPSHOT_EVENT_TYPE).bind(kind).bind(id).bind(i64::try_from(version).map_err(|_|PortError::InvalidState)?).bind(serde_json::to_value(&r.authority.target_scope).map_err(|_|PortError::InvalidState)?).bind(actor).bind(r.identity.correlation_id.as_uuid()).bind(serde_json::json!({"record":projection_document})).execute(&mut *tx).await.map_err(map)?;
        }
        if let Some(draft) = directory::field_service_directory_draft(&r.aggregate)? {
            let actor = serde_json::json!({"principal_ref":format!("principal:{}",r.authority.actor.principal_id.as_uuid()),"kind":"holding_user","session_id":null,"represented_principal_ref":null,"support_capability_id":null});
            directory::append_directory_draft(
                &mut tx,
                holding,
                r.identity.command_id,
                r.identity.correlation_id,
                actor,
                &draft,
            )
            .await?;
        }
        let (event_id, global_sequence) =
            command_event(&mut tx, holding.as_uuid(), r.identity.command_id.as_uuid()).await?;
        let committed_at:DateTime<Utc>=sqlx::query_scalar("INSERT INTO erpx_field_service_write.command_receipts (holding_id,idempotency_key,command_id,request_digest,aggregate_key,aggregate_version) VALUES ($1,$2,$3,$4,$5,$6) RETURNING committed_at").bind(holding.as_uuid()).bind(r.identity.idempotency_key).bind(r.identity.command_id.as_uuid()).bind(r.identity.request_digest).bind(&key).bind(i64::try_from(version).map_err(|_|PortError::InvalidState)?).fetch_one(&mut *tx).await.map_err(map)?;
        tx.commit().await.map_err(map)?;
        Ok(CommitReceipt {
            aggregate_key: key,
            version,
            committed_at,
            replayed: false,
            event_id,
            global_sequence,
        })
    }
}

async fn command_event(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: uuid::Uuid,
    command_id: uuid::Uuid,
) -> Result<(uuid::Uuid, u64), PortError> {
    let (event_id, global_sequence): (uuid::Uuid, i64) = sqlx::query_as(
        "SELECT event_id,global_sequence FROM erpx_cqrs_write.outbox_events WHERE holding_id=$1 AND command_id=$2 ORDER BY global_sequence DESC LIMIT 1",
    )
    .bind(holding_id)
    .bind(command_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(map)?;
    Ok((
        event_id,
        u64::try_from(global_sequence).map_err(|_| PortError::Internal)?,
    ))
}
async fn load_any<T: serde::de::DeserializeOwned>(
    pool: &PgPool,
    a: &AuthorizedFieldServiceAccess,
    id: uuid::Uuid,
) -> Result<Option<T>, PortError> {
    valid(a, &a.target_scope)?;
    let mut tx = pool.begin().await.map_err(map)?;
    set_holding(&mut tx, a.actor.holding_id.as_uuid()).await?;
    let row=sqlx::query("SELECT authority_scope,aggregate_document FROM erpx_field_service_write.aggregate_rehydration WHERE holding_id=$1 AND aggregate_kind IN ('request','dispatch','visit','handoff') AND aggregate_id=$2").bind(a.actor.holding_id.as_uuid()).bind(id).fetch_optional(&mut *tx).await.map_err(map)?;
    tx.commit().await.map_err(map)?;
    let Some(row) = row else { return Ok(None) };
    let scope: erpx_kernel::AuthorityScope = serde_json::from_value(
        row.try_get("authority_scope")
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    if scope != a.target_scope {
        return Err(PortError::Internal);
    }
    serde_json::from_value(
        row.try_get("aggregate_document")
            .map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}
async fn load<T: serde::de::DeserializeOwned>(
    pool: &PgPool,
    a: &AuthorizedFieldServiceAccess,
    kind: &str,
    id: uuid::Uuid,
) -> Result<Option<T>, PortError> {
    valid(a, &a.target_scope)?;
    let mut tx = pool.begin().await.map_err(map)?;
    set_holding(&mut tx, a.actor.holding_id.as_uuid()).await?;
    let row=sqlx::query("SELECT authority_scope,aggregate_document FROM erpx_field_service_write.aggregate_rehydration WHERE holding_id=$1 AND aggregate_kind=$2 AND aggregate_id=$3").bind(a.actor.holding_id.as_uuid()).bind(kind).bind(id).fetch_optional(&mut *tx).await.map_err(map)?;
    tx.commit().await.map_err(map)?;
    let Some(row) = row else { return Ok(None) };
    let scope: erpx_kernel::AuthorityScope = serde_json::from_value(
        row.try_get("authority_scope")
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    if scope != a.target_scope {
        return Err(PortError::Internal);
    };
    serde_json::from_value(
        row.try_get("aggregate_document")
            .map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

#[derive(Debug, Clone)]
pub struct SqlxFieldServiceReadRepository {
    pool: ReadPool,
    holding_id: erpx_kernel::HoldingId,
}
impl SqlxFieldServiceReadRepository {
    pub fn new(pool: PgPool, holding_id: erpx_kernel::HoldingId) -> Result<Self, PortError> {
        let pool = ReadPool::for_holding(pool, holding_id.as_uuid())
            .map_err(|_| PortError::Unavailable)?;
        Ok(Self { pool, holding_id })
    }
}
#[async_trait]
impl FieldServiceReadRepository for SqlxFieldServiceReadRepository {
    async fn get_order(
        &self,
        a: &AuthorizedFieldServiceAccess,
        id: ServiceOrderId,
    ) -> Result<Option<ServiceOrderProjection>, PortError> {
        valid(a, &a.target_scope)?;
        if a.actor.holding_id != self.holding_id {
            return Err(PortError::Denied);
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let generation:Option<uuid::Uuid>=sqlx::query_scalar("SELECT generation FROM erpx_cqrs_read.projection_generations WHERE holding_id=$1 AND projection_name='field_service.orders' AND status='active'").bind(self.holding_id.as_uuid()).fetch_optional(&mut *tx).await.map_err(map)?;
        let generation = generation.ok_or(PortError::Unavailable)?;
        let row=sqlx::query("SELECT p.authority_scope,p.projection,p.generation,p.partition_key,p.source_position,p.projected_at,c.global_sequence AS checkpoint_position,EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_failures f WHERE f.holding_id=p.holding_id AND f.projection_name=p.projection_name AND f.generation=p.generation AND f.partition_key=p.partition_key) AS has_failure FROM erpx_field_service_read.order_projection p LEFT JOIN erpx_cqrs_read.projection_checkpoints c ON c.holding_id=p.holding_id AND c.projection_name=p.projection_name AND c.generation=p.generation AND c.partition_key=p.partition_key WHERE p.holding_id=$1 AND p.projection_name='field_service.orders' AND p.generation=$2 AND p.service_order_id=$3").bind(self.holding_id.as_uuid()).bind(generation).bind(id.as_uuid()).fetch_optional(&mut *tx).await.map_err(map)?;
        tx.commit().await.map_err(map)?;
        let Some(row) = row else { return Ok(None) };
        let source_position = u64::try_from(
            row.try_get::<i64, _>("source_position")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        let checkpoint_position = row
            .try_get::<Option<i64>, _>("checkpoint_position")
            .map_err(|_| PortError::Internal)?
            .and_then(|v| u64::try_from(v).ok())
            .ok_or(PortError::Stale)?;
        if row
            .try_get::<bool, _>("has_failure")
            .map_err(|_| PortError::Internal)?
            || checkpoint_position < source_position
        {
            return Err(PortError::Stale);
        }
        let scope: erpx_kernel::AuthorityScope = serde_json::from_value(
            row.try_get("authority_scope")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        if scope != a.target_scope {
            return Err(PortError::Internal);
        };
        Ok(Some(ServiceOrderProjection {
            order: serde_json::from_value(
                row.try_get("projection").map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            generation,
            source_position,
            checkpoint_position,
            projected_at: row
                .try_get("projected_at")
                .map_err(|_| PortError::Internal)?,
        }))
    }

    async fn get_record(
        &self,
        a: &AuthorizedFieldServiceAccess,
        id: uuid::Uuid,
    ) -> Result<Option<erpx_field_service::FieldServiceRecordProjection>, PortError> {
        valid(a, &a.target_scope)?;
        if a.actor.holding_id != self.holding_id {
            return Err(PortError::Denied);
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let generation: Option<uuid::Uuid> = sqlx::query_scalar("SELECT generation FROM erpx_cqrs_read.projection_generations WHERE holding_id=$1 AND projection_name=$2 AND status='active'")
            .bind(self.holding_id.as_uuid()).bind(PROJECTION_NAME).fetch_optional(&mut *tx).await.map_err(map)?;
        let generation = generation.ok_or(PortError::Unavailable)?;
        let row = sqlx::query("SELECT p.authority_scope,p.projection,p.source_position,p.projected_at,c.global_sequence AS checkpoint_position,EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_failures f WHERE f.holding_id=p.holding_id AND f.projection_name=p.projection_name AND f.generation=p.generation AND f.partition_key=p.partition_key) AS has_failure FROM erpx_field_service_read.governed_record_projection p LEFT JOIN erpx_cqrs_read.projection_checkpoints c ON c.holding_id=p.holding_id AND c.projection_name=p.projection_name AND c.generation=p.generation AND c.partition_key=p.partition_key WHERE p.holding_id=$1 AND p.projection_name=$2 AND p.generation=$3 AND p.record_id=$4")
            .bind(self.holding_id.as_uuid()).bind(PROJECTION_NAME).bind(generation).bind(id).fetch_optional(&mut *tx).await.map_err(map)?;
        tx.commit().await.map_err(map)?;
        let Some(row) = row else { return Ok(None) };
        let (source_position, checkpoint_position) = field_record_projection_positions(&row)?;
        let scope: erpx_kernel::AuthorityScope = serde_json::from_value(
            row.try_get("authority_scope")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        if scope != a.target_scope {
            return Err(PortError::Internal);
        }
        Ok(Some(erpx_field_service::FieldServiceRecordProjection {
            record: serde_json::from_value(
                row.try_get("projection").map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            generation,
            source_position,
            checkpoint_position,
            projected_at: row
                .try_get("projected_at")
                .map_err(|_| PortError::Internal)?,
        }))
    }
}
fn field_record_projection_positions(row: &sqlx::postgres::PgRow) -> Result<(u64, u64), PortError> {
    let source = u64::try_from(
        row.try_get::<i64, _>("source_position")
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    let checkpoint = row
        .try_get::<Option<i64>, _>("checkpoint_position")
        .map_err(|_| PortError::Internal)?
        .and_then(|v| u64::try_from(v).ok())
        .ok_or(PortError::Stale)?;
    if row
        .try_get::<bool, _>("has_failure")
        .map_err(|_| PortError::Internal)?
        || checkpoint < source
    {
        return Err(PortError::Stale);
    }
    Ok((source, checkpoint))
}
fn valid(
    a: &AuthorizedFieldServiceAccess,
    s: &erpx_kernel::AuthorityScope,
) -> Result<(), PortError> {
    if a.actor.authority_scope.contains(s) && &a.target_scope == s {
        Ok(())
    } else {
        Err(PortError::Denied)
    }
}
async fn set_holding(
    tx: &mut Transaction<'_, Postgres>,
    holding: uuid::Uuid,
) -> Result<(), PortError> {
    sqlx::query("SELECT set_config('app.holding_id',$1,true)")
        .bind(holding.to_string())
        .execute(&mut **tx)
        .await
        .map_err(map)?;
    Ok(())
}
fn map(e: sqlx::Error) -> PortError {
    match &e {
        sqlx::Error::Database(d) if d.code().as_deref() == Some("23505") => PortError::Conflict,
        _ => PortError::Internal,
    }
}

#[cfg(test)]
#[path = "../../schema_test.rs"]
mod schema_test;
#[cfg(test)]
mod tests {
    use crate::schema_test;
    const W: &str = include_str!("../../../migration/migrations_holding_write/0001_baseline.sql");
    const R: &str = include_str!("../../../migration/migrations_holding_read/0001_baseline.sql");
    #[test]
    fn schema_persists_parts_evidence_exceptions_and_billing() {
        assert!(W.contains("part_issue_guards"));
        assert!(W.contains("visit_evidence_guards"));
        assert!(W.contains("exception_guards"));
        assert!(W.contains("billing_handoff_guards"));
        assert!(R.contains("dispatch_projection"));
        assert!(W.contains("FORCE ROW LEVEL SECURITY"));
    }
    #[test]
    fn read_side_has_no_write_fallback() {
        let s = include_str!("lib.rs");
        let r = s
            .split("pub struct SqlxFieldServiceReadRepository")
            .nth(1)
            .unwrap();
        assert!(!r.contains("erpx_field_service_write.aggregate_versions"));
        assert!(r.contains("governed_record_projection"));
        assert!(r.contains("status='active'"));
    }
    #[test]
    fn normalized_write_path_is_atomic_and_rehydratable() {
        let owned = schema_test::select_schema_statements(W, &["erpx_field_service_write"]);
        assert!(!owned.contains("aggregate_snapshots"));
        assert!(!owned.contains("snapshot jsonb"));
        for marker in [
            "agreements",
            "service_orders",
            "persist_normalized",
            "aggregate_rehydration",
            "audit_log",
            "outbox",
        ] {
            assert!(W.contains(marker));
        }
        let source = include_str!("lib.rs");
        assert!(source.contains("ReadPool::for_holding"));
        assert!(source.contains("status='active'"));
        let read = source
            .split("pub struct SqlxFieldServiceReadRepository")
            .nth(1)
            .unwrap()
            .split("fn valid")
            .next()
            .unwrap();
        assert!(!read.contains(&["ORDER BY projected_at DESC", " LIMIT 1"].concat()));
    }
}
