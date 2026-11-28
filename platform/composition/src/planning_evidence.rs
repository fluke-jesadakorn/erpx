//! Native Planning evidence search composition.
//!
//! The owner port owns metric SQL, cursor policy and row validation. This
//! composition supplies the current Account/Holding actor and a serializable
//! unit; it never reads or writes TypeScript SQL.
use crate::business_http::BusinessScopeSlugs;
use erpx_identity_access_server::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::{holding_mutation_request, parse_inbound_account_request},
    effective_authority::{HoldingWorkAuthorizer, HoldingWorkSourcePreparation, WorkAuthorizationRequest},
    work_context::HoldingWorkspaceRequest,
};
use erpx_planning_server::public::evidence::{
    EvidenceActor, EvidenceEligibility, EvidenceHttpBackend, EvidenceHttpFuture,
    EvidenceHttpRequest, EvidenceHttpResponse, EvidenceHttpTarget, EvidenceQuery, PgEvidenceRead,
    planning_evidence_router, search_evidence,
};
use erpx_platform_runtime_server::public::http_support::{HeaderMap, Router};
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, Clock, FailureKind, HoldingId};
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_PLANNING_EVIDENCE_UNAVAILABLE",
        "The native Planning evidence service is unavailable.",
    )
}
fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PLANNING_EVIDENCE_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}
fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        _ => unavailable(),
    }
}

#[derive(Clone)]
pub struct PlanningEvidenceConfiguration {
    pub holding: HoldingId,
}

#[derive(Clone)]
pub struct PlanningEvidenceSources;

impl HoldingWorkSourcePreparation for PlanningEvidenceSources {
    fn prepare<'c>(
        &'c self,
        _: &'c mut PgConnection,
    ) -> erpx_platform_runtime_server::public::postgres::PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

pub struct PlanningEvidenceService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: PlanningEvidenceConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> PlanningEvidenceService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: PlanningEvidenceConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }

    fn access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "records.read".into(),
            module: Some("planning".into()),
            destination: "executive/planning".into(),
            api: true,
        }
    }

    pub async fn search<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        query: EvidenceQuery,
    ) -> Result<EvidenceHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    let mut request = input()?;
                    request.mutation = false;
                    Ok(request)
                },
                Self::access(),
                PlanningEvidenceSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let query = query.clone();
                    let _now = clock.now();
                    Box::pin(async move {
                        if current.holding_id() != &holding {
                            return Err(scope().into());
                        }
                        // Enforced package mode needs record-scope eligibility;
                        // fail closed until that witness is mounted rather than
                        // exposing unpurchased scenarios.
                        if std::env::var("ERPX_APPS_PACKAGE_ENFORCEMENT")
                            .map(|value| value == "enforced")
                            .unwrap_or(false)
                        {
                            return Err(PgTransactionError::Business(BusinessError::new(
                                FailureKind::Unavailable,
                                "PLANNING_EVIDENCE_ELIGIBILITY_REQUIRED",
                                "The Planning evidence eligibility witness is not mounted.",
                            )));
                        }
                        let assignment = current.selection().assignment();
                        let workspace = current.selection().workspace();
                        let actor = EvidenceActor {
                            user_id: current.user_id().as_str().to_owned(),
                            company_id: workspace.company_id.as_str().to_owned(),
                            branch_id: workspace.branch_id.as_ref().map(|value| value.as_str().to_owned()),
                            department_id: workspace.department_id.as_ref().map(|value| value.as_str().to_owned()),
                            assignment_id: assignment.id.as_str().to_owned(),
                            role: assignment.role.clone(),
                            modules: current.modules().to_vec(),
                            permissions: current.command_iam_metadata().permissions.clone(),
                        };
                        let rows = search_evidence(
                            &actor,
                            &EvidenceEligibility::All,
                            &query,
                            &mut PgEvidenceRead::new(connection),
                        )
                        .await?;
                        Ok(EvidenceHttpResponse {
                            data: serde_json::json!({ "rows": rows }),
                        })
                    })
                },
            )
            .await
    }
}

pub struct PlanningEvidenceBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<PlanningEvidenceService<C>>,
}

impl<C> PlanningEvidenceBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<PlanningEvidenceService<C>>,
    ) -> Self {
        Self {
            policy,
            scopes,
            service,
        }
    }

    fn read_context(
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
            holding_mutation_request(headers, branch, department)?,
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> EvidenceHttpBackend for PlanningEvidenceBackend<C> {
    fn search<'a>(
        &'a self,
        request: EvidenceHttpRequest<'a>,
    ) -> EvidenceHttpFuture<'a, EvidenceHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.read_context(request.headers)?;
            self.service
                .search(request.host, proof, move || Ok(selection), request.query.clone())
                .await
                .map_err(business)
        })
    }
}

pub fn planning_evidence_native_router(
    host: impl Into<String>,
    holding_slug: impl Into<String>,
    origin: impl Into<String>,
    backend: Arc<dyn EvidenceHttpBackend>,
) -> Result<Router, BusinessError> {
    planning_evidence_router(vec![EvidenceHttpTarget {
        host: host.into(),
        holding_slug: holding_slug.into(),
        origin: origin.into(),
        backend,
    }])
}
