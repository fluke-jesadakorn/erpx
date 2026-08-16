use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::IdempotencyKey;
use erpx_kernel::{
    ActorAttribution, AuditContext, CommandId, CorrelationId, RecordId, TenantScope,
};
use serde::{Deserialize, Serialize};

use super::PortError;
use crate::domain::{
    CustomerAuthorityRecord, CustomerAuthorityRecordKind, CustomerAuthorityRecordStatus, CustomerId,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerAuthorityRecordView {
    pub record_id: RecordId,
    pub kind: CustomerAuthorityRecordKind,
    pub version: u64,
    pub customer_id: CustomerId,
    pub status: CustomerAuthorityRecordStatus,
    pub valid_until: DateTime<Utc>,
}

#[async_trait]
pub trait CustomerAuthorityRecordQueryPort: Send + Sync {
    /// Returns only the active read generation and fails closed when no active
    /// generation exists. Authorization is evaluated before this port.
    async fn get_record(
        &self,
        authority_scope: &TenantScope,
        record_id: RecordId,
    ) -> Result<Option<CustomerAuthorityRecordView>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerAuthorityRecordCommand {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub idempotency_key: IdempotencyKey,
    pub expected_version: Option<u64>,
    pub actor: ActorAttribution,
    pub audit: AuditContext,
    pub authority_scope: TenantScope,
    pub record: CustomerAuthorityRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerAuthorityRecordReceipt {
    pub record: CustomerAuthorityRecord,
    pub replayed: bool,
}

#[async_trait]
pub trait CustomerAuthorityRecordAuthorizationPort: Send + Sync {
    async fn authorize_record(
        &self,
        command: &CustomerAuthorityRecordCommand,
    ) -> Result<(), PortError>;
}

#[async_trait]
pub trait CustomerAuthorityRecordUnitOfWork: Send + Sync {
    async fn save_record(
        &self,
        command: CustomerAuthorityRecordCommand,
    ) -> Result<CustomerAuthorityRecordReceipt, PortError>;
}
