//! Verified external portal session read.
//!
//! Composition supplies the exact opaque token from the registered product
//! cookie and the exact SSO audience/environment for that host. Support and
//! portal private-file consumers must still resolve Customer Authority
//! enrollment before treating this as a `PortalCustomer`.
pub use crate::adapters::portal_session::{require_current_portal_session, verify_portal_session};
pub use crate::domain::portal_session::{
    valid_audience, valid_session_token, PortalSessionProof, PortalSessionReadRequest,
    MAX_PORTAL_AUDIENCE, MAX_PORTAL_SESSION_TOKEN,
};
