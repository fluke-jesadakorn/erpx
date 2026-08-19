use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{IdempotencyKey, QueryConsistency};
use erpx_kernel::{ActorAttribution, AuditContext, AuthorityScope, CommandId, CorrelationId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    CertificationDecision, CertificationItem, IdentityGovernanceRecord, PrincipalId,
    RoleAssignmentId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityGovernanceAction {
    StartAccessReview,
    DecideAccessReview,
    RequestBreakGlass,
    ActivateBreakGlass,
    TerminateBreakGlass,
    ExpireBreakGlass,
    RecordBreakGlassReview,
    View,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum IdentityGovernanceCommand {
    StartAccessReview {
        reviewer_ref: String,
        due_at: DateTime<Utc>,
        items: Vec<CertificationItem>,
    },
    DecideAccessReview {
        assignment_id: RoleAssignmentId,
        decision: CertificationDecision,
    },
    RequestBreakGlass {
        principal_id: PrincipalId,
        reason: String,
        valid_until: DateTime<Utc>,
    },
    ActivateBreakGlass {
        requester_authenticated_at: DateTime<Utc>,
        requester_mfa_at: DateTime<Utc>,
        approver_ref: String,
        approver_authenticated_at: DateTime<Utc>,
        approver_mfa_at: DateTime<Utc>,
        monitoring_available: bool,
    },
    TerminateBreakGlass,
    ExpireBreakGlass,
    RecordBreakGlassReview {
        reviewer_ref: String,
    },
}

impl IdentityGovernanceCommand {
    pub const fn action(&self) -> IdentityGovernanceAction {
        match self {
            Self::StartAccessReview { .. } => IdentityGovernanceAction::StartAccessReview,
            Self::DecideAccessReview { .. } => IdentityGovernanceAction::DecideAccessReview,
            Self::RequestBreakGlass { .. } => IdentityGovernanceAction::RequestBreakGlass,
            Self::ActivateBreakGlass { .. } => IdentityGovernanceAction::ActivateBreakGlass,
            Self::TerminateBreakGlass => IdentityGovernanceAction::TerminateBreakGlass,
            Self::ExpireBreakGlass => IdentityGovernanceAction::ExpireBreakGlass,
            Self::RecordBreakGlassReview { .. } => IdentityGovernanceAction::RecordBreakGlassReview,
        }
    }
    pub const fn is_create(&self) -> bool {
        matches!(
            self,
            Self::StartAccessReview { .. } | Self::RequestBreakGlass { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityGovernanceCommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub record_id: Uuid,
    pub command: IdentityGovernanceCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityGovernanceCommandRequest {
    pub identity: IdentityGovernanceCommandIdentity,
    pub correlation_id: CorrelationId,
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
}

pub struct IdentityGovernanceAuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a AuthorityScope,
    pub target_scope: &'a AuthorityScope,
    pub action: IdentityGovernanceAction,
}

#[async_trait]
pub trait IdentityGovernanceAuthorizationPort: Send + Sync {
    async fn authorize_governance(
        &self,
        request: IdentityGovernanceAuthorizationRequest<'_>,
    ) -> Result<(), super::PortError>;
}

#[async_trait]
pub trait IdentityGovernanceWriteRepository: Send + Sync {
    async fn load_governance_record(
        &self,
        authority_scope: &AuthorityScope,
        record_id: Uuid,
    ) -> Result<Option<IdentityGovernanceRecord>, super::PortError>;
}

/// Disposable governance projection access. Implementations must resolve the
/// exact active generation and must not fall back to the authoritative write
/// model when a projection is unavailable or behind a read-your-writes fence.
#[async_trait]
pub trait IdentityGovernanceReadRepository: Send + Sync {
    async fn get_governance_record(
        &self,
        authority_scope: &AuthorityScope,
        record_id: Uuid,
        consistency: &QueryConsistency,
    ) -> Result<super::ReadProjection<Option<IdentityGovernanceRecord>>, super::PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityGovernanceCommit {
    pub request: IdentityGovernanceCommandRequest,
    pub record: IdentityGovernanceRecord,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredIdentityGovernanceCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub record: IdentityGovernanceRecord,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[async_trait]
pub trait IdentityGovernanceUnitOfWork: Send + Sync {
    async fn find_governance_receipt(
        &self,
        identity: &IdentityGovernanceCommandIdentity,
    ) -> Result<Option<StoredIdentityGovernanceCommit>, super::PortError>;

    async fn commit_governance(
        &self,
        request: IdentityGovernanceCommit,
    ) -> Result<StoredIdentityGovernanceCommit, super::PortError>;
}
