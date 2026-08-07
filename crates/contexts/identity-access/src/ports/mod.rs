use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandReceipt, EventPosition, IdempotencyKey, ProjectionMetadata, ProjectionNotReady,
    QueryConsistency,
};
use erpx_integration_contracts::identity_access;
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, CommandId, CorrelationId, EventId, TenantScope,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    AccessPrincipal, AccessRole, IdentityAccessAggregate, PermissionKey, PrincipalId,
    PrincipalStatus, RiskTier, RoleAssignmentId, RoleCode, RoleId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityAccessAction {
    CreatePrincipal,
    ActivatePrincipal,
    SuspendPrincipal,
    ReactivatePrincipal,
    DisablePrincipal,
    AssignRole,
    RevokeRole,
    DefineRole,
    SetRolePermissions,
    ViewPrincipal,
    ListPrincipals,
    ViewRole,
    ListRoles,
}

impl IdentityAccessAction {
    pub const fn requires_write_entitlement(self) -> bool {
        !matches!(
            self,
            Self::ViewPrincipal | Self::ListPrincipals | Self::ViewRole | Self::ListRoles
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "id", rename_all = "snake_case")]
pub enum AggregateKey {
    Principal(PrincipalId),
    Role(RoleId),
}

impl AggregateKey {
    pub const fn aggregate_type(self) -> &'static str {
        match self {
            Self::Principal(_) => "access_principal",
            Self::Role(_) => "access_role",
        }
    }

    pub const fn as_uuid(self) -> uuid::Uuid {
        match self {
            Self::Principal(id) => id.as_uuid(),
            Self::Role(id) => id.as_uuid(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CommandFingerprint {
    CreatePrincipal {
        principal_id: PrincipalId,
        scope: TenantScope,
        display_name: String,
    },
    ActivatePrincipal {
        principal_id: PrincipalId,
    },
    SuspendPrincipal {
        principal_id: PrincipalId,
    },
    ReactivatePrincipal {
        principal_id: PrincipalId,
    },
    DisablePrincipal {
        principal_id: PrincipalId,
    },
    AssignRole {
        principal_id: PrincipalId,
        assignment_id: RoleAssignmentId,
        role_id: RoleId,
        scope: TenantScope,
        risk_tier: RiskTier,
        valid_from: DateTime<Utc>,
        valid_until: DateTime<Utc>,
        grant_reason: String,
        ticket_reference: Option<String>,
        grantor_ref: String,
        approver_ref: Option<String>,
    },
    RevokeRole {
        principal_id: PrincipalId,
        assignment_id: RoleAssignmentId,
    },
    DefineRole {
        role_id: RoleId,
        scope: TenantScope,
        code: RoleCode,
        display_name: String,
        risk_tier: RiskTier,
        permissions: Vec<PermissionKey>,
    },
    SetRolePermissions {
        role_id: RoleId,
        permissions: Vec<PermissionKey>,
    },
}

/// Correlation and retry timestamps are intentionally excluded so an exact
/// retry can replay its durable receipt while a changed payload conflicts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub fingerprint: CommandFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a TenantScope,
    pub target_scope: &'a TenantScope,
    pub action: IdentityAccessAction,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError>;
}

#[async_trait]
pub trait EntitlementPort: Send + Sync {
    async fn ensure_identity_access_enabled(
        &self,
        scope: &TenantScope,
        write: bool,
    ) -> Result<(), PortError>;
}

/// Authoritative aggregate reads used by command handlers only.
#[async_trait]
pub trait WriteRepository: Send + Sync {
    async fn load(
        &self,
        authority_scope: &TenantScope,
        key: AggregateKey,
    ) -> Result<Option<IdentityAccessAggregate>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityAccessCommit {
    pub identity: CommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub aggregate: IdentityAccessAggregate,
    pub integration_events: Vec<identity_access::Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: IdentityAccessAggregate,
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
                context: "identity_access".into(),
                aggregate_type: self.aggregate.aggregate_type().into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.aggregate_uuid())
                    .expect("an Identity & Access aggregate always has a non-nil identifier"),
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
#[async_trait]
pub trait CommandUnitOfWork: Send + Sync {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError>;

    async fn commit(&self, request: IdentityAccessCommit) -> Result<StoredCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleAssignmentView {
    pub assignment_id: RoleAssignmentId,
    pub role_id: RoleId,
    pub scope: TenantScope,
    pub risk_tier: RiskTier,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub grant_reason: String,
    pub ticket_reference: Option<String>,
    pub grantor_ref: String,
    pub approver_ref: Option<String>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrincipalView {
    pub principal_id: PrincipalId,
    pub version: u64,
    pub scope: TenantScope,
    pub display_name: String,
    pub status: PrincipalStatus,
    pub assignments: Vec<RoleAssignmentView>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&AccessPrincipal> for PrincipalView {
    fn from(value: &AccessPrincipal) -> Self {
        Self {
            principal_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            display_name: value.display_name().into(),
            status: value.status(),
            assignments: value
                .assignments()
                .iter()
                .map(|assignment| RoleAssignmentView {
                    assignment_id: assignment.id(),
                    role_id: assignment.role_id(),
                    scope: assignment.scope().clone(),
                    risk_tier: assignment.risk_tier(),
                    valid_from: assignment.valid_from(),
                    valid_until: assignment.valid_until(),
                    grant_reason: assignment.grant_reason().into(),
                    ticket_reference: assignment.ticket_reference().map(str::to_owned),
                    grantor_ref: assignment.grantor_ref().into(),
                    approver_ref: assignment.approver_ref().map(str::to_owned),
                    revoked_at: assignment.revoked_at(),
                })
                .collect(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleView {
    pub role_id: RoleId,
    pub version: u64,
    pub scope: TenantScope,
    pub code: RoleCode,
    pub display_name: String,
    pub risk_tier: RiskTier,
    pub permissions: Vec<PermissionKey>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&AccessRole> for RoleView {
    fn from(value: &AccessRole) -> Self {
        Self {
            role_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            code: value.code().clone(),
            display_name: value.display_name().into(),
            risk_tier: value.risk_tier(),
            permissions: value.permissions().to_vec(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PrincipalListFilter {
    pub status: Option<PrincipalStatus>,
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrincipalList {
    pub principals: Vec<PrincipalView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RoleListFilter {
    pub risk_tier: Option<RiskTier>,
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleList {
    pub roles: Vec<RoleView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadProjection<T> {
    pub value: T,
    pub metadata: ProjectionMetadata,
}

/// Disposable read-side access only. There is deliberately no write-store
/// fallback when a ReadYourWrites checkpoint has not arrived.
#[async_trait]
pub trait ReadRepository: Send + Sync {
    async fn get_principal(
        &self,
        authority_scope: &TenantScope,
        principal_id: PrincipalId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<PrincipalView>>, PortError>;

    async fn list_principals(
        &self,
        authority_scope: &TenantScope,
        filter: &PrincipalListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<PrincipalList>, PortError>;

    async fn get_role(
        &self,
        authority_scope: &TenantScope,
        role_id: RoleId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<RoleView>>, PortError>;

    async fn list_roles(
        &self,
        authority_scope: &TenantScope,
        filter: &RoleListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<RoleList>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("identity access operation is denied")]
    Denied,
    #[error("identity access entitlement is not enabled")]
    EntitlementDenied,
    #[error("identity access aggregate was not found")]
    NotFound,
    #[error("identity access aggregate type does not match the request")]
    AggregateTypeMismatch,
    #[error("identity access aggregate version conflict (expected {expected}, actual {actual})")]
    Conflict { expected: u64, actual: u64 },
    #[error("idempotency identity was reused for a different command")]
    IdempotencyConflict,
    #[error("identity access storage is temporarily unavailable")]
    Unavailable,
    #[error("identity access projection has not reached the requested command")]
    NotReady(ProjectionNotReady),
    #[error("identity access adapter failed")]
    Internal,
}
