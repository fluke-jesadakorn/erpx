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
fn missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "WORKSPACE_NOT_FOUND",
        "Workspace not found.",
    )
}
