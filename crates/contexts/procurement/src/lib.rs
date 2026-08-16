#![doc = "Procurement bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    CancelPurchaseOrder, CreatePurchaseOrder, CreatePurchaseOrderResult, GetPurchaseOrder,
    GetPurchaseOrderResult, ListPurchaseOrders, ProcurementApplicationError,
    ProcurementCommandHandler, ProcurementQueryHandler, ProcurementRecordApplicationError,
    ProcurementRecordCommandHandler, PurchaseOrderMutationResult, ReleasePurchaseOrder,
};
pub use domain::{
    ProcurementDomainError, ProcurementRecord, ProcurementRecordError, ProcurementRecordKind,
    ProcurementRecordStatus, PurchaseOrder, PurchaseOrderId, PurchaseOrderLine,
    PurchaseOrderLineId, PurchaseOrderState, PurchaseOrderStatus, SupplierId,
};
pub use ports::{
    AuthorizationPort, CommandFingerprint, CommandIdentity, CommandUnitOfWork, EntitlementPort,
    PortError, ProcurementAction, ProcurementCommit, ProcurementRecordAction,
    ProcurementRecordAuthorizationPort, ProcurementRecordAuthorizationRequest,
    ProcurementRecordCommand, ProcurementRecordCommandIdentity, ProcurementRecordCommandRequest,
    ProcurementRecordCommit, ProcurementRecordReadRepository, ProcurementRecordUnitOfWork,
    ProcurementRecordWriteRepository, PurchaseOrderList, PurchaseOrderListFilter,
    PurchaseOrderView, ReadProjection, ReadRepository, StoredCommit, StoredProcurementRecordCommit,
    WriteRepository,
};
