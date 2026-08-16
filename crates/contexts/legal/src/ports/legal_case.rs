use super::PortError;
use crate::domain::{LegalCase, LegalCaseStatus};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::IdempotencyKey;
use erpx_kernel::{
    ActorAttribution, AuditContext, CommandId, CorrelationId, RecordId, TenantScope,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalCaseView {
    pub case_id: RecordId,
    pub version: u64,
    pub case_reference: String,
    pub title: String,
    pub status: LegalCaseStatus,
    pub retention_until: DateTime<Utc>,
}

#[async_trait]
pub trait LegalCaseQueryPort: Send + Sync {
    async fn get_legal_case(
        &self,
        authority_scope: &TenantScope,
        case_id: RecordId,
    ) -> Result<Option<LegalCaseView>, PortError>;
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalCaseCommand {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub idempotency_key: IdempotencyKey,
    pub expected_version: Option<u64>,
    pub actor: ActorAttribution,
    pub audit: AuditContext,
    pub authority_scope: TenantScope,
    pub record: LegalCase,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalCaseReceipt {
    pub record: LegalCase,
    pub replayed: bool,
}
#[async_trait]
pub trait LegalCaseAuthorizationPort: Send + Sync {
    async fn authorize_legal_case(&self, command: &LegalCaseCommand) -> Result<(), PortError>;
}
#[async_trait]
pub trait LegalCaseUnitOfWork: Send + Sync {
    async fn save_legal_case(
        &self,
        command: LegalCaseCommand,
    ) -> Result<LegalCaseReceipt, PortError>;
}
