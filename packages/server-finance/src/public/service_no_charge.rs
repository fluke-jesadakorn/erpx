//! Fully covered Field invoice fulfillment; same-unit native owner contracts.
pub const CONTRACT_VERSION: &str = "finance.service-no-charge.v1";
pub use crate::adapters::service_no_charge::ServiceNoChargeControls;
pub use crate::adapters::service_no_charge::read_optional_service_no_charge;
pub use crate::adapters::service_no_charge::service_no_charge_persistence_error;
pub use crate::adapters::service_no_charge::{
    CurrentServiceNoCharge, RetainedServiceNoCharge, lock_service_no_charge_sources,
    read_service_no_charge,
};
pub use crate::adapters::service_no_charge::{ServiceNoChargeInput,ServiceNoChargeWire,ServiceNoChargeStatus,ServiceNoChargeResult,ServiceNoChargeResponse, normalize_service_no_charge};
pub use crate::domain::service_no_charge::require_no_charge_decision;
