use crate::{
    application::account_ports::{AccountFuture, AuthenticationSecrets},
    domain::{
        account_auth::{EmailAddress, OpaqueSecret, SecretDigest},
        account_credentials::CredentialProofKind,
        account_mail::*,
    },
};
use erpx_server_shared::public::{BusinessError, Clock};
use std::{future::Future, pin::Pin, sync::Arc};
pub type AccountMailSendFuture<'a> =
    Pin<Box<dyn Future<Output = AccountMailSendOutcome> + Send + 'a>>;
pub trait AccountMailProvider: Send + Sync {
    fn profile(&self) -> &AccountMailProviderProfile;
    fn send<'a>(&'a self, message: &'a AccountEmailMessage) -> AccountMailSendFuture<'a>;
}
pub trait AccountMailOpening: Send + Sync {
    fn open_mail(
        &self,
        proof_id: &str,
        ciphertext: &str,
    ) -> Result<OpenedAccountMail, BusinessError>;
}
#[derive(Clone)]
pub struct AccountMailAttempt {
    pub id: String,
    pub proof_id: String,
    pub user_id: String,
    pub kind: CredentialProofKind,
    pub email: EmailAddress,
    pub token_digest: SecretDigest,
    pub payload_ciphertext: String,
    pub attempts: i64,
    pub lease: OpaqueSecret,
    pub lease_until: i64,
    pub expires_at: i64,
}
pub enum AccountMailDispatch {
    Cancelled,
    Completed(AccountMailSendOutcome),
}
pub enum AccountMailClaim {
    Idle,
    Cancelled,
    Ready(AccountMailAttempt),
}
pub trait AccountMailRepository: Send + Sync {
    fn claim(&self, lease: OpaqueSecret, now: i64) -> AccountFuture<'_, AccountMailClaim>;
    fn begin_dispatch<'a>(
        &'a self,
        job: &'a AccountMailAttempt,
        profile: &'a AccountMailProviderProfile,
        fingerprint: &'a str,
        now: i64,
    ) -> AccountFuture<'a, bool>;
    /// Exactly one provider invocation inside a retained, non-retrying current
    /// Control proof lease. A retryable DB callback must never perform I/O.
    fn dispatch_current<'a>(
        &'a self,
        job: &'a AccountMailAttempt,
        message: AccountEmailMessage,
        provider: Arc<dyn AccountMailProvider>,
    ) -> AccountFuture<'a, AccountMailDispatch>;
    fn finish<'a>(
        &'a self,
        job: &'a AccountMailAttempt,
        outcome: AccountMailDispatch,
        now: i64,
    ) -> AccountFuture<'a, ()>;
}
pub struct AccountMailWorker {
    repository: Arc<dyn AccountMailRepository>,
    opening: Arc<dyn AccountMailOpening>,
    provider: Arc<dyn AccountMailProvider>,
    secrets: Arc<dyn AuthenticationSecrets>,
    policy: AccountMailPolicy,
    clock: Arc<dyn Clock + Send + Sync>,
}
impl AccountMailWorker {
    pub fn new(
        repository: Arc<dyn AccountMailRepository>,
        opening: Arc<dyn AccountMailOpening>,
        provider: Arc<dyn AccountMailProvider>,
        secrets: Arc<dyn AuthenticationSecrets>,
        policy: AccountMailPolicy,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> Self {
        Self {
            repository,
            opening,
            provider,
            secrets,
            policy,
            clock,
        }
    }
    pub async fn deliver_one(&self) -> Result<bool, BusinessError> {
        let claimed = self
            .repository
            .claim(self.secrets.issue()?, self.clock.now().get())
            .await?;
        let job = match claimed {
            AccountMailClaim::Idle => return Ok(false),
            AccountMailClaim::Cancelled => return Ok(true),
            AccountMailClaim::Ready(job) => job,
        };
        let message = (|| {
            let payload = self
                .opening
                .open_mail(&job.proof_id, &job.payload_ciphertext)?;
            if payload.kind != job.kind
                || self.secrets.digest(job.kind.purpose(), &payload.token) != job.token_digest
            {
                return Err(account_mail_unavailable());
            }
            self.policy.render(job.email.clone(), &payload, &job.id)
        })();
        let message = match message {
            Ok(message) => message,
            Err(_) => {
                self.repository
                    .finish(
                        &job,
                        AccountMailDispatch::Completed(AccountMailSendOutcome::Rejected(
                            AccountMailFailure::InvalidPayload,
                        )),
                        self.clock.now().get(),
                    )
                    .await?;
                return Ok(true);
            }
        };
        let fingerprint = self.provider.profile().fingerprint(&message)?;
        if !self
            .repository
            .begin_dispatch(
                &job,
                self.provider.profile(),
                &fingerprint,
                self.clock.now().get(),
            )
            .await?
        {
            return Ok(true);
        }
        let outcome = self
            .repository
            .dispatch_current(&job, message, self.provider.clone())
            .await?;
        self.repository
            .finish(&job, outcome, self.clock.now().get())
            .await?;
        Ok(true)
    }
}
