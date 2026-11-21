//! A completed observation/source lease is not a session or Work authority.
//! Only the normal, fresh Work coordinator may lend the final context.
use crate::{
    application::account_ports::{AccountFuture, DeviceProof},
    domain::{
        directory::{CompleteDirectorySnapshot, DirectoryConnectionDescriptor},
        work_context::HoldingWorkspaceRequest,
    },
};
use erpx_server_shared::public::{BusinessError, Clock};
use std::sync::Arc;

/// Server-only retention returned by the actual provider/profile adapter. It
/// carries no permission and cannot be deserialized from a request or an Actor.
/// The concrete adapter holds its actual immutable generation read lease.
pub(crate) trait DirectoryObservationRetention: Send + Sync {
    fn fingerprint(&self) -> &str;
}
pub(crate) struct BoundDirectoryObservation {
    pub snapshot: CompleteDirectorySnapshot,
    pub retention: Arc<dyn DirectoryObservationRetention>,
}
pub(crate) trait DirectoryRefreshProvider: Send + Sync {
    /// Retains the same validated local generation, without provider I/O.
    fn retain<'a>(
        &'a self,
        descriptor: &'a DirectoryConnectionDescriptor,
    ) -> AccountFuture<'a, Arc<dyn DirectoryObservationRetention>>;
    /// One lease acquisition is transferred with the complete observation. The
    /// caller must not reacquire a fair read lock while already retaining it.
    fn observe_bound<'a>(
        &'a self,
        descriptor: &'a DirectoryConnectionDescriptor,
        started: i64,
    ) -> AccountFuture<'a, BoundDirectoryObservation>;
}

/// Factual request fences only: never an AuthorizedWorkContext or Actor DTO.
/// The repository derives these fields from actual session/device/selection rows.
#[derive(Clone)]
pub(crate) struct DirectoryRefreshSource {
    pub descriptor: DirectoryConnectionDescriptor,
    pub identity_id: String,
    pub member_version: i64,
    pub external_id: String,
    pub principal_id: String,
}
#[derive(Clone)]
pub(crate) struct DirectoryRequestPreflight {
    pub host: String,
    pub session_id: erpx_server_shared::public::RecordId,
    pub user_id: erpx_server_shared::public::UserId,
    pub authenticated_at: i64,
    pub session_expires_at: i64,
    pub request_deadline: i64,
    pub workspace: HoldingWorkspaceRequest,
    pub directory: Option<DirectoryRefreshSource>,
    pub stale: bool,
}
pub(crate) struct PreparedDirectoryWork {
    pub preflight: DirectoryRequestPreflight,
    pub retention: Option<Arc<dyn DirectoryObservationRetention>>,
}
pub(crate) type DirectoryWorkspaceInput =
    Box<dyn FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send>;
pub(crate) trait DirectoryRefreshPersistence: Send + Sync {
    /// Session-first current Control/Holding preflight. A hard session, binding,
    /// tombstone, membership or selection denial is returned before any provider
    /// or private-profile work. Every SQL lease ends before this returns.
    fn preflight<'a>(
        &'a self,
        host: &'a str,
        proof: DeviceProof,
        workspace: DirectoryWorkspaceInput,
    ) -> AccountFuture<'a, DirectoryRequestPreflight>;
    /// Recheck the original actual session/selection fences and current sources,
    /// then reconcile the complete system observation. This is non-retrying
    /// Control around an independently retryable Holding unit. The already-held
    /// generation is reused, so no nested profile lock or second checkout exists.
    fn reconcile_bound<'a>(
        &'a self,
        proof: DeviceProof,
        preflight: &'a DirectoryRequestPreflight,
        observation: BoundDirectoryObservation,
    ) -> AccountFuture<'a, ()>;
    /// This method is only for a genuine provider/source observation failure.
    /// Auth revocation or a changed request is not a Directory health failure.
    fn observation_failed<'a>(
        &'a self,
        descriptor: &'a DirectoryConnectionDescriptor,
        started: i64,
        code: &'static str,
    ) -> AccountFuture<'a, ()>;
}

pub(crate) struct DirectoryBusinessRefresh {
    pub persistence: Arc<dyn DirectoryRefreshPersistence>,
    pub provider: Arc<dyn DirectoryRefreshProvider>,
    pub clock: Arc<dyn Clock + Send + Sync>,
}
impl DirectoryBusinessRefresh {
    pub async fn prepare(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: DirectoryWorkspaceInput,
    ) -> Result<PreparedDirectoryWork, BusinessError> {
        let preflight = self
            .persistence
            .preflight(host, proof.clone(), workspace)
            .await?;
        if self.clock.now().get() >= preflight.request_deadline {
            return Err(expired(&preflight, self.clock.now().get()));
        }
        let Some(source) = &preflight.directory else {
            return Ok(PreparedDirectoryWork {
                preflight,
                retention: None,
            });
        };
        let descriptor = &source.descriptor;
        let retention = if preflight.stale {
            let started = self.clock.now().get();
            let observed = match self.provider.observe_bound(descriptor, started).await {
                Ok(observed) => observed,
                Err(error) => {
                    if matches!(
                        error.code,
                        "DIRECTORY_REFRESH_BUSY"
                            | "DIRECTORY_SOURCE_CONFIGURATION"
                            | "DIRECTORY_SECRET_UNAVAILABLE"
                    ) {
                        return Err(error);
                    }
                    self.persistence
                        .observation_failed(descriptor, started, error.code)
                        .await?;
                    return Err(directory_unavailable());
                }
            };
            if observed.retention.fingerprint() != descriptor.profile_fingerprint {
                return Err(directory_unavailable());
            }
            if self.clock.now().get() >= preflight.request_deadline {
                return Err(expired(&preflight, self.clock.now().get()));
            }
            observed.snapshot.require_fresh(self.clock.now().get())?;
            let retention = observed.retention.clone();
            // A revoked/switching request rejects here without calling provider
            // failure handling. Successful system reconciliation grants no Work.
            self.persistence
                .reconcile_bound(proof, &preflight, observed)
                .await?;
            retention
        } else {
            self.provider.retain(descriptor).await?
        };
        if retention.fingerprint() != descriptor.profile_fingerprint {
            return Err(directory_unavailable());
        }
        if self.clock.now().get() >= preflight.request_deadline {
            return Err(expired(&preflight, self.clock.now().get()));
        }
        Ok(PreparedDirectoryWork {
            preflight,
            retention: Some(retention),
        })
    }
}
fn expired(preflight: &DirectoryRequestPreflight, now: i64) -> BusinessError {
    if now >= preflight.session_expires_at {
        BusinessError::new(
            erpx_server_shared::public::FailureKind::Unauthenticated,
            "SESSION_EXPIRED",
            "Sign in again.",
        )
    } else {
        crate::domain::work_context::context_error("WORK_ROLE_UNAVAILABLE")
    }
}
fn directory_unavailable() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Forbidden,
        "DIRECTORY_ACCESS_UNAVAILABLE",
        "Directory verification failed. Dependent access is unavailable until a successful refresh.",
    )
}
