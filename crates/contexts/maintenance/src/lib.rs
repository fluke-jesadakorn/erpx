#![doc = "Maintenance bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    BlockWorkOrder, CancelWorkOrder, CompleteWorkOrder, GetWorkOrder, GetWorkOrderResult,
    ListWorkOrders, MaintenanceApplicationError, MaintenanceCommandHandler,
    MaintenanceLifecycleRecordApplicationError, MaintenanceLifecycleRecordCommandHandler,
    MaintenanceMutationResult, MaintenanceQueryHandler, OpenWorkOrder, OpenWorkOrderResult,
    RecordDowntime, ResumeWorkOrder, ScheduleWorkOrder, StartWorkOrder,
};
pub use domain::{
    DomainEvent, Downtime, MaintenanceDomainError, MaintenanceLifecycleRecord,
    MaintenanceLifecycleRecordError, MaintenanceLifecycleRecordKind,
    MaintenanceLifecycleRecordStatus, MaintenanceRequest, WorkOrder, WorkOrderId, WorkOrderState,
    WorkOrderStatus,
};
pub use ports::{
    AssetFactPort, AuthorizationPort, CommandFingerprint, CommandIdentity, CommandUnitOfWork,
    EntitlementPort, MaintenanceAction, MaintenanceCommit, MaintenanceLifecycleRecordAction,
    MaintenanceLifecycleRecordAuthorizationPort, MaintenanceLifecycleRecordAuthorizationRequest,
    MaintenanceLifecycleRecordCommand, MaintenanceLifecycleRecordCommandIdentity,
    MaintenanceLifecycleRecordCommandRequest, MaintenanceLifecycleRecordCommit,
    MaintenanceLifecycleRecordReadRepository, MaintenanceLifecycleRecordUnitOfWork,
    MaintenanceLifecycleRecordWriteRepository, MaintenanceStoredCommit, PortError, ReadProjection,
    ReadRepository, StoredMaintenanceLifecycleRecordCommit, WorkOrderList, WorkOrderListFilter,
    WorkOrderView, WriteRepository,
};
