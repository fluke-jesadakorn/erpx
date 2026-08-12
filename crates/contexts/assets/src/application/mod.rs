use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryMetadata, QueryResult,
};
use erpx_kernel::{
    ActorAttribution, AuditContext, Clock, CorrelationId, ErrorCategory, ErrorContext, EventId,
    KernelError, TenantScope,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    domain::{Asset, AssetDomainError, AssetId, AssetScope},
    integration,
    ports::{
        AssetAction, AssetCommit, AssetList, AssetListFilter, AssetStoredCommit, AssetView,
        AuthorizationPort, AuthorizationRequest, CommandFingerprint, CommandIdentity,
        CommandUnitOfWork, EntitlementPort, PortError, ReadRepository, WriteRepository,
    },
};

pub const MAX_ASSET_LIST_PAGE_SIZE: u16 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateAsset {
    pub asset_id: AssetId,
    pub code: String,
    pub name: String,
    pub scope: AssetScope,
    pub location_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceAssetInService {
    pub asset_id: AssetId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkAssetDegraded {
    pub asset_id: AssetId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BeginMaintenance {
    pub asset_id: AssetId,
    pub request_code: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReturnAssetToService {
    pub asset_id: AssetId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetireAsset {
    pub asset_id: AssetId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetAsset {
    pub asset_id: AssetId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListAssets {
    pub filter: AssetListFilter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateAssetResult {
    pub asset: AssetView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetMutationResult {
    pub asset: AssetView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetAssetResult {
    pub asset: Option<AssetView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AssetApplicationError {
    #[error(transparent)]
    Domain(#[from] AssetDomainError),
    #[error("command scope does not contain the asset target")]
    ScopeOutsideAuthority,
    #[error("create commands cannot declare an aggregate version")]
    UnexpectedCreateVersion,
    #[error("mutating asset commands require an expected aggregate version")]
    MissingExpectedVersion,
    #[error("asset aggregate version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("asset list page size must be between 1 and 100")]
    InvalidPageSize,
    #[error("asset integration event mapping failed")]
    IntegrationMapping,
    #[error(transparent)]
    Port(#[from] PortError),
}

pub struct AssetCommandHandler<W, U, A, E, C> {
    write_repository: W,
    unit_of_work: U,
    authorization: A,
    entitlement: E,
    clock: C,
}

impl<W, U, A, E, C> AssetCommandHandler<W, U, A, E, C> {
    pub const fn new(
        write_repository: W,
        unit_of_work: U,
        authorization: A,
        entitlement: E,
        clock: C,
    ) -> Self {
        Self {
            write_repository,
            unit_of_work,
            authorization,
            entitlement,
            clock,
        }
    }
}

enum ExistingMutation {
    PlaceInService,
    MarkDegraded,
    BeginMaintenance { request_code: String, title: String },
    ReturnToService,
    Retire,
}

struct ExistingCommand {
    correlation_id: CorrelationId,
    causation_id: Option<EventId>,
    actor: ActorAttribution,
    authority_scope: TenantScope,
    audit: AuditContext,
    expected_version: Option<u64>,
    identity: CommandIdentity,
    asset_id: AssetId,
    action: AssetAction,
    mutation: ExistingMutation,
}

impl<W, U, A, E, C> AssetCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    async fn preflight(
        &self,
        actor: &ActorAttribution,
        authority_scope: &TenantScope,
        target_scope: &TenantScope,
        action: AssetAction,
    ) -> Result<(), AssetApplicationError> {
        if !authority_scope.contains(target_scope) {
            return Err(AssetApplicationError::ScopeOutsideAuthority);
        }
        self.entitlement
            .ensure_assets_enabled(target_scope, action.requires_write_entitlement())
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
    ) -> Result<AssetStoredCommit, CqrsError> {
        if let Some(mut stored) = self
            .unit_of_work
            .find_receipt(&command.identity)
            .await
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))?
        {
            self.preflight(
                &command.actor,
                &command.authority_scope,
                stored.aggregate.scope().tenant_scope(),
                command.action,
            )
            .await
            .map_err(|error| to_cqrs(error, command.correlation_id))?;
            stored.replayed = true;
            return Ok(stored);
        }

        let mut aggregate = self
            .write_repository
            .load(&command.authority_scope, command.asset_id)
            .await
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))?
            .ok_or_else(|| to_cqrs(PortError::NotFound.into(), command.correlation_id))?;
        self.preflight(
            &command.actor,
            &command.authority_scope,
            aggregate.scope().tenant_scope(),
            command.action,
        )
        .await
        .map_err(|error| to_cqrs(error, command.correlation_id))?;
        let expected = command.expected_version.ok_or_else(|| {
            to_cqrs(
                AssetApplicationError::MissingExpectedVersion,
                command.correlation_id,
            )
        })?;
        if expected != aggregate.version() {
            return Err(to_cqrs(
                AssetApplicationError::VersionConflict {
                    expected,
                    actual: aggregate.version(),
                },
                command.correlation_id,
            ));
        }
        let at = self.clock.now();
        match command.mutation {
            ExistingMutation::PlaceInService => aggregate.place_in_service(at),
            ExistingMutation::MarkDegraded => aggregate.mark_degraded(at),
            ExistingMutation::BeginMaintenance {
                request_code,
                title,
            } => aggregate.begin_maintenance(request_code, title, at),
            ExistingMutation::ReturnToService => aggregate.return_to_service(at),
            ExistingMutation::Retire => aggregate.retire(at),
        }
        .map_err(|error| to_cqrs(error.into(), command.correlation_id))?;
        let domain_events = aggregate.take_uncommitted_events();
        let integration_events = integration::published_events(&domain_events).map_err(|_| {
            to_cqrs(
                AssetApplicationError::IntegrationMapping,
                command.correlation_id,
            )
        })?;
        self.unit_of_work
            .commit(AssetCommit {
                identity: command.identity,
                correlation_id: command.correlation_id,
                causation_id: command.causation_id,
                actor: command.actor,
                authority_scope: command.authority_scope,
                audit: command.audit,
                expected_version: Some(expected),
                aggregate,
                integration_events,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))
    }
}

#[async_trait]
impl<W, U, A, E, C> CommandHandler<CreateAsset> for AssetCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = CreateAssetResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<CreateAsset>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        let command = envelope.command;
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key,
            fingerprint: CommandFingerprint::Create {
                asset_id: command.asset_id,
                code: command.code.clone(),
                name: command.name.clone(),
                scope: command.scope.clone(),
                location_id: command.location_id,
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
                stored.aggregate.scope().tenant_scope(),
                AssetAction::Create,
            )
            .await
            .map_err(|error| to_cqrs(error, correlation_id))?;
            stored.replayed = true;
            let response = CreateAssetResult {
                asset: AssetView::from(&stored.aggregate),
            };
            return Ok(stored.into_receipt(response));
        }
        self.preflight(
            &envelope.actor,
            &envelope.tenant_scope,
            command.scope.tenant_scope(),
            AssetAction::Create,
        )
        .await
        .map_err(|error| to_cqrs(error, correlation_id))?;
        if envelope.expected_aggregate_version.is_some() {
            return Err(to_cqrs(
                AssetApplicationError::UnexpectedCreateVersion,
                correlation_id,
            ));
        }
        let mut aggregate = Asset::create(
            command.asset_id,
            command.code,
            command.name,
            command.scope,
            command.location_id,
            self.clock.now(),
        )
        .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        let integration_events =
            integration::published_events(&aggregate.take_uncommitted_events())
                .map_err(|_| to_cqrs(AssetApplicationError::IntegrationMapping, correlation_id))?;
        let stored = self
            .unit_of_work
            .commit(AssetCommit {
                identity,
                correlation_id,
                causation_id: envelope.causation_id,
                actor: envelope.actor,
                authority_scope: envelope.tenant_scope,
                audit: envelope.audit,
                expected_version: None,
                aggregate,
                integration_events,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        let response = CreateAssetResult {
            asset: AssetView::from(&stored.aggregate),
        };
        Ok(stored.into_receipt(response))
    }
}

macro_rules! existing_handler {
    ($command:ty, $action:expr, $fingerprint:expr, $asset_id:expr, $mutation:expr) => {
        #[async_trait]
        impl<W, U, A, E, C> CommandHandler<$command> for AssetCommandHandler<W, U, A, E, C>
        where
            W: WriteRepository,
            U: CommandUnitOfWork,
            A: AuthorizationPort,
            E: EntitlementPort,
            C: Clock,
        {
            type Response = AssetMutationResult;

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
                        asset_id: ($asset_id)(&command),
                        action: $action,
                        mutation: ($mutation)(command),
                    })
                    .await?;
                let response = AssetMutationResult {
                    asset: AssetView::from(&stored.aggregate),
                };
                Ok(stored.into_receipt(response))
            }
        }
    };
}

existing_handler!(
    PlaceAssetInService,
    AssetAction::PlaceInService,
    |command: &PlaceAssetInService| CommandFingerprint::PlaceInService {
        asset_id: command.asset_id,
    },
    |command: &PlaceAssetInService| command.asset_id,
    |_command: PlaceAssetInService| ExistingMutation::PlaceInService
);
existing_handler!(
    MarkAssetDegraded,
    AssetAction::MarkDegraded,
    |command: &MarkAssetDegraded| CommandFingerprint::MarkDegraded {
        asset_id: command.asset_id,
    },
    |command: &MarkAssetDegraded| command.asset_id,
    |_command: MarkAssetDegraded| ExistingMutation::MarkDegraded
);
existing_handler!(
    BeginMaintenance,
    AssetAction::BeginMaintenance,
    |command: &BeginMaintenance| CommandFingerprint::BeginMaintenance {
        asset_id: command.asset_id,
        request_code: command.request_code.clone(),
        title: command.title.clone(),
    },
    |command: &BeginMaintenance| command.asset_id,
    |command: BeginMaintenance| ExistingMutation::BeginMaintenance {
        request_code: command.request_code,
        title: command.title,
    }
);
existing_handler!(
    ReturnAssetToService,
    AssetAction::ReturnToService,
    |command: &ReturnAssetToService| CommandFingerprint::ReturnToService {
        asset_id: command.asset_id,
    },
    |command: &ReturnAssetToService| command.asset_id,
    |_command: ReturnAssetToService| ExistingMutation::ReturnToService
);
existing_handler!(
    RetireAsset,
    AssetAction::Retire,
    |command: &RetireAsset| CommandFingerprint::Retire {
        asset_id: command.asset_id,
    },
    |command: &RetireAsset| command.asset_id,
    |_command: RetireAsset| ExistingMutation::Retire
);

pub struct AssetQueryHandler<R, A, E> {
    read_repository: R,
    authorization: A,
    entitlement: E,
}

impl<R, A, E> AssetQueryHandler<R, A, E> {
    pub const fn new(read_repository: R, authorization: A, entitlement: E) -> Self {
        Self {
            read_repository,
            authorization,
            entitlement,
        }
    }
}

impl<R, A, E> AssetQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    async fn authorize_query(
        &self,
        actor: &ActorAttribution,
        scope: &TenantScope,
        action: AssetAction,
    ) -> Result<(), PortError> {
        self.entitlement.ensure_assets_enabled(scope, false).await?;
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
impl<R, A, E> QueryHandler<GetAsset> for AssetQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = GetAssetResult;

    async fn handle(
        &self,
        envelope: QueryEnvelope<GetAsset>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        self.authorize_query(&envelope.actor, &envelope.tenant_scope, AssetAction::View)
            .await
            .map_err(|error| to_cqrs(error.into(), envelope.correlation_id))?;
        let projection = self
            .read_repository
            .get(
                &envelope.tenant_scope,
                envelope.query.asset_id,
                &envelope.consistency,
            )
            .await
            .map_err(|error| port_to_cqrs(error, envelope.correlation_id))?;
        if projection
            .value
            .as_ref()
            .is_some_and(|view| !envelope.tenant_scope.contains(view.scope.tenant_scope()))
        {
            return Err(to_cqrs(
                AssetApplicationError::ScopeOutsideAuthority,
                envelope.correlation_id,
            ));
        }
        Ok(QueryResult {
            value: GetAssetResult {
                asset: projection.value,
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
impl<R, A, E> QueryHandler<ListAssets> for AssetQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = AssetList;

    async fn handle(
        &self,
        envelope: QueryEnvelope<ListAssets>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        if !(1..=MAX_ASSET_LIST_PAGE_SIZE).contains(&envelope.query.filter.page_size) {
            return Err(to_cqrs(
                AssetApplicationError::InvalidPageSize,
                envelope.correlation_id,
            ));
        }
        self.authorize_query(&envelope.actor, &envelope.tenant_scope, AssetAction::List)
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
            .assets
            .iter()
            .any(|view| !envelope.tenant_scope.contains(view.scope.tenant_scope()))
        {
            return Err(to_cqrs(
                AssetApplicationError::ScopeOutsideAuthority,
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

fn to_cqrs(error: AssetApplicationError, correlation_id: CorrelationId) -> CqrsError {
    let (code, category, safe_message, retryable) = match &error {
        AssetApplicationError::Domain(_)
        | AssetApplicationError::UnexpectedCreateVersion
        | AssetApplicationError::MissingExpectedVersion
        | AssetApplicationError::InvalidPageSize
        | AssetApplicationError::IntegrationMapping => (
            "assets.invalid",
            ErrorCategory::Invalid,
            "The Assets request is invalid.",
            false,
        ),
        AssetApplicationError::ScopeOutsideAuthority
        | AssetApplicationError::Port(PortError::Denied)
        | AssetApplicationError::Port(PortError::EntitlementDenied) => (
            "assets.denied",
            ErrorCategory::Denied,
            "The Assets operation is not permitted.",
            false,
        ),
        AssetApplicationError::VersionConflict { .. }
        | AssetApplicationError::Port(PortError::Conflict { .. })
        | AssetApplicationError::Port(PortError::IdempotencyConflict) => (
            "assets.conflict",
            ErrorCategory::Conflict,
            "The asset changed or the command identity conflicts.",
            false,
        ),
        AssetApplicationError::Port(PortError::NotFound) => (
            "assets.not_found",
            ErrorCategory::NotFound,
            "The asset was not found.",
            false,
        ),
        AssetApplicationError::Port(PortError::Unavailable) => (
            "assets.unavailable",
            ErrorCategory::Unavailable,
            "Assets is temporarily unavailable.",
            true,
        ),
        AssetApplicationError::Port(PortError::Internal)
        | AssetApplicationError::Port(PortError::NotReady(_)) => (
            "assets.internal",
            ErrorCategory::Internal,
            "Assets could not complete the operation.",
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
