//! Native migration slice for the bounded Legal portal surface.
//!
//! `GET /api/legal-portal/{holdingSlug}` executes this owner's share-list SQL
//! in Rust. `GET,POST .../{holdingSlug}/quick-access` uses the existing owner
//! preference SQL through composition-supplied ports. The ordered session,
//! guest-share, purchase-enforced, file and command continuations remain in the
//! TypeScript owner and are reported as blockers; this module never duplicates
//! them and never falls back to them.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    PortalActor, PortalFuture, PortalPreferences, PortalReadRequest, PortalSaveInput,
    PortalSaveRequest, PortalSource, PortalVerifyRequest, PortalVerified, PortalAuthority,
    portal_list, portal_quick_access_get, portal_quick_access_post,
};
pub use domain::{
    BODY_LIMIT, LIST_OPERATION, PURCHASE_PREDICATE_OPEN, QUICK_ACCESS_OPERATION,
    QUICK_ACCESS_SURFACE, PortalShareWire, QuickAccessBody, QuickAccessPreferenceWire,
    valid_body, valid_key, valid_predicate, valid_slug,
};
pub use http::{PortalTarget, router};
pub use postgres::{PgPortalSource, SHARES_SQL};
