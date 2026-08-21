use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::{
    CurrencyCode, ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, IntegrationEvent,
    ItemDescriptorDraft, ItemDescriptorDraftV2, MoneyAmount, Quantity, RecordTypeId,
    RecordVersionRef, ReferenceCode, StableRecordRef, catalog::ContextId, inventory, procurement,
    record_policy_id,
};
use erpx_kernel::{AggregateId, AuthorityScope, CommandId, DataClassification, EventId, RecordId};
use erpx_procurement::{
    GoodsReceipt, GoodsReceiptDomainEvent, GoodsReceiptId, InventoryStockItemId,
    InventoryStockItemSource, PortError, ProcurementSourceGuard, ProcurementWorkflowAggregate,
    ProcurementWorkflowCommandFingerprint, ProcurementWorkflowCommandIdentity,
    ProcurementWorkflowCommit, ProcurementWorkflowDomainEvent, ProcurementWorkflowRepository,
    ProcurementWorkflowUnitOfWork, PurchaseOrderReceiptSource, PurchaseOrderStatus,
    PurchaseRequisition, PurchaseRequisitionDomainEvent, PurchaseRequisitionId,
    ReceiptLineAllowance, StoredProcurementWorkflowCommit, WriteRepository,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    CONTEXT_NAME, EVENT_VERSION, ProcurementWriteRepository,
    workflow_projection::{
        GOODS_RECEIPT_AGGREGATE_TYPE, GOODS_RECEIPT_SNAPSHOT_EVENT_TYPE,
        ProcurementWorkflowOutboxPayload, ProcurementWorkflowSnapshot, REQUISITION_AGGREGATE_TYPE,
        REQUISITION_SNAPSHOT_EVENT_TYPE, goods_receipt_partition, goods_receipt_status_name,
        requisition_partition, requisition_status_name,
    },
};

#[derive(Debug, Clone)]
pub struct ProcurementWorkflowSqlxAdapter {
    pool: WritePool,
    store: CommandStore,
}

impl ProcurementWorkflowSqlxAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool.clone()),
            pool,
        }
    }

    pub fn new_write(pool: WritePool) -> Self {
        Self::new(pool)
    }
}

#[async_trait]
impl ProcurementWorkflowRepository for ProcurementWorkflowSqlxAdapter {
    async fn load_requisition(
        &self,
        authority_scope: &AuthorityScope,
        requisition_id: PurchaseRequisitionId,
    ) -> Result<Option<PurchaseRequisition>, PortError> {
        authority_scope.validate().map_err(|_| PortError::Denied)?;
        let mut transaction = self.pool.begin().await.map_err(map_store_error)?;
        let row = sqlx::query(
            "SELECT state FROM erpx_procurement_write.purchase_requisition_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND requisition_id=$1",
        )
        .bind(requisition_id.as_uuid())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| PortError::Unavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let value = decode_authorized(row, authority_scope, |value: &PurchaseRequisition| {
            value.scope()
        })?;
        if value
            .as_ref()
            .is_some_and(|value| value.validate().is_err())
        {
            return Err(PortError::Internal);
        }
        Ok(value)
    }

    async fn load_goods_receipt(
        &self,
        authority_scope: &AuthorityScope,
        goods_receipt_id: GoodsReceiptId,
    ) -> Result<Option<GoodsReceipt>, PortError> {
        authority_scope.validate().map_err(|_| PortError::Denied)?;
        let mut transaction = self.pool.begin().await.map_err(map_store_error)?;
        let row = sqlx::query(
            "SELECT state FROM erpx_procurement_write.goods_receipt_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND goods_receipt_id=$1",
        )
        .bind(goods_receipt_id.as_uuid())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| PortError::Unavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let value = decode_authorized(row, authority_scope, |value: &GoodsReceipt| value.scope())?;
        if value
            .as_ref()
            .is_some_and(|value| value.validate().is_err())
        {
            return Err(PortError::Internal);
        }
        Ok(value)
    }

    async fn load_purchase_order_receipt_source(
        &self,
        authority_scope: &AuthorityScope,
        source: &RecordVersionRef,
    ) -> Result<Option<PurchaseOrderReceiptSource>, PortError> {
        if source.record().owner_context() != ContextId::Procurement
            || source.record().record_type().as_str() != "procurement.purchase-order"
        {
            return Ok(None);
        }
        let order_id =
            erpx_procurement::PurchaseOrderId::try_from_uuid(source.record().id().as_uuid())
                .map_err(|_| PortError::Internal)?;
        let Some(order) = ProcurementWriteRepository::new(self.pool.clone())
            .load(authority_scope, order_id)
            .await?
        else {
            return Ok(None);
        };
        if order.version() != source.version() || order.status() != PurchaseOrderStatus::Released {
            return Ok(None);
        }
        let mut transaction = self.pool.begin().await.map_err(map_store_error)?;
        let allowances =
            load_current_allowances(&mut transaction, order_id.as_uuid(), false).await?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)?;
        Ok(Some(PurchaseOrderReceiptSource { order, allowances }))
    }

    async fn resolve_inventory_stock_items(
        &self,
        authority_scope: &AuthorityScope,
        stock_item_ids: &[InventoryStockItemId],
    ) -> Result<Vec<InventoryStockItemSource>, PortError> {
        authority_scope.validate().map_err(|_| PortError::Denied)?;
        if stock_item_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = stock_item_ids
            .iter()
            .map(|id| id.as_uuid())
            .collect::<Vec<_>>();
        let mut transaction = self.pool.begin().await.map_err(map_store_error)?;
        let rows = sqlx::query(
            "SELECT stock_item_id,holding_id,company_id,branch_id
             FROM erpx_inventory_write.stock_item_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id()
               AND stock_item_id=ANY($1::uuid[]) ORDER BY stock_item_id FOR SHARE",
        )
        .bind(&ids)
        .fetch_all(&mut *transaction)
        .await
        .map_err(|_| PortError::Unavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)?;
        rows.into_iter()
            .map(|row| {
                let scope = row_scope(&row)?;
                if !authority_scope.contains(&scope) {
                    return Err(PortError::Denied);
                }
                Ok(InventoryStockItemSource {
                    stock_item_id: InventoryStockItemId::try_from_uuid(
                        row.try_get("stock_item_id")
                            .map_err(|_| PortError::Internal)?,
                    )
                    .map_err(|_| PortError::Internal)?,
                    scope,
                })
            })
            .collect()
    }
}

#[async_trait]
impl ProcurementWorkflowUnitOfWork for ProcurementWorkflowSqlxAdapter {
    async fn find_workflow_receipt(
        &self,
        identity: &ProcurementWorkflowCommandIdentity,
    ) -> Result<Option<StoredProcurementWorkflowCommit>, PortError> {
        let (partition, _) = fingerprint_target(&identity.fingerprint);
        self.store
            .find_receipt::<ProcurementWorkflowAggregate>(
                CONTEXT_NAME,
                &partition,
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?
            .map(receipt_to_stored)
            .transpose()
    }

    async fn commit_workflow(
        &self,
        request: ProcurementWorkflowCommit,
    ) -> Result<StoredProcurementWorkflowCommit, PortError> {
        validate_commit(&request)?;
        let partition = aggregate_partition(&request.aggregate);
        let aggregate_type = aggregate_type(&request.aggregate);
        let aggregate_uuid = aggregate_uuid(&request.aggregate);
        let occurred_at = aggregate_updated_at(&request.aggregate);
        let command_id = request.identity.command_id;
        let integration_events = integration_events(&request.domain_events)?;
        let mut events = integration_events
            .iter()
            .enumerate()
            .map(|(index, event)| OutboxEventWrite {
                event_id: deterministic_event_id(command_id, event.event_type(), index),
                event_type: event.event_type().into(),
                event_version: u32::from(event.schema_version()),
                partition: partition.clone(),
                causation_id: None,
                occurred_at,
                payload: ProcurementWorkflowOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        let snapshot_event_type = snapshot_event_type(&request.aggregate);
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, snapshot_event_type, events.len()),
            event_type: snapshot_event_type.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: None,
            occurred_at,
            payload: ProcurementWorkflowOutboxPayload::Snapshot(workflow_snapshot(
                &request.aggregate,
            )),
        });
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(
                command_id,
                ITEM_DIRECTORY_OWNER_EVENT_TYPE,
                events.len(),
            ),
            event_type: ITEM_DIRECTORY_OWNER_EVENT_TYPE.into(),
            event_version: ITEM_DIRECTORY_FACT_VERSION,
            partition: partition.clone(),
            causation_id: None,
            occurred_at,
            payload: ProcurementWorkflowOutboxPayload::Directory(directory_draft(
                &request.aggregate,
            )?),
        });
        let guard = WorkflowGuard {
            aggregate: request.aggregate.clone(),
            source_guards: request.source_guards.clone(),
        };
        let response = request.aggregate;
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition,
            command_id,
            correlation_id: request.correlation_id,
            idempotency_key: request.identity.idempotency_key,
            request_hash: request_hash(&request.identity.fingerprint)?,
            active_scope: response.scope().clone(),
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: aggregate_type.into(),
            aggregate_id: AggregateId::try_from_uuid(aggregate_uuid)
                .map_err(|_| PortError::Internal)?,
            expected_version: request.expected_version,
            response,
            audit: AuditWrite {
                audit_id: deterministic_uuid(
                    command_id.as_uuid(),
                    b"procurement-workflow-audit-v1",
                ),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({
                    "aggregate_id": aggregate_uuid,
                    "aggregate_type": aggregate_type,
                    "expected_version": request.expected_version,
                    "source_guard_count": request.source_guards.len(),
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

struct WorkflowGuard {
    aggregate: ProcurementWorkflowAggregate,
    source_guards: Vec<ProcurementSourceGuard>,
}

#[async_trait]
impl AtomicCommandGuard for WorkflowGuard {
    async fn authoritative_version(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        guard_context(&self.aggregate, context)?;
        authoritative_version(transaction, &self.aggregate).await
    }

    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        guard_context(&self.aggregate, context)?;
        validate_source_guards(transaction, &self.aggregate, &self.source_guards).await?;
        let current = authoritative_version(transaction, &self.aggregate).await?;
        if matches!(
            (context.expected_version, current),
            (None, Some(_)) | (Some(_), None)
        ) {
            return Err(CqrsSqlxError::GuardRejected(
                "procurement workflow existence mismatch",
            ));
        }
        validate_unique_code(transaction, &self.aggregate).await
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if context.context != CONTEXT_NAME
            || context.aggregate_type != aggregate_type(&self.aggregate)
            || context.aggregate_id.as_uuid() != aggregate_uuid(&self.aggregate)
            || context.partition != aggregate_partition(&self.aggregate)
            || context.aggregate_version != self.aggregate.version()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "procurement workflow applied guard mismatch",
            ));
        }
        validate_source_guards(transaction, &self.aggregate, &self.source_guards).await?;
        match &self.aggregate {
            ProcurementWorkflowAggregate::Requisition(value) => {
                persist_requisition(transaction, value).await
            }
            ProcurementWorkflowAggregate::GoodsReceipt(value) => {
                persist_goods_receipt(transaction, value).await
            }
        }
    }
}

async fn authoritative_version(
    transaction: &mut Transaction<'_, Postgres>,
    aggregate: &ProcurementWorkflowAggregate,
) -> Result<Option<u64>, CqrsSqlxError> {
    let (table, column, id) = match aggregate {
        ProcurementWorkflowAggregate::Requisition(value) => (
            "erpx_procurement_write.purchase_requisition_authority",
            "requisition_id",
            value.id().as_uuid(),
        ),
        ProcurementWorkflowAggregate::GoodsReceipt(value) => (
            "erpx_procurement_write.goods_receipt_authority",
            "goods_receipt_id",
            value.id().as_uuid(),
        ),
    };
    let statement = format!(
        "SELECT aggregate_version FROM {table}
         WHERE holding_id=erpx_cqrs_write.current_holding_id() AND {column}=$1 FOR UPDATE"
    );
    let current: Option<i64> = sqlx::query_scalar(&statement)
        .bind(id)
        .fetch_optional(&mut **transaction)
        .await?;
    current
        .map(|value| u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange))
        .transpose()
}

async fn validate_unique_code(
    transaction: &mut Transaction<'_, Postgres>,
    aggregate: &ProcurementWorkflowAggregate,
) -> Result<(), CqrsSqlxError> {
    let duplicate: Option<Uuid> = match aggregate {
        ProcurementWorkflowAggregate::Requisition(value) => {
            sqlx::query_scalar(
                "SELECT requisition_id
             FROM erpx_procurement_write.purchase_requisition_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id()
               AND scope_key=$1 AND requisition_code=$2 AND requisition_id<>$3
             ORDER BY requisition_id LIMIT 1 FOR UPDATE",
            )
            .bind(scope_key(value.scope())?)
            .bind(value.requisition_code())
            .bind(value.id().as_uuid())
            .fetch_optional(&mut **transaction)
            .await?
        }
        ProcurementWorkflowAggregate::GoodsReceipt(value) => {
            sqlx::query_scalar(
                "SELECT goods_receipt_id
             FROM erpx_procurement_write.goods_receipt_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id()
               AND scope_key=$1 AND receipt_code=$2 AND goods_receipt_id<>$3
             ORDER BY goods_receipt_id LIMIT 1 FOR UPDATE",
            )
            .bind(scope_key(value.scope())?)
            .bind(value.receipt_code())
            .bind(value.id().as_uuid())
            .fetch_optional(&mut **transaction)
            .await?
        }
    };
    if duplicate.is_some() {
        return Err(CqrsSqlxError::GuardRejected(
            "procurement workflow code already exists",
        ));
    }
    Ok(())
}

async fn validate_source_guards(
    transaction: &mut Transaction<'_, Postgres>,
    aggregate: &ProcurementWorkflowAggregate,
    guards: &[ProcurementSourceGuard],
) -> Result<(), CqrsSqlxError> {
    match aggregate {
        ProcurementWorkflowAggregate::Requisition(_) if guards.is_empty() => Ok(()),
        ProcurementWorkflowAggregate::Requisition(_) => Err(CqrsSqlxError::GuardRejected(
            "requisition received unrelated source evidence",
        )),
        ProcurementWorkflowAggregate::GoodsReceipt(receipt) => {
            let [
                ProcurementSourceGuard::ReceiptQuantities {
                    purchase_order,
                    allowances,
                    inventory_stock_items,
                },
            ] = guards
            else {
                return Err(CqrsSqlxError::GuardRejected(
                    "goods receipt requires one atomic source guard",
                ));
            };
            if purchase_order != receipt.source_purchase_order() {
                return Err(CqrsSqlxError::GuardRejected(
                    "goods receipt purchase-order evidence changed",
                ));
            }
            validate_purchase_order_allowances(transaction, receipt, allowances).await?;
            validate_inventory_sources(transaction, receipt, inventory_stock_items).await
        }
    }
}

async fn validate_purchase_order_allowances(
    transaction: &mut Transaction<'_, Postgres>,
    receipt: &GoodsReceipt,
    supplied: &[ReceiptLineAllowance],
) -> Result<(), CqrsSqlxError> {
    let source = receipt.source_purchase_order();
    let row = sqlx::query(
        "SELECT aggregate_version,scope_key,order_status
         FROM erpx_procurement_write.purchase_order_authority
         WHERE holding_id=erpx_cqrs_write.current_holding_id() AND purchase_order_id=$1
         FOR UPDATE",
    )
    .bind(source.record().id().as_uuid())
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or(CqrsSqlxError::GuardRejected(
        "goods receipt purchase order is missing",
    ))?;
    if row.try_get::<i64, _>("aggregate_version")? != to_i64(source.version())?
        || row.try_get::<String, _>("scope_key")? != scope_key(receipt.scope())?
        || row.try_get::<String, _>("order_status")? != "released"
    {
        return Err(CqrsSqlxError::GuardRejected(
            "goods receipt purchase order version, status, or scope changed",
        ));
    }
    let current = load_current_allowances(transaction, source.record().id().as_uuid(), true)
        .await
        .map_err(|_| CqrsSqlxError::GuardRejected("receipt allowances are unavailable"))?;
    let supplied = supplied
        .iter()
        .map(|line| (line.purchase_order_line_id, line))
        .collect::<BTreeMap<_, _>>();
    let current = current
        .iter()
        .map(|line| (line.purchase_order_line_id, line))
        .collect::<BTreeMap<_, _>>();
    if supplied.len() != current.len()
        || supplied.iter().any(|(id, line)| {
            current.get(id).is_none_or(|actual| {
                line.ordered_quantity != actual.ordered_quantity
                    || line.already_accepted_quantity != actual.already_accepted_quantity
            })
        })
    {
        return Err(CqrsSqlxError::GuardRejected(
            "receipt allowance snapshot is stale",
        ));
    }
    for line in receipt.lines() {
        let allowance =
            current
                .get(&line.purchase_order_line_id)
                .ok_or(CqrsSqlxError::GuardRejected(
                    "receipt references an unknown purchase-order line",
                ))?;
        let remaining = allowance
            .ordered_quantity
            .checked_sub(allowance.already_accepted_quantity)
            .ok_or(CqrsSqlxError::NumericRange)?;
        if line.received_quantity <= 0 || line.received_quantity > remaining {
            return Err(CqrsSqlxError::GuardRejected(
                "receipt exceeds the current purchase-order line remainder",
            ));
        }
    }
    Ok(())
}

async fn validate_inventory_sources(
    transaction: &mut Transaction<'_, Postgres>,
    receipt: &GoodsReceipt,
    supplied: &[InventoryStockItemSource],
) -> Result<(), CqrsSqlxError> {
    let expected = receipt
        .lines()
        .iter()
        .map(|line| line.inventory_stock_item_id)
        .collect::<BTreeSet<_>>();
    let supplied = supplied
        .iter()
        .map(|source| (source.stock_item_id, &source.scope))
        .collect::<BTreeMap<_, _>>();
    if supplied.len() != expected.len()
        || expected.iter().any(|id| {
            supplied
                .get(id)
                .is_none_or(|scope| *scope != receipt.scope())
        })
    {
        return Err(CqrsSqlxError::GuardRejected(
            "goods receipt inventory source evidence is incomplete",
        ));
    }
    let ids = expected.iter().map(|id| id.as_uuid()).collect::<Vec<_>>();
    let rows = sqlx::query(
        "SELECT stock_item_id,scope_key FROM erpx_inventory_write.stock_item_authority
         WHERE holding_id=erpx_cqrs_write.current_holding_id()
           AND stock_item_id=ANY($1::uuid[]) ORDER BY stock_item_id FOR UPDATE",
    )
    .bind(&ids)
    .fetch_all(&mut **transaction)
    .await?;
    if rows.len() != expected.len()
        || rows.iter().any(|row| {
            row.try_get::<Uuid, _>("stock_item_id")
                .ok()
                .and_then(|id| InventoryStockItemId::try_from_uuid(id).ok())
                .and_then(|id| supplied.get(&id))
                .is_none_or(|scope| {
                    row.try_get::<String, _>("scope_key")
                        .ok()
                        .is_none_or(|key| key != scope.canonical_key())
                })
        })
    {
        return Err(CqrsSqlxError::GuardRejected(
            "goods receipt inventory item scope changed",
        ));
    }
    Ok(())
}

async fn persist_requisition(
    transaction: &mut Transaction<'_, Postgres>,
    value: &PurchaseRequisition,
) -> Result<(), CqrsSqlxError> {
    let (_, company_id, branch_id) = exact_scope_columns(value.scope())?;
    let budget_code = value.budget_code().ok_or(CqrsSqlxError::GuardRejected(
        "new requisition state has no budget evidence",
    ))?;
    let result = sqlx::query(
        "INSERT INTO erpx_procurement_write.purchase_requisition_authority
            (holding_id,requisition_id,aggregate_version,scope_key,company_id,branch_id,
             requisition_code,requesting_department_code,budget_code,currency,
             requisition_status,state,created_at,updated_at)
         VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
         ON CONFLICT (holding_id,requisition_id) DO UPDATE SET
            aggregate_version=EXCLUDED.aggregate_version,scope_key=EXCLUDED.scope_key,
            company_id=EXCLUDED.company_id,branch_id=EXCLUDED.branch_id,
            requisition_code=EXCLUDED.requisition_code,
            requesting_department_code=EXCLUDED.requesting_department_code,
            budget_code=EXCLUDED.budget_code,currency=EXCLUDED.currency,
            requisition_status=EXCLUDED.requisition_status,state=EXCLUDED.state,
            created_at=EXCLUDED.created_at,updated_at=EXCLUDED.updated_at
         WHERE erpx_procurement_write.purchase_requisition_authority.holding_id=
                   erpx_cqrs_write.current_holding_id()
           AND erpx_procurement_write.purchase_requisition_authority.aggregate_version+1=
                   EXCLUDED.aggregate_version",
    )
    .bind(value.id().as_uuid())
    .bind(to_i64(value.version())?)
    .bind(scope_key(value.scope())?)
    .bind(company_id)
    .bind(branch_id)
    .bind(value.requisition_code())
    .bind(value.requesting_department_code())
    .bind(budget_code)
    .bind(value.currency())
    .bind(requisition_status_name(value.status()))
    .bind(serde_json::to_value(value).map_err(CqrsSqlxError::Serialization)?)
    .bind(value.created_at())
    .bind(value.updated_at())
    .execute(&mut **transaction)
    .await?;
    if result.rows_affected() != 1 {
        return Err(CqrsSqlxError::GuardRejected(
            "requisition version did not advance exactly once",
        ));
    }
    sqlx::query(
        "DELETE FROM erpx_procurement_write.purchase_requisition_line_authority
         WHERE holding_id=erpx_cqrs_write.current_holding_id() AND requisition_id=$1",
    )
    .bind(value.id().as_uuid())
    .execute(&mut **transaction)
    .await?;
    for (position, line) in value.lines().iter().enumerate() {
        sqlx::query(
            "INSERT INTO erpx_procurement_write.purchase_requisition_line_authority
                (holding_id,requisition_id,line_id,line_position,item_code,requested_quantity,
                 estimated_unit_price_minor,currency,destination_code)
             VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(value.id().as_uuid())
        .bind(line.line_id.as_uuid())
        .bind(i32::try_from(position).map_err(|_| CqrsSqlxError::NumericRange)?)
        .bind(&line.item_code)
        .bind(line.requested_quantity)
        .bind(line.estimated_unit_price_minor)
        .bind(&line.currency)
        .bind(&line.destination_code)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

async fn persist_goods_receipt(
    transaction: &mut Transaction<'_, Postgres>,
    value: &GoodsReceipt,
) -> Result<(), CqrsSqlxError> {
    let (_, company_id, branch_id) = exact_scope_columns(value.scope())?;
    let result = sqlx::query(
        "INSERT INTO erpx_procurement_write.goods_receipt_authority
            (holding_id,goods_receipt_id,aggregate_version,scope_key,company_id,branch_id,
             receipt_code,source_purchase_order,receipt_status,state,created_at,updated_at)
         VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
         ON CONFLICT (holding_id,goods_receipt_id) DO UPDATE SET
            aggregate_version=EXCLUDED.aggregate_version,scope_key=EXCLUDED.scope_key,
            company_id=EXCLUDED.company_id,branch_id=EXCLUDED.branch_id,
            receipt_code=EXCLUDED.receipt_code,
            source_purchase_order=EXCLUDED.source_purchase_order,
            receipt_status=EXCLUDED.receipt_status,state=EXCLUDED.state,
            created_at=EXCLUDED.created_at,updated_at=EXCLUDED.updated_at
         WHERE erpx_procurement_write.goods_receipt_authority.holding_id=
                   erpx_cqrs_write.current_holding_id()
           AND erpx_procurement_write.goods_receipt_authority.aggregate_version+1=
                   EXCLUDED.aggregate_version",
    )
    .bind(value.id().as_uuid())
    .bind(to_i64(value.version())?)
    .bind(scope_key(value.scope())?)
    .bind(company_id)
    .bind(branch_id)
    .bind(value.receipt_code())
    .bind(
        serde_json::to_value(value.source_purchase_order())
            .map_err(CqrsSqlxError::Serialization)?,
    )
    .bind(goods_receipt_status_name(value.status()))
    .bind(serde_json::to_value(value).map_err(CqrsSqlxError::Serialization)?)
    .bind(value.created_at())
    .bind(value.updated_at())
    .execute(&mut **transaction)
    .await?;
    if result.rows_affected() != 1 {
        return Err(CqrsSqlxError::GuardRejected(
            "goods receipt version did not advance exactly once",
        ));
    }
    sqlx::query(
        "DELETE FROM erpx_procurement_write.goods_receipt_line_authority
         WHERE holding_id=erpx_cqrs_write.current_holding_id() AND goods_receipt_id=$1",
    )
    .bind(value.id().as_uuid())
    .execute(&mut **transaction)
    .await?;
    for (position, line) in value.lines().iter().enumerate() {
        sqlx::query(
            "INSERT INTO erpx_procurement_write.goods_receipt_line_authority
                (holding_id,goods_receipt_id,line_id,line_position,purchase_order_line_id,
                 inventory_stock_item_id,received_quantity)
             VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6)",
        )
        .bind(value.id().as_uuid())
        .bind(line.line_id.as_uuid())
        .bind(i32::try_from(position).map_err(|_| CqrsSqlxError::NumericRange)?)
        .bind(line.purchase_order_line_id.as_uuid())
        .bind(line.inventory_stock_item_id.as_uuid())
        .bind(line.received_quantity)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

async fn load_current_allowances(
    transaction: &mut Transaction<'_, Postgres>,
    purchase_order_id: Uuid,
    lock: bool,
) -> Result<Vec<ReceiptLineAllowance>, PortError> {
    if lock {
        sqlx::query(
            "SELECT line_id FROM erpx_procurement_write.purchase_order_line_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND purchase_order_id=$1
             ORDER BY line_id FOR UPDATE",
        )
        .bind(purchase_order_id)
        .fetch_all(&mut **transaction)
        .await
        .map_err(|_| PortError::Unavailable)?;
    }
    let statement =
        "SELECT line.line_id,line.ordered_quantity,
                COALESCE(SUM(receipt_line.received_quantity)
                    FILTER (WHERE receipt.receipt_status='accepted'
                        AND receipt.source_purchase_order #>> '{record,id}'=$1::uuid::text),0)::bigint
                    AS accepted_quantity
         FROM erpx_procurement_write.purchase_order_line_authority line
         LEFT JOIN erpx_procurement_write.goods_receipt_line_authority receipt_line
           ON receipt_line.holding_id=line.holding_id
          AND receipt_line.purchase_order_line_id=line.line_id
         LEFT JOIN erpx_procurement_write.goods_receipt_authority receipt
           ON receipt.holding_id=receipt_line.holding_id
          AND receipt.goods_receipt_id=receipt_line.goods_receipt_id
         WHERE line.holding_id=erpx_cqrs_write.current_holding_id()
           AND line.purchase_order_id=$1
         GROUP BY line.line_id,line.ordered_quantity ORDER BY line.line_id";
    sqlx::query(statement)
        .bind(purchase_order_id)
        .fetch_all(&mut **transaction)
        .await
        .map_err(|_| PortError::Unavailable)?
        .into_iter()
        .map(|row| {
            Ok(ReceiptLineAllowance {
                purchase_order_line_id: erpx_procurement::PurchaseOrderLineId::try_from_uuid(
                    row.try_get("line_id").map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                ordered_quantity: row
                    .try_get("ordered_quantity")
                    .map_err(|_| PortError::Internal)?,
                already_accepted_quantity: row
                    .try_get("accepted_quantity")
                    .map_err(|_| PortError::Internal)?,
            })
        })
        .collect()
}

fn decode_authorized<T: serde::de::DeserializeOwned>(
    row: Option<sqlx::postgres::PgRow>,
    authority_scope: &AuthorityScope,
    scope: impl Fn(&T) -> &AuthorityScope,
) -> Result<Option<T>, PortError> {
    let Some(row) = row else { return Ok(None) };
    let value: T = serde_json::from_value(row.try_get("state").map_err(|_| PortError::Internal)?)
        .map_err(|_| PortError::Internal)?;
    if !authority_scope.contains(scope(&value)) {
        return Err(PortError::Denied);
    }
    Ok(Some(value))
}

fn row_scope(row: &sqlx::postgres::PgRow) -> Result<AuthorityScope, PortError> {
    let holding_id = row.try_get("holding_id").map_err(|_| PortError::Internal)?;
    let company_id = row.try_get("company_id").map_err(|_| PortError::Internal)?;
    let branch_id: Option<Uuid> = row.try_get("branch_id").map_err(|_| PortError::Internal)?;
    Ok(branch_id.map_or(
        AuthorityScope::Company {
            holding_id,
            company_id,
        },
        |branch_id| AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        },
    ))
}

fn validate_commit(request: &ProcurementWorkflowCommit) -> Result<(), PortError> {
    let next = request
        .expected_version
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(PortError::Internal)?;
    let (partition, kind) = fingerprint_target(&request.identity.fingerprint);
    if request.aggregate.version() != next
        || !request.authority_scope.contains(request.aggregate.scope())
        || aggregate_partition(&request.aggregate) != partition
        || aggregate_type(&request.aggregate) != kind
        || request.domain_events.iter().any(|event| {
            !matches!(
                (event, &request.aggregate),
                (
                    ProcurementWorkflowDomainEvent::Requisition(_),
                    ProcurementWorkflowAggregate::Requisition(_),
                ) | (
                    ProcurementWorkflowDomainEvent::GoodsReceipt(_),
                    ProcurementWorkflowAggregate::GoodsReceipt(_),
                )
            )
        })
    {
        return Err(PortError::Internal);
    }
    Ok(())
}

fn fingerprint_target(
    fingerprint: &ProcurementWorkflowCommandFingerprint,
) -> (String, &'static str) {
    match fingerprint {
        ProcurementWorkflowCommandFingerprint::CreateRequisition { requisition_id, .. }
        | ProcurementWorkflowCommandFingerprint::SubmitRequisition { requisition_id }
        | ProcurementWorkflowCommandFingerprint::ApproveRequisition { requisition_id }
        | ProcurementWorkflowCommandFingerprint::RejectRequisition { requisition_id }
        | ProcurementWorkflowCommandFingerprint::CancelRequisition { requisition_id } => (
            requisition_partition(*requisition_id),
            REQUISITION_AGGREGATE_TYPE,
        ),
        ProcurementWorkflowCommandFingerprint::CreateGoodsReceipt {
            goods_receipt_id, ..
        }
        | ProcurementWorkflowCommandFingerprint::AcceptGoodsReceipt { goods_receipt_id }
        | ProcurementWorkflowCommandFingerprint::RejectGoodsReceipt { goods_receipt_id }
        | ProcurementWorkflowCommandFingerprint::CancelGoodsReceipt { goods_receipt_id } => (
            goods_receipt_partition(*goods_receipt_id),
            GOODS_RECEIPT_AGGREGATE_TYPE,
        ),
    }
}

fn aggregate_partition(aggregate: &ProcurementWorkflowAggregate) -> String {
    match aggregate {
        ProcurementWorkflowAggregate::Requisition(value) => requisition_partition(value.id()),
        ProcurementWorkflowAggregate::GoodsReceipt(value) => goods_receipt_partition(value.id()),
    }
}

const fn aggregate_type(aggregate: &ProcurementWorkflowAggregate) -> &'static str {
    match aggregate {
        ProcurementWorkflowAggregate::Requisition(_) => REQUISITION_AGGREGATE_TYPE,
        ProcurementWorkflowAggregate::GoodsReceipt(_) => GOODS_RECEIPT_AGGREGATE_TYPE,
    }
}

fn aggregate_uuid(aggregate: &ProcurementWorkflowAggregate) -> Uuid {
    match aggregate {
        ProcurementWorkflowAggregate::Requisition(value) => value.id().as_uuid(),
        ProcurementWorkflowAggregate::GoodsReceipt(value) => value.id().as_uuid(),
    }
}

fn aggregate_updated_at(aggregate: &ProcurementWorkflowAggregate) -> chrono::DateTime<chrono::Utc> {
    match aggregate {
        ProcurementWorkflowAggregate::Requisition(value) => value.updated_at(),
        ProcurementWorkflowAggregate::GoodsReceipt(value) => value.updated_at(),
    }
}

fn snapshot_event_type(aggregate: &ProcurementWorkflowAggregate) -> &'static str {
    match aggregate {
        ProcurementWorkflowAggregate::Requisition(_) => REQUISITION_SNAPSHOT_EVENT_TYPE,
        ProcurementWorkflowAggregate::GoodsReceipt(_) => GOODS_RECEIPT_SNAPSHOT_EVENT_TYPE,
    }
}

fn workflow_snapshot(aggregate: &ProcurementWorkflowAggregate) -> ProcurementWorkflowSnapshot {
    match aggregate {
        ProcurementWorkflowAggregate::Requisition(value) => {
            ProcurementWorkflowSnapshot::Requisition(value.into())
        }
        ProcurementWorkflowAggregate::GoodsReceipt(value) => {
            ProcurementWorkflowSnapshot::GoodsReceipt(value.into())
        }
    }
}

fn directory_draft(
    aggregate: &ProcurementWorkflowAggregate,
) -> Result<ItemDescriptorDraftV2, PortError> {
    let (id, record_type, label, lifecycle, scope, version) = match aggregate {
        ProcurementWorkflowAggregate::Requisition(value) => (
            value.id().as_uuid(),
            "procurement.requisition",
            value.requisition_code(),
            requisition_status_name(value.status()),
            value.scope(),
            value.version(),
        ),
        ProcurementWorkflowAggregate::GoodsReceipt(value) => (
            value.id().as_uuid(),
            "procurement.receipt",
            value.receipt_code(),
            goods_receipt_status_name(value.status()),
            value.scope(),
            value.version(),
        ),
    };
    let record = StableRecordRef::try_new(
        RecordId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
        ContextId::Procurement,
        RecordTypeId::try_new(record_type).map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    let draft = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
        record,
        scope: scope.clone(),
        lifecycle_state: lifecycle.into(),
        classification: DataClassification::Confidential,
        descriptor_version: version,
        authorized_search_text: label.into(),
    };
    ItemDescriptorDraftV2::try_new(draft, label).map_err(|_| PortError::Internal)
}

fn integration_events(
    events: &[ProcurementWorkflowDomainEvent],
) -> Result<Vec<procurement::Event>, PortError> {
    events.iter().filter_map(map_integration_event).collect()
}

fn map_integration_event(
    event: &ProcurementWorkflowDomainEvent,
) -> Option<Result<procurement::Event, PortError>> {
    match event {
        ProcurementWorkflowDomainEvent::Requisition(PurchaseRequisitionDomainEvent::Approved {
            state,
            at,
        }) => Some((|| {
            let requisition = version_ref(
                state.requisition_id.as_uuid(),
                "procurement.requisition",
                state.approved_version,
            )?;
            let lines = state
                .lines
                .iter()
                .map(|line| {
                    let currency =
                        CurrencyCode::try_new(&line.currency).map_err(|_| PortError::Internal)?;
                    Ok(procurement::ApprovedPurchaseRequisitionLineFact {
                        line: procurement::PurchaseRequisitionLineRef::try_from_uuid(
                            line.line_id.as_uuid(),
                        )
                        .map_err(|_| PortError::Internal)?,
                        item_code: ReferenceCode::try_new(line.item_code.clone())
                            .map_err(|_| PortError::Internal)?,
                        approved_quantity: Quantity::try_new(line.requested_quantity, 0)
                            .map_err(|_| PortError::Internal)?,
                        approved_line_value: MoneyAmount::new(
                            line.requested_quantity
                                .checked_mul(line.estimated_unit_price_minor)
                                .ok_or(PortError::Internal)?,
                            currency,
                        ),
                    })
                })
                .collect::<Result<Vec<_>, PortError>>()?;
            let fact = procurement::ApprovedPurchaseRequisitionFact {
                requisition,
                requisition_code: ReferenceCode::try_new(state.requisition_code.clone())
                    .map_err(|_| PortError::Internal)?,
                department_code: ReferenceCode::try_new(state.requesting_department_code.clone())
                    .map_err(|_| PortError::Internal)?,
                budget_reference: ReferenceCode::try_new(
                    state.budget_code.clone().ok_or(PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                lines,
                approved_at: *at,
            };
            let event = procurement::Event::PurchaseRequisitionApproved(fact);
            event.validate().map_err(|_| PortError::Internal)?;
            Ok(event)
        })()),
        ProcurementWorkflowDomainEvent::GoodsReceipt(GoodsReceiptDomainEvent::Accepted {
            state,
            at,
        }) => Some((|| {
            let receipt = version_ref(
                state.goods_receipt_id.as_uuid(),
                "procurement.receipt",
                state.accepted_version,
            )?;
            let lines = state
                .lines
                .iter()
                .map(|line| {
                    Ok(procurement::AcceptedGoodsReceiptLineFact {
                        line: procurement::GoodsReceiptLineRef::try_from_uuid(
                            line.goods_receipt_line_id.as_uuid(),
                        )
                        .map_err(|_| PortError::Internal)?,
                        purchase_order_line: procurement::PurchaseOrderLineRef::try_from_uuid(
                            line.purchase_order_line_id.as_uuid(),
                        )
                        .map_err(|_| PortError::Internal)?,
                        stock_item: inventory::StockItemRef::try_from_uuid(
                            line.inventory_stock_item_id.as_uuid(),
                        )
                        .map_err(|_| PortError::Internal)?,
                        accepted_quantity: Quantity::try_new(line.accepted_quantity, 0)
                            .map_err(|_| PortError::Internal)?,
                    })
                })
                .collect::<Result<Vec<_>, PortError>>()?;
            let fact = procurement::AcceptedGoodsReceiptFact {
                receipt,
                purchase_order: state.source_purchase_order.clone(),
                lines,
                accepted_at: *at,
            };
            let event = procurement::Event::GoodsReceiptAccepted(fact);
            event.validate().map_err(|_| PortError::Internal)?;
            Ok(event)
        })()),
        ProcurementWorkflowDomainEvent::Requisition(_)
        | ProcurementWorkflowDomainEvent::GoodsReceipt(_) => None,
    }
}

fn version_ref(id: Uuid, record_type: &str, version: u64) -> Result<RecordVersionRef, PortError> {
    let record = StableRecordRef::try_new(
        RecordId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
        ContextId::Procurement,
        RecordTypeId::try_new(record_type).map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    RecordVersionRef::try_new(record, version).map_err(|_| PortError::Internal)
}

fn guard_context(
    aggregate: &ProcurementWorkflowAggregate,
    context: &GuardCommandContext<'_>,
) -> Result<(), CqrsSqlxError> {
    if context.context != CONTEXT_NAME
        || context.aggregate_type != aggregate_type(aggregate)
        || context.aggregate_id.as_uuid() != aggregate_uuid(aggregate)
        || context.partition != aggregate_partition(aggregate)
    {
        return Err(CqrsSqlxError::GuardRejected(
            "procurement workflow guard context mismatch",
        ));
    }
    Ok(())
}

fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<ProcurementWorkflowAggregate>,
) -> Result<StoredProcurementWorkflowCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != aggregate_type(&receipt.response)
        || receipt.aggregate.aggregate_id.as_uuid() != aggregate_uuid(&receipt.response)
        || receipt.aggregate.version != receipt.response.version()
    {
        return Err(PortError::Internal);
    }
    Ok(StoredProcurementWorkflowCommit {
        command_id: receipt.command_id,
        correlation_id: receipt.correlation_id,
        aggregate: receipt.response,
        committed_events: receipt.committed_events,
        committed_at: receipt.committed_at,
        replayed: receipt.replayed,
    })
}

fn request_hash(
    fingerprint: &ProcurementWorkflowCommandFingerprint,
) -> Result<RequestHash, PortError> {
    let mut digest = Sha256::new();
    digest.update(b"erpx.procurement.workflow-command-fingerprint.v1\0");
    digest.update(serde_json::to_vec(fingerprint).map_err(|_| PortError::Internal)?);
    Ok(RequestHash::new(digest.finalize().into()))
}

fn audit_action(fingerprint: &ProcurementWorkflowCommandFingerprint) -> &'static str {
    match fingerprint {
        ProcurementWorkflowCommandFingerprint::CreateRequisition { .. } => {
            "procurement.requisition.create"
        }
        ProcurementWorkflowCommandFingerprint::SubmitRequisition { .. } => {
            "procurement.requisition.submit"
        }
        ProcurementWorkflowCommandFingerprint::ApproveRequisition { .. } => {
            "procurement.requisition.approve"
        }
        ProcurementWorkflowCommandFingerprint::RejectRequisition { .. } => {
            "procurement.requisition.reject"
        }
        ProcurementWorkflowCommandFingerprint::CancelRequisition { .. } => {
            "procurement.requisition.cancel"
        }
        ProcurementWorkflowCommandFingerprint::CreateGoodsReceipt { .. } => {
            "procurement.goods_receipt.create"
        }
        ProcurementWorkflowCommandFingerprint::AcceptGoodsReceipt { .. } => {
            "procurement.goods_receipt.accept"
        }
        ProcurementWorkflowCommandFingerprint::RejectGoodsReceipt { .. } => {
            "procurement.goods_receipt.reject"
        }
        ProcurementWorkflowCommandFingerprint::CancelGoodsReceipt { .. } => {
            "procurement.goods_receipt.cancel"
        }
    }
}

fn deterministic_event_id(command: CommandId, event_type: &str, index: usize) -> EventId {
    let mut label = event_type.as_bytes().to_vec();
    label.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(command.as_uuid(), &label)).unwrap()
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
        _ => Err(CqrsSqlxError::GuardRejected(
            "procurement workflow scope is not exact",
        )),
    }
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
        _ => Err(CqrsSqlxError::GuardRejected(
            "procurement workflow scope is not exact",
        )),
    }
}

fn to_i64(value: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
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
    use erpx_procurement::{ProcurementWorkflowCommandFingerprint, PurchaseRequisitionId};

    use super::*;

    #[test]
    fn workflow_hash_and_target_are_stable() {
        let id = PurchaseRequisitionId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let submit =
            ProcurementWorkflowCommandFingerprint::SubmitRequisition { requisition_id: id };
        let approve =
            ProcurementWorkflowCommandFingerprint::ApproveRequisition { requisition_id: id };
        assert_eq!(
            request_hash(&submit).unwrap(),
            request_hash(&submit).unwrap()
        );
        assert_ne!(
            request_hash(&submit).unwrap(),
            request_hash(&approve).unwrap()
        );
        assert_eq!(
            fingerprint_target(&submit),
            (
                "procurement.requisition:00000000000000000000000000000001".into(),
                REQUISITION_AGGREGATE_TYPE,
            )
        );
    }

    #[test]
    fn workflow_source_guards_are_visible_in_adapter_source() {
        let source = include_str!("workflow.rs");
        for invariant in [
            "FOR UPDATE",
            "receipt allowance snapshot is stale",
            "receipt exceeds the current purchase-order line remainder",
            "goods receipt inventory item scope changed",
            "ProcurementWorkflowOutboxPayload::Integration",
            "ProcurementWorkflowOutboxPayload::Directory",
        ] {
            assert!(source.contains(invariant), "missing invariant {invariant}");
        }
    }
}
