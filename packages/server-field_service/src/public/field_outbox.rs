//! Read-only owner classification for the outer Field dependency projector.
//! Quarantine is intentionally broader than eligible; malformed owned events
//! must not fall back to the legacy projector. No mutation authority is granted.
pub const CONTRACT_VERSION: &str = "field-outbox-projection.v1";
pub const QUARANTINE: &str = include_str!("field_outbox_quarantine.sql");
pub const ELIGIBLE: &str = include_str!("field_outbox_eligible.sql");
