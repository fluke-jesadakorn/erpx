//! Native migration lane 36 for maintenance.
//!
//! Owner-local successor of the attachment-target descriptor read shared by
//! `attachments-operational-facts.ts` and `attachments-policy-facts.ts`. The
//! axum router owns the statement, the projection and the Company scope; the
//! retained TS consumers only transport bounded identities. No SQL fallback.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{DescriptorRead, handle};
pub use domain::{DescriptorRow, DescriptorScope, TARGET_MAINTENANCE_WORK, TARGET_TYPES};
pub use http::{LaneTarget, router, router_for_holdings, router_with_reader};
pub use postgres::{DESCRIPTOR_SQL, PgDescriptorRead};
