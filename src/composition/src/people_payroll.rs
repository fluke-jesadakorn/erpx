//! Native People Payroll read assembly. Current Account/Holding authority
//! remains with the selected coordinator; owner ports supply the bounded
//! payroll summary and run-list facts.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest, WorkPermissionRequirement,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_people_payroll::public::payroll_summary::{
    PayrollReadActor, PgPayrollRead, payroll_runs, payroll_summary,
};
use erpx_server_commercial::public::purchase_after::write_purchase_shadow_checked_observation;
use erpx_server_commercial::public::purchase_command_boundary::{
    PurchaseCommandBoundary, PurchaseCommandScope,
};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, PgCompanyPurchaseReader, PurchaseRequest,
    PurchaseWitness, current_package_catalogue,
};
use erpx_server_finance::public::{
    FinanceActor, FinanceCollation, JournalAuthority, JournalDraft, JournalError, JournalStatus,
    PgJournalPreparation, PgJournalStore, embedded_currency_rules, prepare_journal,
};
use erpx_server_people_payroll::public::payroll_run::{
    PayrollCommandActor, PayrollCommandFuture, PayrollJournalDraft, PayrollJournalPort,
    PayrollJournalRequest, PayrollPreparedJournal, calculate_payroll_run, cancel_payroll_run,
    create_payroll_run, payroll_run_detail, post_payroll,
};
use erpx_server_people_payroll::public::payroll_run::PgPayrollCommand;
use erpx_server_people_payroll::public::payroll_run_http::{
    PayrollCommandHttpBackend, PayrollCommandHttpFuture, PayrollCommandHttpKind,
    PayrollCommandHttpRequest, PayrollCommandHttpResponse, PayrollCommandHttpTarget,
};
use erpx_server_people_payroll::public::payroll_summary_http::{
    PayrollReadHttpBackend, PayrollReadHttpFuture, PayrollReadHttpKind, PayrollReadHttpRequest,
    PayrollReadHttpResponse, PayrollReadHttpTarget, people_payroll_router,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CompanyId, FailureKind, HoldingId, UserId,
};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PEOPLE_PAYROLL_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_PAYROLL_UNAVAILABLE",
        "The People Payroll workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct PeoplePayrollSources;

impl HoldingWorkSourcePreparation for PeoplePayrollSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct PeoplePayrollConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl PeoplePayrollConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn payroll_actor(current: &AuthorizedWorkContext) -> Result<PayrollReadActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    Ok(PayrollReadActor {
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        permissions: current.command_iam_metadata().permissions.clone(),
        expires_at: current.expires_at(),
    })
}

fn context(current: &AuthorizedWorkContext) -> Value {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    serde_json::json!({
        "userId": current.user_id().as_str(),
        "holdingId": current.holding_id().as_str(),
        "companyId": workspace.company_id.as_str(),
        "branchId": workspace.branch_id.as_ref().map(|value| value.as_str()),
        "departmentId": workspace.department_id.as_ref().map(|value| value.as_str()),
        "branchSlug": workspace.branch_slug,
        "departmentSlug": workspace.department_slug,
        "assignmentId": assignment.id.as_str(),
        "assignmentVersion": assignment.version,
        "revision": selected.revision(),
        "role": assignment.role,
        "assignmentScopeKind": match assignment.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        },
        "modules": current.modules(),
        "permissions": current.command_iam_metadata().permissions,
        "expiresAt": current.expires_at(),
    })
}

pub struct PeoplePayrollService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: PeoplePayrollConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> PeoplePayrollService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: PeoplePayrollConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }

    pub fn company(&self) -> &CompanyId {
        self.configuration.company()
    }

    fn access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: WorkPermissionRequirement::AnyOf(vec![
                "payroll.read".into(),
                "payroll.payment.read".into(),
            ]),
            module: Some("people_payroll".into()),
            destination: "people/payroll".into(),
            api: true,
        }
    }

    fn input<Input>(
        &self,
        input: Input,
    ) -> impl FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = false;
            Ok(request)
        }
    }

    pub async fn summary<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        pay_month: Option<String>,
    ) -> Result<PayrollReadHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let pay_month = pay_month.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                PeoplePayrollSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let pay_month = pay_month.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("payroll.read")?;
                        let actor = payroll_actor(current)?;
                        let now = clock.now();
                        let mut repository = PgPayrollRead::new(connection);
                        let rows =
                            payroll_summary(&actor, &mut repository, pay_month.as_deref(), now)
                                .await?;
                        let data = serde_json::to_value(rows).map_err(|_| unavailable())?;
                        Ok(PayrollReadHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn runs<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
    ) -> Result<PayrollReadHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                PeoplePayrollSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        let actor = payroll_actor(current)?;
                        let now = clock.now();
                        let mut repository = PgPayrollRead::new(connection);
                        let rows = payroll_runs(&actor, &mut repository, now).await?;
                        let data = serde_json::to_value(rows).map_err(|_| unavailable())?;
                        Ok(PayrollReadHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

const PAYROLL_PURCHASE_CAPABILITY: &str = "people.payroll";

/// `ERPX_PEOPLE_PACKAGE_ENFORCEMENT` mirrors the retained development rollout
/// switch. Invalid values fail closed; non-local environments may not enable
/// enforcement. Disabled mode keeps the native command path unguarded exactly
/// like the retired TypeScript wrapper.
fn payroll_purchase_mode() -> Result<PackageEnforcementMode, BusinessError> {
    let raw = std::env::var("ERPX_PEOPLE_PACKAGE_ENFORCEMENT")
        .unwrap_or_else(|_| "disabled".to_owned());
    let mode = match raw.as_str() {
        "disabled" => PackageEnforcementMode::Disabled,
        "shadow" => PackageEnforcementMode::Shadow,
        "enforced" => PackageEnforcementMode::Enforced,
        _ => {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "PACKAGE_ENFORCEMENT_CONFIGURATION",
                "Package enforcement is unavailable.",
            ));
        }
    };
    if mode != PackageEnforcementMode::Disabled
        && std::env::var("ERPX_ENVIRONMENT").as_deref() != Ok("local")
    {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "PACKAGE_ENFORCEMENT_ENVIRONMENT",
            "Package enforcement is unavailable in this environment.",
        ));
    }
    Ok(mode)
}

fn payroll_purchase_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PACKAGE_CAPABILITY_CHANGED",
        "Payroll package access changed. Reload and review before retrying.",
    )
}

/// Begin the retained purchase-command boundary before any payroll run effect.
/// A replay is admitted under `history`, matching the retired TypeScript
/// `withPayrollPurchasedCommand` purpose switch.
#[allow(clippy::too_many_arguments)]
async fn begin_payroll_purchase(
    connection: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    actor_id: &str,
    key: &erpx_server_shared::public::CommandKey,
    operation: &str,
    record_id: &str,
    purpose: CapabilityPurpose,
) -> Result<Option<PurchaseCommandBoundary>, Error> {
    let mode = payroll_purchase_mode()?;
    if mode == PackageEnforcementMode::Disabled {
        return Ok(None);
    }
    let replayed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM command_receipts WHERE command_id=$1 AND operation=$2 \
         AND record_id=$3 AND actor_id=$4 AND authority_json::jsonb->>'companyId'=$5)",
    )
    .bind(key.as_str())
    .bind(operation)
    .bind(record_id)
    .bind(actor_id)
    .bind(company.as_str())
    .fetch_one(&mut *connection)
    .await?;
    let purpose = if replayed {
        CapabilityPurpose::History
    } else {
        purpose
    };
    let catalogue = current_package_catalogue()?;
    let mut boundary = PurchaseCommandBoundary::new(
        mode,
        key.as_str(),
        PurchaseCommandScope {
            company_id: company.clone(),
            capability: PAYROLL_PURCHASE_CAPABILITY.to_owned(),
            purpose,
        },
    )?;
    if let Some(active) = boundary.as_mut() {
        active.begin(connection, holding, &catalogue).await?;
    }
    Ok(boundary)
}

/// Close the boundary after the owner effects; shadow telemetry never fails the
/// command.
async fn complete_payroll_purchase(
    boundary: Option<PurchaseCommandBoundary>,
    connection: &mut PgConnection,
    holding: &HoldingId,
) -> Result<(), Error> {
    let Some(mut boundary) = boundary else {
        return Ok(());
    };
    let catalogue = current_package_catalogue()?;
    let receipt = boundary.complete(connection, holding, &catalogue).await?;
    if let Some(observation) = receipt.observation {
        write_purchase_shadow_checked_observation(&mut std::io::stderr(), &observation);
    }
    Ok(())
}

/// Bounded purchase inspection for the native run detail read. Returns the
/// witness so the caller can revalidate after the owner read. Disabled mode
/// keeps the pre-existing read contract.
async fn payroll_read_witness(
    connection: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    now: erpx_server_shared::public::TimestampMillis,
) -> Result<Option<PurchaseWitness>, Error> {
    let mode = payroll_purchase_mode()?;
    if mode == PackageEnforcementMode::Disabled {
        return Ok(None);
    }
    let catalogue = current_package_catalogue()?;
    let mut reader = PgCompanyPurchaseReader::new(
        holding.clone(),
        company.clone(),
        connection,
        &catalogue,
    );
    let inspection = reader
        .inspect(
            &PurchaseRequest {
                capabilities: vec![PAYROLL_PURCHASE_CAPABILITY.to_owned()],
                purpose: CapabilityPurpose::History,
                mode,
            },
            now,
        )
        .await
        .map_err(business)?;
    let allowed = inspection.access.iter().any(|access| {
        access.capability == PAYROLL_PURCHASE_CAPABILITY && access.allowed_by_mode
    });
    if !allowed {
        return Err(payroll_purchase_forbidden().into());
    }
    Ok(Some(inspection.witness))
}

async fn revalidate_payroll_witness(
    connection: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    witness: Option<PurchaseWitness>,
    now: erpx_server_shared::public::TimestampMillis,
) -> Result<(), Error> {
    let Some(witness) = witness else {
        return Ok(());
    };
    let catalogue = current_package_catalogue()?;
    PgCompanyPurchaseReader::new(holding.clone(), company.clone(), connection, &catalogue)
        .revalidate(&witness, now)
        .await
        .map_err(business)?;
    Ok(())
}

fn payroll_command_actor(
    current: &AuthorizedWorkContext,
) -> Result<PayrollCommandActor, BusinessError> {
    use sha2::{Digest, Sha256};
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    let mut hasher = Sha256::new();
    hasher.update(context(current).to_string().as_bytes());
    let work_context = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(PayrollCommandActor {
        user_id: current.user_id().as_str().to_owned(),
        holding_id: current.holding_id().clone(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        assignment_id: assignment.id.as_str().to_owned(),
        role: assignment.role.clone(),
        permissions: current.command_iam_metadata().permissions.clone(),
        work_context,
        expires_at: current.expires_at(),
    })
}

fn payroll_journal_forbidden() -> JournalError {
    JournalError {
        kind: FailureKind::Forbidden,
        code: "PAYROLL_FINANCE_AUTHORITY_REQUIRED",
        message: "Choose the current authorized Finance workspace for payroll posting.".to_owned(),
    }
}

struct PayrollJournalAuthority<'a> {
    current: &'a AuthorizedWorkContext,
}

impl JournalAuthority for PayrollJournalAuthority<'_> {
    type Error = PgTransactionError<JournalError>;

    async fn authorize(
        &mut self,
        actor: &FinanceActor,
        draft: &JournalDraft,
    ) -> Result<(), Self::Error> {
        let expected = crate::finance_journal_actor::finance_actor(self.current);
        if actor != &expected
            || !self.current.has_permission("finance.write")
            || draft.source_type != "payroll"
            || draft.source_id.is_none()
        {
            return Err(payroll_journal_forbidden().into());
        }
        Ok(())
    }
}

struct PayrollJournalBridge<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
}

impl<C: Clock + Sync + Send> PayrollJournalPort for PayrollJournalBridge<'_, C> {
    fn prepare<'b>(
        &'b mut self,
        connection: &'b mut PgConnection,
        request: &'b PayrollJournalRequest,
    ) -> PayrollCommandFuture<'b, PayrollPreparedJournal, BusinessError> {
        Box::pin(async move {
            let actor = FinanceActor {
                user_id: UserId::new(request.actor.user_id.as_str().to_owned())?,
                holding_id: request.actor.holding_id.clone(),
                company_id: request.actor.company_id.clone(),
                assignment_id: AssignmentId::new(request.actor.assignment_id.as_str().to_owned())?,
            };
            let draft = payroll_journal_draft(&request.draft)?;
            let key = erpx_server_shared::public::CommandKey::new(request.key.clone())?;
            let rules = embedded_currency_rules().map_err(|_| {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "PAYROLL_FINANCE_CURRENCY_RULES_UNAVAILABLE",
                    "Finance currency rules are unavailable.",
                )
            })?;
            let collation = FinanceCollation::english().map_err(|_| {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "PAYROLL_FINANCE_COLLATION_UNAVAILABLE",
                    "Finance collation is unavailable.",
                )
            })?;
            let holding = actor.holding_id.clone();
            let company = actor.company_id.clone();
            let mut authority = PayrollJournalAuthority {
                current: self.current,
            };
            let plan = {
                let mut ports = PgJournalPreparation::new(
                    connection,
                    &actor,
                    &mut authority,
                    self.clock,
                    &rules,
                    &collation,
                )
                .map_err(payroll_journal_error)?;
                prepare_journal(&mut ports, &actor, &draft, &key, JournalStatus::Posted)
                    .await
                    .map_err(payroll_journal_error)?
            };
            PgJournalStore::new(holding, company, connection)
                .persist(&plan)
                .await
                .map_err(payroll_journal_error)?;
            let result = plan.result();
            Ok(PayrollPreparedJournal {
                id: result.id.clone(),
                number: result.number.clone(),
                status: match result.status {
                    JournalStatus::Posted => "posted".to_owned(),
                    JournalStatus::Draft => "draft".to_owned(),
                },
                version: i64::from(result.version),
                debit: result.debit.clone(),
                credit: result.credit.clone(),
                fx_rate_micros: result.fx_rate_micros,
                base_currency: result.base_currency.clone(),
            })
        })
    }
}

fn payroll_journal_error(error: PgTransactionError<JournalError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => BusinessError::new(
            error.kind,
            error.code,
            "The Finance journal request was rejected.",
        ),
        PgTransactionError::Persistence(_) => BusinessError::new(
            FailureKind::Unavailable,
            "PAYROLL_FINANCE_UNAVAILABLE",
            "The Finance journal service is unavailable.",
        ),
    }
}

fn payroll_journal_draft(draft: &PayrollJournalDraft) -> Result<JournalDraft, BusinessError> {
    let lines = draft
        .lines
        .iter()
        .map(|line| erpx_server_finance::public::JournalLineInput {
            account_id: line.account_id.clone(),
            description: Some(line.description.clone()),
            debit: line.debit.clone(),
            credit: line.credit.clone(),
            dimensions: if line.dimensions.is_empty() {
                None
            } else {
                Some(line.dimensions.clone())
            },
        })
        .collect();
    Ok(JournalDraft {
        chart_id: draft.chart_id.clone(),
        period_id: draft.period_id.clone(),
        entry_date: draft.entry_date.clone(),
        currency: draft.currency.clone(),
        fx_rate: draft.fx_rate.clone(),
        is_opening: draft.is_opening,
        memo: draft.memo.clone(),
        source_type: draft.source_type.clone(),
        source_id: draft.source_id.clone(),
        lines,
    })
}

impl<C: Clock + Send + Sync + 'static> PeoplePayrollService<C> {
    pub async fn command<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        kind: PayrollCommandHttpKind,
        body: Value,
        key: Option<erpx_server_shared::public::CommandKey>,
    ) -> Result<PayrollCommandHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        let mutation = kind.is_mutation();
        let input = move || {
            let mut request = input()?;
            if request.company_id != expected {
                return Err(scope());
            }
            request.mutation = mutation;
            Ok(request)
        };
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                Self::access(),
                PeoplePayrollSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let kind = kind.clone();
                    let body = body.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        let actor = payroll_command_actor(current)?;
                        let now = clock.now();
                        let data = match kind {
                            PayrollCommandHttpKind::Create => {
                                let key = command_key(key)?;
                                let boundary = begin_payroll_purchase(
                                    connection,
                                    &holding,
                                    &company,
                                    actor.user_id.as_str(),
                                    &key,
                                    erpx_server_people_payroll::public::payroll_run::CREATE_OPERATION,
                                    &format!("payrun-{}", key.as_str()),
                                    CapabilityPurpose::NewWork,
                                )
                                .await?;
                                let mut repository = PgPayrollCommand::new(&mut *connection, &actor);
                                let data =
                                    create_payroll_run(&actor, &body, &key, &mut repository, now)
                                        .await?;
                                drop(repository);
                                complete_payroll_purchase(boundary, connection, &holding).await?;
                                data
                            }
                            PayrollCommandHttpKind::Calculate { run_id } => {
                                let key = command_key(key)?;
                                let boundary = begin_payroll_purchase(
                                    connection,
                                    &holding,
                                    &company,
                                    actor.user_id.as_str(),
                                    &key,
                                    erpx_server_people_payroll::public::payroll_run::CALCULATE_OPERATION,
                                    &run_id,
                                    CapabilityPurpose::SettleExisting,
                                )
                                .await?;
                                let mut repository = PgPayrollCommand::new(&mut *connection, &actor);
                                let data = calculate_payroll_run(
                                    &actor,
                                    &run_id,
                                    &body,
                                    &key,
                                    &mut repository,
                                    now,
                                )
                                .await?;
                                drop(repository);
                                complete_payroll_purchase(boundary, connection, &holding).await?;
                                data
                            }
                            PayrollCommandHttpKind::Cancel { run_id } => {
                                let key = command_key(key)?;
                                let boundary = begin_payroll_purchase(
                                    connection,
                                    &holding,
                                    &company,
                                    actor.user_id.as_str(),
                                    &key,
                                    erpx_server_people_payroll::public::payroll_run::CANCEL_OPERATION,
                                    &run_id,
                                    CapabilityPurpose::SettleExisting,
                                )
                                .await?;
                                let mut repository = PgPayrollCommand::new(&mut *connection, &actor);
                                let data = cancel_payroll_run(
                                    &actor,
                                    &run_id,
                                    &body,
                                    &key,
                                    &mut repository,
                                    now,
                                )
                                .await?;
                                drop(repository);
                                complete_payroll_purchase(boundary, connection, &holding).await?;
                                data
                            }
                            PayrollCommandHttpKind::Post { run_id } => {
                                let key = command_key(key)?;
                                let boundary = begin_payroll_purchase(
                                    connection,
                                    &holding,
                                    &company,
                                    actor.user_id.as_str(),
                                    &key,
                                    erpx_server_people_payroll::public::payroll_run::POST_OPERATION,
                                    &run_id,
                                    CapabilityPurpose::SettleExisting,
                                )
                                .await?;
                                let mut repository = PgPayrollCommand::new(&mut *connection, &actor);
                                let mut journal = PayrollJournalBridge {
                                    current,
                                    clock: clock.as_ref(),
                                };
                                let data = post_payroll(
                                    &actor,
                                    &run_id,
                                    &body,
                                    &key,
                                    &mut repository,
                                    &mut journal,
                                    now,
                                )
                                .await?;
                                drop(repository);
                                complete_payroll_purchase(boundary, connection, &holding).await?;
                                data
                            }
                            PayrollCommandHttpKind::Detail { run_id } => {
                                let witness = payroll_read_witness(
                                    connection,
                                    &holding,
                                    &company,
                                    now,
                                )
                                .await?;
                                let mut repository = PgPayrollCommand::new(&mut *connection, &actor);
                                let data =
                                    payroll_run_detail(&actor, &run_id, &mut repository, now)
                                        .await?;
                                drop(repository);
                                revalidate_payroll_witness(
                                    connection,
                                    &holding,
                                    &company,
                                    witness,
                                    now,
                                )
                                .await?;
                                data
                            }
                        };
                        Ok(PayrollCommandHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

fn command_key(
    key: Option<erpx_server_shared::public::CommandKey>,
) -> Result<erpx_server_shared::public::CommandKey, BusinessError> {
    key.ok_or_else(|| {
        BusinessError::invalid("COMMAND_KEY_REQUIRED", "Idempotency-Key is required.")
    })
}

pub struct PeoplePayrollBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<PeoplePayrollService<C>>,
}

impl<C: Clock + Send + Sync + 'static> PeoplePayrollBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<PeoplePayrollService<C>>,
    ) -> Self {
        Self {
            policy,
            scopes,
            service,
        }
    }

    fn context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: false,
            },
        ))
    }
}


impl<C: Clock + Send + Sync + 'static> PayrollCommandHttpBackend for PeoplePayrollBackend<C> {
    fn command<'a>(
        &'a self,
        request: PayrollCommandHttpRequest<'a>,
    ) -> PayrollCommandHttpFuture<'a, PayrollCommandHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .command(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.kind.clone(),
                    request.body.clone(),
                    request.key.cloned(),
                )
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> PayrollReadHttpBackend for PeoplePayrollBackend<C> {
    fn read<'a>(
        &'a self,
        request: PayrollReadHttpRequest<'a>,
    ) -> PayrollReadHttpFuture<'a, PayrollReadHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            match request.kind {
                PayrollReadHttpKind::Summary { pay_month } => self
                    .service
                    .summary(request.host, proof, move || Ok(selection), pay_month)
                    .await
                    .map_err(business),
                PayrollReadHttpKind::Runs => self
                    .service
                    .runs(request.host, proof, move || Ok(selection))
                    .await
                    .map_err(business),
            }
        })
    }
}

#[allow(dead_code)]
fn _router(targets: Vec<PayrollReadHttpTarget>) -> Result<axum::Router, BusinessError> {
    people_payroll_router(targets)
}
