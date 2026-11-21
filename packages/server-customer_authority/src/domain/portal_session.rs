//! Verified external portal customer proof.
//!
//! The proof is the only value Support portal projection/acknowledgement and
//! portal private-file consumers accept for an unauthenticated external
//! customer. Identity Access verifies the session; Customer Authority verifies
//! the durable enrollment identity in the exact Holding. Neither layer accepts
//! an HTTP header, caller-supplied email or an unverified token claim.
use erpx_server_shared::public::{BusinessError, FailureKind};

pub const MAX_PORTAL_TOKEN: usize = 512;
pub const MAX_HOLDING_ID: usize = 200;
pub const MAX_HOLDING_SLUG: usize = 100;
pub const MAX_PORTAL_AUDIENCE: usize = 300;
pub const MAX_EMAIL: usize = 254;

/// Identity Access proof normalized for the Customer Authority port. It carries
/// no selected role, Company or business permission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortalSessionEvidence {
    pub session_id: String,
    pub user_id: String,
    pub email: String,
    pub email_verified: bool,
    pub authenticated_at: i64,
    pub expires_at: i64,
}

/// `PortalCustomer`-compatible proof for Support. `holding_id` is the exact
/// registered Holding that owns the enrollment; it is never inferred from a URL
/// by a consumer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortalCustomerProof {
    pub user_id: String,
    pub email: String,
    pub holding_id: Option<String>,
}

impl PortalCustomerProof {
    pub fn normalized_email(&self) -> String {
        self.email.to_lowercase()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortalCustomerRequest<'a> {
    /// Opaque session token from the registered product cookie.
    pub token: &'a str,
    /// Exact SSO audience of the presented portal session. `None` retains the
    /// direct Account-session mode when product SSO routing is disabled.
    pub audience: Option<&'a str>,
    /// Exact registered runtime environment.
    pub environment: &'a str,
    /// Registered Holding that owns the session's enrollment.
    pub holding_id: &'a str,
    /// Registered Holding slug from the requested portal URL.
    pub holding_slug: &'a str,
}

pub fn session_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Unauthenticated,
        "CUSTOMER_PORTAL_SESSION_REQUIRED",
        "Sign in to open customer documents.",
    )
}

pub fn email_unverified() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "CUSTOMER_EMAIL_UNVERIFIED",
        "Verify your account email before opening customer documents.",
    )
}

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CUSTOMER_PORTAL_NOT_FOUND",
        "This verified customer portal is unavailable.",
    )
}

pub fn invalid_request() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_PORTAL_SESSION_INVALID",
        "Provide a verified customer portal session.",
    )
}

fn bounded(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum
}

/// Trims and lowercases one practical email. The session user email and the
/// enrolled binding email must both pass this exact normalization.
pub fn normalize_email(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_EMAIL || value.contains(char::is_whitespace) {
        return None;
    }
    let mut parts = value.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    if local.is_empty()
        || domain.is_empty()
        || parts.next().is_some()
        || !domain.contains('.')
        || value.chars().any(char::is_control)
    {
        return None;
    }
    Some(value.to_lowercase())
}

/// Exact bounded request facts. A missing or malformed tenant/session value is
/// rejected before any source read.
pub fn validate_request(request: &PortalCustomerRequest<'_>) -> Result<(), BusinessError> {
    let token = request.token;
    if !bounded(token, MAX_PORTAL_TOKEN)
        || !token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~'))
        || request.audience.is_some_and(|audience| {
            !bounded(audience, MAX_PORTAL_AUDIENCE)
                || !audience.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'-' | b'_' | b'.')
                })
        })
        || !bounded(request.environment, 40)
        || !request.environment.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
        || !bounded(request.holding_id, MAX_HOLDING_ID)
        || !bounded(request.holding_slug, MAX_HOLDING_SLUG)
        || !request
            .holding_slug
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(invalid_request());
    }
    Ok(())
}

/// Final invariant between the session proof and the enrolled customer. Any
/// disagreement is indistinguishable from absent enrollment.
pub fn proof_is_current(
    session: &PortalSessionEvidence,
    proof: &PortalCustomerProof,
    request: &PortalCustomerRequest<'_>,
    now: i64,
) -> bool {
    let session_email = normalize_email(&session.email);
    let proof_email = normalize_email(&proof.email);
    session.expires_at > now
        && session.authenticated_at <= now
        && !session.session_id.is_empty()
        && proof.user_id == session.user_id
        && session_email.is_some()
        && session_email == proof_email
        && proof.holding_id.as_deref() == Some(request.holding_id)
}
