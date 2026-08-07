use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::IntegrationEvent;
use erpx_kernel::{AggregateId, CommandId, EventId, TenantScope};
use erpx_procurement::{
    CommandFingerprint, CommandIdentity, CommandUnitOfWork, PortError, ProcurementCommit,
    PurchaseOrder, PurchaseOrderId, StoredCommit, WriteRepository,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, EVENT_VERSION, SNAPSHOT_EVENT_TYPE,
    projection::{ProcurementOutboxPayload, ProcurementSnapshot, status_name},
    purchase_order_partition,
};

#[derive(Debug, Clone)]
pub struct ProcurementWriteRepository {
    store: CommandStore,
}

impl ProcurementWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl WriteRepository for ProcurementWriteRepository {
    async fn load(
        &self,
        authority_scope: &TenantScope,
        purchase_order_id: PurchaseOrderId,
    ) -> Result<Option<PurchaseOrder>, PortError> {
        let snapshot = self
            .store
            .load_snapshot::<PurchaseOrder>(
                CONTEXT_NAME,
                &purchase_order_partition(purchase_order_id),
                AGGREGATE_TYPE,
                aggregate_id(purchase_order_id)?,
            )
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        if snapshot.version != snapshot.state.version()
            || snapshot.state.id() != purchase_order_id
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
pub struct ProcurementCommandAdapter {
    store: CommandStore,
}

impl ProcurementCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl CommandUnitOfWork for ProcurementCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        let id = fingerprint_id(&identity.fingerprint);
        self.store
            .find_receipt::<PurchaseOrder>(
                CONTEXT_NAME,
                &purchase_order_partition(id),
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?
            .map(receipt_to_stored)
            .transpose()
    }

    async fn commit(&self, request: ProcurementCommit) -> Result<StoredCommit, PortError> {
        validate_commit(&request)?;
        let id = request.aggregate.id();
        let partition = purchase_order_partition(id);
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
                payload: ProcurementOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, SNAPSHOT_EVENT_TYPE, events.len()),
            event_type: SNAPSHOT_EVENT_TYPE.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: ProcurementOutboxPayload::Snapshot(ProcurementSnapshot {
                order: (&request.aggregate).into(),
            }),
        });
        let guard = ProcurementGuard(request.aggregate.clone());
        let aggregate = request.aggregate;
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition,
            command_id,
            correlation_id: request.correlation_id,
            idempotency_key: request.identity.idempotency_key,
            request_hash: request_hash(&request.identity.fingerprint)?,
            tenant_scope: request.authority_scope,
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: AGGREGATE_TYPE.into(),
            aggregate_id: aggregate_id(id)?,
            expected_version: request.expected_version,
            aggregate_state: serde_json::to_value(&aggregate).map_err(|_| PortError::Internal)?,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"procurement-audit-v1"),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({
                    "purchase_order_id": id.as_uuid(),
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

struct ProcurementGuard(PurchaseOrder);

#[async_trait]
impl AtomicCommandGuard for ProcurementGuard {
    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        guard_context(&self.0, context)?;
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT aggregate_version FROM erpx_procurement_write.purchase_order_guards
             WHERE purchase_order_id=$1 FOR UPDATE",
        )
        .bind(self.0.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        if matches!(
            (context.expected_version, current),
            (None, Some(_)) | (Some(_), None)
        ) {
            return Err(CqrsSqlxError::GuardRejected(
                "procurement guard existence mismatch",
            ));
        }
        let duplicate: Option<Uuid> = sqlx::query_scalar(
            "SELECT purchase_order_id FROM erpx_procurement_write.purchase_order_guards
             WHERE scope_key=$1 AND order_code=$2 AND purchase_order_id<>$3
             ORDER BY purchase_order_id LIMIT 1 FOR UPDATE",
        )
        .bind(scope_key(self.0.scope())?)
        .bind(self.0.order_code())
        .bind(self.0.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        if duplicate.is_some() {
            return Err(CqrsSqlxError::GuardRejected(
                "procurement order code already exists",
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
            || context.aggregate_id.as_uuid() != self.0.id().as_uuid()
            || context.partition != purchase_order_partition(self.0.id())
            || context.aggregate_version != self.0.version()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "procurement applied guard mismatch",
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_procurement_write.purchase_order_guards
                (purchase_order_id,scope_key,order_code,supplier_id,currency,
                 order_status,aggregate_version,updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
             ON CONFLICT (purchase_order_id) DO UPDATE SET
                scope_key=EXCLUDED.scope_key,order_code=EXCLUDED.order_code,
                supplier_id=EXCLUDED.supplier_id,currency=EXCLUDED.currency,
                order_status=EXCLUDED.order_status,
                aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at",
        )
        .bind(self.0.id().as_uuid())
        .bind(scope_key(self.0.scope())?)
        .bind(self.0.order_code())
        .bind(self.0.supplier_id().as_uuid())
        .bind(self.0.currency())
        .bind(status_name(self.0.status()))
        .bind(to_i64(self.0.version())?)
        .bind(self.0.updated_at())
        .execute(&mut **transaction)
        .await?;
        Ok(())
    }
}

fn guard_context(
    aggregate: &PurchaseOrder,
    context: &GuardCommandContext<'_>,
) -> Result<(), CqrsSqlxError> {
    if context.context != CONTEXT_NAME
        || context.aggregate_type != AGGREGATE_TYPE
        || context.aggregate_id.as_uuid() != aggregate.id().as_uuid()
        || context.partition != purchase_order_partition(aggregate.id())
    {
        return Err(CqrsSqlxError::GuardRejected(
            "procurement guard context mismatch",
        ));
    }
    Ok(())
}

fn validate_commit(request: &ProcurementCommit) -> Result<(), PortError> {
    if request.aggregate.validate().is_err()
        || fingerprint_id(&request.identity.fingerprint) != request.aggregate.id()
        || !request.authority_scope.contains(request.aggregate.scope())
    {
        return Err(PortError::Internal);
    }
    Ok(())
}

fn fingerprint_id(fingerprint: &CommandFingerprint) -> PurchaseOrderId {
    match fingerprint {
        CommandFingerprint::CreateOrder {
            purchase_order_id, ..
        }
        | CommandFingerprint::ReleaseOrder { purchase_order_id }
        | CommandFingerprint::CancelOrder { purchase_order_id } => *purchase_order_id,
    }
}

fn aggregate_id(id: PurchaseOrderId) -> Result<AggregateId, PortError> {
    AggregateId::try_from_uuid(id.as_uuid()).map_err(|_| PortError::Internal)
}

fn request_hash(fingerprint: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let mut digest = Sha256::new();
    digest.update(b"erpx.procurement.command-fingerprint.v1\0");
    digest.update(serde_json::to_vec(fingerprint).map_err(|_| PortError::Internal)?);
    Ok(RequestHash::new(digest.finalize().into()))
}

fn audit_action(fingerprint: &CommandFingerprint) -> &'static str {
    match fingerprint {
        CommandFingerprint::CreateOrder { .. } => "procurement.purchase_order.create",
        CommandFingerprint::ReleaseOrder { .. } => "procurement.purchase_order.release",
        CommandFingerprint::CancelOrder { .. } => "procurement.purchase_order.cancel",
    }
}

fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<PurchaseOrder>,
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

fn deterministic_event_id(command: CommandId, event_type: &str, index: usize) -> EventId {
    let mut input = event_type.as_bytes().to_vec();
    input.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(command.as_uuid(), &input)).unwrap()
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

fn scope_key(scope: &TenantScope) -> Result<String, CqrsSqlxError> {
    match scope {
        TenantScope::Company {
            holding_id,
            company_id,
        } => Ok(format!("company:{holding_id}:{company_id}")),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok(format!("branch:{holding_id}:{company_id}:{branch_id}")),
        _ => Err(CqrsSqlxError::GuardRejected(
            "procurement scope is not exact",
        )),
    }
}

fn to_i64(value: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(value).map_err(|_| CqrsSqlxError::GuardRejected("numeric value is too large"))
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
        CqrsSqlxError::GuardRejected(_) => PortError::Conflict {
            expected: 0,
            actual: 0,
        },
        CqrsSqlxError::Database(_) => PortError::Unavailable,
        _ => PortError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_procurement::PurchaseOrderStatus;

    #[test]
    fn stable_hash_is_payload_sensitive() {
        let id = PurchaseOrderId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let release = CommandFingerprint::ReleaseOrder {
            purchase_order_id: id,
        };
        let cancel = CommandFingerprint::CancelOrder {
            purchase_order_id: id,
        };
        assert_eq!(
            request_hash(&release).unwrap(),
            request_hash(&release).unwrap()
        );
        assert_ne!(
            request_hash(&release).unwrap(),
            request_hash(&cancel).unwrap()
        );
    }

    #[test]
    fn database_status_names_are_stable() {
        assert_eq!(status_name(PurchaseOrderStatus::Cancelled), "cancelled");
    }
}
