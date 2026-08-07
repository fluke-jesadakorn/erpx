use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AtomicCommand, AtomicCommandOutcome, AuditWrite, CommandStore, CqrsSqlxError, OutboxEventWrite,
    RequestHash, WritePool,
};
use erpx_integration_contracts::IntegrationEvent;
use erpx_kernel::{AggregateId, CommandId, EventId, TenantScope};
use erpx_quality::{
    domain::{InspectionId, QualityInspection},
    ports::{
        CommandFingerprint, CommandIdentity, CommandUnitOfWork, PortError, QualityCommit,
        StoredCommit, WriteRepository,
    },
};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, EVENT_VERSION, SNAPSHOT_EVENT_TYPE, inspection_partition,
    projection::{QualityOutboxPayload, QualitySnapshot},
};

#[derive(Debug, Clone)]
pub struct QualityWriteRepository {
    store: CommandStore,
}

impl QualityWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl WriteRepository for QualityWriteRepository {
    async fn load(
        &self,
        authority_scope: &TenantScope,
        inspection_id: InspectionId,
    ) -> Result<Option<QualityInspection>, PortError> {
        let aggregate_id = aggregate_id(inspection_id)?;
        let snapshot = self
            .store
            .load_snapshot::<QualityInspection>(
                CONTEXT_NAME,
                &inspection_partition(inspection_id),
                AGGREGATE_TYPE,
                aggregate_id,
            )
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        if snapshot.version != snapshot.state.version()
            || snapshot.state.id() != inspection_id
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

#[derive(Debug, Clone)]
pub struct QualityCommandAdapter {
    store: CommandStore,
}

impl QualityCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl CommandUnitOfWork for QualityCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        let inspection_id = fingerprint_inspection_id(&identity.fingerprint);
        let receipt = self
            .store
            .find_receipt::<QualityInspection>(
                CONTEXT_NAME,
                &inspection_partition(inspection_id),
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?;
        receipt.map(receipt_to_stored).transpose()
    }

    async fn commit(&self, request: QualityCommit) -> Result<StoredCommit, PortError> {
        validate_commit(&request)?;
        let inspection_id = request.aggregate.id();
        let partition = inspection_partition(inspection_id);
        let aggregate_id = aggregate_id(inspection_id)?;
        let occurred_at = request.aggregate.updated_at();
        let command_id = request.identity.command_id;
        let request_hash = request_hash(&request.identity.fingerprint)?;
        let audit_action = audit_action(&request.identity.fingerprint);
        let idempotency_key = request.identity.idempotency_key.clone();
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
                payload: QualityOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();

        // The snapshot is intentionally last: CommandReceipt::projection_target
        // must identify the event consumed by this context's read projector.
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, SNAPSHOT_EVENT_TYPE, events.len()),
            event_type: SNAPSHOT_EVENT_TYPE.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: QualityOutboxPayload::Snapshot(QualitySnapshot {
                inspection: (&request.aggregate).into(),
            }),
        });

        let tenant_scope = request.aggregate.scope().clone();
        let aggregate = request.aggregate;
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition,
            command_id,
            correlation_id: request.correlation_id,
            idempotency_key,
            request_hash,
            tenant_scope,
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: AGGREGATE_TYPE.into(),
            aggregate_id,
            expected_version: request.expected_version,
            aggregate_state: serde_json::to_value(&aggregate).map_err(|_| PortError::Internal)?,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"quality-audit-v1"),
                action: audit_action.into(),
                occurred_at,
                details: json!({
                    "inspection_id": inspection_id.as_uuid(),
                    "expected_version": request.expected_version,
                }),
            },
            events,
        };
        let outcome = self.store.commit(command).await.map_err(map_store_error)?;
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

fn validate_commit(request: &QualityCommit) -> Result<(), PortError> {
    if request.aggregate.validate().is_err()
        || fingerprint_inspection_id(&request.identity.fingerprint) != request.aggregate.id()
        || !request.authority_scope.contains(request.aggregate.scope())
    {
        return Err(PortError::Internal);
    }
    Ok(())
}

fn aggregate_id(inspection_id: InspectionId) -> Result<AggregateId, PortError> {
    AggregateId::try_from_uuid(inspection_id.as_uuid()).map_err(|_| PortError::Internal)
}

fn fingerprint_inspection_id(fingerprint: &CommandFingerprint) -> InspectionId {
    match fingerprint {
        CommandFingerprint::Create { inspection_id, .. }
        | CommandFingerprint::Start { inspection_id }
        | CommandFingerprint::RecordResult { inspection_id, .. }
        | CommandFingerprint::Release { inspection_id }
        | CommandFingerprint::Cancel { inspection_id } => *inspection_id,
    }
}

fn request_hash(fingerprint: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let bytes = serde_json::to_vec(fingerprint).map_err(|_| PortError::Internal)?;
    let mut digest = Sha256::new();
    digest.update(b"erpx.quality.command-fingerprint.v1\0");
    digest.update(bytes);
    Ok(RequestHash::new(digest.finalize().into()))
}

fn deterministic_event_id(command_id: CommandId, event_type: &str, index: usize) -> EventId {
    let mut input = Vec::with_capacity(event_type.len() + 16);
    input.extend_from_slice(event_type.as_bytes());
    input.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(command_id.as_uuid(), &input))
        .expect("a SHA-256-derived UUID is non-nil")
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

fn audit_action(fingerprint: &CommandFingerprint) -> &'static str {
    match fingerprint {
        CommandFingerprint::Create { .. } => "quality.inspection.create",
        CommandFingerprint::Start { .. } => "quality.inspection.start",
        CommandFingerprint::RecordResult { .. } => "quality.inspection.record_result",
        CommandFingerprint::Release { .. } => "quality.inspection.release",
        CommandFingerprint::Cancel { .. } => "quality.inspection.cancel",
    }
}

fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<QualityInspection>,
) -> Result<StoredCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != AGGREGATE_TYPE
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.id().as_uuid()
        || receipt.aggregate.version != receipt.response.version()
        || receipt.response.validate().is_err()
    {
        return Err(PortError::Internal);
    }
    Ok(StoredCommit {
        command_id: receipt.command_id,
        correlation_id: receipt.correlation_id,
        aggregate: receipt.response,
        committed_events: receipt.committed_events,
        committed_at: receipt.committed_at,
        replayed: receipt.replayed,
    })
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
        CqrsSqlxError::Database(_) => PortError::Unavailable,
        _ => PortError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use erpx_quality::domain::InspectionKind;

    use super::*;

    fn inspection_id(value: u128) -> InspectionId {
        InspectionId::try_from_uuid(Uuid::from_u128(value)).unwrap()
    }

    fn fingerprint(quantity: u64) -> CommandFingerprint {
        CommandFingerprint::Create {
            inspection_id: inspection_id(1),
            scope: TenantScope::Company {
                holding_id: Uuid::from_u128(2),
                company_id: Uuid::from_u128(3),
            },
            kind: InspectionKind::Incoming,
            product_id: Uuid::from_u128(4),
            lot_reference: Some("LOT-1".into()),
            quantity,
        }
    }

    #[test]
    fn fingerprint_hash_is_stable_and_payload_sensitive() {
        let first = request_hash(&fingerprint(5)).unwrap();
        let retry = request_hash(&fingerprint(5)).unwrap();
        let changed = request_hash(&fingerprint(6)).unwrap();
        assert_eq!(first, retry);
        assert_ne!(first, changed);
        assert_eq!(first.as_bytes().len(), 32);
    }

    #[test]
    fn deterministic_ids_are_stable_and_namespaced() {
        let command = CommandId::try_from_uuid(Uuid::from_u128(9)).unwrap();
        let snapshot = deterministic_event_id(command, SNAPSHOT_EVENT_TYPE, 0);
        assert_eq!(
            snapshot,
            deterministic_event_id(command, SNAPSHOT_EVENT_TYPE, 0)
        );
        assert_ne!(
            snapshot,
            deterministic_event_id(command, "quality.inspection_completed", 0)
        );
    }
}
