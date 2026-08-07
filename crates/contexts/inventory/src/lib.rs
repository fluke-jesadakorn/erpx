#![doc = "Inventory bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    GetStockItem, GetStockItemResult, InventoryApplicationError, InventoryCommandHandler,
    InventoryMutationResult, InventoryQueryHandler, IssueStock, ListStockItems, ReceiveStock,
    RegisterStockItem, RegisterStockItemResult, ReleaseInventoryReservation, ReserveInventory,
};
pub use domain::{
    InventoryDomainError, ProcurementLineReference, Reservation, ReservationId, ReservationStatus,
    StockItem, StockItemId, StockMovementId, WarehouseId,
};
pub use ports::{
    AuthorizationPort, CommandFingerprint, CommandIdentity, CommandUnitOfWork, EntitlementPort,
    InventoryAction, InventoryCommit, PortError, ReadProjection, ReadRepository, StockItemList,
    StockItemListFilter, StockItemView, StoredCommit, WriteRepository,
};
