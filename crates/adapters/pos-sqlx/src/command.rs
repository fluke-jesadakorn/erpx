use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, IntegrationEvent,
    ItemDescriptorDraft, RecordTypeId, StableRecordRef, catalog::ContextId, record_policy_id,
};
use erpx_kernel::{AggregateId, CommandId, DataClassification, EventId, RecordId, TenantScope};
use erpx_pos::{
    CommandFingerprint, CommandIdentity, CustomerId, Payment, PaymentId, PaymentMethod, PortError,
    PosRefund, RefundCommit, RefundId, RefundLine, RefundStatus, RefundStoredCommit,
    RefundUnitOfWork, RegisterId, Sale, SaleCommit, SaleId, SaleLine, SaleLineId, SaleStatus,
    SaleStoredCommit, SaleUnitOfWork, WriteRepository,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Decode, Postgres, Row, Transaction, Type, postgres::PgRow};
use uuid::Uuid;

use crate::{
    CONTEXT_NAME, EVENT_VERSION, REFUND_AGGREGATE_TYPE, REFUND_SNAPSHOT_EVENT_TYPE,
    SALE_AGGREGATE_TYPE, SALE_SNAPSHOT_EVENT_TYPE,
    projection::{
        PosOutboxPayload, PosRefundSnapshot, PosSaleSnapshot, payment_method_name,
        refund_status_name, sale_status_name, scope_key,
    },
    refund_partition, sale_partition,
};

#[derive(Debug, Clone)]
pub struct PosWriteRepository {
    pool: WritePool,
}

impl PosWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl WriteRepository for PosWriteRepository {
    async fn load_sale(
        &self,
        authority_scope: &TenantScope,
        sale_id: SaleId,
    ) -> Result<Option<Sale>, PortError> {
        load_sale_relational(&self.pool, authority_scope, sale_id).await
    }

    async fn load_refund(
        &self,
        authority_scope: &TenantScope,
        refund_id: RefundId,
    ) -> Result<Option<PosRefund>, PortError> {
        load_refund_relational(&self.pool, authority_scope, refund_id).await
    }
}

async fn load_sale_relational(
    pool: &WritePool,
    authority: &TenantScope,
    sale_id: SaleId,
) -> Result<Option<Sale>, PortError> {
    authority.validate().map_err(|_| PortError::Denied)?;
    let mut tx = pool.begin().await.map_err(map_store_error)?;
    load_sale_in_tx(&mut tx, authority, sale_id).await
}

async fn load_sale_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    authority: &TenantScope,
    sale_id: SaleId,
) -> Result<Option<Sale>, PortError> {
    let (kind, holding_id, company_id, branch_id, company_ids, branch_ids) =
        authority_predicate(authority)?;
    let row = sqlx::query(
        "SELECT aggregate_version,holding_id,company_id,branch_id,receipt_code,register_id,
                customer_id,currency,sale_status,created_at,updated_at
         FROM erpx_pos_write.sale_guards
         WHERE sale_id=$1 AND (
           $2='platform'
           OR ($2='holding' AND holding_id=$3)
           OR ($2='company' AND holding_id=$3 AND company_id=$4)
           OR ($2='branch' AND holding_id=$3 AND company_id=$4 AND branch_id=$5)
           OR ($2='subtree' AND holding_id=$3 AND (
                (branch_id IS NULL AND cardinality($7::uuid[])=0 AND company_id=ANY($6::uuid[]))
                OR (branch_id IS NOT NULL AND company_id=ANY($6::uuid[]) AND branch_id=ANY($7::uuid[]))
           ))
         )",
    )
    .bind(sale_id.as_uuid())
    .bind(kind)
    .bind(holding_id)
    .bind(company_id)
    .bind(branch_id)
    .bind(company_ids)
    .bind(branch_ids)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| PortError::Unavailable)?;
    let Some(row) = row else {
        return missing_or_denied(
            tx,
            "erpx_pos_write.sale_guards",
            "sale_id",
            sale_id.as_uuid(),
        )
        .await;
    };
    let line_rows = sqlx::query(
        "SELECT line_id,item_reference,description,quantity,unit_price_minor,tax_minor
         FROM erpx_pos_write.sale_line_guards WHERE sale_id=$1 ORDER BY line_id",
    )
    .bind(sale_id.as_uuid())
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| PortError::Unavailable)?;
    let payment_rows = sqlx::query(
        "SELECT payment_id,amount_minor,payment_method,received_at
         FROM erpx_pos_write.payment_guards WHERE sale_id=$1 ORDER BY received_at,payment_id",
    )
    .bind(sale_id.as_uuid())
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| PortError::Unavailable)?;
    let lines = line_rows
        .into_iter()
        .map(|line| {
            Ok(SaleLine {
                line_id: SaleLineId::try_from_uuid(port_uuid(&line, "line_id")?)
                    .map_err(|_| PortError::Internal)?,
                item_reference: port_value(&line, "item_reference")?,
                description: port_value(&line, "description")?,
                quantity: port_u64(port_value(&line, "quantity")?)?,
                unit_price_minor: port_value(&line, "unit_price_minor")?,
                tax_minor: port_value(&line, "tax_minor")?,
            })
        })
        .collect::<Result<Vec<_>, PortError>>()?;
    let payments = payment_rows
        .into_iter()
        .map(|payment| {
            Ok(Payment {
                payment_id: PaymentId::try_from_uuid(port_uuid(&payment, "payment_id")?)
                    .map_err(|_| PortError::Internal)?,
                amount_minor: port_value(&payment, "amount_minor")?,
                method: parse_payment_method(&port_value::<String>(&payment, "payment_method")?)?,
                received_at: port_value(&payment, "received_at")?,
            })
        })
        .collect::<Result<Vec<_>, PortError>>()?;
    let scope = exact_scope_from_row(&row)?;
    Sale::try_rehydrate(
        sale_id,
        port_u64(port_value(&row, "aggregate_version")?)?,
        scope,
        port_value(&row, "receipt_code")?,
        RegisterId::try_from_uuid(port_uuid(&row, "register_id")?)
            .map_err(|_| PortError::Internal)?,
        port_value::<Option<Uuid>>(&row, "customer_id")?
            .map(CustomerId::try_from_uuid)
            .transpose()
            .map_err(|_| PortError::Internal)?,
        port_value(&row, "currency")?,
        lines,
        payments,
        parse_sale_status(&port_value::<String>(&row, "sale_status")?)?,
        port_value(&row, "created_at")?,
        port_value(&row, "updated_at")?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn load_refund_relational(
    pool: &WritePool,
    authority: &TenantScope,
    refund_id: RefundId,
) -> Result<Option<PosRefund>, PortError> {
    authority.validate().map_err(|_| PortError::Denied)?;
    let mut tx = pool.begin().await.map_err(map_store_error)?;
    let (kind, holding_id, company_id, branch_id, company_ids, branch_ids) =
        authority_predicate(authority)?;
    let row = sqlx::query(
        "SELECT aggregate_version,holding_id,company_id,branch_id,sale_id,reason,
                requester_principal_ref,reviewer_principal_ref,refund_status,requested_at,updated_at
         FROM erpx_pos_write.refund_guards
         WHERE refund_id=$1 AND (
           $2='platform' OR ($2='holding' AND holding_id=$3)
           OR ($2='company' AND holding_id=$3 AND company_id=$4)
           OR ($2='branch' AND holding_id=$3 AND company_id=$4 AND branch_id=$5)
           OR ($2='subtree' AND holding_id=$3 AND (
                (branch_id IS NULL AND cardinality($7::uuid[])=0 AND company_id=ANY($6::uuid[]))
                OR (branch_id IS NOT NULL AND company_id=ANY($6::uuid[]) AND branch_id=ANY($7::uuid[]))
           )))",
    )
    .bind(refund_id.as_uuid())
    .bind(kind)
    .bind(holding_id)
    .bind(company_id)
    .bind(branch_id)
    .bind(company_ids)
    .bind(branch_ids)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| PortError::Unavailable)?;
    let Some(row) = row else {
        return missing_or_denied(
            &mut tx,
            "erpx_pos_write.refund_guards",
            "refund_id",
            refund_id.as_uuid(),
        )
        .await;
    };
    let sale_id =
        SaleId::try_from_uuid(port_uuid(&row, "sale_id")?).map_err(|_| PortError::Internal)?;
    let sale = load_sale_in_tx(&mut tx, authority, sale_id)
        .await?
        .ok_or(PortError::Internal)?;
    if sale.status() != SaleStatus::Completed || sale.scope() != &exact_scope_from_row(&row)? {
        return Err(PortError::Internal);
    }
    let line_rows = sqlx::query(
        "SELECT sale_line_id,quantity,amount_minor FROM erpx_pos_write.refund_line_guards
         WHERE refund_id=$1 ORDER BY sale_line_id",
    )
    .bind(refund_id.as_uuid())
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| PortError::Unavailable)?;
    let lines = line_rows
        .into_iter()
        .map(|line| {
            Ok(RefundLine {
                sale_line_id: SaleLineId::try_from_uuid(port_uuid(&line, "sale_line_id")?)
                    .map_err(|_| PortError::Internal)?,
                quantity: port_u64(port_value(&line, "quantity")?)?,
                amount_minor: port_value(&line, "amount_minor")?,
            })
        })
        .collect::<Result<Vec<_>, PortError>>()?;
    PosRefund::try_rehydrate(
        refund_id,
        port_u64(port_value(&row, "aggregate_version")?)?,
        &sale,
        lines,
        port_value(&row, "reason")?,
        port_value(&row, "requester_principal_ref")?,
        port_value(&row, "reviewer_principal_ref")?,
        parse_refund_status(&port_value::<String>(&row, "refund_status")?)?,
        port_value(&row, "requested_at")?,
        port_value(&row, "updated_at")?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

#[derive(Debug, Clone)]
pub struct PosCommandAdapter {
    store: CommandStore,
}

impl PosCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl SaleUnitOfWork for PosCommandAdapter {
    async fn find_sale_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<SaleStoredCommit>, PortError> {
        let sale_id = fingerprint_sale_id(&identity.fingerprint).ok_or(PortError::Internal)?;
        let receipt = self
            .store
            .find_receipt::<Sale>(
                CONTEXT_NAME,
                &sale_partition(sale_id),
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?;
        receipt.map(sale_receipt_to_stored).transpose()
    }

    async fn commit_sale(&self, request: SaleCommit) -> Result<SaleStoredCommit, PortError> {
        validate_sale_commit(&request)?;
        let sale_id = request.aggregate.id();
        let partition = sale_partition(sale_id);
        let occurred_at = request.aggregate.updated_at();
        let command_id = request.identity.command_id;
        let mut events = integration_events(
            &request.integration_events,
            command_id,
            &partition,
            request.causation_id,
            occurred_at,
        );
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, SALE_SNAPSHOT_EVENT_TYPE, events.len()),
            event_type: SALE_SNAPSHOT_EVENT_TYPE.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: PosOutboxPayload::SaleSnapshot(PosSaleSnapshot {
                sale: (&request.aggregate).into(),
            }),
        });
        events.push(directory_event(
            command_id,
            &partition,
            request.causation_id,
            occurred_at,
            sale_directory_draft(&request.aggregate)?,
            events.len(),
        ));
        let guard = SaleAtomicGuard {
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
            tenant_scope: aggregate.scope().clone(),
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: SALE_AGGREGATE_TYPE.into(),
            aggregate_id: sale_aggregate_id(sale_id)?,
            expected_version: request.expected_version,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"pos-sale-audit-v1"),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({
                    "sale_id": sale_id.as_uuid(),
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
        let mut stored = sale_receipt_to_stored(receipt)?;
        stored.replayed = replayed;
        Ok(stored)
    }
}

#[async_trait]
impl RefundUnitOfWork for PosCommandAdapter {
    async fn find_refund_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<RefundStoredCommit>, PortError> {
        let refund_id = fingerprint_refund_id(&identity.fingerprint).ok_or(PortError::Internal)?;
        let receipt = self
            .store
            .find_receipt::<PosRefund>(
                CONTEXT_NAME,
                &refund_partition(refund_id),
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?;
        receipt.map(refund_receipt_to_stored).transpose()
    }

    async fn commit_refund(&self, request: RefundCommit) -> Result<RefundStoredCommit, PortError> {
        validate_refund_commit(&request)?;
        let refund_id = request.aggregate.id();
        let partition = refund_partition(refund_id);
        let occurred_at = request.aggregate.updated_at();
        let command_id = request.identity.command_id;
        let mut events = integration_events(
            &request.integration_events,
            command_id,
            &partition,
            request.causation_id,
            occurred_at,
        );
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, REFUND_SNAPSHOT_EVENT_TYPE, events.len()),
            event_type: REFUND_SNAPSHOT_EVENT_TYPE.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: PosOutboxPayload::RefundSnapshot(PosRefundSnapshot {
                refund: (&request.aggregate).into(),
            }),
        });
        events.push(directory_event(
            command_id,
            &partition,
            request.causation_id,
            occurred_at,
            refund_directory_draft(&request.aggregate)?,
            events.len(),
        ));
        let guard = RefundAtomicGuard {
            aggregate: request.aggregate.clone(),
        };
        let aggregate = request.aggregate;
        let original_sale_id = aggregate.sale_id();
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
            aggregate_type: REFUND_AGGREGATE_TYPE.into(),
            aggregate_id: refund_aggregate_id(refund_id)?,
            expected_version: request.expected_version,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"pos-refund-audit-v1"),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({
                    "refund_id": refund_id.as_uuid(),
                    "sale_id": original_sale_id.as_uuid(),
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
        let mut stored = refund_receipt_to_stored(receipt)?;
        stored.replayed = replayed;
        Ok(stored)
    }
}

fn directory_event(
    command_id: CommandId,
    partition: &str,
    causation_id: Option<EventId>,
    occurred_at: chrono::DateTime<chrono::Utc>,
    payload: ItemDescriptorDraft,
    index: usize,
) -> OutboxEventWrite<PosOutboxPayload> {
    OutboxEventWrite {
        event_id: deterministic_event_id(command_id, ITEM_DIRECTORY_OWNER_EVENT_TYPE, index),
        event_type: ITEM_DIRECTORY_OWNER_EVENT_TYPE.into(),
        event_version: ITEM_DIRECTORY_FACT_VERSION,
        partition: partition.into(),
        causation_id,
        occurred_at,
        payload: PosOutboxPayload::Directory(payload),
    }
}

fn sale_directory_draft(aggregate: &Sale) -> Result<ItemDescriptorDraft, PortError> {
    item_directory_draft(
        aggregate.id().as_uuid(),
        "pos.sale",
        aggregate.scope(),
        sale_status_name(aggregate.status()),
        aggregate.version(),
        aggregate.receipt_code(),
    )
}

fn refund_directory_draft(aggregate: &PosRefund) -> Result<ItemDescriptorDraft, PortError> {
    item_directory_draft(
        aggregate.id().as_uuid(),
        "pos.refund",
        aggregate.scope(),
        refund_status_name(aggregate.status()),
        aggregate.version(),
        &aggregate.sale_id().as_uuid().to_string(),
    )
}

fn item_directory_draft(
    id: Uuid,
    record_type: &str,
    scope: &TenantScope,
    lifecycle_state: &str,
    descriptor_version: u64,
    search_text: &str,
) -> Result<ItemDescriptorDraft, PortError> {
    let record_id = RecordId::try_from_uuid(id).map_err(|_| PortError::Internal)?;
    let record = StableRecordRef::try_new(
        record_id,
        ContextId::PointOfSale,
        RecordTypeId::try_new(record_type).map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    let value = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
        record,
        scope: scope.clone(),
        lifecycle_state: lifecycle_state.into(),
        classification: DataClassification::Confidential,
        descriptor_version,
        authorized_search_text: search_text.into(),
    };
    value.validate().map_err(|_| PortError::Internal)?;
    Ok(value)
}

fn integration_events(
    events: &[erpx_integration_contracts::pos::Event],
    command_id: CommandId,
    partition: &str,
    causation_id: Option<EventId>,
    occurred_at: chrono::DateTime<chrono::Utc>,
) -> Vec<OutboxEventWrite<PosOutboxPayload>> {
    events
        .iter()
        .enumerate()
        .map(|(index, event)| OutboxEventWrite {
            event_id: deterministic_event_id(command_id, event.event_type(), index),
            event_type: event.event_type().into(),
            event_version: u32::from(event.schema_version()),
            partition: partition.into(),
            causation_id,
            occurred_at,
            payload: PosOutboxPayload::Integration(event.clone()),
        })
        .collect()
}

struct SaleAtomicGuard {
    aggregate: Sale,
}

#[async_trait]
impl AtomicCommandGuard for SaleAtomicGuard {
    async fn authoritative_version(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        validate_guard_context(
            SALE_AGGREGATE_TYPE,
            self.aggregate.id().as_uuid(),
            &sale_partition(self.aggregate.id()),
            context,
        )?;
        sqlx::query_scalar::<_, i64>(
            "SELECT aggregate_version FROM erpx_pos_write.sale_guards
             WHERE sale_id=$1 FOR UPDATE",
        )
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?
        .map(from_i64)
        .transpose()
    }

    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(
            SALE_AGGREGATE_TYPE,
            self.aggregate.id().as_uuid(),
            &sale_partition(self.aggregate.id()),
            context,
        )?;
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT aggregate_version FROM erpx_pos_write.sale_guards
             WHERE sale_id=$1 FOR UPDATE",
        )
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        validate_existence(context.expected_version, current, "POS sale")?;
        let duplicate: Option<Uuid> = sqlx::query_scalar(
            "SELECT sale_id FROM erpx_pos_write.sale_guards
             WHERE scope_key=$1 AND receipt_code=$2 AND sale_id<>$3
             ORDER BY sale_id LIMIT 1 FOR UPDATE",
        )
        .bind(guard_scope_key(self.aggregate.scope())?)
        .bind(self.aggregate.receipt_code())
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        if duplicate.is_some() {
            return Err(CqrsSqlxError::GuardRejected(
                "POS receipt code already exists in scope",
            ));
        }
        Ok(())
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_applied_context(
            SALE_AGGREGATE_TYPE,
            self.aggregate.id().as_uuid(),
            self.aggregate.version(),
            &sale_partition(self.aggregate.id()),
            context,
        )?;
        let (holding_id, company_id, branch_id) = exact_scope_parts(self.aggregate.scope())
            .map_err(|_| CqrsSqlxError::InvalidInput("POS scope must be exact"))?;
        let changed = sqlx::query(
            "INSERT INTO erpx_pos_write.sale_guards
                (sale_id,scope_key,holding_id,company_id,branch_id,receipt_code,register_id,customer_id,currency,
                 subtotal_minor,tax_minor,total_minor,paid_minor,sale_status,
                 aggregate_version,created_at,updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)
             ON CONFLICT (sale_id) DO UPDATE SET
                scope_key=EXCLUDED.scope_key,holding_id=EXCLUDED.holding_id,
                company_id=EXCLUDED.company_id,branch_id=EXCLUDED.branch_id,
                receipt_code=EXCLUDED.receipt_code,
                register_id=EXCLUDED.register_id,customer_id=EXCLUDED.customer_id,
                currency=EXCLUDED.currency,subtotal_minor=EXCLUDED.subtotal_minor,
                tax_minor=EXCLUDED.tax_minor,total_minor=EXCLUDED.total_minor,
                paid_minor=EXCLUDED.paid_minor,sale_status=EXCLUDED.sale_status,
                aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at
             WHERE erpx_pos_write.sale_guards.aggregate_version + 1=EXCLUDED.aggregate_version",
        )
        .bind(self.aggregate.id().as_uuid())
        .bind(guard_scope_key(self.aggregate.scope())?)
        .bind(holding_id)
        .bind(company_id)
        .bind(branch_id)
        .bind(self.aggregate.receipt_code())
        .bind(self.aggregate.register_id().as_uuid())
        .bind(self.aggregate.customer_id().map(|value| value.as_uuid()))
        .bind(self.aggregate.currency())
        .bind(self.aggregate.subtotal_minor())
        .bind(self.aggregate.tax_minor())
        .bind(self.aggregate.total_minor())
        .bind(self.aggregate.paid_minor())
        .bind(sale_status_name(self.aggregate.status()))
        .bind(to_i64(self.aggregate.version())?)
        .bind(self.aggregate.created_at())
        .bind(self.aggregate.updated_at())
        .execute(&mut **transaction)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(CqrsSqlxError::GuardRejected("POS sale write conflict"));
        }
        sqlx::query("DELETE FROM erpx_pos_write.sale_line_guards WHERE sale_id=$1")
            .bind(self.aggregate.id().as_uuid())
            .execute(&mut **transaction)
            .await?;
        for line in self.aggregate.lines() {
            sqlx::query(
                "INSERT INTO erpx_pos_write.sale_line_guards
                    (sale_id,line_id,item_reference,description,quantity,unit_price_minor,tax_minor)
                 VALUES ($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(self.aggregate.id().as_uuid())
            .bind(line.line_id.as_uuid())
            .bind(&line.item_reference)
            .bind(&line.description)
            .bind(to_i64(line.quantity)?)
            .bind(line.unit_price_minor)
            .bind(line.tax_minor)
            .execute(&mut **transaction)
            .await?;
        }
        sqlx::query("DELETE FROM erpx_pos_write.payment_guards WHERE sale_id=$1")
            .bind(self.aggregate.id().as_uuid())
            .execute(&mut **transaction)
            .await?;
        for payment in self.aggregate.payments() {
            sqlx::query(
                "INSERT INTO erpx_pos_write.payment_guards
                    (sale_id,payment_id,amount_minor,payment_method,received_at)
                 VALUES ($1,$2,$3,$4,$5)",
            )
            .bind(self.aggregate.id().as_uuid())
            .bind(payment.payment_id.as_uuid())
            .bind(payment.amount_minor)
            .bind(payment_method_name(payment.method))
            .bind(payment.received_at)
            .execute(&mut **transaction)
            .await?;
        }
        Ok(())
    }
}

struct RefundAtomicGuard {
    aggregate: PosRefund,
}

#[async_trait]
impl AtomicCommandGuard for RefundAtomicGuard {
    async fn authoritative_version(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        validate_guard_context(
            REFUND_AGGREGATE_TYPE,
            self.aggregate.id().as_uuid(),
            &refund_partition(self.aggregate.id()),
            context,
        )?;
        sqlx::query_scalar::<_, i64>(
            "SELECT aggregate_version FROM erpx_pos_write.refund_guards
             WHERE refund_id=$1 FOR UPDATE",
        )
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?
        .map(from_i64)
        .transpose()
    }

    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(
            REFUND_AGGREGATE_TYPE,
            self.aggregate.id().as_uuid(),
            &refund_partition(self.aggregate.id()),
            context,
        )?;
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT aggregate_version FROM erpx_pos_write.refund_guards
             WHERE refund_id=$1 FOR UPDATE",
        )
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        validate_existence(context.expected_version, current, "POS refund")?;

        // The sale row is the serialization fence for all concurrent refund
        // requests against this sale. Requested/approved/posted refunds reserve
        // quantity and amount; rejected/cancelled refunds release them.
        let sale: Option<(String, String)> = sqlx::query_as(
            "SELECT sale_status,currency FROM erpx_pos_write.sale_guards
             WHERE sale_id=$1 FOR UPDATE",
        )
        .bind(self.aggregate.sale_id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        let Some((sale_status, sale_currency)) = sale else {
            return Err(CqrsSqlxError::GuardRejected(
                "POS refund sale guard is missing",
            ));
        };
        if sale_status != "completed" || sale_currency != self.aggregate.currency() {
            return Err(CqrsSqlxError::GuardRejected(
                "POS refund requires the matching completed sale",
            ));
        }
        let reserve_candidate = reserves_balance(self.aggregate.status());
        for line in self.aggregate.lines() {
            let sale_line: Option<(i64, i64)> = sqlx::query_as(
                "SELECT quantity,(quantity * unit_price_minor) + tax_minor
                 FROM erpx_pos_write.sale_line_guards
                 WHERE sale_id=$1 AND line_id=$2",
            )
            .bind(self.aggregate.sale_id().as_uuid())
            .bind(line.sale_line_id.as_uuid())
            .fetch_optional(&mut **transaction)
            .await?;
            let Some((sale_quantity, sale_amount)) = sale_line else {
                return Err(CqrsSqlxError::GuardRejected(
                    "POS refund references an unknown sale line",
                ));
            };
            let (reserved_quantity, reserved_amount): (i64, i64) = sqlx::query_as(
                "SELECT COALESCE(SUM(line.quantity),0)::bigint,
                        COALESCE(SUM(line.amount_minor),0)::bigint
                 FROM erpx_pos_write.refund_line_guards line
                 JOIN erpx_pos_write.refund_guards refund ON refund.refund_id=line.refund_id
                 WHERE refund.sale_id=$1 AND line.sale_line_id=$2
                   AND refund.refund_status IN ('requested','approved','posted')
                   AND refund.refund_id<>$3",
            )
            .bind(self.aggregate.sale_id().as_uuid())
            .bind(line.sale_line_id.as_uuid())
            .bind(self.aggregate.id().as_uuid())
            .fetch_one(&mut **transaction)
            .await?;
            let candidate_quantity = if reserve_candidate {
                to_i64(line.quantity)?
            } else {
                0
            };
            let candidate_amount = if reserve_candidate {
                line.amount_minor
            } else {
                0
            };
            if reserved_quantity
                .checked_add(candidate_quantity)
                .is_none_or(|value| value > sale_quantity)
                || reserved_amount
                    .checked_add(candidate_amount)
                    .is_none_or(|value| value > sale_amount)
            {
                return Err(CqrsSqlxError::GuardRejected(
                    "POS refund balance is exhausted",
                ));
            }
        }
        Ok(())
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_applied_context(
            REFUND_AGGREGATE_TYPE,
            self.aggregate.id().as_uuid(),
            self.aggregate.version(),
            &refund_partition(self.aggregate.id()),
            context,
        )?;
        let (holding_id, company_id, branch_id) = exact_scope_parts(self.aggregate.scope())
            .map_err(|_| CqrsSqlxError::InvalidInput("POS scope must be exact"))?;
        let changed = sqlx::query(
            "INSERT INTO erpx_pos_write.refund_guards
                (refund_id,scope_key,holding_id,company_id,branch_id,sale_id,currency,total_amount_minor,reason,
                 requester_principal_ref,reviewer_principal_ref,refund_status,
                 aggregate_version,requested_at,updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
             ON CONFLICT (refund_id) DO UPDATE SET
                scope_key=EXCLUDED.scope_key,holding_id=EXCLUDED.holding_id,
                company_id=EXCLUDED.company_id,branch_id=EXCLUDED.branch_id,
                sale_id=EXCLUDED.sale_id,
                currency=EXCLUDED.currency,total_amount_minor=EXCLUDED.total_amount_minor,
                reason=EXCLUDED.reason,requester_principal_ref=EXCLUDED.requester_principal_ref,
                reviewer_principal_ref=EXCLUDED.reviewer_principal_ref,
                refund_status=EXCLUDED.refund_status,
                aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at
             WHERE erpx_pos_write.refund_guards.aggregate_version + 1=EXCLUDED.aggregate_version",
        )
        .bind(self.aggregate.id().as_uuid())
        .bind(guard_scope_key(self.aggregate.scope())?)
        .bind(holding_id)
        .bind(company_id)
        .bind(branch_id)
        .bind(self.aggregate.sale_id().as_uuid())
        .bind(self.aggregate.currency())
        .bind(self.aggregate.total_amount_minor())
        .bind(self.aggregate.reason())
        .bind(self.aggregate.requester_principal_ref())
        .bind(self.aggregate.reviewer_principal_ref())
        .bind(refund_status_name(self.aggregate.status()))
        .bind(to_i64(self.aggregate.version())?)
        .bind(self.aggregate.requested_at())
        .bind(self.aggregate.updated_at())
        .execute(&mut **transaction)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(CqrsSqlxError::GuardRejected("POS refund write conflict"));
        }
        sqlx::query("DELETE FROM erpx_pos_write.refund_line_guards WHERE refund_id=$1")
            .bind(self.aggregate.id().as_uuid())
            .execute(&mut **transaction)
            .await?;
        for line in self.aggregate.lines() {
            sqlx::query(
                "INSERT INTO erpx_pos_write.refund_line_guards
                    (refund_id,sale_line_id,quantity,amount_minor)
                 VALUES ($1,$2,$3,$4)",
            )
            .bind(self.aggregate.id().as_uuid())
            .bind(line.sale_line_id.as_uuid())
            .bind(to_i64(line.quantity)?)
            .bind(line.amount_minor)
            .execute(&mut **transaction)
            .await?;
        }
        Ok(())
    }
}

fn validate_guard_context(
    aggregate_type: &str,
    aggregate_id: Uuid,
    partition: &str,
    context: &GuardCommandContext<'_>,
) -> Result<(), CqrsSqlxError> {
    if context.context != CONTEXT_NAME
        || context.aggregate_type != aggregate_type
        || context.aggregate_id.as_uuid() != aggregate_id
        || context.partition != partition
    {
        return Err(CqrsSqlxError::GuardRejected(
            "POS command guard context mismatch",
        ));
    }
    Ok(())
}

fn validate_applied_context(
    aggregate_type: &str,
    aggregate_id: Uuid,
    version: u64,
    partition: &str,
    context: &AppliedGuardContext<'_>,
) -> Result<(), CqrsSqlxError> {
    if context.context != CONTEXT_NAME
        || context.aggregate_type != aggregate_type
        || context.aggregate_id.as_uuid() != aggregate_id
        || context.partition != partition
        || context.aggregate_version != version
    {
        return Err(CqrsSqlxError::GuardRejected(
            "POS applied guard context mismatch",
        ));
    }
    Ok(())
}

fn validate_existence(
    expected_version: Option<u64>,
    current: Option<i64>,
    aggregate: &'static str,
) -> Result<(), CqrsSqlxError> {
    match (expected_version, current) {
        (None, Some(_)) => Err(CqrsSqlxError::GuardRejected(match aggregate {
            "POS sale" => "POS sale already exists",
            _ => "POS refund already exists",
        })),
        (Some(_), None) => Err(CqrsSqlxError::GuardRejected(match aggregate {
            "POS sale" => "POS sale guard is missing",
            _ => "POS refund guard is missing",
        })),
        _ => Ok(()),
    }
}

type AuthorityPredicate = (
    &'static str,
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
    Vec<Uuid>,
    Vec<Uuid>,
);

fn authority_predicate(scope: &TenantScope) -> Result<AuthorityPredicate, PortError> {
    scope.validate().map_err(|_| PortError::Denied)?;
    Ok(match scope {
        TenantScope::Platform => ("platform", None, None, None, Vec::new(), Vec::new()),
        TenantScope::Holding { holding_id } => (
            "holding",
            Some(*holding_id),
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        TenantScope::Company {
            holding_id,
            company_id,
        } => (
            "company",
            Some(*holding_id),
            Some(*company_id),
            None,
            Vec::new(),
            Vec::new(),
        ),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => (
            "branch",
            Some(*holding_id),
            Some(*company_id),
            Some(*branch_id),
            Vec::new(),
            Vec::new(),
        ),
        TenantScope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } => (
            "subtree",
            Some(*holding_id),
            None,
            None,
            company_ids.clone(),
            branch_ids.clone(),
        ),
    })
}

fn exact_scope_parts(scope: &TenantScope) -> Result<(Uuid, Uuid, Option<Uuid>), PortError> {
    match scope {
        TenantScope::Company {
            holding_id,
            company_id,
        } => Ok((*holding_id, *company_id, None)),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok((*holding_id, *company_id, Some(*branch_id))),
        _ => Err(PortError::Internal),
    }
}

fn exact_scope_from_row(row: &PgRow) -> Result<TenantScope, PortError> {
    let holding_id = port_uuid(row, "holding_id")?;
    let company_id = port_uuid(row, "company_id")?;
    match port_value::<Option<Uuid>>(row, "branch_id")? {
        Some(branch_id) => Ok(TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        }),
        None => Ok(TenantScope::Company {
            holding_id,
            company_id,
        }),
    }
}

async fn missing_or_denied<T>(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    id_column: &str,
    id: Uuid,
) -> Result<Option<T>, PortError> {
    let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE {id_column}=$1)");
    let exists: bool = sqlx::query_scalar(&sql)
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
    if exists {
        Err(PortError::Denied)
    } else {
        Ok(None)
    }
}

fn port_value<'r, T>(row: &'r PgRow, column: &str) -> Result<T, PortError>
where
    T: Decode<'r, Postgres> + Type<Postgres>,
{
    row.try_get(column).map_err(|_| PortError::Internal)
}

fn port_uuid(row: &PgRow, column: &str) -> Result<Uuid, PortError> {
    port_value(row, column)
}

fn port_u64(value: i64) -> Result<u64, PortError> {
    u64::try_from(value).map_err(|_| PortError::Internal)
}

fn from_i64(value: i64) -> Result<u64, CqrsSqlxError> {
    u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

fn parse_payment_method(value: &str) -> Result<PaymentMethod, PortError> {
    match value {
        "cash" => Ok(PaymentMethod::Cash),
        "card" => Ok(PaymentMethod::Card),
        "bank_transfer" => Ok(PaymentMethod::BankTransfer),
        "mobile_wallet" => Ok(PaymentMethod::MobileWallet),
        "voucher" => Ok(PaymentMethod::Voucher),
        "other" => Ok(PaymentMethod::Other),
        _ => Err(PortError::Internal),
    }
}

fn parse_sale_status(value: &str) -> Result<SaleStatus, PortError> {
    match value {
        "draft" => Ok(SaleStatus::Draft),
        "open" => Ok(SaleStatus::Open),
        "held" => Ok(SaleStatus::Held),
        "completed" => Ok(SaleStatus::Completed),
        "voided" => Ok(SaleStatus::Voided),
        _ => Err(PortError::Internal),
    }
}

fn parse_refund_status(value: &str) -> Result<RefundStatus, PortError> {
    match value {
        "requested" => Ok(RefundStatus::Requested),
        "approved" => Ok(RefundStatus::Approved),
        "rejected" => Ok(RefundStatus::Rejected),
        "posted" => Ok(RefundStatus::Posted),
        "cancelled" => Ok(RefundStatus::Cancelled),
        _ => Err(PortError::Internal),
    }
}

fn validate_sale_commit(request: &SaleCommit) -> Result<(), PortError> {
    if request.aggregate.validate().is_err()
        || fingerprint_sale_id(&request.identity.fingerprint) != Some(request.aggregate.id())
        || !request.authority_scope.contains(request.aggregate.scope())
    {
        return Err(PortError::Internal);
    }
    Ok(())
}

fn validate_refund_commit(request: &RefundCommit) -> Result<(), PortError> {
    if request.aggregate.validate().is_err()
        || fingerprint_refund_id(&request.identity.fingerprint) != Some(request.aggregate.id())
        || !request.authority_scope.contains(request.aggregate.scope())
        || matches!(
            &request.identity.fingerprint,
            CommandFingerprint::RequestRefund { sale_id, .. }
                if *sale_id != request.aggregate.sale_id()
        )
    {
        return Err(PortError::Internal);
    }
    Ok(())
}

fn fingerprint_sale_id(fingerprint: &CommandFingerprint) -> Option<SaleId> {
    match fingerprint {
        CommandFingerprint::CreateSale { sale_id, .. }
        | CommandFingerprint::OpenSale { sale_id }
        | CommandFingerprint::HoldSale { sale_id }
        | CommandFingerprint::ResumeSale { sale_id }
        | CommandFingerprint::RecordPayment { sale_id, .. }
        | CommandFingerprint::CompleteSale { sale_id }
        | CommandFingerprint::VoidSale { sale_id } => Some(*sale_id),
        _ => None,
    }
}

fn fingerprint_refund_id(fingerprint: &CommandFingerprint) -> Option<RefundId> {
    match fingerprint {
        CommandFingerprint::RequestRefund { refund_id, .. }
        | CommandFingerprint::ApproveRefund { refund_id }
        | CommandFingerprint::RejectRefund { refund_id }
        | CommandFingerprint::PostRefund { refund_id }
        | CommandFingerprint::CancelRefund { refund_id } => Some(*refund_id),
        _ => None,
    }
}

fn request_hash(fingerprint: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let bytes = serde_json::to_vec(fingerprint).map_err(|_| PortError::Internal)?;
    let mut digest = Sha256::new();
    digest.update(b"erpx.pos.command-fingerprint.v1\0");
    digest.update(bytes);
    Ok(RequestHash::new(digest.finalize().into()))
}

fn audit_action(fingerprint: &CommandFingerprint) -> &'static str {
    match fingerprint {
        CommandFingerprint::CreateSale { .. } => "pos.sale.create",
        CommandFingerprint::OpenSale { .. } => "pos.sale.open",
        CommandFingerprint::HoldSale { .. } => "pos.sale.hold",
        CommandFingerprint::ResumeSale { .. } => "pos.sale.resume",
        CommandFingerprint::RecordPayment { .. } => "pos.sale.record_payment",
        CommandFingerprint::CompleteSale { .. } => "pos.sale.complete",
        CommandFingerprint::VoidSale { .. } => "pos.sale.void",
        CommandFingerprint::RequestRefund { .. } => "pos.refund.request",
        CommandFingerprint::ApproveRefund { .. } => "pos.refund.approve",
        CommandFingerprint::RejectRefund { .. } => "pos.refund.reject",
        CommandFingerprint::PostRefund { .. } => "pos.refund.post",
        CommandFingerprint::CancelRefund { .. } => "pos.refund.cancel",
    }
}

fn sale_receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<Sale>,
) -> Result<SaleStoredCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != SALE_AGGREGATE_TYPE
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.id().as_uuid()
        || receipt.aggregate.version != receipt.response.version()
        || receipt.response.validate().is_err()
    {
        return Err(PortError::Internal);
    }
    Ok(SaleStoredCommit {
        command_id: receipt.command_id,
        correlation_id: receipt.correlation_id,
        aggregate: receipt.response,
        committed_events: receipt.committed_events,
        committed_at: receipt.committed_at,
        replayed: receipt.replayed,
    })
}

fn refund_receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<PosRefund>,
) -> Result<RefundStoredCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != REFUND_AGGREGATE_TYPE
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.id().as_uuid()
        || receipt.aggregate.version != receipt.response.version()
        || receipt.response.validate().is_err()
    {
        return Err(PortError::Internal);
    }
    Ok(RefundStoredCommit {
        command_id: receipt.command_id,
        correlation_id: receipt.correlation_id,
        aggregate: receipt.response,
        committed_events: receipt.committed_events,
        committed_at: receipt.committed_at,
        replayed: receipt.replayed,
    })
}

fn sale_aggregate_id(sale_id: SaleId) -> Result<AggregateId, PortError> {
    AggregateId::try_from_uuid(sale_id.as_uuid()).map_err(|_| PortError::Internal)
}

fn refund_aggregate_id(refund_id: RefundId) -> Result<AggregateId, PortError> {
    AggregateId::try_from_uuid(refund_id.as_uuid()).map_err(|_| PortError::Internal)
}

fn guard_scope_key(scope: &TenantScope) -> Result<String, CqrsSqlxError> {
    scope_key(scope).map_err(|_| CqrsSqlxError::GuardRejected("POS scope is invalid"))
}

const fn reserves_balance(status: RefundStatus) -> bool {
    matches!(
        status,
        RefundStatus::Requested | RefundStatus::Approved | RefundStatus::Posted
    )
}

fn deterministic_event_id(command_id: CommandId, event_type: &str, index: usize) -> EventId {
    let mut label = event_type.as_bytes().to_vec();
    label.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(command_id.as_uuid(), &label))
        .expect("a digest-derived POS event ID is non-nil")
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
    i64::try_from(value).map_err(|_| CqrsSqlxError::GuardRejected("POS number is too large"))
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
        CqrsSqlxError::GuardRejected("POS refund balance is exhausted") => {
            PortError::RefundBalanceConflict
        }
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
    use erpx_pos::{Payment, PaymentId, PaymentMethod, RegisterId, SaleLine, SaleLineId};

    use super::*;

    fn create_fingerprint(receipt_code: &str) -> CommandFingerprint {
        CommandFingerprint::CreateSale {
            sale_id: SaleId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            scope: TenantScope::Company {
                holding_id: Uuid::from_u128(2),
                company_id: Uuid::from_u128(3),
            },
            receipt_code: receipt_code.into(),
            register_id: RegisterId::try_from_uuid(Uuid::from_u128(4)).unwrap(),
            customer_id: None,
            currency: "THB".into(),
            lines: vec![SaleLine {
                line_id: SaleLineId::try_from_uuid(Uuid::from_u128(5)).unwrap(),
                item_reference: "SKU-1".into(),
                description: "Item".into(),
                quantity: 1,
                unit_price_minor: 100,
                tax_minor: 7,
            }],
        }
    }

    #[test]
    fn fingerprint_hash_is_stable_and_payload_sensitive() {
        assert_eq!(
            request_hash(&create_fingerprint("R-1")).unwrap(),
            request_hash(&create_fingerprint("R-1")).unwrap()
        );
        assert_ne!(
            request_hash(&create_fingerprint("R-1")).unwrap(),
            request_hash(&create_fingerprint("R-2")).unwrap()
        );
    }

    #[test]
    fn aggregate_fingerprints_cannot_cross_receipt_partitions() {
        assert!(fingerprint_sale_id(&create_fingerprint("R-1")).is_some());
        assert!(fingerprint_refund_id(&create_fingerprint("R-1")).is_none());
        let refund = CommandFingerprint::ApproveRefund {
            refund_id: RefundId::try_from_uuid(Uuid::from_u128(9)).unwrap(),
        };
        assert!(fingerprint_sale_id(&refund).is_none());
        assert!(fingerprint_refund_id(&refund).is_some());
    }

    #[test]
    fn refund_balance_reservation_states_are_explicit() {
        assert!(reserves_balance(RefundStatus::Requested));
        assert!(reserves_balance(RefundStatus::Approved));
        assert!(reserves_balance(RefundStatus::Posted));
        assert!(!reserves_balance(RefundStatus::Rejected));
        assert!(!reserves_balance(RefundStatus::Cancelled));
    }

    #[test]
    fn audit_actions_and_payment_names_are_secret_free() {
        assert_eq!(
            audit_action(&CommandFingerprint::RecordPayment {
                sale_id: SaleId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
                payment: Payment {
                    payment_id: PaymentId::try_from_uuid(Uuid::from_u128(2)).unwrap(),
                    amount_minor: 100,
                    method: PaymentMethod::Card,
                    received_at: chrono::Utc::now(),
                },
            }),
            "pos.sale.record_payment"
        );
        assert_eq!(payment_method_name(PaymentMethod::Card), "card");
    }

    #[test]
    fn cumulative_refund_guard_uses_sale_row_as_serialization_fence() {
        let source = include_str!("command.rs");
        assert!(source.contains("WHERE sale_id=$1 FOR UPDATE"));
        assert!(source.contains("refund.refund_status IN ('requested','approved','posted')"));
        assert!(source.contains("POS refund balance is exhausted"));
    }
}
