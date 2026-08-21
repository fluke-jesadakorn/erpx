use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, IntegrationEvent,
    ItemDescriptorDraft, ItemDescriptorDraftV2, RecordTypeId, RecordVersionRef, StableRecordRef,
    catalog::ContextId, record_policy_id,
};
use erpx_kernel::{AggregateId, AuthorityScope, CommandId, DataClassification, EventId, RecordId};
use erpx_procurement::{
    CommandFingerprint, CommandIdentity, CommandUnitOfWork, ExactRecordSourceSnapshot, PortError,
    ProcurementCommit, ProcurementSourceGuard, PurchaseOrder, PurchaseOrderId, PurchaseOrderLine,
    PurchaseOrderLineId, PurchaseOrderSourcePort, PurchaseOrderState, PurchaseOrderStatus,
    PurchaseRequisition, StoredCommit, SupplierId, WriteRepository,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, EVENT_VERSION, SNAPSHOT_EVENT_TYPE,
    projection::{ProcurementOutboxPayload, ProcurementSnapshot, status_name},
    purchase_order_partition,
};

#[derive(Debug, Clone)]
pub struct ProcurementWriteRepository {
    pub(crate) pool: WritePool,
}

#[async_trait]
impl PurchaseOrderSourcePort for ProcurementWriteRepository {
    async fn load_approved_requisition_source(
        &self,
        authority_scope: &AuthorityScope,
        source: &RecordVersionRef,
    ) -> Result<Option<PurchaseRequisition>, PortError> {
        if source.record().record_type().as_str() != "procurement.requisition" {
            return Ok(None);
        }
        let mut transaction = self.pool.begin().await.map_err(map_store_error)?;
        let row = sqlx::query(
            "SELECT aggregate_version,state FROM erpx_procurement_write.purchase_requisition_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND requisition_id=$1
               AND aggregate_version=$2 AND requisition_status='approved'",
        )
        .bind(source.record().id().as_uuid())
        .bind(to_i64(source.version()).map_err(|_| PortError::Internal)?)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| PortError::Unavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)?;
        row.map(|row| {
            let aggregate: PurchaseRequisition =
                serde_json::from_value(row.try_get("state").map_err(|_| PortError::Internal)?)
                    .map_err(|_| PortError::Internal)?;
            aggregate.validate().map_err(|_| PortError::Internal)?;
            if !authority_scope.contains(aggregate.scope()) {
                return Err(PortError::Denied);
            }
            Ok(aggregate)
        })
        .transpose()
    }

    async fn resolve_contract_source(
        &self,
        authority_scope: &AuthorityScope,
        source: &RecordVersionRef,
    ) -> Result<Option<ExactRecordSourceSnapshot>, PortError> {
        if source.record().record_type().as_str() != "legal.contract" {
            return Ok(None);
        }
        let mut transaction = self.pool.begin().await.map_err(map_store_error)?;
        let found: Option<String> = sqlx::query_scalar(
            "SELECT scope_key FROM erpx_legal_write.contract_guards
             WHERE contract_id=$1 AND aggregate_version=$2
               AND contract_status IN ('approved','active')",
        )
        .bind(source.record().id().as_uuid())
        .bind(to_i64(source.version()).map_err(|_| PortError::Internal)?)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| PortError::Unavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)?;
        found
            .map(|scope| {
                let scope = parse_scope_key(&scope)?;
                if !authority_scope.contains(&scope) {
                    return Err(PortError::Denied);
                }
                Ok(ExactRecordSourceSnapshot {
                    source: source.clone(),
                    scope,
                })
            })
            .transpose()
    }
}

impl ProcurementWriteRepository {
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
impl WriteRepository for ProcurementWriteRepository {
    async fn load(
        &self,
        authority_scope: &AuthorityScope,
        purchase_order_id: PurchaseOrderId,
    ) -> Result<Option<PurchaseOrder>, PortError> {
        let authority = AuthoritySqlScope::try_from_scope(authority_scope)?;
        let mut transaction = self.pool.begin().await.map_err(map_store_error)?;
        let row = sqlx::query(
            "SELECT aggregate_version,holding_id,company_id,branch_id,order_code,supplier_id,
                    currency,order_status,source_requisition,contract_reference,created_at,updated_at
             FROM erpx_procurement_write.purchase_order_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND purchase_order_id=$1
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
        .bind(purchase_order_id.as_uuid())
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
        let line_rows = sqlx::query(
            "SELECT line_id,item_code,ordered_quantity,unit_price_minor,currency,destination_code
             FROM erpx_procurement_write.purchase_order_line_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND purchase_order_id=$1
             ORDER BY line_position",
        )
        .bind(purchase_order_id.as_uuid())
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
        let lines = line_rows
            .into_iter()
            .map(|line| {
                Ok(PurchaseOrderLine {
                    line_id: PurchaseOrderLineId::try_from_uuid(
                        line.try_get("line_id").map_err(|_| PortError::Internal)?,
                    )
                    .map_err(|_| PortError::Internal)?,
                    item_code: line.try_get("item_code").map_err(|_| PortError::Internal)?,
                    ordered_quantity: line
                        .try_get("ordered_quantity")
                        .map_err(|_| PortError::Internal)?,
                    unit_price_minor: line
                        .try_get("unit_price_minor")
                        .map_err(|_| PortError::Internal)?,
                    currency: line.try_get("currency").map_err(|_| PortError::Internal)?,
                    destination_code: line
                        .try_get("destination_code")
                        .map_err(|_| PortError::Internal)?,
                })
            })
            .collect::<Result<Vec<_>, PortError>>()?;
        let aggregate = PurchaseOrder::try_rehydrate(PurchaseOrderState {
            id: purchase_order_id,
            version: from_i64(
                row.try_get("aggregate_version")
                    .map_err(|_| PortError::Internal)?,
            )?,
            scope,
            order_code: row.try_get("order_code").map_err(|_| PortError::Internal)?,
            supplier_id: SupplierId::try_from_uuid(
                row.try_get("supplier_id")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            currency: row.try_get("currency").map_err(|_| PortError::Internal)?,
            lines,
            source_requisition: row
                .try_get::<Option<serde_json::Value>, _>("source_requisition")
                .map_err(|_| PortError::Internal)?
                .map(serde_json::from_value::<RecordVersionRef>)
                .transpose()
                .map_err(|_| PortError::Internal)?,
            contract: row
                .try_get::<Option<serde_json::Value>, _>("contract_reference")
                .map_err(|_| PortError::Internal)?
                .map(serde_json::from_value::<RecordVersionRef>)
                .transpose()
                .map_err(|_| PortError::Internal)?,
            status: parse_status(
                &row.try_get::<String, _>("order_status")
                    .map_err(|_| PortError::Internal)?,
            )?,
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
        let directory_draft = procurement_directory_draft(&request.aggregate)?;
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
            payload: ProcurementOutboxPayload::Directory(directory_draft),
        });
        let guard = ProcurementGuard {
            aggregate: request.aggregate.clone(),
            source_guards: request.source_guards.clone(),
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

fn procurement_directory_draft(
    aggregate: &PurchaseOrder,
) -> Result<ItemDescriptorDraftV2, PortError> {
    let record_id =
        RecordId::try_from_uuid(aggregate.id().as_uuid()).map_err(|_| PortError::Internal)?;
    let record = StableRecordRef::try_new(
        record_id,
        ContextId::Procurement,
        RecordTypeId::try_new("procurement.purchase-order").map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    let value = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
        record,
        scope: aggregate.scope().clone(),
        lifecycle_state: status_name(aggregate.status()).into(),
        classification: DataClassification::Confidential,
        descriptor_version: aggregate.version(),
        authorized_search_text: aggregate.order_code().to_owned(),
    };
    ItemDescriptorDraftV2::try_new(value, aggregate.order_code()).map_err(|_| PortError::Internal)
}

struct ProcurementGuard {
    aggregate: PurchaseOrder,
    source_guards: Vec<ProcurementSourceGuard>,
}

#[async_trait]
impl AtomicCommandGuard for ProcurementGuard {
    async fn authoritative_version(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        guard_context(&self.aggregate, context)?;
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT aggregate_version FROM erpx_procurement_write.purchase_order_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND purchase_order_id=$1
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
        validate_source_guards(transaction, &self.source_guards).await?;
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT aggregate_version FROM erpx_procurement_write.purchase_order_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND purchase_order_id=$1
             FOR UPDATE",
        )
        .bind(self.aggregate.id().as_uuid())
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
            "SELECT purchase_order_id FROM erpx_procurement_write.purchase_order_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id()
               AND scope_key=$1 AND order_code=$2 AND purchase_order_id<>$3
             ORDER BY purchase_order_id LIMIT 1 FOR UPDATE",
        )
        .bind(scope_key(self.aggregate.scope())?)
        .bind(self.aggregate.order_code())
        .bind(self.aggregate.id().as_uuid())
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
            || context.aggregate_id.as_uuid() != self.aggregate.id().as_uuid()
            || context.partition != purchase_order_partition(self.aggregate.id())
            || context.aggregate_version != self.aggregate.version()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "procurement applied guard mismatch",
            ));
        }
        validate_source_guards(transaction, &self.source_guards).await?;
        let (_, company_id, branch_id) = exact_scope_columns(self.aggregate.scope())?;
        let result = sqlx::query(
            "INSERT INTO erpx_procurement_write.purchase_order_authority
                (holding_id,purchase_order_id,aggregate_version,scope_key,company_id,
                 branch_id,order_code,supplier_id,currency,order_status,source_requisition,
                 contract_reference,created_at,updated_at)
             VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
             ON CONFLICT (holding_id,purchase_order_id) DO UPDATE SET
                aggregate_version=EXCLUDED.aggregate_version,
                scope_key=EXCLUDED.scope_key,order_code=EXCLUDED.order_code,
                company_id=EXCLUDED.company_id,
                branch_id=EXCLUDED.branch_id,
                supplier_id=EXCLUDED.supplier_id,currency=EXCLUDED.currency,
                order_status=EXCLUDED.order_status,
                source_requisition=EXCLUDED.source_requisition,
                contract_reference=EXCLUDED.contract_reference,
                created_at=EXCLUDED.created_at,updated_at=EXCLUDED.updated_at
             WHERE erpx_procurement_write.purchase_order_authority.holding_id=
                       erpx_cqrs_write.current_holding_id()
               AND erpx_procurement_write.purchase_order_authority.aggregate_version + 1=
                       EXCLUDED.aggregate_version",
        )
        .bind(self.aggregate.id().as_uuid())
        .bind(to_i64(self.aggregate.version())?)
        .bind(scope_key(self.aggregate.scope())?)
        .bind(company_id)
        .bind(branch_id)
        .bind(self.aggregate.order_code())
        .bind(self.aggregate.supplier_id().as_uuid())
        .bind(self.aggregate.currency())
        .bind(status_name(self.aggregate.status()))
        .bind(
            self.aggregate
                .source_requisition()
                .map(serde_json::to_value)
                .transpose()
                .map_err(CqrsSqlxError::Serialization)?,
        )
        .bind(
            self.aggregate
                .contract()
                .map(serde_json::to_value)
                .transpose()
                .map_err(CqrsSqlxError::Serialization)?,
        )
        .bind(self.aggregate.created_at())
        .bind(self.aggregate.updated_at())
        .execute(&mut **transaction)
        .await?;
        if result.rows_affected() != 1 {
            return Err(CqrsSqlxError::GuardRejected(
                "procurement normalized version did not advance exactly once",
            ));
        }
        sqlx::query(
            "DELETE FROM erpx_procurement_write.purchase_order_line_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND purchase_order_id=$1",
        )
        .bind(self.aggregate.id().as_uuid())
        .execute(&mut **transaction)
        .await?;
        for (position, line) in self.aggregate.lines().iter().enumerate() {
            sqlx::query(
                "INSERT INTO erpx_procurement_write.purchase_order_line_authority
                    (holding_id,purchase_order_id,line_id,line_position,item_code,ordered_quantity,
                     unit_price_minor,currency,destination_code)
                 VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8)",
            )
            .bind(self.aggregate.id().as_uuid())
            .bind(line.line_id.as_uuid())
            .bind(i32::try_from(position).map_err(|_| CqrsSqlxError::NumericRange)?)
            .bind(&line.item_code)
            .bind(line.ordered_quantity)
            .bind(line.unit_price_minor)
            .bind(&line.currency)
            .bind(&line.destination_code)
            .execute(&mut **transaction)
            .await?;
        }
        Ok(())
    }
}

async fn validate_source_guards(
    transaction: &mut Transaction<'_, Postgres>,
    guards: &[ProcurementSourceGuard],
) -> Result<(), CqrsSqlxError> {
    if guards.is_empty() {
        return Err(CqrsSqlxError::GuardRejected(
            "procurement source evidence is missing",
        ));
    }
    for guard in guards {
        let ProcurementSourceGuard::ExactRecord(snapshot) = guard else {
            return Err(CqrsSqlxError::GuardRejected(
                "purchase order received an unrelated source guard",
            ));
        };
        let scope = scope_key(&snapshot.scope)?;
        let source = &snapshot.source;
        let found = match source.record().record_type().as_str() {
            "procurement.requisition" => {
                sqlx::query_scalar::<_, i64>(
                    "SELECT aggregate_version
                     FROM erpx_procurement_write.purchase_requisition_authority
                     WHERE holding_id=erpx_cqrs_write.current_holding_id()
                       AND requisition_id=$1 AND aggregate_version=$2 AND scope_key=$3
                       AND requisition_status='approved' FOR UPDATE",
                )
                .bind(source.record().id().as_uuid())
                .bind(to_i64(source.version())?)
                .bind(scope)
                .fetch_optional(&mut **transaction)
                .await?
            }
            "legal.contract" => {
                sqlx::query_scalar::<_, i64>(
                    "SELECT aggregate_version FROM erpx_legal_write.contract_guards
                     WHERE contract_id=$1 AND aggregate_version=$2 AND scope_key=$3
                       AND contract_status IN ('approved','active') FOR UPDATE",
                )
                .bind(source.record().id().as_uuid())
                .bind(to_i64(source.version())?)
                .bind(scope)
                .fetch_optional(&mut **transaction)
                .await?
            }
            _ => {
                return Err(CqrsSqlxError::GuardRejected(
                    "procurement source type is not owner-approved",
                ));
            }
        };
        if found != Some(to_i64(source.version())?) {
            return Err(CqrsSqlxError::GuardRejected(
                "procurement source version or scope changed",
            ));
        }
    }
    Ok(())
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
    let next = request
        .expected_version
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(PortError::Internal)?;
    if request.aggregate.validate().is_err()
        || fingerprint_id(&request.identity.fingerprint) != request.aggregate.id()
        || !request.authority_scope.contains(request.aggregate.scope())
        || request.aggregate.version() != next
        || request.source_guards.is_empty()
        || request.source_guards.iter().any(|guard| match guard {
            ProcurementSourceGuard::ExactRecord(snapshot) => {
                snapshot.scope != *request.aggregate.scope()
                    || !request
                        .aggregate
                        .source_requisition()
                        .into_iter()
                        .chain(request.aggregate.contract())
                        .any(|source| source == &snapshot.source)
            }
            ProcurementSourceGuard::ReceiptQuantities { .. } => true,
        })
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
            "procurement scope is not exact",
        )),
    }
}

fn parse_scope_key(value: &str) -> Result<AuthorityScope, PortError> {
    let parts = value.split(':').collect::<Vec<_>>();
    let parse = |value: &str| {
        Uuid::parse_str(value)
            .ok()
            .filter(|value| !value.is_nil())
            .ok_or(PortError::Internal)
    };
    match parts.as_slice() {
        ["company", holding, company] => Ok(AuthorityScope::Company {
            holding_id: parse(holding)?,
            company_id: parse(company)?,
        }),
        ["branch", holding, company, branch] => Ok(AuthorityScope::Branch {
            holding_id: parse(holding)?,
            company_id: parse(company)?,
            branch_id: parse(branch)?,
        }),
        _ => Err(PortError::Internal),
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
        AuthorityScope::Platform
        | AuthorityScope::Holding { .. }
        | AuthorityScope::Subtree { .. } => Err(CqrsSqlxError::GuardRejected(
            "procurement authority requires company or branch scope",
        )),
    }
}

fn parse_status(value: &str) -> Result<PurchaseOrderStatus, PortError> {
    match value {
        "draft" => Ok(PurchaseOrderStatus::Draft),
        "released" => Ok(PurchaseOrderStatus::Released),
        "cancelled" => Ok(PurchaseOrderStatus::Cancelled),
        _ => Err(PortError::Internal),
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
