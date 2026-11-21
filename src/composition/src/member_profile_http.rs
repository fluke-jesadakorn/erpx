//! Composition for the native Organization member-profile save command HTTP
//! route. Account session, current IAM authority, the active holding membership
//! and the serializable Holding unit are resolved here; the owner crate owns the
//! planner and SQL. No business rules live in this module.
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
use erpx_server_organization::public::member_profile::{
    MemberProfileActor, MemberProfileCommandHttpBackend, MemberProfileCommandHttpFuture,
    MemberProfileCommandHttpResponse, MemberProfileCommandHttpTarget, PgMemberProfileCommand,
    save_member_profile,
};
use erpx_server_runtime::public::http_support::{HeaderMap, Router};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORGANIZATION_MEMBER_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ORGANIZATION_MEMBER_UNAVAILABLE",
        "The native member profile service is unavailable.",
    )
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "ORGANIZATION_MEMBER_REQUEST_INVALID",
        "Use one bounded member profile command.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct MemberProfileSources;

impl HoldingWorkSourcePreparation for MemberProfileSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct MemberProfileConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl MemberProfileConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }
}

pub struct MemberProfileService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: MemberProfileConfiguration,
    clock: Arc<C>,
}

fn authority_json(current: &AuthorizedWorkContext) -> String {
    let selected = current.selection();
    serde_json::json!({
        "userId": current.user_id().as_str(),
        "holdingId": current.holding_id().as_str(),
        "companyId": selected.workspace().company_id.as_str(),
        "assignmentId": selected.assignment().id.as_str(),
        "revision": selected.revision(),
    })
    .to_string()
}

impl<C: Clock + Send + Sync + 'static> MemberProfileService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: MemberProfileConfiguration,
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
            permission: "records.read".into(),
            module: None,
            destination: "organization".into(),
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

    pub async fn save<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        body: Value,
        key: CommandKey,
    ) -> Result<MemberProfileCommandHttpResponse, Error>
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
                MemberProfileSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let body = body.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let selected = current.selection();
                        let workspace = selected.workspace();
                        let authority = authority_json(current);
                        let actor = MemberProfileActor {
                            user_id: current.user_id().as_str().to_owned(),
                            company_id: workspace.company_id.clone(),
                            assignment_id: selected.assignment().id.as_str().to_owned(),
                            role: selected.assignment().role.as_str().to_owned(),
                            work_context: hex::encode(Sha256::digest(authority.as_bytes())),
                            authority_json: authority,
                            expires_at: current.expires_at(),
                        };
                        let now = clock.now();
                        let mut repository = PgMemberProfileCommand::new(connection, &actor);
                        let result =
                            save_member_profile(&actor, &body, &key, &mut repository, now).await?;
                        drop(repository);
                        Ok(MemberProfileCommandHttpResponse {
                            data: serde_json::to_value(result).map_err(|_| unavailable())?,
                            context: serde_json::json!({
                                "userId": current.user_id().as_str(),
                                "companyId": workspace.company_id.as_str(),
                            }),
                        })
                    })
                },
            )
            .await
    }
}

pub struct MemberProfileBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<MemberProfileService<C>>,
}

impl<C: Clock + Send + Sync + 'static> MemberProfileBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<MemberProfileService<C>>,
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

impl<C: Clock + Send + Sync + 'static> MemberProfileCommandHttpBackend for MemberProfileBackend<C> {
    fn save<'a>(
        &'a self,
        request: erpx_server_organization::public::member_profile::MemberProfileCommandHttpRequest<
            'a,
        >,
    ) -> MemberProfileCommandHttpFuture<'a> {
        Box::pin(async move {
            use erpx_server_organization::public::member_profile::MemberProfileCommandHttpKind;
            let MemberProfileCommandHttpKind::Save = request.kind else {
                return Err(invalid());
            };
            let key = request.key.cloned().ok_or_else(invalid)?;
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .save(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.body.clone(),
                    key,
                )
                .await
                .map_err(business)
        })
    }
}

/// Build the owner router bound to a backend; the lead mounts the result.
pub fn member_profile_router(
    host: impl Into<String>,
    holding_slug: impl Into<String>,
    origin: impl Into<String>,
    backend: Arc<dyn MemberProfileCommandHttpBackend>,
) -> Result<Router, BusinessError> {
    erpx_server_organization::public::member_profile::member_profile_command_router(vec![
        MemberProfileCommandHttpTarget {
            host: host.into(),
            holding_slug: holding_slug.into(),
            origin: origin.into(),
            backend,
        },
    ])
}
