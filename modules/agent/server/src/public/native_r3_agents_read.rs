//! Public surface for the additive R3 Agents exact-path native read.
//! The lead mounts `agents_read_router` in composition and binds a backend that
//! resolves the current work context before calling `PgAgentsRead`.

pub use crate::adapters::native_r3_agents_read::{
    AgentsFacts, AgentsRead, AgentsReadBackend, AgentsReadFuture, AgentsReadHttpFuture,
    AgentsReadHttpRequest, AgentsReadHttpTarget, AgentsReadRequest, PgAgentsRead,
    agents_read_router, render_agents_payload,
};
