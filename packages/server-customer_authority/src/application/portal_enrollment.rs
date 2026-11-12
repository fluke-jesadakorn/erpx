//! Native Customer Portal invitation enrollment use cases.
//!
//! Ports own the Holding transaction and the Control session proof. This layer
//! only orders the exact steps: token/live claims, reservation, source match,
//! session re-proof, identity binding and immutable acceptance evidence.
use crate::domain::portal_enrollment::{
    AcceptedInvitation, EnrollmentInvite, EnrollmentReservation, InvitationMatch,
    require_accept_match, require_bound_identity, require_live_claims, require_replay_identity,
};
use crate::domain::portal_invitation_delivery::InvitationClaims;
use erpx_server_shared::public::BusinessError;
use std::future::Future;
use std::pin::Pin;

pub type EnrollmentFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// The verified portal session proof consumed by the owner steps.
pub use erpx_server_identity_access::public::portal_session::PortalSessionProof as PortalSession;

/// Control-owned session verification supplied by composition.
pub trait PortalSessionPort: Send {
    fn current<'a>(
        &'a mut self,
        expected: Option<&'a PortalSession>,
    ) -> EnrollmentFuture<'a, PortalSession, BusinessError>;
}

/// The current verified portal identity. Identity Access verifies the Control
/// session; this module only consumes the resulting proof.
#[derive(Clone, Debug)]
pub struct PortalActor {
    pub user_id: String,
    pub email: String,
}

pub trait EnrollmentRepository: Send {
    type Error: From<BusinessError> + Send;

    /// Session proof that is still current for the same session/email.
    fn session<'a>(
        &'a mut self,
        expected: Option<&'a PortalSession>,
    ) -> EnrollmentFuture<'a, PortalSession, Self::Error>;

    /// Reserve or reset the one-hour attempt window for this invitation.
    fn reserve<'a>(
        &'a mut self,
        invitation: &'a str,
        now: i64,
    ) -> EnrollmentFuture<'a, (i64, i64), Self::Error>;

    /// Delete or decrement only the exact in-flight reservation.
    fn release<'a>(
        &'a mut self,
        reservation: &'a EnrollmentReservation,
    ) -> EnrollmentFuture<'a, (), Self::Error>;

    /// Match the exact live invitation for these claims.
    fn invitation<'a>(
        &'a mut self,
        claims: &'a InvitationClaims,
        now: i64,
        allow_accepted: bool,
    ) -> EnrollmentFuture<'a, Option<InvitationMatch>, Self::Error>;

    fn identity<'a>(
        &'a mut self,
        binding_id: &'a str,
        lock: bool,
    ) -> EnrollmentFuture<'a, Option<(String, String)>, Self::Error>;

    fn bind_identity<'a>(
        &'a mut self,
        binding_id: &'a str,
        actor: &'a PortalActor,
        now: i64,
    ) -> EnrollmentFuture<'a, (), Self::Error>;

    fn accept<'a>(
        &'a mut self,
        invitation_id: &'a str,
        actor: &'a PortalActor,
        now: i64,
    ) -> EnrollmentFuture<'a, (), Self::Error>;
}

fn token_hash(token: &str) -> String {
    crate::adapters::portal_invitation_token::invitation_token_hash(token)
}

/// Reserve the one-hour enrollment attempt window for the exact invitation.
pub async fn reserve<R: EnrollmentRepository>(
    repository: &mut R,
    claims: &InvitationClaims,
    token: &str,
    now: i64,
) -> Result<EnrollmentInvite, R::Error> {
    require_live_claims(claims, now)?;
    let Some(invite) = repository.invitation(claims, now, false).await? else {
        return Err(crate::domain::portal_enrollment::invitation_unavailable().into());
    };
    let invite = require_accept_match(
        Some(invite),
        claims,
        &claims.holding_slug,
        None,
        &token_hash(token),
    )?;
    let (attempts, window_start) = repository.reserve(&invite.invitation_id, now).await?;
    Ok(EnrollmentInvite {
        email: invite.email,
        attempts,
        reservation: EnrollmentReservation {
            holding_slug: invite.holding_slug,
            binding_id: invite.binding_id,
            invitation_id: invite.invitation_id,
            window_start,
        },
    })
}

pub async fn release<R: EnrollmentRepository>(
    repository: &mut R,
    reservation: &EnrollmentReservation,
) -> Result<(), R::Error> {
    reservation.validate()?;
    repository.release(reservation).await
}

/// Accept the exact invitation under the currently verified portal session.
/// The identity comes only from the verified session proof.
pub async fn accept<R: EnrollmentRepository>(
    repository: &mut R,
    claims: &InvitationClaims,
    token: &str,
    now: i64,
) -> Result<AcceptedInvitation, R::Error> {
    require_live_claims(claims, now)?;
    let session = repository.session(None).await?;
    let actor = &PortalActor {
        user_id: session.user_id.clone(),
        email: session.email.clone(),
    };
    let invite = require_accept_match(
        repository.invitation(claims, now, true).await?,
        claims,
        &claims.holding_slug,
        Some(&actor.email),
        &token_hash(token),
    )?;
    if invite.state == "accepted" {
        let identity = repository.identity(&invite.binding_id, false).await?;
        require_replay_identity(
            &invite,
            &actor.user_id,
            identity
                .as_ref()
                .map(|(user, mail)| (user.as_str(), mail.as_str())),
        )?;
        repository.session(Some(&session)).await?;
        return Ok(AcceptedInvitation {
            binding_id: invite.binding_id,
            holding_slug: invite.holding_slug,
            accepted: true,
        });
    }
    repository
        .bind_identity(&invite.binding_id, actor, now)
        .await?;
    let identity = repository.identity(&invite.binding_id, true).await?;
    require_bound_identity(
        identity
            .as_ref()
            .map(|(user, mail)| (user.as_str(), mail.as_str())),
        &actor.user_id,
        &actor.email,
    )?;
    repository.accept(&invite.invitation_id, actor, now).await?;
    repository.session(Some(&session)).await?;
    Ok(AcceptedInvitation {
        binding_id: invite.binding_id,
        holding_slug: invite.holding_slug,
        accepted: true,
    })
}
