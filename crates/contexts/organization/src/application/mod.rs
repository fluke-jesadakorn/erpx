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

use crate::{
    domain::{
        OrganizationDomainError, OrganizationNode, OrganizationNodeId, OrganizationPlacement,
    },
    integration,
    ports::{
        AuthorizationPort, AuthorizationRequest, CommandFingerprint, CommandIdentity,
        CommandUnitOfWork, EntitlementPort, HierarchyGuard, HierarchyPort, OrganizationAction,
        OrganizationCommit, PortError, ReadRepository, StoredCommit, WriteRepository,
    },
};

pub use crate::ports::{NodeList, NodeListFilter, NodeView};

pub const MAX_ORGANIZATION_LIST_PAGE_SIZE: u16 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateOrganizationNode {
    pub node_id: OrganizationNodeId,
    pub placement: OrganizationPlacement,
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameOrganizationNode {
    pub node_id: OrganizationNodeId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeOrganizationCode {
    pub node_id: OrganizationNodeId,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReparentOrganizationNode {
    pub node_id: OrganizationNodeId,
    pub placement: OrganizationPlacement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuspendOrganizationNode {
    pub node_id: OrganizationNodeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivateOrganizationNode {
    pub node_id: OrganizationNodeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveOrganizationNode {
    pub node_id: OrganizationNodeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetOrganizationNode {
    pub node_id: OrganizationNodeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListOrganizationNodes {
    pub filter: NodeListFilter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateOrganizationNodeResult {
    pub node: NodeView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationMutationResult {
    pub node: NodeView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetOrganizationNodeResult {
    pub node: Option<NodeView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OrganizationApplicationError {
    #[error(transparent)]
    Domain(#[from] OrganizationDomainError),
    #[error("command scope does not contain the Organization node scope")]
    ScopeOutsideAuthority,
    #[error("create commands cannot declare an existing aggregate version")]
    UnexpectedCreateVersion,
    #[error("mutating commands require an expected aggregate version")]
    MissingExpectedVersion,
    #[error("organization aggregate version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("organization list page size must be between 1 and 100")]
    InvalidPageSize,
    #[error("organization integration event mapping failed")]
    IntegrationMapping,
    #[error(transparent)]
    Port(#[from] PortError),
}

pub struct OrganizationCommandHandler<W, U, A, E, H, C> {
    write_repository: W,
    unit_of_work: U,
    authorization: A,
    entitlement: E,
    hierarchy: H,
    clock: C,
}

impl<W, U, A, E, H, C> OrganizationCommandHandler<W, U, A, E, H, C> {
    pub const fn new(
        write_repository: W,
        unit_of_work: U,
        authorization: A,
        entitlement: E,
        hierarchy: H,
        clock: C,
    ) -> Self {
        Self {
            write_repository,
            unit_of_work,
            authorization,
            entitlement,
            hierarchy,
            clock,
        }
    }
}

enum HierarchyCheck {
    None,
    Placement(OrganizationPlacement),
    Archive,
}

struct ExistingCommand {
    correlation_id: CorrelationId,
    causation_id: Option<EventId>,
    actor: ActorAttribution,
    authority_scope: TenantScope,
    audit: AuditContext,
    expected_version: Option<u64>,
    identity: CommandIdentity,
    node_id: OrganizationNodeId,
    action: OrganizationAction,
    hierarchy_check: HierarchyCheck,
}

impl<W, U, A, E, H, C> OrganizationCommandHandler<W, U, A, E, H, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    H: HierarchyPort,
    C: Clock,
{
    async fn preflight(
        &self,
        actor: &ActorAttribution,
        authority_scope: &TenantScope,
        target_scope: &TenantScope,
        action: OrganizationAction,
    ) -> Result<(), OrganizationApplicationError> {
        if !authority_scope.contains(target_scope) {
            return Err(OrganizationApplicationError::ScopeOutsideAuthority);
        }
        self.entitlement
            .ensure_organization_enabled(target_scope, action.requires_write_entitlement())
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

    async fn execute_existing<F>(
        &self,
        command: ExistingCommand,
        mutate: F,
    ) -> Result<StoredCommit, CqrsError>
    where
        F: FnOnce(
                &mut OrganizationNode,
                chrono::DateTime<chrono::Utc>,
            ) -> Result<(), OrganizationDomainError>
            + Send,
    {
        if let Some(mut receipt) = self
            .unit_of_work
            .find_receipt(&command.identity)
            .await
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))?
        {
            self.preflight(
                &command.actor,
                &command.authority_scope,
                receipt.aggregate.scope(),
                command.action,
            )
            .await
            .map_err(|error| to_cqrs(error, command.correlation_id))?;
            receipt.replayed = true;
            return Ok(receipt);
        }

        let mut aggregate = self
            .write_repository
            .load(&command.authority_scope, command.node_id)
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
                OrganizationApplicationError::MissingExpectedVersion,
                command.correlation_id,
            )
        })?;
        if expected != aggregate.version() {
            return Err(to_cqrs(
                OrganizationApplicationError::VersionConflict {
                    expected,
                    actual: aggregate.version(),
                },
                command.correlation_id,
            ));
        }

        let hierarchy_guard = match &command.hierarchy_check {
            HierarchyCheck::None => None,
            HierarchyCheck::Placement(placement) => {
                let target_scope = placement
                    .tenant_scope(command.node_id)
                    .map_err(|error| to_cqrs(error.into(), command.correlation_id))?;
                self.preflight(
                    &command.actor,
                    &command.authority_scope,
                    &target_scope,
                    command.action,
                )
                .await
                .map_err(|error| to_cqrs(error, command.correlation_id))?;
                self.hierarchy
                    .validate_placement(&command.authority_scope, command.node_id, placement)
                    .await
                    .map_err(|error| to_cqrs(error.into(), command.correlation_id))?;
                Some(HierarchyGuard::Placement(placement.clone()))
            }
            HierarchyCheck::Archive => {
                self.hierarchy
                    .ensure_can_archive(&command.authority_scope, command.node_id)
                    .await
                    .map_err(|error| to_cqrs(error.into(), command.correlation_id))?;
                Some(HierarchyGuard::NoActiveChildren)
            }
        };

        mutate(&mut aggregate, self.clock.now())
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))?;
        let domain_events = aggregate.take_uncommitted_events();
        let integration_events = integration::published_events(&domain_events).map_err(|_| {
            to_cqrs(
                OrganizationApplicationError::IntegrationMapping,
                command.correlation_id,
            )
        })?;
        self.unit_of_work
            .commit(OrganizationCommit {
                identity: command.identity,
                correlation_id: command.correlation_id,
                causation_id: command.causation_id,
                actor: command.actor,
                authority_scope: command.authority_scope,
                audit: command.audit,
                expected_version: Some(expected),
                hierarchy_guard,
                aggregate,
                integration_events,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))
    }
}

macro_rules! existing_handler {
    ($command:ty, $action:expr, $fingerprint:expr, $hierarchy:expr, $mutation:expr) => {
        #[async_trait]
        impl<W, U, A, E, H, C> CommandHandler<$command>
            for OrganizationCommandHandler<W, U, A, E, H, C>
        where
            W: WriteRepository,
            U: CommandUnitOfWork,
            A: AuthorizationPort,
            E: EntitlementPort,
            H: HierarchyPort,
            C: Clock,
        {
            type Response = OrganizationMutationResult;

            async fn handle(
                &self,
                envelope: CommandEnvelope<$command>,
            ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
                envelope.validate()?;
                let command = envelope.command.clone();
                let existing = ExistingCommand {
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
                    node_id: command.node_id,
                    action: $action,
                    hierarchy_check: ($hierarchy)(&command),
                };
                let stored = self
                    .execute_existing(existing, |aggregate, at| {
                        ($mutation)(aggregate, command, at)
                    })
                    .await?;
                let response = OrganizationMutationResult {
                    node: NodeView::from(&stored.aggregate),
                };
                Ok(stored.into_receipt(response))
            }
        }
    };
}

#[async_trait]
impl<W, U, A, E, H, C> CommandHandler<CreateOrganizationNode>
    for OrganizationCommandHandler<W, U, A, E, H, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    H: HierarchyPort,
    C: Clock,
{
    type Response = CreateOrganizationNodeResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<CreateOrganizationNode>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        let command = envelope.command;
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key,
            fingerprint: CommandFingerprint::Create {
                node_id: command.node_id,
                placement: command.placement.clone(),
                code: command.code.clone(),
                name: command.name.clone(),
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
                OrganizationAction::Create,
            )
            .await
            .map_err(|error| to_cqrs(error, correlation_id))?;
            stored.replayed = true;
            let response = CreateOrganizationNodeResult {
                node: NodeView::from(&stored.aggregate),
            };
            return Ok(stored.into_receipt(response));
        }
        if envelope.expected_aggregate_version.is_some() {
            return Err(to_cqrs(
                OrganizationApplicationError::UnexpectedCreateVersion,
                correlation_id,
            ));
        }
        let target_scope = command
            .placement
            .tenant_scope(command.node_id)
            .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        self.preflight(
            &envelope.actor,
            &envelope.tenant_scope,
            &target_scope,
            OrganizationAction::Create,
        )
        .await
        .map_err(|error| to_cqrs(error, correlation_id))?;
        self.hierarchy
            .validate_placement(&envelope.tenant_scope, command.node_id, &command.placement)
            .await
            .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        let hierarchy_guard = HierarchyGuard::Placement(command.placement.clone());
        let mut aggregate = OrganizationNode::create(
            command.node_id,
            command.placement,
            command.code,
            command.name,
            self.clock.now(),
        )
        .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        let integration_events =
            integration::published_events(&aggregate.take_uncommitted_events()).map_err(|_| {
                to_cqrs(
                    OrganizationApplicationError::IntegrationMapping,
                    correlation_id,
                )
            })?;
        let stored = self
            .unit_of_work
            .commit(OrganizationCommit {
                identity,
                correlation_id,
                causation_id: envelope.causation_id,
                actor: envelope.actor,
                authority_scope: envelope.tenant_scope,
                audit: envelope.audit,
                expected_version: None,
                hierarchy_guard: Some(hierarchy_guard),
                aggregate,
                integration_events,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        let response = CreateOrganizationNodeResult {
            node: NodeView::from(&stored.aggregate),
        };
        Ok(stored.into_receipt(response))
    }
}

existing_handler!(
    RenameOrganizationNode,
    OrganizationAction::Rename,
    |command: &RenameOrganizationNode| CommandFingerprint::Rename {
        node_id: command.node_id,
        name: command.name.clone(),
    },
    |_command: &RenameOrganizationNode| HierarchyCheck::None,
    |aggregate: &mut OrganizationNode, command: RenameOrganizationNode, at| aggregate
        .rename(command.name, at)
);
existing_handler!(
    ChangeOrganizationCode,
    OrganizationAction::ChangeCode,
    |command: &ChangeOrganizationCode| CommandFingerprint::ChangeCode {
        node_id: command.node_id,
        code: command.code.clone(),
    },
    |_command: &ChangeOrganizationCode| HierarchyCheck::None,
    |aggregate: &mut OrganizationNode, command: ChangeOrganizationCode, at| aggregate
        .change_code(command.code, at)
);
existing_handler!(
    ReparentOrganizationNode,
    OrganizationAction::Reparent,
    |command: &ReparentOrganizationNode| CommandFingerprint::Reparent {
        node_id: command.node_id,
        placement: command.placement.clone(),
    },
    |command: &ReparentOrganizationNode| HierarchyCheck::Placement(command.placement.clone()),
    |aggregate: &mut OrganizationNode, command: ReparentOrganizationNode, at| aggregate
        .reparent(command.placement, at)
);
existing_handler!(
    SuspendOrganizationNode,
    OrganizationAction::Suspend,
    |command: &SuspendOrganizationNode| CommandFingerprint::Suspend {
        node_id: command.node_id,
    },
    |_command: &SuspendOrganizationNode| HierarchyCheck::None,
    |aggregate: &mut OrganizationNode, _command: SuspendOrganizationNode, at| aggregate.suspend(at)
);
existing_handler!(
    ActivateOrganizationNode,
    OrganizationAction::Activate,
    |command: &ActivateOrganizationNode| CommandFingerprint::Activate {
        node_id: command.node_id,
    },
    |_command: &ActivateOrganizationNode| HierarchyCheck::None,
    |aggregate: &mut OrganizationNode, _command: ActivateOrganizationNode, at| aggregate
        .activate(at)
);
existing_handler!(
    ArchiveOrganizationNode,
    OrganizationAction::Archive,
    |command: &ArchiveOrganizationNode| CommandFingerprint::Archive {
        node_id: command.node_id,
    },
    |_command: &ArchiveOrganizationNode| HierarchyCheck::Archive,
    |aggregate: &mut OrganizationNode, _command: ArchiveOrganizationNode, at| aggregate.archive(at)
);

pub struct OrganizationQueryHandler<R, A, E> {
    read_repository: R,
    authorization: A,
    entitlement: E,
}

impl<R, A, E> OrganizationQueryHandler<R, A, E> {
    pub const fn new(read_repository: R, authorization: A, entitlement: E) -> Self {
        Self {
            read_repository,
            authorization,
            entitlement,
        }
    }
}

impl<R, A, E> OrganizationQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    async fn authorize_query(
        &self,
        actor: &ActorAttribution,
        scope: &TenantScope,
        action: OrganizationAction,
    ) -> Result<(), PortError> {
        self.entitlement
            .ensure_organization_enabled(scope, false)
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
impl<R, A, E> QueryHandler<GetOrganizationNode> for OrganizationQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = GetOrganizationNodeResult;

    async fn handle(
        &self,
        envelope: QueryEnvelope<GetOrganizationNode>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        self.authorize_query(
            &envelope.actor,
            &envelope.tenant_scope,
            OrganizationAction::View,
        )
        .await
        .map_err(|error| to_cqrs(error.into(), envelope.correlation_id))?;
        let projection = self
            .read_repository
            .get(
                &envelope.tenant_scope,
                envelope.query.node_id,
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
                OrganizationApplicationError::ScopeOutsideAuthority,
                envelope.correlation_id,
            ));
        }
        Ok(QueryResult {
            value: GetOrganizationNodeResult {
                node: projection.value,
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
impl<R, A, E> QueryHandler<ListOrganizationNodes> for OrganizationQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = NodeList;

    async fn handle(
        &self,
        envelope: QueryEnvelope<ListOrganizationNodes>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        if !(1..=MAX_ORGANIZATION_LIST_PAGE_SIZE).contains(&envelope.query.filter.page_size) {
            return Err(to_cqrs(
                OrganizationApplicationError::InvalidPageSize,
                envelope.correlation_id,
            ));
        }
        self.authorize_query(
            &envelope.actor,
            &envelope.tenant_scope,
            OrganizationAction::List,
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
            .nodes
            .iter()
            .any(|view| !envelope.tenant_scope.contains(&view.scope))
        {
            return Err(to_cqrs(
                OrganizationApplicationError::ScopeOutsideAuthority,
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

fn to_cqrs(error: OrganizationApplicationError, correlation_id: CorrelationId) -> CqrsError {
    let (code, category, safe_message, retryable) = match &error {
        OrganizationApplicationError::Domain(_)
        | OrganizationApplicationError::UnexpectedCreateVersion
        | OrganizationApplicationError::MissingExpectedVersion
        | OrganizationApplicationError::InvalidPageSize
        | OrganizationApplicationError::IntegrationMapping
        | OrganizationApplicationError::Port(PortError::InvalidParent)
        | OrganizationApplicationError::Port(PortError::HierarchyCycle)
        | OrganizationApplicationError::Port(PortError::ActiveChildren) => (
            "organization.invalid",
            ErrorCategory::Invalid,
            "The Organization request is invalid.",
            false,
        ),
        OrganizationApplicationError::ScopeOutsideAuthority
        | OrganizationApplicationError::Port(PortError::Denied)
        | OrganizationApplicationError::Port(PortError::EntitlementDenied) => (
            "organization.denied",
            ErrorCategory::Denied,
            "The Organization operation is not permitted.",
            false,
        ),
        OrganizationApplicationError::VersionConflict { .. }
        | OrganizationApplicationError::Port(PortError::Conflict { .. })
        | OrganizationApplicationError::Port(PortError::IdempotencyConflict) => (
            "organization.conflict",
            ErrorCategory::Conflict,
            "The Organization node changed or the command identity conflicts.",
            false,
        ),
        OrganizationApplicationError::Port(PortError::NotFound) => (
            "organization.not_found",
            ErrorCategory::NotFound,
            "The Organization node was not found.",
            false,
        ),
        OrganizationApplicationError::Port(PortError::Unavailable) => (
            "organization.unavailable",
            ErrorCategory::Unavailable,
            "Organization is temporarily unavailable.",
            true,
        ),
        OrganizationApplicationError::Port(PortError::Internal)
        | OrganizationApplicationError::Port(PortError::NotReady(_)) => (
            "organization.internal",
            ErrorCategory::Internal,
            "Organization could not complete the operation.",
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
