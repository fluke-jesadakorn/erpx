use serde::{Deserialize, Serialize};

use crate::{BoundedContext, MoneyAmount, Quantity, ReferenceCode};

owned_uuid_reference!(PurchaseOrderRef);
owned_uuid_reference!(PurchaseOrderLineRef);
owned_uuid_reference!(SupplierRef);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleasedPurchaseOrderLineFact {
    pub line: PurchaseOrderLineRef,
    pub item_code: ReferenceCode,
    pub ordered_quantity: Quantity,
    pub line_value: MoneyAmount,
    pub destination_code: ReferenceCode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleasedPurchaseOrderFact {
    pub purchase_order: PurchaseOrderRef,
    pub supplier: SupplierRef,
    pub order_code: ReferenceCode,
    pub lines: Vec<ReleasedPurchaseOrderLineFact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    PurchaseOrderReleased(ReleasedPurchaseOrderFact),
    PurchaseOrderCancelled { purchase_order: PurchaseOrderRef },
}

impl crate::IntegrationEvent for Event {
    fn owner(&self) -> BoundedContext {
        BoundedContext::Procurement
    }

    fn event_type(&self) -> &'static str {
        match self {
            Self::PurchaseOrderReleased(_) => "procurement.purchase_order_released",
            Self::PurchaseOrderCancelled { .. } => "procurement.purchase_order_cancelled",
        }
    }

    fn schema_version(&self) -> u16 {
        1
    }

    fn validate(&self) -> Result<(), crate::ContractValidationError> {
        if let Self::PurchaseOrderReleased(fact) = self
            && fact.lines.is_empty()
        {
            return Err(crate::ContractValidationError::Empty(
                "purchase_order.lines",
            ));
        }
        Ok(())
    }
}
