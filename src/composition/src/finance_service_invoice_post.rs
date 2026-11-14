//! Field-origin Finance invoice command on the selected native Holding unit.
//! No HTTP mounting or production activation is performed by this module.
use crate::finance_journal_create::{FinanceJournalCreateConfiguration, Ids};
use crate::finance_journal_review::{command_actor, finance_actor};
use erpx_server_commercial::public::purchases::*;
use erpx_server_field_service::public::finance_lineage::*;
use erpx_server_finance::public::{field_service_invoice_read::*, service_invoice_posting::*, *};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    selected_command_authority::require_selected_actor_current,
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind, UserId};
use erpx_server_workflow::public::commands::*;
use serde::Deserialize;
use sqlx::PgConnection;
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "INVOICE_SOURCE_CHANGED",
        "The retained Field invoice source changed.",
    )
}
fn journal_error(e: PgTransactionError<JournalError>) -> Error {
    match e {
        PgTransactionError::Persistence(e) => PgTransactionError::Persistence(e),
        PgTransactionError::Business(e) => {
            BusinessError::new(e.kind, e.code, "The invoice journal could not be posted.").into()
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    period_id: String,
    entry_date: String,
    expected_version: Option<i64>,
    fx_rate: Option<String>,
    #[serde(default)]
    attachment_set_ids: Vec<String>,
}
impl Input {
    fn validate(&self) -> Result<(), BusinessError> {
        validate_service_invoice_post_request(
            &self.period_id,
            &self.entry_date,
            self.expected_version,
            self.fx_rate.as_deref(),
            &self.attachment_set_ids,
        )
    }
}
#[derive(Clone)]
pub(crate) struct Sources;
impl HoldingWorkSourcePreparation for Sources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            erpx_server_finance::public::journal_review::lock_journal_review_sources(c).await?;
            lock_service_invoice_sources(c).await?;
            lock_field_service_invoice_finance_sources(c).await
        })
    }
}

/// The invoice ingress must acquire all owner locks before inspecting the
/// production release, since that inspection establishes the unit snapshot.
#[derive(Clone)]
struct InvoiceIngressSources(FinanceJournalCreateConfiguration);
impl HoldingWorkSourcePreparation for InvoiceIngressSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            Sources.prepare(c).await?;
            if matches!(
                self.0.environment(),
                crate::finance_journal_review::FinanceJournalEnvironment::Production
            ) {
                crate::finance_production_release::require_production_release(c, &self.0.company)
                    .await?;
            }
            Ok(())
        })
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum ReceiptAction {
    Create,
    Approve,
    Release,
    BankImport,
    Confirm,
}
impl ReceiptAction {
    fn operation(self) -> &'static str {
        match self {
            Self::Create => "finance.payment.create",
            Self::Approve => "finance.payment.approve",
            Self::Release => "finance.payment.release",
            Self::BankImport => "finance.bank.import",
            Self::Confirm => "finance.payment.confirm",
        }
    }
    fn prefix(self) -> &'static str {
        if self == Self::BankImport {
            "statement-"
        } else {
            "pay-"
        }
    }
}
pub(crate) struct Authority<'a, C> {
    current: &'a AuthorizedWorkContext,
    config: &'a FinanceJournalCreateConfiguration,
    clock: &'a C,
    purchase: Option<(CapabilityPurpose, PurchaseWitness)>,
    base: Option<(bool, PurchaseWitness)>,
    lineage: Option<CurrentFieldServiceInvoiceFinanceLineage>,
    receipt_invoice: Option<String>,
    receipt_payment: Option<String>,
    receipt_action: Option<ReceiptAction>,
    flow_assignment: bool,
    no_charge: bool,
    generic_payment: bool,
    generic_invoice_admitted: bool,
}
impl<C: Clock + Send + Sync> PgCommandAuthority for Authority<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Sources.prepare(c).await
    }
    async fn authorize(
        &mut self,
        c: &mut PgConnection,
        r: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        if self.current.holding_id() != &self.config.holding
            || self.current.selection().workspace().company_id != self.config.company
            || r.operation
                != self.receipt_action.map_or(
                    if self.flow_assignment {
                        "flow.assign"
                    } else if self.no_charge {
                        "finance.service-no-charge.confirm"
                    } else {
                        "finance.invoice.post"
                    },
                    ReceiptAction::operation,
                )
            || !r.record_id.starts_with(
                self.receipt_action
                    .map_or("field-invoice-", ReceiptAction::prefix),
            )
        {
            return Err(if self.no_charge {
                BusinessError::new(
                    FailureKind::Conflict,
                    "SERVICE_NO_CHARGE_SOURCE_CHANGED",
                    "Review the original fully covered Service billing source.",
                )
            } else {
                changed()
            }
            .into());
        }
        require_selected_actor_current(c, self.current, self.clock.now().get()).await?;
        if self.flow_assignment {
            self.current.require_permission("finance.read")?;
        } else if self.receipt_invoice.is_some() {
            if matches!(
                self.receipt_action,
                Some(ReceiptAction::Release | ReceiptAction::Confirm)
            ) {
                self.current.require_permission("finance.pay")?;
            } else {
                self.current.require_permission("finance.write")?;
            }
            if !self.current.has_permission("finance.read") {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "FINANCE_FORBIDDEN",
                    "A finance assignment is required.",
                )
                .into());
            }
            if self.receipt_action == Some(ReceiptAction::Approve)
                && !self.current.has_permission("finance.approve")
            {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "FINANCE_APPROVAL_REQUIRED",
                    "An independent finance approver is required.",
                )
                .into());
            }
            let scope = self.current.selection().workspace();
            if self.receipt_action != Some(ReceiptAction::BankImport)
                && (scope.branch_id.is_some() || scope.department_id.is_some())
            {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    if matches!(
                        self.receipt_action,
                        Some(ReceiptAction::Release | ReceiptAction::Confirm)
                    ) {
                        "COMPANY_TREASURY_REQUIRED"
                    } else {
                        "COMPANY_FINANCE_REQUIRED"
                    },
                    if matches!(
                        self.receipt_action,
                        Some(ReceiptAction::Release | ReceiptAction::Confirm)
                    ) {
                        "Use a Company treasury assignment to release a controlled payment."
                    } else if self.receipt_action == Some(ReceiptAction::Approve) {
                        "Use a Company finance assignment to approve a controlled payment."
                    } else {
                        "Use a Company finance assignment to prepare a controlled payment."
                    },
                )
                .into());
            }
            if !self.current.has_permission("finance.write") {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "READ_ONLY_ROLE",
                    "Auditors have read-only access to finance.",
                )
                .into());
            }
        } else {
            require_service_invoice_poster(
                self.current.has_permission("finance.read"),
                self.current.has_permission("finance.write"),
                self.current.has_permission("finance.approve"),
            )?;
        }
        self.current.require_permission("module.finance.read")?;
        Ok(command_actor(self.current)?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        r: &WorkflowRequest,
        d: CommandDisposition,
    ) -> Result<(), Error> {
        self.authorize(c, r).await?;
        let purpose = if d == CommandDisposition::Replay {
            CapabilityPurpose::History
        } else {
            CapabilityPurpose::SettleExisting
        };
        let mut reader = PgCompanyPurchaseReader::new(
            self.config.holding.clone(),
            self.config.company.clone(),
            c,
            &self.config.catalogue,
        );
        let base_allowed = if self.no_charge {
            false
        } else if let Some((allowed, witness)) = &self.base {
            let now_allowed = reader
                .revalidate(witness, self.clock.now())
                .await?
                .require_allowed()
                .is_ok();
            if now_allowed != *allowed {
                return Err(changed().into());
            }
            *allowed
        } else {
            let inspected = reader
                .inspect(
                    &PurchaseRequest {
                        capabilities: vec![
                            if self.receipt_invoice.is_some() {
                                "finance.banking"
                            } else {
                                "finance.accounting"
                            }
                            .into(),
                        ],
                        purpose,
                        mode: PackageEnforcementMode::Enforced,
                    },
                    self.clock.now(),
                )
                .await?;
            let allowed = inspected.require_allowed().is_ok();
            self.base = Some((allowed, inspected.witness));
            allowed
        };
        if let Some((p, w)) = &self.purchase {
            if *p != purpose {
                return Err(changed().into());
            }
            reader
                .revalidate(w, self.clock.now())
                .await?
                .require_allowed()?;
        } else {
            let found = reader
                .inspect(
                    &PurchaseRequest {
                        capabilities: vec![
                            if base_allowed && !self.no_charge {
                                if self.receipt_invoice.is_some() {
                                    "finance.banking"
                                } else {
                                    "finance.accounting"
                                }
                            } else {
                                "finance.service-invoice"
                            }
                            .into(),
                        ],
                        purpose,
                        mode: PackageEnforcementMode::Enforced,
                    },
                    self.clock.now(),
                )
                .await?;
            found.require_allowed()?;
            self.purchase = Some((purpose, found.witness));
        }
        if self.generic_payment {
            // Generic invoices have no Field release lineage; the owner
            // command revalidates the persisted invoice and purchase below.
        } else if let Some(lineage) = &self.lineage {
            lineage.require_current(c).await?;
        } else {
            let finance = read_service_invoice_workspace(
                c,
                &finance_actor(self.current),
                self.receipt_invoice.as_deref().unwrap_or(&r.record_id),
            )
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(e)
                    if self.no_charge
                        && matches!(e.code, "INVOICE_NOT_FOUND" | "INVOICE_SOURCE_CHANGED") =>
                {
                    PgTransactionError::Business(BusinessError::new(
                        FailureKind::Conflict,
                        "SERVICE_NO_CHARGE_SOURCE_CHANGED",
                        "Review the original fully covered Service billing source.",
                    ))
                }
                other => other,
            })?;
            let src = &finance.data().source;
            let row = &finance.data().invoice;
            let text = |k: &str| {
                row.get(k)
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
                    .ok_or_else(changed)
            };
            self.lineage = Some(
                read_field_service_invoice_finance_lineage(
                    c,
                    FieldServiceInvoiceFinanceRequest {
                        company_id: self.config.company.clone(),
                        order_id: src.order_id.clone(),
                        release_id: src.release_id.clone(),
                        key: src.command_key.clone(),
                        actor_id: src.actor_id.clone(),
                        currency: src.currency.clone(),
                        amount_units: src.amount_units.clone(),
                        customer_id: text("party_id")?,
                        pricing_date: text("invoice_date")?,
                    },
                )
                .await?
                .ok_or_else(changed)?,
            );
        }
        if self.flow_assignment && d==CommandDisposition::Execute && !erpx_server_identity_access::public::journal_flow_participants::service_invoice_flow_administrator(c,self.current,self.clock.now().get()).await? {
            return Err(BusinessError::new(FailureKind::Forbidden,"FLOW_ADMIN_REQUIRED","A company administrator must assign workflow responsibilities.").into());
        }
        if self.no_charge && d == CommandDisposition::Replay {
            let no_charge = erpx_server_finance::public::service_no_charge::read_service_no_charge(
                c,
                &finance_actor(self.current),
                &r.record_id,
            )
            .await?;
            crate::finance_service_no_charge::require_scope(&no_charge, self.current)?;
            no_charge.require_terminal(c, &r.key).await?;
        }
        if let Some(invoice) = &self.receipt_invoice {
            if self.generic_payment {
                let valid: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM finance_invoices i WHERE i.id=$1 AND i.company_id=$2                      AND i.kind IN ('payable','receivable') AND i.status IN ('posted','partially_paid')                      AND i.source_type IS DISTINCT FROM 'expense_claim')",
                )
                .bind(invoice)
                .bind(self.config.company.as_str())
                .fetch_one(&mut *c)
                .await?;
                if !valid {
                    if self.receipt_action == Some(ReceiptAction::Create)
                        && d == CommandDisposition::Execute
                        && !self.generic_invoice_admitted
                    {
                        return Err(BusinessError::invalid(
                            "PAYMENT_SETTLEMENT_SOURCE_INVALID",
                            "Choose an open invoice for the same party, currency and payment amount.",
                        ).into());
                    }
                    return Err(changed().into());
                }
                if d == CommandDisposition::Execute {
                    self.generic_invoice_admitted = true;
                }
            } else {
                erpx_server_finance::public::service_receipt::require_service_receipt_finance_lineage(
                    c,
                    &finance_actor(self.current),
                    self.receipt_payment.as_deref().unwrap_or(&r.record_id),
                    invoice,
                    self.receipt_action == Some(ReceiptAction::Create),
                )
                .await?;
            }
        }
        Ok(())
    }
}
struct JournalAuthorityPort<'a>(&'a AuthorizedWorkContext);
impl JournalAuthority for JournalAuthorityPort<'_> {
    type Error = PgTransactionError<JournalError>;
    async fn authorize(
        &mut self,
        actor: &FinanceActor,
        d: &JournalDraft,
    ) -> Result<(), Self::Error> {
        if actor != &finance_actor(self.0)
            || !self.0.has_permission("finance.approve")
            || !self.0.has_permission("finance.write")
            || d.source_type != "invoice"
            || d.is_opening
        {
            return Err(JournalError::new(
                "FINANCE_CURRENT_AUTHORITY_REQUIRED",
                "Choose the current Finance approver.",
            )
            .into());
        }
        Ok(())
    }
}
struct Effects<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    input: Input,
    source: Option<CurrentServiceInvoicePostSource>,
    plan: Option<JournalPreparation>,
    retained: Option<RetainedServiceInvoicePost>,
    journal_sources: Option<PgManualJournalCreateSources>,
}
impl<C: Clock + Send + Sync> PgOwnerCommandEffects for Effects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Sources.prepare(c).await
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        r: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let actor = finance_actor(self.current);
        let basis = read_service_invoice_workspace(c, &actor, &r.record_id).await?;
        let creator = UserId::new(
            basis
                .data()
                .invoice
                .get("created_by")
                .and_then(|v| v.as_str())
                .ok_or_else(changed)?
                .to_owned(),
        )?;
        let source = prepare_service_invoice_post_source(
            c,
            &actor,
            &r.record_id,
            self.input.expected_version,
            self.current.same_person_user(&creator),
            !self.input.attachment_set_ids.is_empty(),
        )
        .await?;
        let row = &source.workspace().invoice;
        let text = |k: &str| {
            row.get(k)
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .ok_or_else(changed)
        };
        let draft = JournalDraft {
            chart_id: source.chart_id().into(),
            period_id: self.input.period_id.clone(),
            entry_date: self.input.entry_date.clone(),
            currency: text("currency")?,
            fx_rate: self.input.fx_rate.clone(),
            is_opening: false,
            memo: format!("Invoice {}", text("invoice_number")?),
            source_type: "invoice".into(),
            source_id: Some(r.record_id.clone()),
            lines: source.lines().to_vec(),
        };
        self.journal_sources = Some(
            PgManualJournalCreateSources::capture(c, &actor, &draft)
                .await
                .map_err(journal_error)?,
        );
        let rules = embedded_currency_rules().map_err(|_| changed())?;
        let collation = FinanceCollation::english().map_err(|_| changed())?;
        let mut authority = JournalAuthorityPort(self.current);
        let mut ports =
            PgJournalPreparation::new(c, &actor, &mut authority, self.clock, &rules, &collation)
                .map_err(journal_error)?;
        let plan = prepare_journal(
            &mut ports,
            &actor,
            &draft,
            &CommandKey::new(&r.key)?,
            JournalStatus::Posted,
        )
        .await
        .map_err(journal_error)?;
        let result = CommandJson::from_serializable(
            &serde_json::json!({"id":r.record_id,"status":"posted","journalId":plan.journal_id()}),
        )?;
        let version = source.version() + 1;
        self.source = Some(source);
        self.plan = Some(plan);
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
        self.retained = Some(
            self.source
                .take()
                .ok_or_else(changed)?
                .apply(c, self.plan.as_ref().ok_or_else(changed)?)
                .await?,
        );
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.journal_sources
            .as_ref()
            .ok_or_else(changed)?
            .require_current(c)
            .await
            .map_err(journal_error)?;
        self.retained
            .as_ref()
            .ok_or_else(changed)?
            .require_current(c)
            .await
    }
}
/// Normalized input remains the exact Workflow request fingerprint; source and
/// purchase checks do not supply fresh effects before exact receipt replay.
pub async fn execute_service_invoice_post<C, F, P>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    selection: F,
    config: FinanceJournalCreateConfiguration,
    clock: Arc<C>,
    prepare: P,
) -> Result<CommandJson, Error>
where
    C: Clock + Send + Sync + 'static,
    F: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    P: Fn() -> Result<WorkflowRequest, BusinessError> + Clone + Send + Sync + 'static,
{
    let company = config.company.clone();
    let select = move || {
        let mut s = selection()?;
        if s.company_id != company {
            return Err(changed());
        }
        s.mutation = true;
        Ok(s)
    };
    authorizer
        .execute_with_sources_deferred(
            host,
            proof,
            select,
            WorkAuthorizationRequest {
                permission: "finance.write".into(),
                module: Some("finance".into()),
                destination: "finance/invoices".into(),
                api: true,
            },
            InvoiceIngressSources(config.clone()),
            move |c, current| {
                let config = config.clone();
                let clock = clock.clone();
                let prepare = prepare.clone();
                Box::pin(async move {
                    let request = prepare()?;
                    let input: Input = serde_json::from_value(service_invoice_post_physical_input(
                        &request.input,
                    )?)
                    .map_err(|_| {
                        BusinessError::invalid(
                            "VALIDATION_FAILED",
                            "Posting period, date and a valid exchange rate are required.",
                        )
                    })?;
                    input.validate()?;
                    let mut authority = Authority {
                        current,
                        config: &config,
                        clock: clock.as_ref(),
                        purchase: None,
                        base: None,
                        lineage: None,
                        flow_assignment:false,
        no_charge:false,
                        receipt_invoice: None,
                        receipt_payment: None,
                        receipt_action: None,
                        generic_payment:false,
                        generic_invoice_admitted:false,
                    };
                    let mut effects = Effects {
                        current,
                        clock: clock.as_ref(),
                        input,
                        source: None,
                        plan: None,
                        retained: None,
                        journal_sources: None,
                    };
                    let mut ids = Ids;
                    let outcome = PgCommandLedger::new(
                        config.company.clone(),
                        c,
                        &mut authority,
                        clock.as_ref(),
                        &mut ids,
                    )
                    .execute(&request, &mut effects)
                    .await?;
                    if !outcome.replayed {
                        // Project complete secondary journals before immediate
                        // evidence checks, using Finance's existing retained-effect order.
                        erpx_server_finance::public::service_receipt::flush_service_receipt_confirm_effects(c).await?;
                        effects
                            .retain(c, &command_actor(current)?, &request)
                            .await?;
                        authority
                            .verify_command(
                                c,
                                &command_actor(current)?,
                                &request,
                                CommandDisposition::Execute,
                            )
                            .await?;
                    }
                    Ok(outcome.result)
                })
            },
        )
        .await
}

/// Authenticated server-derived dispatch observation, separate from the command
/// transaction. A failed observation must never select a legacy writer.
pub async fn read_service_invoice_post_runtime<C, F>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    selection: F,
    config: FinanceJournalCreateConfiguration,
    clock: Arc<C>,
    id: String,
) -> Result<CommandJson, Error>
where
    C: Clock + Send + Sync + 'static,
    F: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
{
    let company = config.company.clone();
    authorizer
        .execute_with_sources_deferred(
            host,
            proof,
            move || {
                let mut scope = selection()?;
                if scope.company_id != company {
                    return Err(changed());
                }
                scope.mutation = false;
                Ok(scope)
            },
            WorkAuthorizationRequest {
                permission: "finance.read".into(),
                module: Some("finance".into()),
                destination: "finance/invoices".into(),
                api: true,
            },
            InvoiceIngressSources(config.clone()),
            move |c, current| {
                let id = id.clone();
                let clock = clock.clone();
                Box::pin(async move {
                    current.require_permission("module.finance.read")?;
                    require_selected_actor_current(c, current, clock.now().get()).await?;
                    let native =
                        service_invoice_post_runtime(c, &finance_actor(current), &id).await?;
                    require_selected_actor_current(c, current, clock.now().get()).await?;
                    Ok(CommandJson::from_serializable(&serde_json::json!({
                        "id": id, "runtime": if native { "rust" } else { "legacy" }
                    }))?)
                })
            },
        )
        .await
}

pub(crate) fn service_receipt_create_authority<'a, C: Clock + Send + Sync>(
    current: &'a AuthorizedWorkContext,
    config: &'a FinanceJournalCreateConfiguration,
    clock: &'a C,
    invoice: String,
) -> Authority<'a, C> {
    Authority {
        current,
        config,
        clock,
        purchase: None,
        base: None,
        lineage: None,
        flow_assignment: false,
        no_charge: false,
        receipt_invoice: Some(invoice),
        receipt_payment: None,
        receipt_action: Some(ReceiptAction::Create),
        generic_payment: false,
        generic_invoice_admitted: false,
    }
}

pub(crate) fn generic_payment_authority<'a, C: Clock + Send + Sync>(
    current: &'a AuthorizedWorkContext,
    config: &'a FinanceJournalCreateConfiguration,
    clock: &'a C,
    invoice: String,
    payment: Option<String>,
    action: ReceiptAction,
) -> Authority<'a, C> {
    Authority {
        current,
        config,
        clock,
        purchase: None,
        base: None,
        lineage: None,
        flow_assignment: false,
        no_charge: false,
        receipt_invoice: Some(invoice),
        receipt_payment: payment,
        receipt_action: Some(action),
        generic_payment: true,
        generic_invoice_admitted: false,
    }
}

pub(crate) fn service_receipt_review_authority<'a, C: Clock + Send + Sync>(
    current: &'a AuthorizedWorkContext,
    config: &'a FinanceJournalCreateConfiguration,
    clock: &'a C,
    invoice: String,
    payment: String,
    approve: bool,
) -> Authority<'a, C> {
    Authority {
        current,
        config,
        clock,
        purchase: None,
        base: None,
        lineage: None,
        flow_assignment: false,
        no_charge: false,
        receipt_invoice: Some(invoice),
        receipt_payment: Some(payment),
        receipt_action: Some(if approve {
            ReceiptAction::Approve
        } else {
            ReceiptAction::Release
        }),
        generic_payment: false,
        generic_invoice_admitted: false,
    }
}

pub(crate) fn service_receipt_bank_authority<'a, C: Clock + Send + Sync>(
    current: &'a AuthorizedWorkContext,
    config: &'a FinanceJournalCreateConfiguration,
    clock: &'a C,
    invoice: String,
    payment: String,
) -> Authority<'a, C> {
    Authority {
        current,
        config,
        clock,
        purchase: None,
        base: None,
        lineage: None,
        flow_assignment: false,
        no_charge: false,
        receipt_invoice: Some(invoice),
        receipt_payment: Some(payment),
        receipt_action: Some(ReceiptAction::BankImport),
        generic_payment: false,
        generic_invoice_admitted: false,
    }
}

pub(crate) fn service_invoice_flow_authority<'a, C: Clock + Send + Sync>(
    current: &'a AuthorizedWorkContext,
    config: &'a FinanceJournalCreateConfiguration,
    clock: &'a C,
) -> impl PgCommandAuthority + 'a {
    Authority {
        current,
        config,
        clock,
        purchase: None,
        base: None,
        lineage: None,
        receipt_invoice: None,
        receipt_payment: None,
        receipt_action: None,
        flow_assignment: true,
        no_charge: false,
        generic_payment: false,
        generic_invoice_admitted: false,
    }
}

pub(crate) fn service_no_charge_authority<'a, C: Clock + Send + Sync>(
    current: &'a AuthorizedWorkContext,
    config: &'a FinanceJournalCreateConfiguration,
    clock: &'a C,
) -> impl PgCommandAuthority + 'a {
    Authority {
        current,
        config,
        clock,
        purchase: None,
        base: None,
        lineage: None,
        receipt_invoice: None,
        receipt_payment: None,
        receipt_action: None,
        flow_assignment: false,
        no_charge: true,
        generic_payment: false,
        generic_invoice_admitted: false,
    }
}
