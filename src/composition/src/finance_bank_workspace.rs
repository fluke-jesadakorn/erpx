//! Native Finance bank reconciliation workspace GET. The normal Account
//! session, Holding audience/origin, selected work revision, current IAM and
//! the Commercial `finance.banking` purchase are all rechecked inside the
//! retained Holding unit before any workspace row is returned.
use crate::http_scope::BusinessScopeSlugs;
use axum::{
    Json, Router,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, lock_commercial_authority_sources,
};
use erpx_server_finance::public::bank_workspace::{
    BankWorkspace, BankWorkspaceQuery, lock_bank_workspace_sources, read_bank_workspace,
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_holding_bindings::ConfiguredHoldingRegistry,
    account_inbound_http::{parse_inbound_account_request, unique_request_header},
    effective_authority::{
        HoldingWorkAuthorizer, HoldingWorkSourcePreparation, WorkAuthorizationRequest,
    },
    work_context::{ExpectedWorkSelection, HoldingWorkspaceRequest},
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CompanyId, FailureKind, HoldingId,
};
use serde_json::json;
use sqlx::PgConnection;
use std::{collections::BTreeMap, sync::Arc};

type Error = PgTransactionError<BusinessError>;

#[derive(Clone)]
pub struct FinanceBankWorkspaceConfiguration {
    holding: HoldingId,
    company: CompanyId,
    catalogue: Arc<CommercialCatalogue>,
}

impl FinanceBankWorkspaceConfiguration {
    pub fn new(
        holding: HoldingId,
        company: CompanyId,
        catalogue: Arc<CommercialCatalogue>,
    ) -> Result<Self, BusinessError> {
        Ok(Self {
            holding,
            company,
            catalogue,
        })
    }

    pub fn company_id(&self) -> &CompanyId {
        &self.company
    }
}

impl HoldingWorkSourcePreparation for FinanceBankWorkspaceConfiguration {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            // Match the bank command's writer-gate order exactly.
            lock_bank_workspace_sources(c).await?;
            lock_commercial_authority_sources(c).await
        })
    }
}

#[derive(Clone)]
pub struct FinanceBankWorkspaceService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: FinanceBankWorkspaceConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> FinanceBankWorkspaceService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: FinanceBankWorkspaceConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }

    pub fn company_id(&self) -> &CompanyId {
        &self.configuration.company
    }

    pub async fn read<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        query: BankWorkspaceQuery,
    ) -> Result<BankWorkspace, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        self.read_deferred(host, proof, input, Ok(query)).await
    }

    async fn read_deferred<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        query: Result<BankWorkspaceQuery, BusinessError>,
    ) -> Result<BankWorkspace, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = false;
            Ok(request)
        };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: "finance.read".into(),
                    module: Some("finance".into()),
                    destination: "finance/bank".into(),
                    api: true,
                },
                configuration.clone(),
                move |c, current| {
                    let configuration = configuration.clone();
                    let clock = clock.clone();
                    let query = query.clone();
                    Box::pin(async move {
                        let query = query?;
                        if current.holding_id() != &configuration.holding
                            || current.selection().workspace().company_id != configuration.company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("finance.read")?;
                        current.require_permission("module.finance.read")?;
                        let now = clock.now();
                        let inspection = PgCompanyPurchaseReader::new(
                            configuration.holding.clone(),
                            configuration.company.clone(),
                            c,
                            &configuration.catalogue,
                        )
                        .inspect(
                            &PurchaseRequest {
                                capabilities: vec!["finance.banking".into()],
                                purpose: CapabilityPurpose::SettleExisting,
                                mode: PackageEnforcementMode::Enforced,
                            },
                            now,
                        )
                        .await?;
                        inspection.require_allowed()?;
                        let witness = inspection.witness;
                        lock_bank_workspace_sources(c).await?;
                        let data = read_bank_workspace(
                            c,
                            configuration.company.as_str(),
                            &query,
                        )
                        .await?;
                        PgCompanyPurchaseReader::new(
                            configuration.holding.clone(),
                            configuration.company.clone(),
                            c,
                            &configuration.catalogue,
                        )
                        .revalidate(&witness, clock.now())
                        .await?
                        .require_allowed()?;
                        erpx_server_identity_access::public::selected_command_authority::require_selected_actor_current(
                            c,
                            current,
                            clock.now().get(),
                        )
                        .await?;
                        if clock.now().get() >= current.expires_at() {
                            return Err(scope().into());
                        }
                        Ok(data)
                    })
                },
            )
            .await
    }
}

#[derive(Clone)]
pub struct BankWorkspaceScopeHeaders;

impl BusinessScopeSlugs for BankWorkspaceScopeHeaders {
    fn slugs(
        &self,
        headers: &HeaderMap,
    ) -> Result<(Option<String>, Option<String>), BusinessError> {
        let slug = |key| -> Result<Option<String>, BusinessError> {
            unique_request_header(headers, key)?
                .map(|value| {
                    if value.is_empty()
                        || value.len() > 128
                        || !value.bytes().all(|byte| {
                            byte.is_ascii_lowercase()
                                || byte.is_ascii_digit()
                                || byte == b'-'
                                || byte == b'_'
                        })
                    {
                        return Err(BusinessError::invalid(
                            "INVALID_WORKSPACE_SELECTOR",
                            "Use an exact workspace slug.",
                        ));
                    }
                    Ok(value.to_owned())
                })
                .transpose()
        };
        Ok((
            slug("x-erpx-workspace-branch")?,
            slug("x-erpx-workspace-department")?,
        ))
    }
}

pub struct FinanceBankWorkspaceTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub service: Arc<FinanceBankWorkspaceService<C>>,
}

struct WorkspaceState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String, String), FinanceBankWorkspaceTarget<C>>,
}

pub fn finance_bank_workspace_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<FinanceBankWorkspaceTarget<C>>,
) -> Result<Router, BusinessError> {
    policy.validate()?;
    let mut configured = BTreeMap::new();
    for target in targets {
        let valid = policy
            .audience(&target.host)
            .and_then(|audience| registry.expected_for_audience(&audience).ok())
            .is_some_and(|holding| {
                holding.slug == target.holding_slug
                    && holding.holding_id == target.service.configuration.holding
            });
        if !valid
            || configured
                .insert(
                    (
                        target.host.clone(),
                        target.holding_slug.clone(),
                        target.service.company_id().as_str().to_owned(),
                    ),
                    target,
                )
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
            "/api/v1/holdings/{holding_slug}/finance/bank",
            get(read::<C>),
        )
        .with_state(Arc::new(WorkspaceState {
            policy,
            scopes,
            targets: configured,
        })))
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "COMPANY_FINANCE_REQUIRED",
        "Use a current Company Finance assignment for bank reconciliation.",
    )
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
            Json(json!({"error":{"code":error.code,"message":error.message}})),
        )
            .into_response(),
        PgTransactionError::Persistence(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":{"code":"FINANCE_BANK_UNAVAILABLE","message":"The bank workflow is unavailable."}})),
        )
            .into_response(),
    }
}

fn bounded(value: &serde_json::Value) -> Response {
    let mut reply = (StatusCode::OK, Json(value)).into_response();
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        reply
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    reply
}

fn expected(headers: &HeaderMap) -> Result<ExpectedWorkSelection, BusinessError> {
    let revision = unique_request_header(headers, "x-erpx-context-revision")?
        .ok_or_else(missing)?
        .parse()
        .map_err(|_| {
            BusinessError::invalid("VALIDATION_FAILED", "Use the current work revision.")
        })?;
    let assignment_version = unique_request_header(headers, "x-erpx-context-assignment-version")?
        .ok_or_else(missing)?
        .parse()
        .map_err(|_| {
            BusinessError::invalid("VALIDATION_FAILED", "Use the current assignment version.")
        })?;
    let assignment_id = AssignmentId::new(
        unique_request_header(headers, "x-erpx-context-assignment")?.ok_or_else(missing)?,
    )?;
    Ok(ExpectedWorkSelection {
        revision,
        assignment_id,
        assignment_version,
    })
}

fn query(raw: Option<&str>) -> Result<BankWorkspaceQuery, BusinessError> {
    let mut query = BankWorkspaceQuery::default();
    let mut seen = std::collections::BTreeSet::new();
    for (key, value) in url::form_urlencoded::parse(raw.unwrap_or("").as_bytes()) {
        if value.len() > 4096
            || value.chars().any(char::is_control)
            || !seen.insert(key.to_string())
        {
            return Err(BusinessError::invalid(
                "FINANCE_BANK_QUERY_INVALID",
                "Use the bank reconciliation filters.",
            ));
        }
        let value = (!value.is_empty()).then(|| value.into_owned());
        match key.as_ref() {
            "print" => query.print = value,
            "transactionCursor" => query.transaction_cursor = value,
            "statementCursor" => query.statement_cursor = value,
            "match" => query.matched_transaction_id = value,
            "new" => query.new = value,
            "chart" => query.chart = value,
            "view" => query.view = value,
            "q" => query.q = value,
            _ => {
                return Err(BusinessError::invalid(
                    "FINANCE_BANK_QUERY_INVALID",
                    "Use the bank reconciliation filters.",
                ));
            }
        }
    }
    Ok(query)
}

async fn read<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<WorkspaceState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?
            .ok_or_else(missing)?
            .to_owned();
        let (target, target_selection) =
            crate::http_scope::deferred_company_target(&state.targets, &host, &slug, &headers)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|value| value != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let query = query(raw.as_deref());
        let scope_result = state.scopes.slugs(&headers);
        let (branch, department) = scope_result.as_ref().cloned().unwrap_or_default();
        let company = target.service.company_id().clone();
        let input = move || {
            scope_result?;
            target_selection?;
            if let Some(selected) = unique_request_header(&headers, "x-erpx-context-company")?
                && selected != company.as_str()
            {
                return Err(scope());
            }
            let expected = expected(&headers)?;
            Ok(HoldingWorkspaceRequest {
                company_id: company,
                branch_slug: branch,
                department_slug: department,
                expected: Some(expected),
                mutation: false,
            })
        };
        target
            .service
            .read_deferred(&host, inbound.into_proof(), input, query)
            .await
    }
    .await;
    match result {
        Ok(data) => match serde_json::to_value(data) {
            Ok(data) => bounded(&json!({"data":data})),
            Err(_) => response(PgTransactionError::Business(BusinessError::new(
                FailureKind::Unavailable,
                "FINANCE_BANK_WORKSPACE_UNAVAILABLE",
                "The bank reconciliation workspace is unavailable.",
            ))),
        },
        Err(error) => response(error),
    }
}
