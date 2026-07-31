use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use zeroize::Zeroize;

use crate::error::IdentityError;
use rand_core::{OsRng, RngCore};

/// A secret string whose debug representation and drop behavior do not leak
/// the bearer value.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretValue(String);

impl SecretValue {
    pub(crate) fn random(bytes: usize) -> Result<Self, IdentityError> {
        if bytes == 0 {
            return Err(IdentityError::RandomnessUnavailable);
        }
        let mut value = vec![0_u8; bytes];
        OsRng.fill_bytes(&mut value);
        Ok(Self(URL_SAFE_NO_PAD.encode(value)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretValue(REDACTED)")
    }
}

impl Drop for SecretValue {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// Secret bytes are returned only for ephemeral use by an authentication
/// adapter. They are never part of a persisted envelope.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretBytes(Vec<u8>);

impl SecretBytes {
    pub fn from_slice(value: &[u8]) -> Self {
        Self(value.to_vec())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretBytes(REDACTED)")
    }
}

impl Drop for SecretBytes {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// SHA-256 is appropriate for these values because the inputs are generated
/// with 256 bits of entropy. The raw token is never stored in this type.
#[derive(Clone, PartialEq, Eq)]
pub struct TokenHash([u8; 32]);

impl TokenHash {
    pub(crate) fn of(value: &str) -> Self {
        let mut digest = Sha256::new();
        digest.update(value.as_bytes());
        Self(digest.finalize().into())
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn from_bytes(value: &[u8]) -> Result<Self, IdentityError> {
        let value: [u8; 32] = value
            .try_into()
            .map_err(|_| IdentityError::InvalidRefreshToken)?;
        Ok(Self(value))
    }

    pub(crate) fn equals_constant_time(&self, other: &Self) -> bool {
        self.0.ct_eq(&other.0).into()
    }
}

impl fmt::Debug for TokenHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TokenHash(REDACTED)")
    }
}

impl serde::Serialize for TokenHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for TokenHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        let array: [u8; 32] = bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("token hash must be 32 bytes"))?;
        Ok(Self(array))
    }
}
