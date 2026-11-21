//! Native Work Projection work-card preference HTTP assembly.
//!
//! The composition supplies the current Account/Holding proof, selected work
//! role, card availability and one serializable PostgreSQL unit. The owner
//! adapter still validates every fact and owns the Workflow receipt.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::{parse_inbound_account_request, unique_request_header},
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{ExpectedWorkSelection, HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CompanyId, FailureKind, HoldingId, IdentifierSource,
    RecordId,
};
use erpx_server_work_projection::public::work_card_availability_source::{
    WorkCardAvailabilityRequest, read_available_work_cards,
};
use erpx_server_work_projection::public::work_card_preferences::{
    execute_work_card_preferences, read_work_card_preferences,
};
use erpx_server_work_projection::public::work_card_preferences_http::{
    WorkCardPreferencesHttpBackend, WorkCardPreferencesHttpFuture, WorkCardPreferencesHttpResponse,
    WorkCardPreferencesReadHttpRequest, WorkCardPreferencesSaveHttpRequest,
};
use erpx_server_workflow::public::approval_oversight::ApprovalOversightActor;
use erpx_server_workflow::public::commands::{
    CommandActor, CommandDisposition, CommandJson, CommandOutcome, PgCommandAuthority,
    WorkflowRequest,
};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "WORK_CARD_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_CONTEXT_CHANGED",
        "The current work context changed.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "WORK_CARD_PREFERENCES_UNAVAILABLE",
        "The work-card preference is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct WorkCardSources;

impl HoldingWorkSourcePreparation for WorkCardSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

fn access() -> WorkAuthorizationRequest {
    WorkAuthorizationRequest {
        permission: "self.tasks".into(),
        module: None,
        destination: "work/card-preferences".into(),
        api: true,
    }
}

fn context(current: &AuthorizedWorkContext) -> Value {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    json!({
        "userId": current.user_id().as_str(),
        "holdingId": current.holding_id().as_str(),
        "companyId": workspace.company_id.as_str(),
        "branchId": workspace.branch_id.as_ref().map(|value| value.as_str()),
        "departmentId": workspace.department_id.as_ref().map(|value| value.as_str()),
        "assignmentId": assignment.id.as_str(),
        "assignmentVersion": assignment.version,
        "revision": selected.revision(),
        "role": assignment.role,
        "scopeKind": match assignment.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        },
        "permissions": current.command_iam_metadata().permissions,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveWire<'a> {
    pinned_ids: &'a Option<Vec<String>>,
    expected_version: i64,
}

struct Authority<'a, C> {
    current: &'a AuthorizedWorkContext,
    expected: WorkflowRequest,
    clock: &'a C,
}

impl<C: Clock + Sync> Authority<'_, C> {
    fn bind(&self, request: &WorkflowRequest) -> Result<CommandActor, BusinessError> {
        if request.key != self.expected.key
            || request.operation != self.expected.operation
            || request.record_id != self.expected.record_id
            || request.input.as_str() != self.expected.input.as_str()
        {
            return Err(changed());
        }
        if self.clock.now().get() >= self.current.expires_at() {
            return Err(BusinessError::new(
                FailureKind::Unauthenticated,
                "WORK_AUTHORITY_EXPIRED",
                "Your work authority expired. Review your current access.",
            ));
        }
        crate::maintenance_parts::command_actor(self.current)
    }
}

impl<C: Clock + Sync> PgCommandAuthority for Authority<'_, C> {
    async fn lock_sources(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Ok(())
    }

    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        request: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        Ok(self.bind(request)?)
    }

    async fn verify_command(
        &mut self,
        _: &mut PgConnection,
        actor: &CommandActor,
        request: &WorkflowRequest,
        _: CommandDisposition,
    ) -> Result<(), Error> {
        if &self.bind(request)? != actor {
            return Err(changed().into());
        }
        Ok(())
    }
}

struct Ids;

impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("generated UUID")
    }
}

pub struct WorkCardPreferencesService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    holding: HoldingId,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> WorkCardPreferencesService<C> {
    pub fn new(authorizer: Arc<HoldingWorkAuthorizer>, holding: HoldingId, clock: Arc<C>) -> Self {
        Self {
            authorizer,
            holding,
            clock,
        }
    }

    pub async fn read<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
    ) -> Result<WorkCardPreferencesHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.holding.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                access(),
                WorkCardSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding {
                            return Err(scope_required().into());
                        }
                        let actor = crate::maintenance_parts::command_actor(current)?;
                        let preferences =
                            read_work_card_preferences(connection, &actor, clock.now().get())
                                .await?;
                        Ok(WorkCardPreferencesHttpResponse {
                            data: serde_json::to_value(preferences).map_err(|_| unavailable())?,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn save<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        key: &str,
        pins: &Option<Vec<String>>,
        expected_version: i64,
    ) -> Result<WorkCardPreferencesHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.holding.clone();
        let clock = self.clock.clone();
        let key = key.to_owned();
        let pins = pins.clone();
        let expected_version = expected_version;
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                access(),
                WorkCardSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    let pins = pins.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding {
                            return Err(scope_required().into());
                        }
                        let now = clock.now().get();
                        let company = current.selection().workspace().company_id.clone();
                        let request = WorkflowRequest {
                            key,
                            operation: "work.cards.preferences".into(),
                            record_id: format!(
                                "{}:{}",
                                company.as_str(),
                                current.user_id().as_str()
                            ),
                            input: CommandJson::from_serializable(&SaveWire {
                                pinned_ids: &pins,
                                expected_version,
                            })?,
                        };
                        let approval_actor = ApprovalOversightActor {
                            user_id: current.user_id().clone(),
                            holding_id: current.holding_id().clone(),
                            company_id: company.clone(),
                            branch_id: current.selection().workspace().branch_id.clone(),
                            department_id: current.selection().workspace().department_id.clone(),
                            assignment_id: current.selection().assignment().id.clone(),
                            scope_kind: match current.selection().assignment().scope_kind {
                                WorkScopeKind::Holding => "holding",
                                WorkScopeKind::Branch => "branch",
                                WorkScopeKind::Department => "department",
                            }
                            .into(),
                            permissions: current.command_iam_metadata().permissions.clone(),
                        };
                        let permissions = current.command_iam_metadata().permissions.clone();
                        let modules = current.modules().to_vec();
                        let availability = WorkCardAvailabilityRequest {
                            approval_actor: &approval_actor,
                            branch_id: current
                                .selection()
                                .workspace()
                                .branch_id
                                .as_ref()
                                .map(|value| value.as_str()),
                            department_id: current
                                .selection()
                                .workspace()
                                .department_id
                                .as_ref()
                                .map(|value| value.as_str()),
                            beta: false,
                            permissions: &permissions,
                            modules: &modules,
                            inventory: erpx_server_work_projection::public::work_capability_probes::InventoryPreparation {
                                can_prepare_counts: current.has_permission("inventory.prepare"),
                                can_prepare_transfers: current.has_permission("inventory.prepare")
                                    && current.selection().workspace().department_id.is_none(),
                            },
                            // Development has package enforcement disabled; a later
                            // Commercial read must replace this with the exact
                            // `people.expenses` new-work decision.
                            expense_available: true,
                        };
                        let available: Vec<String> = read_available_work_cards(
                            connection,
                            &availability,
                            now,
                        )
                        .await?
                        .into_iter()
                        .collect();
                        let mut authority = Authority {
                            current,
                            expected: WorkflowRequest {
                                key: request.key.clone(),
                                operation: request.operation.clone(),
                                record_id: request.record_id.clone(),
                                input: request.input.clone(),
                            },
                            clock: clock.as_ref(),
                        };
                        let mut identifiers = Ids;
                        let outcome: CommandOutcome = execute_work_card_preferences(
                            company,
                            connection,
                            &mut authority,
                            clock.as_ref(),
                            &mut identifiers,
                            &request,
                            &available,
                        )
                        .await?;
                        let data: Value = serde_json::from_str(outcome.result.as_str())
                            .map_err(|_| unavailable())?;
                        Ok(WorkCardPreferencesHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct WorkCardPreferencesBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<WorkCardPreferencesService<C>>,
}

impl<C: Clock + Send + Sync + 'static> WorkCardPreferencesBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<WorkCardPreferencesService<C>>,
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
        mutation: bool,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        let company = unique_request_header(headers, "x-erpx-requested-company")?
            .or(unique_request_header(headers, "x-erpx-context-company")?)
            .ok_or_else(scope_required)?;
        let expected = match (
            unique_request_header(headers, "x-erpx-context-assignment")?,
            unique_request_header(headers, "x-erpx-context-revision")?,
            unique_request_header(headers, "x-erpx-context-assignment-version")?,
        ) {
            (Some(assignment), Some(revision), Some(version)) => Some(ExpectedWorkSelection {
                assignment_id: AssignmentId::new(assignment)?,
                revision: revision.parse().map_err(|_| scope_required())?,
                assignment_version: version.parse().map_err(|_| scope_required())?,
            }),
            (None, None, None) => None,
            _ => return Err(scope_required()),
        };
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: CompanyId::new(company)?,
                branch_slug: branch,
                department_slug: department,
                expected,
                mutation,
            },
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> WorkCardPreferencesHttpBackend
    for WorkCardPreferencesBackend<C>
{
    fn read<'a>(
        &'a self,
        request: WorkCardPreferencesReadHttpRequest<'a>,
    ) -> WorkCardPreferencesHttpFuture<'a, WorkCardPreferencesHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers, false)?;
            self.service
                .read(request.host, proof, move || Ok(selection))
                .await
                .map_err(business)
        })
    }

    fn save<'a>(
        &'a self,
        request: WorkCardPreferencesSaveHttpRequest<'a>,
    ) -> WorkCardPreferencesHttpFuture<'a, WorkCardPreferencesHttpResponse> {
        Box::pin(async move {
            let key = unique_request_header(request.headers, "idempotency-key")?
                .ok_or_else(|| {
                    BusinessError::invalid(
                        "COMMAND_KEY_REQUIRED",
                        "Use a valid Idempotency-Key for this action.",
                    )
                })?
                .to_owned();
            let (proof, selection) = self.context(request.headers, true)?;
            self.service
                .save(
                    request.host,
                    proof,
                    move || Ok(selection),
                    &key,
                    &request.input.pinned_ids,
                    request.input.expected_version,
                )
                .await
                .map_err(business)
        })
    }
}
