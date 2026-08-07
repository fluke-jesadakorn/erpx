//! Feature-local POS view models. Core/protobuf mapping stays in the gateway.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosSaleLineItem {
    pub line_id: String,
    pub item_reference: String,
    pub description: String,
    pub quantity: u64,
    pub total_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosPaymentItem {
    pub payment_id: String,
    pub amount_minor: i64,
    pub method: String,
    pub received_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosSaleItem {
    pub sale_id: String,
    pub version: u64,
    pub scope: String,
    pub receipt_code: String,
    pub register_id: String,
    pub customer_id: Option<String>,
    pub currency: String,
    pub lines: Vec<PosSaleLineItem>,
    pub payments: Vec<PosPaymentItem>,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub total_minor: i64,
    pub paid_minor: i64,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosSaleItems {
    pub sales: Vec<PosSaleItem>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosRefundLineItem {
    pub sale_line_id: String,
    pub quantity: u64,
    pub amount_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosRefundItem {
    pub refund_id: String,
    pub version: u64,
    pub scope: String,
    pub sale_id: String,
    pub currency: String,
    pub lines: Vec<PosRefundLineItem>,
    pub total_amount_minor: i64,
    pub reason: String,
    pub requester_principal_ref: String,
    pub reviewer_principal_ref: Option<String>,
    pub status: String,
    pub requested_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosRefundItems {
    pub refunds: Vec<PosRefundItem>,
    pub next_cursor: Option<String>,
}
