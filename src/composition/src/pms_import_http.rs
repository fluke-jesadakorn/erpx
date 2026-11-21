//! Composition for the native PMS cutover-import command HTTP route.
//!
//! Account session, current IAM/assignment authority and the serializable
//! Holding unit are resolved here; the owner crate owns the SQL and the command
//! planner. No SQL and no PMS business rules live in this module.
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
use erpx_server_pms::public::pms_import::{
    PgPmsImportCommand, PmsImportActor, PmsImportCommandKind, PmsImportHttpBackend,
    PmsImportHttpFuture, PmsImportHttpResponse, PmsImportHttpTarget, applied, approve, balance,
    document, preview, property,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, FailureKind, HoldingId, TimestampMillis,
};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PMS_IMPORT_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_IMPORT_UNAVAILABLE",
        "The native PMS import service is unavailable.",
    )
}

fn context_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_CONTEXT_CHANGED",
        "Your selected work context changed. Review the current workspace and role.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct PmsImportSources;

impl HoldingWorkSourcePreparation for PmsImportSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct PmsImportConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl PmsImportConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }
}

pub struct PmsImportService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: PmsImportConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> PmsImportService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: PmsImportConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }

    pub fn company(&self) -> &CompanyId {
        &self.configuration.company
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

    pub async fn command<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        context_user_id: &str,
        context_assignment_id: &str,
        key: String,
        kind: PmsImportCommandKind,
        facts: Value,
    ) -> Result<PmsImportHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let context_user_id = context_user_id.to_owned();
        let context_assignment_id = context_assignment_id.to_owned();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                PmsImportSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let context_user_id = context_user_id.clone();
                    let context_assignment_id = context_assignment_id.clone();
                    let key = key.clone();
                    let facts = facts.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("pms.read")?;
                        // The router's context headers are selectors, never authority.
                        // Bind them to the fresh session/assignment resolved above.
                        let selected = current.selection();
                        if context_user_id != current.user_id().as_str()
                            || context_assignment_id != selected.assignment().id.as_str()
                        {
                            return Err(context_changed().into());
                        }
                        let now = clock.now();
                        let actor = PmsImportActor::new(
                            current.user_id().as_str(),
                            selected.assignment().id.as_str(),
                            selected.workspace().company_id.as_str(),
                            &serde_json::json!({
                                "userId": current.user_id().as_str(),
                                "holdingId": current.holding_id().as_str(),
                                "companyId": selected.workspace().company_id.as_str(),
                                "assignmentId": selected.assignment().id.as_str(),
                                "revision": selected.revision(),
                            })
                            .to_string(),
                        );
                        let mut repository = PgPmsImportCommand::new(connection, &actor);
                        let data = match kind {
                            PmsImportCommandKind::Preview => {
                                preview(&actor, &facts, &key, &mut repository).await?
                            }
                            PmsImportCommandKind::Approve => {
                                approve(&actor, &facts, &key, &mut repository).await?
                            }
                            PmsImportCommandKind::Balance => {
                                balance(&actor, &facts, &key, &mut repository).await?
                            }
                            PmsImportCommandKind::Document => {
                                document(&actor, &facts, &key, &mut repository).await?
                            }
                            PmsImportCommandKind::Property => {
                                property(&actor, &facts, &key, &mut repository).await?
                            }
                            PmsImportCommandKind::Applied => {
                                applied(&actor, &facts, &key, &mut repository).await?
                            }
                        };
                        drop(repository);
                        let _ = now;
                        Ok(PmsImportHttpResponse {
                            data,
                            context: serde_json::json!({
                                "userId": current.user_id().as_str(),
                                "holdingId": current.holding_id().as_str(),
                                "companyId": selected.workspace().company_id.as_str(),
                            }),
                        })
                    })
                },
            )
            .await
    }
}

pub struct PmsImportBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<PmsImportService<C>>,
}

impl<C: Clock + Send + Sync + 'static> PmsImportBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<PmsImportService<C>>,
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
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ));
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

impl<C: Clock + Send + Sync + 'static> PmsImportHttpBackend for PmsImportBackend<C> {
    fn command<'a>(
        &'a self,
        request: erpx_server_pms::public::pms_import::PmsImportHttpCommandRequest<'a>,
    ) -> PmsImportHttpFuture<'a, PmsImportHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .command(
                    request.host,
                    proof,
                    move || Ok(selection),
                    &request.context.user_id,
                    &request.context.assignment_id,
                    request.key.as_str().to_owned(),
                    request.kind,
                    request.facts,
                )
                .await
                .map_err(business)
        })
    }
}

/// Build the owner router bound to a backend; the lead mounts the result.
pub fn pms_import_router(
    host: impl Into<String>,
    holding_slug: impl Into<String>,
    origin: impl Into<String>,
    backend: Arc<dyn PmsImportHttpBackend>,
) -> Result<erpx_server_runtime::public::http_support::Router, BusinessError> {
    erpx_server_pms::public::pms_import::pms_import_router(vec![PmsImportHttpTarget {
        host: host.into(),
        holding_slug: holding_slug.into(),
        origin: origin.into(),
        backend,
    }])
}
