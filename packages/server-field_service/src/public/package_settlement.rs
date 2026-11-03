//! Versioned Field-owned settlement decisions for native Commercial consumers.
//! Raw observations remain server-side and do not grant record access.
pub const CONTRACT: &str = "field-service.package-settlement.v1";
pub use crate::adapters::package_settlement::{
    CurrentServicePackageSettlement, ServiceSettlementAgreement, ServiceSettlementRows,
    ServiceSettlementRelease, ServiceSettlementVisit, ServiceSettlementReservation, ServiceSettlementIssueEvent, lock_service_package_settlement, read_service_package_settlement,
};
pub use crate::domain::package_settlement::{
    AgreementSettlementFacts, ServiceSettlementFact, ServiceSettlementPackage,
    VisitSettlementFacts, agreement_settlement_fact, visit_settlement_fact,
};
