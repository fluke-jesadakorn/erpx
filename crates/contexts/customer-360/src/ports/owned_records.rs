use super::PortError;
use crate::domain::{Customer360OwnedRecord, Customer360RecordKind, Customer360RecordStatus};
use async_trait::async_trait;
use erpx_cqrs::IdempotencyKey;
use erpx_kernel::{
    ActorAttribution, AuditContext, CommandId, CorrelationId, RecordId, TenantScope,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Customer360OwnedRecordView {
    pub record_id: RecordId,
    pub kind: Customer360RecordKind,
    pub version: u64,
    pub customer_account_id: RecordId,
    pub title: String,
    pub status: Customer360RecordStatus,
}

#[async_trait]
pub trait Customer360OwnedRecordQueryPort: Send + Sync {
    async fn get_owned_record(
        &self,
        authority_scope: &TenantScope,
        record_id: RecordId,
    ) -> Result<Option<Customer360OwnedRecordView>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Customer360RecordCommand {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub idempotency_key: IdempotencyKey,
    pub expected_version: Option<u64>,
    pub actor: ActorAttribution,
    pub audit: AuditContext,
    pub authority_scope: TenantScope,
    pub record: Customer360OwnedRecord,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Customer360RecordReceipt {
    pub record: Customer360OwnedRecord,
    pub replayed: bool,
}
#[async_trait]
pub trait Customer360RecordAuthorizationPort: Send + Sync {
    async fn authorize_record(&self, command: &Customer360RecordCommand) -> Result<(), PortError>;
}
#[async_trait]
pub trait Customer360RecordUnitOfWork: Send + Sync {
    async fn save_record(
        &self,
        command: Customer360RecordCommand,
    ) -> Result<Customer360RecordReceipt, PortError>;
}
