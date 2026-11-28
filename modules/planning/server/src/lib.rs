//! Native Planning owner.
//!
//! Qualified native slices:
//!
//! - bounded Planning workspace read over scenarios, recommendations,
//!   resources, calendar overrides and active allocations;
//! - scenario calculate with shortage/comparison decisions and the routing
//!   calendar capacity planner (including consolidated, persisted allocations);
//! - scenario cancel and independent recommendation approve/reject with
//!   persisted results, version/CAS guards, replay, receipts, audit and outbox;
//! - production resource create/update and calendar override commands.
//!
//! Manufacturing order creation during an approved release is consumed through
//! [`public::scenario::ManufacturingRelease`] so Planning never writes a
//! Manufacturing-owned row. Composition supplies current IAM authority, the
//! serializable Holding transaction, cross-owner source reads and the release
//! port. The product route merge and the TypeScript retirement remain lead
//! integration steps.
mod adapters;
mod application;
mod domain;
pub mod public;
