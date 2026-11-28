//! R3 native read for `GET /api/v1/holdings/{holding_slug}/page-layouts/{page_key}`.
//!
//! Additive owner module for the lead to mount; no existing router, composition
//! or TypeScript file is changed here. The retained owner SQL, pure projection
//! and exact `{data: LayoutSnapshot}` HTTP transport already live in this crate
//! (`page_layouts_read.rs`, `domain/page_layouts.rs`, `page_layouts_http.rs`).
//! This module supplies the missing read-only Pg backend binding: composition
//! resolves the session/Company/navigation/authority facts into
//! [`PageLayoutsReadContext`], the owner executes the exact `page_layouts`
//! SELECT on its scoped pool, and the existing router applies the envelope and
//! security headers. The mutation half is intentionally not served: `command`
//! fails closed until the advisory-lock/receipt writer is migrated, so the lead
//! keeps the TypeScript POST path mounted separately.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{handle, PageLayoutsReadPort, ReadFuture};
pub use domain::{project_snapshot, validate, ReadQuery};
pub use http::{
    router_for_holdings, LaneTarget, NativePageLayoutsBackend, PageLayoutsReadAuthority,
    PageLayoutsReadContext,
};
pub use postgres::PgPageLayoutRead;
