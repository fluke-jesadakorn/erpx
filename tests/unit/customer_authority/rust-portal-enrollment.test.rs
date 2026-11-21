//! Native Customer Portal invitation enrollment rules and orchestration.
//!
//! Proves live-claims/token binding, one-hour attempt reservation facts, release
//! of only the in-flight reservation, acceptance identity binding, replay and
//! the session re-proof around owner effects.
use erpx_server_customer_authority::public::portal_enrollment::{
    AcceptedInvitation, EnrollmentFuture, EnrollmentInvite, EnrollmentRepository,
    EnrollmentReservation, InvitationMatch, PortalActor, PortalSession, accept,
    invitation_unavailable, release, reserve,
};
use erpx_server_customer_authority::public::portal_invitation_delivery::{
    InvitationClaims, invitation_token, invitation_token_hash,
};
use erpx_server_shared::public::BusinessError;
use std::sync::Mutex;

const SECRET: &str = "vector-secret-2026";
const NOW: i64 = 1_800_000_000_000;

fn claims() -> InvitationClaims {
    InvitationClaims {
        invitation_id: "customer-portal-invitation-1".into(),
        binding_id: "binding-1".into(),
        holding_slug: "acme".into(),
        generation: 1,
        expires_at: NOW + 86_400_000,
        key_version: 1,
    }
}

fn session() -> PortalSession {
    PortalSession {
        session_id: "session-1".into(),
        user_id: "portal-user".into(),
        email: "customer@example.invalid".into(),
        email_verified: true,
        authenticated_at: NOW - 1_000,
        expires_at: NOW + 60_000,
    }
}

fn invite(state: &str, email: &str, claims: &InvitationClaims, token: &str) -> InvitationMatch {
    InvitationMatch {
        invitation_id: claims.invitation_id.clone(),
        binding_id: claims.binding_id.clone(),
        holding_slug: claims.holding_slug.clone(),
        email: email.into(),
        state: state.into(),
        accepted_user_id: None,
        token_hash: invitation_token_hash(token),
    }
}

#[derive(Default)]
struct State {
    match_row: Option<InvitationMatch>,
    identity: Option<(String, String)>,
    sessions: usize,
    released: usize,
    bound: usize,
    accepted: usize,
}

struct Fake {
    state: Mutex<State>,
    session_error: bool,
}

impl Fake {
    fn new(match_row: Option<InvitationMatch>) -> Self {
        Self {
            state: Mutex::new(State {
                match_row,
                ..State::default()
            }),
            session_error: false,
        }
    }
}

impl EnrollmentRepository for Fake {
    type Error = BusinessError;

    fn session<'a>(
        &'a mut self,
        _expected: Option<&'a PortalSession>,
    ) -> EnrollmentFuture<'a, PortalSession, Self::Error> {
        Box::pin(async move {
            self.state.lock().unwrap().sessions += 1;
            if self.session_error {
                return Err(BusinessError::new(
                    erpx_server_shared::public::FailureKind::Unauthenticated,
                    "CUSTOMER_PORTAL_SESSION_REQUIRED",
                    "Sign in again to accept this invitation.",
                ));
            }
            Ok(session())
        })
    }

    fn reserve<'a>(
        &'a mut self,
        _invitation: &'a str,
        now: i64,
    ) -> EnrollmentFuture<'a, (i64, i64), Self::Error> {
        Box::pin(async move { Ok((1, now)) })
    }

    fn release<'a>(
        &'a mut self,
        reservation: &'a EnrollmentReservation,
    ) -> EnrollmentFuture<'a, (), Self::Error> {
        Box::pin(async move {
            reservation.validate()?;
            self.state.lock().unwrap().released += 1;
            Ok(())
        })
    }

    fn invitation<'a>(
        &'a mut self,
        _claims: &'a InvitationClaims,
        _now: i64,
        _allow_accepted: bool,
    ) -> EnrollmentFuture<'a, Option<InvitationMatch>, Self::Error> {
        Box::pin(async move { Ok(self.state.lock().unwrap().match_row.clone()) })
    }

    fn identity<'a>(
        &'a mut self,
        _binding_id: &'a str,
        _lock: bool,
    ) -> EnrollmentFuture<'a, Option<(String, String)>, Self::Error> {
        Box::pin(async move { Ok(self.state.lock().unwrap().identity.clone()) })
    }

    fn bind_identity<'a>(
        &'a mut self,
        _binding_id: &'a str,
        actor: &'a PortalActor,
        _now: i64,
    ) -> EnrollmentFuture<'a, (), Self::Error> {
        Box::pin(async move {
            let mut state = self.state.lock().unwrap();
            state.identity = Some((actor.user_id.clone(), actor.email.clone()));
            state.bound += 1;
            Ok(())
        })
    }

    fn accept<'a>(
        &'a mut self,
        _invitation_id: &'a str,
        actor: &'a PortalActor,
        _now: i64,
    ) -> EnrollmentFuture<'a, (), Self::Error> {
        Box::pin(async move {
            let mut state = self.state.lock().unwrap();
            state.match_row = state.match_row.clone().map(|mut row| {
                row.state = "accepted".into();
                row.accepted_user_id = Some(actor.user_id.clone());
                row
            });
            state.accepted += 1;
            Ok(())
        })
    }
}

#[tokio::test]
async fn reserve_returns_the_exact_invite_facts_and_reservation() {
    let token = invitation_token(SECRET, &claims());
    let mut repository = Fake::new(Some(invite(
        "sent",
        "customer@example.invalid",
        &claims(),
        &token,
    )));
    let EnrollmentInvite {
        email,
        attempts,
        reservation,
    } = reserve(&mut repository, &claims(), &token, NOW)
        .await
        .unwrap();
    assert_eq!(email, "customer@example.invalid");
    assert_eq!(attempts, 1);
    assert_eq!(reservation.holding_slug, "acme");
    assert_eq!(reservation.binding_id, "binding-1");
    assert_eq!(reservation.invitation_id, "customer-portal-invitation-1");
    assert_eq!(reservation.window_start, NOW);
}

#[tokio::test]
async fn reserve_rejects_a_tampered_token_or_unavailable_invitation() {
    let mut repository = Fake::new(Some(invite(
        "sent",
        "customer@example.invalid",
        &claims(),
        "other-token",
    )));
    assert_eq!(
        reserve(&mut repository, &claims(), "token", NOW)
            .await
            .unwrap_err()
            .code,
        "CUSTOMER_PORTAL_INVITATION_UNAVAILABLE"
    );
    let mut empty = Fake::new(None);
    assert_eq!(
        reserve(&mut empty, &claims(), "token", NOW)
            .await
            .unwrap_err()
            .code,
        "CUSTOMER_PORTAL_INVITATION_UNAVAILABLE"
    );
    let mut expired = claims();
    expired.expires_at = NOW;
    assert_eq!(
        reserve(&mut repository, &expired, "token", NOW)
            .await
            .unwrap_err()
            .code,
        "CUSTOMER_PORTAL_INVITATION_UNAVAILABLE"
    );
}

#[tokio::test]
async fn release_accepts_only_the_exact_reservation_shape() {
    let mut repository = Fake::new(None);
    let reservation = EnrollmentReservation {
        holding_slug: "acme".into(),
        binding_id: "binding-1".into(),
        invitation_id: "customer-portal-invitation-1".into(),
        window_start: NOW,
    };
    release(&mut repository, &reservation).await.unwrap();
    assert_eq!(repository.state.lock().unwrap().released, 1);
    let forged = EnrollmentReservation {
        invitation_id: "../etc".into(),
        ..reservation
    };
    assert_eq!(
        release(&mut repository, &forged).await.unwrap_err().code,
        "CUSTOMER_PORTAL_ENROLLMENT_RESERVATION_INVALID"
    );
}

#[tokio::test]
async fn accept_binds_the_verified_session_identity_only_once() {
    let token = invitation_token(SECRET, &claims());
    let mut repository = Fake::new(Some(invite(
        "sent",
        "customer@example.invalid",
        &claims(),
        &token,
    )));
    let accepted = accept(&mut repository, &claims(), &token, NOW)
        .await
        .unwrap();
    assert_eq!(
        accepted,
        AcceptedInvitation {
            binding_id: "binding-1".into(),
            holding_slug: "acme".into(),
            accepted: true
        }
    );
    let state = repository.state.lock().unwrap();
    assert_eq!((state.bound, state.accepted), (1, 1));
    assert_eq!(state.sessions, 2, "the session is re-proved after effects");
}

#[tokio::test]
async fn an_accepted_invitation_replays_only_for_the_same_identity() {
    let token = invitation_token(SECRET, &claims());
    let mut row = invite("accepted", "customer@example.invalid", &claims(), &token);
    row.accepted_user_id = Some("portal-user".into());
    let mut repository = Fake::new(Some(row));
    repository.state.lock().unwrap().identity =
        Some(("portal-user".into(), "customer@example.invalid".into()));
    let accepted = accept(&mut repository, &claims(), &token, NOW)
        .await
        .unwrap();
    assert!(accepted.accepted);
    assert_eq!(
        repository.state.lock().unwrap().accepted,
        0,
        "replay does not re-accept"
    );

    let mut other = Fake::new(Some(invite(
        "accepted",
        "customer@example.invalid",
        &claims(),
        &token,
    )));
    other.state.lock().unwrap().identity =
        Some(("portal-user".into(), "customer@example.invalid".into()));
    assert_eq!(
        accept(&mut other, &claims(), &token, NOW)
            .await
            .unwrap_err()
            .code,
        "CUSTOMER_PORTAL_INVITATION_UNAVAILABLE"
    );
}

#[tokio::test]
async fn a_changed_session_or_email_cannot_accept() {
    let token = invitation_token(SECRET, &claims());
    let mut wrong_email = Fake::new(Some(invite(
        "sent",
        "other@example.invalid",
        &claims(),
        &token,
    )));
    assert_eq!(
        accept(&mut wrong_email, &claims(), &token, NOW)
            .await
            .unwrap_err()
            .code,
        "CUSTOMER_PORTAL_INVITATION_UNAVAILABLE"
    );
    let mut revoked = Fake::new(Some(invite(
        "sent",
        "customer@example.invalid",
        &claims(),
        &token,
    )));
    revoked.session_error = true;
    assert_eq!(
        accept(&mut revoked, &claims(), &token, NOW)
            .await
            .unwrap_err()
            .code,
        "CUSTOMER_PORTAL_SESSION_REQUIRED"
    );
    assert_eq!(
        invitation_unavailable().code,
        "CUSTOMER_PORTAL_INVITATION_UNAVAILABLE"
    );
}
