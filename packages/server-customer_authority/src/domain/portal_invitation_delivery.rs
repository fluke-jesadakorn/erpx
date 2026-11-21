//! Native Customer Portal invitation delivery rules.
//!
//! One queued invitation is claimed under a 30 s lease, its exact approved
//! binding/order source and purchase entitlement are re-read, its signed token
//! is reproduced from immutable facts and verified against the stored digest,
//! and the private enrollment link is emailed with an idempotency key. A lost
//! lease or expired token becomes terminal evidence; a delivery failure is
//! retried at most six times with bounded exponential backoff.
use erpx_server_shared::public::{BusinessError, FailureKind};

pub const SYSTEM_ACTOR: &str = "system:customer-portal-invitation";
pub const INVITATION_AUDIENCE: &str = "erpx-customer-portal-invitation-v1";
pub const INVITATION_SUBJECT: &str = "Your ERPX Customer Portal invitation";
pub const ENROLLMENT_PATH: &str = "/customer-portal/enroll";
pub const MAX_ATTEMPTS: i32 = 6;
pub const LEASE_MILLIS: i64 = 30_000;
pub const MIN_RETRY_MILLIS: i64 = 30_000;
pub const MAX_RETRY_MILLIS: i64 = 3_600_000;

/// Retained invitable order states (`customerPortalInvitableOrderStates`).
pub const INVITABLE_ORDER_STATES: [&str; 4] =
    ["confirmed", "converted", "partially_fulfilled", "fulfilled"];

pub const TRANSACTION_REQUIRED: &str = "CUSTOMER_PORTAL_INVITATION_TRANSACTION_REQUIRED";
pub const SIGNING_SECRET_UNAVAILABLE: &str = "INVITATION_SIGNING_SECRET_UNAVAILABLE";
pub const SIGNING_SECRET_CHANGED: &str = "INVITATION_SIGNING_SECRET_CHANGED";
pub const PUBLIC_ORIGIN_NOT_CONFIGURED: &str = "PUBLIC_ORIGIN_NOT_CONFIGURED";
pub const EMAIL_NOT_CONFIGURED: &str = "EMAIL_NOT_CONFIGURED";
pub const EMAIL_SANDBOX_NOT_CONFIGURED: &str = "EMAIL_SANDBOX_NOT_CONFIGURED";
pub const EMAIL_SANDBOX_FORBIDDEN_IN_PRODUCTION: &str = "EMAIL_SANDBOX_FORBIDDEN_IN_PRODUCTION";
pub const EMAIL_SANDBOX_RECEIPT_MISSING: &str = "EMAIL_SANDBOX_RECEIPT_MISSING";
pub const EMAIL_SENSITIVE_RECEIPT_INVALID: &str = "EMAIL_SENSITIVE_RECEIPT_INVALID";
pub const EMAIL_IDEMPOTENCY_CONFLICT: &str = "EMAIL_IDEMPOTENCY_CONFLICT";
pub const EMAIL_DELIVERY_FAILED: &str = "EMAIL_DELIVERY_FAILED";

/// The single queued invitation claimed by this worker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvitationJob {
    pub id: String,
    pub binding_id: String,
    pub company_id: String,
    pub holding_slug: String,
    pub generation: i32,
    pub email: String,
    pub token_hash: String,
    pub token_key_version: i16,
    pub token_expires_at: i64,
    pub attempts: i32,
    pub lease_token: String,
}

/// Immutable signed token claims, serialized in the retained schema order.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvitationClaims {
    pub invitation_id: String,
    pub binding_id: String,
    pub holding_slug: String,
    pub generation: i32,
    pub expires_at: i64,
    pub key_version: i16,
}

impl InvitationJob {
    pub fn claims(&self) -> InvitationClaims {
        InvitationClaims {
            invitation_id: self.id.clone(),
            binding_id: self.binding_id.clone(),
            holding_slug: self.holding_slug.clone(),
            generation: self.generation,
            expires_at: self.token_expires_at,
            key_version: self.token_key_version,
        }
    }

    /// The private enrollment link the customer receives.
    pub fn enrollment_url(&self, account_origin: &str, token: &str) -> String {
        let origin = account_origin.trim_end_matches('/');
        format!("{origin}{ENROLLMENT_PATH}#token={token}")
    }

    /// Retained idempotency key; a replay reuses the exact provider message.
    pub fn delivery_key(&self) -> String {
        format!("customer-portal-invitation-{}", self.id)
    }

    /// Bounded exponential retry, capped at one hour exactly like the owner.
    pub fn next_attempt_at(&self, now: i64) -> i64 {
        let shift = u32::try_from(self.attempts).unwrap_or(u32::MAX).min(31);
        now.saturating_add(
            MIN_RETRY_MILLIS
                .saturating_mul(1_i64 << shift)
                .min(MAX_RETRY_MILLIS),
        )
    }

    pub fn terminal_state(&self) -> &'static str {
        if self.attempts >= MAX_ATTEMPTS {
            "failed"
        } else {
            "queued"
        }
    }

    /// Retained subject plus body; `betaEmailBody` wraps the private message.
    pub fn email_subject(&self) -> &'static str {
        INVITATION_SUBJECT
    }

    pub fn email_text(&self, link: &str) -> String {
        format!(
            "Use this private link to verify your account and accept the invitation for the approved customer document.\n\n{link}\n\nThe link expires at {}. After verifying your email, reopen this invitation link and sign in to accept it.",
            iso_millis(self.token_expires_at)
        )
    }
}

/// `betaEmailBody(subject, url)` from the retained neutral email owner.
pub fn beta_email_body(subject: &str, url: &str) -> String {
    format!("{subject}\n\n{url}\n\nIf you did not request this, you can ignore this email.")
}

/// Retained safe delivery codes are persisted; arbitrary provider or
/// error text never reaches the invitation row.
pub fn safe_delivery_code(code: &str) -> &'static str {
    match code {
        EMAIL_NOT_CONFIGURED => EMAIL_NOT_CONFIGURED,
        EMAIL_SANDBOX_NOT_CONFIGURED => EMAIL_SANDBOX_NOT_CONFIGURED,
        EMAIL_SANDBOX_FORBIDDEN_IN_PRODUCTION => EMAIL_SANDBOX_FORBIDDEN_IN_PRODUCTION,
        EMAIL_SANDBOX_RECEIPT_MISSING => EMAIL_SANDBOX_RECEIPT_MISSING,
        EMAIL_SENSITIVE_RECEIPT_INVALID => EMAIL_SENSITIVE_RECEIPT_INVALID,
        EMAIL_IDEMPOTENCY_CONFLICT => EMAIL_IDEMPOTENCY_CONFLICT,
        PUBLIC_ORIGIN_NOT_CONFIGURED => PUBLIC_ORIGIN_NOT_CONFIGURED,
        SIGNING_SECRET_UNAVAILABLE => SIGNING_SECRET_UNAVAILABLE,
        SIGNING_SECRET_CHANGED => SIGNING_SECRET_CHANGED,
        _ => EMAIL_DELIVERY_FAILED,
    }
}

/// ECMAScript `Date.prototype.toISOString()` for a millisecond timestamp:
/// `YYYY-MM-DDTHH:MM:SS.sssZ` in UTC, without a leap-second table.
pub fn iso_millis(millis: i64) -> String {
    let mut seconds = millis.div_euclid(1000);
    let millis_part = millis.rem_euclid(1000);
    let days = seconds.div_euclid(86_400);
    seconds = seconds.rem_euclid(86_400);
    let (hour, minute, second) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    // Howard Hinnant's civil-from-days, exact for the proleptic Gregorian range.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis_part:03}Z")
}

/// Exact Account origin for enrollment links.
///
/// Mirrors `publicSurfaceOrigin("account", BETTER_AUTH_URL)` →
/// `configuredProductOrigin('account') ?? legacy`: a configured product host is
/// returned only for the complete development product-routing profile, and the
/// central development Account host is the public or contributor constant. No
/// caller-supplied value is trusted.
#[allow(clippy::too_many_arguments)]
pub fn resolve_account_origin(
    product_routing: bool,
    product_sso: bool,
    product_development: bool,
    local_environment: bool,
    holding_routing: bool,
    contributor_profile: bool,
    legacy: Option<&str>,
) -> Option<String> {
    if product_routing && product_sso && product_development && local_environment {
        return Some(
            if holding_routing && contributor_profile {
                "https://account.erpx.localhost:3443"
            } else {
                "https://account-dev-erpx.jesadakorn.com"
            }
            .to_owned(),
        );
    }
    legacy
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub fn origin_not_configured() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        PUBLIC_ORIGIN_NOT_CONFIGURED,
        "The Account origin for enrollment links is not configured.",
    )
}

pub fn signing_secret_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        SIGNING_SECRET_UNAVAILABLE,
        "The invitation signing secret is unavailable.",
    )
}

pub fn signing_secret_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        SIGNING_SECRET_CHANGED,
        "The invitation signing secret changed; the stored digest no longer matches.",
    )
}
