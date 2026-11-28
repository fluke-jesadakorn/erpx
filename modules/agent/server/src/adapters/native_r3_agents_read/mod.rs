//! Additive native read for the exact R3 route
//! `/api/v1/holdings/{holding_slug}/agents` (GET).
//!
//! The retained TypeScript handler still computes the workspace payload and
//! overlays the native evidence read. This module owns the exact-path ingress
//! and the PostgreSQL execution of the retained Agent workspace statements, so
//! the lead can mount a native route that returns the same `{data: ...}`
//! envelope without the TypeScript business path. No router, `mod.rs`, Cargo
//! or composition file is edited here: the lead mounts the returned router.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{AgentsRead, AgentsReadFuture, AgentsReadRequest, render_agents_payload};
pub use domain::AgentsFacts;
pub use http::{
    AgentsReadBackend, AgentsReadHttpFuture, AgentsReadHttpRequest, AgentsReadHttpTarget,
    agents_read_router,
};
pub use postgres::PgAgentsRead;
