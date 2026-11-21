//! Raw native Service v1 observation; this grants no stock or package writes.
pub const CONTRACT:&str="inventory.service-package-settlement.v1";
pub use crate::adapters::service_package_settlement::{
    CurrentServiceInventorySettlement, ServiceInventorySettlementObservation,
    ServiceInventorySettlementReferences, ServiceInventorySettlementOrder,
    ServiceInventorySettlementReservation, ServiceInventorySettlementEvent,
    lock_service_inventory_settlement, read_service_inventory_settlement,
};
