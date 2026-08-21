//! Deterministic Procurement-to-Payment process-manager decisions.
//!
//! The process manager consumes only validated owner facts. Its outputs are
//! ordinary owner commands: Inventory revalidates every receipt movement and
//! Finance creates a payment proposal against a posted payable. No handoff is
//! an approval, acceptance, posting, or release decision.

use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{CONTRACT_VERSION, CommandEnvelope, CommandHandler, IdempotencyKey};
use erpx_cqrs_sqlx::WritePool;
use erpx_finance::{
    CreatePayment, PayableId, PayablesAction, PayablesAuthorizationPort,
    PayablesAuthorizationRequest, PayablesCommand, PayablesCommandHandler, PayablesEntitlementPort,
    PaymentAllocation, PaymentId,
};
use erpx_finance_sqlx::SqlxPayablesAdapter;
use erpx_infrastructure::{HoldingDatabaseRouter, RoutedDatabaseSide, SqlxHoldingStoreRegistry};
use erpx_integration_contracts::{
    AggregateKind, BoundedContext, IntegrationEvent, IntegrationEventEnvelope, MoneyAmount,
    Quantity, RecordRef, finance, procurement,
};
use erpx_inventory::{
    AuthorizationPort as InventoryAuthorizationPort, EntitlementPort as InventoryEntitlementPort,
    InventoryAction, InventoryCommandHandler, ReceiveStock, StockItemId, StockMovementId,
    WriteRepository as InventoryWritePort,
    ports::AuthorizationRequest as InventoryAuthorizationRequest,
};
use erpx_inventory_sqlx::{InventoryCommandAdapter, InventoryWriteRepository};
use erpx_kernel::{
    ActorAttribution, ActorKind, AggregateId, AuditContext, AuthorityScope, CommandId,
    CorrelationId, EventId, SystemClock,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use thiserror::Error;
use uuid::Uuid;

use crate::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

const RECEIPT_MOVEMENT_NAMESPACE: Uuid = Uuid::from_u128(0x4552_5058_5052_4f43_5245_4354_0000_0001);
const PAYMENT_PROPOSAL_NAMESPACE: Uuid = Uuid::from_u128(0x4552_5058_5052_4f43_5041_594d_0000_0001);
const INVENTORY_COMMAND_NAMESPACE: Uuid =
    Uuid::from_u128(0x4552_5058_5052_4f43_494e_5643_0000_0001);
const PAYMENT_COMMAND_NAMESPACE: Uuid = Uuid::from_u128(0x4552_5058_5052_4f43_5041_5943_0000_0001);

pub const PROCURE_TO_PAY_HANDLER_NAME: &str = "procure_to_pay";
pub const PROCURE_TO_PAY_MAX_PAGE_SIZE: u32 = 100;
pub const PROCURE_TO_PAY_MAX_ACTIVE_HOLDINGS: usize = 1_000;
const PROCESS_NAME: &str = "procure-to-pay-v1";
const RELEVANT_EVENT_TYPES: [&str; 3] = [
    "procurement.purchase_order_released",
    "procurement.goods_receipt_accepted",
    "finance.vendor_payable_posted",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ProcureToPayHandoffKind {
    ReceiveInventory {
        movement_id: Uuid,
        stock_item_id: Uuid,
        purchase_order_line_id: Uuid,
        quantity: i64,
    },
    ProposePayment {
        payment_id: Uuid,
        payable_id: Uuid,
        payable_version: u64,
        supplier_id: Uuid,
        amount_minor: i64,
        currency: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcureToPayHandoff {
    pub source_event_id: EventId,
    pub source_aggregate_version: u64,
    pub source_event_type: String,
    pub active_scope: AuthorityScope,
    pub correlation_id: Uuid,
    pub handoff: ProcureToPayHandoffKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcureToPayConsumeOutcome {
    Staged(Vec<ProcureToPayHandoff>),
    Ignored,
    Replay(Vec<ProcureToPayHandoff>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "owner", content = "envelope", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)] // Typed owner envelopes remain direct in retained process evidence.
pub enum ProcureToPaySourceEvent {
    Procurement(IntegrationEventEnvelope<procurement::Event>),
    Finance(IntegrationEventEnvelope<finance::Event>),
}

impl ProcureToPaySourceEvent {
    fn event_id(&self) -> EventId {
        match self {
            Self::Procurement(value) => value.event_id(),
            Self::Finance(value) => value.event_id(),
        }
    }

    fn validate(&self) -> Result<(), ProcureToPayConsumerError> {
        match self {
            Self::Procurement(value) => value.validate(),
            Self::Finance(value) => value.validate(),
        }
        .map_err(|_| ProcureToPayConsumerError::InvalidEnvelope)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProcureToPayConsumerState {
    checkpoint: u64,
    processed: BTreeMap<EventId, Vec<ProcureToPayHandoff>>,
}

impl ProcureToPayConsumerState {
    pub fn consume(
        &mut self,
        global_sequence: u64,
        source: &ProcureToPaySourceEvent,
    ) -> Result<ProcureToPayConsumeOutcome, ProcureToPayConsumerError> {
        source.validate()?;
        if global_sequence == 0 {
            return Err(ProcureToPayConsumerError::InvalidSequence);
        }
        let event_id = source.event_id();
        if let Some(stored) = self.processed.get(&event_id) {
            return Ok(ProcureToPayConsumeOutcome::Replay(stored.clone()));
        }
        if global_sequence <= self.checkpoint {
            return Err(ProcureToPayConsumerError::CheckpointRegression);
        }
        let handoffs = map_handoffs(source)?;
        self.checkpoint = global_sequence;
        self.processed.insert(event_id, handoffs.clone());
        Ok(if handoffs.is_empty() {
            ProcureToPayConsumeOutcome::Ignored
        } else {
            ProcureToPayConsumeOutcome::Staged(handoffs)
        })
    }

    pub const fn checkpoint(&self) -> u64 {
        self.checkpoint
    }
}

fn map_handoffs(
    source: &ProcureToPaySourceEvent,
) -> Result<Vec<ProcureToPayHandoff>, ProcureToPayConsumerError> {
    match source {
        ProcureToPaySourceEvent::Procurement(envelope) => {
            let procurement::Event::GoodsReceiptAccepted(fact) = envelope.payload() else {
                return Ok(Vec::new());
            };
            fact.lines
                .iter()
                .map(|line| {
                    let mut seed = Vec::with_capacity(32);
                    seed.extend_from_slice(envelope.event_id().as_uuid().as_bytes());
                    seed.extend_from_slice(line.line.as_uuid().as_bytes());
                    let quantity = line.accepted_quantity.units();
                    if quantity <= 0 {
                        return Err(ProcureToPayConsumerError::InvalidOwnerFact);
                    }
                    Ok(ProcureToPayHandoff {
                        source_event_id: envelope.event_id(),
                        source_aggregate_version: envelope.aggregate_version(),
                        source_event_type: envelope.event_type().into(),
                        active_scope: envelope.active_scope().clone(),
                        correlation_id: envelope.correlation_id().as_uuid(),
                        handoff: ProcureToPayHandoffKind::ReceiveInventory {
                            movement_id: Uuid::new_v5(&RECEIPT_MOVEMENT_NAMESPACE, &seed),
                            stock_item_id: line.stock_item.as_uuid(),
                            purchase_order_line_id: line.purchase_order_line.as_uuid(),
                            quantity,
                        },
                    })
                })
                .collect()
        }
        ProcureToPaySourceEvent::Finance(envelope) => {
            let finance::Event::VendorPayablePosted(fact) = envelope.payload() else {
                return Ok(Vec::new());
            };
            if fact.payable_amount.minor_units() <= 0 {
                return Err(ProcureToPayConsumerError::InvalidOwnerFact);
            }
            Ok(vec![ProcureToPayHandoff {
                source_event_id: envelope.event_id(),
                source_aggregate_version: envelope.aggregate_version(),
                source_event_type: envelope.event_type().into(),
                active_scope: envelope.active_scope().clone(),
                correlation_id: envelope.correlation_id().as_uuid(),
                handoff: ProcureToPayHandoffKind::ProposePayment {
                    payment_id: Uuid::new_v5(
                        &PAYMENT_PROPOSAL_NAMESPACE,
                        envelope.event_id().as_uuid().as_bytes(),
                    ),
                    payable_id: fact.payable.as_uuid(),
                    payable_version: fact.payable_version,
                    supplier_id: fact.supplier.as_uuid(),
                    amount_minor: fact.payable_amount.minor_units(),
                    currency: fact.payable_amount.currency().as_str().into(),
                },
            }])
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ProcureToPayConsumerError {
    #[error("Procure-to-Pay integration envelope is invalid")]
    InvalidEnvelope,
    #[error("Procure-to-Pay integration sequence is invalid")]
    InvalidSequence,
    #[error("Procure-to-Pay checkpoint moved backwards")]
    CheckpointRegression,
    #[error("Procure-to-Pay owner fact is invalid")]
    InvalidOwnerFact,
}

#[derive(Debug, Clone)]
pub struct ProcureToPayRuntimeHandler {
    registry: SqlxHoldingStoreRegistry,
    router: HoldingDatabaseRouter,
    page_size: u32,
    max_active_holdings: usize,
}

impl ProcureToPayRuntimeHandler {
    pub fn new(
        registry: SqlxHoldingStoreRegistry,
        router: HoldingDatabaseRouter,
        page_size: u32,
        max_active_holdings: usize,
    ) -> Result<Self, WorkerRuntimeError> {
        if page_size == 0
            || page_size > PROCURE_TO_PAY_MAX_PAGE_SIZE
            || max_active_holdings == 0
            || max_active_holdings > PROCURE_TO_PAY_MAX_ACTIVE_HOLDINGS
        {
            return Err(configuration("Procure-to-Pay worker bounds are invalid"));
        }
        Ok(Self {
            registry,
            router,
            page_size,
            max_active_holdings,
        })
    }

    async fn scan_holding(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        worker_id: Uuid,
    ) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let routed = self
            .router
            .route(holding_id, data_profile_key, RoutedDatabaseSide::Write)
            .await
            .map_err(|_| configuration("Procure-to-Pay holding route is unavailable"))?;
        let bound_holding_id = Uuid::parse_str(&routed.holding_id)
            .ok()
            .filter(|value| !value.is_nil())
            .ok_or_else(|| configuration("Procure-to-Pay holding identity is invalid"))?;
        let database = routed.database_pool().clone();
        let mut process_lock = database
            .begin()
            .await
            .map_err(|_| configuration("Procure-to-Pay process lock is unavailable"))?;
        let locked: bool =
            sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtext($1),hashtext($2))")
                .bind(PROCESS_NAME)
                .bind(bound_holding_id.to_string())
                .fetch_one(&mut *process_lock)
                .await
                .map_err(|_| configuration("Procure-to-Pay process lock is unavailable"))?;
        if !locked {
            return Ok(WorkerHandlerReport::default());
        }
        let write = WritePool::for_holding(database.clone(), bound_holding_id)
            .map_err(|_| configuration("Procure-to-Pay holding binding is invalid"))?;
        let rows = read_source_page(&write, self.page_size).await?;
        let mut report = WorkerHandlerReport {
            claimed: u64::try_from(rows.len())
                .map_err(|_| configuration("Procure-to-Pay page count is invalid"))?,
            has_work: !rows.is_empty(),
            ..WorkerHandlerReport::default()
        };
        for row in rows {
            process_source_event(&write, bound_holding_id, worker_id, &row).await?;
            report.processed = report.processed.saturating_add(1);
            if row.event_type != "procurement.purchase_order_released" {
                report.published = report.published.saturating_add(1);
            }
        }
        process_lock
            .commit()
            .await
            .map_err(|_| configuration("Procure-to-Pay process lock could not close"))?;
        Ok(report)
    }
}

#[async_trait]
impl NamedWorkerHandler for ProcureToPayRuntimeHandler {
    fn name(&self) -> &str {
        PROCURE_TO_PAY_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if worker_id.is_nil() {
            return Err(configuration("Procure-to-Pay worker identity is required"));
        }
        let holdings = self
            .registry
            .list_active_bounded(self.max_active_holdings)
            .await
            .map_err(|_| configuration("Procure-to-Pay holding registry is unavailable"))?;
        let mut total = WorkerHandlerReport::default();
        for holding in holdings {
            let report = self
                .scan_holding(&holding.holding_id, &holding.data_profile_key, worker_id)
                .await?;
            total.claimed = total.claimed.saturating_add(report.claimed);
            total.processed = total.processed.saturating_add(report.processed);
            total.published = total.published.saturating_add(report.published);
            total.has_work |= report.has_work;
        }
        Ok(total)
    }
}

#[derive(Debug, Clone)]
struct SourceRow {
    event_id: Uuid,
    global_sequence: u64,
    context_name: String,
    event_type: String,
    event_version: u16,
    aggregate_type: String,
    aggregate_id: Uuid,
    aggregate_version: u64,
    active_scope: AuthorityScope,
    active_scope_document: Value,
    correlation_id: Uuid,
    causation_id: Option<Uuid>,
    payload: Value,
    occurred_at: DateTime<Utc>,
}

async fn read_source_page(
    write: &WritePool,
    page_size: u32,
) -> Result<Vec<SourceRow>, WorkerRuntimeError> {
    let mut transaction = write
        .begin()
        .await
        .map_err(|_| configuration("Procure-to-Pay source snapshot is unavailable"))?;
    let after_sequence: i64 = sqlx::query_scalar(
        "SELECT COALESCE((SELECT global_sequence
          FROM erpx_procure_to_pay_write.process_cursor
          WHERE process_name=$1),0)::BIGINT",
    )
    .bind(PROCESS_NAME)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|_| configuration("Procure-to-Pay cursor is unavailable"))?;
    let rows = sqlx::query(
        "SELECT event_id,global_sequence,context_name,event_type,event_version,
                aggregate_type,aggregate_id,aggregate_version,active_scope,
                correlation_id,causation_id,payload,occurred_at
         FROM erpx_cqrs_write.outbox_events
         WHERE global_sequence>$1 AND available_at<=now()
           AND event_type=ANY($2::TEXT[])
         ORDER BY global_sequence
         LIMIT $3",
    )
    .bind(after_sequence)
    .bind(RELEVANT_EVENT_TYPES.as_slice())
    .bind(i64::from(page_size))
    .fetch_all(&mut *transaction)
    .await
    .map_err(|_| configuration("Procure-to-Pay source page is unavailable"))?;
    transaction
        .commit()
        .await
        .map_err(|_| configuration("Procure-to-Pay source snapshot could not close"))?;
    rows.into_iter()
        .map(|row| {
            let active_scope_document: Value = row
                .try_get("active_scope")
                .map_err(|_| configuration("Procure-to-Pay scope document is invalid"))?;
            let active_scope = serde_json::from_value(active_scope_document.clone())
                .map_err(|_| configuration("Procure-to-Pay scope is invalid"))?;
            Ok(SourceRow {
                event_id: required_uuid(&row, "event_id")?,
                global_sequence: positive_u64(&row, "global_sequence")?,
                context_name: row
                    .try_get("context_name")
                    .map_err(|_| configuration("Procure-to-Pay source context is invalid"))?,
                event_type: row
                    .try_get("event_type")
                    .map_err(|_| configuration("Procure-to-Pay source type is invalid"))?,
                event_version: positive_u16(&row, "event_version")?,
                aggregate_type: row
                    .try_get("aggregate_type")
                    .map_err(|_| configuration("Procure-to-Pay aggregate type is invalid"))?,
                aggregate_id: required_uuid(&row, "aggregate_id")?,
                aggregate_version: positive_u64(&row, "aggregate_version")?,
                active_scope,
                active_scope_document,
                correlation_id: required_uuid(&row, "correlation_id")?,
                causation_id: row
                    .try_get::<Option<Uuid>, _>("causation_id")
                    .map_err(|_| configuration("Procure-to-Pay causation is invalid"))?
                    .map(required_non_nil)
                    .transpose()?,
                payload: row
                    .try_get("payload")
                    .map_err(|_| configuration("Procure-to-Pay payload is invalid"))?,
                occurred_at: row
                    .try_get("occurred_at")
                    .map_err(|_| configuration("Procure-to-Pay timestamp is invalid"))?,
            })
        })
        .collect()
}

async fn process_source_event(
    write: &WritePool,
    holding_id: Uuid,
    worker_id: Uuid,
    row: &SourceRow,
) -> Result<(), WorkerRuntimeError> {
    ensure_holding_scope(&row.active_scope, holding_id)?;
    match row.event_type.as_str() {
        "procurement.purchase_order_released" => {
            let envelope = procurement_envelope(row)?;
            let procurement::Event::PurchaseOrderReleased(fact) = envelope.payload() else {
                return Err(configuration(
                    "Procure-to-Pay release payload is mismatched",
                ));
            };
            persist_purchase_order_fact(write, row, fact).await?;
        }
        "procurement.goods_receipt_accepted" => {
            let envelope = procurement_envelope(row)?;
            let procurement::Event::GoodsReceiptAccepted(fact) = envelope.payload() else {
                return Err(configuration(
                    "Procure-to-Pay receipt payload is mismatched",
                ));
            };
            let purchase_order = load_purchase_order_fact(write, &row.active_scope, fact).await?;
            let evidence = build_three_way_evidence(fact, &purchase_order, row.occurred_at)?;
            persist_finance_evidence(write, row, &evidence).await?;
            apply_inventory_receipts(write, worker_id, row, fact).await?;
        }
        "finance.vendor_payable_posted" => {
            let envelope = finance_envelope(row)?;
            let finance::Event::VendorPayablePosted(fact) = envelope.payload() else {
                return Err(configuration(
                    "Procure-to-Pay payable payload is mismatched",
                ));
            };
            propose_payment(write, worker_id, row, fact).await?;
        }
        _ => return Err(configuration("Procure-to-Pay event inventory drifted")),
    }
    record_processed(write, row).await?;
    Ok(())
}

fn procurement_envelope(
    row: &SourceRow,
) -> Result<IntegrationEventEnvelope<procurement::Event>, WorkerRuntimeError> {
    if row.context_name != "procurement" {
        return Err(configuration(
            "Procure-to-Pay Procurement owner is mismatched",
        ));
    }
    let payload: procurement::Event = serde_json::from_value(integration_payload(row)?)
        .map_err(|_| configuration("Procure-to-Pay Procurement payload is invalid"))?;
    typed_envelope(row, payload)
}

fn finance_envelope(
    row: &SourceRow,
) -> Result<IntegrationEventEnvelope<finance::Event>, WorkerRuntimeError> {
    if row.context_name != "finance" {
        return Err(configuration("Procure-to-Pay Finance owner is mismatched"));
    }
    let payload: finance::Event = serde_json::from_value(integration_payload(row)?)
        .map_err(|_| configuration("Procure-to-Pay Finance payload is invalid"))?;
    typed_envelope(row, payload)
}

fn integration_payload(row: &SourceRow) -> Result<Value, WorkerRuntimeError> {
    let Some(kind) = row.payload.get("payload_kind") else {
        return Ok(row.payload.clone());
    };
    if kind.as_str() != Some("integration") {
        return Err(configuration(
            "Procure-to-Pay source payload kind is mismatched",
        ));
    }
    row.payload
        .get("payload")
        .cloned()
        .ok_or_else(|| configuration("Procure-to-Pay integration payload is missing"))
}

fn typed_envelope<E: IntegrationEvent>(
    row: &SourceRow,
    payload: E,
) -> Result<IntegrationEventEnvelope<E>, WorkerRuntimeError> {
    if payload.event_type() != row.event_type || payload.schema_version() != row.event_version {
        return Err(configuration(
            "Procure-to-Pay source metadata is mismatched",
        ));
    }
    IntegrationEventEnvelope::try_new(
        EventId::try_from_uuid(row.event_id)
            .map_err(|_| configuration("Procure-to-Pay event id is invalid"))?,
        row.active_scope.clone(),
        AggregateKind::try_new(row.aggregate_type.clone())
            .map_err(|_| configuration("Procure-to-Pay aggregate kind is invalid"))?,
        AggregateId::try_from_uuid(row.aggregate_id)
            .map_err(|_| configuration("Procure-to-Pay aggregate id is invalid"))?,
        row.aggregate_version,
        CorrelationId::try_from_uuid(row.correlation_id)
            .map_err(|_| configuration("Procure-to-Pay correlation is invalid"))?,
        row.causation_id
            .map(EventId::try_from_uuid)
            .transpose()
            .map_err(|_| configuration("Procure-to-Pay causation is invalid"))?,
        row.occurred_at,
        payload,
    )
    .map_err(|_| configuration("Procure-to-Pay envelope is invalid"))
}

async fn persist_purchase_order_fact(
    write: &WritePool,
    row: &SourceRow,
    fact: &procurement::ReleasedPurchaseOrderFact,
) -> Result<(), WorkerRuntimeError> {
    if fact.purchase_order.as_uuid() != row.aggregate_id {
        return Err(configuration(
            "Procure-to-Pay purchase order identity is mismatched",
        ));
    }
    let document = serde_json::to_value(fact)
        .map_err(|_| configuration("Procure-to-Pay purchase order fact is invalid"))?;
    let mut transaction = write
        .begin()
        .await
        .map_err(|_| configuration("Procure-to-Pay purchase order inbox is unavailable"))?;
    sqlx::query(
        "INSERT INTO erpx_procure_to_pay_write.purchase_order_facts
         (purchase_order_id,purchase_order_version,active_scope,supplier_id,fact_document,
          source_event_id,source_global_sequence,captured_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
         ON CONFLICT DO NOTHING",
    )
    .bind(row.aggregate_id)
    .bind(to_i64(row.aggregate_version)?)
    .bind(&row.active_scope_document)
    .bind(fact.supplier.as_uuid())
    .bind(&document)
    .bind(row.event_id)
    .bind(to_i64(row.global_sequence)?)
    .bind(row.occurred_at)
    .execute(&mut *transaction)
    .await
    .map_err(|_| configuration("Procure-to-Pay purchase order fact could not persist"))?;
    let stored = sqlx::query(
        "SELECT active_scope,supplier_id,fact_document,source_event_id,source_global_sequence
         FROM erpx_procure_to_pay_write.purchase_order_facts
         WHERE purchase_order_id=$1 AND purchase_order_version=$2 FOR SHARE",
    )
    .bind(row.aggregate_id)
    .bind(to_i64(row.aggregate_version)?)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|_| configuration("Procure-to-Pay purchase order fact is unavailable"))?;
    let same = stored.try_get::<Value, _>("active_scope").ok().as_ref()
        == Some(&row.active_scope_document)
        && stored.try_get::<Uuid, _>("supplier_id").ok() == Some(fact.supplier.as_uuid())
        && stored.try_get::<Value, _>("fact_document").ok().as_ref() == Some(&document)
        && stored.try_get::<Uuid, _>("source_event_id").ok() == Some(row.event_id)
        && stored.try_get::<i64, _>("source_global_sequence").ok()
            == Some(to_i64(row.global_sequence)?);
    if !same {
        return Err(configuration(
            "Procure-to-Pay purchase order fact conflicts",
        ));
    }
    transaction
        .commit()
        .await
        .map_err(|_| configuration("Procure-to-Pay purchase order fact could not commit"))
}

async fn load_purchase_order_fact(
    write: &WritePool,
    source_scope: &AuthorityScope,
    receipt: &procurement::AcceptedGoodsReceiptFact,
) -> Result<procurement::ReleasedPurchaseOrderFact, WorkerRuntimeError> {
    let mut transaction = write
        .begin()
        .await
        .map_err(|_| configuration("Procure-to-Pay purchase order evidence is unavailable"))?;
    let row = sqlx::query(
        "SELECT active_scope,fact_document
         FROM erpx_procure_to_pay_write.purchase_order_facts
         WHERE purchase_order_id=$1 AND purchase_order_version=$2 FOR SHARE",
    )
    .bind(receipt.purchase_order.record().id().as_uuid())
    .bind(to_i64(receipt.purchase_order.version())?)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|_| configuration("Procure-to-Pay purchase order evidence is unavailable"))?
    .ok_or_else(|| configuration("Procure-to-Pay exact purchase order evidence is missing"))?;
    let scope: AuthorityScope = serde_json::from_value(
        row.try_get("active_scope")
            .map_err(|_| configuration("Procure-to-Pay purchase order scope is invalid"))?,
    )
    .map_err(|_| configuration("Procure-to-Pay purchase order scope is invalid"))?;
    let fact = serde_json::from_value(
        row.try_get("fact_document")
            .map_err(|_| configuration("Procure-to-Pay purchase order fact is invalid"))?,
    )
    .map_err(|_| configuration("Procure-to-Pay purchase order fact is invalid"))?;
    transaction
        .commit()
        .await
        .map_err(|_| configuration("Procure-to-Pay purchase order evidence could not close"))?;
    if &scope != source_scope {
        return Err(configuration("Procure-to-Pay source scopes are mismatched"));
    }
    Ok(fact)
}

fn build_three_way_evidence(
    receipt: &procurement::AcceptedGoodsReceiptFact,
    purchase_order: &procurement::ReleasedPurchaseOrderFact,
    captured_at: DateTime<Utc>,
) -> Result<erpx_finance::ThreeWayMatchEvidence, WorkerRuntimeError> {
    if purchase_order.purchase_order.as_uuid() != receipt.purchase_order.record().id().as_uuid() {
        return Err(configuration(
            "Procure-to-Pay purchase order reference is mismatched",
        ));
    }
    let order_lines = purchase_order
        .lines
        .iter()
        .map(|line| (line.line.as_uuid(), line))
        .collect::<BTreeMap<_, _>>();
    let lines = receipt
        .lines
        .iter()
        .map(|received| {
            let ordered = order_lines
                .get(&received.purchase_order_line.as_uuid())
                .ok_or_else(|| {
                    configuration("Procure-to-Pay receipt line has no order evidence")
                })?;
            let received_amount = proportional_amount(
                ordered.line_value,
                ordered.ordered_quantity,
                received.accepted_quantity,
            )?;
            Ok(erpx_finance::ThreeWayMatchLineEvidence {
                purchase_order_line: received.purchase_order_line,
                receipt_line_id: received.line.as_uuid(),
                item_code: ordered.item_code.clone(),
                ordered_quantity: ordered.ordered_quantity,
                received_quantity: received.accepted_quantity,
                ordered_amount: ordered.line_value,
                received_amount,
            })
        })
        .collect::<Result<Vec<_>, WorkerRuntimeError>>()?;
    let value = erpx_finance::ThreeWayMatchEvidence {
        purchase_order: RecordRef::try_new(
            BoundedContext::Procurement,
            "purchase_order",
            receipt.purchase_order.record().id().as_uuid(),
            receipt.purchase_order.version(),
        )
        .map_err(|_| configuration("Procure-to-Pay purchase order evidence ref is invalid"))?,
        receipt: RecordRef::try_new(
            BoundedContext::Procurement,
            "receipt",
            receipt.receipt.record().id().as_uuid(),
            receipt.receipt.version(),
        )
        .map_err(|_| configuration("Procure-to-Pay receipt evidence ref is invalid"))?,
        supplier: purchase_order.supplier,
        lines,
        captured_at,
    };
    value
        .validate()
        .map_err(|_| configuration("Procure-to-Pay three-way evidence is invalid"))?;
    Ok(value)
}

fn proportional_amount(
    ordered_amount: MoneyAmount,
    ordered_quantity: Quantity,
    received_quantity: Quantity,
) -> Result<MoneyAmount, WorkerRuntimeError> {
    if ordered_quantity.units() <= 0
        || received_quantity.units() <= 0
        || ordered_quantity.scale() != received_quantity.scale()
    {
        return Err(configuration(
            "Procure-to-Pay quantity evidence is incompatible",
        ));
    }
    let numerator = i128::from(ordered_amount.minor_units())
        .checked_mul(i128::from(received_quantity.units()))
        .ok_or_else(|| configuration("Procure-to-Pay evidence amount overflowed"))?;
    let denominator = i128::from(ordered_quantity.units());
    if numerator % denominator != 0 {
        return Err(configuration("Procure-to-Pay evidence amount is not exact"));
    }
    let minor = i64::try_from(numerator / denominator)
        .map_err(|_| configuration("Procure-to-Pay evidence amount overflowed"))?;
    if minor <= 0 {
        return Err(configuration("Procure-to-Pay evidence amount is invalid"));
    }
    Ok(MoneyAmount::new(minor, ordered_amount.currency()))
}

async fn persist_finance_evidence(
    write: &WritePool,
    source: &SourceRow,
    evidence: &erpx_finance::ThreeWayMatchEvidence,
) -> Result<(), WorkerRuntimeError> {
    let document = serde_json::to_value(evidence)
        .map_err(|_| configuration("Procure-to-Pay Finance evidence is invalid"))?;
    let mut transaction = write
        .begin()
        .await
        .map_err(|_| configuration("Procure-to-Pay Finance evidence inbox is unavailable"))?;
    sqlx::query(
        "INSERT INTO erpx_finance_write.procurement_match_evidence_inbox
         (purchase_order_id,purchase_order_version,receipt_id,receipt_version,scope,scope_key,
          supplier_id,evidence_document,source_event_position,captured_at,received_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,now())
         ON CONFLICT DO NOTHING",
    )
    .bind(evidence.purchase_order.aggregate_id)
    .bind(to_i64(evidence.purchase_order.aggregate_version)?)
    .bind(evidence.receipt.aggregate_id)
    .bind(to_i64(evidence.receipt.aggregate_version)?)
    .bind(&source.active_scope_document)
    .bind(source.active_scope.canonical_key())
    .bind(evidence.supplier.as_uuid())
    .bind(&document)
    .bind(to_i64(source.global_sequence)?)
    .bind(evidence.captured_at)
    .execute(&mut *transaction)
    .await
    .map_err(|_| configuration("Procure-to-Pay Finance evidence could not persist"))?;
    let stored = sqlx::query(
        "SELECT scope,scope_key,supplier_id,evidence_document,source_event_position,captured_at
         FROM erpx_finance_write.procurement_match_evidence_inbox
         WHERE purchase_order_id=$1 AND purchase_order_version=$2
           AND receipt_id=$3 AND receipt_version=$4 FOR SHARE",
    )
    .bind(evidence.purchase_order.aggregate_id)
    .bind(to_i64(evidence.purchase_order.aggregate_version)?)
    .bind(evidence.receipt.aggregate_id)
    .bind(to_i64(evidence.receipt.aggregate_version)?)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|_| configuration("Procure-to-Pay Finance evidence is unavailable"))?;
    let same = stored.try_get::<Value, _>("scope").ok().as_ref()
        == Some(&source.active_scope_document)
        && stored.try_get::<String, _>("scope_key").ok().as_deref()
            == Some(source.active_scope.canonical_key().as_str())
        && stored.try_get::<Uuid, _>("supplier_id").ok() == Some(evidence.supplier.as_uuid())
        && stored
            .try_get::<Value, _>("evidence_document")
            .ok()
            .as_ref()
            == Some(&document)
        && stored.try_get::<i64, _>("source_event_position").ok()
            == Some(to_i64(source.global_sequence)?)
        && stored.try_get::<DateTime<Utc>, _>("captured_at").ok() == Some(evidence.captured_at);
    if !same {
        return Err(configuration("Procure-to-Pay Finance evidence conflicts"));
    }
    transaction
        .commit()
        .await
        .map_err(|_| configuration("Procure-to-Pay Finance evidence could not commit"))
}

#[derive(Debug, Clone, Copy, Default)]
struct TrustedProcureToPayAccess;

#[async_trait]
impl InventoryAuthorizationPort for TrustedProcureToPayAccess {
    async fn authorize(
        &self,
        request: InventoryAuthorizationRequest<'_>,
    ) -> Result<(), erpx_inventory::PortError> {
        if request.action == InventoryAction::ReceiveStock
            && request.actor.kind == ActorKind::SystemWorker
            && request.actor.session_id.is_none()
            && request.actor.represented_principal_ref.is_none()
            && request.actor.support_capability_id.is_none()
            && request.authority_scope == request.target_scope
            && request.authority_scope != &AuthorityScope::Platform
        {
            Ok(())
        } else {
            Err(erpx_inventory::PortError::Denied)
        }
    }
}

#[async_trait]
impl InventoryEntitlementPort for TrustedProcureToPayAccess {
    async fn ensure_inventory_enabled(
        &self,
        scope: &AuthorityScope,
        write: bool,
    ) -> Result<(), erpx_inventory::PortError> {
        if write && scope.validate().is_ok() && scope != &AuthorityScope::Platform {
            Ok(())
        } else {
            Err(erpx_inventory::PortError::EntitlementDenied)
        }
    }
}

#[async_trait]
impl PayablesAuthorizationPort for TrustedProcureToPayAccess {
    async fn authorize(
        &self,
        request: PayablesAuthorizationRequest<'_>,
    ) -> Result<(), erpx_finance::PortError> {
        if request.action == PayablesAction::CreatePayment
            && request.actor.kind == ActorKind::SystemWorker
            && request.actor.session_id.is_none()
            && request.actor.represented_principal_ref.is_none()
            && request.actor.support_capability_id.is_none()
            && request.authority_scope == request.target_scope
            && request.authority_scope != &AuthorityScope::Platform
        {
            Ok(())
        } else {
            Err(erpx_finance::PortError::Denied)
        }
    }
}

#[async_trait]
impl PayablesEntitlementPort for TrustedProcureToPayAccess {
    async fn ensure_payables_enabled(
        &self,
        scope: &AuthorityScope,
        write: bool,
    ) -> Result<(), erpx_finance::PortError> {
        if write && scope.validate().is_ok() && scope != &AuthorityScope::Platform {
            Ok(())
        } else {
            Err(erpx_finance::PortError::EntitlementDenied)
        }
    }
}

async fn apply_inventory_receipts(
    write: &WritePool,
    worker_id: Uuid,
    source: &SourceRow,
    receipt: &procurement::AcceptedGoodsReceiptFact,
) -> Result<(), WorkerRuntimeError> {
    let repository = InventoryWriteRepository::new(write.clone());
    for line in &receipt.lines {
        let stock_item_id = StockItemId::try_from_uuid(line.stock_item.as_uuid())
            .map_err(|_| configuration("Procure-to-Pay stock item is invalid"))?;
        let aggregate = repository
            .load(&source.active_scope, stock_item_id)
            .await
            .map_err(|_| configuration("Procure-to-Pay stock item is unavailable"))?
            .ok_or_else(|| configuration("Procure-to-Pay stock item does not exist"))?;
        let movement_id = deterministic_receipt_movement(source.event_id, line.line.as_uuid());
        let command_id = deterministic_command(INVENTORY_COMMAND_NAMESPACE, movement_id);
        let handler = InventoryCommandHandler::new(
            InventoryWriteRepository::new(write.clone()),
            InventoryCommandAdapter::new(write.clone()),
            TrustedProcureToPayAccess,
            TrustedProcureToPayAccess,
            SystemClock,
        );
        handler
            .handle(CommandEnvelope {
                contract_version: CONTRACT_VERSION,
                command_id,
                idempotency_key: IdempotencyKey::try_new(format!(
                    "procure-to-pay:inventory:{}:{}",
                    source.event_id, movement_id
                ))
                .map_err(|_| configuration("Procure-to-Pay inventory identity is invalid"))?,
                correlation_id: typed_correlation(source.correlation_id)?,
                causation_id: Some(typed_event(source.event_id)?),
                actor: worker_actor(worker_id),
                active_scope: source.active_scope.clone(),
                audit: worker_audit("receive accepted Procurement goods into Inventory"),
                expected_aggregate_version: Some(aggregate.version()),
                command: ReceiveStock {
                    stock_item_id,
                    movement_id: StockMovementId::try_from_uuid(movement_id)
                        .map_err(|_| configuration("Procure-to-Pay movement is invalid"))?,
                    quantity: line.accepted_quantity.units(),
                    procurement_line: Some(line.purchase_order_line),
                },
            })
            .await
            .map_err(|_| configuration("Procure-to-Pay Inventory command was rejected"))?;
    }
    Ok(())
}

async fn propose_payment(
    write: &WritePool,
    worker_id: Uuid,
    source: &SourceRow,
    fact: &finance::VendorPayablePostedFact,
) -> Result<(), WorkerRuntimeError> {
    if fact.payable_version == 0 || fact.payable_amount.minor_units() <= 0 {
        return Err(configuration("Procure-to-Pay payable fact is invalid"));
    }
    let payment_id = Uuid::new_v5(&PAYMENT_PROPOSAL_NAMESPACE, source.event_id.as_bytes());
    let command_id = deterministic_command(PAYMENT_COMMAND_NAMESPACE, payment_id);
    let payment_id = PaymentId::try_from_uuid(payment_id)
        .map_err(|_| configuration("Procure-to-Pay payment id is invalid"))?;
    let payable_id = PayableId::try_from_uuid(fact.payable.as_uuid())
        .map_err(|_| configuration("Procure-to-Pay payable id is invalid"))?;
    let adapter = SqlxPayablesAdapter::new(write.clone());
    let handler = PayablesCommandHandler::new(
        adapter.clone(),
        adapter.clone(),
        TrustedProcureToPayAccess,
        TrustedProcureToPayAccess,
        adapter,
        SystemClock,
    );
    handler
        .handle(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            command_id,
            idempotency_key: IdempotencyKey::try_new(format!(
                "procure-to-pay:payment:{}",
                source.event_id
            ))
            .map_err(|_| configuration("Procure-to-Pay payment identity is invalid"))?,
            correlation_id: typed_correlation(source.correlation_id)?,
            causation_id: Some(typed_event(source.event_id)?),
            actor: worker_actor(worker_id),
            active_scope: source.active_scope.clone(),
            audit: worker_audit("prepare a Finance payment proposal for a posted payable"),
            expected_aggregate_version: None,
            command: PayablesCommand::CreatePayment(CreatePayment {
                payment_id,
                scope: source.active_scope.clone(),
                payment_code: format!("AUTO-{}", payment_id.as_uuid().simple()),
                supplier: fact.supplier,
                amount: fact.payable_amount,
                allocations: vec![PaymentAllocation {
                    payable_id,
                    payable_version: fact.payable_version,
                    amount: fact.payable_amount,
                }],
            }),
        })
        .await
        .map_err(|_| configuration("Procure-to-Pay Finance payment proposal was rejected"))?;
    Ok(())
}

async fn record_processed(write: &WritePool, row: &SourceRow) -> Result<(), WorkerRuntimeError> {
    let mut transaction = write
        .begin()
        .await
        .map_err(|_| configuration("Procure-to-Pay receipt store is unavailable"))?;
    sqlx::query(
        "INSERT INTO erpx_procure_to_pay_write.source_receipts
         (event_id,global_sequence,event_type,processed_at)
         VALUES ($1,$2,$3,now()) ON CONFLICT DO NOTHING",
    )
    .bind(row.event_id)
    .bind(to_i64(row.global_sequence)?)
    .bind(&row.event_type)
    .execute(&mut *transaction)
    .await
    .map_err(|_| configuration("Procure-to-Pay source receipt could not persist"))?;
    let stored = sqlx::query(
        "SELECT global_sequence,event_type FROM erpx_procure_to_pay_write.source_receipts
         WHERE event_id=$1 FOR SHARE",
    )
    .bind(row.event_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|_| configuration("Procure-to-Pay source receipt is unavailable"))?;
    if stored.try_get::<i64, _>("global_sequence").ok() != Some(to_i64(row.global_sequence)?)
        || stored.try_get::<String, _>("event_type").ok().as_deref()
            != Some(row.event_type.as_str())
    {
        return Err(configuration("Procure-to-Pay source receipt conflicts"));
    }
    sqlx::query(
        "INSERT INTO erpx_procure_to_pay_write.process_cursor
         (process_name,global_sequence,updated_at) VALUES ($1,$2,now())
         ON CONFLICT (holding_id,process_name) DO UPDATE SET
           global_sequence=EXCLUDED.global_sequence,updated_at=EXCLUDED.updated_at
         WHERE erpx_procure_to_pay_write.process_cursor.global_sequence<EXCLUDED.global_sequence",
    )
    .bind(PROCESS_NAME)
    .bind(to_i64(row.global_sequence)?)
    .execute(&mut *transaction)
    .await
    .map_err(|_| configuration("Procure-to-Pay cursor could not advance"))?;
    transaction
        .commit()
        .await
        .map_err(|_| configuration("Procure-to-Pay receipt could not commit"))
}

fn deterministic_receipt_movement(event_id: Uuid, receipt_line: Uuid) -> Uuid {
    let mut seed = Vec::with_capacity(32);
    seed.extend_from_slice(event_id.as_bytes());
    seed.extend_from_slice(receipt_line.as_bytes());
    Uuid::new_v5(&RECEIPT_MOVEMENT_NAMESPACE, &seed)
}

fn deterministic_command(namespace: Uuid, value: Uuid) -> CommandId {
    CommandId::try_from_uuid(Uuid::new_v5(&namespace, value.as_bytes()))
        .expect("a UUIDv5 command id is non-nil")
}

fn worker_actor(worker_id: Uuid) -> ActorAttribution {
    ActorAttribution {
        principal_ref: format!("procure-to-pay-worker:{worker_id}"),
        kind: ActorKind::SystemWorker,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    }
}

fn worker_audit(purpose: &str) -> AuditContext {
    AuditContext {
        purpose: purpose.into(),
        customer_visible: false,
        module: Some("procure_to_pay".into()),
    }
}

fn typed_event(value: Uuid) -> Result<EventId, WorkerRuntimeError> {
    EventId::try_from_uuid(value).map_err(|_| configuration("Procure-to-Pay event id is invalid"))
}

fn typed_correlation(value: Uuid) -> Result<CorrelationId, WorkerRuntimeError> {
    CorrelationId::try_from_uuid(value)
        .map_err(|_| configuration("Procure-to-Pay correlation is invalid"))
}

fn ensure_holding_scope(
    scope: &AuthorityScope,
    holding_id: Uuid,
) -> Result<(), WorkerRuntimeError> {
    let matches = match scope {
        AuthorityScope::Platform => false,
        AuthorityScope::Holding { holding_id: value }
        | AuthorityScope::Company {
            holding_id: value, ..
        }
        | AuthorityScope::Branch {
            holding_id: value, ..
        }
        | AuthorityScope::Subtree {
            holding_id: value, ..
        } => *value == holding_id,
    };
    if matches && scope.validate().is_ok() {
        Ok(())
    } else {
        Err(configuration(
            "Procure-to-Pay source crossed a holding boundary",
        ))
    }
}

fn required_uuid(
    row: &sqlx::postgres::PgRow,
    name: &'static str,
) -> Result<Uuid, WorkerRuntimeError> {
    row.try_get::<Uuid, _>(name)
        .map_err(|_| configuration("Procure-to-Pay UUID is invalid"))
        .and_then(required_non_nil)
}

fn required_non_nil(value: Uuid) -> Result<Uuid, WorkerRuntimeError> {
    if value.is_nil() {
        Err(configuration("Procure-to-Pay UUID is nil"))
    } else {
        Ok(value)
    }
}

fn positive_u64(
    row: &sqlx::postgres::PgRow,
    name: &'static str,
) -> Result<u64, WorkerRuntimeError> {
    let value = row
        .try_get::<i64, _>(name)
        .map_err(|_| configuration("Procure-to-Pay sequence is invalid"))?;
    u64::try_from(value)
        .ok()
        .filter(|value| *value != 0)
        .ok_or_else(|| configuration("Procure-to-Pay sequence is invalid"))
}

fn positive_u16(
    row: &sqlx::postgres::PgRow,
    name: &'static str,
) -> Result<u16, WorkerRuntimeError> {
    let value = row
        .try_get::<i32, _>(name)
        .map_err(|_| configuration("Procure-to-Pay event version is invalid"))?;
    u16::try_from(value)
        .ok()
        .filter(|value| *value != 0)
        .ok_or_else(|| configuration("Procure-to-Pay event version is invalid"))
}

fn to_i64(value: u64) -> Result<i64, WorkerRuntimeError> {
    i64::try_from(value).map_err(|_| configuration("Procure-to-Pay sequence exceeds storage"))
}

fn configuration(message: &'static str) -> WorkerRuntimeError {
    WorkerRuntimeError::Configuration(message.into())
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use erpx_integration_contracts::{
        AggregateKind, CurrencyCode, MoneyAmount, Quantity, RecordTypeId, RecordVersionRef,
        StableRecordRef, finance, inventory, procurement,
    };
    use erpx_kernel::{AggregateId, AuthorityScope, CorrelationId, RecordId};

    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn record(record_type: &str, id_value: u128, version: u64) -> RecordVersionRef {
        let record_type = RecordTypeId::try_new(record_type).unwrap();
        RecordVersionRef::try_new(
            StableRecordRef::try_new(
                RecordId::try_from_uuid(id(id_value)).unwrap(),
                record_type.descriptor().owner,
                record_type,
            )
            .unwrap(),
            version,
        )
        .unwrap()
    }

    fn receipt_event(event_id: u128) -> ProcureToPaySourceEvent {
        let payload =
            procurement::Event::GoodsReceiptAccepted(procurement::AcceptedGoodsReceiptFact {
                receipt: record("procurement.receipt", 10, 3),
                purchase_order: record("procurement.purchase-order", 11, 5),
                lines: vec![procurement::AcceptedGoodsReceiptLineFact {
                    line: procurement::GoodsReceiptLineRef::try_from_uuid(id(12)).unwrap(),
                    purchase_order_line: procurement::PurchaseOrderLineRef::try_from_uuid(id(13))
                        .unwrap(),
                    stock_item: inventory::StockItemRef::try_from_uuid(id(14)).unwrap(),
                    accepted_quantity: Quantity::try_new(7, 0).unwrap(),
                }],
                accepted_at: Utc.with_ymd_and_hms(2026, 8, 21, 9, 0, 0).unwrap(),
            });
        ProcureToPaySourceEvent::Procurement(
            IntegrationEventEnvelope::try_new(
                EventId::try_from_uuid(id(event_id)).unwrap(),
                AuthorityScope::Company {
                    holding_id: id(2),
                    company_id: id(3),
                },
                AggregateKind::try_new("goods_receipt").unwrap(),
                AggregateId::try_from_uuid(id(10)).unwrap(),
                3,
                CorrelationId::try_from_uuid(id(15)).unwrap(),
                None,
                Utc.with_ymd_and_hms(2026, 8, 21, 9, 0, 0).unwrap(),
                payload,
            )
            .unwrap(),
        )
    }

    fn payable_event(event_id: u128) -> ProcureToPaySourceEvent {
        let currency = CurrencyCode::try_new("THB").unwrap();
        let payload = finance::Event::VendorPayablePosted(finance::VendorPayablePostedFact {
            vendor_invoice: finance::VendorInvoiceRef::try_from_uuid(id(20)).unwrap(),
            payable: finance::PayableRef::try_from_uuid(id(21)).unwrap(),
            payable_version: 1,
            supplier: procurement::SupplierRef::try_from_uuid(id(22)).unwrap(),
            purchase_order: record("procurement.purchase-order", 23, 4),
            goods_receipt: record("procurement.receipt", 24, 2),
            payable_amount: MoneyAmount::new(12_300, currency),
            posted_at: Utc.with_ymd_and_hms(2026, 8, 21, 10, 0, 0).unwrap(),
        });
        ProcureToPaySourceEvent::Finance(
            IntegrationEventEnvelope::try_new(
                EventId::try_from_uuid(id(event_id)).unwrap(),
                AuthorityScope::Company {
                    holding_id: id(2),
                    company_id: id(3),
                },
                AggregateKind::try_new("vendor_invoice").unwrap(),
                AggregateId::try_from_uuid(id(20)).unwrap(),
                2,
                CorrelationId::try_from_uuid(id(25)).unwrap(),
                None,
                Utc.with_ymd_and_hms(2026, 8, 21, 10, 0, 0).unwrap(),
                payload,
            )
            .unwrap(),
        )
    }

    #[test]
    fn accepted_receipt_stages_one_deterministic_inventory_command_per_line() {
        let source = receipt_event(1);
        let mut state = ProcureToPayConsumerState::default();
        let ProcureToPayConsumeOutcome::Staged(first) = state.consume(10, &source).unwrap() else {
            panic!("receipt must stage inventory")
        };
        let ProcureToPayConsumeOutcome::Replay(replay) = state.consume(10, &source).unwrap() else {
            panic!("receipt replay must be stable")
        };
        assert_eq!(first, replay);
        assert!(matches!(
            &first[0].handoff,
            ProcureToPayHandoffKind::ReceiveInventory { quantity: 7, .. }
        ));
    }

    #[test]
    fn posted_payable_proposes_but_does_not_release_a_payment() {
        let source = payable_event(2);
        let mut state = ProcureToPayConsumerState::default();
        let ProcureToPayConsumeOutcome::Staged(values) = state.consume(11, &source).unwrap() else {
            panic!("payable must stage payment proposal")
        };
        assert!(matches!(
            &values[0].handoff,
            ProcureToPayHandoffKind::ProposePayment {
                amount_minor: 12_300,
                currency,
                ..
            } if currency == "THB"
        ));
    }

    #[test]
    fn three_way_evidence_uses_exact_price_evidence_without_rounding() {
        let currency = CurrencyCode::try_new("THB").unwrap();
        assert_eq!(
            proportional_amount(
                MoneyAmount::new(12_000, currency),
                Quantity::try_new(6, 0).unwrap(),
                Quantity::try_new(2, 0).unwrap(),
            )
            .unwrap(),
            MoneyAmount::new(4_000, currency)
        );
        assert!(
            proportional_amount(
                MoneyAmount::new(100, currency),
                Quantity::try_new(3, 0).unwrap(),
                Quantity::try_new(1, 0).unwrap(),
            )
            .is_err()
        );
    }

    #[test]
    fn runtime_event_inventory_is_exact_and_payment_release_is_absent() {
        assert_eq!(
            RELEVANT_EVENT_TYPES,
            [
                "procurement.purchase_order_released",
                "procurement.goods_receipt_accepted",
                "finance.vendor_payable_posted",
            ]
        );
        assert!(!RELEVANT_EVENT_TYPES.contains(&"finance.payment_released"));
    }
}
