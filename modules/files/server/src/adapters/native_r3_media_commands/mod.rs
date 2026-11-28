//! Native owner slice for the exact media-publication command routes:
//! POST /media-publications/order,
//! POST /media-publications/{publication_id}/approve and
//! POST /media-publications/{publication_id}/revoke.
//!
//! `domain` holds pure validation and replay identities, `application` owns
//! authority decisions, `postgres` executes all statements with SQLx in the
//! caller Holding's transaction and `http` is the bounded axum transport.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    CommandContext, CommandFuture, CommandTarget, PublicationCommandStore, approve, reorder,
    revoke,
};
pub use domain::{
    OrderInput, OrderItem, approval_event_id, can_review, order_event_id, parse_order,
    parse_reason, request_hash, revocation_event_id,
};
pub use http::{media_commands_router, media_commands_router_for_holdings};
pub use postgres::PgPublicationCommandStore;
