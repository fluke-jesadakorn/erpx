//! Actual interactive queue producer. Workflow resolves exact replay before
//! canonical Legal effects. This library is intentionally not an HTTP mount.
use crate::legal_owner_policy::{
    ActualExternalPolicy, LegalOwnerConfiguration, actor, authority_revision, scope_error,
};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, QueuedWorkBinding, WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_legal::public::{record_policy::*, *};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, FailureKind, IdentifierSource, RecordId,
};
use erpx_server_workflow::public::commands::*;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;
#[derive(Clone)]
pub struct LegalQueueRequest {
    pub key: CommandKey,
    pub document_id: RecordId,
    pub kind: LegalProcessingKind,
}
pub struct LegalQueueAdministration<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: LegalOwnerConfiguration,
    clock: Arc<C>,
}
impl<C: Clock + Send + Sync + 'static> LegalQueueAdministration<C> {
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
    pub async fn queue(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: HoldingWorkspaceRequest,
        request: LegalQueueRequest,
    ) -> Result<LegalProcessingReceipt, Error> {
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
                configuration.clone(),
                move |c, current| {
                    let config = configuration.clone();
                    let clock = clock.clone();
                    let request = request.clone();
                    Box::pin(async move {
                        let actor = processing_actor(current)?;
                        let mut ids = QueueIds;
                        let mut stage = QueueBridge {
                            current,
                            config: &config,
                            clock: clock.as_ref(),
                            phase: QueuePhase::Stage,
                        };
                        let plan = PgLegalProcessingRepository::new(
                            config.holding_id.clone(),
                            config.company_id.clone(),
                            c,
                            &mut stage,
                            clock.as_ref(),
                            &mut ids,
                        )
                        .stage_queue(&actor, &request.key, &request.document_id, request.kind)
                        .await?;
                        let receipt = plan.receipt()?;
                        // These typed literals retain the B0 property order for hashing.
                        #[derive(serde::Serialize)]
                        #[serde(rename_all = "camelCase")]
                        struct Input<'a> {
                            document_id: &'a str,
                            kind: LegalProcessingKind,
                        }
                        let command = WorkflowRequest {
                            key: request.key.as_str().into(),
                            operation: request.kind.operation().into(),
                            record_id: plan.document().item_id.as_str().into(),
                            input: CommandJson::from_serializable(&Input {
                                document_id: request.document_id.as_str(),
                                kind: request.kind,
                            })?,
                        };
                        let mut authority = QueueAuthority {
                            current,
                            configuration: &config,
                            clock: clock.as_ref(),
                            plan: &plan,
                        };
                        let mut effects = QueueEffects {
                            current,
                            configuration: &config,
                            clock: clock.as_ref(),
                            plan: &plan,
                            applied: false,
                        };
                        let outcome = PgCommandLedger::new(
                            config.company_id.clone(),
                            c,
                            &mut authority,
                            clock.as_ref(),
                            &mut ids,
                        )
                        .execute(&command, &mut effects)
                        .await?;
                        let expected = serde_json::to_value(&receipt).map_err(|_| scope_error())?;
                        let actual: serde_json::Value =
                            serde_json::from_str(outcome.result.as_str())
                                .map_err(|_| scope_error())?;
                        if actual != expected {
                            return Err(scope_error().into());
                        }
                        Ok(receipt)
                    })
                },
            )
            .await
    }
}
fn processing_actor(
    current: &AuthorizedWorkContext,
) -> Result<LegalProcessingActor, BusinessError> {
    Ok(LegalProcessingActor {
        holding_id: current.holding_id().clone(),
        company_id: current.selection().workspace().company_id.clone(),
        user_id: current.user_id().clone(),
        selection_revision: hex::encode(Sha256::digest(
            QueuedWorkBinding::from_current(current).to_json()?,
        )),
    })
}
fn require_plan(
    current: &AuthorizedWorkContext,
    plan: &StagedLegalProcessingQueue,
    command: &WorkflowRequest,
) -> Result<(), BusinessError> {
    if plan.actor() != &processing_actor(current)?
        || command.key != plan.key().as_str()
        || command.record_id != plan.document().item_id.as_str()
        || command.operation != plan.kind().operation()
    {
        return Err(scope_error());
    }
    Ok(())
}
async fn inspect(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    config: &LegalOwnerConfiguration,
    document: &LegalProcessingDocument,
    operation: LegalRecordOperation,
    now: i64,
) -> Result<LegalRecordAccess, Error> {
    current.require_permission("legal.read")?;
    if current.holding_id() != &config.holding_id
        || current.selection().workspace().company_id != config.company_id
    {
        return Err(scope_error().into());
    }
    let mut external = ActualExternalPolicy(config.clone());
    let read = PgLegalRecordPolicy::new(
        config.holding_id.clone(),
        config.company_id.clone(),
        c,
        &mut external,
    )
    .inspect(
        &actor(current),
        &LegalRecordRequest {
            item_id: document.item_id.clone(),
            document_id: Some(document.document_id.clone()),
            operation,
            mode: config.commercial_mode,
        },
        now,
    )
    .await?;
    if read.item.version as u64 != document.item_version
        || read
            .document
            .as_ref()
            .is_none_or(|d| d.checksum_sha256 != document.checksum_sha256)
    {
        return Err(scope_error().into());
    }
    Ok(read)
}
fn command_actor(current: &AuthorizedWorkContext) -> Result<CommandActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(CommandActor {
        user_id: current.user_id().clone(),
        assignment_id: assignment.id.clone(),
        role: assignment.role.clone(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace
            .branch_id
            .as_ref()
            .map(|id| CommandField::Value(id.as_str().into()))
            .unwrap_or(CommandField::Null),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|id| CommandField::Value(id.as_str().into()))
            .unwrap_or(CommandField::Null),
        authority: CommandField::Value(WorkAuthority {
            session_id: current.session_id().as_str().into(),
            revision: selected.revision(),
            assignment_version: assignment.version,
        }),
        iam: CommandField::Value(CommandJson::from_serializable(
            &erpx_server_identity_access::public::command_metadata::project_command_iam(current)?,
        )?),
        assignment_scope_kind: match assignment.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .into(),
        assignment_scope_node_id: assignment
            .scope_node_id
            .as_ref()
            .map(|id| id.as_str().into()),
    })
}
struct QueueAuthority<'a, C> {
    current: &'a AuthorizedWorkContext,
    configuration: &'a LegalOwnerConfiguration,
    clock: &'a C,
    plan: &'a StagedLegalProcessingQueue,
}
impl<C: Clock + Sync> PgCommandAuthority for QueueAuthority<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.configuration.lock_sources(c).await
    }
    async fn authorize(
        &mut self,
        c: &mut PgConnection,
        command: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        require_plan(self.current, self.plan, command)?;
        self.plan.require_current(c).await?;
        inspect(
            c,
            self.current,
            self.configuration,
            self.plan.document(),
            LegalRecordOperation::Document,
            self.clock.now().get(),
        )
        .await?;
        Ok(command_actor(self.current)?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        command: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> Result<(), Error> {
        require_plan(self.current, self.plan, command)?;
        if actor != &command_actor(self.current)? {
            return Err(scope_error().into());
        }
        inspect(
            c,
            self.current,
            self.configuration,
            self.plan.document(),
            match disposition {
                CommandDisposition::Execute => LegalRecordOperation::QueueProcessing,
                CommandDisposition::Replay => LegalRecordOperation::ProcessingReplay,
            },
            self.clock.now().get(),
        )
        .await?;
        Ok(())
    }
}
struct QueueEffects<'a, C> {
    current: &'a AuthorizedWorkContext,
    configuration: &'a LegalOwnerConfiguration,
    clock: &'a C,
    plan: &'a StagedLegalProcessingQueue,
    applied: bool,
}
impl<C: Clock + Sync> PgOwnerCommandEffects for QueueEffects<'_, C> {
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        command: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        require_plan(self.current, self.plan, command)?;
        self.plan.require_current(c).await?;
        if actor != &command_actor(self.current)? || self.applied {
            return Err(scope_error().into());
        }
        Ok(WorkflowOutput {
            version: i64::try_from(self.plan.document().item_version).map_err(|_| scope_error())?,
            result: CommandJson::from_serializable(&self.plan.receipt()?)?,
            event_scope: None,
        })
    }
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.configuration.lock_sources(c).await
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        command: &WorkflowRequest,
    ) -> Result<(), Error> {
        require_plan(self.current, self.plan, command)?;
        if actor != &command_actor(self.current)? || self.applied {
            return Err(scope_error().into());
        }
        let mut bridge = QueueBridge {
            current: self.current,
            config: self.configuration,
            clock: self.clock,
            phase: QueuePhase::Applying(self.plan),
        };
        let mut ids = QueueIds;
        let actual = PgLegalProcessingRepository::new(
            self.configuration.holding_id.clone(),
            self.configuration.company_id.clone(),
            c,
            &mut bridge,
            self.clock,
            &mut ids,
        )
        .apply_staged_queue(self.plan)
        .await?;
        if actual != self.plan.receipt()? {
            return Err(scope_error().into());
        }
        self.applied = true;
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        command: &WorkflowRequest,
    ) -> Result<(), Error> {
        require_plan(self.current, self.plan, command)?;
        if !self.applied {
            return Err(scope_error().into());
        }
        self.plan.require_queued(c).await
    }
}
enum QueuePhase<'a> {
    Stage,
    Applying(&'a StagedLegalProcessingQueue),
}
struct QueueBridge<'a, C> {
    current: &'a AuthorizedWorkContext,
    config: &'a LegalOwnerConfiguration,
    clock: &'a C,
    phase: QueuePhase<'a>,
}
fn queue_only() -> Error {
    BusinessError::new(
        FailureKind::Forbidden,
        "LEGAL_QUEUE_ONLY",
        "This assembly permits only current interactive queue effects.",
    )
    .into()
}
impl<C: Clock + Sync> PgLegalProcessingBridge for QueueBridge<'_, C> {
    async fn authorize_queue(
        &mut self,
        c: &mut PgConnection,
        actor: &LegalProcessingActor,
        key: &CommandKey,
        document: &LegalProcessingDocument,
        kind: LegalProcessingKind,
    ) -> Result<LegalProcessingQueueAuthority, Error> {
        if actor != &processing_actor(self.current)? {
            return Err(scope_error().into());
        }
        if let QueuePhase::Applying(plan) = self.phase
            && (plan.key() != key || plan.document() != document || plan.kind() != kind)
        {
            return Err(scope_error().into());
        }
        let read = inspect(
            c,
            self.current,
            self.config,
            document,
            match self.phase {
                QueuePhase::Stage => LegalRecordOperation::Document,
                QueuePhase::Applying(_) => LegalRecordOperation::QueueProcessing,
            },
            self.clock.now().get(),
        )
        .await?;
        Ok(LegalProcessingQueueAuthority {
            revision: authority_revision(self.current, &read.witness)?,
            actor_snapshot_json: QueuedWorkBinding::from_current(self.current).to_json()?,
        })
    }
    async fn replay(
        &mut self,
        _: &mut PgConnection,
        actor: &LegalProcessingActor,
        key: &CommandKey,
        document: &RecordId,
        kind: LegalProcessingKind,
    ) -> Result<Option<LegalProcessingReceipt>, Error> {
        // Only Workflow's private actual apply object constructs this phase,
        // after its ledger matched/resolved replay and inserted the known receipt.
        let QueuePhase::Applying(plan) = self.phase else {
            return Err(queue_only());
        };
        if actor != plan.actor()
            || key != plan.key()
            || document != &plan.document().document_id
            || kind != plan.kind()
        {
            return Err(scope_error().into());
        }
        Ok(None)
    }
    async fn record_queued(
        &mut self,
        _: &mut PgConnection,
        actor: &LegalProcessingActor,
        key: &CommandKey,
        document: &LegalProcessingDocument,
        kind: LegalProcessingKind,
        receipt: &LegalProcessingReceipt,
    ) -> Result<(), Error> {
        let QueuePhase::Applying(plan) = self.phase else {
            return Err(queue_only());
        };
        if actor != plan.actor()
            || key != plan.key()
            || document != plan.document()
            || kind != plan.kind()
            || receipt != &plan.receipt()?
        {
            return Err(scope_error().into());
        }
        // The actual surrounding ledger owns its receipt, audit and outbox.
        // This validates canonical owner output; it does not substitute a second
        // write path or mark a fictitious Workflow effect as successful.
        Ok(())
    }
    async fn candidate_eligible(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingJob,
    ) -> Result<bool, Error> {
        Err(queue_only())
    }
    async fn authorize_completion(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingLease,
    ) -> Result<(), Error> {
        Err(queue_only())
    }
    async fn authorize_status(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingActor,
        _: &RecordId,
    ) -> Result<(), Error> {
        Err(queue_only())
    }
}
struct QueueIds;
impl IdentifierSource for QueueIds {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("generated UUID")
    }
}
