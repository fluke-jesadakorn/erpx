//! Target-neutral Sales UI models. Core and wire mapping stays in the gateway.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesLineItem {
    pub line_id: String,
    pub item_code: String,
    pub description: String,
    pub quantity: i64,
    pub line_value_minor: i64,
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesQuoteItem {
    pub quote_id: String,
    pub version: u64,
    pub scope: String,
    pub opportunity_id: String,
    pub customer_id: String,
    pub quote_code: String,
    pub currency: String,
    pub lines: Vec<SalesLineItem>,
    pub total_minor: i64,
    pub valid_until: String,
    pub status: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesOrderItem {
    pub sales_order_id: String,
    pub version: u64,
    pub scope: String,
    pub quote_id: String,
    pub opportunity_id: String,
    pub customer_id: String,
    pub order_code: String,
    pub currency: String,
    pub lines: Vec<SalesLineItem>,
    pub total_minor: i64,
    pub status: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesOrderItems {
    pub orders: Vec<SalesOrderItem>,
    pub next_cursor: Option<String>,
}
