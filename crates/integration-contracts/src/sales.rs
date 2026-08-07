use serde::{Deserialize, Serialize};

use crate::{BoundedContext, MoneyAmount, Quantity, ReferenceCode, customer::CustomerRef};

owned_uuid_reference!(SalesOrderRef);
owned_uuid_reference!(SalesOrderLineRef);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SalesOrderStatus {
    Confirmed,
    PartiallyFulfilled,
    Fulfilled,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesOrderLineFact {
    pub line: SalesOrderLineRef,
    pub item_code: ReferenceCode,
    pub quantity: Quantity,
    pub line_value: MoneyAmount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesOrderFact {
    pub sales_order: SalesOrderRef,
    pub order_code: ReferenceCode,
    pub customer: CustomerRef,
    pub status: SalesOrderStatus,
    pub total: MoneyAmount,
    pub lines: Vec<SalesOrderLineFact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    SalesOrderChanged(SalesOrderFact),
}

impl crate::IntegrationEvent for Event {
    fn owner(&self) -> BoundedContext {
        BoundedContext::Sales
    }

    fn event_type(&self) -> &'static str {
        "sales.order_changed"
    }

    fn schema_version(&self) -> u16 {
        1
    }

    fn validate(&self) -> Result<(), crate::ContractValidationError> {
        let Self::SalesOrderChanged(fact) = self;
        if fact.lines.is_empty() {
            return Err(crate::ContractValidationError::Empty("sales_order.lines"));
        }
        Ok(())
    }
}
