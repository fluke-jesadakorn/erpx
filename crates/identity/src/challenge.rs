use std::fmt;

use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::{IdentityError, SecretBytes, SecretValue};

pub const MAX_CHALLENGE_LIFETIME_SECONDS: i64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChallengeCeremony {
    Registration,
    Assertion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChallengeLifetime(Duration);

impl ChallengeLifetime {
    pub fn try_new(duration: Duration) -> Result<Self, IdentityError> {
        if duration <= Duration::zero() {
            return Err(IdentityError::InvalidChallengeLifetime);
        }
        if duration > Duration::seconds(MAX_CHALLENGE_LIFETIME_SECONDS) {
            return Err(IdentityError::ChallengeLifetimeExceeded);
        }
        Ok(Self(duration))
    }

    pub const fn duration(self) -> Duration {
        self.0
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ChallengeMaterial(SecretBytes);

impl ChallengeMaterial {
    pub(crate) fn from_slice(value: &[u8]) -> Self {
        Self(SecretBytes::from_slice(value))
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl fmt::Debug for ChallengeMaterial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ChallengeMaterial(REDACTED)")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebAuthnChallenge {
    pub id: Uuid,
    pub principal_ref: String,
    pub ceremony: ChallengeCeremony,
    digest: [u8; 32],
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    consumed: bool,
}

impl WebAuthnChallenge {
    pub fn issue(
        principal_ref: impl Into<String>,
        ceremony: ChallengeCeremony,
        issued_at: DateTime<Utc>,
        lifetime: ChallengeLifetime,
    ) -> Result<(Self, ChallengeMaterial), IdentityError> {
        let principal_ref = principal_ref.into();
        if principal_ref.trim().is_empty() {
            return Err(IdentityError::InvalidIdentityReference);
        }
        let material = SecretValue::random(32)?;
        let mut digest = Sha256::new();
        digest.update(material.as_str().as_bytes());
        let expires_at = issued_at
            .checked_add_signed(lifetime.duration())
            .ok_or(IdentityError::MalformedChallenge)?;
        Ok((
            Self {
                id: Uuid::new_v4(),
                principal_ref,
                ceremony,
                digest: digest.finalize().into(),
                issued_at,
                expires_at,
                consumed: false,
            },
            ChallengeMaterial(SecretBytes::from_slice(material.as_str().as_bytes())),
        ))
    }

    pub fn consume_for(
        &self,
        principal_ref: &str,
        ceremony: ChallengeCeremony,
        material: &[u8],
        at: DateTime<Utc>,
    ) -> Result<Self, IdentityError> {
        if self.principal_ref != principal_ref {
            return Err(IdentityError::ChallengeMismatch);
        }
        if self.ceremony != ceremony {
            return Err(IdentityError::CeremonyMismatch);
        }
        self.consume(material, at)
    }

    pub(crate) fn from_persisted(
        id: Uuid,
        principal_ref: String,
        ceremony: ChallengeCeremony,
        digest: [u8; 32],
        issued_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        consumed: bool,
    ) -> Result<Self, IdentityError> {
        if principal_ref.trim().is_empty() || expires_at <= issued_at {
            return Err(IdentityError::MalformedChallenge);
        }
        Ok(Self {
            id,
            principal_ref,
            ceremony,
            digest,
            issued_at,
            expires_at,
            consumed,
        })
    }

    pub(crate) fn digest(&self) -> &[u8; 32] {
        &self.digest
    }

    pub fn consume(&self, material: &[u8], at: DateTime<Utc>) -> Result<Self, IdentityError> {
        if self.expires_at <= self.issued_at {
            return Err(IdentityError::MalformedChallenge);
        }
        if at < self.issued_at {
            return Err(IdentityError::ChallengeNotYetValid);
        }
        if at >= self.expires_at {
            return Err(IdentityError::ChallengeExpired);
        }
        if self.consumed {
            return Err(IdentityError::ChallengeReplay);
        }
        let mut digest = Sha256::new();
        digest.update(material);
        let digest: [u8; 32] = digest.finalize().into();
        if !bool::from(digest.ct_eq(&self.digest)) {
            return Err(IdentityError::ChallengeMismatch);
        }
        let mut next = self.clone();
        next.consumed = true;
        Ok(next)
    }

    pub fn is_consumed(&self) -> bool {
        self.consumed
    }
}
