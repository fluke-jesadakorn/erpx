//! Production typed-v1 transports for the Wave 6/7 business contexts.
//!
//! Generated protobuf and Tonic stay below this module. Feature gateways see
//! only context-core commands, queries, receipts, results, and bounded UI
//! errors.

// The integrator registers these leaf adapters before the Console composition
// root constructs them during the coordinated v1 cutover.
#![allow(dead_code, unused_imports)]

mod assets;
pub(crate) mod common;
mod crm;
mod inventory;
mod maintenance;
mod marketing;
mod people_payroll;
mod pos;
mod procurement;
mod sales;

pub(crate) use assets::AssetsGrpcTransport;
pub(crate) use common::Wave67Route;
pub(crate) use crm::CrmGrpcTransport;
pub(crate) use inventory::InventoryGrpcTransport;
pub(crate) use maintenance::MaintenanceGrpcTransport;
pub(crate) use marketing::MarketingGrpcTransport;
pub(crate) use people_payroll::PeoplePayrollGrpcTransport;
pub(crate) use pos::PosGrpcTransport;
pub(crate) use procurement::ProcurementGrpcTransport;
pub(crate) use sales::SalesGrpcTransport;
