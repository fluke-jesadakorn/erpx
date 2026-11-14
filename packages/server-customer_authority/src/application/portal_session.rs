//! Customer Authority verified portal-session use case.
//!
//! Identity Access verifies the presented opaque session. This layer then
//! requires one durable enrollment identity inside the exact registered Holding
//! and produces the only `PortalCustomer`-compatible proof Support may consume.
//! No transaction is opened, no HTTP request is read and no TS fallback exists.
use crate::domain::portal_session::{
    email_unverified, normalize_email, not_found, proof_is_current, session_required,
    validate_request, PortalCustomerProof, PortalCustomerRequest, PortalSessionEvidence,
};
use erpx_server_shared::public::BusinessError;
use std::future::Future;
use std::pin::Pin;

pub type PortalSessionFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait PortalSessionVerifier {
    type Error: From<BusinessError>;

    /// Verify the opaque token against Identity Access. `None` is an absent,
    /// expired or revoked session and must never be treated as anonymous access.
    fn verify_session<'a>(
        &'a mut self,
        request: &'a PortalCustomerRequest<'_>,
        now: i64,
    ) -> PortalSessionFuture<'a, Option<PortalSessionEvidence>, Self::Error>;
}

pub trait PortalEnrollmentSource {
    type Error: From<BusinessError>;

    /// Resolve the durable portal enrollment identity for this verified
    /// session inside the exact Holding transaction. This read must see the
    /// same committed enrollment Core the consumer will act on.
    fn enrolled_customer<'a>(
        &'a mut self,
        session: &'a PortalSessionEvidence,
        request: &'a PortalCustomerRequest<'_>,
        now: i64,
    ) -> PortalSessionFuture<'a, Option<PortalCustomerProof>, Self::Error>;
}

pub async fn resolve_portal_customer<V, E, Err>(
    verifier: &mut V,
    enrollment: &mut E,
    request: &PortalCustomerRequest<'_>,
    now: i64,
) -> Result<PortalCustomerProof, Err>
where
    V: PortalSessionVerifier<Error = Err>,
    E: PortalEnrollmentSource<Error = Err>,
    Err: From<BusinessError>,
{
    validate_request(request)?;
    let session = verifier
        .verify_session(request, now)
        .await?
        .ok_or_else(session_required)?;
    if !session.email_verified {
        return Err(email_unverified().into());
    }
    if session.expires_at <= now
        || session.authenticated_at > now
        || session.session_id.is_empty()
        || session.user_id.is_empty()
    {
        return Err(session_required().into());
    }
    let Some(email) = normalize_email(&session.email) else {
        return Err(session_required().into());
    };
    let proof = enrollment
        .enrolled_customer(&session, request, now)
        .await?
        .ok_or_else(not_found)?;
    if !proof_is_current(&session, &proof, request, now) {
        return Err(not_found().into());
    }
    Ok(PortalCustomerProof {
        user_id: session.user_id,
        email,
        holding_id: Some(request.holding_id.to_owned()),
    })
}
