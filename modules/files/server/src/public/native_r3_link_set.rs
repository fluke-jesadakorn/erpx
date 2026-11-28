//! Public owner exports for the R3 generic attachment links-by-set native read.
//!
//! The lead mounts `router` in composition and adds the gateway entry; no
//! TypeScript route, re-export or shared config is changed by this module.
pub use crate::adapters::native_r3_link_set::{
    LinkSetFuture, LinkSetRead, LinkSetRequest, LinkSetResponse, LinkSetTarget, PgLinkSetRead,
    LINKS_BY_SET_SQL, MAX_LINKS, MAX_RESPONSE_BYTES, handle, router, valid_set_id, valid_slug,
};

/// Exact mount line for the owner crate:
/// `pub mod native_r3_link_set;`
pub use crate::adapters::native_r3_link_set::LinkRow;
