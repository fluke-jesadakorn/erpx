//! Raw Service v1 Finance debt. No ledger, purchase or operator authority is
//! granted by these peer-reference DTOs or the opaque observation revision.
pub const CONTRACT:&str="finance.service-package-settlement.v1";
pub use crate::adapters::service_package_settlement::{
    CurrentServiceFinanceSettlement, ServiceFinanceSettlementObservation,
    ServiceFinanceSettlementReferences, ServiceFinanceSettlementOrder,
    ServiceFinanceSettlementRelease, ServiceFinanceSettlementPart,
    ServiceFinanceSettlementInvoiceOrigin, ServiceFinanceSettlementPartsOrigin,
    lock_service_finance_settlement, read_service_finance_settlement,
};
