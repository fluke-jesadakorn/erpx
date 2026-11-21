//! Native Work snapshot HTTP assembly.
//!
//! Composition supplies the current Account/Holding proof and one serializable
//! Holding unit. The owner read returns availability facts only; authority,
//! presentation and every other projection stay with their owners.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_collaboration::public::staff_facts::unread_home_count;
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
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, HoldingId};
use erpx_server_work_projection::public::work_home_unread_http::{
    WorkHomeUnreadHttpBackend, WorkHomeUnreadHttpFuture, WorkHomeUnreadHttpRequest,
    WorkHomeUnreadHttpResponse,
};
use erpx_server_work_projection::public::work_snapshot_http::{
    WorkSnapshotHttpBackend, WorkSnapshotHttpFuture, WorkSnapshotHttpRequest,
    WorkSnapshotHttpResponse, native_work_snapshot_json,
};
use erpx_server_work_projection::public::work_snapshot_read::read_native_work_snapshot;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "WORK_SNAPSHOT_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "WORK_SNAPSHOT_UNAVAILABLE",
        "The native Work snapshot is unavailable.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct WorkSnapshotSources;

impl HoldingWorkSourcePreparation for WorkSnapshotSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

fn context(current: &AuthorizedWorkContext) -> serde_json::Value {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    serde_json::json!({
        "userId": current.user_id().as_str(),
        "holdingId": current.holding_id().as_str(),
        "companyId": workspace.company_id.as_str(),
        "assignmentId": assignment.id.as_str(),
        "revision": selected.revision(),
        "expiresAt": current.expires_at(),
    })
}

#[derive(Clone)]
pub struct NativeWorkSnapshotConfiguration {
    holding: HoldingId,
}

impl NativeWorkSnapshotConfiguration {
    pub fn new(holding: HoldingId) -> Self {
        Self { holding }
    }
}

pub struct NativeWorkSnapshotService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: NativeWorkSnapshotConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> NativeWorkSnapshotService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: NativeWorkSnapshotConfiguration,
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
            permission: "self.tasks".into(),
            module: None,
            destination: "work/native-snapshot".into(),
            api: true,
        }
    }

    pub async fn read<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
    ) -> Result<WorkSnapshotHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                Self::access(),
                WorkSnapshotSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding {
                            return Err(scope().into());
                        }
                        current.require_permission("self.tasks")?;
                        let snapshot =
                            read_native_work_snapshot(connection, current, clock.now()).await?;
                        Ok(WorkSnapshotHttpResponse {
                            data: native_work_snapshot_json(&snapshot),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct NativeWorkSnapshotBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<NativeWorkSnapshotService<C>>,
}

impl<C: Clock + Send + Sync + 'static> NativeWorkSnapshotBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<NativeWorkSnapshotService<C>>,
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
        let (branch_slug, department_slug) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: erpx_server_shared::public::CompanyId::new(
                    headers
                        .get("x-erpx-context-company")
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or_default(),
                )?,
                branch_slug,
                department_slug,
                expected: None,
                mutation: false,
            },
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> WorkSnapshotHttpBackend for NativeWorkSnapshotBackend<C> {
    fn read<'a>(
        &'a self,
        request: WorkSnapshotHttpRequest<'a>,
    ) -> WorkSnapshotHttpFuture<'a, WorkSnapshotHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .read(request.host, proof, move || Ok(selection))
                .await
                .map_err(business)
        })
    }
}

/// Reproduce the retained `encodeURIComponent` calls in `workspaceBase`.
fn encode_uri_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
            )
        {
            encoded.push(*byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// Reproduce `workspaceBase(publicScope(actor))` from the selected scope. The
/// Holding path is the canonical `/holdings/{slug}`; branch and department
/// segments are appended only when the selected workspace carries their slug.
fn workspace_base(
    holding_slug: &str,
    branch_slug: Option<&str>,
    department_slug: Option<&str>,
) -> String {
    let holding = format!("/holdings/{}", encode_uri_component(holding_slug));
    match (branch_slug, department_slug) {
        (_, Some(department)) => {
            let branch = branch_slug
                .map(|branch| format!("/branches/{}", encode_uri_component(branch)))
                .unwrap_or_default();
            format!(
                "{holding}{branch}/departments/{}",
                encode_uri_component(department)
            )
        }
        (Some(branch), None) => {
            format!("{holding}/branches/{}", encode_uri_component(branch))
        }
        (None, None) => holding,
    }
}

#[derive(Clone)]
pub struct NativeWorkHomeUnreadConfiguration {
    holding: HoldingId,
    holding_slug: String,
}

impl NativeWorkHomeUnreadConfiguration {
    pub fn new(holding: HoldingId, holding_slug: impl Into<String>) -> Self {
        Self {
            holding,
            holding_slug: holding_slug.into(),
        }
    }
}

pub struct NativeWorkHomeUnreadService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: NativeWorkHomeUnreadConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> NativeWorkHomeUnreadService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: NativeWorkHomeUnreadConfiguration,
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
            permission: "self.tasks".into(),
            module: None,
            destination: "work/home-unread".into(),
            api: true,
        }
    }

    pub async fn read<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
    ) -> Result<WorkHomeUnreadHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let holding_slug = self.configuration.holding_slug.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                Self::access(),
                WorkSnapshotSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let holding_slug = holding_slug.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding {
                            return Err(scope().into());
                        }
                        current.require_permission("self.tasks")?;
                        let workspace = current.selection().workspace();
                        let base = workspace_base(
                            &holding_slug,
                            workspace.branch_slug.as_deref(),
                            workspace.department_slug.as_deref(),
                        );
                        let count = unread_home_count(
                            connection,
                            workspace.company_id.as_str(),
                            current.user_id().as_str(),
                            workspace.branch_id.as_ref().map(|id| id.as_str()),
                            &base,
                            clock.now().get(),
                        )
                        .await?;
                        Ok(WorkHomeUnreadHttpResponse {
                            count,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct NativeWorkHomeUnreadBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<NativeWorkHomeUnreadService<C>>,
}

impl<C: Clock + Send + Sync + 'static> NativeWorkHomeUnreadBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<NativeWorkHomeUnreadService<C>>,
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
        let (branch_slug, department_slug) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: erpx_server_shared::public::CompanyId::new(
                    headers
                        .get("x-erpx-context-company")
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or_default(),
                )?,
                branch_slug,
                department_slug,
                expected: None,
                mutation: false,
            },
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> WorkHomeUnreadHttpBackend
    for NativeWorkHomeUnreadBackend<C>
{
    fn read<'a>(
        &'a self,
        request: WorkHomeUnreadHttpRequest<'a>,
    ) -> WorkHomeUnreadHttpFuture<'a, WorkHomeUnreadHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .read(request.host, proof, move || Ok(selection))
                .await
                .map_err(business)
        })
    }
}
