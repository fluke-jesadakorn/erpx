//! Explicit registered-Holding ingress for the bounded Finance journal batch-post owner port.
//! The owner's domain/application/adapters remain authoritative; this module only
//! assembles current Identity/Commercial/Workflow facts and mounts one POST route.
use crate::{
    business_http::BusinessScopeSlugs,
    finance_journal_review::{FinanceJournalConfiguration, command_actor, finance_actor},
};
use axum::{
    Router,
    body::to_bytes,
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use erpx_commercial_server::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness,
};
use erpx_finance_server::public::{
    FinanceActor,
    journal_batch::{
        JournalBatchResult, PgJournalBatch, PgJournalBatchPolicy, post_journal_batch,
    },
    journal_batch_wire::{
        JournalBatchWireFailure, NormalizedJournalBatchCommand, normalize_journal_batch,
    },
    journal_review::{
        CurrentJournalReviewScope, FinanceJournalAccessPurpose, JournalReviewCommand,
        JournalReviewerFacts, JournalReviewRootKind, PgFinanceJournalReviewPolicy, ReviewJournal,
        inspect_journal_review_source,
    },
};
use erpx_identity_access_server::public::{
    DeviceProof, RequestAudience,
    account_holding_bindings::ConfiguredHoldingRegistry,
    account_inbound_http::{
        CapturedHoldingMutation, parse_inbound_account_request, unique_request_header,
    },
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    journal_flow_participants::{
        current_journal_flow_principal, eligible_journal_flow_participant,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_organization_server::public::journal_flow_department::journal_flow_team_name;
use erpx_people_payroll_server::public::journal_flow_people::current_journal_flow_people;
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, IdentifierSource, RecordId,
    TimestampMillis, UserId,
};
use erpx_workflow_server::public::{
    commands::{CommandActor, CommandJson},
    journal_post_flow::{
        JournalFlowCandidate, JournalPostFlowRequest, JournalPostFlowWitness,
        read_journal_flow_basis,
    },
};
use sqlx::PgConnection;
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::Mutex;

type Error = PgTransactionError<BusinessError>;
const BODY_LIMIT: usize = 2 * 1024 * 1024;

fn denied() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FINANCE_CURRENT_AUTHORITY_REQUIRED",
        "Choose the current authorized Finance workspace.",
    )
}
fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn business_error(error: BusinessError) -> Error {
    PgTransactionError::Business(error)
}
fn finance_error(error: PgTransactionError<erpx_finance_server::public::JournalError>) -> Error {
    match error {
        PgTransactionError::Persistence(e) => PgTransactionError::Persistence(e),
        PgTransactionError::Business(e) => BusinessError::new(
            e.kind,
            e.code,
            "The current Finance journal batch could not be completed.",
        )
        .into(),
    }
}

struct BatchIds;
impl IdentifierSource for BatchIds {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("UUID is a bounded record identifier")
    }
}

/// Workflow's retained reviewer route for one admitted child post. It uses only
/// public owner ports and is revalidated after all deferred owner effects.
struct PostFlow {
    witness: JournalPostFlowWitness,
    company: CompanyId,
    creator: String,
    people: Vec<JournalFlowCandidate>,
    preferred: Option<String>,
    mapped: Option<String>,
}
impl PostFlow {
    async fn prepare(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        journal: &ReviewJournal,
        command: &JournalReviewCommand,
        now: i64,
    ) -> Result<Self, Error> {
        if !matches!(journal.source_type.as_str(), "manual" | "opening_import") {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "JOURNAL_FLOW_SOURCE_UNSUPPORTED",
                "This journal source is not available for native batch posting.",
            )
            .into());
        }
        let company = current.selection().workspace().company_id.clone();
        let mut people = Vec::new();
        for person in current_journal_flow_people(c, &company).await? {
            if eligible_journal_flow_participant(c, &company, &person.user_id, &journal.created_by)
                .await?
            {
                people.push(JournalFlowCandidate {
                    user_id: person.user_id,
                    name: person.name,
                    team_name: journal_flow_team_name(
                        c,
                        &company,
                        person.department_id.as_deref(),
                    )
                    .await?,
                });
            }
        }
        let basis = read_journal_flow_basis(c, &company, &journal.id).await?;
        let preferred = basis.preferred_primary()?;
        let mapped = match &preferred {
            Some(id) => Some(current_journal_flow_principal(c, &company, id).await?),
            None => None,
        };
        let actor = current.user_id().as_str();
        let name = people
            .iter()
            .find(|p| p.user_id == actor)
            .map(|p| p.name.as_str())
            .unwrap_or(actor);
        let permitted = current.has_permission("workflow.finance_approver")
            || current.has_permission("workflow.holding_owner");
        let event = uuid::Uuid::new_v4().to_string();
        let witness = JournalPostFlowWitness::prepare(
            basis,
            JournalPostFlowRequest {
                candidates: &people,
                mapped_preferred: mapped.as_deref(),
                actor,
                actor_name: name,
                actor_can_participate: permitted,
                command: command.key.as_str(),
                event: &event,
                version: journal.version + 1,
                now,
            },
        )?;
        Ok(Self {
            witness,
            company,
            creator: journal.created_by.clone(),
            people,
            preferred,
            mapped,
        })
    }
    async fn retain(&self, c: &mut PgConnection) -> Result<(), Error> {
        let mut people = Vec::new();
        for person in current_journal_flow_people(c, &self.company).await? {
            if eligible_journal_flow_participant(c, &self.company, &person.user_id, &self.creator)
                .await?
            {
                people.push(JournalFlowCandidate {
                    user_id: person.user_id,
                    name: person.name,
                    team_name: journal_flow_team_name(
                        c,
                        &self.company,
                        person.department_id.as_deref(),
                    )
                    .await?,
                });
            }
        }
        if people != self.people {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "FLOW_PRIMARY_REQUIRED",
                "The current journal reviewer route changed.",
            )
            .into());
        }
        let mapped = match &self.preferred {
            Some(id) => Some(current_journal_flow_principal(c, &self.company, id).await?),
            None => None,
        };
        if mapped != self.mapped {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "FLOW_PRIMARY_REQUIRED",
                "The current journal reviewer route changed.",
            )
            .into());
        }
        self.witness.retain(c).await
    }
}

/// Current-authority policy for the owner batch port and each of its delegated
/// journal-review child posts. No capability is inferred from request data.
struct BatchPolicy<'a> {
    current: &'a AuthorizedWorkContext,
    configuration: &'a FinanceJournalConfiguration,
    catalogue: &'a CommercialCatalogue,
    sources: BTreeMap<String, CurrentJournalReviewScope>,
    purchase: Option<(CapabilityPurpose, PurchaseWitness)>,
    flow: Option<PostFlow>,
}
impl BatchPolicy<'_> {
    fn actor(&self, actor: &FinanceActor, now: i64) -> Result<CommandActor, Error> {
        if actor != &finance_actor(self.current) || now >= self.current.expires_at() {
            return Err(denied().into());
        }
        self.current.require_permission("finance.read")?;
        self.current.require_permission("finance.write")?;
        self.current.require_permission("module.finance.read")?;
        Ok(command_actor(self.current)?)
    }
    fn selected(&self, actor: &CommandActor, now: i64) -> Result<(), Error> {
        if actor != &self.actor(&finance_actor(self.current), now)? {
            return Err(denied().into());
        }
        Ok(())
    }
}
impl PgFinanceJournalReviewPolicy for BatchPolicy<'_> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        actor: &FinanceActor,
        _: &JournalReviewCommand,
    ) -> Result<(), PgTransactionError<erpx_platform_shared_server::public::BusinessError>> {
        if actor != &finance_actor(self.current) {
            return Err(denied().into());
        }
        self.configuration.prepare(c).await?;
        Ok(())
    }
    async fn authorize_current(
        &mut self,
        _: &mut PgConnection,
        actor: &FinanceActor,
        _: &JournalReviewCommand,
        now: i64,
    ) -> Result<CommandActor, PgTransactionError<erpx_platform_shared_server::public::BusinessError>> {
        Ok(self.actor(actor, now)?)
    }
    async fn reviewer(
        &mut self,
        _: &mut PgConnection,
        actor: &CommandActor,
        journal: Option<&ReviewJournal>,
        now: i64,
    ) -> Result<JournalReviewerFacts, PgTransactionError<erpx_platform_shared_server::public::BusinessError>>
    {
        self.selected(actor, now)?;
        Ok(JournalReviewerFacts {
            can_read: self.current.has_permission("finance.read"),
            can_approve: self.current.has_permission("finance.approve"),
            same_person: journal
                .map(|j| UserId::new(j.created_by.clone()))
                .transpose()?
                .as_ref()
                .is_some_and(|creator| self.current.same_person_user(creator)),
        })
    }
    async fn ready(
        &mut self,
        _: &mut PgConnection,
        actor: &CommandActor,
        now: i64,
    ) -> Result<(), PgTransactionError<erpx_platform_shared_server::public::BusinessError>> {
        self.selected(actor, now)
    }
    async fn prepare_post_flow(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        journal: &ReviewJournal,
        command: &JournalReviewCommand,
        now: i64,
    ) -> Result<(), PgTransactionError<erpx_platform_shared_server::public::BusinessError>> {
        self.selected(actor, now)?;
        // Child posts complete in order, so the previous child's retained flow
        // has already been consumed before the next one is prepared.
        self.flow = Some(PostFlow::prepare(c, self.current, journal, command, now).await?);
        Ok(())
    }
    async fn apply_post_flow_before(
        &mut self,
        c: &mut PgConnection,
    ) -> Result<(), PgTransactionError<erpx_platform_shared_server::public::BusinessError>> {
        self.flow.as_ref().ok_or_else(|| denied())?.witness.apply_before(c).await
    }
    async fn apply_post_flow_after(
        &mut self,
        c: &mut PgConnection,
    ) -> Result<(), PgTransactionError<erpx_platform_shared_server::public::BusinessError>> {
        self.flow.as_ref().ok_or_else(|| denied())?.witness.apply_after(c).await
    }
    async fn retain_post_flow(
        &mut self,
        c: &mut PgConnection,
    ) -> Result<(), PgTransactionError<erpx_platform_shared_server::public::BusinessError>> {
        self.flow.as_ref().ok_or_else(|| denied())?.retain(c).await
    }
    async fn require_source_access(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        command: &JournalReviewCommand,
        purpose: FinanceJournalAccessPurpose,
        now: i64,
    ) -> Result<(), PgTransactionError<erpx_platform_shared_server::public::BusinessError>> {
        self.selected(actor, now)?;
        let key = command.key.as_str();
        if !self.sources.contains_key(key) {
            self.sources.insert(
                key.to_owned(),
                inspect_journal_review_source(c, &finance_actor(self.current), command).await?,
            );
        }
        let source = self.sources.get(key).ok_or_else(|| denied())?;
        if source.journal_id() != source.root_journal_id()
            || !matches!(
                source.root_kind(),
                JournalReviewRootKind::Manual | JournalReviewRootKind::OpeningImport
            )
        {
            return Err(denied().into());
        }
        source.require_current(c).await?;
        let purpose = match purpose {
            FinanceJournalAccessPurpose::BaselineRead => return Ok(()),
            FinanceJournalAccessPurpose::ExactReplayHistory => CapabilityPurpose::History,
            FinanceJournalAccessPurpose::SettleExisting => CapabilityPurpose::SettleExisting,
        };
        let company = self.current.selection().workspace().company_id.clone();
        let mut reader = PgCompanyPurchaseReader::new(
            self.current.holding_id().clone(),
            company,
            c,
            self.catalogue,
        );
        if let Some((original, witness)) = &self.purchase {
            if original != &purpose {
                return Err(denied().into());
            }
            reader
                .revalidate(witness, TimestampMillis::new(now))
                .await?
                .require_allowed()?;
        } else {
            let inspection = reader
                .inspect(
                    &PurchaseRequest {
                        capabilities: vec!["finance.accounting".into()],
                        purpose,
                        mode: PackageEnforcementMode::Enforced,
                    },
                    TimestampMillis::new(now),
                )
                .await?;
            inspection.require_allowed()?;
            self.purchase = Some((purpose, inspection.witness));
        }
        Ok(())
    }
}
impl PgJournalBatchPolicy for BatchPolicy<'_> {}

/// Owner-neutral composition service. It forwards an already authenticated
/// request into the owner port; every business decision stays in the owner.
pub struct FinanceJournalBatchService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: FinanceJournalConfiguration,
    catalogue: Arc<CommercialCatalogue>,
    company: CompanyId,
    clock: Arc<C>,
}
impl<C: Clock + Send + Sync + 'static> FinanceJournalBatchService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: FinanceJournalConfiguration,
        catalogue: Arc<CommercialCatalogue>,
        company: CompanyId,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            catalogue,
            company,
            clock,
        }
    }
    pub async fn batch_transport<Input, Prepare>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        prepare: Prepare,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Prepare: Fn() -> Result<NormalizedJournalBatchCommand, BusinessError>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        let company = self.company.clone();
        let workspace = move || {
            let mut selection = workspace()?;
            if selection.company_id != company {
                return Err(denied());
            }
            selection.mutation = true;
            Ok(selection)
        };
        let configuration = self.configuration.clone();
        let catalogue = self.catalogue.clone();
        let company = self.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                workspace,
                WorkAuthorizationRequest {
                    permission: "finance.read".into(),
                    module: Some("finance".into()),
                    destination: "finance/journals".into(),
                    api: true,
                },
                configuration.clone(),
                move |c, current| {
                    let configuration = configuration.clone();
                    let catalogue = catalogue.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let prepare = prepare.clone();
                    Box::pin(async move {
                        let normalized = prepare().map_err(business_error)?;
                        let command = normalized.command().clone();
                        let actor = finance_actor(current);
                        let policy = Mutex::new(BatchPolicy {
                            current,
                            configuration: &configuration,
                            catalogue: &catalogue,
                            sources: BTreeMap::new(),
                            purchase: None,
                            flow: None,
                        });
                        let mut ids = BatchIds;
                        let mut ports = PgJournalBatch::new(
                            current.holding_id().clone(),
                            company,
                            c,
                            &policy,
                            clock.as_ref(),
                            &mut ids,
                            &normalized,
                        )
                        .map_err(finance_error)?;
                        let result: JournalBatchResult =
                            post_journal_batch(&mut ports, &actor, &command)
                                .await
                                .map_err(finance_error)?;
                        CommandJson::from_serializable(&result).map_err(business_error)
                    })
                },
            )
            .await
    }
}

/// Explicitly configured, distinct registered-Holding batch target.
pub struct FinanceJournalBatchTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub service: Arc<FinanceJournalBatchService<C>>,
}
struct BatchState<C> {
    policy: Arc<erpx_identity_access_server::public::AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String), FinanceJournalBatchTarget<C>>,
}

/// One POST route. Configuration is validated against the actual audience and
/// Holding registry before any router exists; an absent target cannot fall back.
pub fn finance_journal_batch_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<erpx_identity_access_server::public::AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<FinanceJournalBatchTarget<C>>,
) -> Result<Router, BusinessError> {
    policy.validate()?;
    let mut configured = BTreeMap::new();
    for target in targets {
        let valid = policy
            .audience(&target.host)
            .and_then(|a| registry.expected_for_audience(&a).ok())
            .is_some_and(|h| h.slug == target.holding_slug);
        if !valid
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "FINANCE_BATCH_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/finance/journals/batch-post",
            post(batch_post::<C>),
        )
        .with_state(Arc::new(BatchState {
            policy,
            scopes,
            targets: configured,
        })))
}

async fn batch_post<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<BatchState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = state
            .targets
            .get(&(host.clone(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, request.headers())?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(request.headers(), "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let scope = state.scopes.slugs(request.headers());
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(request.headers(), branch, department);
        let mut keys = axum::http::HeaderMap::new();
        for value in request.headers().get_all("idempotency-key") {
            keys.append("idempotency-key", value.clone());
        }
        let proof = inbound.into_proof();
        let body = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map(Arc::new)
            .map_err(|_| BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller request."));
        target
            .service
            .batch_transport(
                &host,
                proof,
                move || {
                    scope?;
                    captured.resolve()
                },
                move || {
                    let bytes = body.as_ref().map_err(Clone::clone)?;
                    let wire = normalize_journal_batch(bytes)
                        .map_err(|f: JournalBatchWireFailure| f.error)?;
                    let key = unique_request_header(&keys, "idempotency-key")?
                        .filter(|v| !v.is_empty())
                        .ok_or_else(|| {
                            BusinessError::invalid(
                                "IDEMPOTENCY_KEY_REQUIRED",
                                "Idempotency-Key is required.",
                            )
                        })?;
                    wire.into_command(CommandKey::new(key)?)
                },
            )
            .await
    }
    .await;
    let mut response = match result {
        Ok(data) => match serde_json::from_str::<serde_json::Value>(data.as_str()) {
            Ok(value) => (StatusCode::OK, axum::Json(serde_json::json!({"data":value})))
                .into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({"error":{"code":"FINANCE_BATCH_UNAVAILABLE","message":"The batch could not be encoded."}})),
            )
                .into_response(),
        },
        Err(PgTransactionError::Business(e)) => {
            let status = match e.kind {
                FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
                FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
                FailureKind::Forbidden => StatusCode::FORBIDDEN,
                FailureKind::NotFound => StatusCode::NOT_FOUND,
                FailureKind::Conflict => StatusCode::CONFLICT,
                FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            };
            (
                status,
                axum::Json(serde_json::json!({"error":{"code":e.code,"message":e.message}})),
            )
                .into_response()
        }
        Err(PgTransactionError::Persistence(_)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({"error":{"code":"FINANCE_BATCH_UNAVAILABLE","message":"The batch could not be posted."}})),
        )
            .into_response(),
    };
    for (name, value) in [
        (axum::http::header::CACHE_CONTROL, "private, no-store"),
        (axum::http::header::REFERRER_POLICY, "no-referrer"),
        (axum::http::header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, axum::http::HeaderValue::from_static(value));
    }
    response
}
