//! Target-neutral Procurement view models. Context-core mapping belongs in the gateway.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcurementOrderLine {
    pub line_id: String,
    pub item_code: String,
    pub ordered_quantity: i64,
    pub unit_price_minor: i64,
    pub currency: String,
    pub destination_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcurementOrder {
    pub purchase_order_id: String,
    pub version: u64,
    pub scope: String,
    pub order_code: String,
    pub supplier_id: String,
    pub currency: String,
    pub lines: Vec<ProcurementOrderLine>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcurementOrders {
    pub orders: Vec<ProcurementOrder>,
    pub next_cursor: Option<String>,
}
