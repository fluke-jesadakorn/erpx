//! Native Quality read and cancellation assembly. Current Account/Holding
//! authority remains with the selected coordinator; owner ports supply the
//! Quality workspace, detail and cancellation facts.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, PgCompanyPurchaseReader, PurchaseRequest,
    current_package_catalogue, lock_commercial_authority_sources,
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_quality::public::quality_http::{
    PgQualityCancellation, PgQualityRead, QualityHttpBackend, QualityHttpCommandRequest,
    QualityHttpFuture, QualityHttpHistoryReply, QualityHttpHistoryRequest, QualityHttpReadReply,
    QualityHttpReadRequest, QualityHttpTarget, parse_cancellation, parse_quality_history_query,
    quality_router, read_quality_cancellation_history_page, read_quality_detail_core,
};
use erpx_server_quality::public::quality_read::{
    QualityActor, QualityCancellationCommand, QualityCancellationReceipt, cancel_quality_hold,
    quality_detail, quality_detail_core_from_source, quality_workspace,
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
        "QUALITY_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn hold_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "QUALITY_HOLD_NOT_FOUND",
        "Quality hold not found.",
    )
}

const QUALITY_HISTORY_PAGE_SIZE: usize = 100;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "QUALITY_UNAVAILABLE",
        "The Quality workspace is unavailable.",
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
            // Commercial purchase authority is fenced before any owner lock so
            // the same serializable unit can prove the current History right.
            lock_commercial_authority_sources(c)
                .await
                .map_err(business)?;
            erpx_server_quality::public::quality_http::lock_quality_cancellation_writes(c)
                .await
                .map_err(business)?;
            erpx_server_quality::public::quality_http::lock_quality_read_sources(c)
                .await
                .map_err(business)?;
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

    /// Detail source facts that deliberately exclude cancellation history. The
    /// caller must fetch the complete history through the paged contract rather
    /// than treating a bounded detail array as the whole history.
    async fn detail_core<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: String,
    ) -> Result<erpx_server_quality::public::quality_read::QualityDetailCore, Error>
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
                    let id = id.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("quality.read")?;
                        let now = clock.now();
                        let witness = {
                            let catalogue = current_package_catalogue()?;
                            let mut purchase = PgCompanyPurchaseReader::new(
                                holding.clone(),
                                company.clone(),
                                connection,
                                &catalogue,
                            );
                            purchase
                                .inspect(
                                    &PurchaseRequest {
                                        capabilities: vec!["supply.quality".into()],
                                        purpose: CapabilityPurpose::History,
                                        mode: PackageEnforcementMode::Enforced,
                                    },
                                    now,
                                )
                                .await
                                .map_err(business)?
                                .require_allowed()?
                                .clone()
                        };
                        let actor = quality_actor(current)?;
                        let Some(current) = read_quality_detail_core(
                            connection,
                            &actor.company_id,
                            actor.branch_id.as_ref(),
                            &id,
                        )
                        .await?
                        else {
                            return Err(hold_not_found().into());
                        };
                        let catalogue = current_package_catalogue()?;
                        PgCompanyPurchaseReader::new(
                            holding.clone(),
                            company.clone(),
                            connection,
                            &catalogue,
                        )
                        .revalidate(&witness, now)
                        .await
                        .map_err(business)?
                        .require_allowed()?;
                        Ok(quality_detail_core_from_source(
                            &actor,
                            current.data().clone(),
                        )?)
                    })
                },
            )
            .await
    }

    /// One complete-history page in the same serializable Quality unit as the
    /// read. The cursor is parsed only after Account/IAM authorization and is a
    /// comparison input, never authority.
    async fn history<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: String,
        raw_query: Option<String>,
    ) -> Result<QualityHttpHistoryReply, Error>
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
                    let id = id.clone();
                    let raw_query = raw_query.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("quality.read")?;
                        let cursor = parse_quality_history_query(raw_query.as_deref())?;
                        let now = clock.now();
                        let witness = {
                            let catalogue = current_package_catalogue()?;
                            let mut purchase = PgCompanyPurchaseReader::new(
                                holding.clone(),
                                company.clone(),
                                connection,
                                &catalogue,
                            );
                            purchase
                                .inspect(
                                    &PurchaseRequest {
                                        capabilities: vec!["supply.quality".into()],
                                        purpose: CapabilityPurpose::History,
                                        mode: PackageEnforcementMode::Enforced,
                                    },
                                    now,
                                )
                                .await
                                .map_err(business)?
                                .require_allowed()?
                                .clone()
                        };
                        let actor = quality_actor(current)?;
                        let page = read_quality_cancellation_history_page(
                            connection,
                            &actor.company_id,
                            actor.branch_id.as_ref(),
                            &id,
                            cursor.before,
                            cursor.head,
                            cursor.revision.as_deref(),
                            QUALITY_HISTORY_PAGE_SIZE,
                        )
                        .await?;
                        let catalogue = current_package_catalogue()?;
                        PgCompanyPurchaseReader::new(
                            holding.clone(),
                            company.clone(),
                            connection,
                            &catalogue,
                        )
                        .revalidate(&witness, now)
                        .await
                        .map_err(business)?
                        .require_allowed()?;
                        Ok(QualityHttpHistoryReply {
                            items: page.rows().to_vec(),
                            next_before: page.next_before(),
                            head_version: page.head_version(),
                            history_revision: page.history_revision().to_owned(),
                        })
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
                        let receipt =
                            cancel_quality_hold(&actor, command, &mut repository, clock.as_ref())
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

    fn history<'a>(
        &'a self,
        request: QualityHttpHistoryRequest<'a>,
    ) -> QualityHttpFuture<'a, QualityHttpHistoryReply> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .history(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.id.to_owned(),
                    request.raw_query.map(str::to_owned),
                )
                .await
                .map_err(business)
        })
    }

    fn detail_core<'a>(
        &'a self,
        request: QualityHttpReadRequest<'a>,
    ) -> QualityHttpFuture<'a, erpx_server_quality::public::quality_read::QualityDetailCore> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let Some(id) = request.id else {
                return Err(not_found());
            };
            self.service
                .detail_core(request.host, proof, move || Ok(selection), id.to_owned())
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
