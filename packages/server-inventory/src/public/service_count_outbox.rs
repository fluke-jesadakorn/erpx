//! Inventory-owned read-only outbox ownership predicates; `e` is the outbox alias.
//! Quarantine is deliberately broader than eligible: malformed source evidence
//! remains outside the legacy claimant and is never silently reassigned.
pub const SERVICE_COUNT_OUTBOX_ELIGIBLE: &str = include_str!("service_count_outbox_eligible.sql");
pub const SERVICE_COUNT_OUTBOX_QUARANTINE: &str = include_str!("service_count_outbox_quarantine.sql");
pub const SERVICE_COUNT_OUTBOX_AMBIGUOUS: &str = include_str!("service_count_outbox_ambiguous.sql");
