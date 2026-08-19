use async_trait::async_trait;
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionEvent, ProjectionFailure, Projector, ProjectorContext, ReadPool,
    ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig, WritePool,
};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, ItemDescriptorDraft,
    RecordTypeId, StableRecordRef, catalog::ContextId, record_policy_id,
};
use erpx_kernel::{AuthorityScope, DataClassification, RecordId};
use erpx_maintenance::{
    MaintenanceLifecycleRecord, MaintenanceLifecycleRecordCommandIdentity,
    MaintenanceLifecycleRecordCommit, MaintenanceLifecycleRecordReadRepository,
    MaintenanceLifecycleRecordUnitOfWork, MaintenanceLifecycleRecordWriteRepository, PortError,
    ReadProjection, StoredMaintenanceLifecycleRecordCommit,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{CONTEXT_NAME, EVENT_VERSION};

pub const RECORD_PROJECTION_NAME: &str = "maintenance.lifecycle-records";
pub const RECORD_SNAPSHOT_EVENT_TYPE: &str = "maintenance.lifecycle-record-snapshot";

#[derive(Debug, Clone)]
pub struct MaintenanceLifecycleRecordSqlxAdapter {
    pool: WritePool,
}

impl MaintenanceLifecycleRecordSqlxAdapter {
    pub const fn new(pool: WritePool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone)]
pub struct MaintenanceLifecycleRecordReadAdapter {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}

impl MaintenanceLifecycleRecordReadAdapter {
    pub fn new(pool: ReadPool) -> Result<Self, CqrsSqlxError> {
        Self::with_waiter_config(pool, WaiterConfig::default())
    }

    pub fn with_waiter_config(pool: ReadPool, config: WaiterConfig) -> Result<Self, CqrsSqlxError> {
        Ok(Self {
            read_side: ReadSideStore::new(pool.clone()),
            waiter: SqlxProjectionWaiter::new(pool.clone(), config)?,
            pool,
        })
    }
}

#[derive(Debug, Clone)]
pub struct MaintenanceLifecycleRecordProjector {
    event_types: Vec<String>,
}

impl MaintenanceLifecycleRecordProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![RECORD_SNAPSHOT_EVENT_TYPE.into()],
        }
    }
}

impl Default for MaintenanceLifecycleRecordProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MaintenanceLifecycleRecordWriteRepository for MaintenanceLifecycleRecordSqlxAdapter {
    async fn load_maintenance_lifecycle_record(
        &self,
        authority_scope: &AuthorityScope,
        record_id: Uuid,
    ) -> Result<Option<MaintenanceLifecycleRecord>, PortError> {
        authority_scope.validate().map_err(|_| PortError::Denied)?;
        let mut tx = self.pool.begin().await.map_err(map_database)?;
        let value = load_record(&mut tx, record_id, false).await?;
        tx.commit().await.map_err(map_database)?;
        if value
            .as_ref()
            .is_some_and(|record| !authority_scope.contains(record.scope()))
        {
            return Err(PortError::Denied);
        }
        Ok(value)
    }
}

#[async_trait]
impl MaintenanceLifecycleRecordUnitOfWork for MaintenanceLifecycleRecordSqlxAdapter {
    async fn find_maintenance_lifecycle_record_receipt(
        &self,
        identity: &MaintenanceLifecycleRecordCommandIdentity,
    ) -> Result<Option<StoredMaintenanceLifecycleRecordCommit>, PortError> {
        let mut tx = self.pool.begin().await.map_err(map_database)?;
        let value = find_receipt(&mut tx, identity).await?;
        tx.commit().await.map_err(map_database)?;
        Ok(value)
    }

    async fn commit_maintenance_lifecycle_record(
        &self,
        request: MaintenanceLifecycleRecordCommit,
    ) -> Result<StoredMaintenanceLifecycleRecordCommit, PortError> {
        validate_commit(&request)?;
        let mut tx = self.pool.begin().await.map_err(map_database)?;
        if let Some(receipt) = find_receipt(&mut tx, &request.request.identity).await? {
            tx.commit().await.map_err(map_database)?;
            return Ok(receipt);
        }
        let current = load_record(&mut tx, request.record.id(), true).await?;
        match (request.request.expected_version, current.as_ref()) {
            (None, None) if request.record.version() == 1 => {}
            (Some(expected), Some(current))
                if current.kind() == request.record.kind()
                    && current.version() == expected
                    && request.record.version() == expected.saturating_add(1) => {}
            (Some(expected), Some(current)) => {
                return Err(PortError::Conflict {
                    expected,
                    actual: current.version(),
                });
            }
            (Some(expected), None) => {
                return Err(PortError::Conflict {
                    expected,
                    actual: 0,
                });
            }
            (None, Some(current)) => {
                return Err(PortError::Conflict {
                    expected: 0,
                    actual: current.version(),
                });
            }
            (None, None) => return Err(PortError::Internal),
        }
        persist_record(&mut tx, &request.record).await?;
        append_snapshot(&mut tx, &request).await?;
        append_directory(&mut tx, &request).await?;
        persist_audit(&mut tx, &request).await?;
        persist_receipt(&mut tx, &request).await?;
        tx.commit().await.map_err(map_database)?;
        Ok(StoredMaintenanceLifecycleRecordCommit {
            command_id: request.request.identity.command_id,
            correlation_id: request.request.correlation_id,
            record: request.record,
            committed_at: request.changed_at,
            replayed: false,
        })
    }
}

#[async_trait]
impl Projector for MaintenanceLifecycleRecordProjector {
    fn name(&self) -> &str {
        RECORD_PROJECTION_NAME
    }
    fn event_types(&self) -> &[String] {
        &self.event_types
    }

    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let record = validate_event(context, event)?;
        sqlx::query(
            "INSERT INTO erpx_maintenance_read.business_record_projection
                (holding_id,projection_name,generation,partition_key,record_id,record_type,
                 source_aggregate_version,source_global_sequence,active_scope,record_title,
                 lifecycle_state,state,aggregate_updated_at,projected_at)
             VALUES (erpx_cqrs_read.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,now())
             ON CONFLICT (holding_id,projection_name,generation,partition_key,record_id)
             DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
               source_global_sequence=EXCLUDED.source_global_sequence,
               active_scope=EXCLUDED.active_scope,record_title=EXCLUDED.record_title,
               lifecycle_state=EXCLUDED.lifecycle_state,state=EXCLUDED.state,
               aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now()
             WHERE erpx_maintenance_read.business_record_projection.holding_id
                    =erpx_cqrs_read.current_holding_id()
               AND erpx_maintenance_read.business_record_projection.source_aggregate_version
                    <EXCLUDED.source_aggregate_version",
        )
        .bind(RECORD_PROJECTION_NAME).bind(context.generation).bind(&event.partition)
        .bind(record.id()).bind(record.kind().record_type()).bind(to_i64_projection(record.version())?)
        .bind(to_i64_projection(event.global_sequence)?).bind(serde_json::to_value(record.scope()).map_err(failure)?)
        .bind(record.title()).bind(record.status().as_str()).bind(serde_json::to_value(&record).map_err(failure)?)
        .bind(event.occurred_at).execute(&mut **tx).await.map_err(failure)?;
        Ok(())
    }

    async fn reset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query("DELETE FROM erpx_maintenance_read.business_record_projection WHERE holding_id=erpx_cqrs_read.current_holding_id() AND projection_name=$1 AND generation=$2")
            .bind(RECORD_PROJECTION_NAME).bind(context.generation)
            .execute(&mut **tx).await.map_err(failure)?;
        Ok(())
    }
}

#[async_trait]
impl MaintenanceLifecycleRecordReadRepository for MaintenanceLifecycleRecordReadAdapter {
    async fn get_maintenance_lifecycle_record(
        &self,
        authority_scope: &AuthorityScope,
        record_id: Uuid,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<MaintenanceLifecycleRecord>>, PortError> {
        authority_scope.validate().map_err(|_| PortError::Denied)?;
        let partition = partition(record_id);
        let checkpoint = self.consistency_checkpoint(consistency, &partition).await?;
        let mut tx = self.pool.begin().await.map_err(map_database)?;
        let row = sqlx::query(
            "SELECT item.state,generation.status
             FROM erpx_maintenance_read.business_record_projection item
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.holding_id=item.holding_id
              AND generation.projection_name=item.projection_name
              AND generation.generation=item.generation AND generation.status='active'
             WHERE item.holding_id=erpx_cqrs_read.current_holding_id()
               AND item.projection_name=$1 AND item.partition_key=$2 AND item.record_id=$3",
        )
        .bind(RECORD_PROJECTION_NAME)
        .bind(partition)
        .bind(record_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database)?;
        let generation_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_generations
             WHERE holding_id=erpx_cqrs_read.current_holding_id()
               AND projection_name=$1 AND status='active')",
        )
        .bind(RECORD_PROJECTION_NAME)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_database)?;
        tx.commit().await.map_err(map_database)?;
        let value = row
            .map(|row| decode_record(row.get("state")))
            .transpose()?
            .filter(|record| authority_scope.contains(record.scope()));
        Ok(ReadProjection {
            value,
            metadata: ProjectionMetadata {
                checkpoint,
                freshness: if generation_exists {
                    FreshnessState::Current
                } else {
                    FreshnessState::Unavailable
                },
                lag_millis: None,
            },
        })
    }
}

impl MaintenanceLifecycleRecordReadAdapter {
    async fn consistency_checkpoint(
        &self,
        consistency: &QueryConsistency,
        partition: &str,
    ) -> Result<Option<ProjectionCheckpoint>, PortError> {
        match consistency {
            QueryConsistency::Eventual => self
                .read_side
                .checkpoint(RECORD_PROJECTION_NAME, partition)
                .await
                .map_err(map_database),
            QueryConsistency::ReadYourWrites(requirement) => {
                if requirement.projection != RECORD_PROJECTION_NAME
                    || requirement.partition != partition
                    || !partition
                        .strip_prefix("maintenance.lifecycle-record:")
                        .is_some_and(valid_compact_uuid)
                {
                    return Err(PortError::NotReady(ProjectionNotReady {
                        projection: requirement.projection.clone(),
                        partition: requirement.partition.clone(),
                        target: requirement.target,
                        observed: None,
                    }));
                }
                self.waiter
                    .wait_until_checked(requirement)
                    .await
                    .map(Some)
                    .map_err(|error| match error {
                        WaitError::NotReady(value) => PortError::NotReady(value),
                        WaitError::Database(_) => PortError::Unavailable,
                    })
            }
        }
    }
}

async fn load_record(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    lock: bool,
) -> Result<Option<MaintenanceLifecycleRecord>, PortError> {
    let lock = if lock { " FOR UPDATE" } else { "" };
    let statement = format!(
        "SELECT record_version,record_kind,record_scope,record_title,record_status,created_at,updated_at
         FROM erpx_maintenance_write.business_records
         WHERE holding_id=erpx_cqrs_write.current_holding_id() AND record_id=$1{lock}"
    );
    sqlx::query(&statement)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(map_database)?
        .map(|row| {
            decode_record(json!({
                "id": id,
                "version": row.get::<i64,_>("record_version"),
                "kind": row.get::<String,_>("record_kind"),
                "scope": row.get::<serde_json::Value,_>("record_scope"),
                "title": row.get::<String,_>("record_title"),
                "status": row.get::<String,_>("record_status"),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),
                "updated_at": row.get::<chrono::DateTime<chrono::Utc>,_>("updated_at"),
            }))
        })
        .transpose()
}

async fn persist_record(
    tx: &mut Transaction<'_, Postgres>,
    record: &MaintenanceLifecycleRecord,
) -> Result<(), PortError> {
    let result = sqlx::query(
        "INSERT INTO erpx_maintenance_write.business_records
            (holding_id,record_id,record_version,record_kind,record_scope,record_title,record_status,created_at,updated_at)
         VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8)
         ON CONFLICT (holding_id,record_id) DO UPDATE SET record_version=EXCLUDED.record_version,
           record_title=EXCLUDED.record_title,record_status=EXCLUDED.record_status,updated_at=EXCLUDED.updated_at
         WHERE erpx_maintenance_write.business_records.holding_id=erpx_cqrs_write.current_holding_id()
           AND erpx_maintenance_write.business_records.record_kind=EXCLUDED.record_kind
           AND erpx_maintenance_write.business_records.record_version+1=EXCLUDED.record_version"
    ).bind(record.id()).bind(to_i64(record.version())?).bind(kind_name(record.kind()))
      .bind(serde_json::to_value(record.scope()).map_err(|_|PortError::Internal)?)
      .bind(record.title()).bind(record.status().as_str()).bind(record.created_at()).bind(record.updated_at())
      .execute(&mut **tx).await.map_err(map_database)?;
    if result.rows_affected() != 1 {
        return Err(PortError::Conflict {
            expected: record.version().saturating_sub(1),
            actual: record.version(),
        });
    }
    Ok(())
}

async fn append_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    request: &MaintenanceLifecycleRecordCommit,
) -> Result<(), PortError> {
    append_event(
        tx,
        request,
        RECORD_SNAPSHOT_EVENT_TYPE,
        EVENT_VERSION,
        serde_json::to_value(&request.record).map_err(|_| PortError::Internal)?,
        b"maintenance-lifecycle-record-snapshot-v1",
    )
    .await
}

async fn append_directory(
    tx: &mut Transaction<'_, Postgres>,
    request: &MaintenanceLifecycleRecordCommit,
) -> Result<(), PortError> {
    let record = StableRecordRef::try_new(
        RecordId::try_from_uuid(request.record.id()).map_err(|_| PortError::Internal)?,
        ContextId::Maintenance,
        RecordTypeId::try_new(request.record.kind().record_type())
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    let draft = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
        record,
        scope: request.record.scope().clone(),
        lifecycle_state: request.record.status().as_str().into(),
        classification: DataClassification::Confidential,
        descriptor_version: request.record.version(),
        authorized_search_text: request.record.title().into(),
    };
    draft.validate().map_err(|_| PortError::Internal)?;
    append_event(
        tx,
        request,
        ITEM_DIRECTORY_OWNER_EVENT_TYPE,
        ITEM_DIRECTORY_FACT_VERSION,
        serde_json::to_value(draft).map_err(|_| PortError::Internal)?,
        b"maintenance-lifecycle-record-directory-v1",
    )
    .await
}

async fn append_event(
    tx: &mut Transaction<'_, Postgres>,
    request: &MaintenanceLifecycleRecordCommit,
    event_type: &str,
    event_version: u32,
    payload: serde_json::Value,
    label: &[u8],
) -> Result<(), PortError> {
    sqlx::query("INSERT INTO erpx_cqrs_write.outbox_events (holding_id,event_id,context_name,partition_key,command_id,event_type,event_version,aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,causation_id,payload,occurred_at,available_at) VALUES (erpx_cqrs_write.current_holding_id(),$1,'maintenance',$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,NULL,$12,$13,now())")
      .bind(deterministic_uuid(request.request.identity.command_id.as_uuid(),label)).bind(partition(request.record.id()))
      .bind(request.request.identity.command_id.as_uuid()).bind(event_type).bind(i32::try_from(event_version).map_err(|_|PortError::Internal)?)
      .bind(request.record.kind().record_type()).bind(request.record.id()).bind(to_i64(request.record.version())?)
      .bind(serde_json::to_value(request.record.scope()).map_err(|_|PortError::Internal)?)
      .bind(serde_json::to_value(&request.request.actor).map_err(|_|PortError::Internal)?)
      .bind(request.request.correlation_id.as_uuid()).bind(payload).bind(request.changed_at)
      .execute(&mut **tx).await.map_err(map_database)?;
    Ok(())
}

async fn persist_audit(
    tx: &mut Transaction<'_, Postgres>,
    request: &MaintenanceLifecycleRecordCommit,
) -> Result<(), PortError> {
    sqlx::query("INSERT INTO erpx_maintenance_write.business_record_audit (holding_id,audit_id,command_id,correlation_id,record_id,record_version,actor,action,occurred_at) VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8)")
      .bind(deterministic_uuid(request.request.identity.command_id.as_uuid(),b"maintenance-lifecycle-record-audit-v1"))
      .bind(request.request.identity.command_id.as_uuid()).bind(request.request.correlation_id.as_uuid()).bind(request.record.id())
      .bind(to_i64(request.record.version())?).bind(serde_json::to_value(&request.request.actor).map_err(|_|PortError::Internal)?)
      .bind(match request.request.identity.command { erpx_maintenance::MaintenanceLifecycleRecordCommand::Create{..}=>"create", erpx_maintenance::MaintenanceLifecycleRecordCommand::Transition{..}=>"transition" })
      .bind(request.changed_at).execute(&mut **tx).await.map_err(map_database)?;
    Ok(())
}

async fn persist_receipt(
    tx: &mut Transaction<'_, Postgres>,
    request: &MaintenanceLifecycleRecordCommit,
) -> Result<(), PortError> {
    sqlx::query("INSERT INTO erpx_maintenance_write.business_record_receipts (holding_id,idempotency_key,command_id,request_hash,record_id,record_version,correlation_id,receipt,committed_at) VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8)")
      .bind(request.request.identity.idempotency_key.as_str()).bind(request.request.identity.command_id.as_uuid())
      .bind(hash(&request.request.identity)?.as_slice()).bind(request.record.id()).bind(to_i64(request.record.version())?)
      .bind(request.request.correlation_id.as_uuid()).bind(serde_json::to_value(&request.record).map_err(|_|PortError::Internal)?)
      .bind(request.changed_at).execute(&mut **tx).await.map_err(|error|if error.as_database_error().is_some_and(|e|e.is_unique_violation()){PortError::IdempotencyConflict}else{PortError::Unavailable})?;
    Ok(())
}

async fn find_receipt(
    tx: &mut Transaction<'_, Postgres>,
    identity: &MaintenanceLifecycleRecordCommandIdentity,
) -> Result<Option<StoredMaintenanceLifecycleRecordCommit>, PortError> {
    let row=sqlx::query("SELECT command_id,request_hash,record_id,record_version,correlation_id,receipt,committed_at FROM erpx_maintenance_write.business_record_receipts WHERE holding_id=erpx_cqrs_write.current_holding_id() AND idempotency_key=$1 FOR UPDATE")
      .bind(identity.idempotency_key.as_str()).fetch_optional(&mut **tx).await.map_err(map_database)?;
    let Some(row) = row else { return Ok(None) };
    if row.get::<Uuid, _>("command_id") != identity.command_id.as_uuid()
        || row.get::<Vec<u8>, _>("request_hash").as_slice() != hash(identity)?
        || row.get::<Uuid, _>("record_id") != identity.record_id
    {
        return Err(PortError::IdempotencyConflict);
    }
    let record = decode_record(row.get("receipt"))?;
    if record.version()
        != u64::try_from(row.get::<i64, _>("record_version")).map_err(|_| PortError::Internal)?
    {
        return Err(PortError::Internal);
    }
    Ok(Some(StoredMaintenanceLifecycleRecordCommit {
        command_id: identity.command_id,
        correlation_id: erpx_kernel::CorrelationId::try_from_uuid(row.get("correlation_id"))
            .map_err(|_| PortError::Internal)?,
        record,
        committed_at: row.get("committed_at"),
        replayed: true,
    }))
}

fn validate_commit(request: &MaintenanceLifecycleRecordCommit) -> Result<(), PortError> {
    request
        .record
        .validate_rehydrated()
        .map_err(|_| PortError::Internal)?;
    if request.record.id() != request.request.identity.record_id
        || !request
            .request
            .authority_scope
            .contains(request.record.scope())
    {
        Err(PortError::Denied)
    } else {
        Ok(())
    }
}
fn validate_event(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<MaintenanceLifecycleRecord, ProjectionFailure> {
    if context.projection != RECORD_PROJECTION_NAME
        || event.context != CONTEXT_NAME
        || event.event_type != RECORD_SNAPSHOT_EVENT_TYPE
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "Maintenance lifecycle record projector received an incompatible event",
        ));
    }
    let record: MaintenanceLifecycleRecord =
        serde_json::from_value(event.payload.clone()).map_err(failure)?;
    record.validate_rehydrated().map_err(failure)?;
    if record.id() != event.aggregate_id.as_uuid()
        || record.version() != event.aggregate_version
        || record.kind().record_type() != event.aggregate_type
        || record.scope() != &event.active_scope
        || event.partition != partition(record.id())
    {
        return Err(ProjectionFailure::new(
            "Maintenance lifecycle record snapshot envelope mismatch",
        ));
    }
    Ok(record)
}
fn decode_record(value: serde_json::Value) -> Result<MaintenanceLifecycleRecord, PortError> {
    let record: MaintenanceLifecycleRecord =
        serde_json::from_value(value).map_err(|_| PortError::Internal)?;
    record
        .validate_rehydrated()
        .map_err(|_| PortError::Internal)?;
    Ok(record)
}
fn kind_name(value: erpx_maintenance::MaintenanceLifecycleRecordKind) -> &'static str {
    match value {
        erpx_maintenance::MaintenanceLifecycleRecordKind::Plan => "plan",
        erpx_maintenance::MaintenanceLifecycleRecordKind::Inspection => "inspection",
        erpx_maintenance::MaintenanceLifecycleRecordKind::Request => "request",
    }
}
fn partition(id: Uuid) -> String {
    format!("maintenance.lifecycle-record:{}", id.simple())
}
fn valid_compact_uuid(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn hash(value: &MaintenanceLifecycleRecordCommandIdentity) -> Result<[u8; 32], PortError> {
    serde_json::to_vec(value)
        .map(|v| Sha256::digest(v).into())
        .map_err(|_| PortError::Internal)
}
fn deterministic_uuid(seed: Uuid, label: &[u8]) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(seed.as_bytes());
    digest.update(label);
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest.finalize()[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}
fn to_i64(value: u64) -> Result<i64, PortError> {
    i64::try_from(value).map_err(|_| PortError::Internal)
}
fn to_i64_projection(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value).map_err(|_| {
        ProjectionFailure::new("Maintenance lifecycle record numeric value is too large")
    })
}
fn failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}
fn map_database(_error: impl std::fmt::Display) -> PortError {
    PortError::Unavailable
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_cqrs_sqlx::Projector;
    #[test]
    fn record_adapter_is_normalized_holding_bound_and_generation_scoped() {
        let source = include_str!("records.rs");
        for table in [
            "business_records",
            "business_record_receipts",
            "business_record_audit",
            "business_record_projection",
        ] {
            assert!(source.contains(table));
        }
        assert!(source.contains("generation.status='active'"));
        assert!(source.contains("ITEM_DIRECTORY_OWNER_EVENT_TYPE"));
        assert!(!source.contains(&["aggregate", "snapshots"].join("_")));
        let projector = MaintenanceLifecycleRecordProjector::new();
        assert_eq!(projector.name(), RECORD_PROJECTION_NAME);
    }
}
