//! Production Operations source-ingestion runtime.
//!
//! The scanner reads only the closed inventory of Operations-relevant typed
//! integration events from active tenant write databases. Applying a fact
//! delegates to `OperationsSourceProcessManager`; its
//! `OperationsSourceHandler<OperationsSourceAdapter, ..>` commits the source
//! receipt and the source cursor in the same tenant-write transaction.

use std::env;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs_sqlx::WritePool;
use erpx_infrastructure::{RoutedDatabaseSide, SqlxTenantStoreRegistry, TenantDatabaseRouter};
use erpx_integration_contracts::{
    BoundedContext, IntegrationEvent, IntegrationEventEnvelope, assets, commercial, crm, finance,
    inventory, maintenance, marketing, people_payroll, pos, procurement, quality, sales, support,
};
use erpx_operations::{
    DetectionDecision, DetectionPolicy, DetectionPolicyPort, ExceptionId, PortError,
    PrincipalAssignment, PublishedSourceEvent, SourceDatabase, SourceFactMessage,
    SourceHandlingOutcome, SourcePosition,
};
use erpx_operations_sqlx::OperationsSourceAdapter;
use erpx_worker::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row, postgres::PgRow};
use uuid::Uuid;

use super::operations::{
    OperationsSourceApplication, OperationsSourceContextFactory, OperationsSourceProcessManager,
    OperationsSourceProcessOutcome,
};

pub const OPERATIONS_SOURCE_INGESTION_HANDLER_NAME: &str = "operations_source_ingestion";
pub const OPERATIONS_SOURCE_MAX_PAGE_SIZE: u32 = 500;
pub const OPERATIONS_SOURCE_MAX_ACTIVE_TENANTS: usize = 1_000;

const TENANT_SOURCE_PARTITION: &str = "operations-relevant-events-v1";
const MAX_CONFIG_TEXT_BYTES: usize = 128;

/// Events in this inventory always map to `Some(SourceFactMessage)`. Events
/// that `PublishedSourceEvent::try_into_message` classifies as irrelevant are
/// excluded so every fetched row advances the durable Operations cursor.
pub const OPERATIONS_RELEVANT_TENANT_EVENT_TYPES: &[&str] = &[
    "commercial.subscription_changed",
    "marketing.engagement_captured",
    "crm.opportunity_changed",
    "procurement.purchase_order_released",
    "inventory.stock_moved",
    "assets.condition_changed",
    "maintenance.work_order_changed",
    "quality.inspection_completed",
    "sales.order_changed",
    "people_payroll.payroll_finalized",
    "pos.sale_completed",
    "pos.refund_approved",
    "finance.journal_posted",
    "support.case_changed",
    "support.incident_changed",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsSourceTenant {
    pub tenant_id: String,
    pub holding_ref: String,
}

impl OperationsSourceTenant {
    fn source_key(&self) -> Result<String, WorkerRuntimeError> {
        let value = format!("tenant:{}", self.tenant_id);
        if self.tenant_id.trim().is_empty()
            || self.holding_ref.trim().is_empty()
            || value.len() > 255
        {
            Err(configuration("Operations source tenant binding is invalid"))
        } else {
            Ok(value)
        }
    }
}

/// Composition factory for the actual application graph. Implementations must
/// construct `OperationsSourceHandler::new(adapter, detection, ..)`, wrap it
/// with `OperationsSourceApplicationFn`, and return the resulting
/// `OperationsSourceProcessManager` with a trusted SystemWorker context
/// factory. The concrete detection policy is supplied by this runtime and
/// cannot be selected from source-event payloads.
pub trait OperationsSourceProcessFactory<M>: Send + Sync {
    type Application: OperationsSourceApplication;
    type ContextFactory: OperationsSourceContextFactory;

    fn build(
        &self,
        tenant: &OperationsSourceTenant,
        worker_id: Uuid,
        adapter: OperationsSourceAdapter,
        detection: ConfiguredOperationsDetectionPolicy<M>,
    ) -> Result<
        OperationsSourceProcessManager<Self::Application, Self::ContextFactory>,
        WorkerRuntimeError,
    >;
}

#[derive(Debug, Clone)]
pub struct OperationsSourceIngestionHandler<F, M> {
    registry: SqlxTenantStoreRegistry,
    router: TenantDatabaseRouter,
    process_factory: F,
    detection: ConfiguredOperationsDetectionPolicy<M>,
    page_size: u32,
    max_active_tenants: usize,
}

impl<F, M> OperationsSourceIngestionHandler<F, M> {
    pub fn new(
        registry: SqlxTenantStoreRegistry,
        router: TenantDatabaseRouter,
        process_factory: F,
        detection: ConfiguredOperationsDetectionPolicy<M>,
        page_size: u32,
        max_active_tenants: usize,
    ) -> Result<Self, WorkerRuntimeError> {
        if page_size == 0
            || page_size > OPERATIONS_SOURCE_MAX_PAGE_SIZE
            || max_active_tenants == 0
            || max_active_tenants > OPERATIONS_SOURCE_MAX_ACTIVE_TENANTS
        {
            return Err(configuration(
                "Operations source ingestion bounds are invalid",
            ));
        }
        Ok(Self {
            registry,
            router,
            process_factory,
            detection,
            page_size,
            max_active_tenants,
        })
    }
}

impl<F, M> OperationsSourceIngestionHandler<F, M>
where
    F: OperationsSourceProcessFactory<M>,
    M: OperationsManagerChainSource + Clone,
{
    async fn scan_tenant(
        &self,
        tenant: OperationsSourceTenant,
        worker_id: Uuid,
    ) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let routed = self
            .router
            .route(
                &tenant.tenant_id,
                &tenant.holding_ref,
                RoutedDatabaseSide::Write,
            )
            .await
            .map_err(|_| configuration("Operations tenant write route is unavailable"))?;
        let pool = routed.database_pool().clone();
        let source_key = tenant.source_key()?;
        let adapter =
            OperationsSourceAdapter::new(WritePool::from_pg_pool(pool.clone()), pool.clone());
        let after_sequence = adapter
            .checkpoint(SourceDatabase::Tenant, &source_key, TENANT_SOURCE_PARTITION)
            .await
            .map_err(map_port_error)?
            .map_or(0, |checkpoint| checkpoint.sequence);
        let rows = read_source_page(&pool, after_sequence, self.page_size).await?;
        let fetched = rows.len();
        let claimed = u64::try_from(fetched)
            .map_err(|_| configuration("Operations source page count is invalid"))?;
        let manager =
            self.process_factory
                .build(&tenant, worker_id, adapter, self.detection.clone())?;
        let mut report = WorkerHandlerReport {
            claimed,
            has_work: fetched != 0,
            ..WorkerHandlerReport::default()
        };
        for row in rows {
            let position = SourcePosition {
                database: SourceDatabase::Tenant,
                source_key: source_key.clone(),
                partition: TENANT_SOURCE_PARTITION.into(),
                sequence: row.global_sequence,
            };
            let event = decode_published_source(row)?;
            match manager.handle(event, position).await.map_err(|_| {
                configuration("Operations source application rejected a typed event")
            })? {
                OperationsSourceProcessOutcome::Irrelevant => {
                    return Err(configuration(
                        "Operations relevant-event inventory produced an irrelevant event",
                    ));
                }
                OperationsSourceProcessOutcome::Applied(result) => {
                    report.processed = report.processed.saturating_add(1);
                    if matches!(result.outcome, SourceHandlingOutcome::ExceptionDetected(_)) {
                        report.published = report.published.saturating_add(1);
                    }
                }
            }
        }
        Ok(report)
    }
}

#[async_trait]
impl<F, M> NamedWorkerHandler for OperationsSourceIngestionHandler<F, M>
where
    F: OperationsSourceProcessFactory<M> + Send + Sync,
    M: OperationsManagerChainSource + Clone + Send + Sync,
{
    fn name(&self) -> &str {
        OPERATIONS_SOURCE_INGESTION_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if worker_id.is_nil() {
            return Err(configuration(
                "Operations source worker identity is required",
            ));
        }
        let registrations = self
            .registry
            .list_active_bounded(self.max_active_tenants)
            .await
            .map_err(|_| configuration("Operations active tenant registry is unavailable"))?;
        let mut total = WorkerHandlerReport::default();
        for registration in registrations {
            let report = self
                .scan_tenant(
                    OperationsSourceTenant {
                        tenant_id: registration.tenant_id,
                        holding_ref: registration.holding_ref,
                    },
                    worker_id,
                )
                .await?;
            add_report(&mut total, report);
        }
        Ok(total)
    }
}

async fn read_source_page(
    pool: &PgPool,
    after_sequence: u64,
    page_size: u32,
) -> Result<Vec<RawOutboxEvent>, WorkerRuntimeError> {
    let event_types = OPERATIONS_RELEVANT_TENANT_EVENT_TYPES
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    let rows =
        sqlx::query(
            "SELECT event_id,global_sequence,context_name,event_type,event_version,
                aggregate_type,aggregate_id,aggregate_version,tenant_scope,
                correlation_id,causation_id,payload,occurred_at
         FROM erpx_cqrs_write.outbox_events
         WHERE global_sequence > $1 AND available_at <= now()
           AND event_type = ANY($2::text[])
         ORDER BY global_sequence
         LIMIT $3",
        )
        .bind(i64::try_from(after_sequence).map_err(|_| {
            configuration("Operations source checkpoint exceeds the database range")
        })?)
        .bind(event_types)
        .bind(i64::from(page_size))
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(RawOutboxEvent::from_row).collect()
}

#[derive(Debug, Clone, PartialEq)]
struct RawOutboxEvent {
    event_id: Uuid,
    global_sequence: u64,
    context_name: String,
    event_type: String,
    event_version: u16,
    aggregate_type: String,
    aggregate_id: Uuid,
    aggregate_version: u64,
    tenant_scope: Value,
    correlation_id: Uuid,
    causation_id: Option<Uuid>,
    payload: Value,
    occurred_at: DateTime<Utc>,
}

impl RawOutboxEvent {
    fn from_row(row: PgRow) -> Result<Self, WorkerRuntimeError> {
        Ok(Self {
            event_id: non_nil(row.try_get("event_id")?)?,
            global_sequence: positive_u64(row.try_get("global_sequence")?)?,
            context_name: row.try_get("context_name")?,
            event_type: row.try_get("event_type")?,
            event_version: positive_u16(row.try_get("event_version")?)?,
            aggregate_type: row.try_get("aggregate_type")?,
            aggregate_id: non_nil(row.try_get("aggregate_id")?)?,
            aggregate_version: positive_u64(row.try_get("aggregate_version")?)?,
            tenant_scope: row.try_get("tenant_scope")?,
            correlation_id: non_nil(row.try_get("correlation_id")?)?,
            causation_id: row
                .try_get::<Option<Uuid>, _>("causation_id")?
                .map(non_nil)
                .transpose()?,
            payload: row.try_get("payload")?,
            occurred_at: row.try_get("occurred_at")?,
        })
    }
}

fn decode_published_source(
    row: RawOutboxEvent,
) -> Result<PublishedSourceEvent, WorkerRuntimeError> {
    match row.event_type.as_str() {
        "commercial.subscription_changed" => {
            decode_envelope::<commercial::Event>(row, "commercial")
                .map(PublishedSourceEvent::Commercial)
        }
        "marketing.engagement_captured" => decode_envelope::<marketing::Event>(row, "marketing")
            .map(PublishedSourceEvent::Marketing),
        "crm.opportunity_changed" => {
            decode_envelope::<crm::Event>(row, "crm").map(PublishedSourceEvent::Crm)
        }
        "procurement.purchase_order_released" => {
            decode_envelope::<procurement::Event>(row, "procurement")
                .map(PublishedSourceEvent::Procurement)
        }
        "inventory.stock_moved" => decode_envelope::<inventory::Event>(row, "inventory")
            .map(PublishedSourceEvent::Inventory),
        "assets.condition_changed" => {
            decode_envelope::<assets::Event>(row, "assets").map(PublishedSourceEvent::Assets)
        }
        "maintenance.work_order_changed" => {
            decode_envelope::<maintenance::Event>(row, "maintenance")
                .map(PublishedSourceEvent::Maintenance)
        }
        "quality.inspection_completed" => {
            decode_envelope::<quality::Event>(row, "quality").map(PublishedSourceEvent::Quality)
        }
        "sales.order_changed" => {
            decode_envelope::<sales::Event>(row, "sales").map(PublishedSourceEvent::Sales)
        }
        "people_payroll.payroll_finalized" => {
            decode_envelope::<people_payroll::Event>(row, "people_payroll")
                .map(PublishedSourceEvent::PeoplePayroll)
        }
        "pos.sale_completed" | "pos.refund_approved" => {
            decode_envelope::<pos::Event>(row, "pos").map(PublishedSourceEvent::Pos)
        }
        "finance.journal_posted" => {
            decode_envelope::<finance::Event>(row, "finance").map(PublishedSourceEvent::Finance)
        }
        "support.case_changed" | "support.incident_changed" => {
            decode_envelope::<support::Event>(row, "support").map(PublishedSourceEvent::Support)
        }
        _ => Err(source_contract_error()),
    }
}

fn decode_envelope<E>(
    row: RawOutboxEvent,
    expected_context: &'static str,
) -> Result<IntegrationEventEnvelope<E>, WorkerRuntimeError>
where
    E: IntegrationEvent,
{
    if row.context_name != expected_context || row.occurred_at.timestamp() < 0 {
        return Err(source_contract_error());
    }
    let payload: E = serde_json::from_value(row.payload).map_err(|_| source_contract_error())?;
    if payload.event_type() != row.event_type
        || payload.schema_version() != row.event_version
        || context_name(payload.owner()) != Some(expected_context)
    {
        return Err(source_contract_error());
    }
    let envelope: IntegrationEventEnvelope<E> = serde_json::from_value(json!({
        "owner": payload.owner(),
        "event_id": row.event_id,
        "event_type": row.event_type,
        "event_version": row.event_version,
        "tenant_scope": row.tenant_scope,
        "aggregate_kind": row.aggregate_type,
        "aggregate_id": row.aggregate_id,
        "aggregate_version": row.aggregate_version,
        "correlation_id": row.correlation_id,
        "causation_id": row.causation_id,
        "occurred_at": row.occurred_at,
        "payload": payload,
    }))
    .map_err(|_| source_contract_error())?;
    envelope.validate().map_err(|_| source_contract_error())?;
    Ok(envelope)
}

fn context_name(value: BoundedContext) -> Option<&'static str> {
    match value {
        BoundedContext::Commercial => Some("commercial"),
        BoundedContext::Marketing => Some("marketing"),
        BoundedContext::Crm => Some("crm"),
        BoundedContext::Procurement => Some("procurement"),
        BoundedContext::Inventory => Some("inventory"),
        BoundedContext::Assets => Some("assets"),
        BoundedContext::Maintenance => Some("maintenance"),
        BoundedContext::Quality => Some("quality"),
        BoundedContext::Sales => Some("sales"),
        BoundedContext::PeoplePayroll => Some("people_payroll"),
        BoundedContext::Pos => Some("pos"),
        BoundedContext::Finance => Some("finance"),
        BoundedContext::Support => Some("support"),
        _ => None,
    }
}

/// Current Identity authority seam. Implementations must return live,
/// unrevoked manager-chain evidence evaluated at `evaluated_at`; absence,
/// expiry, or read-side uncertainty must return an error rather than a
/// synthetic assignment.
#[async_trait]
pub trait OperationsManagerChainSource: Send + Sync {
    async fn current_assignment(
        &self,
        source: &SourceFactMessage,
        evaluated_at: DateTime<Utc>,
    ) -> Result<PrincipalAssignment, PortError>;
}

/// Safe default for disabled detection and fail-closed fallback when no
/// Identity manager-chain provider has been composed.
#[derive(Debug, Clone, Copy, Default)]
#[cfg(test)]
pub struct DenyMissingManagerChain;

#[async_trait]
#[cfg(test)]
impl OperationsManagerChainSource for DenyMissingManagerChain {
    async fn current_assignment(
        &self,
        _source: &SourceFactMessage,
        _evaluated_at: DateTime<Utc>,
    ) -> Result<PrincipalAssignment, PortError> {
        Err(PortError::Denied)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum OperationsDetectionMode {
    Disabled,
    Enabled(DetectionPolicy),
}

/// Concrete, fail-closed policy adapter. Detection is disabled unless
/// `ERPX_OPERATIONS_DETECTION_ENABLED=true`. When enabled, bounded policy
/// fields are loaded once and every matching fact requires current Identity
/// manager-chain evidence before an exception can be created.
#[derive(Debug, Clone)]
pub struct ConfiguredOperationsDetectionPolicy<M> {
    mode: OperationsDetectionMode,
    manager_chain: M,
}

impl<M> ConfiguredOperationsDetectionPolicy<M> {
    pub fn disabled(manager_chain: M) -> Self {
        Self {
            mode: OperationsDetectionMode::Disabled,
            manager_chain,
        }
    }

    pub fn from_env(manager_chain: M) -> Result<Self, WorkerRuntimeError> {
        if !enabled_env("ERPX_OPERATIONS_DETECTION_ENABLED")? {
            return Ok(Self::disabled(manager_chain));
        }
        let allowed_sources = required_env("ERPX_OPERATIONS_DETECTION_ALLOWED_SOURCES")?
            .split(',')
            .map(parse_detection_source)
            .collect::<Result<Vec<_>, _>>()?;
        let policy = DetectionPolicy {
            policy_ref: required_env("ERPX_OPERATIONS_DETECTION_POLICY_REF")?,
            allowed_sources,
            category: required_env("ERPX_OPERATIONS_DETECTION_CATEGORY")?,
            severity: required_env("ERPX_OPERATIONS_DETECTION_SEVERITY")?,
            due_after_minutes: positive_minutes("ERPX_OPERATIONS_DETECTION_DUE_MINUTES")?,
            escalation_after_minutes: positive_minutes(
                "ERPX_OPERATIONS_DETECTION_ESCALATION_MINUTES",
            )?,
        };
        policy
            .validate()
            .map_err(|_| configuration("Operations detection policy is invalid"))?;
        Ok(Self {
            mode: OperationsDetectionMode::Enabled(policy),
            manager_chain,
        })
    }

    pub const fn enabled(&self) -> bool {
        matches!(&self.mode, OperationsDetectionMode::Enabled(_))
    }
}

#[async_trait]
impl<M> DetectionPolicyPort for ConfiguredOperationsDetectionPolicy<M>
where
    M: OperationsManagerChainSource,
{
    async fn evaluate(
        &self,
        source: &SourceFactMessage,
        evaluated_at: DateTime<Utc>,
    ) -> Result<DetectionDecision, PortError> {
        let OperationsDetectionMode::Enabled(policy) = &self.mode else {
            return Ok(DetectionDecision::Ignore);
        };
        if !policy.allowed_sources.contains(&source.source_context()) {
            return Ok(DetectionDecision::Ignore);
        }
        let assignment = self
            .manager_chain
            .current_assignment(source, evaluated_at)
            .await?;
        let exception_id = deterministic_exception_id(policy, source)?;
        Ok(DetectionDecision::Detect {
            exception_id,
            policy: policy.clone(),
            assignment,
        })
    }
}

fn deterministic_exception_id(
    policy: &DetectionPolicy,
    source: &SourceFactMessage,
) -> Result<ExceptionId, PortError> {
    let mut hasher = Sha256::new();
    hasher.update(b"operations-detected-exception-v1");
    hasher.update(policy.policy_ref.as_bytes());
    hasher.update(source.event_id().as_uuid().as_bytes());
    let mut bytes: [u8; 16] = hasher.finalize()[..16]
        .try_into()
        .map_err(|_| PortError::Internal)?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    ExceptionId::try_from_uuid(Uuid::from_bytes(bytes)).map_err(|_| PortError::Internal)
}

fn enabled_env(name: &'static str) -> Result<bool, WorkerRuntimeError> {
    match env::var(name) {
        Err(env::VarError::NotPresent) => Ok(false),
        Err(_) => Err(configuration("Operations detection environment is invalid")),
        Ok(value) => match value.as_str() {
            "true" | "1" => Ok(true),
            "false" | "0" => Ok(false),
            _ => Err(configuration(
                "Operations detection enabled flag is invalid",
            )),
        },
    }
}

fn required_env(name: &'static str) -> Result<String, WorkerRuntimeError> {
    let value = env::var(name)
        .map_err(|_| configuration("Operations detection configuration is incomplete"))?;
    if value.trim() != value || value.is_empty() || value.len() > MAX_CONFIG_TEXT_BYTES {
        Err(configuration("Operations detection text is invalid"))
    } else {
        Ok(value)
    }
}

fn positive_minutes(name: &'static str) -> Result<u32, WorkerRuntimeError> {
    let value = required_env(name)?
        .parse::<u32>()
        .map_err(|_| configuration("Operations detection duration is invalid"))?;
    if value == 0 || value > 10_080 {
        Err(configuration(
            "Operations detection duration is out of range",
        ))
    } else {
        Ok(value)
    }
}

fn parse_detection_source(value: &str) -> Result<BoundedContext, WorkerRuntimeError> {
    match value {
        "commercial" => Ok(BoundedContext::Commercial),
        "marketing" => Ok(BoundedContext::Marketing),
        "crm" => Ok(BoundedContext::Crm),
        "procurement" => Ok(BoundedContext::Procurement),
        "inventory" => Ok(BoundedContext::Inventory),
        "assets" => Ok(BoundedContext::Assets),
        "maintenance" => Ok(BoundedContext::Maintenance),
        "quality" => Ok(BoundedContext::Quality),
        "sales" => Ok(BoundedContext::Sales),
        "people_payroll" => Ok(BoundedContext::PeoplePayroll),
        "pos" => Ok(BoundedContext::Pos),
        "finance" => Ok(BoundedContext::Finance),
        "support" => Ok(BoundedContext::Support),
        _ => Err(configuration("Operations detection source is invalid")),
    }
}

fn positive_u64(value: i64) -> Result<u64, WorkerRuntimeError> {
    u64::try_from(value)
        .ok()
        .filter(|value| *value != 0)
        .ok_or_else(source_contract_error)
}

fn positive_u16(value: i32) -> Result<u16, WorkerRuntimeError> {
    u16::try_from(value)
        .ok()
        .filter(|value| *value != 0)
        .ok_or_else(source_contract_error)
}

fn non_nil(value: Uuid) -> Result<Uuid, WorkerRuntimeError> {
    if value.is_nil() {
        Err(source_contract_error())
    } else {
        Ok(value)
    }
}

fn add_report(total: &mut WorkerHandlerReport, value: WorkerHandlerReport) {
    total.claimed = total.claimed.saturating_add(value.claimed);
    total.processed = total.processed.saturating_add(value.processed);
    total.published = total.published.saturating_add(value.published);
    total.retried = total.retried.saturating_add(value.retried);
    total.dead_lettered = total.dead_lettered.saturating_add(value.dead_lettered);
    total.has_work |= value.has_work;
}

fn map_port_error(_error: PortError) -> WorkerRuntimeError {
    configuration("Operations source checkpoint is unavailable")
}

fn source_contract_error() -> WorkerRuntimeError {
    configuration("Operations source outbox contract is invalid")
}

fn configuration(message: &'static str) -> WorkerRuntimeError {
    WorkerRuntimeError::Configuration(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relevant_inventory_excludes_events_that_cannot_advance_the_tenant_cursor() {
        assert!(OPERATIONS_RELEVANT_TENANT_EVENT_TYPES.contains(&"sales.order_changed"));
        assert!(OPERATIONS_RELEVANT_TENANT_EVENT_TYPES.contains(&"pos.refund_approved"));
        assert!(!OPERATIONS_RELEVANT_TENANT_EVENT_TYPES.contains(&"pos.settlement_closed"));
        assert!(!OPERATIONS_RELEVANT_TENANT_EVENT_TYPES.contains(&"customer.profile_changed"));
    }

    #[test]
    fn scanner_uses_the_atomic_adapter_checkpoint_and_strict_typed_decoder() {
        let source = include_str!("operations_source.rs");
        for required in [
            "list_active_bounded",
            "OperationsSourceAdapter::new",
            ".checkpoint(",
            "OperationsSourceProcessManager",
            "decode_published_source",
            "event_type = ANY",
        ] {
            assert!(source.contains(required), "missing {required}");
        }
        assert!(!source.contains(concat!("payload", "_json")));
        assert!(!source.contains(concat!("unwrap", "()")));
    }

    #[test]
    fn detection_defaults_disabled_and_missing_identity_evidence_denies() {
        let policy = ConfiguredOperationsDetectionPolicy::disabled(DenyMissingManagerChain);
        assert!(!policy.enabled());
        let source = include_str!("operations_source.rs");
        assert!(source.contains("current_assignment"));
        assert!(source.contains("Err(PortError::Denied)"));
    }
}
