//! Minimal actual customer identity/current source, without Finance permission.
pub use crate::adapters::field_service_customer::{
    CurrentAgreementCustomer, CurrentFieldServiceCustomer, CurrentFieldServicePhysicalCustomer,
    FieldServiceCustomerFacts, lock_field_service_customer_sources, read_agreement_customer,
    read_current_field_service_customer, read_current_field_service_physical_customer,
};
