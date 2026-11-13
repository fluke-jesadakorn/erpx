//! Native Inventory transport command composition.
//!
//! The selected Account/Holding context, current IAM permission, Company
//! purchase admission, Inventory transit source/effects, the Workflow command
//! ledger, Files evidence and the Finance loss journal share the same
//! serializable Holding unit. Every owner read happens after the ordered source
//! locks; the plan is produced from those facts and the effects adapter rechecks
//! the same version before writing.
use crate::finance_journal_actor::{command_actor, finance_actor};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, current_package_catalogue, lock_commercial_authority_sources,
};
use erpx_server_files::public::business_evidence::{
    PgBusinessEvidenceFiles, evidence_context, lock_business_evidence_sources, policy_fact,
};
use erpx_server_finance::public::{
    FinanceActor, FinanceCollation, JournalAuthority, JournalDraft, JournalError, JournalStatus,
    PgJournalPreparation, PgJournalStore, embedded_currency_rules, prepare_journal,
};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest, WorkPermissionRequirement,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_inventory::public::transit::{
    TransitCancelInput, TransitCancelPlan, TransitDispatchInput, TransitDispatchPlan,
    TransitLossDecisionInput, TransitLossDecisionPlan, TransitLossInput, TransitLossPlan,
    TransitReceiveInput, TransitReceivePlan, TransitSource, apply_transit_cancel,
    apply_transit_dispatch, apply_transit_loss_decision, apply_transit_loss_request,
    apply_transit_receive, lock_transit_sources, plan_cancel, plan_dispatch, plan_loss_decision,
    plan_loss_request, plan_receive, read_proposed_loss_lots, read_transit_source,
};
use erpx_server_records::public::business_evidence::{
    BusinessEvidenceRequest, prepare_business_evidence,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, IdentifierSource,
    RecordId, TimestampMillis,
};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandDisposition, CommandJson, PgCommandAuthority, PgCommandLedger,
    PgOwnerCommandEffects, WorkflowOutput, WorkflowRequest,
};
use serde::Serialize;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

const TRANSIT_CAPABILITY: &str = "supply.transfers";
const LOSS_CAPABILITY: &str = "finance.inventory-valuation";

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "TRANSIT_SOURCE_CHANGED",
        "The transport record changed. Reload its current state before submitting.",
    )
}

#[derive(Clone, Debug)]
enum TransitWrite {
    Dispatch(TransitDispatchInput),
    Receive(TransitReceiveInput),
    Loss(TransitLossInput),
    Decide(TransitLossDecisionInput),
    Cancel(TransitCancelInput),
}

impl TransitWrite {
    fn operation(&self) -> &'static str {
        match self {
            Self::Dispatch(_) => "inventory.transit.dispatch",
            Self::Receive(_) => "inventory.transit.receive",
            Self::Loss(_) => "inventory.transit.loss.request",
            Self::Decide(_) => "inventory.transit.loss.decide",
            Self::Cancel(_) => "inventory.transit.cancel",
        }
    }

    fn permission(&self) -> &'static str {
        match self {
            Self::Decide(_) => "finance.approve",
            _ => "inventory.prepare",
        }
    }
}

#[derive(Clone)]
pub struct InventoryTransitCommand<C> {
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub holding: HoldingId,
    pub company: CompanyId,
    pub catalogue: Arc<CommercialCatalogue>,
    pub clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> InventoryTransitCommand<C> {
    pub async fn dispatch<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        id: String,
        input: TransitDispatchInput,
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
            id,
            TransitWrite::Dispatch(input),
            wire,
            key,
        )
        .await
    }

    pub async fn receive<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        id: String,
        input: TransitReceiveInput,
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
            id,
            TransitWrite::Receive(input),
            wire,
            key,
        )
        .await
    }

    pub async fn loss<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        id: String,
        input: TransitLossInput,
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
            id,
            TransitWrite::Loss(input),
            wire,
            key,
        )
        .await
    }

    pub async fn decide_loss<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        id: String,
        input: TransitLossDecisionInput,
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
            id,
            TransitWrite::Decide(input),
            wire,
            key,
        )
        .await
    }

    pub async fn cancel<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        id: String,
        input: TransitCancelInput,
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
            id,
            TransitWrite::Cancel(input),
            wire,
            key,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn run<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        id: String,
        write: TransitWrite,
        wire: CommandJson,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        if id.is_empty() || id.len() > 200 {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "TRANSIT_NOT_FOUND",
                "Transport record not found in this scope.",
            )
            .into());
        }
        let expected_company = self.company.clone();
        let company = self.company.clone();
        let clock = self.clock.clone();
        let record_id = id.clone();
        let operation = write.operation().to_owned();
        let permission = write.permission().to_owned();
        let request = WorkflowRequest {
            key: key.as_str().to_owned(),
            operation: operation.clone(),
            record_id,
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
                TransitSources,
                move |c, current| {
                    let expected_company = expected_company.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let write = write.clone();
                    let request = request.clone();
                    let id = id.clone();
                    let permission = permission.clone();
                    Box::pin(async move {
                        if current.selection().workspace().company_id != expected_company {
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
                        };
                        let mut effects = Effects {
                            current,
                            company: &company,
                            clock: clock.as_ref(),
                            request: &request,
                            id: &id,
                            write: &write,
                            prepared: None,
                            evidence: None,
                            parent_transfer: None,
                            journal: None,
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
struct TransitSources;

impl HoldingWorkSourcePreparation for TransitSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            // Ordered owner fences: Commercial -> Inventory -> Finance -> Files.
            lock_commercial_authority_sources(c).await?;
            lock_transit_sources(c).await?;
            sqlx::query(
                "LOCK TABLE finance_journals,finance_journal_lines,finance_periods,\
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
}

impl<C: Clock + Sync> PgCommandAuthority for Authority<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        HoldingWorkSourcePreparation::prepare(&TransitSources, c).await
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
        let now = self.clock.now();
        let purpose = match disposition {
            CommandDisposition::Replay => CapabilityPurpose::History,
            CommandDisposition::Execute => match self.request.operation.as_str() {
                "inventory.transit.loss.decide" => CapabilityPurpose::SettleExisting,
                "inventory.transit.dispatch" => CapabilityPurpose::NewWork,
                _ => CapabilityPurpose::SettleExisting,
            },
        };
        let capabilities = if self.request.operation == "inventory.transit.loss.decide" {
            vec![TRANSIT_CAPABILITY.to_owned(), LOSS_CAPABILITY.to_owned()]
        } else {
            vec![TRANSIT_CAPABILITY.to_owned()]
        };
        let mut reader = PgCompanyPurchaseReader::new(
            self.current.holding_id().clone(),
            actor.company_id.clone(),
            c,
            &self.catalogue,
        );
        if let Some(witness) = &self.purchase {
            reader.revalidate(witness, now).await?.require_allowed()?;
        } else {
            self.purchase = Some(
                reader
                    .inspect(
                        &PurchaseRequest {
                            capabilities,
                            purpose,
                            mode: PackageEnforcementMode::Enforced,
                        },
                        now,
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
    Dispatch(TransitDispatchPlan),
    Receive(TransitReceivePlan),
    Loss(TransitLossPlan),
    Decide(TransitLossDecisionPlan, Option<String>),
    Cancel(TransitCancelPlan),
}

#[derive(Serialize)]
struct DispatchResult {
    id: String,
    status: String,
    version: i64,
}

#[derive(Serialize)]
struct ReceiveResult {
    id: String,
    version: i64,
}

#[derive(Serialize)]
struct LossResult {
    id: String,
    #[serde(rename = "proposalId")]
    proposal_id: String,
    version: i64,
}

#[derive(Serialize)]
struct DecideResult {
    id: String,
    version: i64,
    #[serde(rename = "journalId")]
    journal_id: Option<String>,
}

#[derive(Serialize)]
struct CancelResult {
    id: String,
    status: String,
    version: i64,
}

struct BoundJournal {
    actor: FinanceActor,
    draft: JournalDraft,
}

impl JournalAuthority for BoundJournal {
    type Error = PgTransactionError<JournalError>;

    async fn authorize(
        &mut self,
        actor: &FinanceActor,
        draft: &JournalDraft,
    ) -> Result<(), Self::Error> {
        let lines = draft.lines.len() == self.draft.lines.len()
            && draft.lines.iter().zip(&self.draft.lines).all(|(a, b)| {
                a.account_id == b.account_id
                    && a.debit == b.debit
                    && a.credit == b.credit
                    && a.description == b.description
                    && a.dimensions == b.dimensions
            });
        if actor != &self.actor
            || !lines
            || draft.chart_id != self.draft.chart_id
            || draft.period_id != self.draft.period_id
            || draft.entry_date != self.draft.entry_date
            || draft.currency != self.draft.currency
            || draft.fx_rate != self.draft.fx_rate
            || draft.memo != self.draft.memo
            || draft.source_type != self.draft.source_type
            || draft.source_id != self.draft.source_id
            || draft.is_opening
        {
            return Err(JournalError {
                kind: FailureKind::Forbidden,
                code: "TRANSIT_FINANCE_BINDING_REQUIRED",
                message: "Use the currently authorized transport posting.".into(),
            }
            .into());
        }
        Ok(())
    }
}

struct Effects<'a, C> {
    current: &'a AuthorizedWorkContext,
    company: &'a CompanyId,
    clock: &'a C,
    request: &'a WorkflowRequest,
    id: &'a str,
    write: &'a TransitWrite,
    prepared: Option<Prepared>,
    evidence: Option<erpx_server_files::public::business_evidence::PreparedBusinessEvidence>,
    parent_transfer: Option<(String, i64)>,
    journal: Option<erpx_server_finance::public::JournalPreparation>,
}

fn journal_error(e: PgTransactionError<JournalError>) -> Error {
    match e {
        PgTransactionError::Persistence(e) => PgTransactionError::Persistence(e),
        PgTransactionError::Business(e) => BusinessError::new(
            e.kind,
            e.code,
            "The transport Finance posting is unavailable.",
        )
        .into(),
    }
}

fn decimal_minor(value: i64) -> String {
    let sign = if value < 0 { "-" } else { "" };
    let value = value.abs();
    format!("{sign}{}.{:02}", value / 100, value % 100)
}

async fn active_warehouse(
    c: &mut PgConnection,
    company: &str,
    warehouse: &str,
) -> Result<bool, Error> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM commerce_warehouses \
         WHERE id=$1 AND company_id=$2 AND status='active')",
    )
    .bind(warehouse)
    .bind(company)
    .fetch_one(&mut *c)
    .await?)
}

async fn prepare_evidence(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    company: &CompanyId,
    write: &TransitWrite,
    source: &TransitSource,
    key: &str,
    now: TimestampMillis,
) -> Result<
    (
        Option<erpx_server_files::public::business_evidence::PreparedBusinessEvidence>,
        Option<(String, i64)>,
    ),
    Error,
> {
    let (set_ids, contexts, binding_type, binding_id) = match write {
        TransitWrite::Dispatch(input) => (
            input.attachment_set_ids.clone(),
            vec![evidence_context(
                "dispatch_evidence",
                "action",
                Some("dispatch"),
            )],
            "inventory_transit_dispatch",
            format!("transit-dispatch-{key}"),
        ),
        TransitWrite::Receive(input) => (
            input.attachment_set_ids.clone(),
            input
                .lines
                .iter()
                .map(|line| {
                    evidence_context(
                        "receipt_evidence",
                        "event",
                        Some(&format!("receive:{}", line.line_id)),
                    )
                })
                .collect(),
            "inventory_transit_receipt",
            format!("transit-receipt-{key}"),
        ),
        TransitWrite::Loss(input) => (
            input.attachment_set_ids.clone(),
            vec![evidence_context(
                "loss_evidence",
                "event",
                Some(&format!("loss:{}", input.line_id)),
            )],
            "inventory_transit_loss",
            format!("loss-{key}"),
        ),
        _ => return Ok((None, None)),
    };
    if set_ids.is_empty() {
        return Ok((None, None));
    }
    let first_type: Option<String> =
        sqlx::query_scalar("SELECT target_type FROM attachment_sets WHERE id=$1 AND company_id=$2")
            .bind(&set_ids[0])
            .bind(company.as_str())
            .fetch_optional(&mut *c)
            .await?;
    let (target_types, target_id, expected_version, parent) = match first_type.as_deref() {
        Some("inventory_transit") => (
            vec!["inventory_transit".to_owned()],
            source.transit.id.clone(),
            match write {
                TransitWrite::Dispatch(input) => input.expected_version,
                TransitWrite::Receive(input) => input.expected_version,
                TransitWrite::Loss(input) => input.expected_version,
                _ => return Err(changed().into()),
            },
            None,
        ),
        _ => {
            let expected_transfer = match write {
                TransitWrite::Dispatch(input) => input.expected_transfer_version,
                TransitWrite::Receive(input) => input.expected_transfer_version,
                TransitWrite::Loss(input) => input.expected_transfer_version,
                _ => None,
            };
            let transfer: Option<(String, i64)> = sqlx::query_as(
                "SELECT id,version FROM inventory_transfers WHERE id=$1 AND company_id=$2 \
                 AND status='posted' AND delivery_mode='transit'",
            )
            .bind(&source.transit.transfer_id)
            .bind(company.as_str())
            .fetch_optional(&mut *c)
            .await?;
            let Some((transfer_id, version)) = transfer else {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "ATTACHMENT_TARGET_CHANGED",
                    "Reload the current approved transfer before using its transport evidence.",
                )
                .into());
            };
            if expected_transfer != Some(version) {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "ATTACHMENT_TARGET_CHANGED",
                    "Reload the current approved transfer before using its transport evidence.",
                )
                .into());
            }
            (
                vec!["stock_transfer".to_owned(), "inventory_transfer".to_owned()],
                transfer_id.clone(),
                version,
                Some((transfer_id, version)),
            )
        }
    };
    let policy_target = match first_type.as_deref() {
        Some("stock_transfer") => "stock_transfer",
        Some("inventory_transfer") => "inventory_transfer",
        _ => "inventory_transit",
    };
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
            policy_target,
            match write {
                TransitWrite::Dispatch(_) => "dispatch_evidence",
                TransitWrite::Receive(_) => "receipt_evidence",
                TransitWrite::Loss(_) => "loss_evidence",
                _ => "dispatch_evidence",
            },
            &["action", "event"],
            1,
        )],
        now,
    );
    let prepared = prepare_business_evidence(
        &mut files,
        BusinessEvidenceRequest {
            attachment_set_ids: set_ids,
            target_types,
            contexts,
            target_id: target_id.clone(),
            expected_target_version: expected_version,
            command_id: key.to_owned(),
            binding_type: binding_type.to_owned(),
            binding_id,
        },
    )
    .await?
    .ok_or_else(|| {
        BusinessError::invalid(
            "ATTACHMENT_SETS_REQUIRED",
            "Choose at least one ready evidence set.",
        )
    })?;
    Ok((Some(prepared), parent))
}

impl<C: Clock + Sync> PgOwnerCommandEffects for Effects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        HoldingWorkSourcePreparation::prepare(&TransitSources, c).await
    }

    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let now = self.clock.now();
        let workspace = self.current.selection().workspace();
        let source = read_transit_source(
            c,
            self.company.as_str(),
            workspace.branch_id.as_ref().map(|id| id.as_str()),
            self.id,
        )
        .await?;
        if !active_warehouse(
            c,
            self.company.as_str(),
            &source.transit.source_warehouse_id,
        )
        .await?
        {
            return Err(BusinessError::invalid(
                "SOURCE_WAREHOUSE_INVALID",
                "Choose an active source warehouse.",
            )
            .into());
        }
        if !active_warehouse(
            c,
            self.company.as_str(),
            &source.transit.target_warehouse_id,
        )
        .await?
        {
            return Err(BusinessError::invalid(
                "TARGET_WAREHOUSE_INVALID",
                "Choose an active target warehouse.",
            )
            .into());
        }
        let mut journal_plan: Option<erpx_server_finance::public::JournalPreparation> = None;
        let prepared = match self.write {
            TransitWrite::Dispatch(input) => {
                let (evidence, parent) = prepare_evidence(
                    c,
                    self.current,
                    self.company,
                    self.write,
                    &source,
                    self.request.key.as_str(),
                    now,
                )
                .await?;
                self.evidence = evidence;
                self.parent_transfer = parent;
                Prepared::Dispatch(plan_dispatch(&source, input)?)
            }
            TransitWrite::Receive(input) => {
                let (evidence, parent) = prepare_evidence(
                    c,
                    self.current,
                    self.company,
                    self.write,
                    &source,
                    self.request.key.as_str(),
                    now,
                )
                .await?;
                self.evidence = evidence;
                self.parent_transfer = parent;
                Prepared::Receive(plan_receive(&source, input)?)
            }
            TransitWrite::Loss(input) => {
                let (evidence, parent) = prepare_evidence(
                    c,
                    self.current,
                    self.company,
                    self.write,
                    &source,
                    self.request.key.as_str(),
                    now,
                )
                .await?;
                self.evidence = evidence;
                self.parent_transfer = parent;
                Prepared::Loss(plan_loss_request(&source, input)?)
            }
            TransitWrite::Decide(input) => {
                let lots = read_proposed_loss_lots(c, &input.proposal_id).await?;
                let plan = plan_loss_decision(&source, input, &lots)?;
                let journal = if plan.approved && plan.journal_value_minor > 0 {
                    let period_id = input.period_id.clone().ok_or_else(|| {
                        BusinessError::invalid(
                            "LOSS_ACCOUNT_REQUIRED",
                            "Select the open period and inventory-loss expense account.",
                        )
                    })?;
                    let expense = input.expense_account_id.clone().ok_or_else(|| {
                        BusinessError::invalid(
                            "LOSS_ACCOUNT_REQUIRED",
                            "Select the open period and inventory-loss expense account.",
                        )
                    })?;
                    let amount = decimal_minor(plan.journal_value_minor);
                    let draft = JournalDraft {
                        chart_id: source.transit.chart_id.clone(),
                        period_id,
                        entry_date: plan.date.clone(),
                        currency: source.transit.currency.clone(),
                        fx_rate: None,
                        is_opening: false,
                        memo: format!("Transit loss {}: {}", source.transit.number, plan.evidence),
                        source_type: "inventory_transit_loss".into(),
                        source_id: Some(plan.proposal_id.clone()),
                        lines: vec![
                            erpx_server_finance::public::JournalLineInput {
                                account_id: expense,
                                description: None,
                                debit: amount.clone(),
                                credit: "0.00".into(),
                                dimensions: None,
                            },
                            erpx_server_finance::public::JournalLineInput {
                                account_id: source.transit.inventory_account_id.clone(),
                                description: None,
                                debit: "0.00".into(),
                                credit: amount,
                                dimensions: None,
                            },
                        ],
                    };
                    let actor = finance_actor(self.current);
                    let mut authority = BoundJournal {
                        actor: actor.clone(),
                        draft: draft.clone(),
                    };
                    let rules = embedded_currency_rules().map_err(|_| changed())?;
                    let collation = FinanceCollation::english().map_err(|_| changed())?;
                    let mut ports = PgJournalPreparation::new(
                        c,
                        &actor,
                        &mut authority,
                        self.clock,
                        &rules,
                        &collation,
                    )
                    .map_err(journal_error)?;
                    Some(
                        prepare_journal(
                            &mut ports,
                            &actor,
                            &draft,
                            &CommandKey::new(self.request.key.clone())?,
                            JournalStatus::Posted,
                        )
                        .await
                        .map_err(journal_error)?,
                    )
                } else {
                    None
                };
                let (evidence, parent) = prepare_evidence(
                    c,
                    self.current,
                    self.company,
                    self.write,
                    &source,
                    self.request.key.as_str(),
                    now,
                )
                .await?;
                self.evidence = evidence;
                self.parent_transfer = parent;
                journal_plan = journal;
                Prepared::Decide(
                    plan,
                    journal_plan
                        .as_ref()
                        .map(|plan| plan.journal_id().to_owned()),
                )
            }
            TransitWrite::Cancel(input) => Prepared::Cancel(plan_cancel(&source, input)?),
        };
        self.journal = journal_plan;
        let (version, result) = match &prepared {
            Prepared::Dispatch(plan) => (
                plan.version,
                CommandJson::from_serializable(&DispatchResult {
                    id: self.id.to_owned(),
                    status: "in_transit".into(),
                    version: plan.version,
                })?,
            ),
            Prepared::Receive(plan) => (
                plan.version,
                CommandJson::from_serializable(&ReceiveResult {
                    id: self.id.to_owned(),
                    version: plan.version,
                })?,
            ),
            Prepared::Loss(plan) => (
                plan.version,
                CommandJson::from_serializable(&LossResult {
                    id: self.id.to_owned(),
                    proposal_id: format!("loss-{}", self.request.key),
                    version: plan.version,
                })?,
            ),
            Prepared::Decide(plan, journal) => (
                plan.version,
                CommandJson::from_serializable(&DecideResult {
                    id: self.id.to_owned(),
                    version: plan.version,
                    journal_id: journal.clone(),
                })?,
            ),
            Prepared::Cancel(plan) => (
                plan.version,
                CommandJson::from_serializable(&CancelResult {
                    id: self.id.to_owned(),
                    status: "cancelled".into(),
                    version: plan.version,
                })?,
            ),
        };
        self.prepared = Some(prepared);
        Ok(WorkflowOutput {
            version,
            result,
            event_scope: None,
        })
    }

    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let now = self.clock.now().get();
        let actor = self.current.user_id().as_str();
        match self.prepared.take().ok_or_else(changed)? {
            Prepared::Dispatch(plan) => {
                apply_transit_dispatch(c, self.id, &plan, self.request.key.as_str(), actor, now)
                    .await?;
            }
            Prepared::Receive(plan) => {
                apply_transit_receive(c, self.id, &plan, self.request.key.as_str(), actor, now)
                    .await?;
            }
            Prepared::Loss(plan) => {
                apply_transit_loss_request(
                    c,
                    self.id,
                    &plan,
                    self.request.key.as_str(),
                    actor,
                    now,
                )
                .await?;
            }
            Prepared::Decide(plan, journal_id) => {
                apply_transit_loss_decision(
                    c,
                    self.id,
                    &plan,
                    self.request.key.as_str(),
                    actor,
                    journal_id.as_deref(),
                    now,
                )
                .await?;
            }
            Prepared::Cancel(plan) => {
                apply_transit_cancel(c, self.id, &plan, self.request.key.as_str(), actor, now)
                    .await?;
            }
        }
        if let Some((transfer_id, version)) = self.parent_transfer.take() {
            let updated = sqlx::query(
                "UPDATE inventory_transfers SET version=version+1,updated_at=$1 \
                 WHERE id=$2 AND company_id=$3 AND version=$4",
            )
            .bind(now)
            .bind(&transfer_id)
            .bind(self.company.as_str())
            .bind(version)
            .execute(&mut *c)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(changed().into());
            }
        }
        if let Some(journal) = self.journal.take() {
            PgJournalStore::new(self.current.holding_id().clone(), self.company.clone(), c)
                .persist(&journal)
                .await
                .map_err(journal_error)?;
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
