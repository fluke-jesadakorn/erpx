//! The Customer Authority owner's native Portal invitation delivery interface.
//!
//! Composition binds one `PortalInvitationEmailPort` and one
//! `PgPortalInvitationDelivery` per Holding, then calls `deliver_one`. The
//! owner keeps every lease, source, token and terminal rule.
pub use crate::adapters::portal_invitation_delivery::PgPortalInvitationDelivery;
pub use crate::adapters::portal_invitation_email::{
    PortalEmail, PortalEmailFuture, PortalInvitationEmailPort, ResendPortalInvitationEmail,
    SandboxPortalInvitationEmail, UnconfiguredPortalInvitationEmail,
};
pub use crate::adapters::portal_invitation_token::{
    invitation_token, invitation_token_hash, invitation_token_hash_matches,
    require_invitation_token,
};
pub use crate::application::portal_invitation_delivery::{
    PortalDeliveryFuture, PortalInvitationUnit, TokenSigner, deliver_one,
};
pub use crate::domain::portal_invitation_delivery::{
    EMAIL_DELIVERY_FAILED, EMAIL_IDEMPOTENCY_CONFLICT, EMAIL_NOT_CONFIGURED,
    EMAIL_SANDBOX_FORBIDDEN_IN_PRODUCTION, EMAIL_SANDBOX_NOT_CONFIGURED,
    EMAIL_SANDBOX_RECEIPT_MISSING, EMAIL_SENSITIVE_RECEIPT_INVALID, ENROLLMENT_PATH,
    INVITATION_AUDIENCE, INVITATION_SUBJECT, InvitationClaims, InvitationJob, LEASE_MILLIS,
    MAX_ATTEMPTS, PUBLIC_ORIGIN_NOT_CONFIGURED, SIGNING_SECRET_CHANGED, SIGNING_SECRET_UNAVAILABLE,
    SYSTEM_ACTOR, beta_email_body, iso_millis, resolve_account_origin, safe_delivery_code,
};
