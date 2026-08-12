//! Maintenance projection and typed Assets-fact inbox runtimes.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use erpx_cqrs::{CONTRACT_VERSION, CommandEnvelope, CommandHandler, CqrsError, IdempotencyKey};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionEvent, ProjectionPool, ProjectionRun, ProjectionRunner,
    ProjectionRunnerConfig, ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};
use erpx_integration_contracts::{
    AggregateKind, IntegrationEvent, IntegrationEventEnvelope, assets,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, EventId, SystemClock,
    TenantScope,
};
use erpx_maintenance::{
    AuthorizationPort, EntitlementPort, MaintenanceAction, OpenWorkOrder, PortError, WorkOrderId,
    ports::AuthorizationRequest,
};
use erpx_maintenance_sqlx::{
    ASSET_FACT_EVENT_TYPE, MaintenanceAssetFactInbox, MaintenanceAssetFactRepository,
    MaintenanceCommandAdapter, MaintenanceWriteRepository, WorkOrderProjector,
};
use sqlx::PgPool;
use uuid::Uuid;

const MAINTENANCE_REQUEST_EVENT_TYPE: &str = "assets.maintenance_requested";
const WORK_ORDER_NAMESPACE: Uuid = Uuid::from_u128(0x236d_9e8a_7c0a_5ee4_a4ed_b745_88d9_1a7f);
const COMMAND_NAMESPACE: Uuid = Uuid::from_u128(0xa708_8ee3_d9cd_5e7d_aa6d_19e2_a7bc_d4ba);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenanceProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MaintenanceProjectionFactory;

impl MaintenanceProjectionFactory {
    pub fn registration(self) -> MaintenanceProjectionRegistration {
        let projector = WorkOrderProjector::new();
        MaintenanceProjectionRegistration {
            name: projector.name().into(),
            event_types: projector.event_types().to_vec(),
        }
    }

    pub fn build(
        self,
        write: WritePool,
        projection: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<MaintenanceProjectionRuntime, CqrsSqlxError> {
        let store = ProjectionStore::new(projection);
        let runner = ProjectionRunner::new(ProjectionSource::new(write), store.clone(), config)?;
        Ok(MaintenanceProjectionRuntime {
            projector: WorkOrderProjector::new(),
            runner,
            store,
        })
    }
}

#[derive(Debug, Clone)]
pub struct MaintenanceProjectionRuntime {
    projector: WorkOrderProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}

impl MaintenanceProjectionRuntime {
    pub fn registration(&self) -> MaintenanceProjectionRegistration {
        MaintenanceProjectionFactory.registration()
    }

    pub async fn run_once(&self) -> Result<ProjectionRun, CqrsSqlxError> {
        self.runner.run_active(&self.projector).await
    }

    pub async fn begin_rebuild(&self) -> Result<RebuildHandle, CqrsSqlxError> {
        self.store.begin_rebuild(&self.projector).await
    }

    pub async fn run_rebuild_page(
        &self,
        handle: &RebuildHandle,
        after_sequence: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        self.runner
            .run_rebuild(&self.projector, handle, after_sequence)
            .await
    }

    pub async fn activate_rebuild(&self, handle: &RebuildHandle) -> Result<(), CqrsSqlxError> {
        self.store.activate_rebuild(handle).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenanceAssetFactRegistration {
    pub event_types: [&'static str; 2],
}

#[derive(Debug, Clone, Copy, Default)]
struct TrustedMaintenanceRequestAccess;

#[async_trait::async_trait]
impl AuthorizationPort for TrustedMaintenanceRequestAccess {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError> {
        if request.action == MaintenanceAction::Open
            && request.actor.kind == ActorKind::SystemWorker
            && request.actor.session_id.is_none()
            && request.actor.represented_principal_ref.is_none()
            && request.actor.support_capability_id.is_none()
            && request.authority_scope == request.target_scope
            && request.target_scope != &TenantScope::Platform
        {
            Ok(())
        } else {
            Err(PortError::Denied)
        }
    }
}

#[async_trait::async_trait]
impl EntitlementPort for TrustedMaintenanceRequestAccess {
    async fn ensure_maintenance_enabled(
        &self,
        scope: &TenantScope,
        write: bool,
    ) -> Result<(), PortError> {
        if write && scope.validate().is_ok() && scope != &TenantScope::Platform {
            Ok(())
        } else {
            Err(PortError::EntitlementDenied)
        }
    }
}

/// Targeted consumer called by root worker orchestration after it has decoded
/// an Assets integration envelope. No Assets aggregate can enter this API.
#[derive(Debug, Clone)]
pub struct MaintenanceAssetFactRuntime {
    inbox: MaintenanceAssetFactInbox,
    tenant_write_pool: PgPool,
    source: Option<ProjectionSource>,
    after_sequence: Arc<AtomicU64>,
    page_size: u32,
}

impl MaintenanceAssetFactRuntime {
    /// Manual typed-ingest boundary, useful to composition that already owns a
    /// durable event consumer.
    #[allow(dead_code)] // Manual typed-ingest boundary retained for non-polling consumers.
    pub fn new(tenant_write_pool: PgPool) -> Self {
        Self {
            inbox: MaintenanceAssetFactInbox::new(tenant_write_pool.clone()),
            tenant_write_pool,
            source: None,
            after_sequence: Arc::new(AtomicU64::new(0)),
            page_size: 100,
        }
    }

    /// Polling worker path for the current modular monolith. Restarting from
    /// sequence zero is safe: the inbox verifies exact replays and rejects a
    /// different event/fact at the same Assets aggregate version.
    pub fn with_source(
        source_write: WritePool,
        tenant_write_pool: PgPool,
        page_size: u32,
    ) -> Result<Self, MaintenanceAssetFactRuntimeError> {
        if page_size == 0 || page_size > 1_000 {
            return Err(MaintenanceAssetFactRuntimeError::InvalidContract(
                "Assets fact page size is invalid",
            ));
        }
        Ok(Self {
            inbox: MaintenanceAssetFactInbox::new(tenant_write_pool.clone()),
            tenant_write_pool,
            source: Some(ProjectionSource::new(source_write)),
            after_sequence: Arc::new(AtomicU64::new(0)),
            page_size,
        })
    }

    pub const fn registration(&self) -> MaintenanceAssetFactRegistration {
        MaintenanceAssetFactRegistration {
            event_types: [ASSET_FACT_EVENT_TYPE, MAINTENANCE_REQUEST_EVENT_TYPE],
        }
    }

    #[allow(dead_code)] // Manual typed-ingest boundary retained for non-polling consumers.
    pub async fn ingest(
        &self,
        event: &IntegrationEventEnvelope<assets::Event>,
    ) -> Result<bool, PortError> {
        self.inbox.ingest(event).await
    }

    /// Reads one bounded page from the tenant outbox, decodes only the typed
    /// Assets condition contract, and writes the Maintenance-owned fact inbox.
    pub async fn run_once(
        &self,
    ) -> Result<MaintenanceAssetFactRun, MaintenanceAssetFactRuntimeError> {
        let source =
            self.source
                .as_ref()
                .ok_or(MaintenanceAssetFactRuntimeError::InvalidContract(
                    "Assets fact runtime has no outbox source",
                ))?;
        let after = self.after_sequence.load(Ordering::Acquire);
        let event_types = self.registration().event_types.map(str::to_owned);
        let page = source
            .page_after(after, &event_types, self.page_size)
            .await
            .map_err(MaintenanceAssetFactRuntimeError::Source)?;
        let fetched = u32::try_from(page.events.len()).map_err(|_| {
            MaintenanceAssetFactRuntimeError::InvalidContract("Assets fact page is too large")
        })?;
        let mut applied = 0_u32;
        let mut duplicates = 0_u32;
        let mut last = after;
        for event in &page.events {
            let envelope = decode_projection_event(event)?;
            match envelope.payload() {
                assets::Event::AssetConditionChanged(_) => {
                    if self
                        .inbox
                        .ingest(&envelope)
                        .await
                        .map_err(MaintenanceAssetFactRuntimeError::Inbox)?
                    {
                        applied = applied.saturating_add(1);
                    } else {
                        duplicates = duplicates.saturating_add(1);
                    }
                }
                assets::Event::MaintenanceRequested { .. } => {
                    if self.open_work_order(envelope).await? {
                        applied = applied.saturating_add(1);
                    } else {
                        duplicates = duplicates.saturating_add(1);
                    }
                }
            }
            last = event.global_sequence;
            self.after_sequence.store(last, Ordering::Release);
        }
        Ok(MaintenanceAssetFactRun {
            fetched,
            applied,
            duplicates,
            after_sequence: last,
        })
    }

    async fn open_work_order(
        &self,
        request: IntegrationEventEnvelope<assets::Event>,
    ) -> Result<bool, MaintenanceAssetFactRuntimeError> {
        let assets::Event::MaintenanceRequested {
            request_code,
            title,
            ..
        } = request.payload()
        else {
            return Err(MaintenanceAssetFactRuntimeError::InvalidContract(
                "Assets event is not a maintenance request",
            ));
        };
        let source_event_id = request.event_id();
        let work_order_id = work_order_id_for(source_event_id)?;
        let command_id = CommandId::try_from_uuid(Uuid::new_v5(
            &COMMAND_NAMESPACE,
            source_event_id.as_uuid().as_bytes(),
        ))
        .map_err(|_| {
            MaintenanceAssetFactRuntimeError::InvalidContract(
                "derived Maintenance command ID is invalid",
            )
        })?;
        let correlation_id = CorrelationId::try_from_uuid(request.correlation_id().as_uuid())
            .map_err(|_| {
                MaintenanceAssetFactRuntimeError::InvalidContract(
                    "Assets correlation ID is invalid",
                )
            })?;
        let scope = request.tenant_scope().clone();
        let display_title = maintenance_request_title(request_code, title.as_ref());
        let raw_pool = self.tenant_write_pool.clone();
        let pool = WritePool::from_pg_pool(raw_pool.clone());
        let access = TrustedMaintenanceRequestAccess;
        let receipt = erpx_maintenance::MaintenanceCommandHandler::new(
            MaintenanceWriteRepository::new(pool.clone()),
            MaintenanceCommandAdapter::new(pool),
            access,
            access,
            MaintenanceAssetFactRepository::new(raw_pool),
            SystemClock,
        )
        .handle(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            command_id,
            idempotency_key: IdempotencyKey::try_new(format!(
                "assets-maintenance-request:{}",
                source_event_id
            ))
            .map_err(|_| {
                MaintenanceAssetFactRuntimeError::InvalidContract(
                    "Maintenance idempotency key is invalid",
                )
            })?,
            correlation_id,
            causation_id: Some(source_event_id),
            actor: ActorAttribution {
                principal_ref: "maintenance-request-worker".into(),
                kind: ActorKind::SystemWorker,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            tenant_scope: scope,
            audit: AuditContext {
                purpose: "open work order from Assets maintenance request".into(),
                customer_visible: true,
                module: Some("maintenance".into()),
            },
            expected_aggregate_version: None,
            command: OpenWorkOrder {
                work_order_id,
                request,
                title: display_title,
            },
        })
        .await
        .map_err(MaintenanceAssetFactRuntimeError::Command)?;
        Ok(!receipt.replayed)
    }
}

fn work_order_id_for(
    source_event_id: EventId,
) -> Result<WorkOrderId, MaintenanceAssetFactRuntimeError> {
    WorkOrderId::try_from_uuid(Uuid::new_v5(
        &WORK_ORDER_NAMESPACE,
        source_event_id.as_uuid().as_bytes(),
    ))
    .map_err(|_| {
        MaintenanceAssetFactRuntimeError::InvalidContract(
            "derived Maintenance work-order ID is invalid",
        )
    })
}

fn maintenance_request_title(
    request_code: &erpx_integration_contracts::ReferenceCode,
    title: Option<&erpx_integration_contracts::DisplayName>,
) -> String {
    title
        .map_or_else(|| request_code.as_str(), |value| value.as_str())
        .to_owned()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MaintenanceAssetFactRun {
    pub fetched: u32,
    pub applied: u32,
    pub duplicates: u32,
    pub after_sequence: u64,
}

#[derive(Debug)]
pub enum MaintenanceAssetFactRuntimeError {
    Source(CqrsSqlxError),
    Inbox(PortError),
    Command(CqrsError),
    InvalidContract(&'static str),
}

impl std::fmt::Display for MaintenanceAssetFactRuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source(_) => formatter.write_str("Assets fact source is unavailable"),
            Self::Inbox(_) => formatter.write_str("Maintenance Assets fact inbox failed"),
            Self::Command(_) => formatter.write_str("Maintenance work-order opening failed"),
            Self::InvalidContract(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for MaintenanceAssetFactRuntimeError {}

fn decode_projection_event(
    event: &ProjectionEvent,
) -> Result<IntegrationEventEnvelope<assets::Event>, MaintenanceAssetFactRuntimeError> {
    if event.context != "assets"
        || event.aggregate_type != "asset"
        || !matches!(
            event.event_type.as_str(),
            ASSET_FACT_EVENT_TYPE | MAINTENANCE_REQUEST_EVENT_TYPE
        )
        || event.event_version != 1
    {
        return Err(MaintenanceAssetFactRuntimeError::InvalidContract(
            "outbox event is not a supported Assets maintenance event",
        ));
    }
    let payload: assets::Event = serde_json::from_value(event.payload.clone()).map_err(|_| {
        MaintenanceAssetFactRuntimeError::InvalidContract("Assets fact payload is invalid")
    })?;
    if payload.event_type() != event.event_type {
        return Err(MaintenanceAssetFactRuntimeError::InvalidContract(
            "Assets maintenance payload type does not match its envelope",
        ));
    }
    IntegrationEventEnvelope::try_new(
        event.event_id,
        event.tenant_scope.clone(),
        AggregateKind::try_new(event.aggregate_type.clone()).map_err(|_| {
            MaintenanceAssetFactRuntimeError::InvalidContract("Assets aggregate kind is invalid")
        })?,
        event.aggregate_id,
        event.aggregate_version,
        event.correlation_id,
        event.causation_id,
        event.occurred_at,
        payload,
    )
    .map_err(|_| {
        MaintenanceAssetFactRuntimeError::InvalidContract("Assets integration envelope is invalid")
    })
}

#[cfg(test)]
mod tests {
    use erpx_integration_contracts::{DisplayName, ReferenceCode};
    use erpx_kernel::EventId;
    use erpx_maintenance_sqlx::{PROJECTION_NAME, SNAPSHOT_EVENT_TYPE};

    use super::*;

    #[tokio::test]
    async fn registrations_keep_snapshot_and_cross_context_fact_channels_separate() {
        assert_eq!(
            MaintenanceProjectionFactory.registration(),
            MaintenanceProjectionRegistration {
                name: PROJECTION_NAME.into(),
                event_types: vec![SNAPSHOT_EVENT_TYPE.into()],
            }
        );
        let facts = MaintenanceAssetFactRuntime::new(
            sqlx::PgPool::connect_lazy("postgres://localhost/maintenance_write").unwrap(),
        );
        assert_eq!(
            facts.registration().event_types,
            ["assets.condition_changed", "assets.maintenance_requested"]
        );
    }

    #[tokio::test]
    async fn projection_factory_builds_without_connecting() {
        let write = WritePool::from_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/maintenance_write").unwrap(),
        );
        let projection = ProjectionPool::from_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/maintenance_read").unwrap(),
        );
        let runtime = MaintenanceProjectionFactory
            .build(write, projection, ProjectionRunnerConfig::default())
            .unwrap();
        assert_eq!(
            runtime.registration(),
            MaintenanceProjectionFactory.registration()
        );
    }

    #[test]
    fn source_never_imports_assets_core() {
        let forbidden_core_import = concat!("erpx_", "assets", "::");
        assert!(!include_str!("maintenance.rs").contains(forbidden_core_import));
    }

    #[tokio::test]
    async fn invalid_fact_page_size_fails_without_database_access() {
        let source = WritePool::from_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/assets_write").unwrap(),
        );
        let target = sqlx::PgPool::connect_lazy("postgres://localhost/maintenance_write").unwrap();
        assert!(MaintenanceAssetFactRuntime::with_source(source, target, 0).is_err());
    }

    #[test]
    fn maintenance_request_derives_stable_work_order_and_supports_legacy_title_fallback() {
        let source = EventId::try_from_uuid(Uuid::from_u128(42)).unwrap();
        assert_eq!(
            work_order_id_for(source).unwrap(),
            work_order_id_for(source).unwrap()
        );
        let request_code = ReferenceCode::try_new("MR-42").unwrap();
        assert_eq!(maintenance_request_title(&request_code, None), "MR-42");
        let title = DisplayName::try_new("Replace drive belt").unwrap();
        assert_eq!(
            maintenance_request_title(&request_code, Some(&title)),
            "Replace drive belt"
        );
    }
}
