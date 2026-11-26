//! Stable public surface for the native Maintenance organization-node read.
//!
//! The lead mounts `router_for_holdings` on the private native axum service;
//! this barrel is the only owner export the composition root needs. The
//! TypeScript transport that calls the mounted route carries no SQL or payload
//! shaping.

pub use crate::adapters::native_org_nodes::{
    NodeScope, ORG_NODES_SQL, OrgNode, OrgNodeRead, OrgNodeReadFuture, OrgNodeTarget,
    PgOrgNodeRead, handle, router, router_for_holdings, router_with_reader,
};
