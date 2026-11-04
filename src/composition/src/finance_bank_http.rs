//! Native Finance bank import/reconciliation ingress. Current Account session,
//! Commercial purchase witness and the Finance owner unit are all enforced
//! before any row is written.
use crate::{
    finance_journal_create::{FinanceJournalCreateConfiguration, Ids},
    finance_journal_review::{command_actor, finance_actor},
};
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, PgCompanyPurchaseReader, PurchaseRequest,
    PurchaseWitness,
};
use erpx_server_finance::public::bank_reconciliation::*;
use erpx_server_identity_access::public::{
    AccountPolicy, RequestAudience,
    account_holding_bindings::ConfiguredHoldingRegistry,
    account_inbound_http::{parse_inbound_account_request, unique_request_header},
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{ExpectedWorkSelection, HoldingWorkspaceRequest},
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use erpx_server_workflow::public::commands::*;
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};
type Error = PgTransactionError<BusinessError>;
const BODY_LIMIT: usize = 2 * 1024 * 1024;
pub struct FinanceBankTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub configuration: FinanceJournalCreateConfiguration,
    pub clock: Arc<C>,
}
struct StateMap<C> {
    policy: Arc<AccountPolicy>,
    targets: BTreeMap<(String, String), FinanceBankTarget<C>>,
}
pub fn finance_bank_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    targets: Vec<FinanceBankTarget<C>>,
) -> Result<Router, BusinessError> {
    policy.validate()?;
    let mut configured = BTreeMap::new();
    for target in targets {
        let valid = policy
            .audience(&target.host)
            .and_then(|audience| registry.expected_for_audience(&audience).ok())
            .is_some_and(|holding| holding.slug == target.holding_slug);
        if !valid
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "FINANCE_BANK_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/finance/bank/statements/import",
            post(import_statement::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/bank/transactions/{id}/reconcile",
            post(reconcile_transaction::<C>),
        )
        .with_state(Arc::new(StateMap {
            policy,
            targets: configured,
        })))
}
fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn status(kind: FailureKind) -> StatusCode {
    match kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}
fn response(error: PgTransactionError<BusinessError>) -> Response {
    match error {
        PgTransactionError::Business(error) => (
            status(error.kind),
            Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
        )
            .into_response(),
        PgTransactionError::Persistence(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":{"code":"FINANCE_BANK_UNAVAILABLE","message":"The bank workflow is unavailable."}})),
        )
            .into_response(),
    }
}
fn bounded(value: &Value, code: StatusCode) -> Response {
    let mut reply = (code, Json(value)).into_response();
    for (name, value) in [
        (axum::http::header::CACHE_CONTROL, "private, no-store"),
        (axum::http::header::REFERRER_POLICY, "no-referrer"),
        (axum::http::header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        reply
            .headers_mut()
            .insert(name, axum::http::HeaderValue::from_static(value));
    }
    reply
}
#[derive(Clone)]
struct BankSources;
impl HoldingWorkSourcePreparation for BankSources {
    fn prepare<'c>(&'c self, c: &'c mut sqlx::PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_bank_reconciliation_sources(c).await?;
            erpx_server_commercial::public::purchases::lock_commercial_authority_sources(c).await?;
            Ok(())
        })
    }
}
struct BankAuthority<'a, C> {
    current: &'a AuthorizedWorkContext,
    config: &'a FinanceJournalCreateConfiguration,
    clock: &'a C,
    purchase: Option<(CapabilityPurpose, PurchaseWitness)>,
}
impl<C: Clock + Send + Sync> PgCommandAuthority for BankAuthority<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut sqlx::PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        BankSources.prepare(c).await
    }
    async fn authorize(
        &mut self,
        c: &mut sqlx::PgConnection,
        request: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        let scope = self.current.selection().workspace();
        if self.current.holding_id() != &self.config.holding
            || scope.company_id != self.config.company
            || scope.branch_id.is_some()
            || scope.department_id.is_some()
            || !matches!(request.operation.as_str(), "finance.bank.import" | "finance.bank.reconcile")
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "COMPANY_FINANCE_REQUIRED",
                "Use a current Company Finance assignment for bank reconciliation.",
            )
            .into());
        }
        self.current.require_permission("finance.write")?;
        self.current.require_permission("module.finance.read")?;
        if request.operation == "finance.bank.reconcile" {
            self.current.require_permission("finance.approve")?;
        }
        erpx_server_identity_access::public::selected_command_authority::require_selected_actor_current(
            c,
            self.current,
            self.clock.now().get(),
        )
        .await?;
        Ok(command_actor(self.current)?)
    }
    async fn verify_command(
        &mut self,
        c: &mut sqlx::PgConnection,
        _: &CommandActor,
        request: &WorkflowRequest,
        _: CommandDisposition,
    ) -> Result<(), Error> {
        self.authorize(c, request).await?;
        let mut reader = PgCompanyPurchaseReader::new(
            self.config.holding.clone(),
            self.config.company.clone(),
            c,
            &self.config.catalogue,
        );
        if let Some((_, witness)) = &self.purchase {
            reader
                .revalidate(witness, self.clock.now())
                .await?
                .require_allowed()?;
            return Ok(());
        }
        let found = reader
            .inspect(
                &PurchaseRequest {
                    capabilities: vec!["finance.banking".into()],
                    purpose: CapabilityPurpose::SettleExisting,
                    mode: PackageEnforcementMode::Enforced,
                },
                self.clock.now(),
            )
            .await?;
        found.require_allowed()?;
        self.purchase = Some((CapabilityPurpose::SettleExisting, found.witness));
        Ok(())
    }
}
enum Plan {
    Import(PreparedBankImport),
    Reconcile,
}
struct Effects<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    plan: Option<Plan>,
}
impl<C: Clock + Send + Sync> PgOwnerCommandEffects for Effects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut sqlx::PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        BankSources.prepare(c).await
    }
    async fn prepare(
        &mut self,
        c: &mut sqlx::PgConnection,
        _: &CommandActor,
        request: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let value: Value = serde_json::from_str(request.input.as_str())
            .map_err(|_| BusinessError::invalid("VALIDATION_FAILED", "Invalid bank request."))?;
        let (result, plan) = if request.operation == "finance.bank.import" {
            let input: BankImportInput = serde_json::from_value(value)
                .map_err(|_| BusinessError::invalid("VALIDATION_FAILED", "Invalid bank statement."))?;
            let plan = prepare_bank_import(c, &finance_actor(self.current), &request.key, &input, self.clock).await?;
            (
                serde_json::json!({
                    "id": plan.statement_id(),
                    "status": "imported",
                    "imported": input.rows.len(),
                    "settlementPaymentId": null
                }),
                Plan::Import(plan),
            )
        } else {
            let input: BankReconcileInput = serde_json::from_value(value.clone())
                .map_err(|_| BusinessError::invalid("VALIDATION_FAILED", "Invalid bank match."))?;
            if input.matched_type != "payment" || input.proof.trim().is_empty() {
                return Err(BusinessError::invalid(
                    "MATCH_INPUT_INVALID",
                    "Select a posted payment and provide review evidence.",
                )
                .into());
            }
            (
                serde_json::json!({
                    "id": request.record_id,
                    "status": "matched",
                    "version": input.expected_version + 1,
                    "matchedType": "payment",
                    "matchedId": input.matched_id
                }),
                Plan::Reconcile,
            )
        };
        self.plan = Some(plan);
        Ok(WorkflowOutput {
            version: 1,
            result: CommandJson::from_serializable(&result)?,
            event_scope: None,
        })
    }
    async fn apply(
        &mut self,
        c: &mut sqlx::PgConnection,
        _: &CommandActor,
        request: &WorkflowRequest,
    ) -> Result<(), Error> {
        match self.plan.as_ref() {
            Some(Plan::Import(_)) => {}
            Some(Plan::Reconcile) => {
                let input: BankReconcileInput = serde_json::from_str(request.input.as_str())
                    .map_err(|_| BusinessError::invalid("VALIDATION_FAILED", "Invalid bank match."))?;
                apply_bank_reconciliation(
                    c,
                    &finance_actor(self.current),
                    &request.key,
                    &request.record_id,
                    &input,
                    self.clock,
                )
                .await?;
            }
            None => return Err(BusinessError::new(FailureKind::Unavailable, "BANK_UNAVAILABLE", "The bank request could not be prepared.").into()),
        }
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut sqlx::PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        match self.plan.take() {
            Some(Plan::Import(plan)) => {
                plan.persist(c).await?;
                Ok(())
            }
            Some(Plan::Reconcile) => Ok(()),
            None => Err(BusinessError::new(FailureKind::Unavailable, "BANK_UNAVAILABLE", "The bank request was not prepared.").into()),
        }
    }
}
async fn command<C: Clock + Send + Sync + 'static>(
    state: Arc<StateMap<C>>,
    slug: String,
    id: Option<String>,
    request: Request,
    import: bool,
) -> Response {
    let success = if import { StatusCode::CREATED } else { StatusCode::OK };
    let result = async {
        let host = unique_request_header(request.headers(), "host")?.ok_or_else(missing)?.to_owned();
        let target = state.targets.get(&(host.clone(), slug)).ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, request.headers())?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(request.headers(), "origin")?.is_some_and(|value| value != inbound.expected_origin()) {
            return Err(BusinessError::new(FailureKind::Forbidden, "ORIGIN_FORBIDDEN", "Use the application origin.").into());
        }
        let revision: i64 = unique_request_header(request.headers(), "x-erpx-context-revision")?
            .ok_or_else(missing)?
            .parse()
            .map_err(|_| BusinessError::invalid("VALIDATION_FAILED", "Use the current work revision."))?;
        let assignment_version: i64 = unique_request_header(request.headers(), "x-erpx-context-assignment-version")?
            .ok_or_else(missing)?
            .parse()
            .map_err(|_| BusinessError::invalid("VALIDATION_FAILED", "Use the current assignment version."))?;
        let assignment_id = erpx_server_shared::public::AssignmentId::new(
            unique_request_header(request.headers(), "x-erpx-context-assignment")?.ok_or_else(missing)?,
        )?;
        let key = unique_request_header(request.headers(), "idempotency-key")?
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| BusinessError::invalid("IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required."))?;
        let body = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller bank request."))?;
        let input: Value = serde_json::from_slice(&body)
            .map_err(|_| BusinessError::invalid("VALIDATION_FAILED", "Invalid bank request."))?;
        let record_id = if import { format!("statement-{key}") } else { id.ok_or_else(missing)? };
        let operation = if import { "finance.bank.import" } else { "finance.bank.reconcile" };
        let config = target.configuration.clone();
        let clock = target.clock.clone();
        let authorizer = target.authorizer.clone();
        let host = host.clone();
        let selection_company = config.company.clone();
        authorizer
            .execute_with_sources_deferred(
                &host,
                inbound.into_proof(),
                move || {
                    Ok(HoldingWorkspaceRequest {
                        company_id: selection_company.clone(),
                        branch_slug: None,
                        department_slug: None,
                        expected: Some(ExpectedWorkSelection {
                            revision,
                            assignment_id: assignment_id.clone(),
                            assignment_version,
                        }),
                        mutation: true,
                    })
                },
                WorkAuthorizationRequest {
                    permission: if import { "finance.write" } else { "finance.approve" }.into(),
                    module: Some("finance".into()),
                    destination: "finance/bank".into(),
                    api: true,
                },
                BankSources,
                move |c, current| {
                    let clock = clock.clone();
                    let config = config.clone();
                    let input = input.clone();
                    let record_id = record_id.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        let request = WorkflowRequest {
                            key: key.to_owned(),
                            operation: operation.into(),
                            record_id,
                            input: CommandJson::from_serializable(&input)?,
                        };
                        let mut authority = BankAuthority {
                            current,
                            config: &config,
                            clock: clock.as_ref(),
                            purchase: None,
                        };
                        let mut effects = Effects {
                            current,
                            clock: clock.as_ref(),
                            plan: None,
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
                        Ok::<_, Error>(outcome.result)
                    })
                },
            )
            .await
    }
    .await;
    match result {
        Ok(value) => {
            let value: Value = serde_json::from_str(value.as_str()).unwrap_or(Value::Null);
            bounded(&serde_json::json!({"data":value}), success)
        }
        Err(error) => response(error),
    }
}
async fn import_statement<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<StateMap<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    command(state, slug, None, request, true).await
}
async fn reconcile_transaction<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<StateMap<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    command(state, slug, Some(id), request, false).await
}
