use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{IdempotencyKey, QueryConsistency};
use erpx_kernel::{ActorAttribution, AuditContext, CommandId, CorrelationId, TenantScope};
use serde::{Deserialize, Serialize};

use crate::{
    OrganizationSharingAgreement, ProposeSharingAgreement, ReadProjection, SharingAgreementId,
    SharingStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharingAction {
    Propose,
    ApproveSource,
    ApproveTarget,
    Reject,
    Revoke,
    Expire,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", content = "payload", rename_all = "snake_case")]
pub enum SharingCommandPayload {
    Propose(Box<ProposeSharingAgreement>),
    ApproveSource { approver_ref: String },
    ApproveTarget { approver_ref: String },
    Reject { actor_ref: String },
    Revoke { actor_ref: String },
    Expire,
}

impl SharingCommandPayload {
    pub const fn action(&self) -> SharingAction {
        match self {
            Self::Propose(_) => SharingAction::Propose,
            Self::ApproveSource { .. } => SharingAction::ApproveSource,
            Self::ApproveTarget { .. } => SharingAction::ApproveTarget,
            Self::Reject { .. } => SharingAction::Reject,
            Self::Revoke { .. } => SharingAction::Revoke,
            Self::Expire => SharingAction::Expire,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharingCommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub sharing_agreement_id: SharingAgreementId,
    pub payload: SharingCommandPayload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharingCommandRequest {
    pub identity: SharingCommandIdentity,
    pub correlation_id: CorrelationId,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharingCommit {
    pub request: SharingCommandRequest,
    pub aggregate: OrganizationSharingAgreement,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSharingCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: OrganizationSharingAgreement,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharingAuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a TenantScope,
    pub source_scope: &'a TenantScope,
    pub target_scope: &'a TenantScope,
    pub action: SharingAction,
}

#[async_trait]
pub trait SharingAuthorizationPort: Send + Sync {
    async fn authorize(
        &self,
        request: SharingAuthorizationRequest<'_>,
    ) -> Result<(), super::PortError>;
}

#[async_trait]
pub trait SharingWriteRepository: Send + Sync {
    async fn load_sharing_agreement(
        &self,
        authority_scope: &TenantScope,
        sharing_agreement_id: SharingAgreementId,
    ) -> Result<Option<OrganizationSharingAgreement>, super::PortError>;
}

#[async_trait]
pub trait SharingCommandUnitOfWork: Send + Sync {
    async fn find_sharing_receipt(
        &self,
        identity: &SharingCommandIdentity,
    ) -> Result<Option<StoredSharingCommit>, super::PortError>;

    async fn commit_sharing(
        &self,
        request: SharingCommit,
    ) -> Result<StoredSharingCommit, super::PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharingListFilter {
    pub status: Option<SharingStatus>,
    pub page_size: u16,
    pub after_id: Option<SharingAgreementId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharingAgreementList {
    pub agreements: Vec<OrganizationSharingAgreement>,
    pub next_cursor: Option<SharingAgreementId>,
}

/// Consumer-owned, disposable projection access for sharing agreements. The
/// implementation must resolve an active generation and never fall back to
/// the normalized write authority.
#[async_trait]
pub trait SharingReadRepository: Send + Sync {
    async fn get_sharing_agreement(
        &self,
        authority_scope: &TenantScope,
        sharing_agreement_id: SharingAgreementId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<OrganizationSharingAgreement>>, super::PortError>;

    async fn list_sharing_agreements(
        &self,
        authority_scope: &TenantScope,
        filter: &SharingListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<SharingAgreementList>, super::PortError>;
}
