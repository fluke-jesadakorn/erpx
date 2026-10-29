use crate::{
    application::account_ports::{
        AccountRepository, AuthenticationSecrets, CompleteSignIn, DeviceProof, DeviceSelection,
        PasswordHashing, ReserveSignIn,
    },
    domain::account_auth::{
        AccountPolicy, DeviceAccounts, EmailAddress, MAX_DEVICE_REVISION, OpaqueSecret, Password,
        RequestAudience, SecretPurpose, denied,
    },
};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use std::sync::Arc;

pub struct PresentedSecrets {
    pub browser: Option<OpaqueSecret>,
    pub active: Option<OpaqueSecret>,
    pub saved: Vec<(String, OpaqueSecret)>,
}
impl PresentedSecrets {
    pub fn proof(&self, secrets: &dyn AuthenticationSecrets) -> DeviceProof {
        DeviceProof {
            browser: self
                .browser
                .as_ref()
                .map(|value| secrets.digest(SecretPurpose::DeviceBrowser, value)),
            active: self
                .active
                .as_ref()
                .map(|value| secrets.digest(SecretPurpose::Session, value)),
            saved: self
                .saved
                .iter()
                .map(|(handle, value)| {
                    (
                        handle.clone(),
                        secrets.digest(SecretPurpose::Session, value),
                    )
                })
                .collect(),
        }
    }
}
pub struct SignedIn {
    pub result: crate::application::account_ports::CompletedSignIn,
    pub browser_secret: Option<OpaqueSecret>,
    pub session_secret: OpaqueSecret,
    pub handle: String,
}
pub struct SelectedAccount {
    pub devices: DeviceAccounts,
    pub session_secret: OpaqueSecret,
}
pub struct InitializedDevice {
    pub devices: DeviceAccounts,
    pub browser_secret: Option<OpaqueSecret>,
}

pub struct AccountAuthentication {
    pub(crate) policy: AccountPolicy,
    pub(crate) repository: Arc<dyn AccountRepository>,
    pub(crate) passwords: Arc<dyn PasswordHashing>,
    pub(crate) secrets: Arc<dyn AuthenticationSecrets>,
    pub(crate) clock: Arc<dyn Clock + Send + Sync>,
}
impl AccountAuthentication {
    pub fn new(
        policy: AccountPolicy,
        repository: Arc<dyn AccountRepository>,
        passwords: Arc<dyn PasswordHashing>,
        secrets: Arc<dyn AuthenticationSecrets>,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> Result<Self, BusinessError> {
        policy.validate()?;
        Ok(Self {
            policy,
            repository,
            passwords,
            secrets,
            clock,
        })
    }
    pub async fn sign_in(
        &self,
        email: &str,
        password: String,
        presented: &PresentedSecrets,
        expected_revision: Option<i64>,
    ) -> Result<SignedIn, BusinessError> {
        let email = EmailAddress::new(email)?;
        let password = Password::sign_in(password)?;
        if expected_revision.is_some_and(|revision| !(0..=MAX_DEVICE_REVISION).contains(&revision))
        {
            return Err(BusinessError::invalid(
                "INVALID_DEVICE_REVISION",
                "Review your current saved accounts.",
            ));
        }
        let now = self.clock.now().get();
        let browser = self.secrets.issue()?;
        let nonce = self.secrets.issue()?;
        let proof = presented.proof(self.secrets.as_ref());
        let reservation = self
            .repository
            .reserve_sign_in(ReserveSignIn {
                proof: proof.clone(),
                candidate_browser: self.secrets.digest(SecretPurpose::DeviceBrowser, &browser),
                nonce: self.secrets.digest(SecretPurpose::Reservation, &nonce),
                email,
                expected_revision,
                now,
            })
            .await?;
        let verified = self
            .passwords
            .verify(
                password,
                reservation
                    .credential
                    .as_ref()
                    .map(|credential| credential.password_hash.clone()),
            )
            .await;
        let result = async {
            if !verified? {
                return Err(denied());
            }
            let credential = reservation
                .credential
                .clone()
                .filter(|credential| {
                    credential.can_authenticate(self.policy.require_email_verification)
                })
                .ok_or_else(denied)?;
            let session_secret = self.secrets.issue()?;
            let handle = self.secrets.identifier()?;
            let completed = self
                .repository
                .complete_sign_in(CompleteSignIn {
                    proof,
                    reservation: reservation.clone(),
                    credential,
                    session_id: self.secrets.identifier()?,
                    session_digest: self.secrets.digest(SecretPurpose::Session, &session_secret),
                    handle: handle.clone(),
                    now: self.clock.now().get(),
                })
                .await?;
            Ok(SignedIn {
                result: completed,
                browser_secret: (reservation.created_browser
                    || reservation.recovery_expires_at.is_some())
                .then_some(browser),
                session_secret,
                handle,
            })
        }
        .await;
        if result.is_err() {
            self.repository.release_sign_in(reservation).await?;
        }
        result
    }
    pub async fn initialize(
        &self,
        presented: &PresentedSecrets,
        expected_revision: Option<i64>,
    ) -> Result<InitializedDevice, BusinessError> {
        let browser = self.secrets.issue()?;
        let (devices, created) = self
            .repository
            .initialize_device(
                presented.proof(self.secrets.as_ref()),
                self.secrets.digest(SecretPurpose::DeviceBrowser, &browser),
                expected_revision,
                self.clock.now().get(),
            )
            .await?;
        Ok(InitializedDevice {
            devices,
            browser_secret: created.then_some(browser),
        })
    }
    pub async fn select(
        &self,
        presented: &PresentedSecrets,
        handle: String,
        revision: i64,
    ) -> Result<SelectedAccount, BusinessError> {
        validate_selection(&handle, revision)?;
        if !presented
            .saved
            .iter()
            .any(|(candidate, _)| candidate == &handle)
        {
            return Err(denied());
        }
        let selected = self
            .repository
            .select_device(DeviceSelection {
                proof: presented.proof(self.secrets.as_ref()),
                handle: handle.clone(),
                expected_revision: revision,
                now: self.clock.now().get(),
            })
            .await?;
        let secret = presented
            .saved
            .iter()
            .find(|(candidate, token)| {
                candidate == &handle
                    && self.secrets.digest(SecretPurpose::Session, token) == selected.session_digest
            })
            .map(|(_, token)| token.clone())
            .ok_or_else(|| {
                BusinessError::new(
                    FailureKind::Unauthenticated,
                    "DEVICE_ACCOUNT_EXPIRED",
                    "This saved account expired. Sign in again.",
                )
            })?;
        Ok(SelectedAccount {
            devices: selected.devices,
            session_secret: secret,
        })
    }
    pub async fn devices(
        &self,
        presented: &PresentedSecrets,
    ) -> Result<DeviceAccounts, BusinessError> {
        self.repository
            .devices(
                presented.proof(self.secrets.as_ref()),
                self.clock.now().get(),
            )
            .await
    }
    pub async fn remove(
        &self,
        presented: &PresentedSecrets,
        handle: String,
        revision: i64,
    ) -> Result<DeviceAccounts, BusinessError> {
        validate_selection(&handle, revision)?;
        self.repository
            .remove_device(DeviceSelection {
                proof: presented.proof(self.secrets.as_ref()),
                handle,
                expected_revision: revision,
                now: self.clock.now().get(),
            })
            .await
    }
    pub async fn session(
        &self,
        presented: &PresentedSecrets,
        audience: RequestAudience,
    ) -> Result<Option<crate::domain::account_auth::AccountSession>, BusinessError> {
        self.repository
            .session(
                presented.proof(self.secrets.as_ref()),
                audience,
                self.clock.now().get(),
            )
            .await
    }
    pub async fn sign_out(
        &self,
        presented: &PresentedSecrets,
        audience: RequestAudience,
    ) -> Result<(), BusinessError> {
        self.repository
            .sign_out(
                presented.proof(self.secrets.as_ref()),
                audience,
                self.clock.now().get(),
            )
            .await
    }
}

fn validate_selection(handle: &str, revision: i64) -> Result<(), BusinessError> {
    if !(0..=MAX_DEVICE_REVISION).contains(&revision)
        || handle.len() != 36
        || !handle.bytes().all(|c| c.is_ascii_hexdigit() || c == b'-')
    {
        return Err(BusinessError::invalid(
            "INVALID_DEVICE_SELECTION",
            "Review your saved account selection.",
        ));
    }
    Ok(())
}
