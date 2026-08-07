#![doc = "Procurement bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    CancelPurchaseOrder, CreatePurchaseOrder, CreatePurchaseOrderResult, GetPurchaseOrder,
    GetPurchaseOrderResult, ListPurchaseOrders, ProcurementApplicationError,
    ProcurementCommandHandler, ProcurementQueryHandler, PurchaseOrderMutationResult,
    ReleasePurchaseOrder,
};
pub use domain::{
    ProcurementDomainError, PurchaseOrder, PurchaseOrderId, PurchaseOrderLine, PurchaseOrderLineId,
    PurchaseOrderStatus, SupplierId,
};
pub use ports::{
    AuthorizationPort, CommandFingerprint, CommandIdentity, CommandUnitOfWork, EntitlementPort,
    PortError, ProcurementAction, ProcurementCommit, PurchaseOrderList, PurchaseOrderListFilter,
    PurchaseOrderView, ReadProjection, ReadRepository, StoredCommit, WriteRepository,
};
