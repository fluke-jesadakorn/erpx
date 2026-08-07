use async_trait::async_trait;
use erpx_assets::{
    Asset, AssetCommit, AssetId, AssetStoredCommit, CommandFingerprint, CommandIdentity,
    CommandUnitOfWork, PortError, WriteRepository,
};
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::IntegrationEvent;
use erpx_kernel::{AggregateId, CommandId, EventId, TenantScope};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, EVENT_VERSION, SNAPSHOT_EVENT_TYPE, asset_partition,
    projection::{AssetOutboxPayload, AssetSnapshot, condition_name, scope_key, status_name},
};

#[derive(Debug, Clone)]
pub struct AssetsWriteRepository {
    store: CommandStore,
}

impl AssetsWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl WriteRepository for AssetsWriteRepository {
    async fn load(
        &self,
        authority_scope: &TenantScope,
        asset_id: AssetId,
    ) -> Result<Option<Asset>, PortError> {
        let snapshot = self
            .store
            .load_snapshot::<Asset>(
                CONTEXT_NAME,
                &asset_partition(asset_id),
                AGGREGATE_TYPE,
                aggregate_id(asset_id)?,
            )
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        if snapshot.version != snapshot.state.version()
            || snapshot.state.id() != asset_id
            || snapshot.state.validate().is_err()
        {
            return Err(PortError::Internal);
        }
        if !authority_scope.contains(snapshot.state.scope().tenant_scope()) {
            return Err(PortError::Denied);
        }
        Ok(Some(snapshot.state))
    }
}

#[derive(Debug, Clone)]
pub struct AssetsCommandAdapter {
    store: CommandStore,
}

impl AssetsCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl CommandUnitOfWork for AssetsCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<AssetStoredCommit>, PortError> {
        let asset_id = fingerprint_asset_id(&identity.fingerprint);
        let receipt = self
            .store
            .find_receipt::<Asset>(
                CONTEXT_NAME,
                &asset_partition(asset_id),
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?;
        receipt.map(receipt_to_stored).transpose()
    }

    async fn commit(&self, request: AssetCommit) -> Result<AssetStoredCommit, PortError> {
        validate_commit(&request)?;
        let asset_id = request.aggregate.id();
        let partition = asset_partition(asset_id);
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
                payload: AssetOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, SNAPSHOT_EVENT_TYPE, events.len()),
            event_type: SNAPSHOT_EVENT_TYPE.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: AssetOutboxPayload::Snapshot(AssetSnapshot {
                asset: (&request.aggregate).into(),
            }),
        });
        let guard = AssetAtomicGuard {
            aggregate: request.aggregate.clone(),
        };
        let aggregate = request.aggregate;
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition,
            command_id,
            correlation_id: request.correlation_id,
            idempotency_key: request.identity.idempotency_key,
            request_hash: request_hash(&request.identity.fingerprint)?,
            tenant_scope: aggregate.scope().tenant_scope().clone(),
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: AGGREGATE_TYPE.into(),
            aggregate_id: aggregate_id(asset_id)?,
            expected_version: request.expected_version,
            aggregate_state: serde_json::to_value(&aggregate).map_err(|_| PortError::Internal)?,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"assets-audit-v1"),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({
                    "asset_id": asset_id.as_uuid(),
                    "expected_version": request.expected_version,
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

struct AssetAtomicGuard {
    aggregate: Asset,
}

#[async_trait]
impl AtomicCommandGuard for AssetAtomicGuard {
    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT aggregate_version FROM erpx_assets_write.asset_guards
             WHERE asset_id=$1 FOR UPDATE",
        )
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        match (context.expected_version, current) {
            (None, Some(_)) => {
                return Err(CqrsSqlxError::GuardRejected("asset already exists"));
            }
            (Some(_), None) => {
                return Err(CqrsSqlxError::GuardRejected("asset guard is missing"));
            }
            _ => {}
        }
        let duplicate: Option<Uuid> = sqlx::query_scalar(
            "SELECT asset_id FROM erpx_assets_write.asset_guards
             WHERE scope_key=$1 AND asset_code=$2 AND asset_id<>$3
             ORDER BY asset_id LIMIT 1 FOR UPDATE",
        )
        .bind(guard_scope_key(self.aggregate.scope().tenant_scope())?)
        .bind(self.aggregate.code())
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        if duplicate.is_some() {
            return Err(CqrsSqlxError::GuardRejected(
                "asset code already exists in scope",
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
            || context.partition != asset_partition(self.aggregate.id())
            || context.aggregate_version != self.aggregate.version()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "assets applied guard context mismatch",
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_assets_write.asset_guards
                (asset_id,scope_key,asset_code,location_id,asset_status,
                 asset_condition,aggregate_version,updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
             ON CONFLICT (asset_id) DO UPDATE SET
                scope_key=EXCLUDED.scope_key,asset_code=EXCLUDED.asset_code,
                location_id=EXCLUDED.location_id,asset_status=EXCLUDED.asset_status,
                asset_condition=EXCLUDED.asset_condition,
                aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at",
        )
        .bind(self.aggregate.id().as_uuid())
        .bind(guard_scope_key(self.aggregate.scope().tenant_scope())?)
        .bind(self.aggregate.code())
        .bind(self.aggregate.location_id())
        .bind(status_name(self.aggregate.status()))
        .bind(condition_name(self.aggregate.condition()))
        .bind(to_i64(self.aggregate.version())?)
        .bind(self.aggregate.updated_at())
        .execute(&mut **transaction)
        .await?;
        Ok(())
    }
}

fn validate_guard_context(
    aggregate: &Asset,
    context: &GuardCommandContext<'_>,
) -> Result<(), CqrsSqlxError> {
    if context.context != CONTEXT_NAME
        || context.aggregate_type != AGGREGATE_TYPE
        || context.aggregate_id.as_uuid() != aggregate.id().as_uuid()
        || context.partition != asset_partition(aggregate.id())
    {
        return Err(CqrsSqlxError::GuardRejected(
            "assets command guard context mismatch",
        ));
    }
    Ok(())
}

fn validate_commit(request: &AssetCommit) -> Result<(), PortError> {
    if request.aggregate.validate().is_err()
        || fingerprint_asset_id(&request.identity.fingerprint) != request.aggregate.id()
        || !request
            .authority_scope
            .contains(request.aggregate.scope().tenant_scope())
    {
        return Err(PortError::Internal);
    }
    Ok(())
}

fn guard_scope_key(scope: &TenantScope) -> Result<String, CqrsSqlxError> {
    scope_key(scope).map_err(|_| CqrsSqlxError::GuardRejected("asset scope is invalid"))
}

fn aggregate_id(asset_id: AssetId) -> Result<AggregateId, PortError> {
    AggregateId::try_from_uuid(asset_id.as_uuid()).map_err(|_| PortError::Internal)
}

fn fingerprint_asset_id(fingerprint: &CommandFingerprint) -> AssetId {
    match fingerprint {
        CommandFingerprint::Create { asset_id, .. }
        | CommandFingerprint::PlaceInService { asset_id }
        | CommandFingerprint::MarkDegraded { asset_id }
        | CommandFingerprint::BeginMaintenance { asset_id, .. }
        | CommandFingerprint::ReturnToService { asset_id }
        | CommandFingerprint::Retire { asset_id } => *asset_id,
    }
}

fn request_hash(fingerprint: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let bytes = serde_json::to_vec(fingerprint).map_err(|_| PortError::Internal)?;
    let mut digest = Sha256::new();
    digest.update(b"erpx.assets.command-fingerprint.v1\0");
    digest.update(bytes);
    Ok(RequestHash::new(digest.finalize().into()))
}

fn audit_action(fingerprint: &CommandFingerprint) -> &'static str {
    match fingerprint {
        CommandFingerprint::Create { .. } => "assets.asset.create",
        CommandFingerprint::PlaceInService { .. } => "assets.asset.place_in_service",
        CommandFingerprint::MarkDegraded { .. } => "assets.asset.mark_degraded",
        CommandFingerprint::BeginMaintenance { .. } => "assets.asset.begin_maintenance",
        CommandFingerprint::ReturnToService { .. } => "assets.asset.return_to_service",
        CommandFingerprint::Retire { .. } => "assets.asset.retire",
    }
}

fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<Asset>,
) -> Result<AssetStoredCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != AGGREGATE_TYPE
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.id().as_uuid()
        || receipt.aggregate.version != receipt.response.version()
        || receipt.response.validate().is_err()
    {
        return Err(PortError::Internal);
    }
    Ok(AssetStoredCommit {
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
    i64::try_from(value).map_err(|_| CqrsSqlxError::GuardRejected("asset version is too large"))
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
        CqrsSqlxError::Database(database) => match database
            .as_database_error()
            .and_then(|error| error.constraint().map(str::to_owned))
        {
            Some(name) if name == "asset_guards_scope_key_asset_code_key" => {
                PortError::IdempotencyConflict
            }
            _ => PortError::Unavailable,
        },
        _ => PortError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_assets::{AssetScope, AssetStatus};
    use erpx_integration_contracts::assets;

    fn fingerprint(code: &str) -> CommandFingerprint {
        CommandFingerprint::Create {
            asset_id: AssetId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            code: code.into(),
            name: "Packing line".into(),
            scope: AssetScope::try_new(TenantScope::Company {
                holding_id: Uuid::from_u128(2),
                company_id: Uuid::from_u128(3),
            })
            .unwrap(),
            location_id: Uuid::from_u128(4),
        }
    }

    #[test]
    fn fingerprint_hash_is_stable_and_payload_sensitive() {
        assert_eq!(
            request_hash(&fingerprint("AST-1")).unwrap(),
            request_hash(&fingerprint("AST-1")).unwrap()
        );
        assert_ne!(
            request_hash(&fingerprint("AST-1")).unwrap(),
            request_hash(&fingerprint("AST-2")).unwrap()
        );
    }

    #[test]
    fn status_and_condition_names_match_frozen_ddl() {
        assert_eq!(
            status_name(AssetStatus::UnderMaintenance),
            "under_maintenance"
        );
        assert_eq!(
            condition_name(assets::AssetCondition::OutOfService),
            "out_of_service"
        );
    }
}
