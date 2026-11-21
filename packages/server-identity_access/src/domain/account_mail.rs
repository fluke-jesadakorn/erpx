use crate::domain::{
    account_auth::{AccountPolicy, EmailAddress, OpaqueSecret},
    account_credentials::{CredentialProofKind, account_return_path},
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sha2::{Digest, Sha256};

pub const ACCOUNT_MAIL_MAX_ATTEMPTS: i64 = 6;
pub const ACCOUNT_MAIL_LEASE_MS: i64 = 30_000;
pub const ACCOUNT_MAIL_PROVIDER_TIMEOUT_MS: i64 = 10_000;
pub const RESEND_IDEMPOTENCY_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;
pub struct CurrentAccountMailProof {
    pub kind: CredentialProofKind,
    pub created_at: i64,
    pub expires_at: i64,
    pub consumed: bool,
    pub identity_current: bool,
    pub credential_current: bool,
    pub recipient_current: bool,
    pub email_verified: bool,
}
pub fn account_mail_proof_available(facts: &CurrentAccountMailProof, now: i64) -> bool {
    facts.created_at <= now
        && facts.expires_at > now
        && facts
            .created_at
            .checked_add(crate::domain::account_credentials::CREDENTIAL_PROOF_LIFETIME_MS)
            .is_some_and(|until| facts.expires_at <= until)
        && !facts.consumed
        && facts.identity_current
        && facts.credential_current
        && facts.recipient_current
        && (facts.kind != CredentialProofKind::EmailVerification || !facts.email_verified)
}

/// Decrypted private payload; never a wire DTO, log field or debug printable value.
pub struct OpenedAccountMail {
    pub kind: CredentialProofKind,
    pub account_origin: String,
    pub return_path: String,
    pub token: OpaqueSecret,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountMailProviderKind {
    Resend,
    Smtp,
}
impl AccountMailProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Resend => "resend",
            Self::Smtp => "smtp",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountMailFailure {
    NotConfigured,
    InvalidPayload,
    ProofUnavailable,
    ProviderRejected,
    ProviderRateLimited,
    ProviderUnavailable,
    DeliveryUncertain,
    MessageChanged,
}
impl AccountMailFailure {
    pub fn code(self) -> &'static str {
        match self {
            Self::NotConfigured => "EMAIL_NOT_CONFIGURED",
            Self::InvalidPayload => "ACCOUNT_MAIL_PAYLOAD_INVALID",
            Self::ProofUnavailable => "ACCOUNT_MAIL_PROOF_UNAVAILABLE",
            Self::ProviderRejected => "EMAIL_PROVIDER_REJECTED",
            Self::ProviderRateLimited => "EMAIL_PROVIDER_RATE_LIMITED",
            Self::ProviderUnavailable => "EMAIL_PROVIDER_UNAVAILABLE",
            Self::DeliveryUncertain => "EMAIL_DELIVERY_UNCERTAIN",
            Self::MessageChanged => "EMAIL_IDEMPOTENCY_CONFLICT",
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AccountMailSendOutcome {
    Accepted { provider_id: String },
    Rejected(AccountMailFailure),
    Retryable(AccountMailFailure),
    Uncertain(AccountMailFailure),
}
pub fn account_mail_next_attempt(attempts: i64, now: i64) -> Option<i64> {
    if !(1..ACCOUNT_MAIL_MAX_ATTEMPTS).contains(&attempts) {
        return None;
    }
    now.checked_add((30_000_i64.checked_mul(1_i64.checked_shl(attempts as u32)?)?).min(3_600_000))
}
pub fn account_mail_can_retry_unknown(
    provider: AccountMailProviderKind,
    started_at: i64,
    expires_at: i64,
    now: i64,
) -> bool {
    provider == AccountMailProviderKind::Resend
        && now >= started_at
        && now < expires_at
        && started_at
            .checked_add(RESEND_IDEMPOTENCY_WINDOW_MS)
            .is_some_and(|until| now < until)
}
pub fn account_mail_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ACCOUNT_MAIL_UNAVAILABLE",
        "Account email delivery is unavailable.",
    )
}

pub struct AccountMailPolicy {
    account: AccountPolicy,
}
impl AccountMailPolicy {
    pub fn new(account: AccountPolicy) -> Result<Self, BusinessError> {
        account.validate()?;
        Ok(Self { account })
    }
    pub fn account_origin(&self) -> &str {
        &self.account.account_origin
    }
    pub fn render(
        &self,
        recipient: EmailAddress,
        payload: &OpenedAccountMail,
        outbox_id: &str,
    ) -> Result<AccountEmailMessage, BusinessError> {
        if payload.account_origin != self.account.account_origin
            || outbox_id.is_empty()
            || outbox_id.len() > 128
            || !outbox_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        {
            return Err(account_mail_unavailable());
        }
        let return_path = account_return_path(Some(&payload.return_path))?;
        let (subject, path) = match payload.kind {
            CredentialProofKind::EmailVerification => (
                "Verify your ERPX email",
                format!(
                    "/api/auth/verify-email?token={}&callbackURL={}",
                    component(payload.token.expose()),
                    component(&return_path)
                ),
            ),
            CredentialProofKind::PasswordReset => (
                "Reset your ERPX password",
                format!(
                    "/api/auth/reset-password/{}?callbackURL={}",
                    component(payload.token.expose()),
                    component(&return_path)
                ),
            ),
        };
        let url = format!("{}{path}", self.account.account_origin);
        let parsed = url::Url::parse(&url).map_err(|_| account_mail_unavailable())?;
        if parsed.origin().ascii_serialization() != self.account.account_origin
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.fragment().is_some()
        {
            return Err(account_mail_unavailable());
        }
        let body = format!(
            "{subject}\n\n{url}\n\nIf you did not request this, you can ignore this email."
        );
        Ok(AccountEmailMessage {
            recipient,
            subject: subject.into(),
            body,
            delivery_key: format!("erpx-account-mail/{outbox_id}"),
        })
    }
}
/// Private provider input. Accessors are for injected transports, never JSON responses.
pub struct AccountEmailMessage {
    recipient: EmailAddress,
    subject: String,
    body: String,
    delivery_key: String,
}
/// Non-secret provider identity binds retries to the same sender and transport.
pub struct AccountMailProviderProfile {
    pub kind: AccountMailProviderKind,
    pub profile_id: String,
    pub from: String,
}
impl AccountMailProviderProfile {
    pub fn fingerprint(&self, message: &AccountEmailMessage) -> Result<String, BusinessError> {
        if self.profile_id.is_empty()
            || self.profile_id.len() > 100
            || !self
                .profile_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            || self.from.is_empty()
            || self.from.len() > 998
            || self.from.chars().any(|c| c.is_control())
        {
            return Err(account_mail_unavailable());
        }
        let mut digest = Sha256::new();
        for value in [
            "erpx-account-mail-message-v1",
            self.kind.as_str(),
            &self.profile_id,
            &self.from,
            message.recipient(),
            message.subject(),
            message.body(),
            message.delivery_key(),
        ] {
            digest.update(
                u64::try_from(value.len())
                    .map_err(|_| account_mail_unavailable())?
                    .to_be_bytes(),
            );
            digest.update(value.as_bytes());
        }
        Ok(hex::encode(digest.finalize()))
    }
}
impl AccountEmailMessage {
    pub fn recipient(&self) -> &str {
        self.recipient.as_str()
    }
    pub fn subject(&self) -> &str {
        &self.subject
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn delivery_key(&self) -> &str {
        &self.delivery_key
    }
}
// Match the current JavaScript encodeURIComponent wire, including its exact
// callbackURL punctuation rules; form-urlencoding would encode ~ and use +.
fn component(value: &str) -> String {
    let mut result = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
            )
        {
            result.push(byte as char);
        } else {
            result.push_str(&format!("%{byte:02X}"));
        }
    }
    result
}
