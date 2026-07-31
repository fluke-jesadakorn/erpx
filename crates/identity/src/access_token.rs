use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::IdentityError;

pub const MAX_ACCESS_TOKEN_LIFETIME_SECONDS: i64 = 300;
type TokenMac = Hmac<Sha256>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessTokenLifetime(Duration);

impl AccessTokenLifetime {
    pub fn try_new(duration: Duration) -> Result<Self, IdentityError> {
        if duration <= Duration::zero() {
            return Err(IdentityError::InvalidAccessTokenLifetime);
        }
        if duration > Duration::seconds(MAX_ACCESS_TOKEN_LIFETIME_SECONDS) {
            return Err(IdentityError::AccessTokenLifetimeExceeded);
        }
        Ok(Self(duration))
    }

    pub const fn five_minutes() -> Self {
        Self(Duration::seconds(MAX_ACCESS_TOKEN_LIFETIME_SECONDS))
    }

    pub const fn duration(self) -> Duration {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessTokenClaims {
    pub principal_ref: String,
    pub session_id: Uuid,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct AccessToken {
    token: String,
    claims: AccessTokenClaims,
}

impl AccessToken {
    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn claims(&self) -> &AccessTokenClaims {
        &self.claims
    }
}

impl fmt::Debug for AccessToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AccessToken")
            .field("token", &"REDACTED")
            .field("claims", &self.claims)
            .finish()
    }
}

#[derive(Clone)]
pub struct AccessTokenSigner {
    key: Vec<u8>,
}

impl fmt::Debug for AccessTokenSigner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AccessTokenSigner(REDACTED)")
    }
}

impl AccessTokenSigner {
    pub fn new(key: &[u8]) -> Result<Self, IdentityError> {
        if key.len() < 32 {
            return Err(IdentityError::InvalidSigningKey);
        }
        Ok(Self { key: key.to_vec() })
    }

    pub fn issue(
        &self,
        principal_ref: impl Into<String>,
        session_id: Uuid,
        issued_at: DateTime<Utc>,
        lifetime: AccessTokenLifetime,
    ) -> Result<AccessToken, IdentityError> {
        let principal_ref = principal_ref.into();
        if principal_ref.trim().is_empty() {
            return Err(IdentityError::InvalidIdentityReference);
        }
        let claims = AccessTokenClaims {
            principal_ref,
            session_id,
            issued_at,
            expires_at: issued_at + lifetime.duration(),
        };
        let encoded_claims = URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&claims).map_err(|_| IdentityError::MalformedAccessToken)?);
        let signature = self.sign(&encoded_claims)?;
        Ok(AccessToken {
            token: format!("{encoded_claims}.{}", URL_SAFE_NO_PAD.encode(signature)),
            claims,
        })
    }

    pub fn verify(
        &self,
        token: &str,
        at: DateTime<Utc>,
    ) -> Result<AccessTokenClaims, IdentityError> {
        let (encoded_claims, encoded_signature) = token
            .split_once('.')
            .ok_or(IdentityError::MalformedAccessToken)?;
        let provided = URL_SAFE_NO_PAD
            .decode(encoded_signature)
            .map_err(|_| IdentityError::MalformedAccessToken)?;
        let expected = self.sign(encoded_claims)?;
        if provided.len() != expected.len() || !bool::from(provided.ct_eq(&expected)) {
            return Err(IdentityError::InvalidAccessTokenSignature);
        }
        let claims_bytes = URL_SAFE_NO_PAD
            .decode(encoded_claims)
            .map_err(|_| IdentityError::MalformedAccessToken)?;
        let claims: AccessTokenClaims = serde_json::from_slice(&claims_bytes)
            .map_err(|_| IdentityError::MalformedAccessToken)?;
        if at < claims.issued_at || at >= claims.expires_at {
            return Err(IdentityError::AccessTokenExpired);
        }
        Ok(claims)
    }

    fn sign(&self, value: &str) -> Result<Vec<u8>, IdentityError> {
        let mut mac =
            TokenMac::new_from_slice(&self.key).map_err(|_| IdentityError::InvalidSigningKey)?;
        mac.update(value.as_bytes());
        Ok(mac.finalize().into_bytes().to_vec())
    }
}
