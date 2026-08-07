use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::IntegrationEvent;
use erpx_inventory::{
    CommandFingerprint, CommandIdentity, CommandUnitOfWork, InventoryCommit, PortError,
    ReservationStatus, StockItem, StockItemId, StoredCommit, WriteRepository,
};
use erpx_kernel::{AggregateId, CommandId, EventId, TenantScope};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, EVENT_VERSION, SNAPSHOT_EVENT_TYPE,
    projection::{InventoryOutboxPayload, InventorySnapshot},
    stock_item_partition,
};

#[derive(Debug, Clone)]
pub struct InventoryWriteRepository {
    store: CommandStore,
}

impl InventoryWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl WriteRepository for InventoryWriteRepository {
    async fn load(
        &self,
        authority_scope: &TenantScope,
        stock_item_id: StockItemId,
    ) -> Result<Option<StockItem>, PortError> {
        let snapshot = self
            .store
            .load_snapshot::<StockItem>(
                CONTEXT_NAME,
                &stock_item_partition(stock_item_id),
                AGGREGATE_TYPE,
                aggregate_id(stock_item_id)?,
            )
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        if snapshot.version != snapshot.state.version()
            || snapshot.state.id() != stock_item_id
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
pub struct InventoryCommandAdapter {
    store: CommandStore,
}

impl InventoryCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl CommandUnitOfWork for InventoryCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        let id = fingerprint_id(&identity.fingerprint);
        self.store
            .find_receipt::<StockItem>(
                CONTEXT_NAME,
                &stock_item_partition(id),
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?
            .map(receipt_to_stored)
            .transpose()
    }

    async fn commit(&self, request: InventoryCommit) -> Result<StoredCommit, PortError> {
        validate_commit(&request)?;
        let id = request.aggregate.id();
        let partition = stock_item_partition(id);
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
                payload: InventoryOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, SNAPSHOT_EVENT_TYPE, events.len()),
            event_type: SNAPSHOT_EVENT_TYPE.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: InventoryOutboxPayload::Snapshot(InventorySnapshot {
                item: (&request.aggregate).into(),
            }),
        });
        let guard = InventoryGuard {
            aggregate: request.aggregate.clone(),
            fingerprint: request.identity.fingerprint.clone(),
        };
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
                audit_id: deterministic_uuid(command_id.as_uuid(), b"inventory-audit-v1"),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({
                    "stock_item_id": id.as_uuid(),
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

struct InventoryGuard {
    aggregate: StockItem,
    fingerprint: CommandFingerprint,
}

#[async_trait]
impl AtomicCommandGuard for InventoryGuard {
    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        guard_context(&self.aggregate, context)?;
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT aggregate_version FROM erpx_inventory_write.stock_item_guards
             WHERE stock_item_id=$1 FOR UPDATE",
        )
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        if matches!(
            (context.expected_version, current),
            (None, Some(_)) | (Some(_), None)
        ) {
            return Err(CqrsSqlxError::GuardRejected(
                "inventory guard existence mismatch",
            ));
        }
        let duplicate: Option<Uuid> = sqlx::query_scalar(
            "SELECT stock_item_id FROM erpx_inventory_write.stock_item_guards
             WHERE scope_key=$1 AND warehouse_id=$2 AND item_code=$3 AND stock_item_id<>$4
             ORDER BY stock_item_id LIMIT 1 FOR UPDATE",
        )
        .bind(scope_key(self.aggregate.scope())?)
        .bind(self.aggregate.warehouse_id().as_uuid())
        .bind(self.aggregate.item_code())
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        if duplicate.is_some() {
            return Err(CqrsSqlxError::GuardRejected(
                "inventory stock item already exists",
            ));
        }
        match &self.fingerprint {
            CommandFingerprint::ReceiveStock { movement_id, .. }
            | CommandFingerprint::IssueStock { movement_id, .. } => {
                let exists: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM erpx_inventory_write.movement_guards
                     WHERE movement_id=$1)",
                )
                .bind(movement_id.as_uuid())
                .fetch_one(&mut **transaction)
                .await?;
                if exists {
                    return Err(CqrsSqlxError::GuardRejected(
                        "inventory movement already exists",
                    ));
                }
            }
            CommandFingerprint::Reserve { reservation_id, .. } => {
                let exists: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM erpx_inventory_write.reservation_guards
                     WHERE reservation_id=$1)",
                )
                .bind(reservation_id.as_uuid())
                .fetch_one(&mut **transaction)
                .await?;
                if exists {
                    return Err(CqrsSqlxError::GuardRejected(
                        "inventory reservation already exists",
                    ));
                }
            }
            _ => {}
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
            || context.partition != stock_item_partition(self.aggregate.id())
            || context.aggregate_version != self.aggregate.version()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "inventory applied guard mismatch",
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_inventory_write.stock_item_guards
                (stock_item_id,scope_key,warehouse_id,item_code,on_hand,reserved,
                 aggregate_version,updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
             ON CONFLICT (stock_item_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,
                warehouse_id=EXCLUDED.warehouse_id,item_code=EXCLUDED.item_code,
                on_hand=EXCLUDED.on_hand,reserved=EXCLUDED.reserved,
                aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at",
        )
        .bind(self.aggregate.id().as_uuid())
        .bind(scope_key(self.aggregate.scope())?)
        .bind(self.aggregate.warehouse_id().as_uuid())
        .bind(self.aggregate.item_code())
        .bind(self.aggregate.on_hand())
        .bind(self.aggregate.reserved())
        .bind(to_i64(self.aggregate.version())?)
        .bind(self.aggregate.updated_at())
        .execute(&mut **transaction)
        .await?;

        if let Some((movement_id, quantity_delta, procurement_line)) = movement(&self.fingerprint) {
            sqlx::query(
                "INSERT INTO erpx_inventory_write.movement_guards
                    (movement_id,stock_item_id,quantity_delta,procurement_line_id,
                     aggregate_version,occurred_at) VALUES ($1,$2,$3,$4,$5,$6)",
            )
            .bind(movement_id)
            .bind(self.aggregate.id().as_uuid())
            .bind(quantity_delta)
            .bind(procurement_line)
            .bind(to_i64(self.aggregate.version())?)
            .bind(self.aggregate.updated_at())
            .execute(&mut **transaction)
            .await?;
        }

        sqlx::query("DELETE FROM erpx_inventory_write.reservation_guards WHERE stock_item_id=$1")
            .bind(self.aggregate.id().as_uuid())
            .execute(&mut **transaction)
            .await?;
        for reservation in self.aggregate.reservations().values() {
            sqlx::query(
                "INSERT INTO erpx_inventory_write.reservation_guards
                    (reservation_id,stock_item_id,quantity,demand_reference,reservation_status,
                     aggregate_version,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(reservation.reservation_id.as_uuid())
            .bind(self.aggregate.id().as_uuid())
            .bind(reservation.quantity)
            .bind(&reservation.demand_reference)
            .bind(reservation_status(reservation.status))
            .bind(to_i64(self.aggregate.version())?)
            .bind(self.aggregate.updated_at())
            .execute(&mut **transaction)
            .await?;
        }
        Ok(())
    }
}

fn movement(fingerprint: &CommandFingerprint) -> Option<(Uuid, i64, Option<Uuid>)> {
    match fingerprint {
        CommandFingerprint::ReceiveStock {
            movement_id,
            quantity,
            procurement_line,
            ..
        } => Some((
            movement_id.as_uuid(),
            *quantity,
            procurement_line.map(|line| line.as_uuid()),
        )),
        CommandFingerprint::IssueStock {
            movement_id,
            quantity,
            ..
        } => Some((movement_id.as_uuid(), -*quantity, None)),
        _ => None,
    }
}

fn reservation_status(status: ReservationStatus) -> &'static str {
    match status {
        ReservationStatus::Active => "active",
        ReservationStatus::Released => "released",
    }
}

fn guard_context(
    aggregate: &StockItem,
    context: &GuardCommandContext<'_>,
) -> Result<(), CqrsSqlxError> {
    if context.context != CONTEXT_NAME
        || context.aggregate_type != AGGREGATE_TYPE
        || context.aggregate_id.as_uuid() != aggregate.id().as_uuid()
        || context.partition != stock_item_partition(aggregate.id())
    {
        return Err(CqrsSqlxError::GuardRejected(
            "inventory guard context mismatch",
        ));
    }
    Ok(())
}

fn validate_commit(request: &InventoryCommit) -> Result<(), PortError> {
    if request.aggregate.validate().is_err()
        || fingerprint_id(&request.identity.fingerprint) != request.aggregate.id()
        || !request.authority_scope.contains(request.aggregate.scope())
    {
        return Err(PortError::Internal);
    }
    Ok(())
}

fn fingerprint_id(fingerprint: &CommandFingerprint) -> StockItemId {
    match fingerprint {
        CommandFingerprint::RegisterItem { stock_item_id, .. }
        | CommandFingerprint::ReceiveStock { stock_item_id, .. }
        | CommandFingerprint::IssueStock { stock_item_id, .. }
        | CommandFingerprint::Reserve { stock_item_id, .. }
        | CommandFingerprint::ReleaseReservation { stock_item_id, .. } => *stock_item_id,
    }
}

fn aggregate_id(id: StockItemId) -> Result<AggregateId, PortError> {
    AggregateId::try_from_uuid(id.as_uuid()).map_err(|_| PortError::Internal)
}
fn request_hash(fingerprint: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let mut digest = Sha256::new();
    digest.update(b"erpx.inventory.command-fingerprint.v1\0");
    digest.update(serde_json::to_vec(fingerprint).map_err(|_| PortError::Internal)?);
    Ok(RequestHash::new(digest.finalize().into()))
}
fn audit_action(fingerprint: &CommandFingerprint) -> &'static str {
    match fingerprint {
        CommandFingerprint::RegisterItem { .. } => "inventory.stock_item.register",
        CommandFingerprint::ReceiveStock { .. } => "inventory.stock.receive",
        CommandFingerprint::IssueStock { .. } => "inventory.stock.issue",
        CommandFingerprint::Reserve { .. } => "inventory.stock.reserve",
        CommandFingerprint::ReleaseReservation { .. } => "inventory.reservation.release",
    }
}

fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<StockItem>,
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
        _ => Err(CqrsSqlxError::GuardRejected("inventory scope is not exact")),
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

    #[test]
    fn stable_hash_and_signed_movement_are_deterministic() {
        let item = StockItemId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let movement_id =
            erpx_inventory::StockMovementId::try_from_uuid(Uuid::from_u128(2)).unwrap();
        let fingerprint = CommandFingerprint::IssueStock {
            stock_item_id: item,
            movement_id,
            quantity: 3,
        };
        assert_eq!(
            request_hash(&fingerprint).unwrap(),
            request_hash(&fingerprint).unwrap()
        );
        assert_eq!(
            movement(&fingerprint),
            Some((movement_id.as_uuid(), -3, None))
        );
    }
}
