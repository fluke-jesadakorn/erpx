use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::IntegrationEvent;
use erpx_kernel::{AggregateId, CommandId, EventId, TenantScope};
use erpx_sales::{
    CommandUnitOfWork, PortError, Quote, QuoteId, SalesAggregate, SalesCommit, SalesOrder,
    SalesOrderId, StoredCommit, WriteRepository,
    ports::{CommandFingerprint, CommandIdentity},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    CONTEXT_NAME, EVENT_VERSION, ORDER_AGGREGATE_TYPE, ORDER_SNAPSHOT_EVENT, QUOTE_AGGREGATE_TYPE,
    QUOTE_SNAPSHOT_EVENT, order_partition,
    projection::{SalesOutboxPayload, SalesSnapshot, order_status_name, quote_status_name},
    quote_partition,
};

#[derive(Debug, Clone)]
pub struct SalesWriteRepository {
    store: CommandStore,
}

impl SalesWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }

    async fn load(
        &self,
        authority: &TenantScope,
        partition: &str,
        kind: &str,
        id: Uuid,
    ) -> Result<Option<SalesAggregate>, PortError> {
        let id = AggregateId::try_from_uuid(id).map_err(|_| PortError::Internal)?;
        let snapshot = self
            .store
            .load_snapshot::<SalesAggregate>(CONTEXT_NAME, partition, kind, id)
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        if snapshot.version != snapshot.state.version()
            || snapshot.state.aggregate_type() != kind
            || snapshot.state.id_uuid() != id.as_uuid()
            || snapshot.state.validate().is_err()
        {
            return Err(PortError::Internal);
        }
        if !authority.contains(snapshot.state.scope()) {
            return Err(PortError::Denied);
        }
        Ok(Some(snapshot.state))
    }
}

#[async_trait]
impl WriteRepository for SalesWriteRepository {
    async fn load_quote(
        &self,
        authority_scope: &TenantScope,
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
        authority_scope: &TenantScope,
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
        let guard = SalesGuard(request.aggregate.clone());
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
            aggregate_type: kind.into(),
            aggregate_id: AggregateId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
            expected_version: request.expected_version,
            aggregate_state: serde_json::to_value(&aggregate).map_err(|_| PortError::Internal)?,
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
            .commit_guarded(command, &guard)
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

struct SalesGuard(SalesAggregate);

#[async_trait]
impl AtomicCommandGuard for SalesGuard {
    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(&self.0, context)?;
        match &self.0 {
            SalesAggregate::Quote(value) => validate_quote(transaction, context, value).await,
            SalesAggregate::SalesOrder(value) => validate_order(transaction, context, value).await,
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

fn ensure_existence(
    expected: Option<u64>,
    current: Option<i64>,
    kind: &'static str,
) -> Result<(), CqrsSqlxError> {
    if matches!((expected, current), (None, Some(_)) | (Some(_), None)) {
        Err(CqrsSqlxError::GuardRejected(match kind {
            "Sales quote" => "Sales quote guard existence mismatch",
            _ => "Sales order guard existence mismatch",
        }))
    } else {
        Ok(())
    }
}

async fn apply_quote(
    transaction: &mut Transaction<'_, Postgres>,
    value: &Quote,
) -> Result<(), CqrsSqlxError> {
    sqlx::query(
        "INSERT INTO erpx_sales_write.quote_guards
          (quote_id,scope_key,opportunity_id,customer_id,quote_code,currency,total_minor,
           valid_until,quote_status,aggregate_version,created_at,updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
         ON CONFLICT (quote_id) DO UPDATE SET
          scope_key=EXCLUDED.scope_key,opportunity_id=EXCLUDED.opportunity_id,
          customer_id=EXCLUDED.customer_id,quote_code=EXCLUDED.quote_code,
          currency=EXCLUDED.currency,total_minor=EXCLUDED.total_minor,
          valid_until=EXCLUDED.valid_until,quote_status=EXCLUDED.quote_status,
          aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at",
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
    .await?;
    Ok(())
}

async fn apply_order(
    transaction: &mut Transaction<'_, Postgres>,
    value: &SalesOrder,
) -> Result<(), CqrsSqlxError> {
    sqlx::query(
        "INSERT INTO erpx_sales_write.sales_order_guards
          (sales_order_id,scope_key,quote_id,opportunity_id,customer_id,order_code,currency,
           total_minor,order_status,aggregate_version,created_at,updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
         ON CONFLICT (sales_order_id) DO UPDATE SET
          scope_key=EXCLUDED.scope_key,quote_id=EXCLUDED.quote_id,
          opportunity_id=EXCLUDED.opportunity_id,customer_id=EXCLUDED.customer_id,
          order_code=EXCLUDED.order_code,currency=EXCLUDED.currency,
          total_minor=EXCLUDED.total_minor,order_status=EXCLUDED.order_status,
          aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at",
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
    .await?;
    Ok(())
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
    }
}

fn aggregate_partition(value: &SalesAggregate) -> String {
    match value {
        SalesAggregate::Quote(value) => quote_partition(value.id()),
        SalesAggregate::SalesOrder(value) => order_partition(value.id()),
    }
}

fn aggregate_updated_at(value: &SalesAggregate) -> chrono::DateTime<chrono::Utc> {
    match value {
        SalesAggregate::Quote(value) => value.updated_at(),
        SalesAggregate::SalesOrder(value) => value.updated_at(),
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

pub(crate) fn scope_key(scope: &TenantScope) -> Result<String, CqrsSqlxError> {
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
