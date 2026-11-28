//! R3 native page-layout read contract for composition. The lead registers the
//! `adapters` and this `public` module and mounts
//! [`router_for_holdings`] with one exact `(LaneTarget, DatabasePool)` per
//! registered Holding plus an authority implementation that resolves the
//! current session, work context, navigation widgets and Company authority.

pub use crate::adapters::native_r3_page_layouts_read::{
    handle, project_snapshot, router_for_holdings, validate, LaneTarget,
    NativePageLayoutsBackend, PageLayoutsReadAuthority, PageLayoutsReadContext,
    PageLayoutsReadPort, PgPageLayoutRead, ReadFuture, ReadQuery,
};
