//! Field customer/setup, invoice, receipt and source-verified parts-journal events.
//! Other Finance/Workflow owners retain their explicit consumer partition.
pub const CONTRACT_VERSION:&str="finance.field-outbox.v7";
pub const QUARANTINE:&str=include_str!("field_outbox_quarantine.sql");
pub const ELIGIBLE:&str=include_str!("field_outbox_eligible.sql");
