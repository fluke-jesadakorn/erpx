//! Public owner port for native `GET,POST /api/customer-portal/{holdingSlug}/quick-access`
//! (R3, lane L12). Transport/authority ports only; composition mounts the
//! router and supplies a verified customer scope. The preference SQL stays in
//! this crate's `quick_access_preferences` adapter.

pub use crate::adapters::native_r3_portal_quick_access::{
    PgPortalQuickAccess, PortalQuickAccessAuthority, PortalQuickAccessBody,
    PortalQuickAccessRequest, PortalQuickAccessSaveRequest, PortalQuickAccessScope,
    PortalQuickAccessScopeRequest, PortalQuickAccessTarget, portal_quick_access_get,
    portal_quick_access_post, portal_quick_access_router, portal_quick_access_router_with_pool,
    valid_slug,
};
