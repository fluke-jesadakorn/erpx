//! Native organization-node option read for Maintenance.
//!
//! Owner-local successor of the `nodes` branch of the TypeScript work
//! repository (`adapters/work.ts`). That branch is the remaining TS authority
//! that prepares and executes the organization_nodes projection consumed by the
//! Maintenance/Assets branch/department selectors. The axum router owns the
//! statement, the projection and the Company/branch/department scope; there is
//! no TypeScript SQL fallback.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{OrgNodeRead, OrgNodeReadFuture, handle};
pub use domain::{NodeScope, ORG_NODES_SQL, OrgNode, valid_id};
pub use http::{OrgNodeTarget, router, router_for_holdings, router_with_reader};
pub use postgres::PgOrgNodeRead;
