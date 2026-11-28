//! Native migration lane 39 for ecommerce.
//!
//! The owner-executed workspace channel/cart list family: Rust owns both SQL
//! queries, the Company/Branch/Department scope, the projection and the response
//! shape. TypeScript is a strict, fail-closed multi-holding transport.

mod application;
mod domain;
mod http;
mod postgres;
mod workspace_rows;

pub use application::{WorkspaceListFuture, WorkspaceListRead, read_workspace_lists};
pub use workspace_rows::{
    CHECKOUTS_SQL, COMMERCE_ORDERS, COMMERCE_PRODUCTS, LISTINGS_SQL, PgWorkspaceRowRead,
    WorkspaceRowFuture, WorkspaceRowRead, WorkspaceRows, read_workspace_rows,
};
pub use domain::{
    CartRow, ChannelRow, MAX_ROWS, WorkspaceLists, WorkspaceScope, valid_id, validate_scope,
};
pub use http::{LaneTarget, WorkspaceListTarget, router, router_for_holdings};
pub use postgres::{
    CARTS_SQL, CHANNELS_SQL, PgWorkspaceListRead,
};
