//! Native Sales read and price-list command assembly. Current Account/Holding
//! authority remains with the selected coordinator; owner ports supply the
//! document list and price-list lifecycle facts.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_sales::public::sales_documents::{
    PgSalesDocumentRead, SalesDocumentKind, SalesDocumentReadActor, sales_documents,
};
use erpx_server_sales::public::sales_documents_http::{
    SalesDocumentsHttpBackend, SalesDocumentsHttpFuture, SalesDocumentsHttpRequest,
    SalesDocumentsHttpResponse,
};
use erpx_server_sales::public::sales_price_lists::{
    PgPriceLists, PriceListActor, create_price_list, decide_price_list, update_price_list,
};
use erpx_server_sales::public::sales_price_lists_http::{
    PriceListHttpBackend, PriceListHttpCommand, PriceListHttpFuture, PriceListHttpRequest,
    PriceListHttpResponse,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId,
};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "SALES_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "SALES_UNAVAILABLE",
        "The Sales workspace is unavailable.",
    )
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid("SALES_REQUEST_INVALID", "The Sales request is invalid.")
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct SalesSources;

impl HoldingWorkSourcePreparation for SalesSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct SalesConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl SalesConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn sales_document_actor(current: &AuthorizedWorkContext) -> Result<SalesDocumentReadActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    Ok(SalesDocumentReadActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        permissions: current.command_iam_metadata().permissions.clone(),
        modules: current.modules().to_vec(),
        expires_at: current.expires_at(),
    })
}

fn price_list_actor(current: &AuthorizedWorkContext) -> Result<PriceListActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(PriceListActor {
        user_id: current.user_id().clone(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        assignment_id: assignment.id.clone(),
        role: Some(assignment.role.clone()),
        authority_revision: Some(selected.revision()),
        assignment_version: Some(assignment.version),
        permissions: current.command_iam_metadata().permissions.clone(),
        modules: current.modules().to_vec(),
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

pub struct SalesService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: SalesConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> SalesService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: SalesConfiguration,
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

    fn access_documents() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "commerce.read".into(),
            module: Some("sales".into()),
            destination: "sales".into(),
            api: true,
        }
    }

    fn access_price_lists() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "commerce.read".into(),
            module: Some("sales".into()),
            destination: "sales".into(),
            api: true,
        }
    }

    fn input_documents<Input>(
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

    fn input_price_lists<Input>(
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
            request.mutation = true;
            Ok(request)
        }
    }

    pub async fn documents<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        kind: String,
        search: String,
        status: Option<String>,
        cursor: Option<String>,
    ) -> Result<SalesDocumentsHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let kind_for_closure = kind.clone();
        let search_for_closure = search.clone();
        let status_for_closure = status.clone();
        let cursor_for_closure = cursor.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input_documents(input),
                Self::access_documents(),
                SalesSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let kind = kind_for_closure.clone();
                    let search = search_for_closure.clone();
                    let status = status_for_closure.clone();
                    let cursor = cursor_for_closure.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let actor = sales_document_actor(current)?;
                        let kind = SalesDocumentKind::parse(&kind)?;
                        let now = clock.now();
                        let mut repository = PgSalesDocumentRead::new(connection);
                        let page = sales_documents(
                            &actor,
                            &mut repository,
                            kind,
                            &search,
                            status.as_deref(),
                            cursor.as_deref(),
                            now,
                        )
                        .await?;
                        let data = serde_json::to_value(page).map_err(|_| unavailable())?;
                        Ok(SalesDocumentsHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn price_list<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: PriceListHttpCommand,
    ) -> Result<PriceListHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let command_for_closure = command.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input_price_lists(input),
                Self::access_price_lists(),
                SalesSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let command = command_for_closure.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let actor = price_list_actor(current)?;
                        let now = clock.now();
                        let mut repository = PgPriceLists::new(connection);
                        let receipt = match command {
                            PriceListHttpCommand::Create {
                                idempotency_key,
                                input,
                            } => {
                                let key = CommandKey::new(&idempotency_key)
                                    .map_err(|_| request_invalid())?;
                                create_price_list(&actor, input, &key, &mut repository, now).await?
                            }
                            PriceListHttpCommand::Update {
                                price_list_id,
                                idempotency_key,
                                input,
                            } => {
                                let key = CommandKey::new(&idempotency_key)
                                    .map_err(|_| request_invalid())?;
                                update_price_list(
                                    &actor,
                                    &price_list_id,
                                    input,
                                    &key,
                                    &mut repository,
                                    now,
                                )
                                .await?
                            }
                            PriceListHttpCommand::Decide {
                                price_list_id,
                                idempotency_key,
                                input,
                            } => {
                                let key = CommandKey::new(&idempotency_key)
                                    .map_err(|_| request_invalid())?;
                                decide_price_list(
                                    &actor,
                                    &price_list_id,
                                    input,
                                    &key,
                                    &mut repository,
                                    now,
                                )
                                .await?
                            }
                        };
                        Ok(PriceListHttpResponse {
                            data: receipt.to_wire(),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct SalesBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<SalesService<C>>,
}

impl<C: Clock + Send + Sync + 'static> SalesBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<SalesService<C>>,
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

impl<C: Clock + Send + Sync + 'static> SalesDocumentsHttpBackend for SalesBackend<C> {
    fn read<'a>(
        &'a self,
        request: SalesDocumentsHttpRequest<'a>,
    ) -> SalesDocumentsHttpFuture<'a, SalesDocumentsHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let kind = request.kind.to_owned();
            let search = request.search.to_owned();
            let status = request.status.map(str::to_owned);
            let cursor = request.cursor.map(str::to_owned);
            self.service
                .documents(
                    request.host,
                    proof,
                    move || Ok(selection),
                    kind,
                    search,
                    status,
                    cursor,
                )
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> PriceListHttpBackend for SalesBackend<C> {
    fn execute<'a>(
        &'a self,
        request: PriceListHttpRequest<'a>,
    ) -> PriceListHttpFuture<'a, PriceListHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let command = request.command.clone();
            self.service
                .price_list(request.host, proof, move || Ok(selection), command)
                .await
                .map_err(business)
        })
    }
}
