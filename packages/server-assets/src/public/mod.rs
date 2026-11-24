pub mod asset_register;
pub mod asset_purchase_source;
pub mod asset_http;
pub mod maintenance;

pub mod field_service_reference;

pub mod field_service_workspace;

pub mod native_read;
pub mod native_read_http;
pub use crate::adapters::native_read::{bind, BoundPlan, ReadScope};

pub mod record_source;

pub mod retirement_work;
