#![doc = "Procurement bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    AcceptGoodsReceipt, ApprovePurchaseRequisition, CancelGoodsReceipt, CancelPurchaseOrder,
    CancelPurchaseRequisition, CreateGoodsReceipt, CreatePurchaseOrder, CreatePurchaseOrderResult,
    CreatePurchaseRequisition, GetPurchaseOrder, GetPurchaseOrderResult, ListPurchaseOrders,
    ProcurementApplicationError, ProcurementCommandHandler, ProcurementQueryHandler,
    ProcurementRecordApplicationError, ProcurementRecordCommandHandler,
    ProcurementWorkflowApplicationError, ProcurementWorkflowCommandService,
    PurchaseOrderMutationResult, RejectGoodsReceipt, RejectPurchaseRequisition,
    ReleasePurchaseOrder, SubmitPurchaseRequisition,
};
pub use domain::{
    GoodsReceipt, GoodsReceiptDomainEvent, GoodsReceiptId, GoodsReceiptLine, GoodsReceiptLineId,
    GoodsReceiptState, GoodsReceiptStatus, InventoryStockItemId, ProcurementDomainError,
    ProcurementRecord, ProcurementRecordError, ProcurementRecordKind, ProcurementRecordStatus,
    PurchaseOrder, PurchaseOrderId, PurchaseOrderLine, PurchaseOrderLineId, PurchaseOrderState,
    PurchaseOrderStatus, PurchaseRequisition, PurchaseRequisitionDomainEvent,
    PurchaseRequisitionId, PurchaseRequisitionLine, PurchaseRequisitionLineId,
    PurchaseRequisitionState, PurchaseRequisitionStatus, ReceiptLineAllowance, SupplierId,
};
pub use ports::{
    AuthorizationPort, CommandFingerprint, CommandIdentity, CommandUnitOfWork, EntitlementPort,
    PortError, ProcurementAction, ProcurementCommit, ProcurementRecordAction,
    ProcurementRecordAuthorizationPort, ProcurementRecordAuthorizationRequest,
    ProcurementRecordCommand, ProcurementRecordCommandIdentity, ProcurementRecordCommandRequest,
    ProcurementRecordCommit, ProcurementRecordReadRepository, ProcurementRecordUnitOfWork,
    ProcurementRecordWriteRepository, PurchaseOrderList, PurchaseOrderListFilter,
    PurchaseOrderSourcePort, PurchaseOrderView, ReadProjection, ReadRepository, StoredCommit,
    StoredProcurementRecordCommit, WriteRepository,
};
pub use ports::{
    ExactRecordSourceSnapshot, GoodsReceiptView, InventoryStockItemSource, ProcurementSourceGuard,
    ProcurementWorkflowAction, ProcurementWorkflowAggregate, ProcurementWorkflowAuthorizationPort,
    ProcurementWorkflowAuthorizationRequest, ProcurementWorkflowCommandContext,
    ProcurementWorkflowCommandFingerprint, ProcurementWorkflowCommandIdentity,
    ProcurementWorkflowCommit, ProcurementWorkflowDomainEvent, ProcurementWorkflowReadRepository,
    ProcurementWorkflowRepository, ProcurementWorkflowUnitOfWork, PurchaseOrderReceiptSource,
    PurchaseRequisitionView, StoredProcurementWorkflowCommit,
};
