//! Native Manufacturing production accounting-profiles HTTP ingress.
//! The axum router executes the owner SELECT through the injected reader and
//! returns the complete row projection; no TypeScript prepares or executes SQL.
pub use crate::adapters::production_profiles_http::{
    ProductionProfilesTarget, router, router_for_holdings, router_with_reader,
};
