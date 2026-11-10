//! Verified external portal session proof.
//!
//! Identity Access owns the session/SSO facts. This module only describes the
//! verified proof and validates the opaque token shape; it reads no database,
//! HTTP request, clock or provider.
use erpx_server_shared::public::{BusinessError, FailureKind};

pub const MAX_PORTAL_SESSION_TOKEN: usize = 512;
pub const MAX_PORTAL_AUDIENCE: usize = 300;

/// A normal authenticated browser session bound to one exact SSO audience.
/// It is session identity only: no Company, Holding role or business grant is
/// inferred and no `PortalCustomer` projection is created here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortalSessionProof {
    pub session_id: String,
    pub user_id: String,
    pub email: String,
    pub email_verified: bool,
    pub authenticated_at: i64,
    pub expires_at: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortalSessionReadRequest<'a> {
    /// Opaque Better Auth/native session token, never a cookie header.
    pub token: &'a str,
    /// Exact SSO audience of the presented product/holding session. `None`
    /// retains the direct Account-session mode used when product SSO routing is
    /// disabled; an audience-bound caller must pass the exact host audience.
    pub audience: Option<&'a str>,
    /// Exact registered runtime environment.
    pub environment: &'a str,
}

pub fn session_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Unauthenticated,
        "CUSTOMER_PORTAL_SESSION_REQUIRED",
        "Sign in to open customer documents.",
    )
}

/// Opaque tokens are bounded printable ASCII. Cookie parsing and allow-listed
/// host selection stay in the transport; this is the last shape check before a
/// parameterized session lookup.
pub fn valid_session_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= MAX_PORTAL_SESSION_TOKEN
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~'))
}

pub fn valid_audience(audience: &str) -> bool {
    !audience.is_empty()
        && audience.len() <= MAX_PORTAL_AUDIENCE
        && audience
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'-' | b'_' | b'.'))
}
