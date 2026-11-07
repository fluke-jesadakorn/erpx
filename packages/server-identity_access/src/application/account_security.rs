use super::{
    account_auth::PresentedSecrets,
    account_ports::{AccountFuture, AuthenticationSecrets, DeviceProof},
};
use crate::domain::account_security::{RevokeSecurity, RevokedSecurity, SecuritySeed};
use crate::domain::{
    account_auth::{AccountPolicy, RequestAudience},
    account_mfa::{MfaEnrollment, MfaVerified},
};
use erpx_server_shared::public::BusinessError;
use std::sync::Arc;

pub trait AccountMfaRepository: Send + Sync {
    fn enroll<'a>(
        &'a self,
        proof: DeviceProof,
        audience: RequestAudience,
    ) -> AccountFuture<'a, MfaEnrollment>;
    fn verify<'a>(
        &'a self,
        proof: DeviceProof,
        audience: RequestAudience,
        code: String,
    ) -> AccountFuture<'a, MfaVerified>;
}
pub trait AccountSecurityRepository: Send + Sync {
    fn read<'a>(&'a self, proof: DeviceProof) -> AccountFuture<'a, SecuritySeed>;
    fn revoke<'a>(
        &'a self,
        proof: DeviceProof,
        input: RevokeSecurity,
        key: String,
    ) -> AccountFuture<'a, RevokedSecurity>;
}
/// Personal authentication operations; an HTTP body never selects the actor.
pub struct AccountSecurity {
    pub(crate) policy: AccountPolicy,
    pub(crate) mfa: Arc<dyn AccountMfaRepository>,
    pub(crate) secrets: Arc<dyn AuthenticationSecrets>,
    repository: Arc<dyn AccountSecurityRepository>,
}
impl AccountSecurity {
    pub fn new(
        policy: AccountPolicy,
        mfa: Arc<dyn AccountMfaRepository>,
        repository: Arc<dyn AccountSecurityRepository>,
        secrets: Arc<dyn AuthenticationSecrets>,
    ) -> Result<Self, BusinessError> {
        policy.validate()?;
        Ok(Self {
            policy,
            mfa,
            repository,
            secrets,
        })
    }
    pub async fn enroll(
        &self,
        proof: &PresentedSecrets,
        audience: RequestAudience,
    ) -> Result<MfaEnrollment, BusinessError> {
        self.mfa
            .enroll(proof.proof(self.secrets.as_ref()), audience)
            .await
    }
    pub async fn verify(
        &self,
        proof: &PresentedSecrets,
        audience: RequestAudience,
        code: String,
    ) -> Result<MfaVerified, BusinessError> {
        self.mfa
            .verify(proof.proof(self.secrets.as_ref()), audience, code)
            .await
    }
    pub async fn read(&self, proof: &PresentedSecrets) -> Result<SecuritySeed, BusinessError> {
        self.repository
            .read(proof.proof(self.secrets.as_ref()))
            .await
    }
    pub async fn revoke(
        &self,
        proof: &PresentedSecrets,
        input: RevokeSecurity,
        key: String,
    ) -> Result<RevokedSecurity, BusinessError> {
        if !(8..=128).contains(&key.len())
            || !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'-'))
        {
            return Err(BusinessError::invalid(
                "IDEMPOTENCY_KEY_REQUIRED",
                "Keep the original security request key when retrying.",
            ));
        }
        self.repository
            .revoke(proof.proof(self.secrets.as_ref()), input.validate()?, key)
            .await
    }
}
