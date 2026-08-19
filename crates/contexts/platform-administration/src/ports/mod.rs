use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandReceipt, EventPosition, IdempotencyKey, ProjectionMetadata, ProjectionNotReady,
    QueryConsistency,
};
use erpx_integration_contracts::platform_administration;
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, AuthorityScope, CommandId, CorrelationId, EventId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

mod governance;
pub use governance::{
    GovernanceReadProjection, PlatformGovernanceRecordAction,
    PlatformGovernanceRecordAuthorizationPort, PlatformGovernanceRecordAuthorizationRequest,
    PlatformGovernanceRecordCommand, PlatformGovernanceRecordCommandIdentity,
    PlatformGovernanceRecordCommandRequest, PlatformGovernanceRecordCommit,
    PlatformGovernanceRecordQuery, PlatformGovernanceRecordReadRepository,
    PlatformGovernanceRecordUnitOfWork, PlatformGovernanceRecordWriteRepository,
    StoredPlatformGovernanceRecordCommit,
};

use crate::domain::{
    AdminContextId, AdminContextStatus, ConnectorBinding, ConnectorBindingId, ConnectorKind,
    ConnectorReadinessEvidence, ConnectorScope, ConnectorState, ConnectorTransition, EnvironmentId,
    HoldingAdminContext, HoldingLifecycleAction, HoldingLifecycleState, PlatformAggregate,
    PlatformHolding, PlatformHoldingId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformAdministrationAction {
    RegisterHolding,
    TransitionHolding,
    RegisterConnector,
    TransitionConnector,
    ObserveConnectorReadiness,
    IssueAdminContext,
    RevokeAdminContext,
    ViewHolding,
    ListHoldings,
    ViewConnector,
    ListConnectors,
    ViewAdminContext,
}

impl PlatformAdministrationAction {
    pub const fn high_risk(self) -> bool {
        matches!(
            self,
            Self::RegisterHolding
                | Self::TransitionHolding
                | Self::RegisterConnector
                | Self::TransitionConnector
                | Self::IssueAdminContext
                | Self::RevokeAdminContext
        )
    }

    pub const fn write(self) -> bool {
        !matches!(
            self,
            Self::ViewHolding
                | Self::ListHoldings
                | Self::ViewConnector
                | Self::ListConnectors
                | Self::ViewAdminContext
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a AuthorityScope,
    pub target_scope: &'a AuthorityScope,
    pub action: PlatformAdministrationAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationGrant {
    pub authorized_scope: AuthorityScope,
    pub mfa_verified: bool,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(
        &self,
        request: AuthorizationRequest<'_>,
    ) -> Result<AuthorizationGrant, PortError>;
}

#[async_trait]
pub trait EntitlementPort: Send + Sync {
    async fn ensure_platform_administration_enabled(&self, write: bool) -> Result<(), PortError>;
}

/// Resolves customer/root-holding ownership from authoritative control-plane
/// records. Command payloads are never accepted as ownership evidence.
#[async_trait]
pub trait HoldingAuthorityPort: Send + Sync {
    async fn ensure_binding_available(
        &self,
        customer_id: Uuid,
        canonical_holding_id: Uuid,
        data_profile_key: &str,
    ) -> Result<(), PortError>;

    async fn ensure_holding_scope(
        &self,
        holding_id: PlatformHoldingId,
        target_scope: &AuthorityScope,
    ) -> Result<(), PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CommandFingerprint {
    RegisterHolding {
        holding_id: PlatformHoldingId,
        code: String,
        name: String,
        environment_id: EnvironmentId,
        customer_id: Uuid,
        canonical_holding_id: Uuid,
        data_profile_key: String,
    },
    TransitionHolding {
        holding_id: PlatformHoldingId,
        action: HoldingLifecycleAction,
    },
    RegisterConnector {
        connector_id: ConnectorBindingId,
        scope: ConnectorScope,
        kind: ConnectorKind,
        key: String,
        configuration_digest: String,
        capabilities: std::collections::BTreeSet<String>,
    },
    TransitionConnector {
        connector_id: ConnectorBindingId,
        action: ConnectorTransition,
    },
    ObserveConnectorReadiness {
        connector_id: ConnectorBindingId,
        evidence: ConnectorReadinessEvidence,
    },
    IssueAdminContext {
        context_id: AdminContextId,
        holding_id: PlatformHoldingId,
        scope: AuthorityScope,
        device_id: Uuid,
        session_id: Uuid,
        reason: String,
        ttl_seconds: u32,
    },
    RevokeAdminContext {
        context_id: AdminContextId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub fingerprint: CommandFingerprint,
}

#[async_trait]
pub trait WriteRepository: Send + Sync {
    async fn load_holding(
        &self,
        authority_scope: &AuthorityScope,
        id: PlatformHoldingId,
    ) -> Result<Option<PlatformHolding>, PortError>;
    async fn load_connector(
        &self,
        authority_scope: &AuthorityScope,
        id: ConnectorBindingId,
    ) -> Result<Option<ConnectorBinding>, PortError>;
    async fn load_admin_context(
        &self,
        authority_scope: &AuthorityScope,
        id: AdminContextId,
    ) -> Result<Option<HoldingAdminContext>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformAdministrationCommit {
    pub identity: CommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub aggregate: PlatformAggregate,
    pub integration_events: Vec<platform_administration::Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: PlatformAggregate,
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
                context: "platform_administration".into(),
                aggregate_type: self.aggregate.aggregate_type().into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id_uuid())
                    .expect("Platform Administration aggregate identifiers are non-nil"),
                version: self.aggregate.version(),
            },
            committed_events: self.committed_events,
            committed_at: self.committed_at,
            replayed: self.replayed,
            response,
        }
    }
}

/// Atomic snapshot + stable receipt + audit + outbox boundary.
#[async_trait]
pub trait CommandUnitOfWork: Send + Sync {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError>;
    async fn commit(
        &self,
        request: PlatformAdministrationCommit,
    ) -> Result<StoredCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoldingView {
    pub holding_id: PlatformHoldingId,
    pub version: u64,
    pub code: String,
    pub name: String,
    pub environment_id: EnvironmentId,
    pub customer_id: Uuid,
    pub canonical_holding_id: Uuid,
    pub data_profile_key: String,
    pub status: HoldingLifecycleState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&PlatformHolding> for HoldingView {
    fn from(value: &PlatformHolding) -> Self {
        Self {
            holding_id: value.id(),
            version: value.version(),
            code: value.code().into(),
            name: value.name().into(),
            environment_id: value.environment_id(),
            customer_id: value.customer_id(),
            canonical_holding_id: value.canonical_holding_id(),
            data_profile_key: value.data_profile_key().into(),
            status: value.status(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorView {
    pub connector_id: ConnectorBindingId,
    pub version: u64,
    pub scope: ConnectorScope,
    pub kind: ConnectorKind,
    pub key: String,
    pub configuration_digest: String,
    pub capabilities: std::collections::BTreeSet<String>,
    pub state: ConnectorState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&ConnectorBinding> for ConnectorView {
    fn from(value: &ConnectorBinding) -> Self {
        Self {
            connector_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            kind: value.kind().clone(),
            key: value.key().into(),
            configuration_digest: value.configuration_digest().into(),
            capabilities: value.capabilities().clone(),
            state: value.state(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

/// Deliberately excludes device/session identifiers and the opaque context ID value from diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminContextView {
    pub context_id: AdminContextId,
    pub version: u64,
    pub holding_id: PlatformHoldingId,
    pub scope: AuthorityScope,
    pub principal_ref: String,
    pub reason: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub status: AdminContextStatus,
}

impl AdminContextView {
    pub fn from_context(value: &HoldingAdminContext, at: DateTime<Utc>) -> Self {
        Self {
            context_id: value.id(),
            version: value.version(),
            holding_id: value.holding_id(),
            scope: value.scope().clone(),
            principal_ref: value.principal_ref().into(),
            reason: value.reason().into(),
            issued_at: value.issued_at(),
            expires_at: value.expires_at(),
            revoked_at: value.revoked_at(),
            status: value.status_at(at),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "record", rename_all = "snake_case")]
pub enum PlatformRecordView {
    Holding(HoldingView),
    Connector(ConnectorView),
    AdminContext(AdminContextView),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageFilter {
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    pub records: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadProjection<T> {
    pub value: T,
    pub metadata: ProjectionMetadata,
}

/// Disposable projection access only. Query handlers have no write-store path.
#[async_trait]
pub trait ReadRepository: Send + Sync {
    async fn get_holding(
        &self,
        scope: &AuthorityScope,
        id: PlatformHoldingId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<HoldingView>>, PortError>;
    async fn list_holdings(
        &self,
        scope: &AuthorityScope,
        filter: &PageFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Page<HoldingView>>, PortError>;
    async fn get_connector(
        &self,
        scope: &AuthorityScope,
        id: ConnectorBindingId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ConnectorView>>, PortError>;
    async fn list_connectors(
        &self,
        scope: &AuthorityScope,
        filter: &PageFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Page<ConnectorView>>, PortError>;
    async fn get_admin_context(
        &self,
        scope: &AuthorityScope,
        id: AdminContextId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<AdminContextView>>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("platform administration operation is denied")]
    Denied,
    #[error("platform administration entitlement is not enabled")]
    EntitlementDenied,
    #[error("fresh MFA is required")]
    MfaRequired,
    #[error("platform administration record was not found")]
    NotFound,
    #[error(
        "platform administration aggregate version conflict (expected {expected}, actual {actual})"
    )]
    Conflict { expected: u64, actual: u64 },
    #[error("idempotency identity was reused for a different command")]
    IdempotencyConflict,
    #[error("platform administration storage is temporarily unavailable")]
    Unavailable,
    #[error("platform administration projection has not reached the requested command")]
    NotReady(ProjectionNotReady),
    #[error("platform administration adapter failed")]
    Internal,
}
