//! Minimal actual customer identity/current source, without Finance permission.
pub use crate::adapters::field_service_customer::{
    CurrentFieldServiceCustomer, FieldServiceCustomerFacts, lock_field_service_customer_sources,
    read_current_field_service_customer,
};
