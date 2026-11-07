//! Lifecycle assembly for selected-grant inspection. It does not authorize an owner mutation.
use super::account_holding_bindings::ConfiguredHoldingRegistry;
use crate::{
    adapters::{
        account_sessions::verify_session_in_transaction, postgres_work_context::PgWorkContextReader,
    },
    application::{account_ports::DeviceProof, work_context::inspect_current_work_selection},
    domain::{
        account_auth::{AccountPolicy, RequestAudience},
        work_context::{CurrentWorkSelection, HoldingWorkspaceRequest, WorkContextRequest},
    },
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgRequestLease, PgTransactionError, PgUnitOfWork, SessionLock, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, HoldingId, RecordId, UserId};
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub struct HoldingWorkContextInspector {
    control: PgRequestLease,
    holding: PgUnitOfWork,
    holding_id: HoldingId,
    policy: AccountPolicy,
    holdings: Arc<ConfiguredHoldingRegistry>,
    clock: Arc<dyn Clock + Send + Sync>,
}
impl HoldingWorkContextInspector {
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
        })
    }
    /// Exact configured Holding host only; there is no Account/foreign-host fallback.
    /// The retained Control transaction fences parent session/device/principal changes
    /// while the inner Holding SERIALIZABLE calculation retries independently.
    pub async fn inspect(
        &self,
        host: &str,
        proof: DeviceProof,
        input: HoldingWorkspaceRequest,
    ) -> Result<CurrentWorkSelection, PgTransactionError<BusinessError>> {
        let Some(RequestAudience::Holding { audience }) = self.policy.audience(host) else {
            return Err(missing().into());
        };
        let expected_audience = format!(
            "holding:{}:{}:{host}",
            self.policy.environment,
            self.holding_id.as_str()
        );
        if audience != expected_audience {
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
                    let options = TransactionOptions {
                        session_lock: Some(SessionLock {
                            namespace: "work-context".into(),
                            key: hex::encode(Sha256::digest(
                                format!(
                                    "{}:{}{}:{}",
                                    request.session_id.as_str().len(),
                                    request.session_id.as_str(),
                                    request.company_id.as_str().len(),
                                    request.company_id.as_str()
                                )
                                .as_bytes(),
                            )),
                        }),
                        prefer_indexed_reads: false,
                    };
                    let result = holding
                        .run(options, move |connection| {
                            let request = request.clone();
                            let clock = clock.clone();
                            let holding_id = holding_id.clone();
                            Box::pin(async move {
                                let mut port = PgWorkContextReader::new(holding_id, connection);
                                inspect_current_work_selection(&mut port, clock.as_ref(), &request)
                                    .await
                            })
                        })
                        .await?;
                    binding.require_current(connection).await?;
                    Ok(result)
                })
            })
            .await
    }
}
/// A verified Account/Holding identity borrowed in one Holding transaction.
/// This is not selected-role or business permission authority. Constructors and
/// transaction identity stay private; no serialized actor can replace it.
pub struct HoldingAccountWorkScope {
    holding_name: String,
    request: WorkContextRequest,
    unit: (String, i32, String, String),
}
impl HoldingAccountWorkScope {
    pub fn holding_name(&self) -> &str {
        &self.holding_name
    }
    pub fn request(&self) -> &WorkContextRequest {
        &self.request
    }
    pub async fn require_current(
        &self,
        c: &mut sqlx::PgConnection,
        now: i64,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        if now >= self.request.session_expires_at {
            return Err(BusinessError::new(
                FailureKind::Unauthenticated,
                "SESSION_EXPIRED",
                "Sign in again.",
            )
            .into());
        }
        if super::selected_command_authority::selected_unit(c).await? != self.unit {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "The original work selection transaction changed.",
            )
            .into());
        }
        Ok(())
    }
}
/// Current Account identity in exactly one verified Holding transaction. It
/// carries neither a Company nor a selected work role and cannot grant business access.
pub struct HoldingAccountScope {
    holding_name: String,
    holding_id: HoldingId,
    session_id: RecordId,
    user_id: UserId,
    user: crate::domain::account_auth::AccountUser,
    expires_at: i64,
    unit: (String, i32, String, String),
}
impl HoldingAccountScope {
    pub fn holding_id(&self) -> &HoldingId {
        &self.holding_id
    }
    pub fn session_id(&self) -> &RecordId {
        &self.session_id
    }
    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }
    pub fn user(&self) -> &crate::domain::account_auth::AccountUser {
        &self.user
    }
    pub fn expires_at(&self) -> i64 {
        self.expires_at
    }
    pub async fn require_current(
        &self,
        c: &mut sqlx::PgConnection,
        now: i64,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        if now >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Unauthenticated,
                "SESSION_EXPIRED",
                "Sign in again.",
            )
            .into());
        }
        if super::selected_command_authority::selected_unit(c).await? != self.unit {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "The original account transaction changed.",
            )
            .into());
        }
        Ok(())
    }
    /// An untrusted workspace selector. No grant is minted; Identity's current
    /// Company/grant reader must still validate it in this same transaction.
    pub fn workspace(&self, input: HoldingWorkspaceRequest) -> HoldingAccountWorkScope {
        HoldingAccountWorkScope {
            holding_name: self.holding_name.clone(),
            request: WorkContextRequest {
                holding_id: self.holding_id.clone(),
                company_id: input.company_id,
                branch_slug: input.branch_slug,
                department_slug: input.department_slug,
                session_id: self.session_id.clone(),
                user_id: self.user_id.clone(),
                session_expires_at: self.expires_at,
                expected: input.expected,
                mutation: input.mutation,
            },
            unit: self.unit.clone(),
        }
    }
}
impl HoldingWorkContextInspector {
    /// Selected-work initialization borrows the original authenticated Holding
    /// account scope. Organization and IAM still validate the requested Company.
    pub async fn with_account_work<Output, Sources, Work>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: HoldingWorkspaceRequest,
        sources: Sources,
        work: Work,
    ) -> Result<Output, PgTransactionError<BusinessError>>
    where
        Output: Send + 'static,
        Sources: super::authorized_work::HoldingWorkSourcePreparation + Send + Sync + 'static,
        Work: for<'c> Fn(
                &'c mut sqlx::PgConnection,
                &'c HoldingAccountWorkScope,
            ) -> erpx_server_runtime::public::postgres::PgWorkFuture<
                'c,
                Output,
                BusinessError,
            > + Send
            + Sync
            + 'static,
    {
        let work = Arc::new(work);
        self.with_holding_account(host, proof, sources, move |c, account| {
            let input = input.clone();
            let scope = account.workspace(input);
            let work = work.clone();
            Box::pin(async move { work(c, &scope).await })
        })
        .await
    }
}
impl HoldingWorkContextInspector {
    /// Account-only Company/role recovery must remain available before any role
    /// is selected. The outer composition supplies actual owner ports; this
    /// coordinator never imports Work Projection or writes its preference data.
    pub async fn with_holding_account<Output, Sources, Work>(
        &self,
        host: &str,
        proof: DeviceProof,
        sources: Sources,
        work: Work,
    ) -> Result<Output, PgTransactionError<BusinessError>>
    where
        Output: Send + 'static,
        Sources: super::authorized_work::HoldingWorkSourcePreparation + Send + Sync + 'static,
        Work: for<'c> Fn(
                &'c mut sqlx::PgConnection,
                &'c HoldingAccountScope,
            ) -> erpx_server_runtime::public::postgres::PgWorkFuture<
                'c,
                Output,
                BusinessError,
            > + Send
            + Sync
            + 'static,
    {
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
        let sources = Arc::new(sources);
        let work = Arc::new(work);
        self.control
            .run(move |connection| {
                Box::pin(async move {
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
                    let holding_name:Option<String>=sqlx::query_scalar("SELECT CASE WHEN octet_length(display_name)<=8192 THEN display_name END FROM holding_bindings WHERE holding_id=$1 FOR SHARE").bind(holding_id.as_str()).fetch_one(&mut *connection).await?;
                    let holding_name=holding_name.ok_or_else(||BusinessError::new(FailureKind::Unavailable,"HOLDING_UNAVAILABLE","The current Holding is unavailable."))?;
                    let retained_name=holding_name.clone();
                    let retained_holding=holding_id.clone();
                    let session_id = RecordId::new(session.id)?;
                    let user_id = UserId::new(session.user.id.clone())?;
                    let user = session.user;
                    let expires_at = session.expires_at;
                    let options = TransactionOptions {
                        session_lock: Some(SessionLock {
                            namespace: "work-context-account".into(),
                            key: hex::encode(Sha256::digest(session_id.as_str().as_bytes())),
                        }),
                        prefer_indexed_reads: false,
                    };
                    let result = holding
                        .run(options, move |c| {
                            let session_id = session_id.clone();
                            let user_id = user_id.clone();
                            let user = user.clone();
                            let holding_name = holding_name.clone();
                            let holding_id = holding_id.clone();
                            let sources = sources.clone();
                            let work = work.clone();
                            let clock = clock.clone();
                            Box::pin(async move {
                                sources.prepare(c).await?;
                                let scope = HoldingAccountScope {
                                    holding_name,
                                    session_id,
                                    user_id,
                                    user,
                                    holding_id,
                                    expires_at,
                                    unit: super::selected_command_authority::selected_unit(c)
                                        .await?,
                                };
                                scope.require_current(c, clock.now().get()).await?;
                                let result = work(c, &scope).await?;
                                scope.require_current(c, clock.now().get()).await?;
                                Ok(result)
                            })
                        })
                        .await?;
                    binding.require_current(connection).await?;
                    let current_name:String=sqlx::query_scalar("SELECT display_name FROM holding_bindings WHERE holding_id=$1").bind(retained_holding.as_str()).fetch_one(&mut *connection).await?;
                    if current_name!=retained_name{return Err(BusinessError::new(FailureKind::Conflict,"HOLDING_CHANGED","The current Holding changed.").into());}
                    Ok(result)
                })
            })
            .await
    }
}
fn missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "WORKSPACE_NOT_FOUND",
        "Workspace not found.",
    )
}
