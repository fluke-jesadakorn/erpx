//! Native migration lane 19 for pms: owner-local guest-account finance evidence.
//!
//! Rust owns the statement, the scoped binds, the projection and the response
//! shape. TypeScript is a strict transport only and fails closed with 503.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{GuestAccountEvidenceRead, Lane19ReadFuture, read_guest_account_evidence};
pub use domain::{EntryEvidence, EvidenceRequest, GuestAccountEvidence};
pub use http::{
    LaneTarget, read_router, router, router_for_holdings, router_with_reader, router_with_readers,
};
pub use postgres::{ACCOUNT_SQL, ENTRIES_SQL};
