//! Native Commercial company package HTTP mount. Current Account/Holding
//! authority remains with the selected coordinator; owner ports supply Company
//! package overview and purchase admission facts.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_commercial::public::package_review::{
    PackageLifecycleReviewSigner, PackageReviewActor, PgPackageLifecycleReview,
};
use erpx_server_commercial::public::purchases::{
    CommercialCatalogue, CompanyPackageOverview, CompanyPackageReadHttpBackend,
    CompanyPackageReadHttpFuture, CompanyPackageReadHttpKind, CompanyPackageReadHttpRequest,
    CompanyPackageReadHttpResponse, CompanyPackageReadHttpTarget, CompanyPurchaseAdmission,
    CompanyPurchaseAdmissionPort, GrantChange, PackageGrantHttpCommand, PackageGrantHttpFuture,
    PackageGrantHttpRequest, PackageGrantHttpResponse, PackageGrantsHttpBackend,
    PgCompanyPurchaseReader, PgPackageBatchReview, PgPackageGrantCommands, PgPackageGrantPolicy,
    PurchaseRequest, company_package_overview, company_package_router, current_package_catalogue,
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_holding_bindings::ConfiguredHoldingRegistry,
    account_inbound_http::{parse_inbound_account_request, unique_request_header},
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    platform_commercial_authority::{
        CurrentPlatformCommercialAuthority, PlatformCommercialOperation,
        verify_current_platform_commercial_authority,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{
    DatabasePool, PersistenceError, PgRequestLease, PgTransactionError, PgWorkFuture,
};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, FailureKind, HoldingId, TimestampMillis, UserId,
};
use serde_json::Value;
use sqlx::PgConnection;
use std::{collections::BTreeMap, sync::Arc};

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "COMMERCIAL_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMERCIAL_UNAVAILABLE",
        "The Commercial workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct CommercialSources;

impl HoldingWorkSourcePreparation for CommercialSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct CommercialConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl CommercialConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
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

pub struct CommercialService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: CommercialConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> CommercialService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: CommercialConfiguration,
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
            permission: "commerce.read".into(),
            module: Some("commercial".into()),
            destination: "commercial".into(),
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

    pub async fn read<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        kind: CompanyPackageReadHttpKind,
    ) -> Result<CompanyPackageReadHttpResponse, Error>
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
                CommercialSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let kind = kind.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let catalogue: CommercialCatalogue = current_package_catalogue()
                            .map_err(|_| PgTransactionError::Business(unavailable()))?;
                        let now: TimestampMillis = clock.now();
                        let data = match kind {
                            CompanyPackageReadHttpKind::Overview => {
                                let mut reader = PgCompanyPurchaseReader::new(
                                    current.holding_id().clone(),
                                    current.selection().workspace().company_id.clone(),
                                    connection,
                                    &catalogue,
                                );
                                let overview: CompanyPackageOverview = company_package_overview(
                                    &mut reader,
                                    &catalogue,
                                    &company,
                                    now,
                                )
                                .await
                                .map_err(|error| match error {
                                    PgTransactionError::Business(error) => {
                                        PgTransactionError::Business(error)
                                    }
                                    PgTransactionError::Persistence(_) => {
                                        PgTransactionError::Business(unavailable())
                                    }
                                })?;
                                serde_json::to_value(overview)
                                    .map_err(|_| PgTransactionError::Business(unavailable()))?
                            }
                            CompanyPackageReadHttpKind::Admission {
                                capabilities,
                                purpose,
                                mode,
                            } => {
                                let mut reader = PgCompanyPurchaseReader::new(
                                    current.holding_id().clone(),
                                    current.selection().workspace().company_id.clone(),
                                    connection,
                                    &catalogue,
                                );
                                let admission: CompanyPurchaseAdmission = reader
                                    .inspect_admission(
                                        &PurchaseRequest {
                                            capabilities,
                                            purpose,
                                            mode,
                                        },
                                        now,
                                    )
                                    .await
                                    .map_err(|error| match error {
                                        PgTransactionError::Business(error) => {
                                            PgTransactionError::Business(error)
                                        }
                                        PgTransactionError::Persistence(_) => {
                                            PgTransactionError::Business(unavailable())
                                        }
                                    })?;
                                serde_json::to_value(admission)
                                    .map_err(|_| PgTransactionError::Business(unavailable()))?
                            }
                        };
                        Ok(CompanyPackageReadHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct CommercialBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<CommercialService<C>>,
}

impl<C: Clock + Send + Sync + 'static> CommercialBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<CommercialService<C>>,
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

impl<C: Clock + Send + Sync + 'static> CompanyPackageReadHttpBackend for CommercialBackend<C> {
    fn read<'a>(
        &'a self,
        request: CompanyPackageReadHttpRequest<'a>,
    ) -> CompanyPackageReadHttpFuture<'a, CompanyPackageReadHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let kind = request.kind.clone();
            self.service
                .read(request.host, proof, move || Ok(selection), kind)
                .await
                .map_err(business)
        })
    }
}

#[allow(dead_code)]
fn _router(targets: Vec<CompanyPackageReadHttpTarget>) -> Result<axum::Router, BusinessError> {
    company_package_router(targets)
}

/// Composition-owned clock for the Platform Commercial grant unit. The owner
/// application only observes time through this port.
pub struct CommercialClock(Arc<dyn Clock + Send + Sync>);

impl CommercialClock {
    pub fn new(clock: Arc<dyn Clock + Send + Sync>) -> Self {
        Self(clock)
    }
}

impl Clock for CommercialClock {
    fn now(&self) -> TimestampMillis {
        self.0.now()
    }
}

/// Exact registered Platform Host, Control authority and target Holding pools.
/// A request body selects the target Holding/Company; it never selects the
/// Platform authority, its role or its permissions.
pub struct CommercialPackageGrantsConfiguration {
    platform_host: String,
    platform_holding: HoldingId,
    platform_slug: String,
    control: DatabasePool,
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    holdings: BTreeMap<HoldingId, DatabasePool>,
    signer: Arc<PackageLifecycleReviewSigner>,
    clock: Arc<CommercialClock>,
}

impl CommercialPackageGrantsConfiguration {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        platform_host: String,
        platform_holding: HoldingId,
        platform_slug: String,
        control: DatabasePool,
        policy: Arc<AccountPolicy>,
        registry: Arc<ConfiguredHoldingRegistry>,
        holdings: BTreeMap<HoldingId, DatabasePool>,
        signer: Arc<PackageLifecycleReviewSigner>,
        clock: Arc<CommercialClock>,
    ) -> Self {
        Self {
            platform_host,
            platform_holding,
            platform_slug,
            control,
            policy,
            registry,
            holdings,
            signer,
            clock,
        }
    }
}

pub struct CommercialPackageGrantsBackend {
    configuration: CommercialPackageGrantsConfiguration,
}

impl CommercialPackageGrantsBackend {
    pub fn new(configuration: CommercialPackageGrantsConfiguration) -> Self {
        Self { configuration }
    }
}

impl PackageGrantsHttpBackend for CommercialPackageGrantsBackend {
    fn execute<'a>(
        &'a self,
        request: PackageGrantHttpRequest<'a>,
    ) -> PackageGrantHttpFuture<'a, PackageGrantHttpResponse> {
        Box::pin(async move {
            let inbound =
                parse_inbound_account_request(&self.configuration.policy, request.headers)?;
            if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
                return Err(not_found());
            }
            let proof = inbound.into_proof();
            if proof.active.is_none() {
                return Err(BusinessError::new(
                    FailureKind::Unauthenticated,
                    "UNAUTHENTICATED",
                    "Sign in to continue.",
                ));
            }
            let hint = unique_request_header(request.headers, "x-erpx-platform-assignment")?
                .map(str::to_owned);
            let platform_host = self.configuration.platform_host.clone();
            let platform_holding = self.configuration.platform_holding.clone();
            let platform_slug = self.configuration.platform_slug.clone();
            let control = self.configuration.control.clone();
            let policy = self.configuration.policy.clone();
            let registry = self.configuration.registry.clone();
            let holdings = self.configuration.holdings.clone();
            let signer = self.configuration.signer.clone();
            let clock = self.configuration.clock.clone();
            let holding_id = request.holding_id.to_owned();
            let command = request.command;
            PgRequestLease::new(control.clone())
                .run(move |control_connection| {
                    Box::pin(async move {
                        let now = clock.now().get();
                        let operation = if command.is_apply() {
                            PlatformCommercialOperation::Apply
                        } else {
                            PlatformCommercialOperation::Read
                        };
                        let authority = verify_current_platform_commercial_authority(
                            control_connection,
                            &platform_host,
                            proof,
                            (*policy).clone(),
                            registry,
                            &platform_holding,
                            hint.as_deref(),
                            operation,
                            now,
                        )
                        .await?;
                        let context = serde_json::json!({
                            "userId": authority.selected().user_id.as_str(),
                            "holdingId": platform_holding.as_str(),
                            "holdingSlug": platform_slug,
                            "platformAssignmentId": authority.selected().assignment_id.as_str(),
                            "platformRole": authority.selected().role.as_str(),
                            "roleRevision": authority.selected().role_revision,
                        });
                        let holding = HoldingId::new(&holding_id)?;
                        let pool = holdings.get(&holding).ok_or_else(|| {
                            BusinessError::new(
                                FailureKind::NotFound,
                                "HOLDING_NOT_FOUND",
                                "Select an active Holding.",
                            )
                        })?;
                        let company = CompanyId::new(command.company_id())?;
                        let catalogue = current_package_catalogue()?;
                        let data = run_package_grant(
                            pool,
                            &catalogue,
                            &holding,
                            &company,
                            control_connection,
                            &authority,
                            &signer,
                            clock.as_ref(),
                            &command,
                        )
                        .await?;
                        Ok(PackageGrantHttpResponse { data, context })
                    })
                })
                .await
                .map_err(business)
        })
    }
}

struct PlatformPackageGrantPolicy<'a> {
    control: &'a mut PgConnection,
    authority: &'a CurrentPlatformCommercialAuthority,
    clock: &'a CommercialClock,
    holding: HoldingId,
    company: CompanyId,
}

impl<'a> PlatformPackageGrantPolicy<'a> {
    fn new(
        control: &'a mut PgConnection,
        authority: &'a CurrentPlatformCommercialAuthority,
        clock: &'a CommercialClock,
        holding: HoldingId,
        company: CompanyId,
    ) -> Self {
        Self {
            control,
            authority,
            clock,
            holding,
            company,
        }
    }
}

impl PgPackageGrantPolicy for PlatformPackageGrantPolicy<'_> {
    async fn prepare_sources(
        &mut self,
        _: &mut PgConnection,
        _: &GrantChange,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        // The enclosing owner unit already leases the Commercial authority
        // sources. Owner settlement sources are supplied by the dedicated
        // entitlement port; until then `obligations` stays closed.
        Ok(())
    }

    async fn authorize(
        &mut self,
        connection: &mut PgConnection,
        holding: &HoldingId,
        company: &CompanyId,
    ) -> Result<UserId, PgTransactionError<BusinessError>> {
        if holding != &self.holding || company != &self.company {
            return Err(BusinessError::invalid(
                "COMMERCIAL_COMPANY_MISMATCH",
                "The command belongs to another Company.",
            )
            .into());
        }
        self.authority
            .require_current(self.control, self.clock.now().get())
            .await?;
        let context = erpx_server_organization::public::read_company_context(
            &mut erpx_server_organization::public::PgOrganizationReader::new(
                self.holding.clone(),
                connection,
            ),
            company,
        )
        .await?;
        if context.company().id.as_str() != company.as_str() {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "COMMERCIAL_COMPANY_CHANGED",
                "The selected Company changed.",
            )
            .into());
        }
        UserId::new(&self.authority.selected().user_id).map_err(PgTransactionError::from)
    }

    async fn obligations(
        &mut self,
        _: &mut PgConnection,
        _: &CompanyId,
        _: &str,
    ) -> Result<i64, PgTransactionError<BusinessError>> {
        // Owner settlement evidence is not ported in this packet. Fail closed
        // instead of inventing a zero backlog.
        Err(BusinessError::new(
            FailureKind::Conflict,
            "PACKAGE_OBLIGATION_SOURCES_REQUIRED",
            "The package owners must provide current settlement evidence before this change.",
        )
        .into())
    }
}

/// Preview only. An accidental Apply fails before a grant or review row is
/// staged; the owner batch command never calls these hooks for Preview.
struct PreviewOnlyReview;

impl PgPackageBatchReview for PreviewOnlyReview {
    async fn prepare_sources(
        &mut self,
        _: &mut PgConnection,
        _: &GrantChange,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        Ok(())
    }
    async fn verify(
        &mut self,
        _: &mut PgConnection,
        _: &erpx_server_commercial::public::purchases::GrantBatchPreview,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        Err(platform_commercial_required().into())
    }
    async fn retain(
        &mut self,
        _: &mut PgConnection,
        _: &erpx_server_commercial::public::purchases::GrantBatchPreview,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        Err(platform_commercial_required().into())
    }
}

fn platform_commercial_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PLATFORM_COMMERCIAL_REQUIRED",
        "A current Commercial management and MFA lease is required to apply packages.",
    )
}

struct AppliedReview<'a> {
    inner: PgPackageLifecycleReview<'a, CommercialClock>,
    wrote: bool,
}

impl PgPackageBatchReview for AppliedReview<'_> {
    async fn prepare_sources(
        &mut self,
        connection: &mut PgConnection,
        input: &GrantChange,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        self.inner.prepare_sources(connection, input).await
    }
    async fn verify(
        &mut self,
        connection: &mut PgConnection,
        preview: &erpx_server_commercial::public::purchases::GrantBatchPreview,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        self.inner.verify(connection, preview).await?;
        self.wrote = true;
        Ok(())
    }
    async fn retain(
        &mut self,
        connection: &mut PgConnection,
        preview: &erpx_server_commercial::public::purchases::GrantBatchPreview,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        self.inner.retain(connection, preview).await
    }
}

fn review_actor(authority: &CurrentPlatformCommercialAuthority) -> PackageReviewActor {
    let selected = authority.selected();
    PackageReviewActor {
        user_id: selected.user_id.clone(),
        session_id: selected.session_id.clone(),
        assignment_id: selected.assignment_id.clone(),
        role: selected.role.clone(),
        role_revision: selected.role_revision,
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_package_grant(
    pool: &DatabasePool,
    catalogue: &CommercialCatalogue,
    holding: &HoldingId,
    company: &CompanyId,
    control: &mut PgConnection,
    authority: &CurrentPlatformCommercialAuthority,
    signer: &PackageLifecycleReviewSigner,
    clock: &CommercialClock,
    command: &PackageGrantHttpCommand,
) -> Result<Value, PgTransactionError<BusinessError>> {
    for attempt in 0..4 {
        let mut unit = pool.raw().begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *unit)
            .await?;
        sqlx::query("SET LOCAL lock_timeout='5s'")
            .execute(&mut *unit)
            .await?;
        let outcome: Result<Value, PgTransactionError<BusinessError>> = async {
            let data = match command {
                PackageGrantHttpCommand::Preview(input) => {
                    let mut policy = PlatformPackageGrantPolicy::new(
                        control,
                        authority,
                        clock,
                        holding.clone(),
                        company.clone(),
                    );
                    let preview = PgPackageGrantCommands::new(
                        holding.clone(),
                        company.clone(),
                        &mut unit,
                        catalogue,
                        &mut policy,
                        clock,
                    )
                    .preview(input)
                    .await?;
                    serde_json::json!({
                        "before": preview.before,
                        "after": preview.after,
                        "expectedVersion": preview.expected_version,
                    })
                }
                PackageGrantHttpCommand::Apply(input) => {
                    let mut policy = PlatformPackageGrantPolicy::new(
                        control,
                        authority,
                        clock,
                        holding.clone(),
                        company.clone(),
                    );
                    let grant = PgPackageGrantCommands::new(
                        holding.clone(),
                        company.clone(),
                        &mut unit,
                        catalogue,
                        &mut policy,
                        clock,
                    )
                    .apply(input)
                    .await?;
                    serde_json::to_value(grant)
                        .map_err(|_| PersistenceError::new("COMMERCIAL_REPLY_INVALID"))?
                }
                PackageGrantHttpCommand::PreviewBatch(input) => {
                    let comparison = {
                        let mut policy = PlatformPackageGrantPolicy::new(
                            control,
                            authority,
                            clock,
                            holding.clone(),
                            company.clone(),
                        );
                        PgPackageGrantCommands::new(
                            holding.clone(),
                            company.clone(),
                            &mut unit,
                            catalogue,
                            &mut policy,
                            clock,
                        )
                        .preview_batch(input, &mut PreviewOnlyReview)
                        .await?
                    };
                    let review = signer.issue(
                        catalogue,
                        review_actor(authority),
                        holding.as_str(),
                        input,
                        &input.expected_revision,
                        clock.now().get(),
                    )?;
                    serde_json::json!({
                        "comparison": comparison,
                        "request": input,
                        "review": review,
                    })
                }
                PackageGrantHttpCommand::ApplyBatch { input, token } => {
                    let actor = review_actor(authority);
                    let mut review = AppliedReview {
                        inner: PgPackageLifecycleReview::new(
                            signer,
                            clock,
                            catalogue,
                            &actor,
                            holding.as_str(),
                            input,
                            token,
                        )?,
                        wrote: false,
                    };
                    let result = {
                        let mut policy = PlatformPackageGrantPolicy::new(
                            control,
                            authority,
                            clock,
                            holding.clone(),
                            company.clone(),
                        );
                        PgPackageGrantCommands::new(
                            holding.clone(),
                            company.clone(),
                            &mut unit,
                            catalogue,
                            &mut policy,
                            clock,
                        )
                        .apply_batch(input, &mut review)
                        .await?
                    };
                    if review.wrote {
                        review.retain(&mut unit, &result).await?;
                    }
                    serde_json::json!({ "receipt": result })
                }
            };
            authority
                .require_current(control, clock.now().get())
                .await?;
            unit.commit().await?;
            Ok(data)
        }
        .await;
        match outcome {
            Ok(data) => return Ok(data),
            Err(PgTransactionError::Persistence(error)) if error.retryable() && attempt < 3 => {
                continue;
            }
            Err(error) => return Err(error),
        }
    }
    Err(PersistenceError::new("PACKAGE_GRANT_RETRY_EXHAUSTED").into())
}

#[allow(dead_code)]
fn _package_grant_router(
    targets: Vec<erpx_server_commercial::public::purchases::PackageGrantsHttpTarget>,
) -> Result<axum::Router, BusinessError> {
    erpx_server_commercial::public::purchases::package_grants_router(targets)
}
