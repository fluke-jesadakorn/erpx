//! Native scheduled Support SLA drain contract.
//!
//! Composition supplies the already-authorized Holding transaction, Company
//! target and durable supervisor. This owner-owned surface supplies the single
//! sweep and the interval/recovery policy; it opens no transaction or provider.
pub const CONTRACT_VERSION: &str = "support.sla-drain.v1";

pub use crate::adapters::sla_drain::drain_support_sla;
pub use crate::application::sla_drain::{
    DEFAULT_INTERVAL_MS, MAX_BACKOFF_MS, MAX_INTERVAL_MS, MIN_INTERVAL_MS, SlaDrainSchedule,
    run_sla_drain_tick,
};
