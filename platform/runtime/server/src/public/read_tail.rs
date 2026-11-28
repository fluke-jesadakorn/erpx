//! Owner-local facade for the native read-tail SQL contract.
//!
//! Consumers render the frozen shapes on their own unit; no statement text is
//! authored in TypeScript and no relation or predicate is accepted from an
//! untrusted caller.

pub use crate::adapters::read_tail::{
    READ_TAIL_SOURCES_JSON, executive_external_tokens, executive_generation_jobs,
    executive_report_revisions, executive_report_shares, executive_reports, holding_bindings,
    report_lock, report_version_guard, runtime_health,
};
