use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{BoundedContext, MoneyAmount, ReferenceCode, customer::CustomerRef};

owned_uuid_reference!(PosTransactionRef);
owned_uuid_reference!(SettlementRef);
owned_uuid_reference!(RegisterRef);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PosSaleFact {
    pub transaction: PosTransactionRef,
    pub register: RegisterRef,
    pub receipt_code: ReferenceCode,
    pub customer: Option<CustomerRef>,
    pub gross_amount: MoneyAmount,
    pub completed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PosRefundFact {
    pub refund: PosTransactionRef,
    pub original_sale: PosTransactionRef,
    pub amount: MoneyAmount,
    pub approved_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettlementFact {
    pub settlement: SettlementRef,
    pub register: RegisterRef,
    pub settlement_code: ReferenceCode,
    pub sales_total: MoneyAmount,
    pub refunds_total: MoneyAmount,
    pub closed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    SaleCompleted(PosSaleFact),
    RefundApproved(PosRefundFact),
    SettlementClosed(SettlementFact),
}

impl_integration_event!(Event, BoundedContext::Pos, {
    Self::SaleCompleted(_) => "pos.sale_completed",
    Self::RefundApproved(_) => "pos.refund_approved",
    Self::SettlementClosed(_) => "pos.settlement_closed",
});
