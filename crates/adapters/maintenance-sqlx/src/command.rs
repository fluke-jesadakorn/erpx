use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::{IntegrationEvent, IntegrationEventEnvelope, assets};
use erpx_kernel::{AggregateId, CommandId, EventId, TenantScope};
use erpx_maintenance::{
    AssetFactPort, CommandFingerprint, CommandIdentity, CommandUnitOfWork, MaintenanceCommit,
    MaintenanceStoredCommit, PortError, WorkOrder, WorkOrderId, WriteRepository,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, EVENT_VERSION, SNAPSHOT_EVENT_TYPE,
    projection::{
        MaintenanceOutboxPayload, MaintenanceSnapshot, condition_name, scope_key, status_name,
    },
    work_order_partition,
};

#[derive(Debug, Clone)]
pub struct MaintenanceWriteRepository {
    store: CommandStore,
}

impl MaintenanceWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl WriteRepository for MaintenanceWriteRepository {
    async fn load(
        &self,
        authority_scope: &TenantScope,
        work_order_id: WorkOrderId,
    ) -> Result<Option<WorkOrder>, PortError> {
        let snapshot = self
            .store
            .load_snapshot::<WorkOrder>(
                CONTEXT_NAME,
                &work_order_partition(work_order_id),
                AGGREGATE_TYPE,
                aggregate_id(work_order_id)?,
            )
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        if snapshot.version != snapshot.state.version()
            || snapshot.state.id() != work_order_id
            || snapshot.state.validate().is_err()
        {
            return Err(PortError::Internal);
        }
        if !authority_scope.contains(snapshot.state.scope()) {
            return Err(PortError::Denied);
        }
        Ok(Some(snapshot.state))
    }
}

/// Reads the local typed Assets fact inbox. It does not query Assets snapshots
/// or import the Assets core crate.
#[derive(Debug, Clone)]
pub struct MaintenanceAssetFactRepository {
    pool: PgPool,
}

impl MaintenanceAssetFactRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AssetFactPort for MaintenanceAssetFactRepository {
    async fn get_at_least(
        &self,
        asset: assets::AssetRef,
        source_aggregate_version: u64,
    ) -> Result<assets::AssetConditionFact, PortError> {
        let row = sqlx::query(
            "SELECT asset_code,asset_name,location_id,asset_condition
             FROM erpx_maintenance_write.asset_fact_inbox
             WHERE asset_id=$1 AND source_aggregate_version=$2",
        )
        .bind(asset.as_uuid())
        .bind(to_i64_port(source_aggregate_version)?)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| PortError::Unavailable)?
        .ok_or(PortError::AssetFactNotReady)?;
        decode_asset_fact(asset, &row)
    }
}

/// Idempotent inbox writer invoked by the Worker after consuming an Assets
/// condition event. Source aggregate version is monotonic and event IDs are
/// unique under the frozen DDL.
#[derive(Debug, Clone)]
pub struct MaintenanceAssetFactInbox {
    pool: PgPool,
}

impl MaintenanceAssetFactInbox {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn ingest(
        &self,
        envelope: &IntegrationEventEnvelope<assets::Event>,
    ) -> Result<bool, PortError> {
        envelope.validate().map_err(|_| PortError::Internal)?;
        let assets::Event::AssetConditionChanged(fact) = envelope.payload() else {
            return Ok(false);
        };
        if fact.asset.as_uuid() != envelope.aggregate_id().as_uuid() {
            return Err(PortError::Internal);
        }
        let result = sqlx::query(
            "INSERT INTO erpx_maintenance_write.asset_fact_inbox
                (asset_id,source_aggregate_version,source_event_id,tenant_scope,
                 asset_code,asset_name,location_id,asset_condition,occurred_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
             ON CONFLICT (asset_id,source_aggregate_version) DO NOTHING",
        )
        .bind(fact.asset.as_uuid())
        .bind(to_i64_port(envelope.aggregate_version())?)
        .bind(envelope.event_id().as_uuid())
        .bind(serde_json::to_value(envelope.tenant_scope()).map_err(|_| PortError::Internal)?)
        .bind(fact.asset_code.as_str())
        .bind(fact.name.as_str())
        .bind(fact.location.as_uuid())
        .bind(condition_name(fact.condition))
        .bind(*envelope.occurred_at())
        .execute(&self.pool)
        .await
        .map_err(|error| {
            if error
                .as_database_error()
                .and_then(|value| value.constraint())
                == Some("asset_fact_inbox_source_event_id_key")
            {
                PortError::IdempotencyConflict
            } else {
                PortError::Unavailable
            }
        })?;
        if result.rows_affected() == 1 {
            return Ok(true);
        }

        let row = sqlx::query(
            "SELECT source_event_id,tenant_scope,asset_code,asset_name,location_id,
                    asset_condition,occurred_at
             FROM erpx_maintenance_write.asset_fact_inbox
             WHERE asset_id=$1 AND source_aggregate_version=$2",
        )
        .bind(fact.asset.as_uuid())
        .bind(to_i64_port(envelope.aggregate_version())?)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| PortError::Unavailable)?
        .ok_or(PortError::Internal)?;
        let source_event_id: Uuid = row
            .try_get("source_event_id")
            .map_err(|_| PortError::Internal)?;
        let tenant_scope: serde_json::Value = row
            .try_get("tenant_scope")
            .map_err(|_| PortError::Internal)?;
        let asset_code: String = row.try_get("asset_code").map_err(|_| PortError::Internal)?;
        let asset_name: String = row.try_get("asset_name").map_err(|_| PortError::Internal)?;
        let location_id: Uuid = row
            .try_get("location_id")
            .map_err(|_| PortError::Internal)?;
        let condition: String = row
            .try_get("asset_condition")
            .map_err(|_| PortError::Internal)?;
        let occurred_at: chrono::DateTime<chrono::Utc> = row
            .try_get("occurred_at")
            .map_err(|_| PortError::Internal)?;
        let exact_replay = source_event_id == envelope.event_id().as_uuid()
            && tenant_scope
                == serde_json::to_value(envelope.tenant_scope())
                    .map_err(|_| PortError::Internal)?
            && asset_code == fact.asset_code.as_str()
            && asset_name == fact.name.as_str()
            && location_id == fact.location.as_uuid()
            && condition == condition_name(fact.condition)
            && occurred_at == *envelope.occurred_at();
        if exact_replay {
            Ok(false)
        } else {
            Err(PortError::IdempotencyConflict)
        }
    }
}

#[derive(Debug, Clone)]
pub struct MaintenanceCommandAdapter {
    store: CommandStore,
}

impl MaintenanceCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl CommandUnitOfWork for MaintenanceCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<MaintenanceStoredCommit>, PortError> {
        let id = fingerprint_work_order_id(&identity.fingerprint);
        let receipt = self
            .store
            .find_receipt::<WorkOrder>(
                CONTEXT_NAME,
                &work_order_partition(id),
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?;
        receipt.map(receipt_to_stored).transpose()
    }

    async fn commit(
        &self,
        request: MaintenanceCommit,
    ) -> Result<MaintenanceStoredCommit, PortError> {
        validate_commit(&request)?;
        let id = request.aggregate.id();
        let partition = work_order_partition(id);
        let occurred_at = request.aggregate.updated_at();
        let command_id = request.identity.command_id;
        let mut events = request
            .integration_events
            .iter()
            .enumerate()
            .map(|(index, event)| OutboxEventWrite {
                event_id: deterministic_event_id(command_id, event.event_type(), index),
                event_type: event.event_type().into(),
                event_version: u32::from(event.schema_version()),
                partition: partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: MaintenanceOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, SNAPSHOT_EVENT_TYPE, events.len()),
            event_type: SNAPSHOT_EVENT_TYPE.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: MaintenanceOutboxPayload::Snapshot(MaintenanceSnapshot {
                work_order: (&request.aggregate).into(),
            }),
        });
        let guard = WorkOrderAtomicGuard {
            aggregate: request.aggregate.clone(),
            consumed_source_event: request.consumed_source_event,
        };
        let aggregate = request.aggregate;
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition,
            command_id,
            correlation_id: request.correlation_id,
            idempotency_key: request.identity.idempotency_key,
            request_hash: request_hash(&request.identity.fingerprint)?,
            tenant_scope: aggregate.scope().clone(),
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: AGGREGATE_TYPE.into(),
            aggregate_id: aggregate_id(id)?,
            expected_version: request.expected_version,
            aggregate_state: serde_json::to_value(&aggregate).map_err(|_| PortError::Internal)?,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"maintenance-audit-v1"),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({
                    "work_order_id": id.as_uuid(),
                    "expected_version": request.expected_version,
                    "source_event_id": guard.consumed_source_event.map(EventId::as_uuid),
                }),
            },
            events,
        };
        let outcome = self
            .store
            .commit_guarded(command, &guard)
            .await
            .map_err(map_store_error)?;
        let replayed = matches!(outcome, AtomicCommandOutcome::Replayed(_));
        let receipt = match outcome {
            AtomicCommandOutcome::Committed(receipt) | AtomicCommandOutcome::Replayed(receipt) => {
                receipt
            }
        };
        let mut stored = receipt_to_stored(receipt)?;
        stored.replayed = replayed;
        Ok(stored)
    }
}

struct WorkOrderAtomicGuard {
    aggregate: WorkOrder,
    consumed_source_event: Option<EventId>,
}

#[async_trait]
impl AtomicCommandGuard for WorkOrderAtomicGuard {
    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        let current: Option<Uuid> = sqlx::query_scalar(
            "SELECT source_event_id FROM erpx_maintenance_write.work_order_guards
             WHERE work_order_id=$1 FOR UPDATE",
        )
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        match (context.expected_version, current) {
            (None, Some(_)) => {
                return Err(CqrsSqlxError::GuardRejected("work order already exists"));
            }
            (Some(_), None) => {
                return Err(CqrsSqlxError::GuardRejected("work order guard is missing"));
            }
            _ => {}
        }
        match (context.expected_version, self.consumed_source_event) {
            (None, Some(event)) if event == self.aggregate.request().source_event_id => {}
            (Some(_), None) => {}
            _ => {
                return Err(CqrsSqlxError::GuardRejected(
                    "consumed source-event guard mismatch",
                ));
            }
        }
        let fact = sqlx::query(
            "SELECT asset_code,asset_name,location_id,asset_condition,tenant_scope
             FROM erpx_maintenance_write.asset_fact_inbox
             WHERE asset_id=$1 AND source_aggregate_version=$2 FOR SHARE",
        )
        .bind(self.aggregate.asset_fact().asset.as_uuid())
        .bind(to_i64(self.aggregate.request().source_aggregate_version)?)
        .fetch_optional(&mut **transaction)
        .await?
        .ok_or(CqrsSqlxError::GuardRejected("Assets fact is not ready"))?;
        let stored = decode_asset_fact_store(self.aggregate.asset_fact().asset, &fact)?;
        let tenant_scope: TenantScope = serde_json::from_value(fact.try_get("tenant_scope")?)?;
        if stored != *self.aggregate.asset_fact() || tenant_scope != *self.aggregate.scope() {
            return Err(CqrsSqlxError::GuardRejected(
                "Assets fact changed or belongs to another scope",
            ));
        }
        Ok(())
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if context.context != CONTEXT_NAME
            || context.aggregate_type != AGGREGATE_TYPE
            || context.aggregate_id.as_uuid() != self.aggregate.id().as_uuid()
            || context.partition != work_order_partition(self.aggregate.id())
            || context.aggregate_version != self.aggregate.version()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "Maintenance applied guard context mismatch",
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_maintenance_write.work_order_guards
                (work_order_id,scope_key,work_order_code,asset_id,request_code,
                 source_event_id,source_aggregate_version,work_order_status,
                 aggregate_version,updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
             ON CONFLICT (work_order_id) DO UPDATE SET
                scope_key=EXCLUDED.scope_key,work_order_code=EXCLUDED.work_order_code,
                asset_id=EXCLUDED.asset_id,request_code=EXCLUDED.request_code,
                source_event_id=EXCLUDED.source_event_id,
                source_aggregate_version=EXCLUDED.source_aggregate_version,
                work_order_status=EXCLUDED.work_order_status,
                aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at",
        )
        .bind(self.aggregate.id().as_uuid())
        .bind(guard_scope_key(self.aggregate.scope())?)
        .bind(self.aggregate.code())
        .bind(self.aggregate.asset_fact().asset.as_uuid())
        .bind(&self.aggregate.request().request_code)
        .bind(self.aggregate.request().source_event_id.as_uuid())
        .bind(to_i64(self.aggregate.request().source_aggregate_version)?)
        .bind(status_name(self.aggregate.status()))
        .bind(to_i64(self.aggregate.version())?)
        .bind(self.aggregate.updated_at())
        .execute(&mut **transaction)
        .await?;
        sqlx::query("DELETE FROM erpx_maintenance_write.downtime_guards WHERE work_order_id=$1")
            .bind(self.aggregate.id().as_uuid())
            .execute(&mut **transaction)
            .await?;
        for downtime in self.aggregate.downtime() {
            sqlx::query(
                "INSERT INTO erpx_maintenance_write.downtime_guards
                    (downtime_id,work_order_id,started_at,ended_at,reason,aggregate_version)
                 VALUES ($1,$2,$3,$4,$5,$6)",
            )
            .bind(downtime.downtime_id)
            .bind(self.aggregate.id().as_uuid())
            .bind(downtime.started_at)
            .bind(downtime.ended_at)
            .bind(&downtime.reason)
            .bind(to_i64(self.aggregate.version())?)
            .execute(&mut **transaction)
            .await?;
        }
        Ok(())
    }
}

fn validate_guard_context(
    aggregate: &WorkOrder,
    context: &GuardCommandContext<'_>,
) -> Result<(), CqrsSqlxError> {
    if context.context != CONTEXT_NAME
        || context.aggregate_type != AGGREGATE_TYPE
        || context.aggregate_id.as_uuid() != aggregate.id().as_uuid()
        || context.partition != work_order_partition(aggregate.id())
    {
        return Err(CqrsSqlxError::GuardRejected(
            "Maintenance command guard context mismatch",
        ));
    }
    Ok(())
}

fn validate_commit(request: &MaintenanceCommit) -> Result<(), PortError> {
    if request.aggregate.validate().is_err()
        || fingerprint_work_order_id(&request.identity.fingerprint) != request.aggregate.id()
        || !request.authority_scope.contains(request.aggregate.scope())
    {
        return Err(PortError::Internal);
    }
    Ok(())
}

fn guard_scope_key(scope: &TenantScope) -> Result<String, CqrsSqlxError> {
    scope_key(scope).map_err(|_| CqrsSqlxError::GuardRejected("Maintenance scope is invalid"))
}

fn decode_asset_fact(
    asset: assets::AssetRef,
    row: &sqlx::postgres::PgRow,
) -> Result<assets::AssetConditionFact, PortError> {
    let code: String = row.try_get("asset_code").map_err(|_| PortError::Internal)?;
    let name: String = row.try_get("asset_name").map_err(|_| PortError::Internal)?;
    let location: Uuid = row
        .try_get("location_id")
        .map_err(|_| PortError::Internal)?;
    let condition: String = row
        .try_get("asset_condition")
        .map_err(|_| PortError::Internal)?;
    Ok(assets::AssetConditionFact {
        asset,
        asset_code: erpx_integration_contracts::ReferenceCode::try_new(code)
            .map_err(|_| PortError::Internal)?,
        name: erpx_integration_contracts::DisplayName::try_new(name)
            .map_err(|_| PortError::Internal)?,
        location: assets::AssetLocationRef::try_from_uuid(location)
            .map_err(|_| PortError::Internal)?,
        condition: parse_condition(&condition).ok_or(PortError::Internal)?,
    })
}

fn decode_asset_fact_store(
    asset: assets::AssetRef,
    row: &sqlx::postgres::PgRow,
) -> Result<assets::AssetConditionFact, CqrsSqlxError> {
    decode_asset_fact(asset, row)
        .map_err(|_| CqrsSqlxError::GuardRejected("stored Assets fact is invalid"))
}

fn parse_condition(value: &str) -> Option<assets::AssetCondition> {
    match value {
        "operational" => Some(assets::AssetCondition::Operational),
        "degraded" => Some(assets::AssetCondition::Degraded),
        "out_of_service" => Some(assets::AssetCondition::OutOfService),
        "retired" => Some(assets::AssetCondition::Retired),
        _ => None,
    }
}

fn aggregate_id(id: WorkOrderId) -> Result<AggregateId, PortError> {
    AggregateId::try_from_uuid(id.as_uuid()).map_err(|_| PortError::Internal)
}

fn fingerprint_work_order_id(fingerprint: &CommandFingerprint) -> WorkOrderId {
    match fingerprint {
        CommandFingerprint::Open { work_order_id, .. }
        | CommandFingerprint::Schedule { work_order_id, .. }
        | CommandFingerprint::Start { work_order_id }
        | CommandFingerprint::Block { work_order_id }
        | CommandFingerprint::Resume { work_order_id }
        | CommandFingerprint::Complete { work_order_id }
        | CommandFingerprint::Cancel { work_order_id }
        | CommandFingerprint::RecordDowntime { work_order_id, .. } => *work_order_id,
    }
}

fn request_hash(fingerprint: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let bytes = serde_json::to_vec(fingerprint).map_err(|_| PortError::Internal)?;
    let mut digest = Sha256::new();
    digest.update(b"erpx.maintenance.command-fingerprint.v1\0");
    digest.update(bytes);
    Ok(RequestHash::new(digest.finalize().into()))
}

fn audit_action(fingerprint: &CommandFingerprint) -> &'static str {
    match fingerprint {
        CommandFingerprint::Open { .. } => "maintenance.work_order.open",
        CommandFingerprint::Schedule { .. } => "maintenance.work_order.schedule",
        CommandFingerprint::Start { .. } => "maintenance.work_order.start",
        CommandFingerprint::Block { .. } => "maintenance.work_order.block",
        CommandFingerprint::Resume { .. } => "maintenance.work_order.resume",
        CommandFingerprint::Complete { .. } => "maintenance.work_order.complete",
        CommandFingerprint::Cancel { .. } => "maintenance.work_order.cancel",
        CommandFingerprint::RecordDowntime { .. } => "maintenance.work_order.record_downtime",
    }
}

fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<WorkOrder>,
) -> Result<MaintenanceStoredCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != AGGREGATE_TYPE
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.id().as_uuid()
        || receipt.aggregate.version != receipt.response.version()
        || receipt.response.validate().is_err()
    {
        return Err(PortError::Internal);
    }
    Ok(MaintenanceStoredCommit {
        command_id: receipt.command_id,
        correlation_id: receipt.correlation_id,
        aggregate: receipt.response,
        committed_events: receipt.committed_events,
        committed_at: receipt.committed_at,
        replayed: receipt.replayed,
    })
}

fn deterministic_event_id(command_id: CommandId, event_type: &str, index: usize) -> EventId {
    let mut label = event_type.as_bytes().to_vec();
    label.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(command_id.as_uuid(), &label))
        .expect("a digest-derived event ID is non-nil")
}

fn deterministic_uuid(seed: Uuid, label: &[u8]) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(seed.as_bytes());
    digest.update(label);
    let digest = digest.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn to_i64(value: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(value)
        .map_err(|_| CqrsSqlxError::GuardRejected("Maintenance version is too large"))
}

fn to_i64_port(value: u64) -> Result<i64, PortError> {
    i64::try_from(value).map_err(|_| PortError::Internal)
}

fn map_store_error(error: CqrsSqlxError) -> PortError {
    match error {
        CqrsSqlxError::IdempotencyConflict | CqrsSqlxError::CommandReplayConflict => {
            PortError::IdempotencyConflict
        }
        CqrsSqlxError::OptimisticConcurrency {
            expected: Some(expected),
            actual,
            ..
        } => PortError::Conflict {
            expected,
            actual: actual.unwrap_or(0),
        },
        CqrsSqlxError::GuardRejected("Assets fact is not ready") => PortError::AssetFactNotReady,
        CqrsSqlxError::Database(error) => match error
            .as_database_error()
            .and_then(|value| value.constraint())
        {
            Some("work_order_guards_source_event_id_key") => PortError::SourceEventAlreadyConsumed,
            _ => PortError::Unavailable,
        },
        _ => PortError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn condition_parser_accepts_only_frozen_values() {
        assert_eq!(
            parse_condition("out_of_service"),
            Some(assets::AssetCondition::OutOfService)
        );
        assert_eq!(parse_condition("draft"), None);
    }

    #[test]
    fn fingerprint_hash_is_stable() {
        let id = WorkOrderId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let fingerprint = CommandFingerprint::Start { work_order_id: id };
        assert_eq!(
            request_hash(&fingerprint).unwrap(),
            request_hash(&fingerprint).unwrap()
        );
    }
}
