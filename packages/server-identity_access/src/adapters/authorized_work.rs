use super::account_holding_bindings::ConfiguredHoldingRegistry;
use crate::{
    adapters::{
        account_sessions::verify_session_in_transaction,
        permission_catalog::permission_catalog,
        postgres_effective_authority::{current_authority_facts, lock_effective_authority_sources},
        work_admission::work_module_catalog,
    },
    application::{
        account_ports::DeviceProof,
        effective_authority::{
            InitialAdmissionFailure, authorize_effective_work, authorize_effective_work_tagged,
            inspect_selected_work, same_selected_work,
        },
    },
    domain::{
        account_auth::{AccountPolicy, RequestAudience},
        effective_authority::*,
        work_context::{HoldingWorkspaceRequest, WorkContextRequest},
    },
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgRequestLease, PgTransactionError, PgUnitOfWork, PgWorkFuture, SessionLock,
    TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, HoldingId, RecordId, UserId};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use std::{future::Future, pin::Pin, sync::Arc};

/// Owned asynchronous work created after selected Identity has been checked.
pub type SelectedIngressFuture<I, E> = Pin<Box<dyn Future<Output = Result<I, E>> + Send + 'static>>;

/// Owned transport callbacks; the denial factory sees only prepared data.
pub struct SelectedTransportCallbacks<Ingress, I, E, Action> {
    pub ingress: Ingress,
    pub read_denied: fn(&I, BusinessError) -> Result<E, BusinessError>,
    pub action: Action,
}

impl<Ingress, I, E, Action> SelectedTransportCallbacks<Ingress, I, E, Action> {
    /// Package callbacks without invoking them; the bounds supply closure expectations.
    pub fn new<T>(
        ingress: Ingress,
        read_denied: fn(&I, BusinessError) -> Result<E, BusinessError>,
        action: Action,
    ) -> Self
    where
        T: Send + 'static,
        I: Send + 'static,
        E: Send + 'static,
        Ingress: for<'s> Fn(&SelectedWorkIngress<'s>) -> SelectedIngressFuture<I, E>
            + Clone
            + Send
            + Sync
            + 'static,
        Action: for<'c> Fn(
                &'c mut PgConnection,
                &'c AuthorizedWorkContext,
                I,
            ) -> PgWorkFuture<'c, Result<T, E>, BusinessError>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        Self {
            ingress,
            read_denied,
            action,
        }
    }
}

pub(crate) trait SelectedAction<I, T, E>: Clone + Send + Sync + 'static {
    fn invoke<'c>(
        &self,
        connection: &'c mut PgConnection,
        current: &'c AuthorizedWorkContext,
        input: I,
    ) -> PgWorkFuture<'c, Result<T, E>, BusinessError>;
}
#[derive(Clone)]
struct LegacySelectedAction<F>(F);
impl<I: Send + 'static, T: Send + 'static, E: Send + 'static, F> SelectedAction<I, T, E>
    for LegacySelectedAction<F>
where
    F: for<'c> Fn(
            &'c mut PgConnection,
            &'c AuthorizedWorkContext,
            I,
        ) -> PgWorkFuture<'c, T, BusinessError>
        + Clone
        + Send
        + Sync
        + 'static,
{
    fn invoke<'c>(
        &self,
        c: &'c mut PgConnection,
        current: &'c AuthorizedWorkContext,
        input: I,
    ) -> PgWorkFuture<'c, Result<T, E>, BusinessError> {
        let future = (self.0)(c, current, input);
        Box::pin(async move { future.await.map(Ok) })
    }
}
#[derive(Clone)]
struct TransportSelectedAction<F>(F);
impl<I: Send + 'static, T: Send + 'static, E: Send + 'static, F> SelectedAction<I, T, E>
    for TransportSelectedAction<F>
where
    F: for<'c> Fn(
            &'c mut PgConnection,
            &'c AuthorizedWorkContext,
            I,
        ) -> PgWorkFuture<'c, Result<T, E>, BusinessError>
        + Clone
        + Send
        + Sync
        + 'static,
{
    fn invoke<'c>(
        &self,
        c: &'c mut PgConnection,
        current: &'c AuthorizedWorkContext,
        input: I,
    ) -> PgWorkFuture<'c, Result<T, E>, BusinessError> {
        (self.0)(c, current, input)
    }
}

/// Borrowed selected-identity observation, with no permission-bearing actor.
pub struct SelectedWorkIngress<'a> {
    holding_id: &'a HoldingId,
    company_id: &'a erpx_server_shared::public::CompanyId,
}
impl SelectedWorkIngress<'_> {
    pub fn holding_id(&self) -> &HoldingId {
        self.holding_id
    }
    pub fn company_id(&self) -> &erpx_server_shared::public::CompanyId {
        self.company_id
    }
}

/// Owner-provided utility source fences run before the first Holding snapshot.
/// Implementations acquire locks only; they do not query facts or grant access.
pub trait HoldingWorkSourcePreparation: Clone + Send + Sync + 'static {
    fn prepare<'c>(
        &'c self,
        connection: &'c mut PgConnection,
    ) -> PgWorkFuture<'c, (), BusinessError>;
}
#[derive(Clone)]
pub(crate) struct IamSourcesOnly;
impl HoldingWorkSourcePreparation for IamSourcesOnly {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

/// Additional Identity-internal source constraints. They can only narrow the
/// normal IAM context; no copied selector or source object can mint authority.
pub(crate) trait WorkControlSources: Send + Sync {
    fn prepare_control<'c>(
        &'c self,
        c: &'c mut PgConnection,
    ) -> PgWorkFuture<'c, (), BusinessError>;
    fn retain_control<'c>(
        &'c self,
        c: &'c mut PgConnection,
        session: &'c crate::domain::account_auth::AccountSession,
        input: &'c HoldingWorkspaceRequest,
        now: i64,
    ) -> PgWorkFuture<'c, Arc<dyn RetainedWorkControlSources>, BusinessError>;
}
pub(crate) trait RetainedWorkControlSources: Send + Sync {
    fn prepare_holding<'c>(
        &'c self,
        c: &'c mut PgConnection,
    ) -> PgWorkFuture<'c, (), BusinessError>;
    fn retain_holding<'c>(
        &'c self,
        c: &'c mut PgConnection,
        request: &'c WorkContextRequest,
        now: i64,
    ) -> PgWorkFuture<'c, Box<dyn RetainedWorkHoldingSources>, BusinessError>;
    fn require_current<'c>(
        &'c self,
        c: &'c mut PgConnection,
        now: i64,
    ) -> PgWorkFuture<'c, (), BusinessError>;
}
pub(crate) trait RetainedWorkHoldingSources: Send + Sync {
    #[cfg(feature = "directory-ldaps")]
    fn directory_administration(
        &self,
    ) -> Option<crate::domain::directory_source_review::DirectoryAdministrationProof> {
        None
    }
    fn deadline(&self) -> i64;
    fn require_current<'c>(
        &'c self,
        c: &'c mut PgConnection,
        now: i64,
    ) -> PgWorkFuture<'c, (), BusinessError>;
}

struct WorkSourceBundle<Preparation> {
    holding: Preparation,
    control: Option<Arc<dyn WorkControlSources>>,
}
#[cfg(feature = "directory-ldaps")]
pub(crate) struct IdentityWorkSourcePreparation<Preparation> {
    pub holding: Preparation,
    pub control: Arc<dyn WorkControlSources>,
}
/// Owns transaction freshness, retries only the Holding operation, and retains
/// Control parent/device proof through its commit. This is not an HTTP router.
pub struct HoldingWorkAuthorizer {
    control: PgRequestLease,
    holding: PgUnitOfWork,
    holding_id: HoldingId,
    policy: AccountPolicy,
    holdings: Arc<ConfiguredHoldingRegistry>,
    clock: Arc<dyn Clock + Send + Sync>,
    #[cfg(feature = "directory-ldaps")]
    directory_refresh: Option<Arc<crate::adapters::directory_work::DirectoryRequestRefresh>>,
}
impl HoldingWorkAuthorizer {
    pub fn new(
        control: DatabasePool,
        holding: DatabasePool,
        holding_id: HoldingId,
        policy: AccountPolicy,
        clock: Arc<dyn Clock + Send + Sync>,
        holdings: Arc<ConfiguredHoldingRegistry>,
    ) -> Result<Self, BusinessError> {
        policy.validate()?;
        Ok(Self {
            control: PgRequestLease::new(control),
            holding: PgUnitOfWork::new(holding),
            holding_id,
            policy,
            holdings,
            clock,
            #[cfg(feature = "directory-ldaps")]
            directory_refresh: None,
        })
    }
    #[cfg(feature = "directory-ldaps")]
    pub(crate) fn with_directory_refresh(
        mut self,
        refresh: Arc<crate::adapters::directory_work::DirectoryRequestRefresh>,
    ) -> Self {
        self.directory_refresh = Some(refresh);
        self
    }
    /// The callback may run four times. It must write effects/outbox/receipts on
    /// this borrowed connection, and must not make external effects or commit it.
    /// Its context is borrowed, cannot escape in T, and is checked again after
    /// the callback before the only Holding commit.
    pub async fn execute<T: Send + 'static, F>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: HoldingWorkspaceRequest,
        access: WorkAuthorizationRequest,
        action: F,
    ) -> Result<T, PgTransactionError<BusinessError>>
    where
        F: for<'c> Fn(
                &'c mut PgConnection,
                &'c AuthorizedWorkContext,
            ) -> PgWorkFuture<'c, T, BusinessError>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        self.execute_with_sources(host, proof, input, access, IamSourcesOnly, action)
            .await
    }
    /// Participating owners must use this form to fence denial/phantom sources
    /// before IAM facts establish the transaction snapshot.
    pub async fn execute_with_sources<
        T: Send + 'static,
        F,
        Preparation: HoldingWorkSourcePreparation,
    >(
        &self,
        host: &str,
        proof: DeviceProof,
        input: HoldingWorkspaceRequest,
        access: WorkAuthorizationRequest,
        preparation: Preparation,
        action: F,
    ) -> Result<T, PgTransactionError<BusinessError>>
    where
        F: for<'c> Fn(
                &'c mut PgConnection,
                &'c AuthorizedWorkContext,
            ) -> PgWorkFuture<'c, T, BusinessError>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        self.execute_with_sources_deferred(
            host,
            proof,
            move || Ok(input),
            access,
            preparation,
            action,
        )
        .await
    }
    /// Resolve only transport selectors after the actual Account/Holding session
    /// is verified on the retained Control lease. No input can mint an Actor;
    /// all current IAM/owner policy checks and final retention remain canonical.
    pub async fn execute_with_sources_deferred<
        T: Send + 'static,
        F,
        Input,
        Preparation: HoldingWorkSourcePreparation,
    >(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        access: WorkAuthorizationRequest,
        preparation: Preparation,
        action: F,
    ) -> Result<T, PgTransactionError<BusinessError>>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        F: for<'c> Fn(
                &'c mut PgConnection,
                &'c AuthorizedWorkContext,
            ) -> PgWorkFuture<'c, T, BusinessError>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        #[cfg(feature = "directory-ldaps")]
        if let Some(refresh) = &self.directory_refresh {
            return refresh
                .execute(
                    self,
                    crate::adapters::directory_work::DirectoryWorkInvocation {
                        host: host.to_owned(),
                        proof,
                        input,
                        access,
                        preparation,
                        action,
                    },
                )
                .await;
        }
        self.execute_internal(
            host,
            proof,
            input,
            access,
            WorkSourceBundle {
                holding: preparation,
                control: None,
            },
            action,
        )
        .await
    }
    #[cfg(feature = "directory-ldaps")]
    pub(crate) async fn execute_prepared_without_refresh<
        T: Send + 'static,
        F,
        Preparation: HoldingWorkSourcePreparation,
    >(
        &self,
        host: &str,
        proof: DeviceProof,
        input: HoldingWorkspaceRequest,
        access: WorkAuthorizationRequest,
        preparation: Preparation,
        action: F,
    ) -> Result<T, PgTransactionError<BusinessError>>
    where
        F: for<'c> Fn(
                &'c mut PgConnection,
                &'c AuthorizedWorkContext,
            ) -> PgWorkFuture<'c, T, BusinessError>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        self.execute_internal(
            host,
            proof,
            move || Ok(input),
            access,
            WorkSourceBundle {
                holding: preparation,
                control: None,
            },
            action,
        )
        .await
    }
    #[cfg(feature = "directory-ldaps")]
    pub(crate) async fn execute_with_control_sources<
        T: Send + 'static,
        F,
        Preparation: HoldingWorkSourcePreparation,
    >(
        &self,
        host: &str,
        proof: DeviceProof,
        input: HoldingWorkspaceRequest,
        access: WorkAuthorizationRequest,
        sources: IdentityWorkSourcePreparation<Preparation>,
        action: F,
    ) -> Result<T, PgTransactionError<BusinessError>>
    where
        F: for<'c> Fn(
                &'c mut PgConnection,
                &'c AuthorizedWorkContext,
            ) -> PgWorkFuture<'c, T, BusinessError>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        self.execute_with_control_sources_deferred(
            host,
            proof,
            move || Ok(input),
            access,
            sources,
            action,
        )
        .await
    }
    #[cfg(feature = "directory-ldaps")]
    pub(crate) async fn execute_with_control_sources_deferred<
        T: Send + 'static,
        F,
        Input,
        Preparation: HoldingWorkSourcePreparation,
    >(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        access: WorkAuthorizationRequest,
        sources: IdentityWorkSourcePreparation<Preparation>,
        action: F,
    ) -> Result<T, PgTransactionError<BusinessError>>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        F: for<'c> Fn(
                &'c mut PgConnection,
                &'c AuthorizedWorkContext,
            ) -> PgWorkFuture<'c, T, BusinessError>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        self.execute_internal(
            host,
            proof,
            input,
            access,
            WorkSourceBundle {
                holding: sources.holding,
                control: Some(sources.control),
            },
            action,
        )
        .await
    }
    async fn execute_internal<
        T: Send + 'static,
        F,
        Input,
        Preparation: HoldingWorkSourcePreparation,
    >(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        access: WorkAuthorizationRequest,
        sources: WorkSourceBundle<Preparation>,
        action: F,
    ) -> Result<T, PgTransactionError<BusinessError>>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        F: for<'c> Fn(
                &'c mut PgConnection,
                &'c AuthorizedWorkContext,
            ) -> PgWorkFuture<'c, T, BusinessError>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        let WorkSourceBundle {
            holding: preparation,
            control: control_sources,
        } = sources;
        let Some(RequestAudience::Holding { audience }) = self.policy.audience(host) else {
            return Err(missing().into());
        };
        if audience
            != format!(
                "holding:{}:{}:{host}",
                self.policy.environment,
                self.holding_id.as_str()
            )
        {
            return Err(missing().into());
        }
        let audience = RequestAudience::Holding { audience };
        let environment = self.policy.environment.clone();
        let holding = self.holding.clone();
        let holding_id = self.holding_id.clone();
        let clock = self.clock.clone();
        let holdings = self.holdings.clone();
        self.control
            .run(move |connection| {
                Box::pin(async move {
                    if let Some(source) = &control_sources {
                        crate::adapters::account_sessions::lock_session_reads(connection).await?;
                        source.prepare_control(connection).await?;
                    }
                    let session = verify_session_in_transaction(
                        connection,
                        &proof,
                        &audience,
                        &environment,
                        clock.now().get(),
                        &holdings,
                    )
                    .await?
                    .ok_or_else(|| {
                        BusinessError::new(
                            FailureKind::Unauthenticated,
                            "UNAUTHENTICATED",
                            "Sign in to continue.",
                        )
                    })?;
                    let binding = holdings.read_current(connection, &audience).await?;
                    let input = input()?;
                    let retained_control = match &control_sources {
                        Some(source) => Some(
                            source
                                .retain_control(connection, &session, &input, clock.now().get())
                                .await?,
                        ),
                        None => None,
                    };
                    let request = WorkContextRequest {
                        holding_id: holding_id.clone(),
                        company_id: input.company_id,
                        branch_slug: input.branch_slug,
                        department_slug: input.department_slug,
                        session_id: RecordId::new(session.id)?,
                        user_id: UserId::new(session.user.id)?,
                        session_expires_at: session.expires_at,
                        expected: input.expected,
                        mutation: input.mutation,
                    };
                    let key = hex::encode(Sha256::digest(
                        format!(
                            "{}:{}{}:{}",
                            request.session_id.as_str().len(),
                            request.session_id.as_str(),
                            request.company_id.as_str().len(),
                            request.company_id.as_str()
                        )
                        .as_bytes(),
                    ));
                    let holding_sources = retained_control.clone();
                    let final_clock = clock.clone();
                    let result = holding
                        .run(
                            TransactionOptions {
                                session_lock: Some(SessionLock {
                                    namespace: "work-context".into(),
                                    key,
                                }),
                                prefer_indexed_reads: false,
                            },
                            move |connection| {
                                let request = request.clone();
                                let access = access.clone();
                                let clock = clock.clone();
                                let action = action.clone();
                                let preparation = preparation.clone();
                                let retained_control = holding_sources.clone();
                                Box::pin(async move {
                                    // Nothing may query this Holding transaction before all source locks.
                                    lock_effective_authority_sources(&mut *connection).await?;
                                    if let Some(source) = &retained_control {
                                        source.prepare_holding(connection).await?;
                                    }
                                    preparation.prepare(&mut *connection).await?;
                                    let retained_holding = match &retained_control {
                                        Some(source) => Some(
                                            source
                                                .retain_holding(
                                                    connection,
                                                    &request,
                                                    clock.now().get(),
                                                )
                                                .await?,
                                        ),
                                        None => None,
                                    };
                                    let facts = current_authority_facts(
                                        &mut *connection,
                                        &request,
                                        clock.now().get(),
                                    )
                                    .await?;
                                    let catalog = permission_catalog()?;
                                    let modules = work_module_catalog()?;
                                    let mut context = authorize_effective_work(
                                        &request,
                                        &access,
                                        facts,
                                        catalog,
                                        modules,
                                        clock.now().get(),
                                    )?;
                                    if let Some(source) = &retained_holding {
                                        #[cfg(feature = "directory-ldaps")]
                                        {
                                            context.directory_administration =
                                                source.directory_administration();
                                        }
                                        context.expires_at =
                                            context.expires_at.min(source.deadline());
                                        context.require_current_time(clock.now().get())?;
                                    }
                                    let result = action(&mut *connection, &context).await?;
                                    if let Some(source) = &retained_holding {
                                        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
                                            .execute(&mut *connection)
                                            .await?;
                                        source
                                            .require_current(connection, clock.now().get())
                                            .await?;
                                    }
                                    context.require_current_time(clock.now().get())?;
                                    let facts = current_authority_facts(
                                        &mut *connection,
                                        &request,
                                        clock.now().get(),
                                    )
                                    .await?;
                                    let mut final_context = authorize_effective_work(
                                        &request,
                                        &access,
                                        facts,
                                        catalog,
                                        modules,
                                        clock.now().get(),
                                    )?;
                                    if let Some(source) = &retained_holding {
                                        final_context.expires_at =
                                            final_context.expires_at.min(source.deadline());
                                        final_context.require_current_time(clock.now().get())?;
                                    }
                                    if !same_authority(&context, &final_context) {
                                        return Err(authority_error(
                                            "WORK_AUTHORITY_CHANGED",
                                            "Your current work authority changed.",
                                        )
                                        .into());
                                    }
                                    Ok(result)
                                })
                            },
                        )
                        .await?;
                    binding.require_current(connection).await?;
                    if let Some(source) = retained_control {
                        source
                            .require_current(connection, final_clock.now().get())
                            .await?;
                    }
                    Ok(result)
                })
            })
            .await
    }
    /// Run deterministic schema ingress only after current selected Identity, and
    /// grant the normal Work context only after schema succeeds.
    pub async fn execute_with_selected_ingress_deferred<T, I, E, F, Ingress, Input, Preparation>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        access_and_preparation: (WorkAuthorizationRequest, Preparation),
        ingress: Ingress,
        action: F,
    ) -> Result<Result<T, E>, PgTransactionError<BusinessError>>
    where
        T: Send + 'static,
        I: Send + 'static,
        E: Send + 'static,
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Preparation: HoldingWorkSourcePreparation,
        Ingress:
            for<'c> Fn(&SelectedWorkIngress<'c>) -> Result<I, E> + Clone + Send + Sync + 'static,
        F: for<'c> Fn(
                &'c mut PgConnection,
                &'c AuthorizedWorkContext,
                I,
            ) -> PgWorkFuture<'c, T, BusinessError>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        let owned_ingress =
            move |selected: &SelectedWorkIngress<'_>| -> SelectedIngressFuture<I, E> {
                let result = ingress(selected);
                Box::pin(async move { result })
            };
        self.execute_selected_transport_core(
            host,
            proof,
            input,
            access_and_preparation,
            SelectedTransportCallbacks {
                ingress: owned_ingress,
                read_denied: |_, error| Err(error),
                action: LegacySelectedAction(action),
            },
        )
        .await
    }
    /// Await owned request transport only after current selected Identity. The
    /// returned future cannot retain the borrowed witness or a connection.
    pub async fn execute_with_selected_transport_ingress_deferred<
        T,
        I,
        E,
        F,
        Ingress,
        Input,
        Preparation,
    >(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        access_and_preparation: (WorkAuthorizationRequest, Preparation),
        callbacks: SelectedTransportCallbacks<Ingress, I, E, F>,
    ) -> Result<Result<T, E>, PgTransactionError<BusinessError>>
    where
        T: Send + 'static,
        I: Send + 'static,
        E: Send + 'static,
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Preparation: HoldingWorkSourcePreparation,
        Ingress: for<'c> Fn(&SelectedWorkIngress<'c>) -> SelectedIngressFuture<I, E>
            + Clone
            + Send
            + Sync
            + 'static,
        F: for<'c> Fn(
                &'c mut PgConnection,
                &'c AuthorizedWorkContext,
                I,
            ) -> PgWorkFuture<'c, Result<T, E>, BusinessError>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        let SelectedTransportCallbacks {
            ingress,
            read_denied,
            action,
        } = callbacks;
        self.execute_selected_transport_core(
            host,
            proof,
            input,
            access_and_preparation,
            SelectedTransportCallbacks {
                ingress,
                read_denied,
                action: TransportSelectedAction(action),
            },
        )
        .await
    }
    async fn execute_selected_transport_core<T, I, E, A, Ingress, Input, Preparation>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        access_and_preparation: (WorkAuthorizationRequest, Preparation),
        callbacks: SelectedTransportCallbacks<Ingress, I, E, A>,
    ) -> Result<Result<T, E>, PgTransactionError<BusinessError>>
    where
        T: Send + 'static,
        I: Send + 'static,
        E: Send + 'static,
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Preparation: HoldingWorkSourcePreparation,
        Ingress: for<'c> Fn(&SelectedWorkIngress<'c>) -> SelectedIngressFuture<I, E>
            + Clone
            + Send
            + Sync
            + 'static,
        A: SelectedAction<I, T, E>,
    {
        let (access, preparation) = access_and_preparation;
        #[cfg(feature = "directory-ldaps")]
        if let Some(refresh) = &self.directory_refresh {
            return refresh
                .execute_selected(self, host, proof, input, (access, preparation), callbacks)
                .await;
        }
        self.execute_selected_internal(
            host,
            proof,
            input,
            (
                access,
                WorkSourceBundle {
                    holding: preparation,
                    control: None,
                },
            ),
            callbacks,
        )
        .await
    }
    #[cfg(feature = "directory-ldaps")]
    pub(crate) async fn execute_selected_prepared<T, I, E, F, Ingress, Preparation>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: HoldingWorkspaceRequest,
        policy_sources: (
            WorkAuthorizationRequest,
            Preparation,
            Option<Arc<dyn WorkControlSources>>,
        ),
        callbacks: SelectedTransportCallbacks<Ingress, I, E, F>,
    ) -> Result<Result<T, E>, PgTransactionError<BusinessError>>
    where
        T: Send + 'static,
        I: Send + 'static,
        E: Send + 'static,
        Preparation: HoldingWorkSourcePreparation,
        Ingress: for<'c> Fn(&SelectedWorkIngress<'c>) -> SelectedIngressFuture<I, E>
            + Clone
            + Send
            + Sync
            + 'static,
        F: SelectedAction<I, T, E>,
    {
        let (access, preparation, control) = policy_sources;
        self.execute_selected_internal(
            host,
            proof,
            move || Ok(input),
            (
                access,
                WorkSourceBundle {
                    holding: preparation,
                    control,
                },
            ),
            callbacks,
        )
        .await
    }
    async fn execute_selected_internal<
        T: Send + 'static,
        I: Send + 'static,
        E: Send + 'static,
        F,
        Ingress,
        Input,
        Preparation: HoldingWorkSourcePreparation,
    >(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        policy_sources: (WorkAuthorizationRequest, WorkSourceBundle<Preparation>),
        callbacks: SelectedTransportCallbacks<Ingress, I, E, F>,
    ) -> Result<Result<T, E>, PgTransactionError<BusinessError>>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Ingress: for<'c> Fn(&SelectedWorkIngress<'c>) -> SelectedIngressFuture<I, E>
            + Clone
            + Send
            + Sync
            + 'static,
        F: SelectedAction<I, T, E>,
    {
        let SelectedTransportCallbacks {
            ingress,
            read_denied,
            action,
        } = callbacks;
        let (access, sources) = policy_sources;
        let WorkSourceBundle {
            holding: preparation,
            control: control_sources,
        } = sources;
        let Some(RequestAudience::Holding { audience }) = self.policy.audience(host) else {
            return Err(missing().into());
        };
        if audience
            != format!(
                "holding:{}:{}:{host}",
                self.policy.environment,
                self.holding_id.as_str()
            )
        {
            return Err(missing().into());
        }
        let audience = RequestAudience::Holding { audience };
        let environment = self.policy.environment.clone();
        let holding = self.holding.clone();
        let holding_id = self.holding_id.clone();
        let clock = self.clock.clone();
        let holdings = self.holdings.clone();
        self.control
            .run(move |connection| {
                Box::pin(async move {
                    if let Some(source) = &control_sources {
                        crate::adapters::account_sessions::lock_session_reads(connection).await?;
                        source.prepare_control(connection).await?;
                    }
                    let session = verify_session_in_transaction(
                        connection,
                        &proof,
                        &audience,
                        &environment,
                        clock.now().get(),
                        &holdings,
                    )
                    .await?
                    .ok_or_else(|| {
                        BusinessError::new(
                            FailureKind::Unauthenticated,
                            "UNAUTHENTICATED",
                            "Sign in to continue.",
                        )
                    })?;
                    let binding = holdings.read_current(connection, &audience).await?;
                    let input = input()?;
                    let retained_control = match &control_sources {
                        Some(source) => Some(
                            source
                                .retain_control(connection, &session, &input, clock.now().get())
                                .await?,
                        ),
                        None => None,
                    };
                    let request = WorkContextRequest {
                        holding_id: holding_id.clone(),
                        company_id: input.company_id,
                        branch_slug: input.branch_slug,
                        department_slug: input.department_slug,
                        session_id: RecordId::new(session.id)?,
                        user_id: UserId::new(session.user.id)?,
                        session_expires_at: session.expires_at,
                        expected: input.expected,
                        mutation: input.mutation,
                    };
                    let key = hex::encode(Sha256::digest(
                        format!(
                            "{}:{}{}:{}",
                            request.session_id.as_str().len(),
                            request.session_id.as_str(),
                            request.company_id.as_str().len(),
                            request.company_id.as_str()
                        )
                        .as_bytes(),
                    ));
                    let holding_sources = retained_control.clone();
                    let final_clock = clock.clone();
                    let result = holding
                        .run(
                            TransactionOptions {
                                session_lock: Some(SessionLock {
                                    namespace: "work-context".into(),
                                    key,
                                }),
                                prefer_indexed_reads: false,
                            },
                            move |connection| {
                                let request = request.clone();
                                let access = access.clone();
                                let clock = clock.clone();
                                let action = action.clone();
                                let ingress = ingress.clone();
                                let preparation = preparation.clone();
                                let retained_control = holding_sources.clone();
                                Box::pin(async move {
                                    // Nothing may query this Holding transaction before all source locks.
                                    lock_effective_authority_sources(&mut *connection).await?;
                                    if let Some(source) = &retained_control {
                                        source.prepare_holding(connection).await?;
                                    }
                                    preparation.prepare(&mut *connection).await?;
                                    let retained_holding = match &retained_control {
                                        Some(source) => Some(
                                            source
                                                .retain_holding(
                                                    connection,
                                                    &request,
                                                    clock.now().get(),
                                                )
                                                .await?,
                                        ),
                                        None => None,
                                    };
                                    let facts = current_authority_facts(
                                        &mut *connection,
                                        &request,
                                        clock.now().get(),
                                    )
                                    .await?;
                                    let mut deadline = inspect_selected_work(&facts, &access, clock.now().get())?;
                                    if let Some(source) = &retained_holding { deadline = deadline.min(source.deadline()); }
                                    if clock.now().get() >= deadline { return Err(authority_error("WORK_AUTHORITY_EXPIRED", "Your work authority expired. Review your current access.").into()); }
                                    let witness = SelectedWorkIngress { holding_id: &request.holding_id, company_id: &facts.selection.workspace().company_id };
                                    let initial = match ingress(&witness).await {
                                        Ok(value) => {
                                            let catalog = permission_catalog()?;
                                            let modules = work_module_catalog()?;
                                            match authorize_effective_work_tagged(&request, &access, facts.clone(), catalog, modules, clock.now().get()) {
                                                Ok(context) => Ok((value, context)),
                                                Err(InitialAdmissionFailure::PermissionRequired(error)) => match read_denied(&value, error) {
                                                    Ok(local) => Err(local),
                                                    Err(error) => return Err(error.into()),
                                                },
                                                Err(InitialAdmissionFailure::Other(error)) => return Err(error.into()),
                                            }
                                        },
                                        Err(error) => Err(error),
                                    };
                                    let (normalized, mut context) = match initial {
                                        Ok(value) => value,
                                        Err(error) => {
                                            if let Some(source) = &retained_holding {
                                                sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *connection).await?;
                                                source.require_current(connection, clock.now().get()).await?;
                                            }
                                            if clock.now().get() >= deadline { return Err(authority_error("WORK_AUTHORITY_EXPIRED", "Your work authority expired. Review your current access.").into()); }
                                            let final_facts = current_authority_facts(&mut *connection, &request, clock.now().get()).await?;
                                            let mut final_deadline = inspect_selected_work(&final_facts, &access, clock.now().get())?;
                                            if let Some(source) = &retained_holding { final_deadline = final_deadline.min(source.deadline()); }
                                            if clock.now().get() >= final_deadline { return Err(authority_error("WORK_AUTHORITY_EXPIRED", "Your work authority expired. Review your current access.").into()); }
                                            if final_deadline != deadline || !same_selected_work(&facts, &final_facts) {
                                                return Err(authority_error("WORK_AUTHORITY_CHANGED", "Your current work authority changed.").into());
                                            }
                                            return Ok(Err(error));
                                        }
                                    };
                                    let catalog = permission_catalog()?;
                                    let modules = work_module_catalog()?;
                                    if let Some(source) = &retained_holding {
                                        #[cfg(feature = "directory-ldaps")]
                                        {
                                            context.directory_administration =
                                                source.directory_administration();
                                        }
                                        context.expires_at =
                                            context.expires_at.min(source.deadline());
                                        context.require_current_time(clock.now().get())?;
                                    }
                                    let result = action.invoke(&mut *connection, &context, normalized).await?;
                                    if let Some(source) = &retained_holding {
                                        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
                                            .execute(&mut *connection)
                                            .await?;
                                        source
                                            .require_current(connection, clock.now().get())
                                            .await?;
                                    }
                                    context.require_current_time(clock.now().get())?;
                                    let facts = current_authority_facts(
                                        &mut *connection,
                                        &request,
                                        clock.now().get(),
                                    )
                                    .await?;
                                    let mut final_context = authorize_effective_work(
                                        &request,
                                        &access,
                                        facts,
                                        catalog,
                                        modules,
                                        clock.now().get(),
                                    )?;
                                    if let Some(source) = &retained_holding {
                                        final_context.expires_at =
                                            final_context.expires_at.min(source.deadline());
                                        final_context.require_current_time(clock.now().get())?;
                                    }
                                    if !same_authority(&context, &final_context) {
                                        return Err(authority_error(
                                            "WORK_AUTHORITY_CHANGED",
                                            "Your current work authority changed.",
                                        )
                                        .into());
                                    }
                                    Ok(result)
                                })
                            },
                        )
                        .await?;
                    binding.require_current(connection).await?;
                    if let Some(source) = retained_control {
                        source
                            .require_current(connection, final_clock.now().get())
                            .await?;
                    }
                    Ok(result)
                })
            })
            .await
    }
}
pub(crate) fn same_authority(a: &AuthorizedWorkContext, b: &AuthorizedWorkContext) -> bool {
    a.user_id == b.user_id
        && a.session_id == b.session_id
        && a.holding_id == b.holding_id
        && a.selection.assignment().id == b.selection.assignment().id
        && a.selection.assignment().version == b.selection.assignment().version
        && a.selection.revision() == b.selection.revision()
        && a.selection.role_revision() == b.selection.role_revision()
        && a.identity_version == b.identity_version
        && a.hr_appointment_version == b.hr_appointment_version
        && a.emergency_until == b.emergency_until
        && a.principal_id == b.principal_id
        && a.person_id == b.person_id
        && a.person_user_ids == b.person_user_ids
        && a.permissions == b.permissions
        && a.grantable_permissions == b.grantable_permissions
        && a.modules == b.modules
        && a.expires_at == b.expires_at
}
fn missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "WORKSPACE_NOT_FOUND",
        "Workspace not found.",
    )
}
