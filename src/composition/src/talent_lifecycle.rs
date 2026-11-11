//! Native Talent lifecycle composition.
//!
//! Account session, current IAM authority and the serializable Holding unit are
//! resolved here; candidate/application/proposal/offer rules stay in the Talent
//! owner. Offers without a Finance money-snapshot producer fail closed.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use erpx_server_talent::public::candidate::{
    CandidateRequest, PgTalentCandidates, PrivacyRequest, TalentActor, candidate_privacy_workspace,
    change_candidate_privacy, create_candidate,
};
use erpx_server_talent::public::offer::{
    OfferChange, OfferRequest, PgTalentOffers, change_offer, create_offer, hire_handoff_queue,
    hire_handoff_source,
};
use erpx_server_talent::public::talent_application::{
    ApplicationChange, ApplicationRequest, PgTalentApplications, ProposalChange, ProposalRequest,
    change_application, change_proposal, create_application, create_proposal,
};
use erpx_server_talent::public::talent_http::{
    TalentLifecycleHttpBackend, TalentLifecycleHttpCommand, TalentLifecycleHttpFuture,
    TalentLifecycleHttpRequest, TalentLifecycleHttpResponse,
};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "TALENT_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "TALENT_UNAVAILABLE",
        "The Talent service is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

fn parse<T: serde::de::DeserializeOwned>(body: &Value) -> Result<T, BusinessError> {
    serde_json::from_value(body.clone()).map_err(|_| {
        BusinessError::invalid("TALENT_INPUT_INVALID", "Review the Talent input.")
    })
}

#[derive(Clone, Copy)]
struct TalentSources;

impl HoldingWorkSourcePreparation for TalentSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct TalentConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl TalentConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn talent_actor(current: &AuthorizedWorkContext) -> Result<TalentActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(TalentActor {
        user_id: current.user_id().clone(),
        holding_id: current.holding_id().clone(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        assignment_id: assignment.id.clone(),
        role: Some(assignment.role.clone()),
        authority_revision: Some(selected.revision()),
        assignment_version: Some(assignment.version),
        permissions: current.command_iam_metadata().permissions.clone(),
        expires_at: current.expires_at(),
    })
}

async fn today(connection: &mut PgConnection) -> Result<String, Error> {
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')",
    )
    .fetch_one(&mut *connection)
    .await?)
}

pub struct TalentService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: TalentConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> TalentService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: TalentConfiguration,
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

    pub async fn command(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        command: TalentLifecycleHttpCommand,
    ) -> Result<TalentLifecycleHttpResponse, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        let mutation = !matches!(
            command,
            TalentLifecycleHttpCommand::CandidatePrivacy { .. }
                | TalentLifecycleHttpCommand::HireHandoff { .. }
                | TalentLifecycleHttpCommand::HireHandoffQueue
        );
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    let mut request = selection;
                    if request.company_id != expected {
                        return Err(scope());
                    }
                    request.mutation = mutation;
                    Ok(request)
                },
                WorkAuthorizationRequest {
                    permission: if mutation { "records.write".into() } else { "records.read".into() },
                    module: Some("talent".into()),
                    destination: "talent".into(),
                    api: true,
                },
                TalentSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        let actor = talent_actor(current)?;
                        let now = clock.now().get();
                        let data = match command {
                            TalentLifecycleHttpCommand::CandidateCreate {
                                body,
                                idempotency_key,
                            } => {
                                let input: CandidateRequest = parse(&body)?;
                                let key =
                                    erpx_server_shared::public::CommandKey::new(idempotency_key)?;
                                let date = today(connection).await?;
                                let mut repository = PgTalentCandidates::new(connection);
                                create_candidate(&mut repository, &actor, &key, &input, &date, now)
                                    .await?
                            }
                            TalentLifecycleHttpCommand::CandidatePrivacy { candidate_id } => {
                                let mut repository = PgTalentCandidates::new(connection);
                                candidate_privacy_workspace(
                                    &mut repository,
                                    &actor,
                                    &candidate_id,
                                    now,
                                )
                                .await?
                            }
                            TalentLifecycleHttpCommand::CandidatePrivacyChange {
                                candidate_id,
                                body,
                                idempotency_key,
                            } => {
                                let input: PrivacyRequest = parse(&body)?;
                                let key =
                                    erpx_server_shared::public::CommandKey::new(idempotency_key)?;
                                let mut repository = PgTalentCandidates::new(connection);
                                change_candidate_privacy(
                                    &mut repository,
                                    &actor,
                                    &key,
                                    &candidate_id,
                                    &input,
                                    now,
                                )
                                .await?
                            }
                            TalentLifecycleHttpCommand::ApplicationCreate {
                                body,
                                idempotency_key,
                            } => {
                                let input: ApplicationRequest = parse(&body)?;
                                let key =
                                    erpx_server_shared::public::CommandKey::new(idempotency_key)?;
                                let date = today(connection).await?;
                                let mut repository = PgTalentApplications::new(connection);
                                create_application(
                                    &mut repository,
                                    &actor,
                                    &key,
                                    &input,
                                    &date,
                                    now,
                                )
                                .await?
                            }
                            TalentLifecycleHttpCommand::ApplicationChange {
                                application_id,
                                body,
                                idempotency_key,
                            } => {
                                let input: ApplicationChange = parse(&body)?;
                                let key =
                                    erpx_server_shared::public::CommandKey::new(idempotency_key)?;
                                let date = today(connection).await?;
                                let mut repository = PgTalentApplications::new(connection);
                                change_application(
                                    &mut repository,
                                    &actor,
                                    &key,
                                    &application_id,
                                    &input,
                                    &date,
                                    now,
                                )
                                .await?
                            }
                            TalentLifecycleHttpCommand::ProposalCreate {
                                body,
                                idempotency_key,
                            } => {
                                let input: ProposalRequest = parse(&body)?;
                                let key =
                                    erpx_server_shared::public::CommandKey::new(idempotency_key)?;
                                let date = today(connection).await?;
                                let mut repository = PgTalentApplications::new(connection);
                                create_proposal(&mut repository, &actor, &key, &input, &date, now)
                                    .await?
                            }
                            TalentLifecycleHttpCommand::ProposalChange {
                                proposal_id,
                                body,
                                idempotency_key,
                            } => {
                                let input: ProposalChange = parse(&body)?;
                                let key =
                                    erpx_server_shared::public::CommandKey::new(idempotency_key)?;
                                let date = today(connection).await?;
                                let mut repository = PgTalentApplications::new(connection);
                                change_proposal(
                                    &mut repository,
                                    &actor,
                                    &key,
                                    &proposal_id,
                                    &input,
                                    &date,
                                    now,
                                )
                                .await?
                            }
                            TalentLifecycleHttpCommand::OfferCreate {
                                body,
                                idempotency_key,
                            } => {
                                let input: OfferRequest = parse(&body)?;
                                let key =
                                    erpx_server_shared::public::CommandKey::new(idempotency_key)?;
                                let date = today(connection).await?;
                                let mut repository = PgTalentOffers::new(connection);
                                create_offer(&mut repository, &actor, &key, &input, &date, now)
                                    .await?
                            }
                            TalentLifecycleHttpCommand::OfferChange {
                                offer_id,
                                body,
                                idempotency_key,
                            } => {
                                let input: OfferChange = parse(&body)?;
                                let key =
                                    erpx_server_shared::public::CommandKey::new(idempotency_key)?;
                                let date = today(connection).await?;
                                let mut repository = PgTalentOffers::new(connection);
                                change_offer(
                                    &mut repository,
                                    &actor,
                                    &key,
                                    &offer_id,
                                    &input,
                                    &date,
                                    now,
                                )
                                .await?
                            }
                            TalentLifecycleHttpCommand::HireHandoff { handoff_id } => {
                                let mut repository = PgTalentOffers::new(connection);
                                hire_handoff_source(&mut repository, &actor, &handoff_id, now)
                                    .await?
                            }
                            TalentLifecycleHttpCommand::HireHandoffQueue => {
                                let mut repository = PgTalentOffers::new(connection);
                                hire_handoff_queue(&mut repository, &actor, now).await?
                            }
                        };
                        Ok(TalentLifecycleHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

fn context(current: &AuthorizedWorkContext) -> Value {
    let selected = current.selection();
    let workspace = selected.workspace();
    serde_json::json!({
        "userId": current.user_id().as_str(),
        "holdingId": current.holding_id().as_str(),
        "companyId": workspace.company_id.as_str(),
        "branchId": workspace.branch_id.as_ref().map(|v| v.as_str()),
        "departmentId": workspace.department_id.as_ref().map(|v| v.as_str()),
        "assignmentId": selected.assignment().id.as_str(),
        "role": selected.assignment().role,
        "revision": selected.revision(),
        "assignmentVersion": selected.assignment().version,
        "scopeKind": match selected.assignment().scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        },
    })
}

pub struct TalentBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<TalentService<C>>,
}

impl<C: Clock + Send + Sync + 'static> TalentBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<TalentService<C>>,
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

impl<C: Clock + Send + Sync + 'static> TalentLifecycleHttpBackend for TalentBackend<C> {
    fn execute<'a>(
        &'a self,
        request: TalentLifecycleHttpRequest<'a>,
    ) -> TalentLifecycleHttpFuture<'a, TalentLifecycleHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .command(request.host, proof, selection, request.command)
                .await
                .map_err(business)
        })
    }
}
