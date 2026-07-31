use std::fmt;

use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use rand_core::OsRng;

use crate::IdentityError;

const MIN_MEMORY_KIB: u32 = 19_456;
const MIN_TIME_COST: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasswordHashingConfig {
    memory_cost_kib: u32,
    time_cost: u32,
    parallelism: u32,
}

impl PasswordHashingConfig {
    pub fn new(
        memory_cost_kib: u32,
        time_cost: u32,
        parallelism: u32,
    ) -> Result<Self, IdentityError> {
        if memory_cost_kib < MIN_MEMORY_KIB || time_cost < MIN_TIME_COST || parallelism == 0 {
            return Err(IdentityError::InvalidPasswordParameters);
        }
        Ok(Self {
            memory_cost_kib,
            time_cost,
            parallelism,
        })
    }

    fn params(self) -> Result<Params, IdentityError> {
        Params::new(self.memory_cost_kib, self.time_cost, self.parallelism, None)
            .map_err(|_| IdentityError::InvalidPasswordParameters)
    }
}

impl Default for PasswordHashingConfig {
    fn default() -> Self {
        Self {
            memory_cost_kib: MIN_MEMORY_KIB,
            time_cost: MIN_TIME_COST,
            parallelism: 1,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PasswordService {
    config: PasswordHashingConfig,
}

impl PasswordService {
    pub fn new(config: PasswordHashingConfig) -> Self {
        Self { config }
    }

    pub fn hash(&self, password: &str) -> Result<PasswordDigest, IdentityError> {
        if password.is_empty() {
            return Err(IdentityError::PasswordHashingFailed);
        }
        let algorithm = Argon2::new(Algorithm::Argon2id, Version::V0x13, self.config.params()?);
        let salt = SaltString::generate(&mut OsRng);
        let digest = algorithm
            .hash_password(password.as_bytes(), &salt)
            .map_err(|_| IdentityError::PasswordHashingFailed)?
            .to_string();
        Ok(PasswordDigest(digest))
    }

    /// Argon2's password-hash verifier performs the expensive KDF and uses a
    /// constant-time comparison for the derived output.
    pub fn verify(&self, password: &str, digest: &PasswordDigest) -> Result<(), IdentityError> {
        let parsed =
            PasswordHash::new(digest.as_str()).map_err(|_| IdentityError::InvalidPasswordDigest)?;
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .map_err(|_| IdentityError::PasswordVerificationFailed)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct PasswordDigest(String);

impl PasswordDigest {
    pub fn from_stored(value: impl Into<String>) -> Result<Self, IdentityError> {
        let value = value.into();
        argon2::password_hash::PasswordHash::new(&value)
            .map_err(|_| IdentityError::InvalidPasswordDigest)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for PasswordDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PasswordDigest(REDACTED)")
    }
}
