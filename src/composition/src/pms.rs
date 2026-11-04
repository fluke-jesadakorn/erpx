//! Native PMS room-board and front-desk HTTP assembly. Current Account/Holding
//! authority remains with the selected coordinator; owner ports supply the PMS
//! workspace, staff and package entitlement facts.
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
use erpx_server_pms::public::front_desk::{
    PgPmsFrontDeskRead, PmsFrontDeskSelection, lock_front_desk_sources, pms_front_desk,
};
use erpx_server_pms::public::pms_http::{
    PmsHttpBackend, PmsHttpFuture, PmsHttpResponse, PmsHttpReadRequest, PmsHttpFrontDeskRequest,
};
use erpx_server_pms::public::room_board::{
    PmsBoardActor, PmsBoardEntitlement, PmsBoardEntitlementPort, PmsBoardSelection, PmsStaff,
    PmsStaffDirectory, PgPmsBoardRead, pms_room_board,
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
        "PMS_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_UNAVAILABLE",
        "The PMS workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct PmsSources;

impl HoldingWorkSourcePreparation for PmsSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move { lock_front_desk_sources(c).await })
    }
}

struct PmsStaffDirectoryImpl;

impl PmsStaffDirectory for PmsStaffDirectoryImpl {
    type Error = BusinessError;
    fn staff<'a>(
        &'a mut self,
        _company_id: &'a str,
        _branch_id: Option<&'a str>,
    ) -> erpx_server_pms::public::room_board::PmsBoardFuture<'a, Vec<PmsStaff>, Self::Error> {
        Box::pin(async { Ok(Vec::new()) })
    }
}

struct PmsEntitlementImpl;

impl PmsBoardEntitlementPort for PmsEntitlementImpl {
    type Error = BusinessError;
    fn entitlement<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> erpx_server_pms::public::room_board::PmsBoardFuture<'a, PmsBoardEntitlement, Self::Error>
    {
        Box::pin(async { Ok(PmsBoardEntitlement { enforced: false, history_allowed: false }) })
    }
}

#[derive(Clone)]
pub struct PmsConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl PmsConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn pms_actor(current: &AuthorizedWorkContext) -> Result<PmsBoardActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    Ok(PmsBoardActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.as_str().to_owned(),
        branch_id: workspace.branch_id.as_ref().map(|value| value.as_str().to_owned()),
        department_id: workspace.department_id.as_ref().map(|value| value.as_str().to_owned()),
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

pub struct PmsService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: PmsConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> PmsService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: PmsConfiguration,
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
            permission: "pms.read".into(),
            module: Some("pms".into()),
            destination: "pms".into(),
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
        selection: PmsBoardSelection,
    ) -> Result<PmsHttpResponse, Error>
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
                PmsSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let selection = selection.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("pms.read")?;
                        let actor = pms_actor(current)?;
                        let mut repository = PgPmsBoardRead::new(connection);
                        let mut staff = PmsStaffDirectoryImpl;
                        let mut entitlement = PmsEntitlementImpl;
                        let now = clock.now().get();
                        let data = pms_room_board(
                            &actor,
                            &selection,
                            &mut repository,
                            &mut staff,
                            &mut entitlement,
                            now,
                        )
                        .await?;
                        Ok(PmsHttpResponse {
                            data: data.value(),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn front_desk<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        selection: PmsFrontDeskSelection,
    ) -> Result<PmsHttpResponse, Error>
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
                PmsSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let selection = selection.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("pms.read")?;
                        let actor = pms_actor(current)?;
                        let mut repository = PgPmsFrontDeskRead::new(connection);
                        let mut entitlement = PmsEntitlementImpl;
                        let now = clock.now().get();
                        let data = pms_front_desk(
                            &actor,
                            &selection,
                            &mut repository,
                            &mut entitlement,
                            now,
                        )
                        .await?;
                        Ok(PmsHttpResponse {
                            data: data.value(),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct PmsBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<PmsService<C>>,
}

impl<C: Clock + Send + Sync + 'static> PmsBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<PmsService<C>>,
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

impl<C: Clock + Send + Sync + 'static> PmsHttpBackend for PmsBackend<C> {
    fn read<'a>(
        &'a self,
        request: PmsHttpReadRequest<'a>,
    ) -> PmsHttpFuture<'a, PmsHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let board_selection = request.selection.clone();
            self.service
                .read(request.host, proof, move || Ok(selection), board_selection)
                .await
                .map_err(business)
        })
    }

    fn front_desk<'a>(
        &'a self,
        request: PmsHttpFrontDeskRequest<'a>,
    ) -> PmsHttpFuture<'a, PmsHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let desk_selection = request.selection.clone();
            self.service
                .front_desk(request.host, proof, move || Ok(selection), desk_selection)
                .await
                .map_err(business)
        })
    }
}
