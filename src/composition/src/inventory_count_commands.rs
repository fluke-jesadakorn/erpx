//! Native Inventory generic stock-count save composition.
//!
//! The selected Account/Holding context, current IAM permission, Company
//! purchase admission, Inventory count source/effects and the Workflow command
//! ledger share the same serializable Holding unit. Count transitions keep the
//! existing Service Count native path or their retained adapter until a native
//! transition effect port exists; this mount owns draft create/revision only.
use crate::finance_journal_actor::{command_actor, finance_actor};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, current_package_catalogue, lock_commercial_authority_sources,
};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest, WorkPermissionRequirement,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_files::public::business_evidence::{
    PgBusinessEvidenceFiles, evidence_context, lock_business_evidence_sources, policy_fact,
};
use erpx_server_finance::public::{
    FinanceActor, FinanceCollation, JournalAuthority, JournalDraft, JournalError, JournalStatus,
    PgJournalPreparation, PgJournalStore, embedded_currency_rules, prepare_journal,
};
use erpx_server_inventory::public::counts::{
    CountRecord, CountSaveInput, CountSavePlan, CountSeedKind, CountSeedPlan, CountSeedSource,
    CountTransitionEffects, CountTransitionPlan, apply_count_save, apply_count_transition,
    insert_count_seed_source, lock_count_sources, plan_count_save, plan_count_seed,
    plan_count_transition, plan_count_transition_effects, read_count_save_source,
    read_count_transition_source,
};
use erpx_server_inventory::public::service_initial_physical_count::{
    CountAction, CountAuthority, CountStatus,
};
use erpx_server_records::public::business_evidence::{
    BusinessEvidenceRequest, prepare_business_evidence,
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
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

const COUNT_CAPABILITY: &str = "supply.counts";

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COUNT_VERSION_CHANGED",
        "The physical count changed. Reload its current state before submitting.",
    )
}

#[derive(Clone)]
pub struct InventoryCountCommand<C> {
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub holding: HoldingId,
    pub company: CompanyId,
    pub catalogue: Arc<CommercialCatalogue>,
    pub clock: Arc<C>,
}

#[derive(Serialize)]
struct CountSaveResult {
    id: String,
    status: String,
    version: i64,
}

/// Immutable first-Count provenance forwarded with an initial/manufacturing/
/// POS/service draft create. `kind` selects one closed owner table; `proof` is
/// the retained captured source proof, rechecked against the saved draft.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CountSeedInput {
    pub kind: String,
    pub proof: serde_json::Value,
}

/// The command receipt keeps the private source marker that the immutable
/// PostgreSQL triggers require; the HTTP client only sees the public draft.
fn strip_seed_marker(result: CommandJson) -> Result<CommandJson, Error> {
    let mut value: serde_json::Value = serde_json::from_str(result.as_str())
        .map_err(|_| PgTransactionError::Persistence(erpx_server_runtime::public::postgres::PersistenceError::new("COUNT_RESULT_INVALID")))?;
    if let Some(object) = value.as_object_mut() {
        object.retain(|key, _| !key.starts_with('_'));
    }
    CommandJson::from_serializable(&value).map_err(Into::into)
}

impl<C: Clock + Send + Sync + 'static> InventoryCountCommand<C> {
    #[allow(clippy::too_many_arguments)]
    pub async fn save<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        id: Option<String>,
        input: CountSaveInput,
        seed: Option<CountSeedInput>,
        wire: CommandJson,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        if seed.is_some() && id.is_some() {
            return Err(BusinessError::invalid(
                "COUNT_SEED_REVISION_NOT_ALLOWED",
                "A first physical count is immutable; create a separate count.",
            )
            .into());
        }
        if let Some(seed) = &seed
            && CountSeedKind::parse(&seed.kind).is_none()
        {
            return Err(BusinessError::invalid(
                "COUNT_SEED_PROOF_INVALID",
                "Use a current first-Count source.",
            )
            .into());
        }
        let record_id = id
            .clone()
            .unwrap_or_else(|| format!("count-{}", key.as_str()));
        if record_id.is_empty() || record_id.len() > 200 {
            return Err(BusinessError::invalid(
                "COUNT_INVALID",
                "Use a current stock Count command.",
            )
            .into());
        }
        let expected_company = self.company.clone();
        let company = self.company.clone();
        let clock = self.clock.clone();
        let operation = if id.is_some() {
            "inventory.count.update"
        } else {
            "inventory.count.create"
        }
        .to_owned();
        let request = WorkflowRequest {
            key: key.as_str().to_owned(),
            operation: operation.clone(),
            record_id: record_id.clone(),
            input: wire,
        };
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                workspace()?,
                WorkAuthorizationRequest {
                    permission: WorkPermissionRequirement::AnyOf(vec!["inventory.prepare".into()]),
                    module: Some("inventory".into()),
                    destination: operation,
                    api: true,
                },
                CountSources,
                move |c, current| {
                    let expected_company = expected_company.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let request = request.clone();
                    let input = input.clone();
                    let seed = seed.clone();
                    let record_id = record_id.clone();
                    Box::pin(async move {
                        if current.selection().workspace().company_id != expected_company {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "INVENTORY_SCOPE_REQUIRED",
                                "Use a Company or Branch inventory assignment.",
                            )
                            .into());
                        }
                        current.require_permission("inventory.prepare")?;
                        let mut authority = Authority {
                            current,
                            request: &request,
                            clock: clock.as_ref(),
                            catalogue: current_package_catalogue()?,
                            purchase: None,
                            permission: "inventory.prepare",
                        };
                        let mut effects = Effects {
                            current,
                            company: &company,
                            clock: clock.as_ref(),
                            request: &request,
                            input: &input,
                            record_id: &record_id,
                            seed,
                            plan: None,
                            seed_plan: None,
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
                        strip_seed_marker(outcome.result)
                    })
                },
            )
            .await
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CountTransitionInput {
    pub expected_version: i64,
    #[serde(default)]
    pub attachment_set_ids: Vec<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Serialize)]
struct CountTransitionResult {
    id: String,
    status: String,
    version: i64,
    #[serde(rename = "journalId")]
    journal_id: Option<String>,
}

fn action_name(action: CountAction) -> &'static str {
    match action {
        CountAction::Submit => "submit",
        CountAction::Approve => "approve",
        CountAction::Post => "post",
        CountAction::Reject => "reject",
        CountAction::Cancel => "cancel",
    }
}

fn action_status(action: CountAction) -> &'static str {
    match action {
        CountAction::Submit => "submitted",
        CountAction::Approve => "approved",
        CountAction::Post => "posted",
        CountAction::Reject => "rejected",
        CountAction::Cancel => "cancelled",
    }
}

fn action_permission(action: CountAction) -> &'static str {
    match action {
        CountAction::Submit | CountAction::Cancel => "inventory.prepare",
        CountAction::Approve | CountAction::Post | CountAction::Reject => "inventory.approve",
    }
}

fn count_status(value: &str) -> Option<CountStatus> {
    match value {
        "draft" => Some(CountStatus::Draft),
        "submitted" => Some(CountStatus::Submitted),
        "approved" => Some(CountStatus::Approved),
        "rejected" => Some(CountStatus::Rejected),
        "cancelled" => Some(CountStatus::Cancelled),
        "posted" => Some(CountStatus::Posted),
        _ => None,
    }
}

fn journal_error(e: PgTransactionError<JournalError>) -> Error {
    match e {
        PgTransactionError::Persistence(e) => PgTransactionError::Persistence(e),
        PgTransactionError::Business(e) => BusinessError::new(
            e.kind,
            e.code,
            "The stock Count Finance posting is unavailable.",
        )
        .into(),
    }
}

fn decimal_minor(value: i64) -> String {
    let sign = if value < 0 { "-" } else { "" };
    let value = value.abs();
    format!("{sign}{}.{:02}", value / 100, value % 100)
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
                code: "COUNT_FINANCE_BINDING_REQUIRED",
                message: "Use the currently authorized Count posting.".into(),
            }
            .into());
        }
        Ok(())
    }
}

struct CountPostingConfig {
    chart_id: String,
    inventory_account_id: String,
    variance_account_id: String,
    currency: String,
    period_id: String,
}

async fn read_count_posting_config(
    c: &mut PgConnection,
    company: &str,
    count_date: &str,
) -> Result<CountPostingConfig, Error> {
    let config: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT s.chart_id,s.inventory_account_id,c.base_currency,a.id \
         FROM commerce_settings s \
         JOIN finance_charts c ON c.id=s.chart_id AND c.company_id=s.company_id \
         JOIN finance_accounts a ON a.chart_id=s.chart_id AND a.company_id=s.company_id \
           AND a.currency=c.base_currency AND a.code='5400' AND a.category='expense' \
           AND a.status='active' \
         WHERE s.company_id=$1",
    )
    .bind(company)
    .fetch_optional(&mut *c)
    .await?;
    let Some((chart_id, inventory_account_id, currency, variance_account_id)) = config else {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "VARIANCE_MAPPING_REQUIRED",
            "Configure the stock count variance account (5400) in the operations ledger.",
        )
        .into());
    };
    let periods: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM finance_periods WHERE company_id=$1 AND state='open' \
         AND start_date<=$2 AND end_date>=$2",
    )
    .bind(company)
    .bind(count_date)
    .fetch_all(&mut *c)
    .await?;
    if periods.len() != 1 {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "PERIOD_UNAVAILABLE",
            "Exactly one open period must cover the count date.",
        )
        .into());
    }
    Ok(CountPostingConfig {
        chart_id,
        inventory_account_id,
        variance_account_id,
        currency,
        period_id: periods.into_iter().next().expect("one period"),
    })
}

impl<C: Clock + Send + Sync + 'static> InventoryCountCommand<C> {
    #[allow(clippy::too_many_arguments)]
    pub async fn transition<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        id: String,
        action: CountAction,
        input: CountTransitionInput,
        wire: CommandJson,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        if id.is_empty() || id.len() > 200 {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "COUNT_NOT_FOUND",
                "Stock count not found in this workspace.",
            )
            .into());
        }
        let expected_company = self.company.clone();
        let company = self.company.clone();
        let clock = self.clock.clone();
        let operation = format!("inventory.count.{}", action_name(action));
        let permission = action_permission(action).to_owned();
        let request = WorkflowRequest {
            key: key.as_str().to_owned(),
            operation: operation.clone(),
            record_id: id.clone(),
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
                CountSources,
                move |c, current| {
                    let expected_company = expected_company.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let request = request.clone();
                    let input = input.clone();
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
                        let mut effects = TransitionEffects {
                            current,
                            company: &company,
                            clock: clock.as_ref(),
                            request: &request,
                            id: &id,
                            action,
                            input: &input,
                            record: None,
                            plan: None,
                            effects: None,
                            journal: None,
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
struct CountSources;

impl HoldingWorkSourcePreparation for CountSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            // Ordered owner fences: Commercial -> Inventory.
            lock_commercial_authority_sources(c).await?;
            lock_count_sources(c).await?;
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
        HoldingWorkSourcePreparation::prepare(&CountSources, c).await
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
            CommandDisposition::Execute
                if matches!(
                    self.request.operation.as_str(),
                    "inventory.count.submit"
                        | "inventory.count.create"
                        | "inventory.count.update"
                ) =>
            {
                CapabilityPurpose::NewWork
            }
            CommandDisposition::Execute => CapabilityPurpose::SettleExisting,
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
                            capabilities: vec![COUNT_CAPABILITY.into()],
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

struct Effects<'a, C> {
    current: &'a AuthorizedWorkContext,
    company: &'a CompanyId,
    clock: &'a C,
    request: &'a WorkflowRequest,
    input: &'a CountSaveInput,
    record_id: &'a str,
    seed: Option<CountSeedInput>,
    plan: Option<CountSavePlan>,
    seed_plan: Option<CountSeedPlan>,
}

impl<C: Clock + Sync> PgOwnerCommandEffects for Effects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        HoldingWorkSourcePreparation::prepare(&CountSources, c).await
    }

    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let workspace = self.current.selection().workspace();
        let product_ids: Vec<String> = self
            .input
            .lines
            .iter()
            .map(|line| line.product_id.clone())
            .collect();
        let source = read_count_save_source(
            c,
            self.company.as_str(),
            workspace.branch_id.as_ref().map(|id| id.as_str()),
            if self.request.operation == "inventory.count.update" {
                Some(self.record_id)
            } else {
                None
            },
            &self.input.warehouse_id,
            &product_ids,
        )
        .await?;
        let plan = plan_count_save(&source, self.input, self.record_id, self.company.as_str())?;
        let version = plan.version;
        let seed_plan = match (&self.seed, plan.lines.first()) {
            (Some(seed), Some(line)) => {
                let kind = CountSeedKind::parse(&seed.kind)
                    .ok_or_else(|| BusinessError::invalid("COUNT_SEED_PROOF_INVALID", "Use a current first-Count source."))?;
                Some(plan_count_seed(
                    kind,
                    &seed.proof,
                    &plan.record_id,
                    &plan.company_id,
                    &plan.warehouse_id,
                    &plan.branch_id,
                    &line.product_id,
                    &self.request.key,
                )?)
            }
            (Some(_), None) => {
                return Err(BusinessError::invalid(
                    "COUNT_SEED_PROOF_INVALID",
                    "Use a current first-Count source.",
                )
                .into())
            }
            (None, _) => None,
        };
        let result = if let Some(seed_plan) = &seed_plan {
            let mut value = serde_json::to_value(CountSaveResult {
                id: self.record_id.to_owned(),
                status: "draft".into(),
                version,
            })
            .map_err(|_| changed())?;
            if let Some(object) = value.as_object_mut() {
                object.insert(
                    seed_plan.kind.marker().to_owned(),
                    seed_plan.proof.clone(),
                );
            }
            CommandJson::from_serializable(&value)?
        } else {
            CommandJson::from_serializable(&CountSaveResult {
                id: self.record_id.to_owned(),
                status: "draft".into(),
                version,
            })?
        };
        self.plan = Some(plan);
        self.seed_plan = seed_plan;
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
        let plan = self.plan.take().ok_or_else(changed)?;
        let now = self.clock.now().get();
        let actor = self.current.user_id().as_str();
        apply_count_save(c, &plan, self.input.expected_version, actor, now).await?;
        if let Some(seed) = self.seed_plan.take() {
            let proof_json = serde_json::to_string(&seed.proof).map_err(|_| changed())?;
            insert_count_seed_source(
                c,
                &CountSeedSource {
                    kind: seed.kind,
                    count_id: &plan.record_id,
                    company_id: &plan.company_id,
                    branch_id: &seed.branch_id,
                    warehouse_id: &plan.warehouse_id,
                    product_id: &seed.product_id,
                    count_command_id: &self.request.key,
                    setup_command_id: &seed.setup_command_id,
                    chart_id: &seed.chart_id,
                    inventory_account_id: &seed.inventory_account_id,
                    variance_account_id: &seed.variance_account_id,
                    currency: &seed.currency,
                    proof_json: &proof_json,
                    created_by: actor,
                    created_at: now,
                    pos_bootstrap_receipt_id: seed.pos_bootstrap_receipt_id.as_deref(),
                    pos_approval_receipt_id: seed.pos_approval_receipt_id.as_deref(),
                },
            )
            .await?;
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

struct TransitionEffects<'a, C> {
    current: &'a AuthorizedWorkContext,
    company: &'a CompanyId,
    clock: &'a C,
    request: &'a WorkflowRequest,
    id: &'a str,
    action: CountAction,
    input: &'a CountTransitionInput,
    record: Option<CountRecord>,
    plan: Option<CountTransitionPlan>,
    effects: Option<CountTransitionEffects>,
    journal: Option<erpx_server_finance::public::JournalPreparation>,
    evidence: Option<erpx_server_files::public::business_evidence::PreparedBusinessEvidence>,
}

impl<C: Clock + Sync> PgOwnerCommandEffects for TransitionEffects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        HoldingWorkSourcePreparation::prepare(&CountSources, c).await
    }

    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let now = self.clock.now();
        let workspace = self.current.selection().workspace();
        let (record, lines) = read_count_transition_source(
            c,
            self.company.as_str(),
            workspace.branch_id.as_ref().map(|id| id.as_str()),
            self.id,
        )
        .await?;
        if record.quality_hold_id.is_some() {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "COUNT_QUALITY_DISPOSITION_REQUIRED",
                "Use the quality disposition command for a controlled stock count.",
            )
            .into());
        }
        let status = count_status(&record.status).ok_or_else(changed)?;
        let authority = CountAuthority {
            same_as_creator: record.created_by == self.current.user_id().as_str(),
            same_as_preparer: record.prepared_by == self.current.user_id().as_str(),
            company_scope: workspace.branch_id.is_none(),
            department_scope: workspace.department_id.is_some(),
            can_prepare: self.current.has_permission("inventory.prepare"),
            can_approve: self.current.has_permission("inventory.approve"),
        };
        let plan = plan_count_transition(
            &record.status,
            record.version,
            self.input.expected_version,
            self.action,
            &authority,
            self.input.reason.as_deref(),
            self.input.attachment_set_ids.len(),
        )?;
        let effects = plan_count_transition_effects(&plan, &lines)?;
        let active: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM commerce_warehouses \
             WHERE id=$1 AND company_id=$2 AND status='active')",
        )
        .bind(&record.warehouse_id)
        .bind(self.company.as_str())
        .fetch_one(&mut *c)
        .await?;
        if !active {
            return Err(BusinessError::invalid(
                "WAREHOUSE_INVALID",
                "Choose an active warehouse in your scope.",
            )
            .into());
        }
        let mut journal = None;
        if effects.posting {
            let config =
                read_count_posting_config(c, self.company.as_str(), &record.count_date).await?;
            let mut journal_lines = Vec::new();
            if effects.gains_minor > 0 {
                let amount = decimal_minor(effects.gains_minor);
                journal_lines.push(erpx_server_finance::public::JournalLineInput {
                    account_id: config.inventory_account_id.clone(),
                    description: None,
                    debit: amount.clone(),
                    credit: "0.00".into(),
                    dimensions: None,
                });
                journal_lines.push(erpx_server_finance::public::JournalLineInput {
                    account_id: config.variance_account_id.clone(),
                    description: None,
                    debit: "0.00".into(),
                    credit: amount,
                    dimensions: None,
                });
            }
            if effects.losses_minor > 0 {
                let amount = decimal_minor(effects.losses_minor);
                journal_lines.push(erpx_server_finance::public::JournalLineInput {
                    account_id: config.variance_account_id.clone(),
                    description: None,
                    debit: amount.clone(),
                    credit: "0.00".into(),
                    dimensions: None,
                });
                journal_lines.push(erpx_server_finance::public::JournalLineInput {
                    account_id: config.inventory_account_id.clone(),
                    description: None,
                    debit: "0.00".into(),
                    credit: amount,
                    dimensions: None,
                });
            }
            if !journal_lines.is_empty() {
                let draft = JournalDraft {
                    chart_id: config.chart_id,
                    period_id: config.period_id,
                    entry_date: record.count_date.clone(),
                    currency: config.currency,
                    fx_rate: None,
                    is_opening: false,
                    memo: format!(
                        "{} · {}",
                        record.number,
                        self.input.reason.as_deref().unwrap_or("")
                    ),
                    source_type: "stock_count".into(),
                    source_id: Some(record.id.clone()),
                    lines: journal_lines,
                };
                let actor = finance_actor(self.current);
                let mut bound = BoundJournal {
                    actor: actor.clone(),
                    draft: draft.clone(),
                };
                let rules = embedded_currency_rules().map_err(|_| changed())?;
                let collation = FinanceCollation::english().map_err(|_| changed())?;
                let mut ports = PgJournalPreparation::new(
                    c,
                    &actor,
                    &mut bound,
                    self.clock,
                    &rules,
                    &collation,
                )
                .map_err(journal_error)?;
                journal = Some(
                    prepare_journal(
                        &mut ports,
                        &actor,
                        &draft,
                        &CommandKey::new(self.request.key.clone())?,
                        JournalStatus::Posted,
                    )
                    .await
                    .map_err(journal_error)?,
                );
            }
        }
        let mut evidence = None;
        if self.action == CountAction::Submit && !self.input.attachment_set_ids.is_empty() {
            let mut files = PgBusinessEvidenceFiles::new(
                c,
                self.company.clone(),
                self.current.user_id().as_str().to_owned(),
                workspace
                    .branch_id
                    .as_ref()
                    .map(|id| id.as_str().to_owned()),
                None,
                vec![policy_fact("stock_count", "count_sheet", &["action"], 1)],
                now,
            );
            evidence = prepare_business_evidence(
                &mut files,
                BusinessEvidenceRequest {
                    attachment_set_ids: self.input.attachment_set_ids.clone(),
                    target_types: vec!["stock_count".into(), "inventory_stock_count".into()],
                    contexts: ["count_sheet", "count_photo", "count_evidence"]
                        .into_iter()
                        .map(|purpose| evidence_context(purpose, "action", Some("submit")))
                        .collect(),
                    target_id: record.id.clone(),
                    expected_target_version: self.input.expected_version,
                    command_id: self.request.key.clone(),
                    binding_type: "stock_count_submission".into(),
                    binding_id: format!("count-submit-{}", self.request.key),
                },
            )
            .await?;
        }
        let journal_id = journal.as_ref().map(|plan| plan.journal_id().to_owned());
        let version = record.version + 1;
        let result = CommandJson::from_serializable(&CountTransitionResult {
            id: record.id.clone(),
            status: action_status(self.action).into(),
            version,
            journal_id,
        })?;
        self.record = Some(record);
        self.plan = Some(plan);
        self.effects = Some(effects);
        self.journal = journal;
        self.evidence = evidence;
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
        let record = self.record.take().ok_or_else(changed)?;
        let effects = self.effects.take().ok_or_else(changed)?;
        let journal_id = self
            .journal
            .as_ref()
            .map(|plan| plan.journal_id().to_owned());
        apply_count_transition(
            c,
            &record,
            &effects,
            self.input.expected_version,
            actor,
            journal_id.as_deref(),
            now,
        )
        .await?;
        if self.action != CountAction::Submit {
            sqlx::query(
                "INSERT INTO inventory_count_decisions(id,count_id,command_id,action,actor_id,\
                 reviewed_version,reason,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
            )
            .bind(format!("decision-{}", self.request.key))
            .bind(&record.id)
            .bind(&self.request.key)
            .bind(action_name(self.action))
            .bind(actor)
            .bind(self.input.expected_version)
            .bind(self.input.reason.as_deref())
            .bind(now)
            .execute(&mut *c)
            .await?;
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
