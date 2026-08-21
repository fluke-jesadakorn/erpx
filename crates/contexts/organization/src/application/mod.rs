use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryMetadata, QueryResult,
};
use erpx_kernel::{
    ActorAttribution, AuditContext, AuthorityScope, Clock, CorrelationId, ErrorCategory,
    ErrorContext, EventId, HumanDecisionProof, KernelError,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod sharing;
pub use sharing::{OrganizationSharingCommandHandler, SharingApplicationError};

use crate::{
    domain::{
        OrganizationDomainError, OrganizationNode, OrganizationNodeId, OrganizationPlacement,
    },
    integration,
    ports::{
        AuthorizationPort, AuthorizationRequest, CommandFingerprint, CommandIdentity,
        CommandUnitOfWork, EntitlementPort, HierarchyGuard, HierarchyPort, OrganizationAction,
        OrganizationCommit, PlacementScopeContainment, PortError, ReadRepository,
        ScopeContainmentPort, StoredCommit, WriteRepository,
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
    #[error("organization activation requires an authenticated human decision")]
    HumanDecisionRequired,
    #[error(transparent)]
    Port(#[from] PortError),
}

pub struct OrganizationCommandHandler<W, U, A, E, H, C, S = PlacementScopeContainment> {
    write_repository: W,
    unit_of_work: U,
    authorization: A,
    entitlement: E,
    hierarchy: H,
    clock: C,
    scope_containment: S,
}

impl<W, U, A, E, H, C> OrganizationCommandHandler<W, U, A, E, H, C, PlacementScopeContainment> {
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
            scope_containment: PlacementScopeContainment,
        }
    }
}

impl<W, U, A, E, H, C, S> OrganizationCommandHandler<W, U, A, E, H, C, S> {
    pub const fn new_with_scope_containment(
        write_repository: W,
        unit_of_work: U,
        authorization: A,
        entitlement: E,
        hierarchy: H,
        clock: C,
        scope_containment: S,
    ) -> Self {
        Self {
            write_repository,
            unit_of_work,
            authorization,
            entitlement,
            hierarchy,
            clock,
            scope_containment,
        }
    }
}

enum HierarchyCheck {
    None,
    Activate,
    ReparentPlacement(OrganizationPlacement),
    Archive,
}

struct ExistingCommand {
    correlation_id: CorrelationId,
    causation_id: Option<EventId>,
    actor: ActorAttribution,
    authority_scope: AuthorityScope,
    audit: AuditContext,
    expected_version: Option<u64>,
    identity: CommandIdentity,
    node_id: OrganizationNodeId,
    action: OrganizationAction,
    hierarchy_check: HierarchyCheck,
    human_decision: Option<HumanDecisionProof>,
}

impl<W, U, A, E, H, C, S> OrganizationCommandHandler<W, U, A, E, H, C, S>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    H: HierarchyPort,
    C: Clock,
    S: ScopeContainmentPort,
{
    async fn preflight(
        &self,
        actor: &ActorAttribution,
        authority_scope: &AuthorityScope,
        node_id: OrganizationNodeId,
        placement: &OrganizationPlacement,
        action: OrganizationAction,
    ) -> Result<(), OrganizationApplicationError> {
        let target_scope = placement.active_scope(node_id)?;
        if !self
            .scope_containment
            .contains_node(authority_scope, node_id, placement)
            .await?
        {
            return Err(OrganizationApplicationError::ScopeOutsideAuthority);
        }
        self.authorize_target(actor, authority_scope, &target_scope, action)
            .await
    }

    async fn preflight_parent(
        &self,
        actor: &ActorAttribution,
        authority_scope: &AuthorityScope,
        node_id: OrganizationNodeId,
        placement: &OrganizationPlacement,
        action: OrganizationAction,
    ) -> Result<(), OrganizationApplicationError> {
        let target_scope = placement.active_scope(node_id)?;
        if !self
            .scope_containment
            .contains_parent(authority_scope, node_id, placement)
            .await?
        {
            return Err(OrganizationApplicationError::ScopeOutsideAuthority);
        }
        self.authorize_target(actor, authority_scope, &target_scope, action)
            .await
    }

    async fn authorize_target(
        &self,
        actor: &ActorAttribution,
        authority_scope: &AuthorityScope,
        target_scope: &AuthorityScope,
        action: OrganizationAction,
    ) -> Result<(), OrganizationApplicationError> {
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
                Option<&HumanDecisionProof>,
                erpx_kernel::CommandId,
                u64,
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
                receipt.aggregate.id(),
                receipt.aggregate.placement(),
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
            aggregate.id(),
            aggregate.placement(),
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
            HierarchyCheck::Activate => {
                self.hierarchy
                    .validate_placement(
                        &command.authority_scope,
                        command.node_id,
                        aggregate.placement(),
                    )
                    .await
                    .map_err(|error| to_cqrs(error.into(), command.correlation_id))?;
                Some(HierarchyGuard::Placement(aggregate.placement().clone()))
            }
            HierarchyCheck::ReparentPlacement(placement) => {
                self.preflight_parent(
                    &command.actor,
                    &command.authority_scope,
                    command.node_id,
                    placement,
                    command.action,
                )
                .await
                .map_err(|error| to_cqrs(error, command.correlation_id))?;
                self.hierarchy
                    .validate_placement(&command.authority_scope, command.node_id, placement)
                    .await
                    .map_err(|error| to_cqrs(error.into(), command.correlation_id))?;
                Some(HierarchyGuard::ReparentPlacement(placement.clone()))
            }
            HierarchyCheck::Archive => {
                self.hierarchy
                    .ensure_can_archive(&command.authority_scope, command.node_id)
                    .await
                    .map_err(|error| to_cqrs(error.into(), command.correlation_id))?;
                Some(HierarchyGuard::NoActiveChildren)
            }
        };

        let at = command
            .human_decision
            .as_ref()
            .map_or_else(|| self.clock.now(), HumanDecisionProof::decided_at);
        mutate(
            &mut aggregate,
            at,
            command.human_decision.as_ref(),
            command.identity.command_id,
            expected,
        )
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
        impl<W, U, A, E, H, C, S> CommandHandler<$command>
            for OrganizationCommandHandler<W, U, A, E, H, C, S>
        where
            W: WriteRepository,
            U: CommandUnitOfWork,
            A: AuthorizationPort,
            E: EntitlementPort,
            H: HierarchyPort,
            C: Clock,
            S: ScopeContainmentPort,
        {
            type Response = OrganizationMutationResult;

            async fn handle(
                &self,
                envelope: CommandEnvelope<$command>,
            ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
                envelope.validate()?;
                let command = envelope.command.clone();
                let human_decision = if $action == OrganizationAction::Activate {
                    let at = self.clock.now();
                    Some(
                        HumanDecisionProof::try_new(
                            envelope.actor.clone(),
                            envelope.active_scope.clone(),
                            crate::domain::organization_node_activation_decision_ref(
                                envelope.command_id,
                                command.node_id,
                                envelope.expected_aggregate_version.unwrap_or(0),
                            ),
                            at,
                        )
                        .map_err(|_| {
                            to_cqrs(
                                OrganizationApplicationError::HumanDecisionRequired,
                                envelope.correlation_id,
                            )
                        })?,
                    )
                } else {
                    None
                };
                let existing = ExistingCommand {
                    correlation_id: envelope.correlation_id,
                    causation_id: envelope.causation_id,
                    actor: envelope.actor,
                    authority_scope: envelope.active_scope,
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
                    human_decision,
                };
                let stored = self
                    .execute_existing(existing, |aggregate, at, decision, command_id, expected| {
                        ($mutation)(aggregate, command, at, decision, command_id, expected)
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
impl<W, U, A, E, H, C, S> CommandHandler<CreateOrganizationNode>
    for OrganizationCommandHandler<W, U, A, E, H, C, S>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    H: HierarchyPort,
    C: Clock,
    S: ScopeContainmentPort,
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
                &envelope.active_scope,
                stored.aggregate.id(),
                stored.aggregate.placement(),
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
        // A fresh node has no authoritative target row yet. Authorize its
        // current proposed parent; idempotent replay above remains bound to
        // the already-committed target lineage.
        self.preflight_parent(
            &envelope.actor,
            &envelope.active_scope,
            command.node_id,
            &command.placement,
            OrganizationAction::Create,
        )
        .await
        .map_err(|error| to_cqrs(error, correlation_id))?;
        self.hierarchy
            .validate_placement(&envelope.active_scope, command.node_id, &command.placement)
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
                authority_scope: envelope.active_scope,
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
    |aggregate: &mut OrganizationNode, command: RenameOrganizationNode, at, _, _, _| aggregate
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
    |aggregate: &mut OrganizationNode, command: ChangeOrganizationCode, at, _, _, _| aggregate
        .change_code(command.code, at)
);
existing_handler!(
    ReparentOrganizationNode,
    OrganizationAction::Reparent,
    |command: &ReparentOrganizationNode| CommandFingerprint::Reparent {
        node_id: command.node_id,
        placement: command.placement.clone(),
    },
    |command: &ReparentOrganizationNode| HierarchyCheck::ReparentPlacement(
        command.placement.clone()
    ),
    |aggregate: &mut OrganizationNode, command: ReparentOrganizationNode, at, _, _, _| aggregate
        .reparent(command.placement, at)
);
existing_handler!(
    SuspendOrganizationNode,
    OrganizationAction::Suspend,
    |command: &SuspendOrganizationNode| CommandFingerprint::Suspend {
        node_id: command.node_id,
    },
    |_command: &SuspendOrganizationNode| HierarchyCheck::None,
    |aggregate: &mut OrganizationNode, _command: SuspendOrganizationNode, at, _, _, _| aggregate
        .suspend(at)
);
existing_handler!(
    ActivateOrganizationNode,
    OrganizationAction::Activate,
    |command: &ActivateOrganizationNode| CommandFingerprint::Activate {
        node_id: command.node_id,
    },
    |_command: &ActivateOrganizationNode| HierarchyCheck::Activate,
    |aggregate: &mut OrganizationNode,
     _command: ActivateOrganizationNode,
     at,
     decision: Option<&HumanDecisionProof>,
     command_id,
     expected| {
        let from = aggregate.status();
        let decision = decision.ok_or(OrganizationDomainError::InvalidTransition {
            from,
            to: crate::domain::OrganizationNodeStatus::Active,
        })?;
        aggregate.activate(decision, command_id, expected, at)
    }
);
existing_handler!(
    ArchiveOrganizationNode,
    OrganizationAction::Archive,
    |command: &ArchiveOrganizationNode| CommandFingerprint::Archive {
        node_id: command.node_id,
    },
    |_command: &ArchiveOrganizationNode| HierarchyCheck::Archive,
    |aggregate: &mut OrganizationNode, _command: ArchiveOrganizationNode, at, _, _, _| aggregate
        .archive(at)
);

pub struct OrganizationQueryHandler<R, A, E, S = PlacementScopeContainment> {
    read_repository: R,
    authorization: A,
    entitlement: E,
    scope_containment: S,
}

impl<R, A, E> OrganizationQueryHandler<R, A, E, PlacementScopeContainment> {
    pub const fn new(read_repository: R, authorization: A, entitlement: E) -> Self {
        Self {
            read_repository,
            authorization,
            entitlement,
            scope_containment: PlacementScopeContainment,
        }
    }
}

impl<R, A, E, S> OrganizationQueryHandler<R, A, E, S> {
    pub const fn new_with_scope_containment(
        read_repository: R,
        authorization: A,
        entitlement: E,
        scope_containment: S,
    ) -> Self {
        Self {
            read_repository,
            authorization,
            entitlement,
            scope_containment,
        }
    }
}

impl<R, A, E, S> OrganizationQueryHandler<R, A, E, S>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
    S: ScopeContainmentPort,
{
    async fn authorize_query(
        &self,
        actor: &ActorAttribution,
        scope: &AuthorityScope,
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
impl<R, A, E, S> QueryHandler<GetOrganizationNode> for OrganizationQueryHandler<R, A, E, S>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
    S: ScopeContainmentPort,
{
    type Response = GetOrganizationNodeResult;

    async fn handle(
        &self,
        envelope: QueryEnvelope<GetOrganizationNode>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        self.authorize_query(
            &envelope.actor,
            &envelope.active_scope,
            OrganizationAction::View,
        )
        .await
        .map_err(|error| to_cqrs(error.into(), envelope.correlation_id))?;
        let projection = self
            .read_repository
            .get(
                &envelope.active_scope,
                envelope.query.node_id,
                &envelope.consistency,
            )
            .await
            .map_err(|error| port_to_cqrs(error, envelope.correlation_id))?;
        if let Some(view) = projection.value.as_ref() {
            let contained = self
                .scope_containment
                .contains_node(&envelope.active_scope, view.node_id, &view.placement)
                .await
                .map_err(|error| to_cqrs(error.into(), envelope.correlation_id))?;
            if !contained {
                return Err(to_cqrs(
                    OrganizationApplicationError::ScopeOutsideAuthority,
                    envelope.correlation_id,
                ));
            }
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
impl<R, A, E, S> QueryHandler<ListOrganizationNodes> for OrganizationQueryHandler<R, A, E, S>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
    S: ScopeContainmentPort,
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
            &envelope.active_scope,
            OrganizationAction::List,
        )
        .await
        .map_err(|error| to_cqrs(error.into(), envelope.correlation_id))?;
        let projection = self
            .read_repository
            .list(
                &envelope.active_scope,
                &envelope.query.filter,
                &envelope.consistency,
            )
            .await
            .map_err(|error| port_to_cqrs(error, envelope.correlation_id))?;
        let targets = projection
            .value
            .nodes
            .iter()
            .map(|view| (view.node_id, view.placement.clone()))
            .collect::<Vec<_>>();
        let contained = self
            .scope_containment
            .contains_nodes(&envelope.active_scope, &targets)
            .await
            .map_err(|error| to_cqrs(error.into(), envelope.correlation_id))?;
        if contained.len() != targets.len() || contained.iter().any(|decision| !decision) {
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
        | OrganizationApplicationError::HumanDecisionRequired
        | OrganizationApplicationError::Port(PortError::Denied)
        | OrganizationApplicationError::Port(PortError::EntitlementDenied) => (
            "organization.denied",
            ErrorCategory::Denied,
            "The Organization operation is not permitted.",
            false,
        ),
        OrganizationApplicationError::Port(PortError::ReparentBlockedByChildren) => (
            "organization.conflict",
            ErrorCategory::Conflict,
            "The Organization node cannot be reparented while it has children.",
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
