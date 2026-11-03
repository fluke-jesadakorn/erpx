//! Minimal actual customer identity/current source, without Finance permission.
pub use crate::adapters::field_service_customer::{
    CurrentAgreementCustomer, CurrentFieldServiceCustomer, CurrentFieldServicePhysicalCustomer,
    FieldServiceCustomerFacts, lock_field_service_customer_sources, read_agreement_customer,
    read_current_field_service_customer, read_current_field_service_physical_customer,
};

pub use crate::adapters::service_customer_preparation::{
    CurrentServiceCustomerReferences, PreparedServiceCustomer, RetainedServiceCustomerCreation,
    ServiceCustomerCurrency, ServiceCustomerCurrent, ServiceCustomerReference,
    lock_service_customer_sources, prepare_service_customer, read_service_customer_references,
};
