#![doc = "Versioned, transport-neutral facts exchanged between ERPX bounded contexts."]

#[macro_use]
mod macros;

mod common;
mod envelope;

pub mod assets;
pub mod brand;
pub mod commercial;
pub mod crm;
pub mod customer;
pub mod finance;
pub mod identity_access;
pub mod inventory;
pub mod legal;
pub mod maintenance;
pub mod marketing;
pub mod operations;
pub mod organization;
pub mod people_payroll;
pub mod platform_administration;
pub mod pos;
pub mod procurement;
pub mod quality;
pub mod sales;
pub mod support;

pub use common::{
    BoundedText, ContractValidationError, CurrencyCode, DisplayName, MoneyAmount, Quantity,
    ReferenceCode,
};
pub use envelope::{
    AggregateKind, BoundedContext, EventType, IntegrationEvent, IntegrationEventEnvelope,
};

#[cfg(test)]
mod tests;
