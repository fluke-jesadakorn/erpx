//! Public owner port for the native `GET /api/v1/account/quick-access/{area}`
//! read (R3, lane L12). Transport/authority ports only; composition mounts the
//! router and supplies verified Account authority through the trait below.

pub use crate::adapters::native_r3_quick_access_get::{
    AccountArea, PgQuickAccessGet, QuickAccessGetAuthority, QuickAccessGetFuture,
    QuickAccessGetReader, QuickAccessGetRequest, QuickAccessGetScope, QuickAccessGetScopeRequest,
    QuickAccessPreferenceWire, area_from, quick_access_get_router, quick_access_get_router_with_pool,
    read_quick_access_get,
};
