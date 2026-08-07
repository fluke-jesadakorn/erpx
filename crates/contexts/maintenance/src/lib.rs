#![doc = "Maintenance bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    BlockWorkOrder, CancelWorkOrder, CompleteWorkOrder, GetWorkOrder, GetWorkOrderResult,
    ListWorkOrders, MaintenanceApplicationError, MaintenanceCommandHandler,
    MaintenanceMutationResult, MaintenanceQueryHandler, OpenWorkOrder, OpenWorkOrderResult,
    RecordDowntime, ResumeWorkOrder, ScheduleWorkOrder, StartWorkOrder,
};
pub use domain::{
    DomainEvent, Downtime, MaintenanceDomainError, MaintenanceRequest, WorkOrder, WorkOrderId,
    WorkOrderStatus,
};
pub use ports::{
    AssetFactPort, AuthorizationPort, CommandFingerprint, CommandIdentity, CommandUnitOfWork,
    EntitlementPort, MaintenanceAction, MaintenanceCommit, MaintenanceStoredCommit, PortError,
    ReadProjection, ReadRepository, WorkOrderList, WorkOrderListFilter, WorkOrderView,
    WriteRepository,
};
