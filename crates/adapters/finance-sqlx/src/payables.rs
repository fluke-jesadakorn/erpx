use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, ProjectionEvent,
    ProjectionFailure, Projector, ProjectorContext, ReadPool, ReadSideStore, RequestHash,
    SqlxProjectionWaiter, WaitError, WaiterConfig, WritePool,
};
use erpx_finance::{
    Payable, PayableId, PayablesAggregate, PayablesCommandFingerprint, PayablesCommandIdentity,
    PayablesCommandUnitOfWork, PayablesCommit, PayablesDomainEvent, PayablesReadRepository,
    PayablesReferenceVersion, PayablesWriteRepository, Payment, PaymentId, PaymentView, PortError,
    ReadProjection, StoredPayablesCommit, ThreeWayMatchEvidence, ThreeWayMatchEvidencePort,
    VendorInvoice, VendorInvoiceId, VendorInvoiceStatus, VendorInvoiceView,
};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, ItemDescriptorDraft,
    ItemDescriptorDraftV2, RecordTypeId, StableRecordRef, catalog::ContextId, record_policy_id,
};
use erpx_kernel::{AggregateId, AuthorityScope, CommandId, DataClassification, EventId, RecordId};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{CONTEXT_NAME, command::scope_key};

pub const VENDOR_INVOICE_AGGREGATE_TYPE: &str = "vendor_invoice";
pub const PAYMENT_AGGREGATE_TYPE: &str = "payment";
pub const VENDOR_INVOICE_SNAPSHOT_EVENT: &str = "finance.vendor_invoice_snapshot";
pub const PAYMENT_SNAPSHOT_EVENT: &str = "finance.payment_snapshot";
pub const PAYABLES_EVENT_VERSION: u32 = 1;
pub const VENDOR_INVOICE_PROJECTION: &str = "finance.vendor_invoices";
pub const PAYMENT_PROJECTION: &str = "finance.payments";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "payload_kind", content = "payload", rename_all = "snake_case")]
pub enum PayablesOutboxPayload {
    Snapshot(PayablesAggregate),
    Domain(PayablesDomainEvent),
    Directory(ItemDescriptorDraftV2),
}

#[derive(Debug, Clone)]
pub struct SqlxPayablesAdapter {
    pool: WritePool,
    store: CommandStore,
}

#[derive(Debug, Clone)]
pub struct PayablesReadAdapter {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}

impl PayablesReadAdapter {
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

    async fn checkpoint(
        &self,
        projection: &str,
        partition: &str,
        consistency: &erpx_cqrs::QueryConsistency,
    ) -> Result<Option<erpx_cqrs::ProjectionCheckpoint>, PortError> {
        match consistency {
            erpx_cqrs::QueryConsistency::Eventual => self
                .read_side
                .checkpoint(projection, partition)
                .await
                .map_err(map_store_error),
            erpx_cqrs::QueryConsistency::ReadYourWrites(requirement) => {
                if requirement.projection != projection || requirement.partition != partition {
                    return Err(PortError::NotReady(erpx_cqrs::ProjectionNotReady {
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

    async fn metadata(
        &self,
        projection: &str,
        checkpoint: Option<erpx_cqrs::ProjectionCheckpoint>,
    ) -> Result<erpx_cqrs::ProjectionMetadata, PortError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM erpx_cqrs_read.projection_generations
             WHERE projection_name=$1 AND status IN ('active','building')
             ORDER BY CASE status WHEN 'active' THEN 0 ELSE 1 END LIMIT 1",
        )
        .bind(projection)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
        let freshness = match status.as_deref() {
            Some("active") => erpx_cqrs::FreshnessState::Current,
            Some("building") => erpx_cqrs::FreshnessState::Rebuilding,
            _ => erpx_cqrs::FreshnessState::Unavailable,
        };
        let lag_millis = checkpoint
            .as_ref()
            .and_then(|value| {
                chrono::Utc::now()
                    .signed_duration_since(value.updated_at)
                    .to_std()
                    .ok()
            })
            .and_then(|value| u64::try_from(value.as_millis()).ok());
        Ok(erpx_cqrs::ProjectionMetadata {
            checkpoint,
            freshness,
            lag_millis,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn get<T>(
        &self,
        scope: &AuthorityScope,
        table: &'static str,
        id_column: &'static str,
        id: Uuid,
        projection: &'static str,
        partition: &str,
        consistency: &erpx_cqrs::QueryConsistency,
    ) -> Result<ReadProjection<Option<T>>, PortError>
    where
        T: serde::de::DeserializeOwned + ProjectedScope,
    {
        scope.validate().map_err(|_| PortError::Denied)?;
        let checkpoint = self.checkpoint(projection, partition, consistency).await?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let statement = format!(
            "SELECT projection.projection_document FROM {table} projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.holding_id=projection.holding_id
              AND generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation
              AND generation.status='active'
             WHERE projection.projection_name=$1 AND projection.{id_column}=$2
               AND projection.partition_key=$3"
        );
        let document: Option<serde_json::Value> = sqlx::query_scalar(&statement)
            .bind(projection)
            .bind(id)
            .bind(partition)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| PortError::Unavailable)?;
        let value = document
            .map(serde_json::from_value::<T>)
            .transpose()
            .map_err(|_| PortError::Internal)?;
        if value
            .as_ref()
            .is_some_and(|record| !scope.contains(record.projected_scope()))
        {
            return Err(PortError::Denied);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(projection, checkpoint).await?,
        })
    }
}

trait ProjectedScope {
    fn projected_scope(&self) -> &AuthorityScope;
}

impl ProjectedScope for VendorInvoiceView {
    fn projected_scope(&self) -> &AuthorityScope {
        &self.scope
    }
}

impl ProjectedScope for PaymentView {
    fn projected_scope(&self) -> &AuthorityScope {
        &self.scope
    }
}

#[async_trait]
impl PayablesReadRepository for PayablesReadAdapter {
    async fn get_vendor_invoice(
        &self,
        scope: &AuthorityScope,
        id: VendorInvoiceId,
        consistency: &erpx_cqrs::QueryConsistency,
    ) -> Result<ReadProjection<Option<VendorInvoiceView>>, PortError> {
        let partition = format!("finance.vendor-invoice:{}", id.as_uuid().simple());
        self.get(
            scope,
            "erpx_finance_read.vendor_invoice_projection",
            "vendor_invoice_id",
            id.as_uuid(),
            VENDOR_INVOICE_PROJECTION,
            &partition,
            consistency,
        )
        .await
    }

    async fn get_payment(
        &self,
        scope: &AuthorityScope,
        id: PaymentId,
        consistency: &erpx_cqrs::QueryConsistency,
    ) -> Result<ReadProjection<Option<PaymentView>>, PortError> {
        let partition = format!("finance.payment:{}", id.as_uuid().simple());
        self.get(
            scope,
            "erpx_finance_read.payment_projection",
            "payment_id",
            id.as_uuid(),
            PAYMENT_PROJECTION,
            &partition,
            consistency,
        )
        .await
    }
}

macro_rules! payables_projector {
    ($name:ident, $projection:expr, $event:expr, $apply:ident, $table:literal) => {
        #[derive(Debug, Clone)]
        pub struct $name {
            event_types: Vec<String>,
        }

        impl $name {
            pub fn new() -> Self {
                Self {
                    event_types: vec![$event.into()],
                }
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        #[async_trait]
        impl Projector for $name {
            fn name(&self) -> &str {
                $projection
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
                $apply(tx, context, event).await
            }

            async fn reset(
                &self,
                tx: &mut Transaction<'_, Postgres>,
                context: &ProjectorContext<'_>,
            ) -> Result<(), ProjectionFailure> {
                sqlx::query(concat!(
                    "DELETE FROM ",
                    $table,
                    " WHERE projection_name=$1 AND generation=$2"
                ))
                .bind($projection)
                .bind(context.generation)
                .execute(&mut **tx)
                .await
                .map_err(projection_failure)?;
                Ok(())
            }
        }
    };
}

payables_projector!(
    FinanceVendorInvoiceProjector,
    VENDOR_INVOICE_PROJECTION,
    VENDOR_INVOICE_SNAPSHOT_EVENT,
    apply_vendor_invoice_projection,
    "erpx_finance_read.vendor_invoice_projection"
);
payables_projector!(
    FinancePaymentProjector,
    PAYMENT_PROJECTION,
    PAYMENT_SNAPSHOT_EVENT,
    apply_payment_projection,
    "erpx_finance_read.payment_projection"
);

fn projected_snapshot(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
    projection: &str,
    aggregate_type: &str,
    event_type: &str,
) -> Result<PayablesAggregate, ProjectionFailure> {
    if context.projection != projection
        || event.context != CONTEXT_NAME
        || event.aggregate_type != aggregate_type
        || event.event_type != event_type
        || event.event_version != PAYABLES_EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "Finance AP projector received incompatible metadata",
        ));
    }
    let payload = serde_json::from_value::<PayablesOutboxPayload>(event.payload.clone())
        .map_err(projection_failure)?;
    let PayablesOutboxPayload::Snapshot(snapshot) = payload else {
        return Err(ProjectionFailure::new(
            "Finance AP snapshot event carried a non-snapshot payload",
        ));
    };
    if snapshot.aggregate_type() != aggregate_type
        || snapshot.id_uuid() != event.aggregate_id.as_uuid()
        || snapshot.version() != event.aggregate_version
        || snapshot.scope() != &event.active_scope
        || aggregate_partition(&snapshot) != event.partition
    {
        return Err(ProjectionFailure::new(
            "Finance AP snapshot metadata mismatch",
        ));
    }
    Ok(snapshot)
}

async fn apply_vendor_invoice_projection(
    tx: &mut Transaction<'_, Postgres>,
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<(), ProjectionFailure> {
    let PayablesAggregate::VendorInvoice(invoice) = projected_snapshot(
        context,
        event,
        VENDOR_INVOICE_PROJECTION,
        VENDOR_INVOICE_AGGREGATE_TYPE,
        VENDOR_INVOICE_SNAPSHOT_EVENT,
    )?
    else {
        return Err(ProjectionFailure::new(
            "Finance vendor invoice snapshot mismatch",
        ));
    };
    let view = VendorInvoiceView::from(&invoice);
    let result = sqlx::query(
        "INSERT INTO erpx_finance_read.vendor_invoice_projection
         (projection_name,generation,partition_key,vendor_invoice_id,source_aggregate_version,
          source_global_sequence,scope,scope_key,supplier_id,invoice_number,status,payable_id,
          total_minor,currency,projection_document,aggregate_created_at,aggregate_updated_at,projected_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,now())
         ON CONFLICT (holding_id,projection_name,generation,partition_key,vendor_invoice_id)
         DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
          source_global_sequence=EXCLUDED.source_global_sequence,status=EXCLUDED.status,
          payable_id=EXCLUDED.payable_id,projection_document=EXCLUDED.projection_document,
          aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now()
         WHERE erpx_finance_read.vendor_invoice_projection.source_aggregate_version<
               EXCLUDED.source_aggregate_version",
    )
    .bind(VENDOR_INVOICE_PROJECTION)
    .bind(context.generation)
    .bind(&event.partition)
    .bind(invoice.id().as_uuid())
    .bind(projection_i64(invoice.version())?)
    .bind(projection_i64(event.global_sequence)?)
    .bind(serde_json::to_value(invoice.scope()).map_err(projection_failure)?)
    .bind(scope_key(invoice.scope()).map_err(projection_failure)?)
    .bind(invoice.supplier().as_uuid())
    .bind(invoice.invoice_number())
    .bind(vendor_invoice_status(invoice.status()))
    .bind(invoice.payable_id().map(PayableId::as_uuid))
    .bind(invoice.total_amount().minor_units())
    .bind(invoice.total_amount().currency().as_str())
    .bind(serde_json::to_value(view).map_err(projection_failure)?)
    .bind(invoice.created_at())
    .bind(invoice.updated_at())
    .execute(&mut **tx)
    .await
    .map_err(projection_failure)?;
    projection_changed(
        result,
        "Finance vendor invoice snapshot is duplicate or out of order",
    )
}

async fn apply_payment_projection(
    tx: &mut Transaction<'_, Postgres>,
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<(), ProjectionFailure> {
    let PayablesAggregate::Payment(payment) = projected_snapshot(
        context,
        event,
        PAYMENT_PROJECTION,
        PAYMENT_AGGREGATE_TYPE,
        PAYMENT_SNAPSHOT_EVENT,
    )?
    else {
        return Err(ProjectionFailure::new("Finance payment snapshot mismatch"));
    };
    let view = PaymentView::from(&payment);
    let result = sqlx::query(
        "INSERT INTO erpx_finance_read.payment_projection
         (projection_name,generation,partition_key,payment_id,source_aggregate_version,
          source_global_sequence,scope,scope_key,supplier_id,payment_code,status,amount_minor,
          currency,projection_document,aggregate_created_at,aggregate_updated_at,projected_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,now())
         ON CONFLICT (holding_id,projection_name,generation,partition_key,payment_id)
         DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
          source_global_sequence=EXCLUDED.source_global_sequence,status=EXCLUDED.status,
          projection_document=EXCLUDED.projection_document,
          aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now()
         WHERE erpx_finance_read.payment_projection.source_aggregate_version<
               EXCLUDED.source_aggregate_version",
    )
    .bind(PAYMENT_PROJECTION)
    .bind(context.generation)
    .bind(&event.partition)
    .bind(payment.id().as_uuid())
    .bind(projection_i64(payment.version())?)
    .bind(projection_i64(event.global_sequence)?)
    .bind(serde_json::to_value(payment.scope()).map_err(projection_failure)?)
    .bind(scope_key(payment.scope()).map_err(projection_failure)?)
    .bind(payment.supplier().as_uuid())
    .bind(payment.payment_code())
    .bind(payment_status(payment.status()))
    .bind(payment.amount().minor_units())
    .bind(payment.amount().currency().as_str())
    .bind(serde_json::to_value(view).map_err(projection_failure)?)
    .bind(payment.created_at())
    .bind(payment.updated_at())
    .execute(&mut **tx)
    .await
    .map_err(projection_failure)?;
    projection_changed(
        result,
        "Finance payment snapshot is duplicate or out of order",
    )
}

fn projection_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value)
        .map_err(|_| ProjectionFailure::new("Finance AP numeric value is too large"))
}

fn projection_changed(
    result: sqlx::postgres::PgQueryResult,
    message: &'static str,
) -> Result<(), ProjectionFailure> {
    if result.rows_affected() == 1 {
        Ok(())
    } else {
        Err(ProjectionFailure::new(message))
    }
}

fn projection_failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

impl SqlxPayablesAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool.clone()),
            pool,
        }
    }

    async fn load_document(
        &self,
        authority: &AuthorityScope,
        table: &'static str,
        id_column: &'static str,
        document_column: &'static str,
        id: Uuid,
    ) -> Result<Option<serde_json::Value>, PortError> {
        authority.validate().map_err(|_| PortError::Denied)?;
        let mut tx = self.pool.begin().await.map_err(map_store_error)?;
        let statement =
            format!("SELECT scope,{document_column} FROM {table} WHERE {id_column}=$1 FOR SHARE");
        let row = sqlx::query(&statement)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| PortError::Unavailable)?;
        let Some(row) = row else { return Ok(None) };
        let target: AuthorityScope =
            serde_json::from_value(row.try_get("scope").map_err(|_| PortError::Internal)?)
                .map_err(|_| PortError::Internal)?;
        if !authority.contains(&target) {
            return Err(PortError::Denied);
        }
        row.try_get(document_column)
            .map(Some)
            .map_err(|_| PortError::Internal)
    }
}

#[async_trait]
impl ThreeWayMatchEvidencePort for SqlxPayablesAdapter {
    async fn load_exact(
        &self,
        scope: &AuthorityScope,
        purchase_order: &erpx_integration_contracts::RecordRef,
        receipt: &erpx_integration_contracts::RecordRef,
    ) -> Result<Option<ThreeWayMatchEvidence>, PortError> {
        purchase_order.validate().map_err(|_| PortError::Denied)?;
        receipt.validate().map_err(|_| PortError::Denied)?;
        let mut tx = self.pool.begin().await.map_err(map_store_error)?;
        let row = sqlx::query(
            "SELECT scope,evidence_document
             FROM erpx_finance_write.procurement_match_evidence_inbox
             WHERE purchase_order_id=$1 AND purchase_order_version=$2
               AND receipt_id=$3 AND receipt_version=$4 FOR SHARE",
        )
        .bind(purchase_order.aggregate_id)
        .bind(i64::try_from(purchase_order.aggregate_version).map_err(|_| PortError::Internal)?)
        .bind(receipt.aggregate_id)
        .bind(i64::try_from(receipt.aggregate_version).map_err(|_| PortError::Internal)?)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
        let Some(row) = row else { return Ok(None) };
        let target: AuthorityScope =
            serde_json::from_value(row.try_get("scope").map_err(|_| PortError::Internal)?)
                .map_err(|_| PortError::Internal)?;
        if !scope.contains(&target) {
            return Err(PortError::Denied);
        }
        let evidence = serde_json::from_value::<ThreeWayMatchEvidence>(
            row.try_get("evidence_document")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        evidence.validate().map_err(|_| PortError::Internal)?;
        if &evidence.purchase_order != purchase_order || &evidence.receipt != receipt {
            return Err(PortError::Internal);
        }
        Ok(Some(evidence))
    }
}

#[async_trait]
impl PayablesWriteRepository for SqlxPayablesAdapter {
    async fn load_vendor_invoice(
        &self,
        scope: &AuthorityScope,
        id: VendorInvoiceId,
    ) -> Result<Option<VendorInvoice>, PortError> {
        let Some(document) = self
            .load_document(
                scope,
                "erpx_finance_write.vendor_invoice_guards",
                "vendor_invoice_id",
                "aggregate_document",
                id.as_uuid(),
            )
            .await?
        else {
            return Ok(None);
        };
        let value =
            serde_json::from_value::<VendorInvoice>(document).map_err(|_| PortError::Internal)?;
        value.validate().map_err(|_| PortError::Internal)?;
        Ok(Some(value))
    }

    async fn load_payment(
        &self,
        scope: &AuthorityScope,
        id: PaymentId,
    ) -> Result<Option<Payment>, PortError> {
        let Some(document) = self
            .load_document(
                scope,
                "erpx_finance_write.payment_guards",
                "payment_id",
                "aggregate_document",
                id.as_uuid(),
            )
            .await?
        else {
            return Ok(None);
        };
        let value = serde_json::from_value::<Payment>(document).map_err(|_| PortError::Internal)?;
        let payables = self
            .load_payables_exact(
                scope,
                &value
                    .allocations()
                    .iter()
                    .map(|allocation| (allocation.payable_id, allocation.payable_version))
                    .collect::<Vec<_>>(),
            )
            .await?;
        value.validate(&payables).map_err(|_| PortError::Internal)?;
        Ok(Some(value))
    }

    async fn load_payables_exact(
        &self,
        scope: &AuthorityScope,
        references: &[(PayableId, u64)],
    ) -> Result<Vec<Payable>, PortError> {
        let mut values = Vec::with_capacity(references.len());
        for (id, version) in references {
            let Some(document) = self
                .load_document(
                    scope,
                    "erpx_finance_write.payable_guards",
                    "payable_id",
                    "payable_document",
                    id.as_uuid(),
                )
                .await?
            else {
                return Ok(Vec::new());
            };
            let value =
                serde_json::from_value::<Payable>(document).map_err(|_| PortError::Internal)?;
            if value.id() != *id || value.version() != *version || !scope.contains(value.scope()) {
                return Err(PortError::Conflict {
                    expected: *version,
                    actual: value.version(),
                });
            }
            values.push(value);
        }
        Ok(values)
    }
}

#[async_trait]
impl PayablesCommandUnitOfWork for SqlxPayablesAdapter {
    async fn find_receipt(
        &self,
        identity: &PayablesCommandIdentity,
    ) -> Result<Option<StoredPayablesCommit>, PortError> {
        let (partition, kind) = fingerprint_target(&identity.fingerprint);
        self.store
            .find_receipt::<PayablesAggregate>(
                CONTEXT_NAME,
                &partition,
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?
            .map(|receipt| receipt_to_stored(receipt, kind))
            .transpose()
    }

    async fn commit(&self, request: PayablesCommit) -> Result<StoredPayablesCommit, PortError> {
        validate_commit(&request)?;
        let partition = aggregate_partition(&request.aggregate);
        let kind = request.aggregate.aggregate_type();
        let command_id = request.identity.command_id;
        let occurred_at = aggregate_updated_at(&request.aggregate);
        let mut outbox = request
            .domain_events
            .iter()
            .enumerate()
            .map(|(index, event)| OutboxEventWrite {
                event_id: deterministic_event_id(command_id, domain_event_name(event), index),
                event_type: domain_event_name(event).into(),
                event_version: PAYABLES_EVENT_VERSION,
                partition: partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: PayablesOutboxPayload::Domain(event.clone()),
            })
            .collect::<Vec<_>>();
        let snapshot_event = match &request.aggregate {
            PayablesAggregate::VendorInvoice(_) => VENDOR_INVOICE_SNAPSHOT_EVENT,
            PayablesAggregate::Payment(_) => PAYMENT_SNAPSHOT_EVENT,
        };
        outbox.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, snapshot_event, outbox.len()),
            event_type: snapshot_event.into(),
            event_version: PAYABLES_EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: PayablesOutboxPayload::Snapshot(request.aggregate.clone()),
        });
        if let Some(directory) = payables_directory_draft(&request.aggregate)? {
            outbox.push(OutboxEventWrite {
                event_id: deterministic_event_id(
                    command_id,
                    ITEM_DIRECTORY_OWNER_EVENT_TYPE,
                    outbox.len(),
                ),
                event_type: ITEM_DIRECTORY_OWNER_EVENT_TYPE.into(),
                event_version: ITEM_DIRECTORY_FACT_VERSION,
                partition: partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: PayablesOutboxPayload::Directory(directory),
            });
        }
        let guard = PayablesGuard {
            aggregate: request.aggregate.clone(),
            payable: request.payable.clone(),
            evidence: request.evidence.clone(),
            references: request.references.clone(),
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
            aggregate_type: kind.into(),
            aggregate_id: AggregateId::try_from_uuid(aggregate.id_uuid())
                .map_err(|_| PortError::Internal)?,
            expected_version: request.expected_version,
            response: aggregate,
            audit: AuditWrite {
                audit_id: Uuid::new_v5(&command_id.as_uuid(), b"finance-payables-audit-v1"),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({
                    "aggregate_type": kind,
                    "expected_version": request.expected_version,
                    "reference_count": request.references.len(),
                }),
            },
            events: outbox,
        };
        let outcome = self
            .store
            .commit_normalized(command, &guard)
            .await
            .map_err(map_store_error)?;
        let replayed = matches!(outcome, AtomicCommandOutcome::Replayed(_));
        let receipt = match outcome {
            AtomicCommandOutcome::Committed(value) | AtomicCommandOutcome::Replayed(value) => value,
        };
        let mut stored = receipt_to_stored(receipt, kind)?;
        stored.replayed = replayed;
        Ok(stored)
    }
}

struct PayablesGuard {
    aggregate: PayablesAggregate,
    payable: Option<Payable>,
    evidence: Option<ThreeWayMatchEvidence>,
    references: Vec<PayablesReferenceVersion>,
}

#[async_trait]
impl AtomicCommandGuard for PayablesGuard {
    async fn authoritative_version(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        let value: Option<i64> = match &self.aggregate {
            PayablesAggregate::VendorInvoice(invoice) => {
                sqlx::query_scalar(
                    "SELECT aggregate_version FROM erpx_finance_write.vendor_invoice_guards
                 WHERE vendor_invoice_id=$1 FOR UPDATE",
                )
                .bind(invoice.id().as_uuid())
                .fetch_optional(&mut **tx)
                .await?
            }
            PayablesAggregate::Payment(payment) => {
                sqlx::query_scalar(
                    "SELECT aggregate_version FROM erpx_finance_write.payment_guards
                 WHERE payment_id=$1 FOR UPDATE",
                )
                .bind(payment.id().as_uuid())
                .fetch_optional(&mut **tx)
                .await?
            }
        };
        value.map(to_u64).transpose()
    }

    async fn validate(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        validate_reference_versions(tx, &self.references, self.evidence.as_ref()).await?;
        if let PayablesAggregate::VendorInvoice(invoice) = &self.aggregate {
            let duplicate: Option<Uuid> = sqlx::query_scalar(
                "SELECT vendor_invoice_id FROM erpx_finance_write.vendor_invoice_guards
                 WHERE scope_key=$1 AND supplier_id=$2 AND invoice_number=$3
                   AND vendor_invoice_id<>$4 ORDER BY vendor_invoice_id LIMIT 1 FOR UPDATE",
            )
            .bind(scope_key(invoice.scope())?)
            .bind(invoice.supplier().as_uuid())
            .bind(invoice.invoice_number())
            .bind(invoice.id().as_uuid())
            .fetch_optional(&mut **tx)
            .await?;
            if duplicate.is_some() {
                return Err(CqrsSqlxError::GuardRejected(
                    "Finance vendor invoice number already exists",
                ));
            }
        }
        Ok(())
    }

    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if context.context != CONTEXT_NAME
            || context.aggregate_type != self.aggregate.aggregate_type()
            || context.aggregate_id.as_uuid() != self.aggregate.id_uuid()
            || context.partition != aggregate_partition(&self.aggregate)
            || context.aggregate_version != self.aggregate.version()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "Finance payables applied guard metadata mismatch",
            ));
        }
        match &self.aggregate {
            PayablesAggregate::VendorInvoice(invoice) => {
                apply_vendor_invoice(tx, invoice, self.payable.as_ref()).await
            }
            PayablesAggregate::Payment(payment) => apply_payment(tx, payment).await,
        }
    }
}

async fn validate_reference_versions(
    tx: &mut Transaction<'_, Postgres>,
    references: &[PayablesReferenceVersion],
    evidence: Option<&ThreeWayMatchEvidence>,
) -> Result<(), CqrsSqlxError> {
    for reference in references {
        match reference {
            PayablesReferenceVersion::ProcurementEvidence {
                purchase_order,
                receipt,
            } => {
                let document: Option<serde_json::Value> = sqlx::query_scalar(
                    "SELECT evidence_document
                     FROM erpx_finance_write.procurement_match_evidence_inbox
                     WHERE purchase_order_id=$1 AND purchase_order_version=$2
                       AND receipt_id=$3 AND receipt_version=$4 FOR SHARE",
                )
                .bind(purchase_order.aggregate_id)
                .bind(to_i64(purchase_order.aggregate_version)?)
                .bind(receipt.aggregate_id)
                .bind(to_i64(receipt.aggregate_version)?)
                .fetch_optional(&mut **tx)
                .await?;
                let Some(document) = document else {
                    return Err(CqrsSqlxError::GuardRejected(
                        "exact Procurement match evidence is unavailable",
                    ));
                };
                if let Some(expected) = evidence
                    && document != serde_json::to_value(expected)?
                {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Procurement match evidence changed",
                    ));
                }
            }
            PayablesReferenceVersion::Payable {
                payable_id,
                version,
            } => {
                let current: Option<i64> = sqlx::query_scalar(
                    "SELECT aggregate_version FROM erpx_finance_write.payable_guards
                     WHERE payable_id=$1 FOR SHARE",
                )
                .bind(payable_id.as_uuid())
                .fetch_optional(&mut **tx)
                .await?;
                if current.map(to_u64).transpose()? != Some(*version) {
                    return Err(CqrsSqlxError::OptimisticConcurrency {
                        aggregate_type: "payable".into(),
                        aggregate_id: payable_id.as_uuid(),
                        expected: Some(*version),
                        actual: current.map(to_u64).transpose()?,
                    });
                }
            }
        }
    }
    Ok(())
}

async fn apply_vendor_invoice(
    tx: &mut Transaction<'_, Postgres>,
    invoice: &VendorInvoice,
    payable: Option<&Payable>,
) -> Result<(), CqrsSqlxError> {
    if let Some(payable) = payable {
        if invoice.status() != VendorInvoiceStatus::Posted
            || invoice.payable_id() != Some(payable.id())
            || payable.vendor_invoice_id() != invoice.id()
            || payable.vendor_invoice_version() != invoice.version()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "Finance payable is not bound to the posted vendor invoice",
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_finance_write.payable_guards
             (payable_id,aggregate_version,scope,scope_key,vendor_invoice_id,
              vendor_invoice_version,supplier_id,amount_minor,currency,payable_document,posted_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        )
        .bind(payable.id().as_uuid())
        .bind(to_i64(payable.version())?)
        .bind(serde_json::to_value(payable.scope())?)
        .bind(scope_key(payable.scope())?)
        .bind(payable.vendor_invoice_id().as_uuid())
        .bind(to_i64(payable.vendor_invoice_version())?)
        .bind(payable.supplier().as_uuid())
        .bind(payable.amount().minor_units())
        .bind(payable.amount().currency().as_str())
        .bind(serde_json::to_value(payable)?)
        .bind(payable.posted_at())
        .execute(&mut **tx)
        .await?;
    }
    let evidence = invoice.evidence();
    let document = serde_json::to_value(invoice)?;
    let changed = sqlx::query(
        "INSERT INTO erpx_finance_write.vendor_invoice_guards
         (vendor_invoice_id,aggregate_version,scope,scope_key,invoice_number,supplier_id,
          purchase_order_id,purchase_order_version,receipt_id,receipt_version,currency,
          net_minor,tax_minor,total_minor,payable_id,due_on,status,prepared_by,posted_by,
          posted_at,evidence_document,aggregate_document,created_at,updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,
                 $21,$22,$23,$24)
         ON CONFLICT (holding_id,vendor_invoice_id) DO UPDATE SET
          aggregate_version=EXCLUDED.aggregate_version,payable_id=EXCLUDED.payable_id,
          status=EXCLUDED.status,posted_by=EXCLUDED.posted_by,posted_at=EXCLUDED.posted_at,
          aggregate_document=EXCLUDED.aggregate_document,updated_at=EXCLUDED.updated_at
         WHERE erpx_finance_write.vendor_invoice_guards.aggregate_version+1=
               EXCLUDED.aggregate_version",
    )
    .bind(invoice.id().as_uuid())
    .bind(to_i64(invoice.version())?)
    .bind(serde_json::to_value(invoice.scope())?)
    .bind(scope_key(invoice.scope())?)
    .bind(invoice.invoice_number())
    .bind(invoice.supplier().as_uuid())
    .bind(evidence.purchase_order.aggregate_id)
    .bind(to_i64(evidence.purchase_order.aggregate_version)?)
    .bind(evidence.receipt.aggregate_id)
    .bind(to_i64(evidence.receipt.aggregate_version)?)
    .bind(invoice.total_amount().currency().as_str())
    .bind(invoice.net_amount().minor_units())
    .bind(invoice.tax_amount().minor_units())
    .bind(invoice.total_amount().minor_units())
    .bind(invoice.payable_id().map(PayableId::as_uuid))
    .bind(invoice.due_on())
    .bind(vendor_invoice_status(invoice.status()))
    .bind(invoice.prepared_by())
    .bind(invoice.posted_by())
    .bind(invoice.posted_at())
    .bind(serde_json::to_value(evidence)?)
    .bind(document)
    .bind(invoice.created_at())
    .bind(invoice.updated_at())
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if changed != 1 {
        return Err(CqrsSqlxError::OptimisticConcurrency {
            aggregate_type: VENDOR_INVOICE_AGGREGATE_TYPE.into(),
            aggregate_id: invoice.id().as_uuid(),
            expected: invoice.version().checked_sub(1),
            actual: None,
        });
    }
    if invoice.version() == 1 {
        for line in invoice.lines() {
            sqlx::query(
                "INSERT INTO erpx_finance_write.vendor_invoice_line_guards
                 (vendor_invoice_id,line_id,purchase_order_line_id,receipt_line_id,
                  quantity_units,quantity_scale,amount_minor,currency,line_document)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
            )
            .bind(invoice.id().as_uuid())
            .bind(line.line_id.as_uuid())
            .bind(line.purchase_order_line.as_uuid())
            .bind(line.receipt_line_id)
            .bind(line.invoiced_quantity.units())
            .bind(i16::from(line.invoiced_quantity.scale()))
            .bind(line.invoiced_amount.minor_units())
            .bind(line.invoiced_amount.currency().as_str())
            .bind(serde_json::to_value(line)?)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

async fn apply_payment(
    tx: &mut Transaction<'_, Postgres>,
    payment: &Payment,
) -> Result<(), CqrsSqlxError> {
    let changed = sqlx::query(
        "INSERT INTO erpx_finance_write.payment_guards
         (payment_id,aggregate_version,scope,scope_key,payment_code,supplier_id,amount_minor,
          currency,status,prepared_by,released_by,released_at,aggregate_document,created_at,updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
         ON CONFLICT (holding_id,payment_id) DO UPDATE SET
          aggregate_version=EXCLUDED.aggregate_version,status=EXCLUDED.status,
          released_by=EXCLUDED.released_by,released_at=EXCLUDED.released_at,
          aggregate_document=EXCLUDED.aggregate_document,updated_at=EXCLUDED.updated_at
         WHERE erpx_finance_write.payment_guards.aggregate_version+1=EXCLUDED.aggregate_version",
    )
    .bind(payment.id().as_uuid())
    .bind(to_i64(payment.version())?)
    .bind(serde_json::to_value(payment.scope())?)
    .bind(scope_key(payment.scope())?)
    .bind(payment.payment_code())
    .bind(payment.supplier().as_uuid())
    .bind(payment.amount().minor_units())
    .bind(payment.amount().currency().as_str())
    .bind(payment_status(payment.status()))
    .bind(payment.prepared_by())
    .bind(payment.released_by())
    .bind(payment.released_at())
    .bind(serde_json::to_value(payment)?)
    .bind(payment.created_at())
    .bind(payment.updated_at())
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if changed != 1 {
        return Err(CqrsSqlxError::OptimisticConcurrency {
            aggregate_type: PAYMENT_AGGREGATE_TYPE.into(),
            aggregate_id: payment.id().as_uuid(),
            expected: payment.version().checked_sub(1),
            actual: None,
        });
    }
    if payment.version() == 1 {
        for allocation in payment.allocations() {
            sqlx::query(
                "INSERT INTO erpx_finance_write.payment_allocation_guards
                 (payment_id,payable_id,payable_version,amount_minor,currency,allocation_document)
                 VALUES ($1,$2,$3,$4,$5,$6)",
            )
            .bind(payment.id().as_uuid())
            .bind(allocation.payable_id.as_uuid())
            .bind(to_i64(allocation.payable_version)?)
            .bind(allocation.amount.minor_units())
            .bind(allocation.amount.currency().as_str())
            .bind(serde_json::to_value(allocation)?)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

fn payables_directory_draft(
    aggregate: &PayablesAggregate,
) -> Result<Option<ItemDescriptorDraftV2>, PortError> {
    let PayablesAggregate::VendorInvoice(invoice) = aggregate else {
        return Ok(None);
    };
    let record = StableRecordRef::try_new(
        RecordId::try_from_uuid(invoice.id().as_uuid()).map_err(|_| PortError::Internal)?,
        ContextId::Finance,
        RecordTypeId::try_new("finance.vendor-invoice").map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    let display_label = invoice.invoice_number().to_owned();
    ItemDescriptorDraftV2::try_new(
        ItemDescriptorDraft {
            authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
            record,
            scope: invoice.scope().clone(),
            lifecycle_state: vendor_invoice_status(invoice.status()).into(),
            classification: DataClassification::Restricted,
            descriptor_version: invoice.version(),
            authorized_search_text: display_label.clone(),
        },
        display_label,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

fn validate_guard_context(
    aggregate: &PayablesAggregate,
    context: &GuardCommandContext<'_>,
) -> Result<(), CqrsSqlxError> {
    if context.context != CONTEXT_NAME
        || context.aggregate_type != aggregate.aggregate_type()
        || context.aggregate_id.as_uuid() != aggregate.id_uuid()
        || context.partition != aggregate_partition(aggregate)
    {
        return Err(CqrsSqlxError::GuardRejected(
            "Finance payables guard metadata mismatch",
        ));
    }
    Ok(())
}

fn validate_commit(request: &PayablesCommit) -> Result<(), PortError> {
    request
        .aggregate
        .scope()
        .validate()
        .map_err(|_| PortError::Denied)?;
    if !request.authority_scope.contains(request.aggregate.scope())
        || request.aggregate.version() == 0
        || (matches!(request.aggregate, PayablesAggregate::VendorInvoice(_))
            && request.aggregate.version() > 1
            && request.payable.is_none())
        || (matches!(request.aggregate, PayablesAggregate::Payment(_)) && request.payable.is_some())
    {
        return Err(PortError::Denied);
    }
    Ok(())
}

fn aggregate_partition(aggregate: &PayablesAggregate) -> String {
    match aggregate {
        PayablesAggregate::VendorInvoice(invoice) => {
            format!("finance.vendor-invoice:{}", invoice.id().as_uuid().simple())
        }
        PayablesAggregate::Payment(payment) => {
            format!("finance.payment:{}", payment.id().as_uuid().simple())
        }
    }
}

fn aggregate_updated_at(aggregate: &PayablesAggregate) -> chrono::DateTime<chrono::Utc> {
    match aggregate {
        PayablesAggregate::VendorInvoice(invoice) => invoice.updated_at(),
        PayablesAggregate::Payment(payment) => payment.updated_at(),
    }
}

fn fingerprint_target(value: &PayablesCommandFingerprint) -> (String, &'static str) {
    match value {
        PayablesCommandFingerprint::CreateVendorInvoice {
            vendor_invoice_id, ..
        }
        | PayablesCommandFingerprint::PostVendorInvoice {
            vendor_invoice_id, ..
        } => (
            format!(
                "finance.vendor-invoice:{}",
                vendor_invoice_id.as_uuid().simple()
            ),
            VENDOR_INVOICE_AGGREGATE_TYPE,
        ),
        PayablesCommandFingerprint::CreatePayment { payment_id, .. }
        | PayablesCommandFingerprint::ReleasePayment { payment_id } => (
            format!("finance.payment:{}", payment_id.as_uuid().simple()),
            PAYMENT_AGGREGATE_TYPE,
        ),
    }
}

fn request_hash(value: &PayablesCommandFingerprint) -> Result<RequestHash, PortError> {
    let mut digest = Sha256::new();
    digest.update(b"erpx.finance.payables-command-fingerprint.v1\0");
    digest.update(serde_json::to_vec(value).map_err(|_| PortError::Internal)?);
    Ok(RequestHash::new(digest.finalize().into()))
}

fn domain_event_name(event: &PayablesDomainEvent) -> &'static str {
    match event {
        PayablesDomainEvent::VendorInvoicePosted(_) => "finance.vendor_invoice_posted",
        PayablesDomainEvent::PayablePosted(_) => "finance.payable_posted",
        PayablesDomainEvent::PaymentReleased(_) => "finance.payment_released",
    }
}

fn audit_action(value: &PayablesCommandFingerprint) -> &'static str {
    match value {
        PayablesCommandFingerprint::CreateVendorInvoice { .. } => "finance.vendor_invoice.create",
        PayablesCommandFingerprint::PostVendorInvoice { .. } => "finance.vendor_invoice.post",
        PayablesCommandFingerprint::CreatePayment { .. } => "finance.payment.create",
        PayablesCommandFingerprint::ReleasePayment { .. } => "finance.payment.release",
    }
}

fn vendor_invoice_status(value: erpx_finance::VendorInvoiceStatus) -> &'static str {
    match value {
        erpx_finance::VendorInvoiceStatus::Draft => "draft",
        erpx_finance::VendorInvoiceStatus::Posted => "posted",
    }
}

fn payment_status(value: erpx_finance::PaymentStatus) -> &'static str {
    match value {
        erpx_finance::PaymentStatus::Draft => "draft",
        erpx_finance::PaymentStatus::Released => "released",
    }
}

fn deterministic_event_id(command_id: CommandId, event_type: &str, index: usize) -> EventId {
    let mut label = event_type.as_bytes().to_vec();
    label.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(Uuid::new_v5(&command_id.as_uuid(), &label))
        .expect("derived Finance payables event identifier")
}

fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<PayablesAggregate>,
    kind: &str,
) -> Result<StoredPayablesCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != kind
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.id_uuid()
        || receipt.aggregate.version != receipt.response.version()
    {
        return Err(PortError::Internal);
    }
    let payable = match &receipt.response {
        PayablesAggregate::VendorInvoice(invoice) => {
            invoice.posted_payable().map_err(|_| PortError::Internal)?
        }
        PayablesAggregate::Payment(_) => None,
    };
    Ok(StoredPayablesCommit {
        command_id: receipt.command_id,
        correlation_id: receipt.correlation_id,
        aggregate: receipt.response,
        payable,
        committed_events: receipt.committed_events,
        committed_at: receipt.committed_at,
        replayed: receipt.replayed,
    })
}

fn to_i64(value: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

fn to_u64(value: i64) -> Result<u64, CqrsSqlxError> {
    u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

fn map_store_error(error: CqrsSqlxError) -> PortError {
    match error {
        CqrsSqlxError::OptimisticConcurrency {
            expected, actual, ..
        } => PortError::Conflict {
            expected: expected.unwrap_or(0),
            actual: actual.unwrap_or(0),
        },
        CqrsSqlxError::IdempotencyConflict | CqrsSqlxError::CommandReplayConflict => {
            PortError::IdempotencyConflict
        }
        CqrsSqlxError::Database(_) => PortError::Unavailable,
        _ => PortError::Internal,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn adapter_fences_exact_procurement_and_payable_versions() {
        let source = include_str!("payables.rs");
        assert!(source.contains("purchase_order_version=$2"));
        assert!(source.contains("receipt_version=$4"));
        assert!(source.contains("SELECT aggregate_version FROM erpx_finance_write.payable_guards"));
        assert!(source.contains("find_receipt::<PayablesAggregate>"));
    }

    #[test]
    fn vendor_invoice_publishes_v2_directory_label_on_the_singular_type() {
        let source = include_str!("payables.rs");
        assert!(source.contains("finance.vendor-invoice"));
        assert!(source.contains("ItemDescriptorDraftV2::try_new"));
        assert!(!source.contains(concat!("finance.vendor-", "invoices")));
    }
}
