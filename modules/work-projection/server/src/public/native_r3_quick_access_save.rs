//! Public owner port for the native `POST /api/v1/account/quick-access/{area}`
//! command (R3, lane L12). Transport/authority ports only; composition mounts
//! the router and supplies verified Account authority through the trait below.
//! The SQL stays in this crate's `quick_access_preferences` adapter.

pub use crate::adapters::native_r3_quick_access_save::{
    PgQuickAccessSave, QuickAccessSaveAuthority, QuickAccessSaveBody, QuickAccessSaveFuture,
    QuickAccessSaveRequest, QuickAccessSaveScope, QuickAccessSaveScopeRequest,
    QuickAccessSaveTarget, QuickAccessSaveWriter, now_millis, quick_access_save_router,
    quick_access_save_router_with_pool, save_quick_access_command, valid_command_key,
};
