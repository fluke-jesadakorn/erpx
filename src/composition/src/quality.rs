//! Native Quality read and cancellation assembly. Current Account/Holding
//! authority remains with the selected coordinator; owner ports supply the
//! Quality workspace, detail and cancellation facts.
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
use erpx_server_quality::public::quality_http::{
    PgQualityCancellation, PgQualityRead, QualityHttpBackend, QualityHttpCommandRequest,
    QualityHttpFuture, QualityHttpReadReply, QualityHttpReadRequest, QualityHttpTarget,
    parse_cancellation, quality_router,
};
use erpx_server_quality::public::quality_read::{
    QualityActor, QualityCancellationCommand, QualityCancellationReceipt, cancel_quality_hold,
    quality_detail, quality_workspace,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "QUALITY_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "QUALITY_UNAVAILABLE",
        "The Quality workspace is unavailable.",
    )
}

fn command_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "QUALITY_COMMAND_UNAVAILABLE",
        "Native Quality commands are not enabled yet.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct QualitySources;

impl HoldingWorkSourcePreparation for QualitySources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            erpx_server_quality::public::quality_http::lock_quality_cancellation_writes(c).await.map_err(business)?;
            erpx_server_quality::public::quality_http::lock_quality_read_sources(c).await.map_err(business)?;
            Ok(())
        })
    }
}

#[derive(Clone)]
pub struct QualityConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl QualityConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn quality_actor(current: &AuthorizedWorkContext) -> Result<QualityActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    Ok(QualityActor {
        user_id: current.user_id().clone(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        permissions: current.command_iam_metadata().permissions.clone(),
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

pub struct QualityService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: QualityConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> QualityService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: QualityConfiguration,
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
            permission: "quality.read".into(),
            module: Some("quality".into()),
            destination: "quality".into(),
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

    pub async fn workspace<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
    ) -> Result<QualityHttpReadReply, Error>
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
                QualitySources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("quality.read")?;
                        let actor = quality_actor(current)?;
                        let mut repository = PgQualityRead { connection };
                        let data = quality_workspace(&actor, &mut repository).await?;
                        let _ = clock;
                        Ok(QualityHttpReadReply::Workspace(Box::new(data)))
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
    ) -> Result<QualityHttpReadReply, Error>
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
                QualitySources,
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
                        current.require_permission("quality.read")?;
                        let actor = quality_actor(current)?;
                        let mut repository = PgQualityRead { connection };
                        let data = quality_detail(&actor, &id, &mut repository).await?;
                        Ok(QualityHttpReadReply::Detail(Box::new(data)))
                    })
                },
            )
            .await
    }

    pub async fn cancel<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: QualityCancellationCommand,
    ) -> Result<QualityCancellationReceipt, Error>
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
                QualitySources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let command = command.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("quality.read")?;
                        let actor = quality_actor(current)?;
                        let mut repository = PgQualityCancellation { connection };
                        let receipt = cancel_quality_hold(
                            &actor,
                            command,
                            &mut repository,
                            clock.as_ref(),
                        )
                        .await?;
                        Ok(receipt)
                    })
                },
            )
            .await
    }
}

pub struct QualityBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<QualityService<C>>,
}

impl<C: Clock + Send + Sync + 'static> QualityBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<QualityService<C>>,
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

impl<C: Clock + Send + Sync + 'static> QualityHttpBackend for QualityBackend<C> {
    fn read<'a>(
        &'a self,
        request: QualityHttpReadRequest<'a>,
    ) -> QualityHttpFuture<'a, QualityHttpReadReply> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            if let Some(id) = request.id {
                self.service
                    .detail(request.host, proof, move || Ok(selection), id.to_owned())
                    .await
                    .map_err(business)
            } else {
                self.service
                    .workspace(request.host, proof, move || Ok(selection))
                    .await
                    .map_err(business)
            }
        })
    }

    fn cancel<'a>(
        &'a self,
        request: QualityHttpCommandRequest<'a>,
    ) -> QualityHttpFuture<'a, QualityCancellationReceipt> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let command = parse_cancellation(request.body, request.id, request.key.clone())?;
            self.service
                .cancel(request.host, proof, move || Ok(selection), command)
                .await
                .map_err(business)
        })
    }
}

// Retain the router constructor in feature-specific builds.
#[allow(dead_code)]
fn _router(targets: Vec<QualityHttpTarget>) -> Result<axum::Router, BusinessError> {
    quality_router(targets)
}
