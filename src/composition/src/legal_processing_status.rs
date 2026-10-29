//! Current viewer polling. A queued actor snapshot never grants read access.
use crate::legal_owner_policy::{
    ActualExternalPolicy, LegalOwnerConfiguration, actor, scope_error,
};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        HoldingWorkAuthorizer, HoldingWorkSourcePreparation, WorkAuthorizationRequest,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_legal::public::{processing_status::*, record_policy::*};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, RecordId};
use sqlx::PgConnection;
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;

#[derive(Debug, serde::Serialize)]
pub struct LegalProcessingStatusResponse {
    pub id: RecordId,
    pub item_id: RecordId,
    pub document_id: RecordId,
    pub state: String,
    pub error: Option<String>,
    pub attempts: u32,
    #[serde(rename = "jobId")]
    pub job_id: RecordId,
    pub result: Option<serde_json::Value>,
}
#[derive(Clone)]
struct StatusSources(LegalOwnerConfiguration);
impl HoldingWorkSourcePreparation for StatusSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            self.0.lock_sources(c).await?;
            lock_legal_processing_status_sources(c).await
        })
    }
}
pub struct LegalProcessingStatusService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: LegalOwnerConfiguration,
    clock: Arc<C>,
}
impl<C: Clock + Send + Sync + 'static> LegalProcessingStatusService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: LegalOwnerConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }
    pub async fn read(
        &self,
        host: &str,
        proof: DeviceProof,
        mut workspace: HoldingWorkspaceRequest,
        id: RecordId,
    ) -> Result<LegalProcessingStatusResponse, Error> {
        workspace.mutation = false;
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                workspace,
                WorkAuthorizationRequest {
                    permission: "legal.read".into(),
                    module: Some("legal".into()),
                    destination: "legal/processing".into(),
                    api: true,
                },
                StatusSources(configuration.clone()),
                move |c, current| {
                    let config = configuration.clone();
                    let clock = clock.clone();
                    let id = id.clone();
                    Box::pin(async move {
                        if current.holding_id() != &config.holding_id
                            || current.selection().workspace().company_id != config.company_id
                        {
                            return Err(scope_error().into());
                        }
                        let source =
                            load_legal_processing_status(c, &config.company_id, &id).await?;
                        let mut external = ActualExternalPolicy(config.clone());
                        let access = {
                            let mut policy = PgLegalRecordPolicy::new(
                                config.holding_id.clone(),
                                config.company_id.clone(),
                                c,
                                &mut external,
                            );
                            // B0 polling authorizes item metadata/History. It does not
                            // require a new processing purchase or reopen document bytes.
                            policy
                                .inspect(
                                    &actor(current),
                                    &LegalRecordRequest {
                                        item_id: source.item_id().clone(),
                                        document_id: None,
                                        operation: LegalRecordOperation::Metadata,
                                        mode: config.commercial_mode,
                                    },
                                    clock.now().get(),
                                )
                                .await?
                        };
                        source.require_current(c).await?;
                        let status = source.into_status()?;
                        // Check time-sensitive policy again after bounded JSON
                        // decoding, with all actual source locks still retained.
                        PgLegalRecordPolicy::new(
                            config.holding_id.clone(),
                            config.company_id.clone(),
                            c,
                            &mut external,
                        )
                        .revalidate(&access.witness, clock.now().get())
                        .await?;
                        Ok(LegalProcessingStatusResponse {
                            id: status.id.clone(),
                            item_id: status.item_id,
                            document_id: status.document_id,
                            state: status.state,
                            error: status.error,
                            attempts: status.attempts,
                            job_id: status.id,
                            result: status.result,
                        })
                    })
                },
            )
            .await
    }
}
