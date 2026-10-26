use crate::{
    application::{
        account_credentials::AccountCredentials,
        account_ports::{AccountFuture, AuthenticationSecrets, PasswordHashing},
    },
    domain::{
        account_auth::{
            CredentialSnapshot, OpaqueSecret, Password, PasswordHash, SecretDigest, SecretPurpose,
        },
        account_credentials::signup_unavailable,
    },
};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use std::sync::Arc;

pub struct InvitationAccountState {
    pub name: String,
    pub email: String,
    pub credential: Option<CredentialSnapshot>,
}
pub trait AccountInvitationRepository: Send + Sync {
    fn current(&self, digest: SecretDigest, now: i64) -> AccountFuture<'_, InvitationAccountState>;
    fn activate(&self, digest: SecretDigest, verified_hash: PasswordHash) -> AccountFuture<'_, ()>;
}
pub struct AccountInvitations {
    repository: Arc<dyn AccountInvitationRepository>,
    credentials: Arc<AccountCredentials>,
    passwords: Arc<dyn PasswordHashing>,
    secrets: Arc<dyn AuthenticationSecrets>,
    clock: Arc<dyn Clock + Send + Sync>,
}
impl AccountInvitations {
    pub(crate) fn policy(&self) -> &crate::domain::account_auth::AccountPolicy {
        &self.credentials.policy
    }
    pub fn new(
        repository: Arc<dyn AccountInvitationRepository>,
        credentials: Arc<AccountCredentials>,
        passwords: Arc<dyn PasswordHashing>,
        secrets: Arc<dyn AuthenticationSecrets>,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> Self {
        Self {
            repository,
            credentials,
            passwords,
            secrets,
            clock,
        }
    }
    pub async fn inspect(&self, token: String) -> Result<InvitationAccountState, BusinessError> {
        let token = OpaqueSecret::new(token).map_err(|_| signup_unavailable())?;
        self.repository
            .current(
                self.secrets.digest(SecretPurpose::Invitation, &token),
                self.clock.now().get(),
            )
            .await
    }
    pub async fn accept(&self, token: String, password: String) -> Result<(), BusinessError> {
        if password.encode_utf16().count() > 128 {
            return Err(BusinessError::invalid(
                "INVALID_PASSWORD",
                "Use a password of at most 128 characters.",
            ));
        }
        let token = OpaqueSecret::new(token).map_err(|_| signup_unavailable())?;
        let validated_password = Password::new_credential(password.clone())?;
        let digest = self.secrets.digest(SecretPurpose::Invitation, &token);
        let mut state = self
            .repository
            .current(digest.clone(), self.clock.now().get())
            .await?;
        if state.credential.is_none() {
            // Only this step may retry Control credential writes. No Holding
            // commit or external delivery occurs inside that retry boundary.
            let prepared = self
                .credentials
                .sign_up(
                    state.name.clone(),
                    &state.email,
                    password,
                    Some(token.expose().to_owned()),
                    None,
                )
                .await;
            state = match self
                .repository
                .current(digest.clone(), self.clock.now().get())
                .await
            {
                Ok(state) if state.credential.is_some() => state,
                _ => {
                    prepared?;
                    return Err(signup_unavailable());
                }
            };
        }
        let credential = state.credential.ok_or_else(signup_unavailable)?;
        // Retry proves the original password outside every database lease. A
        // newly salted hash never serves as evidence for an existing credential.
        if !self
            .passwords
            .verify(validated_password, Some(credential.password_hash.clone()))
            .await?
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "ACTIVATION_RETRY_PASSWORD",
                "Use the password from your first activation attempt.",
            ));
        }
        self.repository
            .activate(digest, credential.password_hash)
            .await
    }
}
