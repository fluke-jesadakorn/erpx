//! Target-neutral Inventory view models. Context-core mapping belongs in the gateway.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryReservationItem {
    pub reservation_id: String,
    pub quantity: i64,
    pub demand_reference: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryStockItem {
    pub stock_item_id: String,
    pub version: u64,
    pub scope: String,
    pub warehouse_id: String,
    pub item_code: String,
    pub on_hand: i64,
    pub reserved: i64,
    pub available: i64,
    pub reservations: Vec<InventoryReservationItem>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryStockItems {
    pub items: Vec<InventoryStockItem>,
    pub next_cursor: Option<String>,
}
