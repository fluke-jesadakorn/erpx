//! Native People workforce directory command assembly. Account/Holding
//! authority stays with the selected coordinator; Organization and Identity
//! ports supply the cross-owner effects inside the owner transaction.
use crate::business_http::BusinessScopeSlugs;
use crate::people_workforce_ports::{IdentityAccountLinkPort, OrganizationStaffPlacementPort};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest, WorkPermissionRequirement,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_people_payroll::public::workforce::{
    PgWorkforceCommand, WorkforceCommandActor, save_person,
};
use erpx_server_people_payroll::public::workforce_http::{
    WorkforceHttpBackend, WorkforceHttpFuture, WorkforceHttpKind, WorkforceHttpRequest,
    WorkforceHttpResponse, WorkforceHttpTarget,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{DatabasePool, PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, FailureKind, HoldingId, TimestampMillis,
};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PEOPLE_WORKFORCE_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_WORKFORCE_UNAVAILABLE",
        "The native People directory service is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct WorkforceSources;

impl HoldingWorkSourcePreparation for WorkforceSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

fn actor(current: &AuthorizedWorkContext) -> Result<WorkforceCommandActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    let metadata = current.command_iam_metadata();
    Ok(WorkforceCommandActor {
        user_id: current.user_id().as_str().to_owned(),
        holding_id: current.holding_id().clone(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        assignment_id: assignment.id.as_str().to_owned(),
        role: assignment.role.to_owned(),
        permissions: metadata.permissions.clone(),
        work_context: serde_json::to_string(&serde_json::json!({
            "assignmentId": assignment.id.as_str(),
            "companyId": workspace.company_id.as_str(),
            "branchId": workspace.branch_id.as_ref().map(|value| value.as_str()),
            "departmentId": workspace.department_id.as_ref().map(|value| value.as_str()),
            "revision": selected.revision(),
            "assignmentVersion": assignment.version,
        }))
        .map_err(|_| unavailable())?,
        expires_at: current.expires_at(),
        beta: false,
    })
}

#[derive(Clone)]
pub struct WorkforceConfiguration {
    holding: HoldingId,
    company: erpx_server_shared::public::CompanyId,
}

impl WorkforceConfiguration {
    pub fn new(holding: HoldingId, company: erpx_server_shared::public::CompanyId) -> Self {
        Self { holding, company }
    }
}

#[derive(Clone)]
pub struct WorkforceService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    configuration: WorkforceConfiguration,
    control: Arc<DatabasePool>,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> WorkforceService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        configuration: WorkforceConfiguration,
        control: Arc<DatabasePool>,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            policy,
            scopes,
            configuration,
            control,
            clock,
        }
    }

    fn access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: WorkPermissionRequirement::AnyOf(vec!["people.manage".into()]),
            module: Some("people_payroll".into()),
            destination: "people/directory".into(),
            api: true,
        }
    }

    fn context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(scope());
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.configuration.company.clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: true,
            },
        ))
    }

    pub async fn command<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        kind: WorkforceHttpKind,
        body: Value,
        key: CommandKey,
    ) -> Result<WorkforceHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let input_company = company.clone();
        let control = self.control.clone();
        let clock = self.clock.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != input_company {
                return Err(scope());
            }
            request.mutation = true;
            Ok(request)
        };
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                Self::access(),
                WorkforceSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let control = control.clone();
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
                        let actor = actor(current).map_err(PgTransactionError::Business)?;
                        let now = clock.now();
                        let (person_id, raw) = match kind {
                            WorkforceHttpKind::Create => (None, body),
                            WorkforceHttpKind::Update { person_id } => (Some(person_id), body),
                        };
                        let mut repository = PgWorkforceCommand::new(connection, &actor);
                        let mut placement = OrganizationStaffPlacementPort;
                        let mut account_link = IdentityAccountLinkPort::new(control);
                        let result = save_person(
                            &actor,
                            person_id.as_deref(),
                            &raw,
                            &key,
                            &mut repository,
                            &mut placement,
                            &mut account_link,
                            TimestampMillis::new(now.get()),
                        )
                        .await?;
                        let data = serde_json::to_value(result)
                            .map_err(|_| PgTransactionError::Business(unavailable()))?;
                        Ok(WorkforceHttpResponse {
                            data,
                            context: serde_json::json!({
                                "companyId": company.as_str(),
                                "assignmentId": actor.assignment_id,
                            }),
                        })
                    })
                },
            )
            .await
    }
}

#[derive(Clone)]
pub struct WorkforceBackend<C> {
    service: Arc<WorkforceService<C>>,
}

impl<C> WorkforceBackend<C> {
    pub fn new(service: Arc<WorkforceService<C>>) -> Self {
        Self { service }
    }
}

impl<C: Clock + Send + Sync + 'static> WorkforceHttpBackend for WorkforceBackend<C> {
    fn command<'a>(
        &'a self,
        request: WorkforceHttpRequest<'a>,
    ) -> WorkforceHttpFuture<'a, WorkforceHttpResponse> {
        let (proof, selection) = match self.service.context(request.headers) {
            Ok(context) => context,
            Err(error) => return Box::pin(async move { Err(error) }),
        };
        Box::pin(async move {
            self.service
                .command(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.kind.clone(),
                    request.body.clone(),
                    request.key.clone(),
                )
                .await
                .map_err(business)
        })
    }
}

#[allow(dead_code)]
fn _targets(targets: Vec<WorkforceHttpTarget>) -> Vec<WorkforceHttpTarget> {
    targets
}
