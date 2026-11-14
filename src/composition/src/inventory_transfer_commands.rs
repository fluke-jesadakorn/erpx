//! Native Inventory warehouse-transfer command composition.
//!
//! Selected Account/Holding context, current IAM permission, Company purchase
//! admission, Inventory stock/lot/move effects, the Workflow command ledger and
//! Files direct-receipt evidence share one serializable Holding unit. A posted
//! transit transfer creates its native carrier record in the same unit; later
//! dispatch/receive/loss decisions are separate native transit commands.
use crate::finance_journal_actor::command_actor;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, current_package_catalogue, lock_commercial_authority_sources,
};
use erpx_server_files::public::business_evidence::{
    PgBusinessEvidenceFiles, evidence_context, lock_business_evidence_sources, policy_fact,
};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest, WorkPermissionRequirement,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_inventory::public::transfers::{
    TransferAction, TransferActionInput, TransferSaveInput, TransferTransitionPlan, apply_transfer_save,
    apply_transfer_transition, lock_transfer_sources, plan_save, plan_transition,
    read_transfer_save_source, read_transfer_transition_source,
};
use erpx_server_records::public::business_evidence::{
    BusinessEvidenceRequest, prepare_business_evidence,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, IdentifierSource, RecordId,
    TimestampMillis,
};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandDisposition, CommandJson, PgCommandAuthority, PgCommandLedger,
    PgOwnerCommandEffects, WorkflowOutput, WorkflowRequest,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

const TRANSFER_CAPABILITY: &str = "supply.transfers";

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "TRANSFER_SOURCE_CHANGED",
        "The transfer changed. Reload its current state before submitting.",
    )
}

fn number(record: &str) -> String {
    let digest = Sha256::digest(record.as_bytes());
    let hex: String = digest.iter().take(12).map(|byte| format!("{byte:02X}")).collect();
    format!("TR-{hex}")
}

#[derive(Clone, Debug)]
enum TransferWrite {
    Save(Option<String>, TransferSaveInput),
    Transition(String, TransferAction, TransferActionInput),
}

impl TransferWrite {
    fn operation(&self) -> String {
        match self {
            Self::Save(Some(_), _) => "inventory.transfer.update".into(),
            Self::Save(None, _) => "inventory.transfer.create".into(),
            Self::Transition(_, action, _) => action.operation(),
        }
    }

    fn permission(&self) -> &'static str {
        match self {
            Self::Transition(_, action, _) if action.review() => "inventory.transfer.review",
            _ => "inventory.prepare",
        }
    }

    fn purpose(&self) -> CapabilityPurpose {
        match self {
            Self::Save(None, _) => CapabilityPurpose::NewWork,
            Self::Transition(_, TransferAction::Submit, _) => CapabilityPurpose::NewWork,
            _ => CapabilityPurpose::SettleExisting,
        }
    }

    fn record_id(&self) -> Option<String> {
        match self {
            Self::Save(id, _) => id.clone(),
            Self::Transition(id, _, _) => Some(id.clone()),
        }
    }
}

#[derive(Clone)]
pub struct InventoryTransferCommand<C> {
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub holding: HoldingId,
    pub company: CompanyId,
    pub catalogue: Arc<CommercialCatalogue>,
    pub clock: Arc<C>,
}

#[derive(Serialize)]
struct TransferResult {
    id: String,
    status: String,
    version: i64,
}

impl<C: Clock + Send + Sync + 'static> InventoryTransferCommand<C> {
    #[allow(clippy::too_many_arguments)]
    pub async fn save<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        id: Option<String>,
        input: TransferSaveInput,
        wire: CommandJson,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        self.run(host, proof, workspace, TransferWrite::Save(id, input), wire, key)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn transition<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        id: String,
        action: TransferAction,
        input: TransferActionInput,
        wire: CommandJson,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        self.run(
            host,
            proof,
            workspace,
            TransferWrite::Transition(id, action, input),
            wire,
            key,
        )
        .await
    }

    async fn run<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        write: TransferWrite,
        wire: CommandJson,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let record = write
            .record_id()
            .unwrap_or_else(|| format!("transfer-{}", key.as_str()));
        if record.is_empty() || record.len() > 200 {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "TRANSFER_NOT_FOUND",
                "Transfer not found in this scope.",
            )
            .into());
        }
        let expected_company = self.company.clone();
        let company = self.company.clone();
        let clock = self.clock.clone();
        let operation = write.operation();
        let permission = write.permission().to_owned();
        let purpose = write.purpose();
        let request = WorkflowRequest {
            key: key.as_str().to_owned(),
            operation: operation.clone(),
            record_id: record.clone(),
            input: wire,
        };
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                workspace()?,
                WorkAuthorizationRequest {
                    permission: WorkPermissionRequirement::AnyOf(vec![permission.clone()]),
                    module: Some("inventory".into()),
                    destination: operation,
                    api: true,
                },
                TransferSources,
                move |c, current| {
                    let expected_company = expected_company.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let write = write.clone();
                    let request = request.clone();
                    let record = record.clone();
                    let permission = permission.clone();
                    let purpose = purpose;
                    Box::pin(async move {
                        if current.selection().workspace().company_id != expected_company {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "INVENTORY_SCOPE_REQUIRED",
                                "Use a Company or Branch inventory assignment.",
                            )
                            .into());
                        }
                        if current.selection().workspace().department_id.is_some() {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "INVENTORY_SCOPE_REQUIRED",
                                "Use a Company or Branch inventory assignment.",
                            )
                            .into());
                        }
                        current.require_permission(&permission)?;
                        let mut authority = Authority {
                            current,
                            request: &request,
                            clock: clock.as_ref(),
                            catalogue: current_package_catalogue()?,
                            purchase: None,
                            permission: &permission,
                            purpose,
                        };
                        let mut effects = Effects {
                            current,
                            company: &company,
                            clock: clock.as_ref(),
                            request: &request,
                            record_id: &record,
                            write: &write,
                            prepared: None,
                            evidence: None,
                        };
                        let mut ids = Ids;
                        let outcome = PgCommandLedger::new(
                            company.clone(),
                            c,
                            &mut authority,
                            clock.as_ref(),
                            &mut ids,
                        )
                        .execute(&request, &mut effects)
                        .await?;
                        Ok(outcome.result)
                    })
                },
            )
            .await
    }
}

#[derive(Clone)]
struct TransferSources;

impl HoldingWorkSourcePreparation for TransferSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_transfer_sources(c).await?;
            sqlx::query(
                "LOCK TABLE finance_periods,finance_journals,finance_journal_lines,\
                 finance_accounts,finance_charts IN SHARE ROW EXCLUSIVE MODE",
            )
            .execute(&mut *c)
            .await?;
            lock_business_evidence_sources(c).await?;
            Ok(())
        })
    }
}

struct Authority<'a, C> {
    current: &'a AuthorizedWorkContext,
    request: &'a WorkflowRequest,
    clock: &'a C,
    catalogue: CommercialCatalogue,
    purchase: Option<PurchaseWitness>,
    permission: &'a str,
    purpose: CapabilityPurpose,
}

impl<C: Clock + Sync> PgCommandAuthority for Authority<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        HoldingWorkSourcePreparation::prepare(&TransferSources, c).await
    }

    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        r: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        if r.key != self.request.key
            || r.operation != self.request.operation
            || r.record_id != self.request.record_id
            || r.input.as_str() != self.request.input.as_str()
        {
            return Err(changed().into());
        }
        Ok(command_actor(self.current)?)
    }

    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        _: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> Result<(), Error> {
        self.current.require_permission(self.permission)?;
        if &command_actor(self.current)? != actor {
            return Err(changed().into());
        }
        let purpose = match disposition {
            CommandDisposition::Replay => CapabilityPurpose::History,
            CommandDisposition::Execute => self.purpose,
        };
        let mut reader = PgCompanyPurchaseReader::new(
            self.current.holding_id().clone(),
            actor.company_id.clone(),
            c,
            &self.catalogue,
        );
        if let Some(witness) = &self.purchase {
            reader
                .revalidate(witness, self.clock.now())
                .await?
                .require_allowed()?;
        } else {
            self.purchase = Some(
                reader
                    .inspect(
                        &PurchaseRequest {
                            capabilities: vec![TRANSFER_CAPABILITY.to_owned()],
                            purpose,
                            mode: PackageEnforcementMode::Enforced,
                        },
                        self.clock.now(),
                    )
                    .await?
                    .require_allowed()?
                    .clone(),
            );
        }
        Ok(())
    }
}

enum Prepared {
    Save(erpx_server_inventory::public::transfers::TransferSavePlan),
    Transition(TransferTransitionPlan),
}

struct Effects<'a, C> {
    current: &'a AuthorizedWorkContext,
    company: &'a CompanyId,
    clock: &'a C,
    request: &'a WorkflowRequest,
    record_id: &'a str,
    write: &'a TransferWrite,
    prepared: Option<Prepared>,
    evidence: Option<erpx_server_files::public::business_evidence::PreparedBusinessEvidence>,
}

async fn prepare_evidence(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    company: &CompanyId,
    plan: &TransferTransitionPlan,
    key: &str,
    now: TimestampMillis,
) -> Result<
    Option<erpx_server_files::public::business_evidence::PreparedBusinessEvidence>,
    Error,
> {
    if plan.attachment_set_ids.is_empty() {
        return Ok(None);
    }
    let mut files = PgBusinessEvidenceFiles::new(
        c,
        company.clone(),
        current.user_id().as_str().to_owned(),
        current
            .selection()
            .workspace()
            .branch_id
            .as_ref()
            .map(|id| id.as_str().to_owned()),
        None,
        vec![policy_fact(
            "stock_transfer",
            "direct_receipt_evidence",
            &["action"],
            1,
        )],
        now,
    );
    let prepared = prepare_business_evidence(
        &mut files,
        BusinessEvidenceRequest {
            attachment_set_ids: plan.attachment_set_ids.clone(),
            target_types: vec!["stock_transfer".into(), "inventory_transfer".into()],
            contexts: vec![evidence_context(
                "direct_receipt_evidence",
                "action",
                Some("direct-receipt"),
            )],
            target_id: plan.id.clone(),
            expected_target_version: plan.expected_version,
            command_id: key.to_owned(),
            binding_type: "direct_transfer_receipt".to_owned(),
            binding_id: format!("{}:{}", plan.id, plan.version),
        },
    )
    .await?
    .ok_or_else(|| {
        BusinessError::invalid(
            "TRANSFER_ATTACHMENT_PURPOSE",
            "Choose only direct receipt evidence prepared for this transfer review.",
        )
    })?;
    Ok(Some(prepared))
}

impl<C: Clock + Sync> PgOwnerCommandEffects for Effects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        HoldingWorkSourcePreparation::prepare(&TransferSources, c).await
    }

    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let now = self.clock.now();
        let workspace = self.current.selection().workspace();
        let branch = workspace.branch_id.as_ref().map(|id| id.as_str());
        let actor = self.current.user_id().as_str().to_owned();
        match self.write {
            TransferWrite::Save(id, input) => {
                if self.write.permission() == "inventory.prepare"
                    && !self.current.has_permission("inventory.prepare")
                {
                    return Err(BusinessError::new(
                        FailureKind::Forbidden,
                        "TRANSFER_PREPARER_REQUIRED",
                        "An inventory operator must prepare a warehouse transfer.",
                    )
                    .into());
                }
                let source = read_transfer_save_source(
                    c,
                    self.company.as_str(),
                    branch,
                    id.as_deref(),
                    input,
                )
                .await?;
                let record = self.record_id.to_owned();
                let plan = plan_save(
                    &source,
                    input,
                    &actor,
                    self.company.as_str(),
                    &record,
                    &number(&record),
                )?;
                self.prepared = Some(Prepared::Save(plan.clone()));
                Ok(WorkflowOutput {
                    version: plan.version,
                    result: CommandJson::from_serializable(&TransferResult {
                        id: plan.id.clone(),
                        status: "draft".into(),
                        version: plan.version,
                    })?,
                    event_scope: None,
                })
            }
            TransferWrite::Transition(id, action, input) => {
                let source = read_transfer_transition_source(
                    c,
                    self.company.as_str(),
                    branch,
                    id,
                    *action,
                )
                .await?;
                let plan = plan_transition(&source, input, *action, &actor)?;
                if *action == TransferAction::Post && source.transfer.delivery_mode != "transit" {
                    self.evidence =
                        prepare_evidence(c, self.current, self.company, &plan, self.request.key.as_str(), now)
                            .await?;
                }
                self.prepared = Some(Prepared::Transition(plan.clone()));
                Ok(WorkflowOutput {
                    version: plan.version,
                    result: CommandJson::from_serializable(&TransferResult {
                        id: plan.id.clone(),
                        status: plan.status.to_owned(),
                        version: plan.version,
                    })?,
                    event_scope: None,
                })
            }
        }
    }

    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let now = self.clock.now().get();
        let actor = self.current.user_id().as_str().to_owned();
        match self.prepared.take().ok_or_else(changed)? {
            Prepared::Save(plan) => {
                let existing = plan.version > 1;
                apply_transfer_save(c, &plan, self.request.key.as_str(), &actor, now, existing)
                    .await?;
            }
            Prepared::Transition(plan) => {
                apply_transfer_transition(c, &plan, self.request.key.as_str(), &actor, now).await?;
            }
        }
        Ok(())
    }

    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *c)
            .await?;
        if let Some(evidence) = &self.evidence {
            let mut files = PgBusinessEvidenceFiles::new(
                c,
                self.company.clone(),
                self.current.user_id().as_str().to_owned(),
                self.current
                    .selection()
                    .workspace()
                    .branch_id
                    .as_ref()
                    .map(|id| id.as_str().to_owned()),
                None,
                vec![],
                self.clock.now(),
            );
            files.require_current(evidence).await?;
        }
        Ok(())
    }
}

struct Ids;

impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("generated UUID")
    }
}
