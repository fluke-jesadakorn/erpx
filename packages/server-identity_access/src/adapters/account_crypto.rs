use crate::{
    application::account_ports::{AccountFuture, AuthenticationSecrets, PasswordHashing},
    domain::account_auth::{OpaqueSecret, Password, PasswordHash, SecretDigest, SecretPurpose},
};
use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{PasswordHasher, PasswordVerifier},
};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, TimestampMillis};
use sha2::{Digest, Sha256};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::Semaphore;

const PASSWORD_PARAMETERS: &str = "$argon2id$v=19$m=65536,t=3,p=1$";
fn unavailable(code: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        code,
        "Account authentication is temporarily unavailable.",
    )
}
fn algorithm() -> Argon2<'static> {
    Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(65536, 3, 1, Some(32)).expect("fixed Argon2 parameters are valid"),
    )
}

pub struct SystemAuthenticationSecrets;
impl AuthenticationSecrets for SystemAuthenticationSecrets {
    fn issue(&self) -> Result<OpaqueSecret, BusinessError> {
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes).map_err(|_| unavailable("SECURE_RANDOM_UNAVAILABLE"))?;
        OpaqueSecret::new(hex::encode(bytes))
    }
    fn digest(&self, purpose: SecretPurpose, secret: &OpaqueSecret) -> SecretDigest {
        let label = match purpose {
            SecretPurpose::Session => "session",
            SecretPurpose::DeviceBrowser => "device",
            SecretPurpose::Reservation => "reservation",
            SecretPurpose::EmailVerification => "email-verification",
            SecretPurpose::PasswordReset => "password-reset",
            SecretPurpose::Invitation => "invitation",
        };
        let mut hash = Sha256::new();
        hash.update(b"erpx-account-v1\0");
        hash.update(label.as_bytes());
        hash.update([0]);
        hash.update(secret.expose().as_bytes());
        SecretDigest::new(hex::encode(hash.finalize()))
            .expect("SHA-256 has a fixed lowercase hexadecimal representation")
    }
    fn identifier(&self) -> Result<String, BusinessError> {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes).map_err(|_| unavailable("SECURE_RANDOM_UNAVAILABLE"))?;
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        let value = hex::encode(bytes);
        Ok(format!(
            "{}-{}-{}-{}-{}",
            &value[..8],
            &value[8..12],
            &value[12..16],
            &value[16..20],
            &value[20..]
        ))
    }
}

pub struct SystemAccountClock;
impl Clock for SystemAccountClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .try_into()
                .unwrap_or(i64::MAX),
        )
    }
}

/// Memory-hard work is bounded and runs outside async executor threads and database leases.
pub struct Argon2Passwords {
    permits: Arc<Semaphore>,
    dummy: PasswordHash,
}
impl Argon2Passwords {
    pub fn new(maximum_parallel: usize) -> Result<Self, BusinessError> {
        if !(1..=8).contains(&maximum_parallel) {
            return Err(BusinessError::invalid(
                "INVALID_PASSWORD_WORK_LIMIT",
                "Choose one to eight password workers.",
            ));
        }
        let dummy = SystemAuthenticationSecrets.issue()?;
        let encoded = algorithm()
            .hash_password(dummy.expose().as_bytes())
            .map_err(|_| unavailable("PASSWORD_HASH_UNAVAILABLE"))?
            .to_string();
        Ok(Self {
            permits: Arc::new(Semaphore::new(maximum_parallel)),
            dummy: PasswordHash::new(encoded)?,
        })
    }
}
impl PasswordHashing for Argon2Passwords {
    fn hash(&self, password: Password) -> AccountFuture<'_, PasswordHash> {
        Box::pin(async move {
            let permit = self
                .permits
                .clone()
                .try_acquire_owned()
                .map_err(|_| unavailable("PASSWORD_WORK_LIMIT"))?;
            tokio::task::spawn_blocking(move || {
                let _permit = permit;
                let encoded = algorithm()
                    .hash_password(password.as_bytes())
                    .map_err(|_| unavailable("PASSWORD_HASH_UNAVAILABLE"))?
                    .to_string();
                PasswordHash::new(encoded)
            })
            .await
            .map_err(|_| unavailable("PASSWORD_WORK_UNAVAILABLE"))?
        })
    }
    fn verify(
        &self,
        password: Password,
        expected: Option<PasswordHash>,
    ) -> AccountFuture<'_, bool> {
        let supplied = expected.is_some();
        let expected = expected.unwrap_or_else(|| self.dummy.clone());
        Box::pin(async move {
            if !expected.as_str().starts_with(PASSWORD_PARAMETERS) {
                return Err(unavailable("CREDENTIAL_PARAMETERS_UNAVAILABLE"));
            }
            let permit = self
                .permits
                .clone()
                .try_acquire_owned()
                .map_err(|_| unavailable("PASSWORD_WORK_LIMIT"))?;
            tokio::task::spawn_blocking(move || {
                let _permit = permit;
                Ok(algorithm()
                    .verify_password(password.as_bytes(), expected.as_str())
                    .is_ok()
                    && supplied)
            })
            .await
            .map_err(|_| unavailable("PASSWORD_WORK_UNAVAILABLE"))?
        })
    }
}
