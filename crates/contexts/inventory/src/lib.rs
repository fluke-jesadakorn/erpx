#![doc = "Inventory bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    FulfillInventoryReservation, GetInventoryReservation, GetInventoryReservationResult,
    GetStockItem, GetStockItemResult, InventoryApplicationError, InventoryCommandHandler,
    InventoryLifecycleRecordApplicationError, InventoryLifecycleRecordCommandHandler,
    InventoryMutationResult, InventoryQueryHandler, IssueStock, ListStockItems, ReceiveStock,
    RegisterStockItem, RegisterStockItemResult, ReleaseInventoryReservation, ReserveInventory,
};
pub use domain::{
    InventoryDomainError, InventoryLifecycleRecord, InventoryLifecycleRecordError,
    InventoryLifecycleRecordKind, InventoryLifecycleRecordStatus, ProcurementLineReference,
    Reservation, ReservationId, ReservationStatus, StockItem, StockItemId, StockItemState,
    StockMovementId, WarehouseId,
};
pub use ports::{
    AuthorizationPort, CommandFingerprint, CommandIdentity, CommandUnitOfWork, EntitlementPort,
    InventoryAction, InventoryCommit, InventoryLifecycleRecordAction,
    InventoryLifecycleRecordAuthorizationPort, InventoryLifecycleRecordAuthorizationRequest,
    InventoryLifecycleRecordCommand, InventoryLifecycleRecordCommandIdentity,
    InventoryLifecycleRecordCommandRequest, InventoryLifecycleRecordCommit,
    InventoryLifecycleRecordReadRepository, InventoryLifecycleRecordUnitOfWork,
    InventoryLifecycleRecordWriteRepository, PortError, ReadProjection, ReadRepository,
    ReservationReadRepository, ReservationView, StockItemList, StockItemListFilter, StockItemView,
    StoredCommit, StoredInventoryLifecycleRecordCommit, WriteRepository,
};
