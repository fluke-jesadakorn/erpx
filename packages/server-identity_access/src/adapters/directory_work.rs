//! Refresh is a prerequisite, never an alternate authority issuer.
use super::{
    authorized_work::{
        HoldingWorkAuthorizer, HoldingWorkSourcePreparation, IdentityWorkSourcePreparation,
    },
    directory_ldap::LdapDirectoryProvider,
    directory_postgres::{PostgresDirectory, finish, matches_descriptor, unavailable},
    directory_work_sources::DirectoryControlSources,
};
use crate::{
    application::{
        account_ports::{AccountFuture, DeviceProof},
        directory_refresh::*,
    },
    domain::{
        account_auth::{AccountPolicy, RequestAudience},
        effective_authority::{AuthorizedWorkContext, WorkAuthorizationRequest},
        work_context::HoldingWorkspaceRequest,
    },
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgRequestLease, PgTransactionError, PgWorkFuture,
};
use erpx_server_shared::public::{BusinessError, FailureKind, HoldingId};
use std::sync::Arc;

/// One configured runtime shares a strict provider-I/O budget across Holdings.
#[derive(Clone)]
pub struct DirectoryRefreshRuntime {
    directory: Arc<PostgresDirectory>,
    provider: Arc<dyn DirectoryRefreshProvider>,
}
impl DirectoryRefreshRuntime {
    pub fn new(directory: Arc<PostgresDirectory>) -> Self {
        let inner = Arc::new(LdapDirectoryProvider::new(
            directory.profiles.clone(),
            directory.clock.clone(),
        ));
        Self {
            directory,
            provider: Arc::new(BoundedRefreshProvider {
                inner,
                permits: Arc::new(tokio::sync::Semaphore::new(2)),
            }),
        }
    }
    pub fn holding(
        &self,
        id: HoldingId,
        policy: AccountPolicy,
    ) -> Result<HoldingWorkAuthorizer, BusinessError> {
        policy.validate()?;
        if policy.environment != self.directory.environment {
            return Err(unavailable());
        }
        let configured = self.directory.holdings.get(&id).ok_or_else(unavailable)?;
        let holding = configured.pool.clone();
        let authorizer = HoldingWorkAuthorizer::new(
            self.directory.control.clone(),
            holding.clone(),
            id.clone(),
            policy.clone(),
            self.directory.clock.clone(),
            self.directory.registry.clone(),
        )?;
        let persistence = Arc::new(DirectoryWorkPersistence {
            directory: self.directory.clone(),
            holding,
            holding_id: id,
            policy,
        });
        let refresh = DirectoryRequestRefresh {
            refresh: DirectoryBusinessRefresh {
                persistence: persistence.clone(),
                provider: self.provider.clone(),
                clock: self.directory.clock.clone(),
            },
            persistence,
        };
        Ok(authorizer.with_directory_refresh(Arc::new(refresh)))
    }
}
struct BoundedRefreshProvider {
    inner: Arc<LdapDirectoryProvider>,
    permits: Arc<tokio::sync::Semaphore>,
}
impl DirectoryRefreshProvider for BoundedRefreshProvider {
    fn retain<'a>(
        &'a self,
        descriptor: &'a crate::domain::directory::DirectoryConnectionDescriptor,
    ) -> AccountFuture<'a, Arc<dyn DirectoryObservationRetention>> {
        self.inner.retain(descriptor)
    }
    fn observe_bound<'a>(
        &'a self,
        descriptor: &'a crate::domain::directory::DirectoryConnectionDescriptor,
        started: i64,
    ) -> AccountFuture<'a, BoundDirectoryObservation> {
        Box::pin(async move {
            let _permit = self.permits.clone().try_acquire_owned().map_err(|_| {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "DIRECTORY_REFRESH_BUSY",
                    "Directory refresh is busy. Try this original request again.",
                )
            })?;
            self.inner.observe_bound(descriptor, started).await
        })
    }
}

pub(crate) struct DirectoryWorkInvocation<Input, Preparation, F> {
    pub host: String,
    pub proof: DeviceProof,
    pub input: Input,
    pub access: WorkAuthorizationRequest,
    pub preparation: Preparation,
    pub action: F,
}
/// This object contains only preparation ports and never owns an authorizer,
/// so configuring the concrete coordinator creates no ownership cycle.
pub(crate) struct DirectoryRequestRefresh {
    refresh: DirectoryBusinessRefresh,
    persistence: Arc<DirectoryWorkPersistence>,
}
impl DirectoryRequestRefresh {
    pub(crate) async fn execute<
        T: Send + 'static,
        F,
        Input,
        Preparation: HoldingWorkSourcePreparation,
    >(
        &self,
        authorizer: &HoldingWorkAuthorizer,
        invocation: DirectoryWorkInvocation<Input, Preparation, F>,
    ) -> Result<T, PgTransactionError<BusinessError>>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        F: for<'c> Fn(
                &'c mut sqlx::PgConnection,
                &'c AuthorizedWorkContext,
            ) -> PgWorkFuture<'c, T, BusinessError>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        let DirectoryWorkInvocation {
            host,
            proof,
            input,
            access,
            preparation,
            action,
        } = invocation;
        let host = host.as_str();
        let prepared = self
            .refresh
            .prepare(host, proof.clone(), Box::new(input))
            .await?;
        if prepared.preflight.directory.is_none() {
            return authorizer
                .execute_prepared_without_refresh(
                    host,
                    proof,
                    prepared.preflight.workspace,
                    access,
                    preparation,
                    action,
                )
                .await;
        }
        let hook = DirectoryControlSources::new(self.persistence.clone(), prepared)?;
        authorizer
            .execute_with_control_sources(
                host,
                proof,
                hook.workspace().clone(),
                access,
                IdentityWorkSourcePreparation {
                    holding: preparation,
                    control: Arc::new(hook),
                },
                action,
            )
            .await
    }
}
#[derive(Clone)]
pub(super) struct DirectoryWorkPersistence {
    pub(super) directory: Arc<PostgresDirectory>,
    pub(super) holding: DatabasePool,
    pub(super) holding_id: HoldingId,
    pub(super) policy: AccountPolicy,
}
impl DirectoryWorkPersistence {
    pub(super) fn audience(&self, host: &str) -> Result<RequestAudience, BusinessError> {
        let Some(RequestAudience::Holding { audience }) = self.policy.audience(host) else {
            return Err(missing());
        };
        let expected = self
            .directory
            .holdings
            .get(&self.holding_id)
            .ok_or_else(unavailable)?;
        if audience != expected.audience
            || audience
                != format!(
                    "holding:{}:{}:{host}",
                    self.policy.environment,
                    self.holding_id.as_str()
                )
        {
            return Err(missing());
        }
        Ok(RequestAudience::Holding { audience })
    }
}
impl DirectoryRefreshPersistence for DirectoryWorkPersistence {
    fn preflight<'a>(
        &'a self,
        host: &'a str,
        proof: DeviceProof,
        input: DirectoryWorkspaceInput,
    ) -> AccountFuture<'a, DirectoryRequestPreflight> {
        Box::pin(async move { self.read_preflight(host, proof, input).await })
    }
    fn reconcile_bound<'a>(
        &'a self,
        proof: DeviceProof,
        preflight: &'a DirectoryRequestPreflight,
        observation: BoundDirectoryObservation,
    ) -> AccountFuture<'a, ()> {
        Box::pin(async move {
            let this = self.clone();
            let preflight = preflight.clone();
            let audience = this.audience(&preflight.host)?;
            let result = PgRequestLease::new(self.directory.control.clone())
                .run(move |control| {
                    Box::pin(async move {
                        let workspace = preflight.workspace.clone();
                        let current = this
                            .read_preflight_locked(
                                control,
                                preflight.host.clone(),
                                proof,
                                Box::new(move || Ok(workspace)),
                                audience,
                            )
                            .await?;
                        same_preflight(&current, &preflight)?;
                        let source = preflight.directory.as_ref().ok_or_else(unavailable)?;
                        if observation.retention.fingerprint()
                            != source.descriptor.profile_fingerprint
                        {
                            return Err(unavailable().into());
                        }
                        this.directory
                            .apply_with_retained_generation(
                                control,
                                &source.descriptor,
                                observation.snapshot,
                                observation.retention.as_ref(),
                            )
                            .await?;
                        Ok(())
                    })
                })
                .await;
            finish(result)
        })
    }
    fn observation_failed<'a>(
        &'a self,
        descriptor: &'a crate::domain::directory::DirectoryConnectionDescriptor,
        started: i64,
        code: &'static str,
    ) -> AccountFuture<'a, ()> {
        crate::application::directory::DirectorySynchronization::failed(
            self.directory.as_ref(),
            descriptor,
            started,
            code,
        )
    }
}
pub(super) fn same_preflight(
    current: &DirectoryRequestPreflight,
    original: &DirectoryRequestPreflight,
) -> Result<(), BusinessError> {
    let a = current
        .workspace
        .expected
        .as_ref()
        .ok_or_else(unavailable)?;
    let b = original
        .workspace
        .expected
        .as_ref()
        .ok_or_else(unavailable)?;
    if current.session_id != original.session_id
        || current.user_id != original.user_id
        || current.authenticated_at != original.authenticated_at
        || current.session_expires_at != original.session_expires_at
        || current.host != original.host
        || current.workspace.company_id != original.workspace.company_id
        || current.workspace.branch_slug != original.workspace.branch_slug
        || current.workspace.department_slug != original.workspace.department_slug
        || a.assignment_id != b.assignment_id
        || a.assignment_version != b.assignment_version
        || a.revision != b.revision
    {
        return Err(changed());
    }
    match (&current.directory, &original.directory) {
        (None, None) => {}
        (Some(current), Some(original)) => {
            matches_descriptor(&current.descriptor, &original.descriptor)?;
            if current.identity_id != original.identity_id
                || current.member_version != original.member_version
                || current.external_id != original.external_id
                || current.principal_id != original.principal_id
            {
                return Err(changed());
            }
        }
        _ => return Err(changed()),
    }
    Ok(())
}
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_CONTEXT_CHANGED",
        "Your original account or work context changed. Review it before retrying.",
    )
}
fn missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "WORKSPACE_NOT_FOUND",
        "Workspace not found.",
    )
}
