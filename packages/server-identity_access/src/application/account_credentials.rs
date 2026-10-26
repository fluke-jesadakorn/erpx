use crate::{
    application::{
        account_auth::{PresentedSecrets, SignedIn},
        account_ports::*,
    },
    domain::{account_auth::*, account_credentials::*},
};
use erpx_server_shared::public::{BusinessError, Clock};
use std::sync::Arc;

pub trait AccountMailCryptography: Send + Sync {
    fn seal(
        &self,
        proof_id: &str,
        kind: CredentialProofKind,
        account_origin: &str,
        return_path: &str,
        token: &OpaqueSecret,
    ) -> Result<String, BusinessError>;
}
#[derive(Clone)]
pub struct PreparedCredentialMail {
    pub proof_id: String,
    pub outbox_id: String,
    pub event_id: String,
    pub kind: CredentialProofKind,
    pub email: EmailAddress,
    pub token_digest: SecretDigest,
    pub ciphertext: String,
    pub now: i64,
}
#[derive(Clone)]
pub struct ResetCredential {
    pub token_digest: SecretDigest,
    pub password_hash: PasswordHash,
    pub event_id: String,
    pub now: i64,
}
#[derive(Clone)]
pub struct ChangeCredential {
    pub proof: DeviceProof,
    pub previous: PasswordHash,
    pub password_hash: PasswordHash,
    pub event_id: String,
    pub now: i64,
}
#[derive(Clone)]
pub struct VerifyCredential {
    pub token_digest: SecretDigest,
    pub proof: DeviceProof,
    pub candidate_browser: SecretDigest,
    pub session_digest: SecretDigest,
    pub session_id: String,
    pub handle: String,
    pub expected_revision: Option<i64>,
    pub event_id: String,
    pub now: i64,
}
pub struct VerifiedCredential {
    pub completed: CompletedSignIn,
    pub created_browser: bool,
}
#[derive(Clone)]
pub struct SignUpCredential {
    pub invitation_digest: SecretDigest,
    pub name: String,
    pub email: EmailAddress,
    pub password_hash: PasswordHash,
    pub credential_id: String,
    pub principal_id: String,
    pub mail: PreparedCredentialMail,
}

pub trait AccountCredentialRepository: Send + Sync {
    fn request_mail(&self, input: PreparedCredentialMail) -> AccountFuture<'_, ()>;
    fn reset_password(&self, input: ResetCredential) -> AccountFuture<'_, ()>;
    fn verify_email(&self, input: VerifyCredential) -> AccountFuture<'_, VerifiedCredential>;
    fn current_credential(
        &self,
        proof: DeviceProof,
        now: i64,
    ) -> AccountFuture<'_, CredentialSnapshot>;
    fn change_password(&self, input: ChangeCredential) -> AccountFuture<'_, ()>;
    fn sign_up(&self, input: SignUpCredential) -> AccountFuture<'_, ()>;
}

/// A trusted Holding adapter must retain current IAM/People source locks until the Control commit.
pub trait InvitationCredentialLease: Send {
    fn facts(&self) -> &InvitationCredentialFacts;
}
pub struct InvitationCredentialQuery {
    pub invitation_id: String,
    pub holding_id: String,
    pub company_id: String,
    pub person_id: String,
    pub name: String,
    pub role_payload: String,
    pub scope_kind: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub created_by: String,
    pub assignment_id: String,
    pub expires_at: i64,
    pub created_at: i64,
    pub email: EmailAddress,
    pub now: i64,
}
pub trait InvitationCredentialAuthority: Send + Sync {
    fn inspect<'a>(
        &'a self,
        request: &'a InvitationCredentialQuery,
    ) -> AccountFuture<'a, Box<dyn InvitationCredentialLease>>;
}
pub struct UnavailableInvitationCredentialAuthority;
impl InvitationCredentialAuthority for UnavailableInvitationCredentialAuthority {
    fn inspect<'a>(
        &'a self,
        _: &'a InvitationCredentialQuery,
    ) -> AccountFuture<'a, Box<dyn InvitationCredentialLease>> {
        Box::pin(async { Err(signup_unavailable()) })
    }
}

pub struct AccountCredentials {
    pub(crate) policy: AccountPolicy,
    repository: Arc<dyn AccountCredentialRepository>,
    passwords: Arc<dyn PasswordHashing>,
    secrets: Arc<dyn AuthenticationSecrets>,
    mail: Arc<dyn AccountMailCryptography>,
    clock: Arc<dyn Clock + Send + Sync>,
}
impl AccountCredentials {
    pub fn new(
        policy: AccountPolicy,
        repository: Arc<dyn AccountCredentialRepository>,
        passwords: Arc<dyn PasswordHashing>,
        secrets: Arc<dyn AuthenticationSecrets>,
        mail: Arc<dyn AccountMailCryptography>,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> Result<Self, BusinessError> {
        policy.validate()?;
        Ok(Self {
            policy,
            repository,
            passwords,
            secrets,
            mail,
            clock,
        })
    }
    fn prepared_mail(
        &self,
        kind: CredentialProofKind,
        email: EmailAddress,
        return_path: Option<&str>,
    ) -> Result<PreparedCredentialMail, BusinessError> {
        let path = account_return_path(return_path)?;
        let token = self.secrets.issue()?;
        let proof_id = self.secrets.identifier()?;
        let ciphertext =
            self.mail
                .seal(&proof_id, kind, &self.policy.account_origin, &path, &token)?;
        Ok(PreparedCredentialMail {
            proof_id,
            outbox_id: self.secrets.identifier()?,
            event_id: self.secrets.identifier()?,
            kind,
            email,
            token_digest: self.secrets.digest(kind.purpose(), &token),
            ciphertext,
            now: self.clock.now().get(),
        })
    }
    pub async fn request(
        &self,
        kind: CredentialProofKind,
        email: &str,
        return_path: Option<&str>,
    ) -> Result<(), BusinessError> {
        self.repository
            .request_mail(self.prepared_mail(kind, EmailAddress::new(email)?, return_path)?)
            .await
    }
    pub async fn reset_password(
        &self,
        token: String,
        new_password: String,
    ) -> Result<(), BusinessError> {
        let token = OpaqueSecret::new(token).map_err(|_| invalid_proof())?;
        let password_hash = self
            .passwords
            .hash(Password::new_credential(new_password)?)
            .await?;
        self.repository
            .reset_password(ResetCredential {
                token_digest: self.secrets.digest(SecretPurpose::PasswordReset, &token),
                password_hash,
                event_id: self.secrets.identifier()?,
                now: self.clock.now().get(),
            })
            .await
    }
    pub async fn verify_email(
        &self,
        token: String,
        presented: &PresentedSecrets,
        revision: Option<i64>,
    ) -> Result<SignedIn, BusinessError> {
        if revision.is_some_and(|revision| !(0..=MAX_DEVICE_REVISION).contains(&revision)) {
            return Err(stale_device());
        }
        let token = OpaqueSecret::new(token).map_err(|_| invalid_proof())?;
        let browser = self.secrets.issue()?;
        let session = self.secrets.issue()?;
        let handle = self.secrets.identifier()?;
        let result = self
            .repository
            .verify_email(VerifyCredential {
                token_digest: self
                    .secrets
                    .digest(SecretPurpose::EmailVerification, &token),
                proof: presented.proof(self.secrets.as_ref()),
                candidate_browser: self.secrets.digest(SecretPurpose::DeviceBrowser, &browser),
                session_digest: self.secrets.digest(SecretPurpose::Session, &session),
                session_id: self.secrets.identifier()?,
                handle: handle.clone(),
                expected_revision: revision,
                event_id: self.secrets.identifier()?,
                now: self.clock.now().get(),
            })
            .await?;
        Ok(SignedIn {
            result: result.completed,
            browser_secret: result.created_browser.then_some(browser),
            session_secret: session,
            handle,
        })
    }
    pub async fn change_password(
        &self,
        presented: &PresentedSecrets,
        current_password: String,
        new_password: String,
    ) -> Result<(), BusinessError> {
        let proof = presented.proof(self.secrets.as_ref());
        let snapshot = self
            .repository
            .current_credential(proof.clone(), self.clock.now().get())
            .await?;
        if !self
            .passwords
            .verify(
                Password::sign_in(current_password)?,
                Some(snapshot.password_hash.clone()),
            )
            .await?
        {
            return Err(denied());
        }
        let password_hash = self
            .passwords
            .hash(Password::new_credential(new_password)?)
            .await?;
        self.repository
            .change_password(ChangeCredential {
                proof,
                previous: snapshot.password_hash,
                password_hash,
                event_id: self.secrets.identifier()?,
                now: self.clock.now().get(),
            })
            .await
    }
    pub async fn sign_up(
        &self,
        name: String,
        email: &str,
        password: String,
        invitation_token: Option<String>,
        return_path: Option<&str>,
    ) -> Result<(), BusinessError> {
        let token = OpaqueSecret::new(invitation_token.ok_or_else(signup_unavailable)?)
            .map_err(|_| signup_unavailable())?;
        let name = name.trim().to_owned();
        if name.is_empty() || name.chars().count() > 100 || name.chars().any(char::is_control) {
            return Err(BusinessError::invalid(
                "INVALID_ACCOUNT_NAME",
                "Enter a name of at most 100 characters.",
            ));
        }
        let email = EmailAddress::new(email)?;
        let mail = self.prepared_mail(
            CredentialProofKind::EmailVerification,
            email.clone(),
            return_path,
        )?;
        let password_hash = self
            .passwords
            .hash(Password::new_credential(password)?)
            .await?;
        self.repository
            .sign_up(SignUpCredential {
                invitation_digest: self.secrets.digest(SecretPurpose::Invitation, &token),
                name,
                email,
                password_hash,
                credential_id: self.secrets.identifier()?,
                principal_id: self.secrets.identifier()?,
                mail,
            })
            .await
    }
}
