//! Public owner port for the bounded native Legal portal slice.
//!
//! Composition mounts the router and supplies the verified external actor, the
//! share reader and the existing quick-access preference owner. No session is
//! resolved or widened in the owner crate.

pub use crate::adapters::native_r3_legal_portal::{
    BODY_LIMIT, LIST_OPERATION, PURCHASE_PREDICATE_OPEN, QUICK_ACCESS_OPERATION,
    QUICK_ACCESS_SURFACE, PortalActor, PortalAuthority, PortalFuture, PortalPreferences,
    PortalReadRequest, PortalSaveInput, PortalSaveRequest, PortalShareWire, PortalSource,
    PortalTarget, PortalVerifyRequest, PortalVerified, QuickAccessBody,
    QuickAccessPreferenceWire, PgPortalSource, SHARES_SQL, portal_list,
    portal_quick_access_get, portal_quick_access_post, router, valid_body, valid_key,
    valid_predicate, valid_slug,
};
