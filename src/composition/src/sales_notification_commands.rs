//! Native Sales customer-notification settings/queue/retry composition.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_commercial::public::purchases::PackageEnforcementMode;
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_sales::public::notification_commands::{
    NotificationActor, NotificationHttpBackend, NotificationHttpCommand,
    NotificationHttpFuture, NotificationHttpRequest, NotificationHttpResponse,
    NotificationHttpTarget, PgNotificationCommands, queue_notification,
    retry_notification, sales_notification_commands_router, update_settings,
};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
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
        "The Sales service is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

fn package_mode() -> PackageEnforcementMode {
    match std::env::var("ERPX_COMMERCE_PACKAGE_ENFORCEMENT")
        .unwrap_or_else(|_| "disabled".to_owned())
        .as_str()
    {
        "disabled" => PackageEnforcementMode::Disabled,
        "shadow" => PackageEnforcementMode::Shadow,
        _ => PackageEnforcementMode::Enforced,
    }
}

fn production() -> bool {
    std::env::var("NODE_ENV").map(|value| value == "production").unwrap_or(false)
}

#[derive(Clone, Copy)]
struct NotificationSources;

impl HoldingWorkSourcePreparation for NotificationSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct SalesNotificationCommandConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl SalesNotificationCommandConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn actor(current: &AuthorizedWorkContext) -> Result<NotificationActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(NotificationActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.as_str().to_owned(),
        branch_id: workspace.branch_id.as_ref().map(|id| id.as_str().to_owned()),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|id| id.as_str().to_owned()),
        assignment_id: assignment.id.as_str().to_owned(),
        role: assignment.role.clone(),
        session_id: Some(current.session_id().as_str().to_owned()),
        assignment_version: Some(assignment.version),
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

pub struct SalesNotificationCommandService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: SalesNotificationCommandConfiguration,
    clock: Arc<C>,
    mode: PackageEnforcementMode,
    production: bool,
}

impl<C: Clock + Send + Sync + 'static> SalesNotificationCommandService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: SalesNotificationCommandConfiguration,
        clock: Arc<C>,
        mode: PackageEnforcementMode,
        production: bool,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
            mode,
            production,
        }
    }

    pub fn from_runtime(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: SalesNotificationCommandConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self::new(authorizer, configuration, clock, package_mode(), production())
    }

    pub fn company(&self) -> &CompanyId {
        self.configuration.company()
    }

    pub async fn execute(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        key: String,
        command: NotificationHttpCommand,
    ) -> Result<NotificationHttpResponse, Error> {
        let permission = match &command {
            NotificationHttpCommand::Settings { .. } => "commerce.approve",
            _ => "commerce.sales",
        };
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let mode = self.mode;
        let production = self.production;
        let expected = company.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    let mut request = selection;
                    if request.company_id != expected {
                        return Err(scope());
                    }
                    request.mutation = true;
                    Ok(request)
                },
                WorkAuthorizationRequest {
                    permission: permission.into(),
                    module: Some("sales".into()),
                    destination: "sales".into(),
                    api: true,
                },
                NotificationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission(permission)?;
                        let actor = actor(current)?;
                        if !actor.can(permission) {
                            return Err(
                                erpx_server_sales::public::notification_commands::access_changed().into(),
                            );
                        }
                        let now = clock.now().get();
                        let mut repository =
                            PgNotificationCommands::new(connection, mode, production);
                        let data = match command {
                            NotificationHttpCommand::Settings { input } => {
                                let receipt =
                                    update_settings(&actor, input, &key, &mut repository, now)
                                        .await?;
                                receipt.wire()
                            }
                            NotificationHttpCommand::Queue {
                                fulfillment_id,
                                input,
                            } => {
                                let id = format!("customer-notification-{}", uuid::Uuid::new_v4());
                                let receipt = queue_notification(
                                    &actor,
                                    &fulfillment_id,
                                    input,
                                    &key,
                                    &mut repository,
                                    now,
                                    &id,
                                )
                                .await?;
                                receipt.wire()
                            }
                            NotificationHttpCommand::Retry {
                                notification_id,
                                input,
                            } => {
                                let receipt = retry_notification(
                                    &actor,
                                    &notification_id,
                                    input,
                                    &key,
                                    &mut repository,
                                    now,
                                )
                                .await?;
                                receipt.wire()
                            }
                        };
                        Ok(NotificationHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}


pub struct SalesNotificationCommandBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<SalesNotificationCommandService<C>>,
}

impl<C: Clock + Send + Sync + 'static> SalesNotificationCommandBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<SalesNotificationCommandService<C>>,
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

impl<C: Clock + Send + Sync + 'static> NotificationHttpBackend
    for SalesNotificationCommandBackend<C>
{
    fn execute<'a>(
        &'a self,
        request: NotificationHttpRequest<'a>,
    ) -> NotificationHttpFuture<'a, NotificationHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .execute(
                    request.host,
                    proof,
                    selection,
                    request.idempotency_key,
                    request.command,
                )
                .await
                .map_err(business)
        })
    }
}

pub fn target(
    host: String,
    holding_slug: String,
    origin: String,
    backend: Arc<dyn NotificationHttpBackend>,
) -> NotificationHttpTarget {
    NotificationHttpTarget {
        host,
        holding_slug,
        origin,
        backend,
    }
}

pub fn router(
    targets: Vec<NotificationHttpTarget>,
) -> Result<erpx_server_runtime::public::http_support::Router, BusinessError> {
    sales_notification_commands_router(targets)
}
