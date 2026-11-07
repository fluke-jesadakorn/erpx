use crate::business_http::BusinessScopeSlugs;
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_legal::public::legal_records::{
    LegalCommandReply, LegalDetailReply, LegalHttpBackend, LegalHttpCommandRequest, LegalHttpFuture, LegalHttpRead,
    LegalHttpReadReply, LegalHttpReadRequest, LegalRegisterQuery, LegalRegisterReply,
    LegalRenderedContext, LegalRecordsActor, legal_detail, legal_register, PgLegalRecords,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "LEGAL_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LEGAL_UNAVAILABLE",
        "The Legal service is unavailable.",
    )
}

fn command_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LEGAL_COMMAND_UNAVAILABLE",
        "Native Legal commands are not enabled yet.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct LegalSources;

impl HoldingWorkSourcePreparation for LegalSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct LegalConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl LegalConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn legal_actor(current: &AuthorizedWorkContext) -> Result<LegalRecordsActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    Ok(LegalRecordsActor {
        holding_id: current.holding_id().clone(),
        company_id: workspace.company_id.clone(),
        user_id: current.user_id().clone(),
        assignment_id: selected.assignment().id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        role: selected.assignment().role.clone(),
        permissions: current.command_iam_metadata().permissions.iter().cloned().collect(),
    })
}

fn rendered_context(current: &AuthorizedWorkContext) -> LegalRenderedContext {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    LegalRenderedContext {
        user_id: current.user_id().as_str().to_owned(),
        holding_id: current.holding_id().as_str().to_owned(),
        company_id: workspace.company_id.as_str().to_owned(),
        branch_id: workspace.branch_id.as_ref().map(|value| value.as_str().to_owned()),
        department_id: workspace.department_id.as_ref().map(|value| value.as_str().to_owned()),
        assignment_id: assignment.id.as_str().to_owned(),
        assignment_version: assignment.version,
        revision: selected.revision(),
        role: assignment.role.clone(),
        modules: current.modules().to_vec(),
        permissions: current.command_iam_metadata().permissions.clone(),
        expires_at: current.expires_at(),
    }
}

pub struct LegalService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: LegalConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> LegalService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: LegalConfiguration,
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
            permission: "legal.read".into(),
            module: Some("legal".into()),
            destination: "legal".into(),
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

    pub async fn register<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        query: LegalRegisterQuery,
    ) -> Result<LegalHttpReadReply, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let _clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                LegalSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let query = query.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("legal.read")?;
                        let actor = legal_actor(current)?;
                        let mut ports = PgLegalRecords::new(connection, None, None);
                        let data = legal_register(&mut ports, &actor, query).await?;
                        Ok(LegalHttpReadReply::Register(Box::new(LegalRegisterReply {
                            data,
                            context: rendered_context(current),
                        })))
                    })
                },
            )
            .await
    }

    pub async fn detail<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: String,
    ) -> Result<LegalHttpReadReply, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                LegalSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let id = id.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("legal.read")?;
                        let actor = legal_actor(current)?;
                        let mut ports = PgLegalRecords::new(connection, None, None);
                        let data = legal_detail(&mut ports, &actor, &id).await?;
                        Ok(LegalHttpReadReply::Detail(Box::new(LegalDetailReply {
                            data,
                            context: rendered_context(current),
                        })))
                    })
                },
            )
            .await
    }
}

pub struct LegalBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<LegalService<C>>,
}

impl<C: Clock + Send + Sync + 'static> LegalBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<LegalService<C>>,
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

impl<C: Clock + Send + Sync + 'static> LegalHttpBackend for LegalBackend<C> {
    fn read<'a>(
        &'a self,
        request: LegalHttpReadRequest<'a>,
    ) -> LegalHttpFuture<'a, LegalHttpReadReply> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            match request.read {
                LegalHttpRead::Register(query) => self
                    .service
                    .register(request.host, proof, move || Ok(selection), query.clone())
                    .await
                    .map_err(business),
                LegalHttpRead::Detail(id) => self
                    .service
                    .detail(request.host, proof, move || Ok(selection), id)
                    .await
                    .map_err(business),
            }
        })
    }

    fn command<'a>(
        &'a self,
        request: LegalHttpCommandRequest<'a>,
    ) -> LegalHttpFuture<'a, LegalCommandReply> {
        Box::pin(async move {
            let (_proof, _selection) = self.context(request.headers)?;
            let _ = request.body;
            let _ = request.key;
            let _ = request.request_hash;
            Err(command_unavailable())
        })
    }
}
