//! Marketing owner. The native slice reads the Company-scoped campaign ledger,
//! campaign/lead attribution summary, Marketing asset profiles and lead
//! candidate detail/engagement history, and performs owner-local campaign
//! create, candidate capture, engagement and CRM-port handoff commands. Assets'
//! Files-owned readiness, Finance cost linking and the composition HTTP mount
//! remain outside this crate.
mod adapters;
mod application;
mod domain;
pub mod public;
