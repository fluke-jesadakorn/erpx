use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::{
    DimensionSet, ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, IntegrationEvent,
    ItemDescriptorDraft, ItemDescriptorDraftV2, RecordTypeId, StableRecordRef, assets,
    catalog::ContextId, crm, customer, legal, marketing, organization, people_payroll,
    record_policy_id, sales,
};
use erpx_kernel::{AggregateId, AuthorityScope, CommandId, DataClassification, EventId, RecordId};
use erpx_sales::{
    AcceptedFulfillment, BillingEventId, CommandUnitOfWork, FulfillmentId, PerformanceObligation,
    PerformanceObligationId, PortError, Quote, QuoteId, QuoteStatus, RevenueArrangement,
    RevenueArrangementId, RevenueArrangementStatus, SalesAggregate, SalesCommit, SalesOrder,
    SalesOrderId, SalesOrderLine, SalesOrderLineId, SalesOrderStatus, StoredCommit,
    WriteRepository,
    ports::{CommandFingerprint, CommandIdentity},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    ARRANGEMENT_AGGREGATE_TYPE, ARRANGEMENT_SNAPSHOT_EVENT, CONTEXT_NAME, EVENT_VERSION,
    ORDER_AGGREGATE_TYPE, ORDER_SNAPSHOT_EVENT, QUOTE_AGGREGATE_TYPE, QUOTE_SNAPSHOT_EVENT,
    arrangement_partition, order_partition,
    projection::{
        SalesOutboxPayload, SalesSnapshot, arrangement_status_name, order_status_name,
        quote_status_name,
    },
    quote_partition,
};

#[derive(Debug, Clone)]
pub struct SalesWriteRepository {
    pool: WritePool,
}

impl SalesWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self { pool }
    }

    async fn load(
        &self,
        authority: &AuthorityScope,
        partition: &str,
        kind: &str,
        id: Uuid,
    ) -> Result<Option<SalesAggregate>, PortError> {
        let _ = partition;
        load_sales_relational(&self.pool, authority, kind, id).await
    }
}

async fn load_sales_relational(
    pool: &WritePool,
    authority: &AuthorityScope,
    kind: &str,
    id: Uuid,
) -> Result<Option<SalesAggregate>, PortError> {
    authority.validate().map_err(|_| PortError::Denied)?;
    let mut tx = pool.begin().await.map_err(map_store_error)?;
    match kind {
        QUOTE_AGGREGATE_TYPE => load_quote_relational(&mut tx, authority, id)
            .await
            .map(|value| value.map(SalesAggregate::Quote)),
        ORDER_AGGREGATE_TYPE => load_order_relational(&mut tx, authority, id)
            .await
            .map(|value| value.map(SalesAggregate::SalesOrder)),
        ARRANGEMENT_AGGREGATE_TYPE => load_arrangement_relational(&mut tx, authority, id)
            .await
            .map(|value| value.map(|item| SalesAggregate::RevenueArrangement(Box::new(item)))),
        _ => Err(PortError::Internal),
    }
}

async fn load_quote_relational(
    tx: &mut Transaction<'_, Postgres>,
    authority: &AuthorityScope,
    id: Uuid,
) -> Result<Option<Quote>, PortError> {
    let row = sales_authorized_root(
        tx,
        authority,
        "SELECT aggregate_version,holding_id,company_id,branch_id,opportunity_id,customer_id,
                quote_code,currency,valid_until,quote_status,created_at,updated_at
         FROM erpx_sales_write.quote_guards WHERE quote_id=$1",
        "erpx_sales_write.quote_guards",
        "quote_id",
        id,
    )
    .await?;
    let Some(row) = row else { return Ok(None) };
    let lines =
        load_sales_lines(tx, "erpx_sales_write.quote_line_authority", "quote_id", id).await?;
    Quote::try_rehydrate(
        QuoteId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
        row_u64(&row, "aggregate_version")?,
        sales_exact_scope(&row)?,
        crm::OpportunityRef::try_from_uuid(
            row.try_get("opportunity_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        customer::CustomerRef::try_from_uuid(
            row.try_get("customer_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        row.try_get("quote_code").map_err(|_| PortError::Internal)?,
        row.try_get("currency").map_err(|_| PortError::Internal)?,
        lines,
        row.try_get("valid_until")
            .map_err(|_| PortError::Internal)?,
        parse_quote_status(
            row.try_get("quote_status")
                .map_err(|_| PortError::Internal)?,
        )?,
        row.try_get("created_at").map_err(|_| PortError::Internal)?,
        row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn load_order_relational(
    tx: &mut Transaction<'_, Postgres>,
    authority: &AuthorityScope,
    id: Uuid,
) -> Result<Option<SalesOrder>, PortError> {
    let row = sales_authorized_root(
        tx,
        authority,
        "SELECT aggregate_version,holding_id,company_id,branch_id,quote_id,opportunity_id,
                customer_id,order_code,currency,order_status,created_at,updated_at
         FROM erpx_sales_write.sales_order_guards WHERE sales_order_id=$1",
        "erpx_sales_write.sales_order_guards",
        "sales_order_id",
        id,
    )
    .await?;
    let Some(row) = row else { return Ok(None) };
    let lines = load_sales_lines(
        tx,
        "erpx_sales_write.sales_order_line_authority",
        "sales_order_id",
        id,
    )
    .await?;
    SalesOrder::try_rehydrate(
        SalesOrderId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
        row_u64(&row, "aggregate_version")?,
        sales_exact_scope(&row)?,
        QuoteId::try_from_uuid(row.try_get("quote_id").map_err(|_| PortError::Internal)?)
            .map_err(|_| PortError::Internal)?,
        crm::OpportunityRef::try_from_uuid(
            row.try_get("opportunity_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        customer::CustomerRef::try_from_uuid(
            row.try_get("customer_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        row.try_get("order_code").map_err(|_| PortError::Internal)?,
        row.try_get("currency").map_err(|_| PortError::Internal)?,
        lines,
        parse_order_status(
            row.try_get("order_status")
                .map_err(|_| PortError::Internal)?,
        )?,
        row.try_get("created_at").map_err(|_| PortError::Internal)?,
        row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn load_arrangement_relational(
    tx: &mut Transaction<'_, Postgres>,
    authority: &AuthorityScope,
    id: Uuid,
) -> Result<Option<RevenueArrangement>, PortError> {
    let row = sales_authorized_root(
        tx,
        authority,
        "SELECT aggregate_version,holding_id,company_id,branch_id,sales_order_id,customer_id,
                legal_contract_id,order_code,currency,arrangement_status,created_at,updated_at
         FROM erpx_sales_write.revenue_arrangement_guards WHERE arrangement_id=$1",
        "erpx_sales_write.revenue_arrangement_guards",
        "arrangement_id",
        id,
    )
    .await?;
    let Some(row) = row else { return Ok(None) };
    let dimensions_row = sqlx::query(
        "SELECT * FROM erpx_sales_write.arrangement_dimension_authority WHERE arrangement_id=$1",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| PortError::Unavailable)?
    .ok_or(PortError::Internal)?;
    let dimensions = dimensions_from_row(&dimensions_row)?;
    let obligation_rows = sqlx::query("SELECT obligation_id,line_id,revenue_model,allocated_minor,allocated_tax_minor,accepted_minor,accepted_tax_minor,service_starts_on,service_ends_on FROM erpx_sales_write.arrangement_obligation_authority WHERE arrangement_id=$1 ORDER BY obligation_id")
        .bind(id).fetch_all(&mut **tx).await.map_err(|_|PortError::Unavailable)?;
    let obligations = obligation_rows
        .into_iter()
        .map(|item| {
            Ok(PerformanceObligation::try_rehydrate(
                PerformanceObligationId::try_from_uuid(
                    item.try_get("obligation_id")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                SalesOrderLineId::try_from_uuid(
                    item.try_get("line_id").map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                parse_revenue_model(
                    item.try_get("revenue_model")
                        .map_err(|_| PortError::Internal)?,
                )?,
                item.try_get("allocated_minor")
                    .map_err(|_| PortError::Internal)?,
                item.try_get("allocated_tax_minor")
                    .map_err(|_| PortError::Internal)?,
                item.try_get("accepted_minor")
                    .map_err(|_| PortError::Internal)?,
                item.try_get("accepted_tax_minor")
                    .map_err(|_| PortError::Internal)?,
                item.try_get("service_starts_on")
                    .map_err(|_| PortError::Internal)?,
                item.try_get("service_ends_on")
                    .map_err(|_| PortError::Internal)?,
            ))
        })
        .collect::<Result<Vec<_>, PortError>>()?;
    let fulfillment_rows = sqlx::query("SELECT fulfillment_id,billing_event_id,obligation_id,fulfillment_kind,accepted_minor,accepted_tax_minor,accepted_at,eligible_on FROM erpx_sales_write.arrangement_fulfillment_authority WHERE arrangement_id=$1 ORDER BY accepted_at,fulfillment_id")
        .bind(id).fetch_all(&mut **tx).await.map_err(|_|PortError::Unavailable)?;
    let fulfillments = fulfillment_rows
        .into_iter()
        .map(|item| {
            Ok(AcceptedFulfillment::try_rehydrate(
                FulfillmentId::try_from_uuid(
                    item.try_get("fulfillment_id")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                BillingEventId::try_from_uuid(
                    item.try_get("billing_event_id")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                PerformanceObligationId::try_from_uuid(
                    item.try_get("obligation_id")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                parse_fulfillment_kind(
                    item.try_get("fulfillment_kind")
                        .map_err(|_| PortError::Internal)?,
                )?,
                item.try_get("accepted_minor")
                    .map_err(|_| PortError::Internal)?,
                item.try_get("accepted_tax_minor")
                    .map_err(|_| PortError::Internal)?,
                item.try_get("accepted_at")
                    .map_err(|_| PortError::Internal)?,
                item.try_get("eligible_on")
                    .map_err(|_| PortError::Internal)?,
            ))
        })
        .collect::<Result<Vec<_>, PortError>>()?;
    RevenueArrangement::try_rehydrate(
        RevenueArrangementId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
        row_u64(&row, "aggregate_version")?,
        sales_exact_scope(&row)?,
        SalesOrderId::try_from_uuid(
            row.try_get("sales_order_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        customer::CustomerRef::try_from_uuid(
            row.try_get("customer_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        row.try_get::<Option<Uuid>, _>("legal_contract_id")
            .map_err(|_| PortError::Internal)?
            .map(legal::LegalContractRef::try_from_uuid)
            .transpose()
            .map_err(|_| PortError::Internal)?,
        row.try_get("order_code").map_err(|_| PortError::Internal)?,
        row.try_get("currency").map_err(|_| PortError::Internal)?,
        dimensions,
        obligations,
        fulfillments,
        parse_arrangement_status(
            row.try_get("arrangement_status")
                .map_err(|_| PortError::Internal)?,
        )?,
        row.try_get("created_at").map_err(|_| PortError::Internal)?,
        row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn load_sales_lines(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    id_column: &str,
    id: Uuid,
) -> Result<Vec<SalesOrderLine>, PortError> {
    let query = format!(
        "SELECT line_id,item_code,description,quantity,unit_price_minor,tax_minor,currency FROM {table} WHERE {id_column}=$1 ORDER BY line_id"
    );
    sqlx::query(&query)
        .bind(id)
        .fetch_all(&mut **tx)
        .await
        .map_err(|_| PortError::Unavailable)?
        .into_iter()
        .map(|line| {
            Ok(SalesOrderLine {
                line_id: SalesOrderLineId::try_from_uuid(
                    line.try_get("line_id").map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                item_code: line.try_get("item_code").map_err(|_| PortError::Internal)?,
                description: line
                    .try_get("description")
                    .map_err(|_| PortError::Internal)?,
                quantity: line.try_get("quantity").map_err(|_| PortError::Internal)?,
                unit_price_minor: line
                    .try_get("unit_price_minor")
                    .map_err(|_| PortError::Internal)?,
                tax_minor: line.try_get("tax_minor").map_err(|_| PortError::Internal)?,
                currency: line.try_get("currency").map_err(|_| PortError::Internal)?,
            })
        })
        .collect()
}

#[async_trait]
impl WriteRepository for SalesWriteRepository {
    async fn load_quote(
        &self,
        authority_scope: &AuthorityScope,
        quote_id: QuoteId,
    ) -> Result<Option<Quote>, PortError> {
        match self
            .load(
                authority_scope,
                &quote_partition(quote_id),
                QUOTE_AGGREGATE_TYPE,
                quote_id.as_uuid(),
            )
            .await?
        {
            Some(SalesAggregate::Quote(value)) => Ok(Some(value)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }

    async fn load_order(
        &self,
        authority_scope: &AuthorityScope,
        sales_order_id: SalesOrderId,
    ) -> Result<Option<SalesOrder>, PortError> {
        match self
            .load(
                authority_scope,
                &order_partition(sales_order_id),
                ORDER_AGGREGATE_TYPE,
                sales_order_id.as_uuid(),
            )
            .await?
        {
            Some(SalesAggregate::SalesOrder(value)) => Ok(Some(value)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }

    async fn load_revenue_arrangement(
        &self,
        authority_scope: &AuthorityScope,
        arrangement_id: RevenueArrangementId,
    ) -> Result<Option<RevenueArrangement>, PortError> {
        match self
            .load(
                authority_scope,
                &arrangement_partition(arrangement_id),
                ARRANGEMENT_AGGREGATE_TYPE,
                arrangement_id.as_uuid(),
            )
            .await?
        {
            Some(SalesAggregate::RevenueArrangement(value)) => Ok(Some(*value)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SalesCommandAdapter {
    store: CommandStore,
}

impl SalesCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl CommandUnitOfWork for SalesCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        let (partition, kind) = fingerprint_target(&identity.fingerprint);
        self.store
            .find_receipt::<SalesAggregate>(
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

    async fn commit(&self, request: SalesCommit) -> Result<StoredCommit, PortError> {
        validate_commit(&request)?;
        let partition = aggregate_partition(&request.aggregate);
        let kind = request.aggregate.aggregate_type();
        let id = request.aggregate.id_uuid();
        let occurred_at = aggregate_updated_at(&request.aggregate);
        let snapshot_event = match &request.aggregate {
            SalesAggregate::Quote(_) => QUOTE_SNAPSHOT_EVENT,
            SalesAggregate::SalesOrder(_) => ORDER_SNAPSHOT_EVENT,
            SalesAggregate::RevenueArrangement(_) => ARRANGEMENT_SNAPSHOT_EVENT,
        };
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
                payload: SalesOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, snapshot_event, events.len()),
            event_type: snapshot_event.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: SalesOutboxPayload::Snapshot(SalesSnapshot::from(&request.aggregate)),
        });
        if let Some(directory_draft) = sales_directory_draft(&request.aggregate)? {
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
                payload: SalesOutboxPayload::Directory(directory_draft),
            });
        }
        let guard = SalesGuard(request.aggregate.clone());
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
            aggregate_id: AggregateId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
            expected_version: request.expected_version,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"sales-audit-v1"),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({
                    "aggregate_type": kind,
                    "aggregate_id": id,
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
            AtomicCommandOutcome::Committed(value) | AtomicCommandOutcome::Replayed(value) => value,
        };
        let mut stored = receipt_to_stored(receipt, kind)?;
        stored.replayed = replayed;
        Ok(stored)
    }
}

fn sales_directory_draft(
    aggregate: &SalesAggregate,
) -> Result<Option<ItemDescriptorDraftV2>, PortError> {
    let (record_type, lifecycle_state, display_label) = match aggregate {
        SalesAggregate::Quote(value) => (
            "sales.quote",
            quote_status_name(value.status()),
            value.quote_code().to_owned(),
        ),
        SalesAggregate::SalesOrder(value) => (
            "sales.order",
            order_status_name(value.status()),
            value.order_code().to_owned(),
        ),
        // Revenue arrangements are an accounting handoff aggregate rather
        // than a separately routed first-class Sales record.
        SalesAggregate::RevenueArrangement(_) => return Ok(None),
    };
    let record_id =
        RecordId::try_from_uuid(aggregate.id_uuid()).map_err(|_| PortError::Internal)?;
    let record = StableRecordRef::try_new(
        record_id,
        ContextId::Sales,
        RecordTypeId::try_new(record_type).map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    let value = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
        record,
        scope: aggregate.scope().clone(),
        lifecycle_state: lifecycle_state.into(),
        classification: DataClassification::Confidential,
        descriptor_version: aggregate.version(),
        authorized_search_text: display_label.clone(),
    };
    ItemDescriptorDraftV2::try_new(value, display_label)
        .map(Some)
        .map_err(|_| PortError::Internal)
}

struct SalesGuard(SalesAggregate);

#[async_trait]
impl AtomicCommandGuard for SalesGuard {
    async fn authoritative_version(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        validate_guard_context(&self.0, context)?;
        let current: Option<i64> = match &self.0 {
            SalesAggregate::Quote(value) => sqlx::query_scalar("SELECT aggregate_version FROM erpx_sales_write.quote_guards WHERE quote_id=$1 FOR UPDATE").bind(value.id().as_uuid()).fetch_optional(&mut **transaction).await?,
            SalesAggregate::SalesOrder(value) => sqlx::query_scalar("SELECT aggregate_version FROM erpx_sales_write.sales_order_guards WHERE sales_order_id=$1 FOR UPDATE").bind(value.id().as_uuid()).fetch_optional(&mut **transaction).await?,
            SalesAggregate::RevenueArrangement(value) => sqlx::query_scalar("SELECT aggregate_version FROM erpx_sales_write.revenue_arrangement_guards WHERE arrangement_id=$1 FOR UPDATE").bind(value.id().as_uuid()).fetch_optional(&mut **transaction).await?,
        };
        current
            .map(|value| u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange))
            .transpose()
    }

    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(&self.0, context)?;
        match &self.0 {
            SalesAggregate::Quote(value) => validate_quote(transaction, context, value).await,
            SalesAggregate::SalesOrder(value) => validate_order(transaction, context, value).await,
            SalesAggregate::RevenueArrangement(value) => {
                validate_arrangement(transaction, context, value).await
            }
        }
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if context.context != CONTEXT_NAME
            || context.aggregate_type != self.0.aggregate_type()
            || context.aggregate_id.as_uuid() != self.0.id_uuid()
            || context.partition != aggregate_partition(&self.0)
            || context.aggregate_version != self.0.version()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "Sales applied guard metadata mismatch",
            ));
        }
        match &self.0 {
            SalesAggregate::Quote(value) => apply_quote(transaction, value).await,
            SalesAggregate::SalesOrder(value) => apply_order(transaction, value).await,
            SalesAggregate::RevenueArrangement(value) => {
                apply_arrangement(transaction, value).await
            }
        }
    }
}

async fn validate_quote(
    transaction: &mut Transaction<'_, Postgres>,
    context: &GuardCommandContext<'_>,
    value: &Quote,
) -> Result<(), CqrsSqlxError> {
    let current: Option<i64> = sqlx::query_scalar(
        "SELECT aggregate_version FROM erpx_sales_write.quote_guards
         WHERE quote_id=$1 FOR UPDATE",
    )
    .bind(value.id().as_uuid())
    .fetch_optional(&mut **transaction)
    .await?;
    ensure_existence(context.expected_version, current, "Sales quote")?;
    let duplicate: Option<Uuid> = sqlx::query_scalar(
        "SELECT quote_id FROM erpx_sales_write.quote_guards
         WHERE scope_key=$1 AND quote_code=$2 AND quote_id<>$3
         ORDER BY quote_id LIMIT 1 FOR UPDATE",
    )
    .bind(scope_key(value.scope())?)
    .bind(value.quote_code())
    .bind(value.id().as_uuid())
    .fetch_optional(&mut **transaction)
    .await?;
    if duplicate.is_some() {
        return Err(CqrsSqlxError::GuardRejected(
            "Sales quote code already exists",
        ));
    }
    Ok(())
}

async fn validate_order(
    transaction: &mut Transaction<'_, Postgres>,
    context: &GuardCommandContext<'_>,
    value: &SalesOrder,
) -> Result<(), CqrsSqlxError> {
    let current: Option<i64> = sqlx::query_scalar(
        "SELECT aggregate_version FROM erpx_sales_write.sales_order_guards
         WHERE sales_order_id=$1 FOR UPDATE",
    )
    .bind(value.id().as_uuid())
    .fetch_optional(&mut **transaction)
    .await?;
    ensure_existence(context.expected_version, current, "Sales order")?;
    let quote = sqlx::query(
        "SELECT scope_key,opportunity_id,customer_id,currency,total_minor,quote_status
         FROM erpx_sales_write.quote_guards WHERE quote_id=$1 FOR SHARE",
    )
    .bind(value.quote_id().as_uuid())
    .fetch_optional(&mut **transaction)
    .await?;
    let Some(quote) = quote else {
        return Err(CqrsSqlxError::GuardRejected("Sales quote guard is missing"));
    };
    let quote_scope: String = quote.try_get("scope_key")?;
    let opportunity: Uuid = quote.try_get("opportunity_id")?;
    let customer: Uuid = quote.try_get("customer_id")?;
    let currency: String = quote.try_get("currency")?;
    let total: i64 = quote.try_get("total_minor")?;
    let status: String = quote.try_get("quote_status")?;
    if quote_scope != scope_key(value.scope())?
        || opportunity != value.opportunity().as_uuid()
        || customer != value.customer().as_uuid()
        || currency != value.currency()
        || total != value.total_minor()
        || status != "accepted"
    {
        return Err(CqrsSqlxError::GuardRejected(
            "Sales order does not match its accepted quote",
        ));
    }
    let duplicate: Option<Uuid> = sqlx::query_scalar(
        "SELECT sales_order_id FROM erpx_sales_write.sales_order_guards
         WHERE scope_key=$1 AND order_code=$2 AND sales_order_id<>$3
         ORDER BY sales_order_id LIMIT 1 FOR UPDATE",
    )
    .bind(scope_key(value.scope())?)
    .bind(value.order_code())
    .bind(value.id().as_uuid())
    .fetch_optional(&mut **transaction)
    .await?;
    if duplicate.is_some() {
        return Err(CqrsSqlxError::GuardRejected(
            "Sales order code already exists",
        ));
    }
    Ok(())
}

async fn validate_arrangement(
    transaction: &mut Transaction<'_, Postgres>,
    context: &GuardCommandContext<'_>,
    value: &RevenueArrangement,
) -> Result<(), CqrsSqlxError> {
    let current: Option<i64> = sqlx::query_scalar(
        "SELECT aggregate_version FROM erpx_sales_write.revenue_arrangement_guards
         WHERE arrangement_id=$1 FOR UPDATE",
    )
    .bind(value.id().as_uuid())
    .fetch_optional(&mut **transaction)
    .await?;
    ensure_existence(
        context.expected_version,
        current,
        "Sales revenue arrangement",
    )?;
    let order = sqlx::query(
        "SELECT scope_key,customer_id,order_code,currency,order_status
         FROM erpx_sales_write.sales_order_guards WHERE sales_order_id=$1 FOR SHARE",
    )
    .bind(value.sales_order_id().as_uuid())
    .fetch_optional(&mut **transaction)
    .await?;
    let Some(order) = order else {
        return Err(CqrsSqlxError::GuardRejected(
            "Sales order guard for revenue arrangement is missing",
        ));
    };
    let order_scope: String = order.try_get("scope_key")?;
    let customer: Uuid = order.try_get("customer_id")?;
    let order_code: String = order.try_get("order_code")?;
    let currency: String = order.try_get("currency")?;
    let status: String = order.try_get("order_status")?;
    if order_scope != scope_key(value.scope())?
        || customer != value.customer().as_uuid()
        || order_code != value.order_code()
        || currency != value.currency()
        || (current.is_none() && status != "confirmed")
    {
        return Err(CqrsSqlxError::GuardRejected(
            "Sales revenue arrangement does not match its confirmed order",
        ));
    }
    let duplicate: Option<Uuid> = sqlx::query_scalar(
        "SELECT arrangement_id FROM erpx_sales_write.revenue_arrangement_guards
         WHERE sales_order_id=$1 AND arrangement_id<>$2
         ORDER BY arrangement_id LIMIT 1 FOR UPDATE",
    )
    .bind(value.sales_order_id().as_uuid())
    .bind(value.id().as_uuid())
    .fetch_optional(&mut **transaction)
    .await?;
    if duplicate.is_some() {
        return Err(CqrsSqlxError::GuardRejected(
            "Sales order already has a revenue arrangement",
        ));
    }
    Ok(())
}

fn ensure_existence(
    expected: Option<u64>,
    current: Option<i64>,
    kind: &'static str,
) -> Result<(), CqrsSqlxError> {
    if matches!((expected, current), (None, Some(_)) | (Some(_), None)) {
        Err(CqrsSqlxError::GuardRejected(match kind {
            "Sales quote" => "Sales quote guard existence mismatch",
            "Sales order" => "Sales order guard existence mismatch",
            _ => "Sales revenue arrangement guard existence mismatch",
        }))
    } else {
        Ok(())
    }
}

async fn apply_quote(
    transaction: &mut Transaction<'_, Postgres>,
    value: &Quote,
) -> Result<(), CqrsSqlxError> {
    let changed = sqlx::query(
        "INSERT INTO erpx_sales_write.quote_guards
          (quote_id,scope_key,opportunity_id,customer_id,quote_code,currency,total_minor,
           valid_until,quote_status,aggregate_version,created_at,updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
         ON CONFLICT (quote_id) DO UPDATE SET
          scope_key=EXCLUDED.scope_key,opportunity_id=EXCLUDED.opportunity_id,
          customer_id=EXCLUDED.customer_id,quote_code=EXCLUDED.quote_code,
          currency=EXCLUDED.currency,total_minor=EXCLUDED.total_minor,
          valid_until=EXCLUDED.valid_until,quote_status=EXCLUDED.quote_status,
          aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at
         WHERE erpx_sales_write.quote_guards.aggregate_version + 1=EXCLUDED.aggregate_version",
    )
    .bind(value.id().as_uuid())
    .bind(scope_key(value.scope())?)
    .bind(value.opportunity().as_uuid())
    .bind(value.customer().as_uuid())
    .bind(value.quote_code())
    .bind(value.currency())
    .bind(value.total_minor())
    .bind(value.valid_until())
    .bind(quote_status_name(value.status()))
    .bind(to_i64(value.version())?)
    .bind(value.created_at())
    .bind(value.updated_at())
    .execute(&mut **transaction)
    .await?
    .rows_affected();
    require_sales_write(changed, "Sales quote write conflict")?;
    write_sales_scope(
        transaction,
        "erpx_sales_write.quote_guards",
        "quote_id",
        value.id().as_uuid(),
        value.scope(),
    )
    .await?;
    sqlx::query("DELETE FROM erpx_sales_write.quote_line_authority WHERE quote_id=$1")
        .bind(value.id().as_uuid())
        .execute(&mut **transaction)
        .await?;
    write_sales_lines(
        transaction,
        "erpx_sales_write.quote_line_authority",
        "quote_id",
        value.id().as_uuid(),
        value.lines(),
    )
    .await?;
    Ok(())
}

async fn apply_order(
    transaction: &mut Transaction<'_, Postgres>,
    value: &SalesOrder,
) -> Result<(), CqrsSqlxError> {
    let changed = sqlx::query(
        "INSERT INTO erpx_sales_write.sales_order_guards
          (sales_order_id,scope_key,quote_id,opportunity_id,customer_id,order_code,currency,
           total_minor,order_status,aggregate_version,created_at,updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
         ON CONFLICT (sales_order_id) DO UPDATE SET
          scope_key=EXCLUDED.scope_key,quote_id=EXCLUDED.quote_id,
          opportunity_id=EXCLUDED.opportunity_id,customer_id=EXCLUDED.customer_id,
          order_code=EXCLUDED.order_code,currency=EXCLUDED.currency,
          total_minor=EXCLUDED.total_minor,order_status=EXCLUDED.order_status,
          aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at
         WHERE erpx_sales_write.sales_order_guards.aggregate_version + 1=EXCLUDED.aggregate_version",
    )
    .bind(value.id().as_uuid())
    .bind(scope_key(value.scope())?)
    .bind(value.quote_id().as_uuid())
    .bind(value.opportunity().as_uuid())
    .bind(value.customer().as_uuid())
    .bind(value.order_code())
    .bind(value.currency())
    .bind(value.total_minor())
    .bind(order_status_name(value.status()))
    .bind(to_i64(value.version())?)
    .bind(value.created_at())
    .bind(value.updated_at())
    .execute(&mut **transaction)
    .await?
    .rows_affected();
    require_sales_write(changed, "Sales order write conflict")?;
    write_sales_scope(
        transaction,
        "erpx_sales_write.sales_order_guards",
        "sales_order_id",
        value.id().as_uuid(),
        value.scope(),
    )
    .await?;
    sqlx::query("DELETE FROM erpx_sales_write.sales_order_line_authority WHERE sales_order_id=$1")
        .bind(value.id().as_uuid())
        .execute(&mut **transaction)
        .await?;
    write_sales_lines(
        transaction,
        "erpx_sales_write.sales_order_line_authority",
        "sales_order_id",
        value.id().as_uuid(),
        value.lines(),
    )
    .await?;
    Ok(())
}

async fn apply_arrangement(
    transaction: &mut Transaction<'_, Postgres>,
    value: &RevenueArrangement,
) -> Result<(), CqrsSqlxError> {
    let dimensions = serde_json::to_value(value.dimensions()).map_err(|_| {
        CqrsSqlxError::GuardRejected("Sales arrangement dimensions cannot be serialized")
    })?;
    let obligations = serde_json::to_value(value.obligations()).map_err(|_| {
        CqrsSqlxError::GuardRejected("Sales arrangement obligations cannot be serialized")
    })?;
    let fulfillments = serde_json::to_value(value.fulfillments()).map_err(|_| {
        CqrsSqlxError::GuardRejected("Sales arrangement fulfillments cannot be serialized")
    })?;
    let changed = sqlx::query(
        "INSERT INTO erpx_sales_write.revenue_arrangement_guards
          (arrangement_id,scope_key,sales_order_id,customer_id,legal_contract_id,order_code,
           currency,arrangement_status,dimensions,obligations,fulfillments,aggregate_version,
           created_at,updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
         ON CONFLICT (arrangement_id) DO UPDATE SET
          scope_key=EXCLUDED.scope_key,sales_order_id=EXCLUDED.sales_order_id,
          customer_id=EXCLUDED.customer_id,legal_contract_id=EXCLUDED.legal_contract_id,
          order_code=EXCLUDED.order_code,currency=EXCLUDED.currency,
          arrangement_status=EXCLUDED.arrangement_status,dimensions=EXCLUDED.dimensions,
          obligations=EXCLUDED.obligations,fulfillments=EXCLUDED.fulfillments,
          aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at
         WHERE erpx_sales_write.revenue_arrangement_guards.aggregate_version + 1=EXCLUDED.aggregate_version",
    )
    .bind(value.id().as_uuid())
    .bind(scope_key(value.scope())?)
    .bind(value.sales_order_id().as_uuid())
    .bind(value.customer().as_uuid())
    .bind(value.legal_contract().map(|reference| reference.as_uuid()))
    .bind(value.order_code())
    .bind(value.currency())
    .bind(arrangement_status_name(value.status()))
    .bind(dimensions)
    .bind(obligations)
    .bind(fulfillments)
    .bind(to_i64(value.version())?)
    .bind(value.created_at())
    .bind(value.updated_at())
    .execute(&mut **transaction)
    .await?
    .rows_affected();
    require_sales_write(changed, "Sales arrangement write conflict")?;
    write_sales_scope(
        transaction,
        "erpx_sales_write.revenue_arrangement_guards",
        "arrangement_id",
        value.id().as_uuid(),
        value.scope(),
    )
    .await?;
    write_arrangement_authority(transaction, value).await?;
    Ok(())
}

async fn sales_authorized_root(
    tx: &mut Transaction<'_, Postgres>,
    authority: &AuthorityScope,
    base: &str,
    table: &str,
    id_column: &str,
    id: Uuid,
) -> Result<Option<sqlx::postgres::PgRow>, PortError> {
    let (kind, holding, company, branch, companies, branches) =
        sales_authority_parameters(authority)?;
    let query = format!(
        "{base} AND ($2='platform' OR ($2='holding' AND holding_id=$3) OR ($2='company' AND holding_id=$3 AND company_id=$4) OR ($2='branch' AND holding_id=$3 AND company_id=$4 AND branch_id=$5) OR ($2='subtree' AND holding_id=$3 AND ((branch_id IS NULL AND cardinality($7::uuid[])=0 AND company_id=ANY($6::uuid[])) OR (branch_id IS NOT NULL AND company_id=ANY($6::uuid[]) AND branch_id=ANY($7::uuid[])))))"
    );
    let row = sqlx::query(&query)
        .bind(id)
        .bind(kind)
        .bind(holding)
        .bind(company)
        .bind(branch)
        .bind(companies)
        .bind(branches)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
    if row.is_some() {
        return Ok(row);
    }
    let exists_query = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE {id_column}=$1)");
    let exists: bool = sqlx::query_scalar(&exists_query)
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

type SalesAuthorityParameters = (
    &'static str,
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
    Vec<Uuid>,
    Vec<Uuid>,
);
fn sales_authority_parameters(
    scope: &AuthorityScope,
) -> Result<SalesAuthorityParameters, PortError> {
    scope.validate().map_err(|_| PortError::Denied)?;
    Ok(match scope {
        AuthorityScope::Platform => ("platform", None, None, None, vec![], vec![]),
        AuthorityScope::Holding { holding_id } => {
            ("holding", Some(*holding_id), None, None, vec![], vec![])
        }
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => (
            "company",
            Some(*holding_id),
            Some(*company_id),
            None,
            vec![],
            vec![],
        ),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => (
            "branch",
            Some(*holding_id),
            Some(*company_id),
            Some(*branch_id),
            vec![],
            vec![],
        ),
        AuthorityScope::Subtree {
            holding_id,
            organization_node_id,
        } => (
            "subtree",
            Some(*holding_id),
            None,
            None,
            vec![*organization_node_id],
            Vec::new(),
        ),
    })
}
fn sales_exact_scope(row: &sqlx::postgres::PgRow) -> Result<AuthorityScope, PortError> {
    let holding_id = row.try_get("holding_id").map_err(|_| PortError::Internal)?;
    let company_id = row.try_get("company_id").map_err(|_| PortError::Internal)?;
    Ok(
        match row.try_get("branch_id").map_err(|_| PortError::Internal)? {
            Some(branch_id) => AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            },
            None => AuthorityScope::Company {
                holding_id,
                company_id,
            },
        },
    )
}
fn row_u64(row: &sqlx::postgres::PgRow, column: &str) -> Result<u64, PortError> {
    u64::try_from(
        row.try_get::<i64, _>(column)
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)
}
fn parse_quote_status(value: &str) -> Result<QuoteStatus, PortError> {
    match value {
        "draft" => Ok(QuoteStatus::Draft),
        "sent" => Ok(QuoteStatus::Sent),
        "accepted" => Ok(QuoteStatus::Accepted),
        "rejected" => Ok(QuoteStatus::Rejected),
        "expired" => Ok(QuoteStatus::Expired),
        _ => Err(PortError::Internal),
    }
}
fn parse_order_status(value: &str) -> Result<SalesOrderStatus, PortError> {
    match value {
        "draft" => Ok(SalesOrderStatus::Draft),
        "confirmed" => Ok(SalesOrderStatus::Confirmed),
        "partially_fulfilled" => Ok(SalesOrderStatus::PartiallyFulfilled),
        "fulfilled" => Ok(SalesOrderStatus::Fulfilled),
        "cancelled" => Ok(SalesOrderStatus::Cancelled),
        _ => Err(PortError::Internal),
    }
}
fn parse_arrangement_status(value: &str) -> Result<RevenueArrangementStatus, PortError> {
    match value {
        "active" => Ok(RevenueArrangementStatus::Active),
        "completed" => Ok(RevenueArrangementStatus::Completed),
        "cancelled" => Ok(RevenueArrangementStatus::Cancelled),
        _ => Err(PortError::Internal),
    }
}
fn parse_revenue_model(value: &str) -> Result<sales::RevenueModel, PortError> {
    match value {
        "goods" => Ok(sales::RevenueModel::Goods),
        "service_milestone" => Ok(sales::RevenueModel::ServiceMilestone),
        "recurring" => Ok(sales::RevenueModel::Recurring),
        _ => Err(PortError::Internal),
    }
}
fn revenue_model_name(value: sales::RevenueModel) -> &'static str {
    match value {
        sales::RevenueModel::Goods => "goods",
        sales::RevenueModel::ServiceMilestone => "service_milestone",
        sales::RevenueModel::Recurring => "recurring",
    }
}
fn parse_fulfillment_kind(value: &str) -> Result<sales::FulfillmentKind, PortError> {
    match value {
        "goods_delivery" => Ok(sales::FulfillmentKind::GoodsDelivery),
        "service_milestone" => Ok(sales::FulfillmentKind::ServiceMilestone),
        "recurring_period" => Ok(sales::FulfillmentKind::RecurringPeriod),
        _ => Err(PortError::Internal),
    }
}
fn fulfillment_kind_name(value: sales::FulfillmentKind) -> &'static str {
    match value {
        sales::FulfillmentKind::GoodsDelivery => "goods_delivery",
        sales::FulfillmentKind::ServiceMilestone => "service_milestone",
        sales::FulfillmentKind::RecurringPeriod => "recurring_period",
    }
}

fn dimensions_from_row(row: &sqlx::postgres::PgRow) -> Result<DimensionSet, PortError> {
    macro_rules! required {
        ($constructor:path,$column:literal) => {
            $constructor(row.try_get($column).map_err(|_| PortError::Internal)?)
                .map_err(|_| PortError::Internal)?
        };
    }
    macro_rules! optional {
        ($constructor:path,$column:literal) => {
            row.try_get::<Option<Uuid>, _>($column)
                .map_err(|_| PortError::Internal)?
                .map($constructor)
                .transpose()
                .map_err(|_| PortError::Internal)?
        };
    }
    Ok(DimensionSet {
        legal_entity: required!(organization::CompanyRef::try_from_uuid, "legal_entity_id"),
        branch: optional!(
            organization::BranchRef::try_from_uuid,
            "branch_dimension_id"
        ),
        business_unit: optional!(
            organization::BusinessUnitRef::try_from_uuid,
            "business_unit_id"
        ),
        cost_center: optional!(
            erpx_integration_contracts::CostCenterRef::try_from_uuid,
            "cost_center_id"
        ),
        location: optional!(
            erpx_integration_contracts::LocationRef::try_from_uuid,
            "location_id"
        ),
        project: optional!(
            erpx_integration_contracts::ProjectRef::try_from_uuid,
            "project_id"
        ),
        campaign: optional!(marketing::CampaignRef::try_from_uuid, "campaign_id"),
        channel: optional!(marketing::ChannelRef::try_from_uuid, "channel_id"),
        party: optional!(
            erpx_integration_contracts::PartyRef::try_from_uuid,
            "party_id"
        ),
        employee: optional!(people_payroll::EmployeeRef::try_from_uuid, "employee_id"),
        asset: optional!(assets::AssetRef::try_from_uuid, "asset_id"),
        legal_matter: optional!(legal::LegalMatterRef::try_from_uuid, "legal_matter_id"),
        intercompany_partner: optional!(
            organization::CompanyRef::try_from_uuid,
            "intercompany_partner_id"
        ),
    })
}

fn exact_sales_scope_ids(
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
        _ => Err(CqrsSqlxError::GuardRejected("Sales scope must be exact")),
    }
}
async fn write_sales_scope(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    id_column: &str,
    id: Uuid,
    scope: &AuthorityScope,
) -> Result<(), CqrsSqlxError> {
    let (h, c, b) = exact_sales_scope_ids(scope)?;
    let query =
        format!("UPDATE {table} SET holding_id=$2,company_id=$3,branch_id=$4 WHERE {id_column}=$1");
    require_sales_write(
        sqlx::query(&query)
            .bind(id)
            .bind(h)
            .bind(c)
            .bind(b)
            .execute(&mut **tx)
            .await?
            .rows_affected(),
        "Sales scope write conflict",
    )
}
async fn write_sales_lines(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    id_column: &str,
    id: Uuid,
    lines: &[SalesOrderLine],
) -> Result<(), CqrsSqlxError> {
    let query = format!(
        "INSERT INTO {table} ({id_column},line_id,item_code,description,quantity,unit_price_minor,tax_minor,currency) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"
    );
    for line in lines {
        sqlx::query(&query)
            .bind(id)
            .bind(line.line_id.as_uuid())
            .bind(&line.item_code)
            .bind(&line.description)
            .bind(line.quantity)
            .bind(line.unit_price_minor)
            .bind(line.tax_minor)
            .bind(&line.currency)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}
async fn write_arrangement_authority(
    tx: &mut Transaction<'_, Postgres>,
    value: &RevenueArrangement,
) -> Result<(), CqrsSqlxError> {
    let d = value.dimensions();
    sqlx::query("INSERT INTO erpx_sales_write.arrangement_dimension_authority (arrangement_id,legal_entity_id,branch_dimension_id,business_unit_id,cost_center_id,location_id,project_id,campaign_id,channel_id,party_id,employee_id,asset_id,legal_matter_id,intercompany_partner_id) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) ON CONFLICT (arrangement_id) DO UPDATE SET legal_entity_id=EXCLUDED.legal_entity_id,branch_dimension_id=EXCLUDED.branch_dimension_id,business_unit_id=EXCLUDED.business_unit_id,cost_center_id=EXCLUDED.cost_center_id,location_id=EXCLUDED.location_id,project_id=EXCLUDED.project_id,campaign_id=EXCLUDED.campaign_id,channel_id=EXCLUDED.channel_id,party_id=EXCLUDED.party_id,employee_id=EXCLUDED.employee_id,asset_id=EXCLUDED.asset_id,legal_matter_id=EXCLUDED.legal_matter_id,intercompany_partner_id=EXCLUDED.intercompany_partner_id")
        .bind(value.id().as_uuid()).bind(d.legal_entity.as_uuid()).bind(d.branch.map(|x|x.as_uuid())).bind(d.business_unit.map(|x|x.as_uuid())).bind(d.cost_center.map(|x|x.as_uuid())).bind(d.location.map(|x|x.as_uuid())).bind(d.project.map(|x|x.as_uuid())).bind(d.campaign.map(|x|x.as_uuid())).bind(d.channel.map(|x|x.as_uuid())).bind(d.party.map(|x|x.as_uuid())).bind(d.employee.map(|x|x.as_uuid())).bind(d.asset.map(|x|x.as_uuid())).bind(d.legal_matter.map(|x|x.as_uuid())).bind(d.intercompany_partner.map(|x|x.as_uuid())).execute(&mut **tx).await?;
    sqlx::query(
        "DELETE FROM erpx_sales_write.arrangement_fulfillment_authority WHERE arrangement_id=$1",
    )
    .bind(value.id().as_uuid())
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "DELETE FROM erpx_sales_write.arrangement_obligation_authority WHERE arrangement_id=$1",
    )
    .bind(value.id().as_uuid())
    .execute(&mut **tx)
    .await?;
    for o in value.obligations() {
        sqlx::query("INSERT INTO erpx_sales_write.arrangement_obligation_authority (arrangement_id,obligation_id,line_id,revenue_model,allocated_minor,allocated_tax_minor,accepted_minor,accepted_tax_minor,service_starts_on,service_ends_on) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(value.id().as_uuid()).bind(o.obligation_id().as_uuid()).bind(o.line_id().as_uuid()).bind(revenue_model_name(o.model())).bind(o.allocated_minor()).bind(o.allocated_tax_minor()).bind(o.accepted_minor()).bind(o.accepted_tax_minor()).bind(o.service_starts_on()).bind(o.service_ends_on()).execute(&mut **tx).await?;
    }
    for f in value.fulfillments() {
        sqlx::query("INSERT INTO erpx_sales_write.arrangement_fulfillment_authority (arrangement_id,fulfillment_id,billing_event_id,obligation_id,fulfillment_kind,accepted_minor,accepted_tax_minor,accepted_at,eligible_on) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(value.id().as_uuid()).bind(f.fulfillment_id().as_uuid()).bind(f.billing_event_id().as_uuid()).bind(f.obligation_id().as_uuid()).bind(fulfillment_kind_name(f.kind())).bind(f.accepted_minor()).bind(f.accepted_tax_minor()).bind(f.accepted_at()).bind(f.eligible_on()).execute(&mut **tx).await?;
    }
    Ok(())
}
fn require_sales_write(changed: u64, message: &'static str) -> Result<(), CqrsSqlxError> {
    if changed == 1 {
        Ok(())
    } else {
        Err(CqrsSqlxError::GuardRejected(message))
    }
}

fn validate_guard_context(
    aggregate: &SalesAggregate,
    context: &GuardCommandContext<'_>,
) -> Result<(), CqrsSqlxError> {
    if context.context != CONTEXT_NAME
        || context.aggregate_type != aggregate.aggregate_type()
        || context.aggregate_id.as_uuid() != aggregate.id_uuid()
        || context.partition != aggregate_partition(aggregate)
    {
        Err(CqrsSqlxError::GuardRejected(
            "Sales guard metadata mismatch",
        ))
    } else {
        Ok(())
    }
}

fn validate_commit(request: &SalesCommit) -> Result<(), PortError> {
    let (partition, kind) = fingerprint_target(&request.identity.fingerprint);
    if request.aggregate.validate().is_err()
        || partition != aggregate_partition(&request.aggregate)
        || kind != request.aggregate.aggregate_type()
        || !request.authority_scope.contains(request.aggregate.scope())
    {
        Err(PortError::Internal)
    } else {
        Ok(())
    }
}

fn fingerprint_target(value: &CommandFingerprint) -> (String, &'static str) {
    match value {
        CommandFingerprint::CreateQuote { quote_id, .. }
        | CommandFingerprint::SendQuote { quote_id }
        | CommandFingerprint::AcceptQuote { quote_id }
        | CommandFingerprint::RejectQuote { quote_id }
        | CommandFingerprint::ExpireQuote { quote_id } => {
            (quote_partition(*quote_id), QUOTE_AGGREGATE_TYPE)
        }
        CommandFingerprint::CreateOrder { sales_order_id, .. }
        | CommandFingerprint::ConfirmOrder { sales_order_id }
        | CommandFingerprint::PartiallyFulfillOrder { sales_order_id }
        | CommandFingerprint::FulfillOrder { sales_order_id }
        | CommandFingerprint::CancelOrder { sales_order_id } => {
            (order_partition(*sales_order_id), ORDER_AGGREGATE_TYPE)
        }
        CommandFingerprint::ActivateRevenueArrangement { arrangement_id, .. }
        | CommandFingerprint::AcceptRevenueFulfillment { arrangement_id, .. }
        | CommandFingerprint::CancelRevenueArrangement { arrangement_id } => (
            arrangement_partition(*arrangement_id),
            ARRANGEMENT_AGGREGATE_TYPE,
        ),
    }
}

fn aggregate_partition(value: &SalesAggregate) -> String {
    match value {
        SalesAggregate::Quote(value) => quote_partition(value.id()),
        SalesAggregate::SalesOrder(value) => order_partition(value.id()),
        SalesAggregate::RevenueArrangement(value) => arrangement_partition(value.id()),
    }
}

fn aggregate_updated_at(value: &SalesAggregate) -> chrono::DateTime<chrono::Utc> {
    match value {
        SalesAggregate::Quote(value) => value.updated_at(),
        SalesAggregate::SalesOrder(value) => value.updated_at(),
        SalesAggregate::RevenueArrangement(value) => value.updated_at(),
    }
}

fn audit_action(value: &CommandFingerprint) -> &'static str {
    match value {
        CommandFingerprint::CreateQuote { .. } => "sales.quote.create",
        CommandFingerprint::SendQuote { .. } => "sales.quote.send",
        CommandFingerprint::AcceptQuote { .. } => "sales.quote.accept",
        CommandFingerprint::RejectQuote { .. } => "sales.quote.reject",
        CommandFingerprint::ExpireQuote { .. } => "sales.quote.expire",
        CommandFingerprint::CreateOrder { .. } => "sales.order.create",
        CommandFingerprint::ConfirmOrder { .. } => "sales.order.confirm",
        CommandFingerprint::PartiallyFulfillOrder { .. } => "sales.order.partially_fulfill",
        CommandFingerprint::FulfillOrder { .. } => "sales.order.fulfill",
        CommandFingerprint::CancelOrder { .. } => "sales.order.cancel",
        CommandFingerprint::ActivateRevenueArrangement { .. } => {
            "sales.revenue_arrangement.activate"
        }
        CommandFingerprint::AcceptRevenueFulfillment { .. } => {
            "sales.revenue_arrangement.fulfillment.accept"
        }
        CommandFingerprint::CancelRevenueArrangement { .. } => "sales.revenue_arrangement.cancel",
    }
}

fn request_hash(value: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let mut digest = Sha256::new();
    digest.update(b"erpx.sales.command-fingerprint.v1\0");
    digest.update(serde_json::to_vec(value).map_err(|_| PortError::Internal)?);
    Ok(RequestHash::new(digest.finalize().into()))
}

fn deterministic_event_id(command: CommandId, event_type: &str, index: usize) -> EventId {
    let mut input = event_type.as_bytes().to_vec();
    input.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(command.as_uuid(), &input)).expect("derived event id")
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

pub(crate) fn scope_key(scope: &AuthorityScope) -> Result<String, CqrsSqlxError> {
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
        _ => Err(CqrsSqlxError::GuardRejected("Sales scope must be exact")),
    }
}

fn to_i64(value: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<SalesAggregate>,
    kind: &str,
) -> Result<StoredCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != kind
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.id_uuid()
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
    fn stable_hash_is_payload_sensitive() {
        let id = QuoteId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let send = CommandFingerprint::SendQuote { quote_id: id };
        let accept = CommandFingerprint::AcceptQuote { quote_id: id };
        assert_eq!(request_hash(&send).unwrap(), request_hash(&send).unwrap());
        assert_ne!(request_hash(&send).unwrap(), request_hash(&accept).unwrap());
    }

    #[test]
    fn fingerprint_targets_are_aggregate_specific() {
        let quote = QuoteId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let order = SalesOrderId::try_from_uuid(Uuid::from_u128(2)).unwrap();
        assert_eq!(
            fingerprint_target(&CommandFingerprint::SendQuote { quote_id: quote }),
            (quote_partition(quote), QUOTE_AGGREGATE_TYPE)
        );
        assert_eq!(
            fingerprint_target(&CommandFingerprint::ConfirmOrder {
                sales_order_id: order,
            }),
            (order_partition(order), ORDER_AGGREGATE_TYPE)
        );
    }
}
