use super::PortError;
use crate::domain::{PlatformCaseStatus, PlatformSupportCase};
use async_trait::async_trait;
use erpx_cqrs::IdempotencyKey;
use erpx_kernel::{
    ActorAttribution, AuditContext, AuthorityScope, CommandId, CorrelationId, RecordId,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformCaseView {
    pub case_id: RecordId,
    pub version: u64,
    pub case_code: String,
    pub title: String,
    pub status: PlatformCaseStatus,
}

#[async_trait]
pub trait PlatformCaseQueryPort: Send + Sync {
    async fn get_platform_case(
        &self,
        authority_scope: &AuthorityScope,
        case_id: RecordId,
    ) -> Result<Option<PlatformCaseView>, PortError>;
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformCaseCommand {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub idempotency_key: IdempotencyKey,
    pub expected_version: Option<u64>,
    pub actor: ActorAttribution,
    pub audit: AuditContext,
    pub authority_scope: AuthorityScope,
    pub record: PlatformSupportCase,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformCaseReceipt {
    pub record: PlatformSupportCase,
    pub replayed: bool,
}
#[async_trait]
pub trait PlatformCaseAuthorizationPort: Send + Sync {
    async fn authorize_platform_case(&self, command: &PlatformCaseCommand)
    -> Result<(), PortError>;
}
#[async_trait]
pub trait PlatformCaseUnitOfWork: Send + Sync {
    async fn save_platform_case(
        &self,
        command: PlatformCaseCommand,
    ) -> Result<PlatformCaseReceipt, PortError>;
}
