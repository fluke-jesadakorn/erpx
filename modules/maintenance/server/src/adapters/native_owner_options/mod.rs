//! Native owner-option read for Maintenance.
//!
//! Owner-local successor of the `people` branch of the TypeScript work
//! repository (`adapters/work.ts`). That branch is the only remaining TS
//! authority that prepares and executes the eligible-owner SQL consumed by the
//! feature-maintenance preventive console. The axum router owns the statement,
//! the projection and the Company scope; there is no TypeScript SQL fallback.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{OwnerOptionRead, OwnerOptionReadFuture, handle};
pub use domain::{OwnerOption, OwnerScope, OWNER_OPTIONS_SQL, valid_id};
pub use http::{OwnerOptionTarget, router, router_for_holdings, router_with_reader};
pub use postgres::PgOwnerOptionRead;
