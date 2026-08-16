mod records;

pub use records::{
    MaintenanceLifecycleRecordApplicationError, MaintenanceLifecycleRecordCommandHandler,
};

use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryMetadata, QueryResult,
};
use erpx_integration_contracts::{IntegrationEventEnvelope, assets};
use erpx_kernel::{
    ActorAttribution, AuditContext, Clock, CorrelationId, ErrorCategory, ErrorContext, EventId,
    KernelError, TenantScope,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    domain::{Downtime, MaintenanceDomainError, MaintenanceRequest, WorkOrder, WorkOrderId},
    integration,
    ports::{
        AssetFactPort, AuthorizationPort, AuthorizationRequest, CommandFingerprint,
        CommandIdentity, CommandUnitOfWork, EntitlementPort, MaintenanceAction, MaintenanceCommit,
        MaintenanceStoredCommit, PortError, ReadRepository, WorkOrderList, WorkOrderListFilter,
        WorkOrderView, WriteRepository,
    },
};

pub const MAX_MAINTENANCE_LIST_PAGE_SIZE: u16 = 100;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenWorkOrder {
    pub work_order_id: WorkOrderId,
    pub request: IntegrationEventEnvelope<assets::Event>,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleWorkOrder {
    pub work_order_id: WorkOrderId,
    pub scheduled_for: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartWorkOrder {
    pub work_order_id: WorkOrderId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockWorkOrder {
    pub work_order_id: WorkOrderId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeWorkOrder {
    pub work_order_id: WorkOrderId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompleteWorkOrder {
    pub work_order_id: WorkOrderId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelWorkOrder {
    pub work_order_id: WorkOrderId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordDowntime {
    pub work_order_id: WorkOrderId,
    pub downtime: Downtime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetWorkOrder {
    pub work_order_id: WorkOrderId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListWorkOrders {
    pub filter: WorkOrderListFilter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenWorkOrderResult {
    pub work_order: WorkOrderView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaintenanceMutationResult {
    pub work_order: WorkOrderView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetWorkOrderResult {
    pub work_order: Option<WorkOrderView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MaintenanceApplicationError {
    #[error(transparent)]
    Domain(#[from] MaintenanceDomainError),
    #[error("command scope does not contain the maintenance target")]
    ScopeOutsideAuthority,
    #[error("open commands cannot declare an aggregate version")]
    UnexpectedCreateVersion,
    #[error("mutating maintenance commands require an expected aggregate version")]
    MissingExpectedVersion,
    #[error("maintenance aggregate version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("maintenance list page size must be between 1 and 100")]
    InvalidPageSize,
    #[error("work order opening requires an Assets maintenance-request event")]
    InvalidSourceEvent,
    #[error("maintenance integration event mapping failed")]
    IntegrationMapping,
    #[error(transparent)]
    Port(#[from] PortError),
}

pub struct MaintenanceCommandHandler<W, U, A, E, F, C> {
    write_repository: W,
    unit_of_work: U,
    authorization: A,
    entitlement: E,
    asset_facts: F,
    clock: C,
}

impl<W, U, A, E, F, C> MaintenanceCommandHandler<W, U, A, E, F, C> {
    pub const fn new(
        write_repository: W,
        unit_of_work: U,
        authorization: A,
        entitlement: E,
        asset_facts: F,
        clock: C,
    ) -> Self {
        Self {
            write_repository,
            unit_of_work,
            authorization,
            entitlement,
            asset_facts,
            clock,
        }
    }
}

enum ExistingMutation {
    Schedule {
        scheduled_for: chrono::DateTime<chrono::Utc>,
    },
    Start,
    Block,
    Resume,
    Complete,
    Cancel,
    RecordDowntime {
        downtime: Downtime,
    },
}

struct ExistingCommand {
    correlation_id: CorrelationId,
    causation_id: Option<EventId>,
    actor: ActorAttribution,
    authority_scope: TenantScope,
    audit: AuditContext,
    expected_version: Option<u64>,
    identity: CommandIdentity,
    work_order_id: WorkOrderId,
    action: MaintenanceAction,
    mutation: ExistingMutation,
}

impl<W, U, A, E, F, C> MaintenanceCommandHandler<W, U, A, E, F, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    F: AssetFactPort,
    C: Clock,
{
    async fn preflight(
        &self,
        actor: &ActorAttribution,
        authority_scope: &TenantScope,
        target_scope: &TenantScope,
        action: MaintenanceAction,
    ) -> Result<(), MaintenanceApplicationError> {
        if !authority_scope.contains(target_scope) {
            return Err(MaintenanceApplicationError::ScopeOutsideAuthority);
        }
        self.entitlement
            .ensure_maintenance_enabled(target_scope, action.requires_write_entitlement())
            .await?;
        self.authorization
            .authorize(AuthorizationRequest {
                actor,
                authority_scope,
                target_scope,
                action,
            })
            .await?;
        Ok(())
    }

    async fn execute_existing(
        &self,
        command: ExistingCommand,
    ) -> Result<MaintenanceStoredCommit, CqrsError> {
        if let Some(mut stored) = self
            .unit_of_work
            .find_receipt(&command.identity)
            .await
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))?
        {
            self.preflight(
                &command.actor,
                &command.authority_scope,
                stored.aggregate.scope(),
                command.action,
            )
            .await
            .map_err(|error| to_cqrs(error, command.correlation_id))?;
            stored.replayed = true;
            return Ok(stored);
        }
        let mut aggregate = self
            .write_repository
            .load(&command.authority_scope, command.work_order_id)
            .await
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))?
            .ok_or_else(|| to_cqrs(PortError::NotFound.into(), command.correlation_id))?;
        self.preflight(
            &command.actor,
            &command.authority_scope,
            aggregate.scope(),
            command.action,
        )
        .await
        .map_err(|error| to_cqrs(error, command.correlation_id))?;
        let expected = command.expected_version.ok_or_else(|| {
            to_cqrs(
                MaintenanceApplicationError::MissingExpectedVersion,
                command.correlation_id,
            )
        })?;
        if expected != aggregate.version() {
            return Err(to_cqrs(
                MaintenanceApplicationError::VersionConflict {
                    expected,
                    actual: aggregate.version(),
                },
                command.correlation_id,
            ));
        }
        let at = self.clock.now();
        match command.mutation {
            ExistingMutation::Schedule { scheduled_for } => aggregate.schedule(scheduled_for, at),
            ExistingMutation::Start => aggregate.start(at),
            ExistingMutation::Block => aggregate.block(at),
            ExistingMutation::Resume => aggregate.resume(at),
            ExistingMutation::Complete => aggregate.complete(at),
            ExistingMutation::Cancel => aggregate.cancel(at),
            ExistingMutation::RecordDowntime { downtime } => {
                aggregate.record_downtime(downtime, at)
            }
        }
        .map_err(|error| to_cqrs(error.into(), command.correlation_id))?;
        let work_order_code = aggregate.code().to_owned();
        let integration_events =
            integration::published_events(&aggregate.take_uncommitted_events(), &work_order_code)
                .map_err(|_| {
                to_cqrs(
                    MaintenanceApplicationError::IntegrationMapping,
                    command.correlation_id,
                )
            })?;
        self.unit_of_work
            .commit(MaintenanceCommit {
                identity: command.identity,
                correlation_id: command.correlation_id,
                causation_id: command.causation_id,
                actor: command.actor,
                authority_scope: command.authority_scope,
                audit: command.audit,
                expected_version: Some(expected),
                consumed_source_event: None,
                aggregate,
                integration_events,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))
    }
}

#[async_trait]
impl<W, U, A, E, F, C> CommandHandler<OpenWorkOrder> for MaintenanceCommandHandler<W, U, A, E, F, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    F: AssetFactPort,
    C: Clock,
{
    type Response = OpenWorkOrderResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<OpenWorkOrder>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        let command = envelope.command;
        command.request.validate().map_err(|_| {
            to_cqrs(
                MaintenanceApplicationError::InvalidSourceEvent,
                correlation_id,
            )
        })?;
        let source_event_id = command.request.event_id();
        if envelope.causation_id != Some(source_event_id) {
            return Err(to_cqrs(
                MaintenanceApplicationError::InvalidSourceEvent,
                correlation_id,
            ));
        }
        let (asset, request_code) = match command.request.payload() {
            assets::Event::MaintenanceRequested {
                asset,
                request_code,
                ..
            } => (*asset, request_code.as_str().to_owned()),
            assets::Event::AssetConditionChanged(_) => {
                return Err(to_cqrs(
                    MaintenanceApplicationError::InvalidSourceEvent,
                    correlation_id,
                ));
            }
        };
        let source_version = command.request.aggregate_version();
        let target_scope = command.request.tenant_scope().clone();
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key,
            fingerprint: CommandFingerprint::Open {
                work_order_id: command.work_order_id,
                source_event_id,
                source_aggregate_version: source_version,
                asset,
                request_code: request_code.clone(),
                scope: target_scope.clone(),
                title: command.title.clone(),
            },
        };
        if let Some(mut stored) = self
            .unit_of_work
            .find_receipt(&identity)
            .await
            .map_err(|error| to_cqrs(error.into(), correlation_id))?
        {
            self.preflight(
                &envelope.actor,
                &envelope.tenant_scope,
                stored.aggregate.scope(),
                MaintenanceAction::Open,
            )
            .await
            .map_err(|error| to_cqrs(error, correlation_id))?;
            stored.replayed = true;
            let response = OpenWorkOrderResult {
                work_order: WorkOrderView::from(&stored.aggregate),
            };
            return Ok(stored.into_receipt(response));
        }
        self.preflight(
            &envelope.actor,
            &envelope.tenant_scope,
            &target_scope,
            MaintenanceAction::Open,
        )
        .await
        .map_err(|error| to_cqrs(error, correlation_id))?;
        if envelope.expected_aggregate_version.is_some() {
            return Err(to_cqrs(
                MaintenanceApplicationError::UnexpectedCreateVersion,
                correlation_id,
            ));
        }
        let asset_fact = self
            .asset_facts
            .get_at_least(asset, source_version)
            .await
            .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        let request =
            MaintenanceRequest::try_new(source_event_id, source_version, asset, request_code)
                .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        let mut aggregate = WorkOrder::open(
            command.work_order_id,
            target_scope,
            request,
            asset_fact,
            command.title,
            self.clock.now(),
        )
        .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        let work_order_code = aggregate.code().to_owned();
        let integration_events =
            integration::published_events(&aggregate.take_uncommitted_events(), &work_order_code)
                .map_err(|_| {
                to_cqrs(
                    MaintenanceApplicationError::IntegrationMapping,
                    correlation_id,
                )
            })?;
        let stored = self
            .unit_of_work
            .commit(MaintenanceCommit {
                identity,
                correlation_id,
                causation_id: Some(source_event_id),
                actor: envelope.actor,
                authority_scope: envelope.tenant_scope,
                audit: envelope.audit,
                expected_version: None,
                consumed_source_event: Some(source_event_id),
                aggregate,
                integration_events,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        let response = OpenWorkOrderResult {
            work_order: WorkOrderView::from(&stored.aggregate),
        };
        Ok(stored.into_receipt(response))
    }
}

macro_rules! existing_handler {
    ($command:ty, $action:expr, $fingerprint:expr, $work_order_id:expr, $mutation:expr) => {
        #[async_trait]
        impl<W, U, A, E, F, C> CommandHandler<$command>
            for MaintenanceCommandHandler<W, U, A, E, F, C>
        where
            W: WriteRepository,
            U: CommandUnitOfWork,
            A: AuthorizationPort,
            E: EntitlementPort,
            F: AssetFactPort,
            C: Clock,
        {
            type Response = MaintenanceMutationResult;

            async fn handle(
                &self,
                envelope: CommandEnvelope<$command>,
            ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
                envelope.validate()?;
                let command = envelope.command;
                let stored = self
                    .execute_existing(ExistingCommand {
                        correlation_id: envelope.correlation_id,
                        causation_id: envelope.causation_id,
                        actor: envelope.actor,
                        authority_scope: envelope.tenant_scope,
                        audit: envelope.audit,
                        expected_version: envelope.expected_aggregate_version,
                        identity: CommandIdentity {
                            command_id: envelope.command_id,
                            idempotency_key: envelope.idempotency_key,
                            fingerprint: ($fingerprint)(&command),
                        },
                        work_order_id: ($work_order_id)(&command),
                        action: $action,
                        mutation: ($mutation)(command),
                    })
                    .await?;
                let response = MaintenanceMutationResult {
                    work_order: WorkOrderView::from(&stored.aggregate),
                };
                Ok(stored.into_receipt(response))
            }
        }
    };
}

existing_handler!(
    ScheduleWorkOrder,
    MaintenanceAction::Schedule,
    |command: &ScheduleWorkOrder| CommandFingerprint::Schedule {
        work_order_id: command.work_order_id,
        scheduled_for: command.scheduled_for,
    },
    |command: &ScheduleWorkOrder| command.work_order_id,
    |command: ScheduleWorkOrder| ExistingMutation::Schedule {
        scheduled_for: command.scheduled_for,
    }
);
existing_handler!(
    StartWorkOrder,
    MaintenanceAction::Start,
    |command: &StartWorkOrder| CommandFingerprint::Start {
        work_order_id: command.work_order_id,
    },
    |command: &StartWorkOrder| command.work_order_id,
    |_command: StartWorkOrder| ExistingMutation::Start
);
existing_handler!(
    BlockWorkOrder,
    MaintenanceAction::Block,
    |command: &BlockWorkOrder| CommandFingerprint::Block {
        work_order_id: command.work_order_id,
    },
    |command: &BlockWorkOrder| command.work_order_id,
    |_command: BlockWorkOrder| ExistingMutation::Block
);
existing_handler!(
    ResumeWorkOrder,
    MaintenanceAction::Resume,
    |command: &ResumeWorkOrder| CommandFingerprint::Resume {
        work_order_id: command.work_order_id,
    },
    |command: &ResumeWorkOrder| command.work_order_id,
    |_command: ResumeWorkOrder| ExistingMutation::Resume
);
existing_handler!(
    CompleteWorkOrder,
    MaintenanceAction::Complete,
    |command: &CompleteWorkOrder| CommandFingerprint::Complete {
        work_order_id: command.work_order_id,
    },
    |command: &CompleteWorkOrder| command.work_order_id,
    |_command: CompleteWorkOrder| ExistingMutation::Complete
);
existing_handler!(
    CancelWorkOrder,
    MaintenanceAction::Cancel,
    |command: &CancelWorkOrder| CommandFingerprint::Cancel {
        work_order_id: command.work_order_id,
    },
    |command: &CancelWorkOrder| command.work_order_id,
    |_command: CancelWorkOrder| ExistingMutation::Cancel
);
existing_handler!(
    RecordDowntime,
    MaintenanceAction::RecordDowntime,
    |command: &RecordDowntime| CommandFingerprint::RecordDowntime {
        work_order_id: command.work_order_id,
        downtime: command.downtime.clone(),
    },
    |command: &RecordDowntime| command.work_order_id,
    |command: RecordDowntime| ExistingMutation::RecordDowntime {
        downtime: command.downtime,
    }
);

pub struct MaintenanceQueryHandler<R, A, E> {
    read_repository: R,
    authorization: A,
    entitlement: E,
}

impl<R, A, E> MaintenanceQueryHandler<R, A, E> {
    pub const fn new(read_repository: R, authorization: A, entitlement: E) -> Self {
        Self {
            read_repository,
            authorization,
            entitlement,
        }
    }
}

impl<R, A, E> MaintenanceQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    async fn authorize_query(
        &self,
        actor: &ActorAttribution,
        scope: &TenantScope,
        action: MaintenanceAction,
    ) -> Result<(), PortError> {
        self.entitlement
            .ensure_maintenance_enabled(scope, false)
            .await?;
        self.authorization
            .authorize(AuthorizationRequest {
                actor,
                authority_scope: scope,
                target_scope: scope,
                action,
            })
            .await
    }
}

#[async_trait]
impl<R, A, E> QueryHandler<GetWorkOrder> for MaintenanceQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = GetWorkOrderResult;

    async fn handle(
        &self,
        envelope: QueryEnvelope<GetWorkOrder>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        self.authorize_query(
            &envelope.actor,
            &envelope.tenant_scope,
            MaintenanceAction::View,
        )
        .await
        .map_err(|error| to_cqrs(error.into(), envelope.correlation_id))?;
        let projection = self
            .read_repository
            .get(
                &envelope.tenant_scope,
                envelope.query.work_order_id,
                &envelope.consistency,
            )
            .await
            .map_err(|error| port_to_cqrs(error, envelope.correlation_id))?;
        if projection
            .value
            .as_ref()
            .is_some_and(|view| !envelope.tenant_scope.contains(&view.scope))
        {
            return Err(to_cqrs(
                MaintenanceApplicationError::ScopeOutsideAuthority,
                envelope.correlation_id,
            ));
        }
        Ok(QueryResult {
            value: GetWorkOrderResult {
                work_order: projection.value,
            },
            metadata: QueryMetadata {
                correlation_id: envelope.correlation_id,
                projection: projection.metadata,
                consistency: envelope.consistency,
            },
        })
    }
}

#[async_trait]
impl<R, A, E> QueryHandler<ListWorkOrders> for MaintenanceQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = WorkOrderList;

    async fn handle(
        &self,
        envelope: QueryEnvelope<ListWorkOrders>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        if !(1..=MAX_MAINTENANCE_LIST_PAGE_SIZE).contains(&envelope.query.filter.page_size) {
            return Err(to_cqrs(
                MaintenanceApplicationError::InvalidPageSize,
                envelope.correlation_id,
            ));
        }
        self.authorize_query(
            &envelope.actor,
            &envelope.tenant_scope,
            MaintenanceAction::List,
        )
        .await
        .map_err(|error| to_cqrs(error.into(), envelope.correlation_id))?;
        let projection = self
            .read_repository
            .list(
                &envelope.tenant_scope,
                &envelope.query.filter,
                &envelope.consistency,
            )
            .await
            .map_err(|error| port_to_cqrs(error, envelope.correlation_id))?;
        if projection
            .value
            .work_orders
            .iter()
            .any(|view| !envelope.tenant_scope.contains(&view.scope))
        {
            return Err(to_cqrs(
                MaintenanceApplicationError::ScopeOutsideAuthority,
                envelope.correlation_id,
            ));
        }
        Ok(QueryResult {
            value: projection.value,
            metadata: QueryMetadata {
                correlation_id: envelope.correlation_id,
                projection: projection.metadata,
                consistency: envelope.consistency,
            },
        })
    }
}

fn port_to_cqrs(error: PortError, correlation_id: CorrelationId) -> CqrsError {
    match error {
        PortError::NotReady(not_ready) => CqrsError::NotReady(not_ready),
        other => to_cqrs(other.into(), correlation_id),
    }
}

fn to_cqrs(error: MaintenanceApplicationError, correlation_id: CorrelationId) -> CqrsError {
    let (code, category, safe_message, retryable) = match &error {
        MaintenanceApplicationError::Domain(_)
        | MaintenanceApplicationError::UnexpectedCreateVersion
        | MaintenanceApplicationError::MissingExpectedVersion
        | MaintenanceApplicationError::InvalidPageSize
        | MaintenanceApplicationError::InvalidSourceEvent
        | MaintenanceApplicationError::IntegrationMapping => (
            "maintenance.invalid",
            ErrorCategory::Invalid,
            "The Maintenance request is invalid.",
            false,
        ),
        MaintenanceApplicationError::ScopeOutsideAuthority
        | MaintenanceApplicationError::Port(PortError::Denied)
        | MaintenanceApplicationError::Port(PortError::EntitlementDenied) => (
            "maintenance.denied",
            ErrorCategory::Denied,
            "The Maintenance operation is not permitted.",
            false,
        ),
        MaintenanceApplicationError::VersionConflict { .. }
        | MaintenanceApplicationError::Port(PortError::Conflict { .. })
        | MaintenanceApplicationError::Port(PortError::IdempotencyConflict)
        | MaintenanceApplicationError::Port(PortError::SourceEventAlreadyConsumed) => (
            "maintenance.conflict",
            ErrorCategory::Conflict,
            "The work order changed or its command/source identity conflicts.",
            false,
        ),
        MaintenanceApplicationError::Port(PortError::NotFound) => (
            "maintenance.not_found",
            ErrorCategory::NotFound,
            "The work order or Assets fact was not found.",
            false,
        ),
        MaintenanceApplicationError::Port(PortError::AssetFactNotReady) => (
            "maintenance.asset_fact_not_ready",
            ErrorCategory::NotReady,
            "The Assets fact is not ready for Maintenance.",
            true,
        ),
        MaintenanceApplicationError::Port(PortError::Unavailable) => (
            "maintenance.unavailable",
            ErrorCategory::Unavailable,
            "Maintenance is temporarily unavailable.",
            true,
        ),
        MaintenanceApplicationError::Port(PortError::Internal)
        | MaintenanceApplicationError::Port(PortError::NotReady(_)) => (
            "maintenance.internal",
            ErrorCategory::Internal,
            "Maintenance could not complete the operation.",
            false,
        ),
    };
    CqrsError::Operation(KernelError {
        code: code.into(),
        category,
        safe_message: safe_message.into(),
        retryable,
        correlation_id,
        context: ErrorContext::empty(),
    })
}

#[cfg(test)]
mod tests;
