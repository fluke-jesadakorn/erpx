//! Verified external portal customer proof.
//!
//! Composition resolves the registered portal host/slug, extracts the exact
//! opaque product-cookie token, verifies it with the Identity Access session
//! owner, and maps that proof into [`resolve_portal_customer`] with one Holding
//! connection:
//!
//! ```ignore
//! struct SessionVerifier<'c>(&'c mut sqlx::PgConnection);
//! impl PortalSessionVerifier for SessionVerifier<'_> {
//!     type Error = PgTransactionError<BusinessError>;
//!     fn verify_session<'a>(
//!         &'a mut self,
//!         request: &'a PortalCustomerRequest<'_>,
//!         now: i64,
//!     ) -> PortalSessionFuture<'a, Option<PortalSessionEvidence>, Self::Error> {
//!         Box::pin(async move {
//!             let read = PortalSessionReadRequest {
//!                 token: request.token,
//!                 audience: request.audience,
//!                 environment: request.environment,
//!             };
//!             Ok(verify_portal_session(self.0, &read, now).await?.map(|proof| {
//!                 PortalSessionEvidence {
//!                     session_id: proof.session_id,
//!                     user_id: proof.user_id,
//!                     email: proof.email,
//!                     email_verified: proof.email_verified,
//!                     authenticated_at: proof.authenticated_at,
//!                     expires_at: proof.expires_at,
//!                 }
//!             }))
//!         })
//!     }
//! }
//!
//! let mut verifier = SessionVerifier(control);
//! let mut enrollment = PgPortalEnrollment::new(holding);
//! let proof = resolve_portal_customer(&mut verifier, &mut enrollment, &request, now).await?;
//! let customer = PortalCustomer {
//!     user_id: proof.user_id,
//!     email: proof.email,
//!     holding_id: proof.holding_id,
//! };
//! ```
//!
//! Support portal projection/acknowledgement and portal private files consume
//! that proof only. An absent, expired, revoked, unverified or wrong-Holding
//! session fails closed; there is no TS fallback and no HTTP-header identity.
pub use crate::adapters::portal_session::PgPortalEnrollment;
pub use crate::application::portal_session::{
    resolve_portal_customer, PortalEnrollmentSource, PortalSessionFuture, PortalSessionVerifier,
};
pub use crate::domain::portal_session::{
    email_unverified, invalid_request, normalize_email, not_found, proof_is_current,
    session_required, validate_request, PortalCustomerProof, PortalCustomerRequest,
    PortalSessionEvidence, MAX_EMAIL, MAX_HOLDING_ID, MAX_HOLDING_SLUG, MAX_PORTAL_AUDIENCE,
    MAX_PORTAL_TOKEN,
};
