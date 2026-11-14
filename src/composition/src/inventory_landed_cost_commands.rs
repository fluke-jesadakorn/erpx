//! Native Inventory landed-cost command composition.
//!
//! Selected Account/Holding context, current Finance authority, Company purchase
//! admission, Inventory receipt/stock/lot effects, the Workflow command ledger
//! and the Finance revaluation journal share one serializable Holding unit.
use crate::finance_journal_actor::{command_actor, finance_actor};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, current_package_catalogue, lock_commercial_authority_sources,
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
use erpx_server_inventory::public::landed_costs::{
    LandedCostAction, LandedCostActionInput, LandedCostCreateInput, LandedCostCreatePlan,
    LandedCostTransitionPlan, apply_landed_cost_create, apply_landed_cost_transition,
    landed_cost_credit_account, lock_landed_cost_sources, plan_create, plan_transition,
    read_landed_cost_create_source, read_landed_cost_transition_source,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, IdentifierSource, RecordId,
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

const CAPABILITIES: [&str; 3] = [
    "supply.advanced-inventory",
    "supply.receiving",
    "supply.stock",
];
const FINANCE_CAPABILITY: &str = "finance.inventory-valuation";

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "LANDED_COST_STATUS_INVALID",
        "The landed cost changed. Reload it before reviewing.",
    )
}

fn number(record: &str) -> String {
    let digest = Sha256::digest(record.as_bytes());
    let hex: String = digest.iter().take(10).map(|byte| format!("{byte:02X}")).collect();
    format!("LC-{hex}")
}

#[derive(Clone, Debug)]
enum LandedCostWrite {
    Create(LandedCostCreateInput),
    Transition(String, LandedCostAction, LandedCostActionInput),
}

impl LandedCostWrite {
    fn operation(&self) -> String {
        match self {
            Self::Create(_) => "inventory.landed_cost.create".into(),
            Self::Transition(_, action, _) => action.operation(),
        }
    }

    fn permission(&self) -> &'static str {
        match self {
            Self::Create(_) => "finance.write",
            Self::Transition(_, LandedCostAction::Cancel, _) => "finance.write",
            Self::Transition(..) => "finance.approve",
        }
    }

    fn purpose(&self) -> CapabilityPurpose {
        match self {
            Self::Create(_) => CapabilityPurpose::NewWork,
            _ => CapabilityPurpose::SettleExisting,
        }
    }

    fn financial(&self) -> bool {
        matches!(self, Self::Transition(_, LandedCostAction::Post, _))
    }

    fn record_id(&self, key: &str) -> String {
        match self {
            Self::Create(_) => format!("landed-{key}"),
            Self::Transition(id, _, _) => id.clone(),
        }
    }

}

#[derive(Clone)]
pub struct InventoryLandedCostCommand<C> {
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub holding: HoldingId,
    pub company: CompanyId,
    pub catalogue: Arc<CommercialCatalogue>,
    pub clock: Arc<C>,
}

#[derive(Serialize)]
struct LandedCostResult {
    id: String,
    status: String,
    version: i64,
    #[serde(rename = "journalId")]
    journal_id: Option<String>,
}

impl<C: Clock + Send + Sync + 'static> InventoryLandedCostCommand<C> {
    pub async fn create<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        input: LandedCostCreateInput,
        wire: CommandJson,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        self.run(host, proof, workspace, LandedCostWrite::Create(input), wire, key)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn transition<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        id: String,
        action: LandedCostAction,
        input: LandedCostActionInput,
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
            LandedCostWrite::Transition(id, action, input),
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
        write: LandedCostWrite,
        wire: CommandJson,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let record = write.record_id(key.as_str());
        let expected_company = self.company.clone();
        let company = self.company.clone();
        let clock = self.clock.clone();
        let operation = write.operation();
        let permission = write.permission().to_owned();
        let purpose = write.purpose();
        let financial = write.financial();
        let request = WorkflowRequest {
            key: key.as_str().to_owned(),
            operation: operation.clone(),
            record_id: record,
            input: wire,
        };
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                workspace()?,
                WorkAuthorizationRequest {
                    permission: WorkPermissionRequirement::AnyOf(vec![
                        permission.clone(),
                        "finance.read".into(),
                    ]),
                    module: Some("inventory".into()),
                    destination: operation,
                    api: true,
                },
                LandedCostSources,
                move |c, current| {
                    let expected_company = expected_company.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let write = write.clone();
                    let request = request.clone();
                    let permission = permission.clone();
                    let purpose = purpose;
                    Box::pin(async move {
                        if current.selection().workspace().company_id != expected_company {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "FINANCE_SCOPE_REQUIRED",
                                "Use a Branch or Company Finance assignment.",
                            )
                            .into());
                        }
                        if current.selection().workspace().department_id.is_some() {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "FINANCE_SCOPE_REQUIRED",
                                "Use a Branch or Company Finance assignment.",
                            )
                            .into());
                        }
                        current.require_permission(&permission)?;
                        current.require_permission("finance.read")?;
                        let mut authority = Authority {
                            current,
                            request: &request,
                            clock: clock.as_ref(),
                            catalogue: current_package_catalogue()?,
                            purchase: None,
                            permission: &permission,
                            purpose,
                            financial,
                        };
                        let mut effects = Effects {
                            current,
                            company: &company,
                            clock: clock.as_ref(),
                            request: &request,
                            write: &write,
                            prepared: None,
                            journal: None,
                            journal_id: None,
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
struct LandedCostSources;

impl HoldingWorkSourcePreparation for LandedCostSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_landed_cost_sources(c).await?;
            sqlx::query(
                "LOCK TABLE finance_journals,finance_journal_lines,finance_periods,\
                 finance_accounts,finance_charts IN SHARE ROW EXCLUSIVE MODE",
            )
            .execute(&mut *c)
            .await?;
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
    financial: bool,
}

impl<C: Clock + Sync> PgCommandAuthority for Authority<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        HoldingWorkSourcePreparation::prepare(&LandedCostSources, c).await
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
        let mut capabilities: Vec<String> =
            CAPABILITIES.iter().map(|value| (*value).to_owned()).collect();
        if self.financial {
            capabilities.push(FINANCE_CAPABILITY.to_owned());
        }
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
                            capabilities,
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
                code: "LANDED_COST_FINANCE_BINDING_REQUIRED",
                message: "Use the currently authorized landed-cost posting.".into(),
            }
            .into());
        }
        Ok(())
    }
}

fn journal_error(e: PgTransactionError<JournalError>) -> Error {
    match e {
        PgTransactionError::Persistence(e) => PgTransactionError::Persistence(e),
        PgTransactionError::Business(e) => BusinessError::new(
            e.kind,
            e.code,
            "The landed-cost Finance posting is unavailable.",
        )
        .into(),
    }
}

fn decimal_minor(value: i64) -> String {
    let sign = if value < 0 { "-" } else { "" };
    let value = value.abs();
    format!("{sign}{}.{:02}", value / 100, value % 100)
}

enum Prepared {
    Create(LandedCostCreatePlan),
    Transition(LandedCostTransitionPlan),
}

struct Effects<'a, C> {
    current: &'a AuthorizedWorkContext,
    company: &'a CompanyId,
    clock: &'a C,
    request: &'a WorkflowRequest,
    write: &'a LandedCostWrite,
    prepared: Option<Prepared>,
    journal: Option<erpx_server_finance::public::JournalPreparation>,
    journal_id: Option<String>,
}

impl<C: Clock + Sync> PgOwnerCommandEffects for Effects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        HoldingWorkSourcePreparation::prepare(&LandedCostSources, c).await
    }

    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let actor = self.current.user_id().as_str().to_owned();
        let workspace = self.current.selection().workspace();
        let branch = workspace.branch_id.as_ref().map(|id| id.as_str());
        match self.write {
            LandedCostWrite::Create(input) => {
                let source = read_landed_cost_create_source(
                    c,
                    self.company.as_str(),
                    branch,
                    &input.fulfillment_id,
                )
                .await?;
                let credit = landed_cost_credit_account(
                    c,
                    self.company.as_str(),
                    &source.receipt.chart_id,
                    &input.credit_account_id,
                )
                .await?;
                let source = erpx_server_inventory::public::landed_costs::LandedCostCreateSource {
                    credit_account_exists: credit,
                    ..source
                };
                let record = format!("landed-{}", self.request.key);
                let plan = plan_create(
                    &source,
                    input,
                    self.company.as_str(),
                    &actor,
                    &record,
                    &number(&record),
                )?;
                self.prepared = Some(Prepared::Create(plan.clone()));
                Ok(WorkflowOutput {
                    version: 1,
                    result: CommandJson::from_serializable(&LandedCostResult {
                        id: plan.id,
                        status: "submitted".into(),
                        version: 1,
                        journal_id: None,
                    })?,
                    event_scope: None,
                })
            }
            LandedCostWrite::Transition(id, action, input) => {
                let source = read_landed_cost_transition_source(
                    c,
                    self.company.as_str(),
                    branch,
                    id,
                )
                .await?;
                let period = if source.periods.len() == 1 {
                    Some(source.periods[0].clone())
                } else {
                    None
                };
                let plan = plan_transition(
                    &source,
                    input,
                    *action,
                    &actor,
                    branch,
                    period,
                )?;
                let mut journal_plan = None;
                if *action == LandedCostAction::Post {
                    let journal = plan.journal.as_ref().ok_or_else(changed)?;
                    let draft = JournalDraft {
                        chart_id: journal.chart_id.clone(),
                        period_id: journal.period_id.clone(),
                        entry_date: journal.entry_date.clone(),
                        currency: journal.currency.clone(),
                        fx_rate: None,
                        is_opening: false,
                        memo: journal.memo.clone(),
                        source_type: "landed_cost".into(),
                        source_id: Some(plan.id.clone()),
                        lines: vec![
                            erpx_server_finance::public::JournalLineInput {
                                account_id: journal.debit_account_id.clone(),
                                description: None,
                                debit: decimal_minor(journal.amount_minor),
                                credit: "0.00".into(),
                                dimensions: None,
                            },
                            erpx_server_finance::public::JournalLineInput {
                                account_id: journal.credit_account_id.clone(),
                                description: None,
                                debit: "0.00".into(),
                                credit: decimal_minor(journal.amount_minor),
                                dimensions: None,
                            },
                        ],
                    };
                    let actor_finance = finance_actor(self.current);
                    let mut authority = BoundJournal {
                        actor: actor_finance.clone(),
                        draft: draft.clone(),
                    };
                    let rules = embedded_currency_rules().map_err(|_| changed())?;
                    let collation = FinanceCollation::english().map_err(|_| changed())?;
                    let mut ports = PgJournalPreparation::new(
                        c,
                        &actor_finance,
                        &mut authority,
                        self.clock,
                        &rules,
                        &collation,
                    )
                    .map_err(journal_error)?;
                    let prepared = prepare_journal(
                        &mut ports,
                        &actor_finance,
                        &draft,
                        &CommandKey::new(self.request.key.clone())?,
                        JournalStatus::Posted,
                    )
                    .await
                    .map_err(journal_error)?;
                    journal_plan = Some(prepared);
                }
                self.journal_id = journal_plan
                    .as_ref()
                    .map(|journal| journal.journal_id().to_owned());
                self.journal = journal_plan;
                self.prepared = Some(Prepared::Transition(plan.clone()));
                Ok(WorkflowOutput {
                    version: plan.version,
                    result: CommandJson::from_serializable(&LandedCostResult {
                        id: plan.id,
                        status: plan.status.to_owned(),
                        version: plan.version,
                        journal_id: self.journal_id.clone(),
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
            Prepared::Create(plan) => {
                apply_landed_cost_create(c, &plan, self.request.key.as_str(), &actor, now).await?;
            }
            Prepared::Transition(plan) => {
                apply_landed_cost_transition(
                    c,
                    &plan,
                    self.request.key.as_str(),
                    &actor,
                    now,
                    self.journal_id.as_deref(),
                )
                .await?;
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
        Ok(())
    }
}

struct Ids;

impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("generated UUID")
    }
}
