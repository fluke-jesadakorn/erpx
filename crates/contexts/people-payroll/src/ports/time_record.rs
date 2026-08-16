use super::PortError;
use crate::domain::{EmployeeId, TimeRecord, TimeRecordStatus};
use async_trait::async_trait;
use chrono::NaiveDate;
use erpx_cqrs::IdempotencyKey;
use erpx_kernel::{
    ActorAttribution, AuditContext, CommandId, CorrelationId, RecordId, TenantScope,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRecordView {
    pub time_record_id: RecordId,
    pub version: u64,
    pub employee_id: EmployeeId,
    pub work_date: NaiveDate,
    pub minutes: u16,
    pub cost_center: String,
    pub status: TimeRecordStatus,
}

#[async_trait]
pub trait TimeRecordQueryPort: Send + Sync {
    async fn get_time_record(
        &self,
        authority_scope: &TenantScope,
        time_record_id: RecordId,
    ) -> Result<Option<TimeRecordView>, PortError>;
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRecordCommand {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub idempotency_key: IdempotencyKey,
    pub expected_version: Option<u64>,
    pub actor: ActorAttribution,
    pub audit: AuditContext,
    pub authority_scope: TenantScope,
    pub record: TimeRecord,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeRecordReceipt {
    pub record: TimeRecord,
    pub replayed: bool,
}
#[async_trait]
pub trait TimeRecordAuthorizationPort: Send + Sync {
    async fn authorize_time_record(&self, command: &TimeRecordCommand) -> Result<(), PortError>;
}
#[async_trait]
pub trait TimeRecordUnitOfWork: Send + Sync {
    async fn save_time_record(
        &self,
        command: TimeRecordCommand,
    ) -> Result<TimeRecordReceipt, PortError>;
}
