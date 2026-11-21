use crate::{
    adapters::account_crypto::SystemAuthenticationSecrets,
    application::account_ports::AuthenticationSecrets, application::account_sso::SsoCryptography,
    domain::account_sso::*,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sha2::{Digest, Sha256};
pub struct SystemSsoCryptography;
impl SsoCryptography for SystemSsoCryptography {
    fn issue(&self) -> Result<SsoSecret, BusinessError> {
        let mut value = [0; 32];
        getrandom::fill(&mut value).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "SSO_RANDOM_UNAVAILABLE",
                "Sign-in is temporarily unavailable.",
            )
        })?;
        SsoSecret::new(URL_SAFE_NO_PAD.encode(value))
    }
    fn digest(&self, value: &str) -> SsoDigest {
        SsoDigest::new(URL_SAFE_NO_PAD.encode(Sha256::digest(value.as_bytes())))
            .expect("SHA256 URL-safe digest has43ASCII characters")
    }
    fn transaction_id(&self) -> Result<SsoTransactionId, BusinessError> {
        SsoTransactionId::new(SystemAuthenticationSecrets.identifier()?)
    }
}
