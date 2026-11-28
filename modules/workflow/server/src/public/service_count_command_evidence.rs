//! Same-unit, planned Count receipt, audit and outbox evidence. It grants no
//! authority and is used only for newly executed commands, never replay.
pub use crate::adapters::service_count_command_evidence::{
    CountCommandClock, CountCommandGeneratedIds, CountCommandIdSource, PlannedCountCommandEvidence,
    RetainedCountCommandEvidence, plan_count_command_evidence, verify_historical_count_command,
};
