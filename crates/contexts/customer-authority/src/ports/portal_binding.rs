use std::collections::BTreeSet;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::IdempotencyKey;
use erpx_integration_contracts::RecordVersionRef;
use erpx_kernel::{ActorAttribution, AuthorityScope, CommandId, CorrelationId, PrincipalId};
use serde::{Deserialize, Serialize};

use crate::{
    CustomerPortalBindingId, CustomerPortalPrincipalBinding, PortalPrincipalKind, PortalUseCase,
    VerifiedIdentityEvidence,
};

use super::PortError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortalBindingAction {
    Propose,
    Approve,
    Reject,
    Revoke,
    Expire,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortalBindingProposal {
    pub binding_id: CustomerPortalBindingId,
    pub customer: RecordVersionRef,
    pub portal_principal_id: PrincipalId,
    pub principal_kind: PortalPrincipalKind,
    pub scope: AuthorityScope,
    pub verified_identity: VerifiedIdentityEvidence,
    pub permitted_use_cases: BTreeSet<PortalUseCase>,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum PortalBindingCommandPayload {
    Propose(Box<PortalBindingProposal>),
    Approve {
        binding_id: CustomerPortalBindingId,
        decision_evidence_digest: String,
    },
    Reject {
        binding_id: CustomerPortalBindingId,
        decision_evidence_digest: String,
    },
    Revoke {
        binding_id: CustomerPortalBindingId,
        decision_evidence_digest: String,
    },
    Expire {
        binding_id: CustomerPortalBindingId,
    },
}

impl PortalBindingCommandPayload {
    pub const fn binding_id(&self) -> CustomerPortalBindingId {
        match self {
            Self::Propose(value) => value.binding_id,
            Self::Approve { binding_id, .. }
            | Self::Reject { binding_id, .. }
            | Self::Revoke { binding_id, .. }
            | Self::Expire { binding_id } => *binding_id,
        }
    }

    pub const fn action(&self) -> PortalBindingAction {
        match self {
            Self::Propose(_) => PortalBindingAction::Propose,
            Self::Approve { .. } => PortalBindingAction::Approve,
            Self::Reject { .. } => PortalBindingAction::Reject,
            Self::Revoke { .. } => PortalBindingAction::Revoke,
            Self::Expire { .. } => PortalBindingAction::Expire,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortalBindingCommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub payload: PortalBindingCommandPayload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortalBindingCommandRequest {
    pub identity: PortalBindingCommandIdentity,
    pub correlation_id: CorrelationId,
    pub actor: ActorAttribution,
    pub actor_principal_id: PrincipalId,
    pub authority_scope: AuthorityScope,
    pub expected_version: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortalBindingCommit {
    pub request: PortalBindingCommandRequest,
    pub aggregate: CustomerPortalPrincipalBinding,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPortalBindingCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: CustomerPortalPrincipalBinding,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[async_trait]
pub trait PortalBindingAuthorizationPort: Send + Sync {
    async fn authorize_portal_binding(
        &self,
        actor: &ActorAttribution,
        actor_principal_id: PrincipalId,
        authority_scope: &AuthorityScope,
        target_scope: &AuthorityScope,
        action: PortalBindingAction,
    ) -> Result<(), PortError>;
}

#[async_trait]
pub trait CustomerRecordAuthorityPort: Send + Sync {
    async fn ensure_active_customer_version(
        &self,
        customer: &RecordVersionRef,
        scope: &AuthorityScope,
    ) -> Result<(), PortError>;
}

#[async_trait]
pub trait PortalBindingWriteRepository: Send + Sync {
    async fn load_portal_binding(
        &self,
        authority_scope: &AuthorityScope,
        binding_id: CustomerPortalBindingId,
    ) -> Result<Option<CustomerPortalPrincipalBinding>, PortError>;
}

#[async_trait]
pub trait PortalBindingCommandUnitOfWork: Send + Sync {
    async fn find_portal_binding_receipt(
        &self,
        request: &PortalBindingCommandRequest,
    ) -> Result<Option<StoredPortalBindingCommit>, PortError>;

    async fn commit_portal_binding(
        &self,
        request: PortalBindingCommit,
    ) -> Result<StoredPortalBindingCommit, PortError>;
}
