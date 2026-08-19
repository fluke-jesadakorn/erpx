#![doc = "PostgreSQL CQRS adapters for the Manufacturing bounded context."]
#![forbid(unsafe_code)]

mod projection;
pub use projection::{ManufacturingOrderSnapshot, ManufacturingProjector};

mod directory;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs_sqlx::ReadPool;
use erpx_manufacturing::{
    AuthorizedManufacturingAccess, CommitReceipt, ManufacturingAggregate, ManufacturingCommit,
    ManufacturingOrderProjection, ManufacturingReadRepository, ManufacturingRecord,
    ManufacturingSpecification, ManufacturingSpecificationProjection, ManufacturingWriteRepository,
    PortError, ProductionOrder, ProductionOrderId, SpecificationId,
};
use sqlx::{PgPool, Postgres, Row, Transaction};

pub const EVENT_VERSION: u32 = 1;
pub const PROJECTION_NAME: &str = "manufacturing.orders";
pub const SNAPSHOT_EVENT_TYPE: &str = "manufacturing.order.snapshot.v1";
pub const RECORD_SNAPSHOT_EVENT_TYPE: &str = "manufacturing.record.snapshot.v1";
pub const SPECIFICATION_SNAPSHOT_EVENT_TYPE: &str = "manufacturing.specification.snapshot.v1";

#[derive(Debug, Clone)]
pub struct SqlxManufacturingWriteRepository {
    pool: PgPool,
}
impl SqlxManufacturingWriteRepository {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait]
impl ManufacturingWriteRepository for SqlxManufacturingWriteRepository {
    async fn load_specification(
        &self,
        authority: &AuthorizedManufacturingAccess,
        id: SpecificationId,
    ) -> Result<Option<ManufacturingSpecification>, PortError> {
        load_aggregate(&self.pool, authority, "specification", id.as_uuid()).await
    }
    async fn load_order(
        &self,
        authority: &AuthorizedManufacturingAccess,
        id: ProductionOrderId,
    ) -> Result<Option<ProductionOrder>, PortError> {
        load_aggregate(&self.pool, authority, "order", id.as_uuid()).await
    }
    async fn load_record(
        &self,
        authority: &AuthorizedManufacturingAccess,
        id: uuid::Uuid,
    ) -> Result<Option<ManufacturingRecord>, PortError> {
        load_aggregate_any_kind(&self.pool, authority, id).await
    }
    async fn commit(&self, request: ManufacturingCommit) -> Result<CommitReceipt, PortError> {
        request.identity.validate()?;
        if !request
            .authority
            .actor
            .authority_scope
            .contains(&request.authority.target_scope)
            || request.aggregate.scope() != &request.authority.target_scope
        {
            return Err(PortError::Denied);
        }
        let holding_id = request.authority.actor.holding_id;
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        set_holding(&mut tx, holding_id.as_uuid()).await?;
        if let Some(row) = sqlx::query("SELECT command_id,request_digest,aggregate_keys,aggregate_versions,committed_at FROM erpx_manufacturing_write.command_receipts WHERE holding_id=$1 AND idempotency_key=$2")
            .bind(holding_id.as_uuid()).bind(&request.identity.idempotency_key).fetch_optional(&mut *tx).await.map_err(map_error)? {
            let command_id: uuid::Uuid = row.try_get("command_id").map_err(|_| PortError::Internal)?;
            let digest: String = row.try_get("request_digest").map_err(|_| PortError::Internal)?;
            if command_id != request.identity.command_id.as_uuid() || digest != request.identity.request_digest { return Err(PortError::Conflict); }
            let keys: Vec<String> = row.try_get("aggregate_keys").map_err(|_| PortError::Internal)?;
            let versions: Vec<i64> = row.try_get("aggregate_versions").map_err(|_| PortError::Internal)?;
            if keys != vec![request.aggregate.aggregate_key()] || versions != vec![i64::try_from(request.aggregate.version()).map_err(|_| PortError::InvalidState)?] { return Err(PortError::Conflict); }
            let committed_at = row.try_get("committed_at").map_err(|_| PortError::Internal)?;
            tx.commit().await.map_err(map_error)?;
            return Ok(CommitReceipt { aggregate_key: keys[0].clone(), version: u64::try_from(versions[0]).map_err(|_| PortError::Internal)?, committed_at, replayed: true });
        }
        let key = request.aggregate.aggregate_key();
        let version = request.aggregate.version();
        let (kind, id) = match &request.aggregate {
            ManufacturingAggregate::Specification(value) => (
                "specification",
                value.reference().specification_id.as_uuid(),
            ),
            ManufacturingAggregate::Order(value) => ("order", value.id().as_uuid()),
            ManufacturingAggregate::Record(value) => (value.kind().token(), value.id()),
        };
        let current = sqlx::query_scalar::<_, i64>("SELECT aggregate_version FROM erpx_manufacturing_write.aggregate_versions WHERE holding_id=$1 AND aggregate_key=$2 FOR UPDATE").bind(holding_id.as_uuid()).bind(&key).fetch_optional(&mut *tx).await.map_err(map_error)?;
        match (request.expected_version, current) {
            (None, None) if version == 1 => {}
            (Some(expected), Some(actual))
                if i64::try_from(expected).ok() == Some(actual)
                    && version == expected.checked_add(1).ok_or(PortError::Conflict)? => {}
            _ => return Err(PortError::Conflict),
        }
        let document = match &request.aggregate {
            ManufacturingAggregate::Specification(value) => serde_json::to_value(value),
            ManufacturingAggregate::Order(value) => serde_json::to_value(value),
            ManufacturingAggregate::Record(value) => serde_json::to_value(value),
        }
        .map_err(|_| PortError::InvalidState)?;
        let scope =
            serde_json::to_value(request.aggregate.scope()).map_err(|_| PortError::InvalidState)?;
        let projection_document = document.clone();
        sqlx::query("SELECT erpx_manufacturing_write.persist_normalized($1,$2,$3,$4,$5,$6,$7)")
            .bind(holding_id.as_uuid())
            .bind(&key)
            .bind(kind)
            .bind(id)
            .bind(&scope)
            .bind(i64::try_from(version).map_err(|_| PortError::InvalidState)?)
            .bind(document)
            .execute(&mut *tx)
            .await
            .map_err(map_error)?;
        sqlx::query("INSERT INTO erpx_manufacturing_write.audit_log (holding_id,command_id,actor_principal_id,action,authority_scope,aggregate_key,aggregate_version,correlation_id,evidence_digest) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(holding_id.as_uuid()).bind(request.identity.command_id.as_uuid()).bind(request.authority.actor.principal_id.as_uuid())
            .bind(&request.event_type).bind(scope).bind(&key).bind(i64::try_from(version).map_err(|_| PortError::InvalidState)?)
            .bind(request.correlation_id.as_uuid()).bind(&request.identity.request_digest).execute(&mut *tx).await.map_err(map_error)?;
        sqlx::query("INSERT INTO erpx_manufacturing_write.outbox (holding_id,aggregate_key,aggregate_version,event_type,event_version,correlation_id,payload) VALUES ($1,$2,$3,$4,$5,$6,$7)")
            .bind(holding_id.as_uuid()).bind(&key).bind(i64::try_from(version).map_err(|_| PortError::InvalidState)?).bind(&request.event_type).bind(i32::try_from(EVENT_VERSION).map_err(|_| PortError::InvalidState)?).bind(request.correlation_id.as_uuid()).bind(request.event_payload).execute(&mut *tx).await.map_err(map_error)?;
        if kind == "specification" {
            let actor = serde_json::json!({"principal_ref":format!("principal:{}",request.authority.actor.principal_id.as_uuid()),"kind":"holding_user","session_id":null,"represented_principal_ref":null,"support_capability_id":null});
            sqlx::query("INSERT INTO erpx_cqrs_write.outbox_events (holding_id,event_id,context_name,partition_key,command_id,event_type,event_version,aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,causation_id,payload,occurred_at,available_at) VALUES ($1,$2,'manufacturing',$3,$2,$4,1,'specification',$5,$6,$7,$8,$9,NULL,$10,clock_timestamp(),clock_timestamp())")
                .bind(holding_id.as_uuid()).bind(request.identity.command_id.as_uuid()).bind(format!("manufacturing.specification:{}",id.simple())).bind(SPECIFICATION_SNAPSHOT_EVENT_TYPE).bind(id).bind(i64::try_from(version).map_err(|_|PortError::InvalidState)?).bind(serde_json::to_value(&request.authority.target_scope).map_err(|_|PortError::InvalidState)?).bind(actor).bind(request.correlation_id.as_uuid()).bind(serde_json::json!({"specification":projection_document})).execute(&mut *tx).await.map_err(map_error)?;
        } else if kind == "order" {
            let payload = serde_json::json!({"order":projection_document});
            let actor = serde_json::json!({"principal_ref":format!("principal:{}",request.authority.actor.principal_id.as_uuid()),"kind":"holding_user","session_id":null,"represented_principal_ref":null,"support_capability_id":null});
            sqlx::query("INSERT INTO erpx_cqrs_write.outbox_events (holding_id,event_id,context_name,partition_key,command_id,event_type,event_version,aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,causation_id,payload,occurred_at,available_at) VALUES ($1,$2,'manufacturing',$3,$2,$4,1,'order',$5,$6,$7,$8,$9,NULL,$10,clock_timestamp(),clock_timestamp())")
                .bind(holding_id.as_uuid()).bind(request.identity.command_id.as_uuid()).bind(format!("manufacturing.order:{}",id.simple())).bind(SNAPSHOT_EVENT_TYPE).bind(id).bind(i64::try_from(version).map_err(|_|PortError::InvalidState)?).bind(serde_json::to_value(&request.authority.target_scope).map_err(|_|PortError::InvalidState)?).bind(actor).bind(request.correlation_id.as_uuid()).bind(payload).execute(&mut *tx).await.map_err(map_error)?;
        } else if matches!(kind, "routing" | "batch" | "result") {
            let actor = serde_json::json!({"principal_ref":format!("principal:{}",request.authority.actor.principal_id.as_uuid()),"kind":"holding_user","session_id":null,"represented_principal_ref":null,"support_capability_id":null});
            sqlx::query("INSERT INTO erpx_cqrs_write.outbox_events (holding_id,event_id,context_name,partition_key,command_id,event_type,event_version,aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,causation_id,payload,occurred_at,available_at) VALUES ($1,$2,'manufacturing',$3,$2,$4,1,$5,$6,$7,$8,$9,$10,NULL,$11,clock_timestamp(),clock_timestamp())")
                .bind(holding_id.as_uuid()).bind(request.identity.command_id.as_uuid()).bind(format!("manufacturing.record:{}",id.simple())).bind(RECORD_SNAPSHOT_EVENT_TYPE).bind(kind).bind(id).bind(i64::try_from(version).map_err(|_|PortError::InvalidState)?).bind(serde_json::to_value(&request.authority.target_scope).map_err(|_|PortError::InvalidState)?).bind(actor).bind(request.correlation_id.as_uuid()).bind(serde_json::json!({"record":projection_document})).execute(&mut *tx).await.map_err(map_error)?;
        }
        let directory_actor = serde_json::json!({"principal_ref":format!("principal:{}",request.authority.actor.principal_id.as_uuid()),"kind":"holding_user","session_id":null,"represented_principal_ref":null,"support_capability_id":null});
        for draft in directory::manufacturing_directory_drafts(&request.aggregate)? {
            directory::append_directory_draft(
                &mut tx,
                holding_id,
                request.identity.command_id,
                request.correlation_id,
                directory_actor.clone(),
                &draft,
            )
            .await?;
        }
        let committed_at: DateTime<Utc> = sqlx::query_scalar("INSERT INTO erpx_manufacturing_write.command_receipts (holding_id,idempotency_key,command_id,request_digest,aggregate_keys,aggregate_versions) VALUES ($1,$2,$3,$4,$5,$6) RETURNING committed_at")
            .bind(holding_id.as_uuid()).bind(&request.identity.idempotency_key).bind(request.identity.command_id.as_uuid()).bind(&request.identity.request_digest).bind(vec![key.clone()]).bind(vec![i64::try_from(version).map_err(|_| PortError::InvalidState)?]).fetch_one(&mut *tx).await.map_err(map_error)?;
        tx.commit().await.map_err(map_error)?;
        Ok(CommitReceipt {
            aggregate_key: key,
            version,
            committed_at,
            replayed: false,
        })
    }
}

async fn load_aggregate_any_kind<T: serde::de::DeserializeOwned>(
    pool: &PgPool,
    authority: &AuthorizedManufacturingAccess,
    id: uuid::Uuid,
) -> Result<Option<T>, PortError> {
    if !authority
        .actor
        .authority_scope
        .contains(&authority.target_scope)
    {
        return Err(PortError::Denied);
    }
    let mut tx = pool.begin().await.map_err(map_error)?;
    set_holding(&mut tx, authority.actor.holding_id.as_uuid()).await?;
    let row = sqlx::query("SELECT authority_scope,aggregate_document FROM erpx_manufacturing_write.aggregate_rehydration WHERE holding_id=$1 AND aggregate_kind IN ('routing','batch','result') AND aggregate_id=$2")
        .bind(authority.actor.holding_id.as_uuid()).bind(id).fetch_optional(&mut *tx).await.map_err(map_error)?;
    tx.commit().await.map_err(map_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let scope: erpx_kernel::AuthorityScope = serde_json::from_value(
        row.try_get("authority_scope")
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    if scope != authority.target_scope {
        return Err(PortError::Internal);
    }
    serde_json::from_value(
        row.try_get("aggregate_document")
            .map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn load_aggregate<T: serde::de::DeserializeOwned>(
    pool: &PgPool,
    authority: &AuthorizedManufacturingAccess,
    kind: &str,
    id: uuid::Uuid,
) -> Result<Option<T>, PortError> {
    if !authority
        .actor
        .authority_scope
        .contains(&authority.target_scope)
    {
        return Err(PortError::Denied);
    }
    let mut tx = pool.begin().await.map_err(map_error)?;
    set_holding(&mut tx, authority.actor.holding_id.as_uuid()).await?;
    let row = sqlx::query("SELECT authority_scope,aggregate_document FROM erpx_manufacturing_write.aggregate_rehydration WHERE holding_id=$1 AND aggregate_kind=$2 AND aggregate_id=$3").bind(authority.actor.holding_id.as_uuid()).bind(kind).bind(id).fetch_optional(&mut *tx).await.map_err(map_error)?;
    tx.commit().await.map_err(map_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let stored_scope: erpx_kernel::AuthorityScope = serde_json::from_value(
        row.try_get("authority_scope")
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    if stored_scope != authority.target_scope {
        return Err(PortError::Internal);
    }
    serde_json::from_value(
        row.try_get("aggregate_document")
            .map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

#[derive(Debug, Clone)]
pub struct SqlxManufacturingReadRepository {
    pool: ReadPool,
    holding_id: erpx_kernel::HoldingId,
}
impl SqlxManufacturingReadRepository {
    pub fn new(pool: PgPool, holding_id: erpx_kernel::HoldingId) -> Result<Self, PortError> {
        let pool = ReadPool::for_holding(pool, holding_id.as_uuid())
            .map_err(|_| PortError::Unavailable)?;
        Ok(Self { pool, holding_id })
    }
}
#[async_trait]
impl ManufacturingReadRepository for SqlxManufacturingReadRepository {
    async fn get_specification(
        &self,
        authority: &AuthorizedManufacturingAccess,
        id: SpecificationId,
    ) -> Result<Option<ManufacturingSpecificationProjection>, PortError> {
        if !authority
            .actor
            .authority_scope
            .contains(&authority.target_scope)
            || authority.actor.holding_id != self.holding_id
        {
            return Err(PortError::Denied);
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let generation: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT generation FROM erpx_cqrs_read.projection_generations WHERE holding_id=$1 AND projection_name=$2 AND status='active'",
        )
        .bind(self.holding_id.as_uuid())
        .bind(PROJECTION_NAME)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_error)?;
        let generation = generation.ok_or(PortError::Unavailable)?;
        let row = sqlx::query(
            "SELECT p.authority_scope,p.projection,p.source_position,p.projected_at,
                    c.global_sequence AS checkpoint_position,
                    EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_failures f
                      WHERE f.holding_id=p.holding_id AND f.projection_name=p.projection_name
                        AND f.generation=p.generation AND f.partition_key=p.partition_key) AS has_failure
             FROM erpx_manufacturing_read.specification_projection p
             LEFT JOIN erpx_cqrs_read.projection_checkpoints c
               ON c.holding_id=p.holding_id AND c.projection_name=p.projection_name
              AND c.generation=p.generation AND c.partition_key=p.partition_key
             WHERE p.holding_id=$1 AND p.projection_name=$2 AND p.generation=$3 AND p.specification_id=$4",
        )
        .bind(self.holding_id.as_uuid())
        .bind(PROJECTION_NAME)
        .bind(generation)
        .bind(id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_error)?;
        tx.commit().await.map_err(map_error)?;
        let Some(row) = row else { return Ok(None) };
        let (source_position, checkpoint_position) = projection_positions(&row)?;
        let scope: erpx_kernel::AuthorityScope = serde_json::from_value(
            row.try_get("authority_scope")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        if scope != authority.target_scope {
            return Err(PortError::Internal);
        }
        Ok(Some(ManufacturingSpecificationProjection {
            specification: serde_json::from_value(
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
    async fn get_order(
        &self,
        authority: &AuthorizedManufacturingAccess,
        id: ProductionOrderId,
    ) -> Result<Option<ManufacturingOrderProjection>, PortError> {
        if !authority
            .actor
            .authority_scope
            .contains(&authority.target_scope)
        {
            return Err(PortError::Denied);
        }
        if authority.actor.holding_id != self.holding_id {
            return Err(PortError::Denied);
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let generation: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT generation FROM erpx_cqrs_read.projection_generations WHERE holding_id=$1 AND projection_name='manufacturing.orders' AND status='active'"
        ).bind(self.holding_id.as_uuid()).fetch_optional(&mut *tx).await.map_err(map_error)?;
        let generation = generation.ok_or(PortError::Unavailable)?;
        let row = sqlx::query("SELECT p.authority_scope,p.projection,p.generation,p.partition_key,p.source_position,p.projected_at,c.global_sequence AS checkpoint_position,EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_failures f WHERE f.holding_id=p.holding_id AND f.projection_name=p.projection_name AND f.generation=p.generation AND f.partition_key=p.partition_key) AS has_failure FROM erpx_manufacturing_read.order_projection p LEFT JOIN erpx_cqrs_read.projection_checkpoints c ON c.holding_id=p.holding_id AND c.projection_name=p.projection_name AND c.generation=p.generation AND c.partition_key=p.partition_key WHERE p.holding_id=$1 AND p.projection_name='manufacturing.orders' AND p.generation=$2 AND p.order_id=$3")
            .bind(self.holding_id.as_uuid()).bind(generation).bind(id.as_uuid()).fetch_optional(&mut *tx).await.map_err(map_error)?;
        tx.commit().await.map_err(map_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
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
        if scope != authority.target_scope {
            return Err(PortError::Internal);
        }
        Ok(Some(ManufacturingOrderProjection {
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
        authority: &AuthorizedManufacturingAccess,
        id: uuid::Uuid,
    ) -> Result<Option<erpx_manufacturing::ManufacturingRecordProjection>, PortError> {
        if !authority
            .actor
            .authority_scope
            .contains(&authority.target_scope)
        {
            return Err(PortError::Denied);
        }
        if authority.actor.holding_id != self.holding_id {
            return Err(PortError::Denied);
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let generation: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT generation FROM erpx_cqrs_read.projection_generations WHERE holding_id=$1 AND projection_name=$2 AND status='active'",
        )
        .bind(self.holding_id.as_uuid())
        .bind(PROJECTION_NAME)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_error)?;
        let generation = generation.ok_or(PortError::Unavailable)?;
        let row = sqlx::query(
            "SELECT p.authority_scope,p.projection,p.source_position,p.projected_at,
                    c.global_sequence AS checkpoint_position,
                    EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_failures f
                      WHERE f.holding_id=p.holding_id AND f.projection_name=p.projection_name
                        AND f.generation=p.generation AND f.partition_key=p.partition_key) AS has_failure
             FROM erpx_manufacturing_read.governed_record_projection p
             LEFT JOIN erpx_cqrs_read.projection_checkpoints c
               ON c.holding_id=p.holding_id AND c.projection_name=p.projection_name
              AND c.generation=p.generation AND c.partition_key=p.partition_key
             WHERE p.holding_id=$1 AND p.projection_name=$2 AND p.generation=$3 AND p.record_id=$4",
        )
        .bind(self.holding_id.as_uuid())
        .bind(PROJECTION_NAME)
        .bind(generation)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_error)?;
        tx.commit().await.map_err(map_error)?;
        let Some(row) = row else { return Ok(None) };
        let (source_position, checkpoint_position) = projection_positions(&row)?;
        let scope: erpx_kernel::AuthorityScope = serde_json::from_value(
            row.try_get("authority_scope")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        if scope != authority.target_scope {
            return Err(PortError::Internal);
        }
        Ok(Some(erpx_manufacturing::ManufacturingRecordProjection {
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

fn projection_positions(row: &sqlx::postgres::PgRow) -> Result<(u64, u64), PortError> {
    let source = u64::try_from(
        row.try_get::<i64, _>("source_position")
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    let checkpoint = row
        .try_get::<Option<i64>, _>("checkpoint_position")
        .map_err(|_| PortError::Internal)?
        .and_then(|value| u64::try_from(value).ok())
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

async fn set_holding(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: uuid::Uuid,
) -> Result<(), PortError> {
    sqlx::query("SELECT set_config('app.holding_id', $1, true)")
        .bind(holding_id.to_string())
        .execute(&mut **tx)
        .await
        .map_err(map_error)?;
    Ok(())
}
fn map_error(error: sqlx::Error) -> PortError {
    match &error {
        sqlx::Error::Database(database) if database.code().as_deref() == Some("23505") => {
            PortError::Conflict
        }
        _ => PortError::Internal,
    }
}

#[cfg(test)]
#[path = "../../schema_test.rs"]
mod schema_test;
#[cfg(test)]
mod tests {
    use crate::schema_test;
    const WRITE_DDL: &str =
        include_str!("../../../migration/migrations_holding_write/0001_baseline.sql");
    const READ_DDL: &str =
        include_str!("../../../migration/migrations_holding_read/0001_baseline.sql");
    const RECORD_READ_DDL: &str =
        include_str!("../../../migration/migrations_holding_read/0001_baseline.sql");
    #[test]
    fn schema_preserves_material_genealogy_wip_and_rls() {
        assert!(WRITE_DDL.contains("material_fact_guards"));
        assert!(WRITE_DDL.contains("genealogy_guards"));
        assert!(READ_DDL.contains("wip_cost_projection"));
        assert!(WRITE_DDL.contains("FORCE ROW LEVEL SECURITY"));
    }
    #[test]
    fn read_adapter_never_reads_write_authority() {
        let source = include_str!("lib.rs");
        let read = source
            .split("pub struct SqlxManufacturingReadRepository")
            .nth(1)
            .unwrap();
        assert!(!read.contains("erpx_manufacturing_write.aggregate_versions"));
        assert!(read.contains("governed_record_projection"));
        assert!(read.contains("status='active'"));
    }
    #[test]
    fn write_authority_is_normalized_and_atomic() {
        let source = include_str!("lib.rs");
        let owned = schema_test::select_schema_statements(WRITE_DDL, &["erpx_manufacturing_write"]);
        assert!(!owned.contains("aggregate_snapshots"));
        assert!(!owned.contains("snapshot jsonb"));
        for marker in [
            "specifications",
            "production_orders",
            "persist_normalized",
            "audit_log",
            "command_receipts",
            "outbox",
        ] {
            assert!(WRITE_DDL.contains(marker));
        }
        assert!(source.contains("set_config('app.holding_id', $1, true)"));
        for marker in [
            "command_id != request.identity.command_id",
            "digest != request.identity.request_digest",
            "FOR UPDATE",
        ] {
            assert!(source.contains(marker));
        }
        assert!(READ_DDL.contains("projection_name text NOT NULL DEFAULT 'manufacturing.orders'"));
        assert!(READ_DDL.contains("source_aggregate_version"));
        assert!(source.contains("ReadPool::for_holding"));
        assert!(source.contains("status='active'"));
        let read = source
            .split("pub struct SqlxManufacturingReadRepository")
            .nth(1)
            .unwrap()
            .split("async fn set_holding")
            .next()
            .unwrap();
        assert!(!read.contains(&["ORDER BY projected_at DESC", " LIMIT 1"].concat()));
    }
    #[test]
    fn every_manufacturing_planning_table_is_forced_under_rls() {
        let write = schema_test::select_schema_statements(
            WRITE_DDL,
            &["erpx_manufacturing_write", "erpx_planning_write"],
        );
        let read = schema_test::select_schema_statements(
            READ_DDL,
            &["erpx_manufacturing_read", "erpx_planning_read"],
        );
        assert_eq!(write.matches("CREATE TABLE").count(), 20);
        assert_eq!(read.matches("CREATE TABLE").count(), 10);
        for ddl in [WRITE_DDL, READ_DDL] {
            assert!(ddl.contains("ENABLE ROW LEVEL SECURITY"));
            assert!(ddl.contains("FORCE ROW LEVEL SECURITY"));
            assert!(ddl.contains("current_setting(''app.holding_id'', true)"));
        }
        for marker in [
            "specification_projection",
            "projection_generations",
            "ENABLE ROW LEVEL SECURITY",
            "FORCE ROW LEVEL SECURITY",
            "WITH CHECK",
        ] {
            assert!(RECORD_READ_DDL.contains(marker));
        }
        assert!(include_str!("projection.rs").contains("SPECIFICATION_SNAPSHOT_EVENT_TYPE"));
    }
}
