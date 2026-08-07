use serde::{Deserialize, Serialize};

use crate::{BoundedContext, Quantity, ReferenceCode, procurement::PurchaseOrderLineRef};

owned_uuid_reference!(StockItemRef);
owned_uuid_reference!(WarehouseRef);
owned_uuid_reference!(StockMovementRef);
owned_uuid_reference!(ReservationRef);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockMovementFact {
    pub movement: StockMovementRef,
    pub stock_item: StockItemRef,
    pub warehouse: WarehouseRef,
    pub item_code: ReferenceCode,
    pub quantity_delta: Quantity,
    pub procurement_line: Option<PurchaseOrderLineRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReservationFact {
    pub reservation: ReservationRef,
    pub stock_item: StockItemRef,
    pub warehouse: WarehouseRef,
    pub quantity: Quantity,
    pub demand_reference: ReferenceCode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    StockMoved(StockMovementFact),
    InventoryReserved(ReservationFact),
    InventoryReservationReleased { reservation: ReservationRef },
}

impl_integration_event!(Event, BoundedContext::Inventory, {
    Self::StockMoved(_) => "inventory.stock_moved",
    Self::InventoryReserved(_) => "inventory.reserved",
    Self::InventoryReservationReleased { .. } => "inventory.reservation_released",
});
