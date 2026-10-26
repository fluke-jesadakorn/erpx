use crate::domain::account_auth::{
    AccountSession, CredentialSnapshot, DeviceAccounts, EmailAddress, OpaqueSecret, Password,
    PasswordHash, RequestAudience, SecretDigest, SecretPurpose,
};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type AccountFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Default)]
pub struct DeviceProof {
    pub browser: Option<SecretDigest>,
    pub active: Option<SecretDigest>,
    pub saved: Vec<(String, SecretDigest)>,
}
#[derive(Clone)]
pub struct SignInReservation {
    pub browser: SecretDigest,
    pub created_browser: bool,
    pub nonce: SecretDigest,
    pub credential: Option<CredentialSnapshot>,
}
#[derive(Clone)]
pub struct ReserveSignIn {
    pub proof: DeviceProof,
    pub candidate_browser: SecretDigest,
    pub nonce: SecretDigest,
    pub email: EmailAddress,
    pub expected_revision: Option<i64>,
    pub now: i64,
}
#[derive(Clone)]
pub struct CompleteSignIn {
    pub proof: DeviceProof,
    pub reservation: SignInReservation,
    pub credential: CredentialSnapshot,
    pub session_id: String,
    pub session_digest: SecretDigest,
    pub handle: String,
    pub now: i64,
}
#[derive(Clone)]
pub struct CompletedSignIn {
    pub session: AccountSession,
    pub devices: DeviceAccounts,
}
#[derive(Clone)]
pub struct DeviceSelection {
    pub proof: DeviceProof,
    pub handle: String,
    pub expected_revision: i64,
    pub now: i64,
}
#[derive(Clone)]
pub struct SelectedDevice {
    pub devices: DeviceAccounts,
    pub session_digest: SecretDigest,
}

/// All database effects for each command share the adapter's single Control transaction.
pub trait AccountRepository: Send + Sync {
    fn reserve_sign_in(&self, input: ReserveSignIn) -> AccountFuture<'_, SignInReservation>;
    fn complete_sign_in(&self, input: CompleteSignIn) -> AccountFuture<'_, CompletedSignIn>;
    fn release_sign_in(&self, reservation: SignInReservation) -> AccountFuture<'_, ()>;
    fn session(
        &self,
        proof: DeviceProof,
        audience: RequestAudience,
        now: i64,
    ) -> AccountFuture<'_, Option<AccountSession>>;
    fn devices(&self, proof: DeviceProof, now: i64) -> AccountFuture<'_, DeviceAccounts>;
    fn initialize_device(
        &self,
        proof: DeviceProof,
        candidate: SecretDigest,
        now: i64,
    ) -> AccountFuture<'_, (DeviceAccounts, bool)>;
    fn select_device(&self, input: DeviceSelection) -> AccountFuture<'_, SelectedDevice>;
    fn remove_device(&self, input: DeviceSelection) -> AccountFuture<'_, DeviceAccounts>;
    fn sign_out(
        &self,
        proof: DeviceProof,
        audience: RequestAudience,
        now: i64,
    ) -> AccountFuture<'_, ()>;
}

pub trait PasswordHashing: Send + Sync {
    fn hash(&self, password: Password) -> AccountFuture<'_, PasswordHash>;
    /// Missing accounts perform the same bounded Argon2 work using a private dummy hash.
    fn verify(&self, password: Password, expected: Option<PasswordHash>)
    -> AccountFuture<'_, bool>;
}
pub trait AuthenticationSecrets: Send + Sync {
    fn issue(&self) -> Result<OpaqueSecret, BusinessError>;
    fn digest(&self, purpose: SecretPurpose, secret: &OpaqueSecret) -> SecretDigest;
    fn identifier(&self) -> Result<String, BusinessError>;
}
