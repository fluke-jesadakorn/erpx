use std::collections::BTreeMap;

use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, IntegrationEvent,
    ItemDescriptorDraft, ItemDescriptorDraftV2, RecordTypeId, StableRecordRef, catalog::ContextId,
    record_policy_id,
};
use erpx_inventory::{
    CommandFingerprint, CommandIdentity, CommandUnitOfWork, InventoryCommit, PortError,
    Reservation, ReservationId, ReservationStatus, StockItem, StockItemId, StockItemState,
    StoredCommit, WarehouseId, WriteRepository,
};
use erpx_kernel::{AggregateId, AuthorityScope, CommandId, DataClassification, EventId, RecordId};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, EVENT_VERSION, SNAPSHOT_EVENT_TYPE,
    projection::{InventoryOutboxPayload, InventorySnapshot},
    stock_item_partition,
};

#[derive(Debug, Clone)]
pub struct InventoryWriteRepository {
    pool: WritePool,
}

impl InventoryWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self { pool }
    }
}

#[derive(Debug)]
struct AuthoritySqlScope {
    mode: &'static str,
    holding_id: Option<Uuid>,
    company_id: Option<Uuid>,
    branch_id: Option<Uuid>,
    company_ids: Vec<Uuid>,
    branch_ids: Vec<Uuid>,
}

impl AuthoritySqlScope {
    fn try_from_scope(scope: &AuthorityScope) -> Result<Self, PortError> {
        scope.validate().map_err(|_| PortError::Denied)?;
        Ok(match scope {
            AuthorityScope::Platform => Self {
                mode: "platform",
                holding_id: None,
                company_id: None,
                branch_id: None,
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            AuthorityScope::Holding { holding_id } => Self {
                mode: "holding",
                holding_id: Some(*holding_id),
                company_id: None,
                branch_id: None,
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            AuthorityScope::Company {
                holding_id,
                company_id,
            } => Self {
                mode: "company",
                holding_id: Some(*holding_id),
                company_id: Some(*company_id),
                branch_id: None,
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Self {
                mode: "branch",
                holding_id: Some(*holding_id),
                company_id: Some(*company_id),
                branch_id: Some(*branch_id),
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            AuthorityScope::Subtree {
                holding_id,
                organization_node_id,
            } => Self {
                mode: "subtree",
                holding_id: Some(*holding_id),
                company_id: None,
                branch_id: None,
                company_ids: vec![*organization_node_id],
                branch_ids: Vec::new(),
            },
        })
    }
}

#[async_trait]
impl WriteRepository for InventoryWriteRepository {
    async fn load(
        &self,
        authority_scope: &AuthorityScope,
        stock_item_id: StockItemId,
    ) -> Result<Option<StockItem>, PortError> {
        let authority = AuthoritySqlScope::try_from_scope(authority_scope)?;
        let mut transaction = self.pool.begin().await.map_err(map_store_error)?;
        let row = sqlx::query(
            "SELECT aggregate_version,holding_id,company_id,branch_id,warehouse_id,item_code,
                    on_hand,reserved,created_at,updated_at
             FROM erpx_inventory_write.stock_item_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND stock_item_id=$1
               AND (
                 $2='platform'
                 OR ($2='holding' AND holding_id=$3)
                 OR ($2='company' AND holding_id=$3 AND company_id=$4)
                 OR ($2='branch' AND holding_id=$3 AND company_id=$4 AND branch_id=$5)
                 OR ($2='subtree' AND holding_id=$3 AND company_id=ANY($6)
                     AND ((branch_id IS NULL AND cardinality($7::uuid[])=0)
                          OR branch_id=ANY($7)))
               )",
        )
        .bind(stock_item_id.as_uuid())
        .bind(authority.mode)
        .bind(authority.holding_id)
        .bind(authority.company_id)
        .bind(authority.branch_id)
        .bind(&authority.company_ids)
        .bind(&authority.branch_ids)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| PortError::Unavailable)?;
        let Some(row) = row else {
            transaction
                .commit()
                .await
                .map_err(|_| PortError::Unavailable)?;
            return Ok(None);
        };
        let reservation_rows = sqlx::query(
            "SELECT reservation_id,quantity,demand_reference,reservation_status
             FROM erpx_inventory_write.reservation_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND stock_item_id=$1
             ORDER BY reservation_id",
        )
        .bind(stock_item_id.as_uuid())
        .fetch_all(&mut *transaction)
        .await
        .map_err(|_| PortError::Unavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let holding_id = row.try_get("holding_id").map_err(|_| PortError::Internal)?;
        let company_id = row.try_get("company_id").map_err(|_| PortError::Internal)?;
        let branch_id: Option<Uuid> = row.try_get("branch_id").map_err(|_| PortError::Internal)?;
        let scope = branch_id.map_or(
            AuthorityScope::Company {
                holding_id,
                company_id,
            },
            |branch_id| AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            },
        );
        let mut reservations = BTreeMap::new();
        for reservation_row in reservation_rows {
            let reservation_id = ReservationId::try_from_uuid(
                reservation_row
                    .try_get("reservation_id")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?;
            let value = Reservation {
                reservation_id,
                quantity: reservation_row
                    .try_get("quantity")
                    .map_err(|_| PortError::Internal)?,
                demand_reference: reservation_row
                    .try_get("demand_reference")
                    .map_err(|_| PortError::Internal)?,
                status: parse_reservation_status(
                    &reservation_row
                        .try_get::<String, _>("reservation_status")
                        .map_err(|_| PortError::Internal)?,
                )?,
            };
            if reservations.insert(reservation_id, value).is_some() {
                return Err(PortError::Internal);
            }
        }
        let aggregate = StockItem::try_rehydrate(StockItemState {
            id: stock_item_id,
            version: from_i64(
                row.try_get("aggregate_version")
                    .map_err(|_| PortError::Internal)?,
            )?,
            scope,
            warehouse_id: WarehouseId::try_from_uuid(
                row.try_get("warehouse_id")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            item_code: row.try_get("item_code").map_err(|_| PortError::Internal)?,
            on_hand: row.try_get("on_hand").map_err(|_| PortError::Internal)?,
            reserved: row.try_get("reserved").map_err(|_| PortError::Internal)?,
            reservations,
            created_at: row.try_get("created_at").map_err(|_| PortError::Internal)?,
            updated_at: row.try_get("updated_at").map_err(|_| PortError::Internal)?,
        })
        .map_err(|_| PortError::Internal)?;
        if !authority_scope.contains(aggregate.scope()) {
            return Err(PortError::Denied);
        }
        Ok(Some(aggregate))
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
        for draft in reservation_directory_drafts(&request.aggregate)? {
            events.push(OutboxEventWrite {
                event_id: deterministic_event_id(
                    command_id,
                    ITEM_DIRECTORY_OWNER_EVENT_TYPE,
                    events.len(),
                ),
                event_type: ITEM_DIRECTORY_OWNER_EVENT_TYPE.into(),
                event_version: ITEM_DIRECTORY_FACT_VERSION,
                partition: partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: InventoryOutboxPayload::Directory(draft),
            });
        }
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
            active_scope: aggregate.scope().clone(),
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: AGGREGATE_TYPE.into(),
            aggregate_id: aggregate_id(id)?,
            expected_version: request.expected_version,
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
            .commit_normalized(command, &guard)
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
    async fn authoritative_version(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        guard_context(&self.aggregate, context)?;
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT aggregate_version FROM erpx_inventory_write.stock_item_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND stock_item_id=$1
             FOR UPDATE",
        )
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        current.map(from_i64_store).transpose()
    }

    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        guard_context(&self.aggregate, context)?;
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT aggregate_version FROM erpx_inventory_write.stock_item_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND stock_item_id=$1 FOR UPDATE",
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
            "SELECT stock_item_id FROM erpx_inventory_write.stock_item_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id()
               AND scope_key=$1 AND warehouse_id=$2 AND item_code=$3 AND stock_item_id<>$4
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
            | CommandFingerprint::IssueStock { movement_id, .. }
            | CommandFingerprint::FulfillReservation { movement_id, .. } => {
                let exists: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM erpx_inventory_write.movement_authority
                     WHERE holding_id=erpx_cqrs_write.current_holding_id() AND movement_id=$1)",
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
                    "SELECT EXISTS (SELECT 1 FROM erpx_inventory_write.reservation_authority
                     WHERE holding_id=erpx_cqrs_write.current_holding_id() AND reservation_id=$1)",
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
        let (holding_id, company_id, branch_id) = exact_scope_columns(self.aggregate.scope())?;
        let result = sqlx::query(
            "INSERT INTO erpx_inventory_write.stock_item_authority
                (holding_id,stock_item_id,aggregate_version,scope_key,holding_id,company_id,
                 branch_id,warehouse_id,item_code,on_hand,reserved,created_at,updated_at)
             VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
             ON CONFLICT (holding_id,stock_item_id) DO UPDATE SET
                aggregate_version=EXCLUDED.aggregate_version,scope_key=EXCLUDED.scope_key,
                holding_id=EXCLUDED.holding_id,company_id=EXCLUDED.company_id,
                branch_id=EXCLUDED.branch_id,
                warehouse_id=EXCLUDED.warehouse_id,item_code=EXCLUDED.item_code,
                on_hand=EXCLUDED.on_hand,reserved=EXCLUDED.reserved,
                created_at=EXCLUDED.created_at,updated_at=EXCLUDED.updated_at
             WHERE erpx_inventory_write.stock_item_authority.holding_id=
                       erpx_cqrs_write.current_holding_id()
               AND erpx_inventory_write.stock_item_authority.aggregate_version + 1=
                       EXCLUDED.aggregate_version",
        )
        .bind(self.aggregate.id().as_uuid())
        .bind(to_i64(self.aggregate.version())?)
        .bind(scope_key(self.aggregate.scope())?)
        .bind(holding_id)
        .bind(company_id)
        .bind(branch_id)
        .bind(self.aggregate.warehouse_id().as_uuid())
        .bind(self.aggregate.item_code())
        .bind(self.aggregate.on_hand())
        .bind(self.aggregate.reserved())
        .bind(self.aggregate.created_at())
        .bind(self.aggregate.updated_at())
        .execute(&mut **transaction)
        .await?;
        if result.rows_affected() != 1 {
            return Err(CqrsSqlxError::GuardRejected(
                "inventory normalized version did not advance exactly once",
            ));
        }

        if let Some((movement_id, quantity_delta, procurement_line)) =
            movement(&self.fingerprint, &self.aggregate)
        {
            sqlx::query(
                "INSERT INTO erpx_inventory_write.movement_authority
                    (holding_id,movement_id,stock_item_id,quantity_delta,procurement_line_id,
                     aggregate_version,occurred_at)
                 VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6)",
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

        sqlx::query(
            "DELETE FROM erpx_inventory_write.reservation_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND stock_item_id=$1",
        )
        .bind(self.aggregate.id().as_uuid())
        .execute(&mut **transaction)
        .await?;
        for reservation in self.aggregate.reservations().values() {
            sqlx::query(
                "INSERT INTO erpx_inventory_write.reservation_authority
                    (holding_id,reservation_id,stock_item_id,quantity,demand_reference,
                     reservation_status,aggregate_version,updated_at)
                 VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7)",
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
        sqlx::query(
            "DELETE FROM erpx_inventory_write.reservation_records
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND stock_item_id=$1",
        )
        .bind(self.aggregate.id().as_uuid())
        .execute(&mut **transaction)
        .await?;
        for reservation in self.aggregate.reservations().values() {
            sqlx::query(
                "INSERT INTO erpx_inventory_write.reservation_records
                    (holding_id,reservation_id,stock_item_id,source_item_version,active_scope,
                     warehouse_id,item_code,quantity,demand_reference,reservation_status,updated_at)
                 VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
            )
            .bind(reservation.reservation_id.as_uuid())
            .bind(self.aggregate.id().as_uuid())
            .bind(to_i64(self.aggregate.version())?)
            .bind(
                serde_json::to_value(self.aggregate.scope())
                    .map_err(|_| CqrsSqlxError::GuardRejected("inventory scope is invalid"))?,
            )
            .bind(self.aggregate.warehouse_id().as_uuid())
            .bind(self.aggregate.item_code())
            .bind(reservation.quantity)
            .bind(&reservation.demand_reference)
            .bind(reservation_status(reservation.status))
            .bind(self.aggregate.updated_at())
            .execute(&mut **transaction)
            .await?;
        }
        Ok(())
    }
}

fn reservation_directory_drafts(
    aggregate: &StockItem,
) -> Result<Vec<ItemDescriptorDraftV2>, PortError> {
    aggregate
        .reservations()
        .values()
        .map(|reservation| {
            let record = StableRecordRef::try_new(
                RecordId::try_from_uuid(reservation.reservation_id.as_uuid())
                    .map_err(|_| PortError::Internal)?,
                ContextId::Inventory,
                RecordTypeId::try_new("inventory.reservation").map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?;
            let display_label =
                format!("{} {}", aggregate.item_code(), reservation.demand_reference);
            let draft = ItemDescriptorDraft {
                authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
                record,
                scope: aggregate.scope().clone(),
                lifecycle_state: reservation_status(reservation.status).into(),
                classification: DataClassification::Confidential,
                descriptor_version: aggregate.version(),
                authorized_search_text: display_label.clone(),
            };
            ItemDescriptorDraftV2::try_new(draft, display_label).map_err(|_| PortError::Internal)
        })
        .collect()
}

fn movement(
    fingerprint: &CommandFingerprint,
    aggregate: &StockItem,
) -> Option<(Uuid, i64, Option<Uuid>)> {
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
        CommandFingerprint::FulfillReservation {
            reservation_id,
            movement_id,
            ..
        } => aggregate
            .reservations()
            .get(reservation_id)
            .map(|reservation| (movement_id.as_uuid(), -reservation.quantity, None)),
        _ => None,
    }
}

fn reservation_status(status: ReservationStatus) -> &'static str {
    match status {
        ReservationStatus::Active => "active",
        ReservationStatus::Released => "released",
    }
}

fn parse_reservation_status(value: &str) -> Result<ReservationStatus, PortError> {
    match value {
        "active" => Ok(ReservationStatus::Active),
        "released" => Ok(ReservationStatus::Released),
        _ => Err(PortError::Internal),
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
    let next = request
        .expected_version
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(PortError::Internal)?;
    if request.aggregate.validate().is_err()
        || fingerprint_id(&request.identity.fingerprint) != request.aggregate.id()
        || !request.authority_scope.contains(request.aggregate.scope())
        || request.aggregate.version() != next
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
        | CommandFingerprint::ReleaseReservation { stock_item_id, .. }
        | CommandFingerprint::FulfillReservation { stock_item_id, .. } => *stock_item_id,
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
        CommandFingerprint::FulfillReservation { .. } => "inventory.reservation.fulfill",
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
fn scope_key(scope: &AuthorityScope) -> Result<String, CqrsSqlxError> {
    match scope {
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => Ok(format!("company:{holding_id}:{company_id}")),
        AuthorityScope::Branch {
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
fn from_i64(value: i64) -> Result<u64, PortError> {
    u64::try_from(value).map_err(|_| PortError::Internal)
}
fn from_i64_store(value: i64) -> Result<u64, CqrsSqlxError> {
    u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}
fn exact_scope_columns(
    scope: &AuthorityScope,
) -> Result<(Uuid, Uuid, Option<Uuid>), CqrsSqlxError> {
    match scope {
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => Ok((*holding_id, *company_id, None)),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok((*holding_id, *company_id, Some(*branch_id))),
        _ => Err(CqrsSqlxError::GuardRejected("inventory scope is not exact")),
    }
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
            movement(
                &fingerprint,
                &StockItem::register(
                    StockItemId::try_from_uuid(Uuid::from_u128(10)).unwrap(),
                    AuthorityScope::Company {
                        holding_id: Uuid::from_u128(1),
                        company_id: Uuid::from_u128(2)
                    },
                    erpx_inventory::WarehouseId::try_from_uuid(Uuid::from_u128(11)).unwrap(),
                    "ITEM-1".into(),
                    chrono::Utc::now(),
                )
                .unwrap()
            ),
            Some((movement_id.as_uuid(), -3, None))
        );
    }

    #[test]
    fn reservation_records_and_directory_facts_share_the_owner_uow() {
        let source = include_str!("command.rs");
        assert!(source.contains("reservation_directory_drafts(&request.aggregate)"));
        assert!(source.contains("InventoryOutboxPayload::Directory(draft)"));
        assert!(source.contains("erpx_inventory_write.reservation_records"));
        assert!(source.contains("source_item_version"));
        assert!(!source.contains(concat!("reservation_", "snapshot")));
    }
}
