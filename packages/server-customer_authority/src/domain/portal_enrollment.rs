//! Native Customer Portal invitation enrollment policy.
//!
//! The signed token names one immutable invitation. Enrollment reserves a
//! one-hour attempt window, and acceptance binds exactly one verified portal
//! identity to the approved binding while retaining immutable evidence. No
//! database, clock or HTTP effect is reachable from this module.
use crate::domain::portal_invitation_delivery::InvitationClaims;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

pub const ATTEMPT_WINDOW_MILLIS: i64 = 3_600_000;
pub const MAX_ATTEMPTS: i64 = 3;
pub const ACCEPT_REASON: &str = "Verified invited customer accepted this exact binding.";

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CUSTOMER_PORTAL_INVITATION_UNAVAILABLE",
        "This customer invitation is no longer available.",
    )
}

pub fn invitation_unavailable() -> BusinessError {
    unavailable()
}

pub fn session_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Unauthenticated,
        "CUSTOMER_PORTAL_SESSION_REQUIRED",
        "Sign in again to accept this invitation.",
    )
}

pub fn attempt_invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_PORTAL_ENROLLMENT_RESERVATION_INVALID",
        "Provide the exact in-flight enrollment reservation.",
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvitationMatch {
    pub invitation_id: String,
    pub binding_id: String,
    pub holding_slug: String,
    pub email: String,
    pub state: String,
    pub accepted_user_id: Option<String>,
    pub token_hash: String,
}

/// The exact in-flight reservation returned with the invite facts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollmentReservation {
    pub holding_slug: String,
    pub binding_id: String,
    pub invitation_id: String,
    pub window_start: i64,
}

impl EnrollmentReservation {
    pub fn validate(&self) -> Result<(), BusinessError> {
        let identifier = |value: &str| {
            !value.is_empty()
                && value.len() <= 200
                && !value
                    .bytes()
                    .any(|byte| byte < 0x20 || byte == 0x7f || byte == b'/' || byte == b'\\')
        };
        if !identifier(&self.holding_slug)
            || !identifier(&self.binding_id)
            || !identifier(&self.invitation_id)
            || self.window_start < 0
        {
            return Err(attempt_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollmentInvite {
    pub email: String,
    pub attempts: i64,
    pub reservation: EnrollmentReservation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptedInvitation {
    pub binding_id: String,
    pub holding_slug: String,
    pub accepted: bool,
}

/// Claims must be live and self-consistent before any SQL.
pub fn require_live_claims(claims: &InvitationClaims, now: i64) -> Result<(), BusinessError> {
    let shaped = !claims.invitation_id.is_empty()
        && claims.invitation_id.len() <= 200
        && !claims.binding_id.is_empty()
        && claims.binding_id.len() <= 200
        && !claims.holding_slug.is_empty()
        && claims.holding_slug.len() <= 100
        && claims.generation >= 1
        && claims.expires_at >= 1
        && claims.key_version == 1;
    if !shaped || claims.expires_at <= now {
        return Err(unavailable());
    }
    Ok(())
}

/// The exact live invitation for these claims. `actor_email` is required for
/// acceptance and omitted while reserving the attempt window.
pub fn require_accept_match(
    invite: Option<InvitationMatch>,
    claims: &InvitationClaims,
    holding_slug: &str,
    actor_email: Option<&str>,
    token_hash: &str,
) -> Result<InvitationMatch, BusinessError> {
    let Some(invite) = invite else {
        return Err(unavailable());
    };
    if invite.invitation_id != claims.invitation_id
        || invite.binding_id != claims.binding_id
        || invite.holding_slug != holding_slug
        || invite.holding_slug != claims.holding_slug
        || invite.token_hash != token_hash
        || !matches!(invite.state.as_str(), "sent" | "accepted")
        || actor_email
            .is_some_and(|email| invite.email.to_lowercase() != email.trim().to_lowercase())
    {
        return Err(unavailable());
    }
    Ok(invite)
}

/// A replayed acceptance is valid only for the same original identity.
pub fn require_replay_identity(
    invite: &InvitationMatch,
    user_id: &str,
    identity: Option<(&str, &str)>,
) -> Result<(), BusinessError> {
    let same = invite.accepted_user_id.as_deref() == Some(user_id)
        && identity.is_some_and(|(identity_user, identity_email)| {
            identity_user == user_id && identity_email.eq_ignore_ascii_case(&invite.email)
        });
    if !same {
        return Err(unavailable());
    }
    Ok(())
}

pub fn require_bound_identity(
    identity: Option<(&str, &str)>,
    user_id: &str,
    email: &str,
) -> Result<(), BusinessError> {
    if !identity.is_some_and(|(identity_user, identity_email)| {
        identity_user == user_id && identity_email.eq_ignore_ascii_case(email)
    }) {
        return Err(unavailable());
    }
    Ok(())
}

pub fn accepted_event_id(invitation_id: &str) -> String {
    format!("customer-portal-invitation-event-accepted:{invitation_id}")
}
