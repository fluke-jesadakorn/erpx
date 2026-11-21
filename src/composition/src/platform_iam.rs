//! Native Platform IAM command assembly.
//!
//! Platform Hosts have no Holding slug. The retained Platform Administration
//! session/assignment resolution supplies the current Account session, the
//! selected `manage_erpx.access` work context and the exact
//! `x-erpx-platform-assignment`. Those facts build the Platform IAM actor for
//! the `platform` Company/scope, and the owner role/review handlers run in one
//! serializable Control unit. This module never mints a session, assignment or
//! Platform permission.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    iam_roles::{
        IamRoleActor, PgIamRolesReviews, cancel_access_review_campaign, decide_access_review_item,
        decide_review, save_role, start_access_review,
    },
    iam_roles_http::{
        IamPlatformRolesReviewsHttpRequest, IamRolesReviewsHttpBackend, IamRolesReviewsHttpCommand,
        IamRolesReviewsHttpFuture, IamRolesReviewsHttpRequest, IamRolesReviewsHttpResponse,
    },
    permission_catalog::permission_catalog,
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::http_support::{HeaderMap, HeaderName};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, PgWorkFuture, TransactionOptions,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, RecordId,
    UserId,
};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

/// The retained Platform namespace stores Platform roles and reviews under this
/// literal Company so they are shared across Holdings.
const PLATFORM_COMPANY: &str = "platform";

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PLATFORM_IAM_SCOPE_REQUIRED",
        "Choose the current authorized Platform workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PLATFORM_IAM_UNAVAILABLE",
        "The Platform IAM workspace is unavailable.",
    )
}

fn database_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PLATFORM_DATABASE_UNAVAILABLE",
        "Platform IAM persistence is unavailable.",
    )
}

fn forbidden_platform() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PLATFORM_OPERATIONS_FORBIDDEN",
        "A current Platform assignment is required.",
    )
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "PLATFORM_IAM_REQUEST_INVALID",
        "Use one unambiguous request header value.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct PlatformIamSources;

impl HoldingWorkSourcePreparation for PlatformIamSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct PlatformIamConfiguration {
    holding: HoldingId,
    holding_slug: String,
    company: CompanyId,
    control: DatabasePool,
}

impl PlatformIamConfiguration {
    pub fn new(
        holding: HoldingId,
        holding_slug: String,
        company: CompanyId,
        control: DatabasePool,
    ) -> Self {
        Self {
            holding,
            holding_slug,
            company,
            control,
        }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

/// Owned render/command facts from the verified Work context. The Platform
/// assignment and its published permissions are re-read inside the Control unit.
#[derive(Clone)]
struct PlatformIamFacts {
    user_id: UserId,
    session_id: RecordId,
    holding_id: HoldingId,
    work_company: CompanyId,
    work_assignment: AssignmentId,
    work_assignment_version: i64,
    work_revision: Option<i64>,
    modules: Vec<String>,
    work_permissions: Vec<String>,
    person_user_ids: Vec<UserId>,
    person_id: Option<RecordId>,
    emergency_until: Option<i64>,
    expires_at: i64,
}

impl PlatformIamFacts {
    fn from_context(current: &AuthorizedWorkContext) -> Self {
        let selected = current.selection();
        let workspace = selected.workspace();
        let assignment = selected.assignment();
        let iam = current.command_iam_metadata();
        Self {
            user_id: current.user_id().clone(),
            session_id: current.session_id().clone(),
            holding_id: current.holding_id().clone(),
            work_company: workspace.company_id.clone(),
            work_assignment: assignment.id.clone(),
            work_assignment_version: assignment.version,
            work_revision: Some(selected.revision()),
            modules: current.modules().to_vec(),
            work_permissions: iam.permissions.clone(),
            person_user_ids: iam.person_user_ids.clone(),
            person_id: iam.person_id.clone(),
            emergency_until: iam.emergency_until,
            expires_at: current.expires_at(),
        }
    }
}

fn context(
    facts: &PlatformIamFacts,
    holding_slug: &str,
    platform_assignment_id: &str,
    platform_role: &str,
    expires_at: i64,
) -> Value {
    serde_json::json!({
        "userId": facts.user_id.as_str(),
        "holdingId": facts.holding_id.as_str(),
        "holdingSlug": holding_slug,
        "companyId": facts.work_company.as_str(),
        "branchId": Value::Null,
        "departmentId": Value::Null,
        "workAssignmentId": facts.work_assignment.as_str(),
        "platformAssignmentId": platform_assignment_id,
        "platformRole": platform_role,
        "assignmentVersion": facts.work_assignment_version,
        "revision": facts.work_revision,
        "modules": facts.modules,
        "permissions": facts.work_permissions,
        "expiresAt": expires_at,
    })
}

struct PlatformAssignment {
    role: String,
    version: i64,
    valid_until: i64,
}

/// Resolve the exact Platform assignment and its published permission revision
/// on the caller's Control connection (the same serializable owner unit).
async fn platform_actor(
    connection: &mut PgConnection,
    facts: &PlatformIamFacts,
    platform_assignment_id: &str,
    now: i64,
) -> Result<(IamRoleActor, String), Error> {
    let assignment: Option<(String, i64, i64)> = sqlx::query_as(
        "SELECT role, version::bigint, valid_until FROM platform_assignments \
         WHERE id=$1 AND user_id=$2 AND revoked_at IS NULL \
         AND valid_from <= $3 AND valid_until > $3",
    )
    .bind(platform_assignment_id)
    .bind(facts.user_id.as_str())
    .bind(now)
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| PgTransactionError::Business(database_unavailable()))?;
    let PlatformAssignment {
        role,
        version,
        valid_until,
    } = assignment
        .map(|(role, version, valid_until)| PlatformAssignment {
            role,
            version,
            valid_until,
        })
        .ok_or_else(forbidden_platform)?;
    if !matches!(role.as_str(), "platform_admin" | "platform_auditor") {
        return Err(forbidden_platform().into());
    }
    let revision: Option<(String, String, i64)> = sqlx::query_as(
        "SELECT v.permissions_json::text, v.grantable_json::text, v.revision::bigint \
         FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id \
          AND v.role_id=r.id AND v.revision=r.published_revision \
         WHERE r.id=$1 AND r.company_id IN ('*',$2) AND r.status='active' LIMIT 1",
    )
    .bind(&role)
    .bind(PLATFORM_COMPANY)
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| PgTransactionError::Business(database_unavailable()))?;
    let (mut permissions, grantable_permissions, role_revision) = revision
        .map(|(permissions, grantable, revision)| {
            let permissions = serde_json::from_str::<Vec<String>>(&permissions)
                .map_err(|_| PgTransactionError::Business(database_unavailable()))?;
            let grantable = serde_json::from_str::<Vec<String>>(&grantable)
                .map_err(|_| PgTransactionError::Business(database_unavailable()))?;
            Ok::<_, PgTransactionError<BusinessError>>((permissions, grantable, revision))
        })
        .transpose()?
        .unwrap_or_default();
    permissions.extend(facts.work_permissions.iter().cloned());
    permissions.sort();
    permissions.dedup();
    let company_id = CompanyId::new(PLATFORM_COMPANY).map_err(PgTransactionError::Business)?;
    let assignment_id = AssignmentId::new(platform_assignment_id.to_owned())
        .map_err(PgTransactionError::Business)?;
    Ok((
        IamRoleActor {
            user_id: facts.user_id.clone(),
            session_id: facts.session_id.clone(),
            holding_id: facts.holding_id.clone(),
            company_id,
            branch_id: None,
            department_id: None,
            assignment_id,
            scope_kind: "platform".into(),
            scope_node_id: None,
            role: Some(role.clone()),
            role_revision: Some(role_revision),
            assignment_version: Some(version),
            identity_version: None,
            authority_revision: None,
            permissions,
            grantable_permissions,
            person_user_ids: facts.person_user_ids.clone(),
            person_id: facts.person_id.clone(),
            platform: true,
            emergency_until: facts.emergency_until,
            expires_at: facts.expires_at.min(valid_until),
        },
        role,
    ))
}

pub struct PlatformIamService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: PlatformIamConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> PlatformIamService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: PlatformIamConfiguration,
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
            permission: "manage_erpx.access".into(),
            module: Some("platform_administration".into()),
            destination: "platform_administration".into(),
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
            request.mutation = true;
            Ok(request)
        }
    }

    pub async fn iam_command<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        platform_assignment_id: String,
        command: IamRolesReviewsHttpCommand,
    ) -> Result<IamRolesReviewsHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let holding_slug = self.configuration.holding_slug.clone();
        let company = self.configuration.company.clone();
        let control = self.configuration.control.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                PlatformIamSources,
                move |_connection, current| {
                    let holding = holding.clone();
                    let holding_slug = holding_slug.clone();
                    let company = company.clone();
                    let control = control.clone();
                    let clock = clock.clone();
                    let platform_assignment_id = platform_assignment_id.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("manage_erpx.access")?;
                        let now = clock.now().get();
                        let facts = PlatformIamFacts::from_context(current);
                        let unit_facts = facts.clone();
                        let reply = PgUnitOfWork::new(control)
                            .run_once(TransactionOptions::default(), move |connection| {
                                let facts = unit_facts.clone();
                                let platform_assignment_id = platform_assignment_id.clone();
                                let command = command.clone();
                                let holding_slug = holding_slug.clone();
                                Box::pin(async move {
                                    let (actor, role) = platform_actor(
                                        connection,
                                        &facts,
                                        &platform_assignment_id,
                                        now,
                                    )
                                    .await?;
                                    let catalog = permission_catalog()
                                        .map_err(PgTransactionError::Business)?;
                                    let key = match &command {
                                        IamRolesReviewsHttpCommand::SaveRole {
                                            idempotency_key,
                                            ..
                                        }
                                        | IamRolesReviewsHttpCommand::DecideReview {
                                            idempotency_key,
                                            ..
                                        }
                                        | IamRolesReviewsHttpCommand::StartAccessReview {
                                            idempotency_key,
                                            ..
                                        }
                                        | IamRolesReviewsHttpCommand::DecideAccessReviewItem {
                                            idempotency_key,
                                            ..
                                        }
                                        | IamRolesReviewsHttpCommand::CancelAccessReview {
                                            idempotency_key,
                                            ..
                                        } => CommandKey::new(idempotency_key.clone())
                                            .map_err(PgTransactionError::Business)?,
                                    };
                                    let mut repository =
                                        PgIamRolesReviews::new_platform(connection);
                                    let data = match command {
                                        IamRolesReviewsHttpCommand::SaveRole { input, .. } => {
                                            save_role(
                                                &mut repository,
                                                &actor,
                                                &catalog,
                                                &key,
                                                &input,
                                                now,
                                            )
                                            .await
                                        }
                                        IamRolesReviewsHttpCommand::DecideReview {
                                            review_id,
                                            input,
                                            ..
                                        } => {
                                            decide_review(
                                                &mut repository,
                                                &actor,
                                                &catalog,
                                                &key,
                                                &review_id,
                                                &input,
                                                now,
                                            )
                                            .await
                                        }
                                        IamRolesReviewsHttpCommand::StartAccessReview {
                                            input,
                                            ..
                                        } => {
                                            start_access_review(
                                                &mut repository,
                                                &actor,
                                                &key,
                                                &input,
                                                now,
                                            )
                                            .await
                                        }
                                        IamRolesReviewsHttpCommand::DecideAccessReviewItem {
                                            item_id,
                                            input,
                                            ..
                                        } => {
                                            decide_access_review_item(
                                                &mut repository,
                                                &actor,
                                                &key,
                                                &item_id,
                                                &input,
                                                now,
                                            )
                                            .await
                                        }
                                        IamRolesReviewsHttpCommand::CancelAccessReview {
                                            campaign_id,
                                            input,
                                            ..
                                        } => {
                                            cancel_access_review_campaign(
                                                &mut repository,
                                                &actor,
                                                &key,
                                                &campaign_id,
                                                &input,
                                                now,
                                            )
                                            .await
                                        }
                                    }?;
                                    Ok(IamRolesReviewsHttpResponse {
                                        data,
                                        context: context(
                                            &facts,
                                            &holding_slug,
                                            &platform_assignment_id,
                                            &role,
                                            actor.expires_at,
                                        ),
                                    })
                                })
                            })
                            .await?;
                        Ok(reply)
                    })
                },
            )
            .await
    }
}

pub struct PlatformIamBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<PlatformIamService<C>>,
}

impl<C: Clock + Send + Sync + 'static> PlatformIamBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<PlatformIamService<C>>,
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
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest, String), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        let platform_assignment_id = unique_header(
            headers,
            HeaderName::from_static("x-erpx-platform-assignment"),
        )?
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::Forbidden,
                "PLATFORM_AUTHORITY_REQUIRED",
                "Your selected Platform assignment is unavailable.",
            )
        })?
        .to_owned();
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: false,
            },
            platform_assignment_id,
        ))
    }
}

fn unique_header<'a>(
    headers: &'a HeaderMap,
    name: HeaderName,
) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(malformed());
    }
    first
        .map(|value| value.to_str().map_err(|_| malformed()))
        .transpose()
}

impl<C: Clock + Send + Sync + 'static> IamRolesReviewsHttpBackend for PlatformIamBackend<C> {
    fn execute<'a>(
        &'a self,
        _: IamRolesReviewsHttpRequest<'a>,
    ) -> IamRolesReviewsHttpFuture<'a, IamRolesReviewsHttpResponse> {
        Box::pin(async { Err(not_found()) })
    }

    fn execute_platform<'a>(
        &'a self,
        request: IamPlatformRolesReviewsHttpRequest<'a>,
    ) -> IamRolesReviewsHttpFuture<'a, IamRolesReviewsHttpResponse> {
        Box::pin(async move {
            let (proof, selection, platform_assignment_id) = self.context(request.headers)?;
            self.service
                .iam_command(
                    request.host,
                    proof,
                    move || Ok(selection),
                    platform_assignment_id,
                    request.command,
                )
                .await
                .map_err(business)
        })
    }
}
