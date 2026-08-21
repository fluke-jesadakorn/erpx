use serde::{Deserialize, Serialize};

use chrono::{DateTime, Utc};

use crate::{BoundedContext, MoneyAmount, Quantity, RecordVersionRef, ReferenceCode, inventory};

owned_uuid_reference!(PurchaseRequisitionRef);
owned_uuid_reference!(PurchaseRequisitionLineRef);
owned_uuid_reference!(PurchaseOrderRef);
owned_uuid_reference!(PurchaseOrderLineRef);
owned_uuid_reference!(GoodsReceiptRef);
owned_uuid_reference!(GoodsReceiptLineRef);
owned_uuid_reference!(SupplierRef);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovedPurchaseRequisitionLineFact {
    pub line: PurchaseRequisitionLineRef,
    pub item_code: ReferenceCode,
    pub approved_quantity: Quantity,
    pub approved_line_value: MoneyAmount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovedPurchaseRequisitionFact {
    pub requisition: RecordVersionRef,
    pub requisition_code: ReferenceCode,
    pub department_code: ReferenceCode,
    pub budget_reference: ReferenceCode,
    pub lines: Vec<ApprovedPurchaseRequisitionLineFact>,
    pub approved_at: DateTime<Utc>,
}

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
pub struct AcceptedGoodsReceiptLineFact {
    pub line: GoodsReceiptLineRef,
    pub purchase_order_line: PurchaseOrderLineRef,
    pub stock_item: inventory::StockItemRef,
    pub accepted_quantity: Quantity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedGoodsReceiptFact {
    pub receipt: RecordVersionRef,
    pub purchase_order: RecordVersionRef,
    pub lines: Vec<AcceptedGoodsReceiptLineFact>,
    pub accepted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    PurchaseRequisitionApproved(ApprovedPurchaseRequisitionFact),
    PurchaseOrderReleased(ReleasedPurchaseOrderFact),
    PurchaseOrderCancelled { purchase_order: PurchaseOrderRef },
    GoodsReceiptAccepted(AcceptedGoodsReceiptFact),
}

impl crate::IntegrationEvent for Event {
    fn owner(&self) -> BoundedContext {
        BoundedContext::Procurement
    }

    fn event_type(&self) -> &'static str {
        match self {
            Self::PurchaseRequisitionApproved(_) => "procurement.purchase_requisition_approved",
            Self::PurchaseOrderReleased(_) => "procurement.purchase_order_released",
            Self::PurchaseOrderCancelled { .. } => "procurement.purchase_order_cancelled",
            Self::GoodsReceiptAccepted(_) => "procurement.goods_receipt_accepted",
        }
    }

    fn schema_version(&self) -> u16 {
        1
    }

    fn validate(&self) -> Result<(), crate::ContractValidationError> {
        match self {
            Self::PurchaseRequisitionApproved(fact) => {
                fact.requisition.validate().map_err(|_| {
                    crate::ContractValidationError::OutOfRange("purchase_requisition.ref")
                })?;
                if fact.requisition.record().record_type().as_str() != "procurement.requisition"
                    || fact.lines.is_empty()
                    || fact.lines.iter().any(|line| {
                        line.approved_quantity.units() <= 0
                            || line.approved_line_value.minor_units() < 0
                    })
                {
                    return Err(crate::ContractValidationError::OutOfRange(
                        "purchase_requisition",
                    ));
                }
            }
            Self::PurchaseOrderReleased(fact) if fact.lines.is_empty() => {
                return Err(crate::ContractValidationError::Empty(
                    "purchase_order.lines",
                ));
            }
            Self::GoodsReceiptAccepted(fact) => {
                fact.receipt
                    .validate()
                    .map_err(|_| crate::ContractValidationError::OutOfRange("goods_receipt.ref"))?;
                fact.purchase_order.validate().map_err(|_| {
                    crate::ContractValidationError::OutOfRange("purchase_order.ref")
                })?;
                if fact.receipt.record().record_type().as_str() != "procurement.receipt"
                    || fact.purchase_order.record().record_type().as_str()
                        != "procurement.purchase-order"
                    || fact.lines.is_empty()
                    || fact
                        .lines
                        .iter()
                        .any(|line| line.accepted_quantity.units() <= 0)
                {
                    return Err(crate::ContractValidationError::OutOfRange("goods_receipt"));
                }
            }
            Self::PurchaseOrderReleased(_) | Self::PurchaseOrderCancelled { .. } => {}
        }
        Ok(())
    }
}
