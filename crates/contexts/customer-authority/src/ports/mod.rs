use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandReceipt, EventPosition, IdempotencyKey, ProjectionMetadata, ProjectionNotReady,
    QueryConsistency,
};
use erpx_integration_contracts::customer;
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, AuthorityScope, CommandId, CorrelationId, EventId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{CustomerAccount, CustomerCode, CustomerId, CustomerIdentity, CustomerStatus};

mod authority_records;
mod portal_binding;

pub use authority_records::{
    CustomerAuthorityRecordAuthorizationPort, CustomerAuthorityRecordCommand,
    CustomerAuthorityRecordQueryPort, CustomerAuthorityRecordReceipt,
    CustomerAuthorityRecordUnitOfWork, CustomerAuthorityRecordView,
};

pub use portal_binding::{
    CustomerRecordAuthorityPort, PortalBindingAction, PortalBindingAuthorizationPort,
    PortalBindingCommandIdentity, PortalBindingCommandPayload, PortalBindingCommandRequest,
    PortalBindingCommandUnitOfWork, PortalBindingCommit, PortalBindingProposal,
    PortalBindingWriteRepository, StoredPortalBindingCommit,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomerAuthorityAction {
    Create,
    UpdateIdentity,
    ChangeStatus,
    LinkAuthority,
    UnlinkAuthority,
    Merge,
    View,
    List,
}

impl CustomerAuthorityAction {
    pub const fn requires_write_entitlement(self) -> bool {
        !matches!(self, Self::View | Self::List)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CommandFingerprint {
    Create {
        customer_id: CustomerId,
        code: CustomerCode,
        identity: CustomerIdentity,
        primary_authority: AuthorityScope,
    },
    UpdateIdentity {
        customer_id: CustomerId,
        identity: CustomerIdentity,
    },
    ChangeStatus {
        customer_id: CustomerId,
        status: CustomerStatus,
    },
    LinkAuthority {
        customer_id: CustomerId,
        scope: AuthorityScope,
    },
    UnlinkAuthority {
        customer_id: CustomerId,
        scope: AuthorityScope,
    },
    Merge {
        retired_customer_id: CustomerId,
        surviving_customer_id: CustomerId,
    },
}

/// Stable retry identity. Correlation, causation, and timestamps are excluded
/// so an exact retry replays while a payload change conflicts.
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
    pub action: CustomerAuthorityAction,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError>;
}

#[async_trait]
pub trait EntitlementPort: Send + Sync {
    async fn ensure_customer_authority_enabled(
        &self,
        scope: &AuthorityScope,
        write: bool,
    ) -> Result<(), PortError>;
}

/// Authoritative aggregate reads used by command handlers only.
#[async_trait]
pub trait WriteRepository: Send + Sync {
    async fn load(
        &self,
        authority_scope: &AuthorityScope,
        customer_id: CustomerId,
    ) -> Result<Option<CustomerAccount>, PortError>;
}

/// Optimistic guard for the surviving aggregate in a merge. It prevents a
/// concurrently retired merge target from being accepted between preflight
/// and the atomic commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MergeTargetVersion {
    pub customer_id: CustomerId,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerAuthorityCommit {
    pub identity: CommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub merge_target: Option<MergeTargetVersion>,
    pub aggregate: CustomerAccount,
    pub integration_events: Vec<customer::Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: CustomerAccount,
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
                context: "customer_authority".into(),
                aggregate_type: "customer_account".into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id().as_uuid())
                    .expect("a Customer Authority aggregate always has a non-nil identifier"),
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

    async fn commit(&self, request: CustomerAuthorityCommit) -> Result<StoredCommit, PortError>;
}

/// Access-safe projection. Legal registration and tax identifiers remain in
/// the write aggregate and are deliberately absent here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerAuthorityView {
    pub customer_id: CustomerId,
    pub version: u64,
    pub code: CustomerCode,
    pub trading_name: String,
    pub legal_name: Option<String>,
    pub country: String,
    pub status: CustomerStatus,
    pub primary_authority: AuthorityScope,
    pub authority_links: Vec<AuthorityScope>,
    pub merged_into: Option<CustomerId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl CustomerAuthorityView {
    pub fn visible_under(&self, authority: &AuthorityScope) -> bool {
        authority.validate().is_ok()
            && self
                .authority_links
                .iter()
                .any(|link| authority.contains(link) || link.contains(authority))
    }
}

impl From<&CustomerAccount> for CustomerAuthorityView {
    fn from(value: &CustomerAccount) -> Self {
        Self {
            customer_id: value.id(),
            version: value.version(),
            code: value.code().clone(),
            trading_name: value.identity().trading_name().into(),
            legal_name: value.identity().legal_name().map(str::to_owned),
            country: value.identity().country().into(),
            status: value.status(),
            primary_authority: value.primary_authority().clone(),
            authority_links: value.authority_links().to_vec(),
            merged_into: value.merged_into(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerListFilter {
    pub search: Option<String>,
    pub status: Option<CustomerStatus>,
    pub include_merged: bool,
    pub page_size: u16,
    pub after_code: Option<CustomerCode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerList {
    pub customers: Vec<CustomerAuthorityView>,
    pub next_cursor: Option<CustomerCode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadProjection<T> {
    pub value: T,
    pub metadata: ProjectionMetadata,
}

/// Query-side repository. There is intentionally no write-store fallback.
#[async_trait]
pub trait ReadRepository: Send + Sync {
    async fn get(
        &self,
        authority_scope: &AuthorityScope,
        customer_id: CustomerId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<CustomerAuthorityView>>, PortError>;

    async fn list(
        &self,
        authority_scope: &AuthorityScope,
        filter: &CustomerListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<CustomerList>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("customer authority operation is denied")]
    Denied,
    #[error("customer authority entitlement is not enabled")]
    EntitlementDenied,
    #[error("customer was not found")]
    NotFound,
    #[error("customer aggregate version conflict (expected {expected}, actual {actual})")]
    Conflict { expected: u64, actual: u64 },
    #[error("idempotency identity was reused for a different command")]
    IdempotencyConflict,
    #[error("customer authority storage is temporarily unavailable")]
    Unavailable,
    #[error("customer authority projection has not reached the requested command")]
    NotReady(ProjectionNotReady),
    #[error("customer authority adapter failed")]
    Internal,
}
