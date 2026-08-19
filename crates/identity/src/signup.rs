//! Public signup invariants shared by HTTP and persistence adapters.
//!
//! Challenge material is returned only to a mail adapter and is represented by
//! a redacting type. Durable stores persist `secret_digest`, never the secret.

use chrono::{DateTime, Duration, Utc};
use erpx_domain::contracts::HoldingId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use thiserror::Error;
use uuid::Uuid;

pub const SIGNUP_CHALLENGE_LIFETIME: Duration = Duration::minutes(15);
pub const SIGNUP_RATE_LIMIT_WINDOW: Duration = Duration::minutes(15);
pub const SIGNUP_RATE_LIMIT_ATTEMPTS: u8 = 3;

#[derive(Clone, PartialEq, Eq)]
pub struct SignupChallengeSecret(String);

impl SignupChallengeSecret {
    pub fn new(value: impl Into<String>) -> Result<Self, SignupError> {
        let value = value.into();
        if !(32..=512).contains(&value.len()) || value.chars().any(char::is_whitespace) {
            return Err(SignupError::InvalidSecret);
        }
        Ok(Self(value))
    }

    pub fn expose_to_mail_adapter(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SignupChallengeSecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SignupChallengeSecret(<redacted>)")
    }
}

#[derive(Debug)]
pub struct SignupEmailDelivery<'a> {
    pub email: &'a str,
    pub secret: &'a SignupChallengeSecret,
    pub expires_at: DateTime<Utc>,
}

/// Production and local adapters implement the same secret-safe boundary.
/// Implementations must not log or persist `delivery.secret`.
pub trait SignupMailAdapter: Send + Sync {
    fn deliver(&self, delivery: SignupEmailDelivery<'_>) -> Result<(), SignupMailError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("signup email delivery failed")]
pub struct SignupMailError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignupEmailChallenge {
    pub id: Uuid,
    email_digest: [u8; 32],
    secret_digest: [u8; 32],
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
}

impl SignupEmailChallenge {
    pub fn issue(
        id: Uuid,
        email: &str,
        secret: &SignupChallengeSecret,
        at: DateTime<Utc>,
    ) -> Result<Self, SignupError> {
        if id.is_nil() {
            return Err(SignupError::NilIdentifier);
        }
        let normalized_email = normalize_email(email)?;
        Ok(Self {
            id,
            email_digest: digest(normalized_email.as_bytes()),
            secret_digest: digest(secret.0.as_bytes()),
            issued_at: at,
            expires_at: at
                .checked_add_signed(SIGNUP_CHALLENGE_LIFETIME)
                .ok_or(SignupError::TimestampOverflow)?,
            consumed_at: None,
        })
    }

    pub fn consume(
        &mut self,
        email: &str,
        secret: &SignupChallengeSecret,
        at: DateTime<Utc>,
    ) -> Result<(), SignupError> {
        if self.consumed_at.is_some() {
            return Err(SignupError::AlreadyConsumed);
        }
        if at < self.issued_at || at >= self.expires_at {
            return Err(SignupError::Expired);
        }
        let normalized_email = normalize_email(email)?;
        if self
            .email_digest
            .ct_eq(&digest(normalized_email.as_bytes()))
            .unwrap_u8()
            != 1
            || self
                .secret_digest
                .ct_eq(&digest(secret.0.as_bytes()))
                .unwrap_u8()
                != 1
        {
            return Err(SignupError::Mismatch);
        }
        self.consumed_at = Some(at);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignupRateWindow {
    started_at: DateTime<Utc>,
    attempts: u8,
}

impl SignupRateWindow {
    pub const fn new(at: DateTime<Utc>) -> Self {
        Self {
            started_at: at,
            attempts: 0,
        }
    }

    pub fn admit(&mut self, at: DateTime<Utc>) -> Result<(), SignupError> {
        if at < self.started_at {
            return Err(SignupError::ClockMovedBackwards);
        }
        if at >= self.started_at + SIGNUP_RATE_LIMIT_WINDOW {
            self.started_at = at;
            self.attempts = 0;
        }
        if self.attempts >= SIGNUP_RATE_LIMIT_ATTEMPTS {
            return Err(SignupError::RateLimited);
        }
        self.attempts += 1;
        Ok(())
    }
}

/// Idempotency fence for assigning exactly one initial owner to a holding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitialOwnerAssignment {
    pub holding_id: HoldingId,
    pub principal_id: Uuid,
    pub assigned_at: DateTime<Utc>,
}

impl InitialOwnerAssignment {
    pub fn assign(
        existing: Option<&Self>,
        holding_id: HoldingId,
        principal_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<Self, SignupError> {
        if principal_id.is_nil() {
            return Err(SignupError::NilIdentifier);
        }
        if let Some(existing) = existing {
            return if existing.holding_id == holding_id && existing.principal_id == principal_id {
                Ok(existing.clone())
            } else {
                Err(SignupError::OwnerAlreadyAssigned)
            };
        }
        Ok(Self {
            holding_id,
            principal_id,
            assigned_at: at,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SignupError {
    #[error("signup identifier must not be nil")]
    NilIdentifier,
    #[error("signup email is invalid")]
    InvalidEmail,
    #[error("signup challenge secret is invalid")]
    InvalidSecret,
    #[error("signup challenge timestamp overflow")]
    TimestampOverflow,
    #[error("signup challenge has expired")]
    Expired,
    #[error("signup challenge has already been consumed")]
    AlreadyConsumed,
    #[error("signup challenge does not match")]
    Mismatch,
    #[error("signup clock moved backwards")]
    ClockMovedBackwards,
    #[error("signup rate limit exceeded")]
    RateLimited,
    #[error("initial holding owner has already been assigned")]
    OwnerAlreadyAssigned,
}

fn normalize_email(value: &str) -> Result<String, SignupError> {
    let value = value.trim().to_ascii_lowercase();
    let Some((local, domain)) = value.split_once('@') else {
        return Err(SignupError::InvalidEmail);
    };
    if value.len() > 254
        || local.is_empty()
        || domain.is_empty()
        || domain.starts_with('.')
        || domain.ends_with('.')
        || value.chars().any(char::is_whitespace)
    {
        return Err(SignupError::InvalidEmail);
    }
    Ok(value)
}

fn digest(value: &[u8]) -> [u8; 32] {
    Sha256::digest(value).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(value: char) -> SignupChallengeSecret {
        SignupChallengeSecret::new(value.to_string().repeat(32)).unwrap()
    }

    #[test]
    fn challenge_is_single_use_and_expires_at_exactly_fifteen_minutes() {
        let at = Utc::now();
        let mut challenge =
            SignupEmailChallenge::issue(Uuid::new_v4(), "Owner@Example.test", &secret('a'), at)
                .unwrap();
        assert_eq!(challenge.expires_at, at + SIGNUP_CHALLENGE_LIFETIME);
        assert_eq!(
            challenge
                .clone()
                .consume("owner@example.test", &secret('a'), challenge.expires_at),
            Err(SignupError::Expired)
        );
        challenge
            .consume("owner@example.test", &secret('a'), at)
            .unwrap();
        assert_eq!(
            challenge.consume("owner@example.test", &secret('a'), at),
            Err(SignupError::AlreadyConsumed)
        );
    }

    #[test]
    fn challenge_and_delivery_debug_never_expose_secret_material() {
        let value = secret('z');
        let challenge =
            SignupEmailChallenge::issue(Uuid::new_v4(), "owner@example.test", &value, Utc::now())
                .unwrap();
        let debug = format!("{value:?} {challenge:?}");
        assert!(!debug.contains(value.expose_to_mail_adapter()));
    }

    #[test]
    fn rate_limit_and_owner_assignment_are_bounded_and_idempotent() {
        let at = Utc::now();
        let mut window = SignupRateWindow::new(at);
        for _ in 0..SIGNUP_RATE_LIMIT_ATTEMPTS {
            window.admit(at).unwrap();
        }
        assert_eq!(window.admit(at), Err(SignupError::RateLimited));
        window.admit(at + SIGNUP_RATE_LIMIT_WINDOW).unwrap();

        let holding_id = HoldingId::try_from_uuid(Uuid::new_v4()).unwrap();
        let principal_id = Uuid::new_v4();
        let first = InitialOwnerAssignment::assign(None, holding_id, principal_id, at).unwrap();
        assert_eq!(
            InitialOwnerAssignment::assign(Some(&first), holding_id, principal_id, at).unwrap(),
            first
        );
        assert_eq!(
            InitialOwnerAssignment::assign(Some(&first), holding_id, Uuid::new_v4(), at),
            Err(SignupError::OwnerAlreadyAssigned)
        );
    }
}
