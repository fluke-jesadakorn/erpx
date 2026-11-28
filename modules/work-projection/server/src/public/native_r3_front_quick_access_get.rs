//! Public owner port for the native `GET /api/v1/front-access/quick-access`
//! read (R3, lane L14). Transport/authority ports only; composition mounts the
//! router and supplies verified Front account authority through the trait below.

pub use crate::adapters::native_r3_front_quick_access_get::{
    default_front_hosts, front_quick_access_router, front_quick_access_router_with_pool,
    read_front_quick_access, FrontApp, FrontQuickAccessAuthority, FrontQuickAccessFuture,
    FrontQuickAccessPreferenceWire, FrontQuickAccessRequest, FrontQuickAccessReader,
    FrontQuickAccessScope, FrontQuickAccessScopeRequest, PgFrontQuickAccess, DEFAULT_FRONT_HOSTS,
};
