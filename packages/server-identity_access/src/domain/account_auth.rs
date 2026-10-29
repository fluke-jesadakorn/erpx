use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeSet;

pub const SESSION_LIFETIME_MS: i64 = 12 * 60 * 60 * 1000;
pub const SIGN_IN_RESERVATION_MS: i64 = 2 * 60 * 1000;
pub const MAXIMUM_SAVED_ACCOUNTS: usize = 5;
pub const MAX_DEVICE_REVISION: i64 = 9_007_199_254_740_991;

pub fn denied() -> BusinessError {
    BusinessError::new(
        FailureKind::Unauthenticated,
        "INVALID_EMAIL_OR_PASSWORD",
        "Check your email, password or approved Directory sign-in.",
    )
}
pub fn stale_device() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "DEVICE_ACCOUNTS_CHANGED",
        "Your saved accounts changed. Review the current accounts and try again.",
    )
}

#[derive(Clone, Eq, PartialEq)]
pub struct EmailAddress(String);
impl EmailAddress {
    pub fn new(value: &str) -> Result<Self, BusinessError> {
        if !value.is_ascii() {
            return Err(BusinessError::invalid(
                "INVALID_EMAIL",
                "Enter a supported email address.",
            ));
        }
        let value = value.trim().to_lowercase();
        let mut pieces = value.split('@');
        let local = pieces.next().unwrap_or_default();
        let domain = pieces.next().unwrap_or_default();
        if value.len() > 320
            || local.is_empty()
            || local.len() > 64
            || domain.is_empty()
            || !domain.contains('.')
            || !practical_email(local, domain)
            || pieces.next().is_some()
            || value.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(BusinessError::invalid(
                "INVALID_EMAIL",
                "Enter a valid email address.",
            ));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
// Match the current installed Zod practical email grammar, with bounded address lengths.
fn practical_email(local: &str, domain: &str) -> bool {
    let local_allowed =
        |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'\'' | b'+' | b'-');
    let local_valid = local
        .split('.')
        .all(|piece| !piece.is_empty() && piece.bytes().all(local_allowed))
        && local
            .as_bytes()
            .last()
            .is_some_and(|byte| local_allowed(*byte) && *byte != b'\'');
    let mut labels = domain.rsplit('.');
    let top = labels.next().unwrap_or_default();
    let tail = labels.collect::<Vec<_>>();
    local_valid
        && top.len() >= 2
        && top.bytes().all(|byte| byte.is_ascii_alphabetic())
        && !tail.is_empty()
        && tail.iter().all(|label| {
            !label.is_empty()
                && label.as_bytes()[0].is_ascii_alphanumeric()
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

/// A password is never a public contract or a debug-printable account field.
pub struct Password(String);
impl Password {
    pub fn sign_in(value: String) -> Result<Self, BusinessError> {
        if value.is_empty() || value.len() > 4096 {
            return Err(BusinessError::invalid(
                "INVALID_PASSWORD",
                "Enter a password of at most 4096 bytes.",
            ));
        }
        Ok(Self(value))
    }
    pub fn new_credential(value: String) -> Result<Self, BusinessError> {
        // The current invitation policy uses JavaScript String.length (UTF-16).
        if value.encode_utf16().count() < 12 {
            return Err(BusinessError::invalid(
                "PASSWORD_TOO_SHORT",
                "Use at least 12 characters.",
            ));
        }
        Self::sign_in(value)
    }
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct PasswordHash(String);
impl PasswordHash {
    pub fn new(value: String) -> Result<Self, BusinessError> {
        if !value.starts_with("$argon2id$v=19$") || value.len() > 1024 {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "CREDENTIAL_FORMAT_UNAVAILABLE",
                "This account requires a supported password credential.",
            ));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The raw browser secret exists only at the adapter/application boundary.
#[derive(Clone)]
pub struct OpaqueSecret(String);
impl OpaqueSecret {
    pub fn new(value: String) -> Result<Self, BusinessError> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(BusinessError::invalid(
                "INVALID_SESSION_PROOF",
                "This session proof is invalid.",
            ));
        }
        Ok(Self(value))
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SecretDigest(String);
impl SecretDigest {
    pub fn new(value: String) -> Result<Self, BusinessError> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(BusinessError::invalid(
                "INVALID_SECRET_DIGEST",
                "The session digest is invalid.",
            ));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy)]
pub enum SecretPurpose {
    Session,
    DeviceBrowser,
    Reservation,
    EmailVerification,
    PasswordReset,
    Invitation,
}

#[derive(Clone)]
pub struct CredentialSnapshot {
    pub user_id: String,
    pub name: String,
    pub email: String,
    pub image: Option<String>,
    pub email_verified: bool,
    pub active: bool,
    pub password_hash: PasswordHash,
}
impl CredentialSnapshot {
    pub fn can_authenticate(&self, require_verification: bool) -> bool {
        self.active
            && !self.user_id.starts_with("dir_")
            && (!require_verification || self.email_verified)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountUser {
    pub id: String,
    pub name: String,
    pub email: String,
    pub email_verified: bool,
    pub image: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountSession {
    pub id: String,
    pub user: AccountUser,
    pub authenticated_at: i64,
    pub expires_at: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceAccount {
    pub handle: String,
    pub name: String,
    pub email: String,
    pub image: Option<String>,
    pub current: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceAccounts {
    pub initialized: bool,
    pub revision: i64,
    pub maximum: usize,
    pub accounts: Vec<DeviceAccount>,
}
impl DeviceAccounts {
    pub fn empty() -> Self {
        Self {
            initialized: false,
            revision: 0,
            maximum: MAXIMUM_SAVED_ACCOUNTS,
            accounts: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequestAudience {
    Account,
    Holding { audience: String },
}
#[derive(Clone)]
pub struct HoldingOrigin {
    pub origin: String,
    pub host: String,
    pub audience: String,
}
#[derive(Clone)]
pub struct AccountPolicy {
    pub account_origin: String,
    pub account_host: String,
    pub environment: String,
    pub require_email_verification: bool,
    pub holdings: Vec<HoldingOrigin>,
}
impl AccountPolicy {
    pub fn validate(&self) -> Result<(), BusinessError> {
        let mut hosts = BTreeSet::new();
        let contributor = self.account_host == "account.erpx.localhost:3443";
        if contributor
            && (self.holdings.len() != 3
                || ![
                    ("h001", "acme.erpx.localhost:3443"),
                    ("h002", "northstar.erpx.localhost:3443"),
                    ("h003", "erpx-internal.erpx.localhost:3443"),
                ]
                .iter()
                .all(|(id, host)| {
                    self.holdings.iter().any(|h| {
                        h.host == *host && h.audience == format!("holding:development:{id}:{host}")
                    })
                }))
        {
            return Err(BusinessError::invalid(
                "INVALID_HOLDING_AUTHORITY",
                "Configure the exact contributor Holding origins.",
            ));
        }
        if self.account_origin != format!("https://{}", self.account_host)
            || !(exact_host(&self.account_host) || contributor)
            || self.environment != "development"
        {
            return Err(BusinessError::invalid(
                "INVALID_ACCOUNT_AUTHORITY",
                "Configure the exact supported Account origin and environment.",
            ));
        }
        hosts.insert(self.account_host.clone());
        for holding in &self.holdings {
            let parts = holding.audience.splitn(4, ':').collect::<Vec<_>>();
            if holding.origin != format!("https://{}", holding.host)
                || !(exact_host(&holding.host)
                    || contributor && contributor_holding_host(&holding.host))
                || parts.len() != 4
                || parts[0] != "holding"
                || parts[1] != self.environment
                || parts[2].is_empty()
                || parts[3] != holding.host
                || !hosts.insert(holding.host.clone())
            {
                return Err(BusinessError::invalid(
                    "INVALID_HOLDING_AUTHORITY",
                    "Configure distinct exact Holding origins and audiences.",
                ));
            }
        }
        Ok(())
    }
    pub fn audience(&self, host: &str) -> Option<RequestAudience> {
        if host == self.account_host {
            Some(RequestAudience::Account)
        } else {
            self.holdings
                .iter()
                .find(|holding| holding.host == host)
                .map(|holding| RequestAudience::Holding {
                    audience: holding.audience.clone(),
                })
        }
    }
    pub fn device_origin_allowed(&self, origin: &str) -> bool {
        origin == self.account_origin
            || self.holdings.iter().any(|holding| holding.origin == origin)
    }
}
pub(crate) fn contributor_holding_host(host: &str) -> bool {
    matches!(
        host,
        "acme.erpx.localhost:3443"
            | "northstar.erpx.localhost:3443"
            | "erpx-internal.erpx.localhost:3443"
    )
}
fn exact_host(host: &str) -> bool {
    host.len() <= 253
        && host.contains('.')
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.bytes().all(|value| {
                    value.is_ascii_lowercase() || value.is_ascii_digit() || value == b'-'
                })
        })
}

pub fn valid_session_window(authenticated_at: i64, expires_at: i64, now: i64) -> bool {
    authenticated_at <= now
        && expires_at > now
        && expires_at
            .checked_sub(authenticated_at)
            .is_some_and(|lifetime| lifetime > 0 && lifetime <= SESSION_LIFETIME_MS)
}
