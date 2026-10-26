use crate::{
    adapters::{
        authorized_work::same_authority,
        permission_catalog::permission_catalog,
        postgres_effective_authority::{current_authority_facts, lock_effective_authority_sources},
        work_admission::work_module_catalog,
    },
    application::effective_authority::authorize_effective_work,
    domain::{
        effective_authority::{AuthorizedWorkContext, WorkAuthorizationRequest},
        queued_authority::{QueuedWorkBinding, changed},
    },
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, PgWorkFuture, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, Clock, HoldingId};
use sqlx::PgConnection;
use std::sync::Arc;

/// Trusted composition implements this adapter port using the owner's opaque
/// current source witness. JSON or a public lease data struct is not a source.
/// No default implementation or permission-bearing actor is accepted.
pub trait PgQueuedWorkSourcePort: Clone + Send + Sync + 'static {
    type Source: Send;
    fn load_source<'c>(
        &'c self,
        connection: &'c mut PgConnection,
        now: i64,
    ) -> PgWorkFuture<'c, Self::Source, BusinessError>;
    fn binding(&self, source: &Self::Source) -> Result<QueuedWorkBinding, BusinessError>;
    /// Prove the same database/backend/transaction and owner row/source/lease.
    /// The owner's completion-aware policy must allow only its actual transition.
    fn require_current<'c>(
        &'c self,
        connection: &'c mut PgConnection,
        source: &'c Self::Source,
        now: i64,
    ) -> PgWorkFuture<'c, (), BusinessError>;
}
pub struct PgQueuedWorkAuthorizer {
    holding: PgUnitOfWork,
    holding_id: HoldingId,
    clock: Arc<dyn Clock + Send + Sync>,
}
impl PgQueuedWorkAuthorizer {
    pub fn new(
        holding: DatabasePool,
        holding_id: HoldingId,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> Self {
        Self {
            holding: PgUnitOfWork::new(holding),
            holding_id,
            clock,
        }
    }
    /// Owns a fresh Holding transaction. Original durable source and live IAM
    /// are both mandatory; this does not authenticate an Account browser/device.
    /// Only the inner callback may replay and all effects remain transactional.
    pub async fn execute<P, T, F>(
        &self,
        source: P,
        access: WorkAuthorizationRequest,
        action: F,
    ) -> Result<T, PgTransactionError<BusinessError>>
    where
        P: PgQueuedWorkSourcePort,
        T: Send + 'static,
        F: for<'c> Fn(
                &'c mut PgConnection,
                &'c AuthorizedWorkContext,
                &'c P::Source,
            ) -> PgWorkFuture<'c, T, BusinessError>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        let holding_id = self.holding_id.clone();
        let clock = self.clock.clone();
        self.holding
            .run(TransactionOptions::default(), move |connection| {
                let source = source.clone();
                let access = access.clone();
                let action = action.clone();
                let holding_id = holding_id.clone();
                let clock = clock.clone();
                Box::pin(async move {
                    lock_effective_authority_sources(&mut *connection).await?;
                    let owned = source
                        .load_source(&mut *connection, clock.now().get())
                        .await?;
                    let binding = source.binding(&owned)?;
                    let request = binding.request(&holding_id, clock.now().get())?;
                    source
                        .require_current(&mut *connection, &owned, clock.now().get())
                        .await?;
                    let catalog = permission_catalog()?;
                    let modules = work_module_catalog()?;
                    let facts =
                        current_authority_facts(&mut *connection, &request, clock.now().get())
                            .await?;
                    let current = authorize_effective_work(
                        &request,
                        &access,
                        facts,
                        catalog,
                        modules,
                        clock.now().get(),
                    )?;
                    if !binding.matches_current(&current) {
                        return Err(changed().into());
                    }
                    let result = action(&mut *connection, &current, &owned).await?;
                    current.require_current_time(clock.now().get())?;
                    source
                        .require_current(&mut *connection, &owned, clock.now().get())
                        .await?;
                    let facts =
                        current_authority_facts(&mut *connection, &request, clock.now().get())
                            .await?;
                    let final_context = authorize_effective_work(
                        &request,
                        &access,
                        facts,
                        catalog,
                        modules,
                        clock.now().get(),
                    )?;
                    if !binding.matches_current(&final_context)
                        || !same_authority(&current, &final_context)
                    {
                        return Err(changed().into());
                    }
                    Ok(result)
                })
            })
            .await
    }
}
