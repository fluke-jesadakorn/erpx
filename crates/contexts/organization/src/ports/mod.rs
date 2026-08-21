use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandReceipt, EventPosition, IdempotencyKey, ProjectionMetadata, ProjectionNotReady,
    QueryConsistency,
};
use erpx_integration_contracts::organization;
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, AuthorityScope, CommandId, CorrelationId, EventId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod sharing;
pub use sharing::{
    SharingAction, SharingAgreementList, SharingAuthorizationPort, SharingAuthorizationRequest,
    SharingCommandIdentity, SharingCommandPayload, SharingCommandRequest, SharingCommandUnitOfWork,
    SharingCommit, SharingListFilter, SharingReadRepository, SharingWriteRepository,
    StoredSharingCommit,
};

use crate::domain::{
    OrganizationNode, OrganizationNodeId, OrganizationNodeKind, OrganizationNodeStatus,
    OrganizationPlacement,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationAction {
    Create,
    Rename,
    ChangeCode,
    Reparent,
    Suspend,
    Activate,
    Archive,
    View,
    List,
}

impl OrganizationAction {
    pub const fn requires_write_entitlement(self) -> bool {
        !matches!(self, Self::View | Self::List)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CommandFingerprint {
    Create {
        node_id: OrganizationNodeId,
        placement: OrganizationPlacement,
        code: String,
        name: String,
    },
    Rename {
        node_id: OrganizationNodeId,
        name: String,
    },
    ChangeCode {
        node_id: OrganizationNodeId,
        code: String,
    },
    Reparent {
        node_id: OrganizationNodeId,
        placement: OrganizationPlacement,
    },
    Suspend {
        node_id: OrganizationNodeId,
    },
    Activate {
        node_id: OrganizationNodeId,
    },
    Archive {
        node_id: OrganizationNodeId,
    },
}

/// Stable identity checked by the receipt store. Retry timestamps and
/// correlation IDs are intentionally excluded from the fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub fingerprint: CommandFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a AuthorityScope,
    pub target_scope: &'a AuthorityScope,
    pub action: OrganizationAction,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError>;
}

/// Resolves organization-aware scope containment. The target placement is
/// supplied explicitly because an exact `Subtree` scope intentionally carries
/// only its node identity, not mutable hierarchy metadata.
#[async_trait]
pub trait ScopeContainmentPort: Send + Sync {
    async fn contains_node(
        &self,
        authority_scope: &AuthorityScope,
        node_id: OrganizationNodeId,
        placement: &OrganizationPlacement,
    ) -> Result<bool, PortError>;

    async fn contains_parent(
        &self,
        authority_scope: &AuthorityScope,
        node_id: OrganizationNodeId,
        placement: &OrganizationPlacement,
    ) -> Result<bool, PortError>;

    /// Bounded list authorization. SQL-backed implementations override this
    /// to resolve the page in one request-local batch; the default preserves
    /// the scalar contract for pure adapters and tests.
    async fn contains_nodes(
        &self,
        authority_scope: &AuthorityScope,
        nodes: &[(OrganizationNodeId, OrganizationPlacement)],
    ) -> Result<Vec<bool>, PortError> {
        let mut decisions = Vec::with_capacity(nodes.len());
        for (node_id, placement) in nodes {
            decisions.push(
                self.contains_node(authority_scope, *node_id, placement)
                    .await?,
            );
        }
        Ok(decisions)
    }
}

/// Pure containment for callers that already hold a trusted typed placement.
/// SQL adapters may replace this with an implementation bound to the same
/// authoritative hierarchy snapshot used by their read or write operation.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlacementScopeContainment;

#[async_trait]
impl ScopeContainmentPort for PlacementScopeContainment {
    async fn contains_node(
        &self,
        authority_scope: &AuthorityScope,
        node_id: OrganizationNodeId,
        placement: &OrganizationPlacement,
    ) -> Result<bool, PortError> {
        placement
            .is_within_scope(node_id, authority_scope)
            .map_err(|_| PortError::Denied)
    }

    async fn contains_parent(
        &self,
        authority_scope: &AuthorityScope,
        node_id: OrganizationNodeId,
        placement: &OrganizationPlacement,
    ) -> Result<bool, PortError> {
        if matches!(placement, OrganizationPlacement::Holding) {
            placement
                .validate_for(node_id)
                .map_err(|_| PortError::Denied)?;
            if authority_scope.validate().is_err() {
                return Ok(false);
            }
            return Ok(match authority_scope {
                AuthorityScope::Platform => true,
                AuthorityScope::Holding { holding_id } => *holding_id == node_id.as_uuid(),
                AuthorityScope::Company { .. }
                | AuthorityScope::Branch { .. }
                | AuthorityScope::Subtree { .. } => false,
            });
        }
        placement
            .is_parent_within_scope(node_id, authority_scope)
            .map_err(|_| PortError::Denied)
    }
}

#[async_trait]
pub trait EntitlementPort: Send + Sync {
    async fn ensure_organization_enabled(
        &self,
        scope: &AuthorityScope,
        write: bool,
    ) -> Result<(), PortError>;
}

/// Authoritative hierarchy validation for commands. Implementations check the
/// prospective parent on the write side, including parent kind, active state,
/// and ancestry-cycle constraints.
#[async_trait]
pub trait HierarchyPort: Send + Sync {
    async fn validate_placement(
        &self,
        authority_scope: &AuthorityScope,
        node_id: OrganizationNodeId,
        placement: &OrganizationPlacement,
    ) -> Result<(), PortError>;

    async fn ensure_can_archive(
        &self,
        authority_scope: &AuthorityScope,
        node_id: OrganizationNodeId,
    ) -> Result<(), PortError>;
}

/// Authoritative aggregate reads used by command handlers only.
#[async_trait]
pub trait WriteRepository: Send + Sync {
    async fn load(
        &self,
        authority_scope: &AuthorityScope,
        node_id: OrganizationNodeId,
    ) -> Result<Option<OrganizationNode>, PortError>;
}

/// Hierarchy constraint that the unit of work must revalidate while holding
/// the same transaction/locks used to persist the aggregate and outbox. The
/// separate `HierarchyPort` check gives an early failure but is not the atomic
/// authority because a parent or child can change concurrently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HierarchyGuard {
    Placement(OrganizationPlacement),
    ReparentPlacement(OrganizationPlacement),
    NoActiveChildren,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrganizationCommit {
    pub identity: CommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub hierarchy_guard: Option<HierarchyGuard>,
    pub aggregate: OrganizationNode,
    pub integration_events: Vec<organization::Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: OrganizationNode,
    pub committed_events: Vec<EventPosition>,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

impl StoredCommit {
    pub fn into_receipt<R>(self, response: R) -> CommandReceipt<R> {
        CommandReceipt {
            command_id: self.command_id,
            correlation_id: self.correlation_id,
            aggregate: erpx_cqrs::AggregateVersion {
                context: "organization".into(),
                aggregate_type: "organization_node".into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id().as_uuid())
                    .expect("an Organization aggregate always has a non-nil identifier"),
                version: self.aggregate.version(),
            },
            committed_events: self.committed_events,
            committed_at: self.committed_at,
            replayed: self.replayed,
            response,
        }
    }
}

/// Atomic boundary for snapshot, idempotency receipt, audit row, and outbox.
/// Implementations also resolve concurrent expected-version conflicts and
/// enforce `hierarchy_guard` in the same transaction.
#[async_trait]
pub trait CommandUnitOfWork: Send + Sync {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError>;

    async fn commit(&self, request: OrganizationCommit) -> Result<StoredCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeView {
    pub node_id: OrganizationNodeId,
    pub version: u64,
    pub code: String,
    pub name: String,
    pub kind: OrganizationNodeKind,
    pub placement: OrganizationPlacement,
    pub parent_id: Option<OrganizationNodeId>,
    pub scope: AuthorityScope,
    pub status: OrganizationNodeStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&OrganizationNode> for NodeView {
    fn from(value: &OrganizationNode) -> Self {
        Self {
            node_id: value.id(),
            version: value.version(),
            code: value.code().into(),
            name: value.name().into(),
            kind: value.kind(),
            placement: value.placement().clone(),
            parent_id: value.placement().parent_id(value.id()),
            scope: value.scope().clone(),
            status: value.status(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct NodeListFilter {
    pub kind: Option<OrganizationNodeKind>,
    pub status: Option<OrganizationNodeStatus>,
    pub parent_id: Option<OrganizationNodeId>,
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeList {
    pub nodes: Vec<NodeView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadProjection<T> {
    pub value: T,
    pub metadata: ProjectionMetadata,
}

/// Disposable read-side access only. There is deliberately no write-store
/// escape hatch, so ReadYourWrites cannot fall back to aggregate snapshots.
#[async_trait]
pub trait ReadRepository: Send + Sync {
    async fn get(
        &self,
        authority_scope: &AuthorityScope,
        node_id: OrganizationNodeId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<NodeView>>, PortError>;

    async fn list(
        &self,
        authority_scope: &AuthorityScope,
        filter: &NodeListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<NodeList>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("organization operation is denied")]
    Denied,
    #[error("organization entitlement is not enabled")]
    EntitlementDenied,
    #[error("organization node was not found")]
    NotFound,
    #[error("organization parent was not found or has the wrong kind")]
    InvalidParent,
    #[error("organization placement would create an ancestry cycle")]
    HierarchyCycle,
    #[error("organization node has active children and cannot be archived")]
    ActiveChildren,
    #[error("organization node has children and cannot be reparented")]
    ReparentBlockedByChildren,
    #[error("organization aggregate version conflict (expected {expected}, actual {actual})")]
    Conflict { expected: u64, actual: u64 },
    #[error("idempotency identity was reused for a different command")]
    IdempotencyConflict,
    #[error("organization storage is temporarily unavailable")]
    Unavailable,
    #[error("organization projection has not reached the requested command")]
    NotReady(ProjectionNotReady),
    #[error("organization adapter failed")]
    Internal,
}
