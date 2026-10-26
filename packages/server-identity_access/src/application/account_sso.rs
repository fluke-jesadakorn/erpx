use super::{
    account_auth::PresentedSecrets,
    account_ports::{AccountFuture, AuthenticationSecrets, DeviceProof},
};
use crate::domain::{
    account_auth::{OpaqueSecret, SecretDigest, SecretPurpose},
    account_sso::*,
};
use erpx_server_shared::public::{BusinessError, Clock};
use std::sync::Arc;

pub trait SsoCryptography: Send + Sync {
    fn issue(&self) -> Result<SsoSecret, BusinessError>;
    fn digest(&self, value: &str) -> SsoDigest;
    fn transaction_id(&self) -> Result<SsoTransactionId, BusinessError>;
}
#[derive(Clone)]
pub struct AuthorizeSso {
    pub transaction_id: SsoTransactionId,
    pub state_hash: SsoDigest,
    pub challenge: SsoDigest,
    pub proof: DeviceProof,
    pub code_hash: SsoDigest,
    pub now: i64,
}
#[derive(Clone)]
pub struct ConsumeSso {
    pub transaction_id: SsoTransactionId,
    pub audience: String,
    pub code_hash: SsoDigest,
    pub state_hash: SsoDigest,
    pub browser_hash: SsoDigest,
    pub verifier_challenge: SsoDigest,
    pub session_id: String,
    pub session_digest: SecretDigest,
    pub now: i64,
}
pub struct SsoConsumption {
    pub session_id: String,
    pub expires_at: i64,
    pub return_path: SsoReturnPath,
}
pub trait AccountSsoRepository: Send + Sync {
    fn start(&self, input: BrowserSsoTransaction) -> AccountFuture<'_, ()>;
    fn authorize(&self, input: AuthorizeSso) -> AccountFuture<'_, Option<String>>;
    fn consume(&self, input: ConsumeSso) -> AccountFuture<'_, SsoConsumption>;
}
pub struct StartedSso {
    pub transaction_id: SsoTransactionId,
    pub state: SsoSecret,
    pub challenge: SsoDigest,
    pub browser_cookie: SsoBrowserCookie,
}
pub struct AuthorizedSso {
    pub code: SsoSecret,
    pub callback: String,
}
pub struct IssuedSso {
    pub token: OpaqueSecret,
    pub result: SsoConsumption,
}
pub struct AccountSso {
    pub(crate) policy: SsoPolicy,
    repository: Arc<dyn AccountSsoRepository>,
    cryptography: Arc<dyn SsoCryptography>,
    sessions: Arc<dyn AuthenticationSecrets>,
    clock: Arc<dyn Clock + Send + Sync>,
}
impl AccountSso {
    pub fn new(
        policy: SsoPolicy,
        repository: Arc<dyn AccountSsoRepository>,
        cryptography: Arc<dyn SsoCryptography>,
        sessions: Arc<dyn AuthenticationSecrets>,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> Self {
        Self {
            policy,
            repository,
            cryptography,
            sessions,
            clock,
        }
    }
    pub async fn start(
        &self,
        audience: &str,
        return_path: Option<&str>,
    ) -> Result<StartedSso, BusinessError> {
        let callback = self.policy.callback(audience)?;
        let transaction_id = self.cryptography.transaction_id()?;
        let browser = self.cryptography.issue()?;
        let verifier = self.cryptography.issue()?;
        let state = self.cryptography.issue()?;
        let challenge = self.cryptography.digest(verifier.expose());
        let now = self.clock.now().get();
        let expires_at = now
            .checked_add(SSO_BROWSER_LIFETIME_MS)
            .ok_or_else(rejected_sso)?;
        self.repository
            .start(BrowserSsoTransaction {
                id: transaction_id.clone(),
                browser_hash: self.cryptography.digest(browser.expose()),
                state_hash: self.cryptography.digest(state.expose()),
                challenge: challenge.clone(),
                audience: audience.into(),
                environment: self.policy.account().environment.clone(),
                callback,
                return_path: SsoReturnPath::sanitize(return_path),
                created_at: now,
                expires_at,
                consumed_at: None,
            })
            .await?;
        Ok(StartedSso {
            transaction_id: transaction_id.clone(),
            state,
            challenge,
            browser_cookie: SsoBrowserCookie {
                transaction_id,
                browser,
                verifier,
            },
        })
    }
    pub async fn authorize(
        &self,
        transaction_id: SsoTransactionId,
        state: &SsoSecret,
        challenge: SsoDigest,
        presented: &PresentedSecrets,
    ) -> Result<Option<AuthorizedSso>, BusinessError> {
        let code = self.cryptography.issue()?;
        let callback = self
            .repository
            .authorize(AuthorizeSso {
                transaction_id,
                state_hash: self.cryptography.digest(state.expose()),
                challenge,
                proof: presented.proof(self.sessions.as_ref()),
                code_hash: self.cryptography.digest(code.expose()),
                now: self.clock.now().get(),
            })
            .await?;
        Ok(callback.map(|callback| AuthorizedSso { code, callback }))
    }
    pub async fn consume(
        &self,
        audience: &str,
        code: &SsoSecret,
        state: &SsoSecret,
        browser_cookie: SsoBrowserCookie,
    ) -> Result<IssuedSso, BusinessError> {
        self.policy.holding(audience)?;
        let token = self.sessions.issue()?;
        let result = self
            .repository
            .consume(ConsumeSso {
                transaction_id: browser_cookie.transaction_id,
                audience: audience.into(),
                code_hash: self.cryptography.digest(code.expose()),
                state_hash: self.cryptography.digest(state.expose()),
                browser_hash: self.cryptography.digest(browser_cookie.browser.expose()),
                verifier_challenge: self.cryptography.digest(browser_cookie.verifier.expose()),
                session_id: self.sessions.identifier()?,
                session_digest: self.sessions.digest(SecretPurpose::Session, &token),
                now: self.clock.now().get(),
            })
            .await?;
        Ok(IssuedSso { token, result })
    }
}
