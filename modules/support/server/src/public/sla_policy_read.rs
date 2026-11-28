//! Native Support SLA policy point/work read contract.
//!
//! Composition supplies the already-authorized serializable Holding unit,
//! selected Support scope and `support.sla` purchase gates. These functions
//! execute the real owner SQL and return the complete retained payloads; they
//! open no transaction and keep no TypeScript fallback.
pub const CONTRACT_VERSION: &str = "support.sla-policy-read.v1";

pub use crate::adapters::sla_policy_read::{load_policy_point, load_policy_work, PgSlaPolicyRead};
pub use crate::application::sla_policy_read::{
    read_policy_point, read_policy_work, SlaPolicyReadFuture, SlaPolicyReadPorts,
};
pub use crate::domain::sla_policy_read::{
    point, work_items, SlaPolicyPointView, SlaPolicyReadScope, SlaPolicyRow, SlaPolicyWorkItem,
};
