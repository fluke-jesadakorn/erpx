//! Files owns exact Field set/command correlation; target IDs alone do not
//! classify events because another owner can use the same record identifier.
pub const QUARANTINE: &str = include_str!("field_outbox_quarantine.sql");
pub const ELIGIBLE: &str = include_str!("field_outbox_eligible.sql");
