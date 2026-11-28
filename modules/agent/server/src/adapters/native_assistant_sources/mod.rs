//! Native owner slice for the Agent assistant source-reference read.
//!
//! This is the read the retained TypeScript `assistantSources` helper performed
//! for the `GET .../assistant` family. The axum owner validates one bounded
//! page, executes a Company/user/assignment-scoped statement with sqlx, shapes
//! the exact `sourceReference` payload in Rust and returns it. No TypeScript
//! prepare/batch fallback is part of this slice.
//!
//! Composition and Next route deletion are lead-owned; this module owns only
//! the slice and its public constructors.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    AssistantSourceQuery, AssistantSourcesFuture, AssistantSourcesRead, assistant_sources,
};
pub use domain::{
    MAX_PATH, MAX_SOURCES, SQL_ASSISTANT_SOURCES, AssistantSourceRef, canonical, guide_reference,
    invalid, unavailable,
};
pub use http::{
    AssistantSourcesTarget, router_for_holdings, router_with_reader,
};
pub use postgres::PgAssistantSourcesRead;
