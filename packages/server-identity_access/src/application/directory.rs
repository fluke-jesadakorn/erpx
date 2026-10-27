//! Provider I/O precedes all retained database leases.
use crate::application::{
    account_auth::{PresentedSecrets, SignedIn},
    account_ports::AccountFuture,
};
use crate::domain::directory::{
    CompleteDirectorySnapshot, DirectoryConnectionDescriptor, DirectoryMode,
};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use std::{future::Future, pin::Pin, sync::Arc};

/// Password buffers/credentials deliberately implement neither Debug nor Serialize.
pub struct DirectoryPassword(String);
impl DirectoryPassword {
    pub fn new(value: String) -> Result<Self, BusinessError> {
        if value.is_empty() || value.encode_utf16().count() > 1024 {
            return Err(BusinessError::invalid(
                "INVALID_DIRECTORY_PASSWORD",
                "Enter your Directory credentials.",
            ));
        }
        Ok(Self(value))
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}

/// This is only successful provider credential evidence. It is not an account,
/// IAM Actor, reviewed membership or permission. The final issuer rechecks all
/// current route/member/identity/source facts under its retained owner leases.
pub struct VerifiedDirectorySubject {
    connection_id: String,
    external_id: String,
    config_fingerprint: String,
    profile_fingerprint: String,
    connection_version: i64,
    company_id: String,
    holding_id: Option<String>,
    provider: crate::domain::directory::DirectoryProvider,
    mode: DirectoryMode,
    verified_at: i64,
    expires_at: i64,
}
impl VerifiedDirectorySubject {
    #[cfg(feature = "directory-ldaps")]
    pub(crate) fn from_verified_bind(
        connection: &DirectoryConnectionDescriptor,
        external_id: String,
        verified_at: i64,
    ) -> Result<Self, BusinessError> {
        let expires_at = verified_at
            .checked_add(crate::domain::account_auth::SIGN_IN_RESERVATION_MS)
            .filter(|_| verified_at >= 0)
            .ok_or_else(login_failed)?;
        Ok(Self {
            connection_id: connection.connection_id.clone(),
            external_id,
            config_fingerprint: connection.config_fingerprint.clone(),
            profile_fingerprint: connection.profile_fingerprint.clone(),
            connection_version: connection.version,
            company_id: connection.company_id.clone(),
            holding_id: connection.holding_id.clone(),
            provider: connection.provider,
            mode: connection.mode,
            verified_at,
            expires_at,
        })
    }
    pub(crate) fn require_current_credential(&self, now: i64) -> Result<(), BusinessError> {
        if now < self.verified_at || now >= self.expires_at {
            return Err(login_failed());
        }
        Ok(())
    }
    #[cfg(feature = "directory-ldaps")]
    pub(crate) fn expires_at(&self) -> i64 {
        self.expires_at
    }
    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }
    pub fn external_id(&self) -> &str {
        &self.external_id
    }
    pub fn matches(&self, connection: &DirectoryConnectionDescriptor) -> bool {
        self.connection_id == connection.connection_id
            && self.config_fingerprint == connection.config_fingerprint
            && self.profile_fingerprint == connection.profile_fingerprint
            && self.connection_version == connection.version
            && self.company_id == connection.company_id
            && self.holding_id == connection.holding_id
            && self.provider == connection.provider
            && self.mode == connection.mode
    }
}

pub enum SynchronizationOutcome {
    Applied,
    NewerCompleteObservationRetained,
}
pub trait DirectoryRoutes: Send + Sync {
    fn current<'a>(
        &'a self,
        connection_id: &'a str,
    ) -> AccountFuture<'a, DirectoryConnectionDescriptor>;
}
pub trait DirectoryProviderRead: Send + Sync {
    fn verify<'a>(
        &'a self,
        connection: &'a DirectoryConnectionDescriptor,
        username: &'a str,
        password: DirectoryPassword,
    ) -> AccountFuture<'a, VerifiedDirectorySubject>;
    /// Must finish every required search/page/range. A partial source never
    /// becomes CompleteDirectorySnapshot and cannot disable missing identities.
    fn observe<'a>(
        &'a self,
        connection: &'a DirectoryConnectionDescriptor,
        observed_at: i64,
    ) -> AccountFuture<'a, CompleteDirectorySnapshot>;
}
pub trait DirectorySynchronization: Send + Sync {
    /// Retain current Control route before the inner owner unit, rechecking the
    /// exact connection version/config/profile. All observations, removals,
    /// reviewed mapping-policy evaluation/effects and audit commit together.
    fn apply<'a>(
        &'a self,
        connection: &'a DirectoryConnectionDescriptor,
        snapshot: CompleteDirectorySnapshot,
    ) -> AccountFuture<'a, SynchronizationOutcome>;
    /// Older failures must not invalidate a newer healthy observation. A current
    /// failure backdates identity freshness to the strict five-minute cutoff.
    fn failed<'a>(
        &'a self,
        connection: &'a DirectoryConnectionDescriptor,
        started_at: i64,
        code: &'static str,
    ) -> AccountFuture<'a, ()>;
}
pub trait DirectoryAttemptCounter: Send + Sync {
    /// Committed separately: invalid credentials must not roll this attempt back.
    fn attempt<'a>(&'a self, connection_id: &'a str, username: &'a str) -> AccountFuture<'a, ()>;
}
/// Identity eligibility remains enumeration-safe. Device CAS/nonce conflict,
/// bounded runtime failure and partial Holding retirement retain their actual
/// command status, matching the outer B0 synchronizeAccountAuth boundary.
pub enum DirectoryAdmissionError {
    IdentityDenied,
    AccountCommand(BusinessError),
}
pub type DirectoryIssueFuture<'a> =
    Pin<Box<dyn Future<Output = Result<SignedIn, DirectoryAdmissionError>> + Send + 'a>>;

pub trait DirectoryAccountIssuance: Send + Sync {
    /// Implementation owns the session-first non-retrying Control command,
    /// acquires current exact Directory eligibility only after that lease,
    /// retains it through normal opaque Account issuance/commit, and performs
    /// existing group/revision/nonce CAS + mandatory Holding child fences.
    /// No constructor may replace this with a local dummy password/hash/Actor.
    fn issue<'a>(
        &'a self,
        subject: VerifiedDirectorySubject,
        connection: DirectoryConnectionDescriptor,
        presented: &'a PresentedSecrets,
        expected_revision: Option<i64>,
    ) -> DirectoryIssueFuture<'a>;
}
pub struct DirectoryAuthentication {
    routes: Arc<dyn DirectoryRoutes>,
    provider: Arc<dyn DirectoryProviderRead>,
    synchronization: Arc<dyn DirectorySynchronization>,
    attempts: Arc<dyn DirectoryAttemptCounter>,
    issuance: Arc<dyn DirectoryAccountIssuance>,
    clock: Arc<dyn Clock + Send + Sync>,
}
impl DirectoryAuthentication {
    pub fn new(
        routes: Arc<dyn DirectoryRoutes>,
        provider: Arc<dyn DirectoryProviderRead>,
        synchronization: Arc<dyn DirectorySynchronization>,
        attempts: Arc<dyn DirectoryAttemptCounter>,
        issuance: Arc<dyn DirectoryAccountIssuance>,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> Self {
        Self {
            routes,
            provider,
            synchronization,
            attempts,
            issuance,
            clock,
        }
    }
    pub async fn sign_in(
        &self,
        connection_id: &str,
        username: &str,
        password: String,
        presented: &PresentedSecrets,
        expected_revision: Option<i64>,
    ) -> Result<SignedIn, BusinessError> {
        if username.is_empty() || username.encode_utf16().count() > 200 {
            return Err(BusinessError::invalid(
                "INVALID_DIRECTORY_USERNAME",
                "Enter your Directory credentials.",
            ));
        }
        let password = DirectoryPassword::new(password)?;
        self.attempts.attempt(connection_id, username).await?;
        // B0's plugin hides credential/provider/approval failures. Its outer
        // Account synchronization retains revision/nonce and runtime statuses.
        let (subject, connection) = self
            .prepare_verified(connection_id, username, password)
            .await
            .map_err(|_| login_failed())?;
        self.issuance
            .issue(subject, connection, presented, expected_revision)
            .await
            .map_err(|error| match error {
                DirectoryAdmissionError::IdentityDenied => login_failed(),
                DirectoryAdmissionError::AccountCommand(error) => error,
            })
    }
    async fn prepare_verified(
        &self,
        connection_id: &str,
        username: &str,
        password: DirectoryPassword,
    ) -> Result<(VerifiedDirectorySubject, DirectoryConnectionDescriptor), BusinessError> {
        let connection = self.routes.current(connection_id).await?;
        if connection.mode != DirectoryMode::Ldaps {
            return Err(BusinessError::invalid(
                "DIRECTORY_MODE_UNAVAILABLE",
                "This Directory sign-in method is unavailable.",
            ));
        }
        let subject = self
            .provider
            .verify(&connection, username, password)
            .await?;
        if !subject.matches(&connection) {
            return Err(BusinessError::invalid(
                "DIRECTORY_SOURCE_CHANGED",
                "The Directory source changed.",
            ));
        }
        subject.require_current_credential(self.clock.now().get())?;
        let observed_at = self.clock.now().get();
        let synchronized = async {
            let snapshot = self.provider.observe(&connection, observed_at).await?;
            snapshot.require_fresh(self.clock.now().get())?;
            self.synchronization.apply(&connection, snapshot).await
        }
        .await;
        if let Err(error) = synchronized {
            self.synchronization
                .failed(&connection, observed_at, error.code)
                .await?;
            return Err(error);
        }
        // Neither an applied snapshot nor a discarded older observation grants
        // access: issuance retains actual current eligibility through commit.
        subject.require_current_credential(self.clock.now().get())?;
        Ok((subject, connection))
    }
}
fn login_failed() -> BusinessError {
    BusinessError::new(
        FailureKind::Unauthenticated,
        "DIRECTORY_LOGIN_FAILED",
        "Directory sign-in failed. Check your credentials and approved access.",
    )
}
