//! Private Holding Account role selection. No selected-role header is trusted
//! for this bootstrap; current grants and CAS revision are resolved in Rust.
use crate::{
    http_scope::BusinessScopeSlugs,
    native_work_roles::{self, WorkRoleAction},
};
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Path, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use erpx_server_identity_access::public::{
    AccountPolicy, RequestAudience,
    account_holding_bindings::ConfiguredHoldingRegistry,
    account_inbound_http::{
        CapturedHoldingMutation, parse_inbound_account_request, unique_request_header,
    },
    work_context::{HoldingWorkContextInspector, HoldingWorkspaceRequest},
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{AssignmentId, BusinessError, Clock, CompanyId, FailureKind};
use std::{collections::BTreeMap, sync::Arc};
pub struct NativeWorkRoleTarget {
    pub control: erpx_server_runtime::public::postgres::DatabasePool,
    pub host: String,
    pub holding_slug: String,
    pub inspector: Arc<HoldingWorkContextInspector>,
}
struct RoleState<C> {
    registry: Arc<ConfiguredHoldingRegistry>,
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    clock: Arc<C>,
    targets: BTreeMap<(String, String), NativeWorkRoleTarget>,
}
pub fn native_work_roles_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    clock: Arc<C>,
    targets: Vec<NativeWorkRoleTarget>,
) -> Result<Router, BusinessError> {
    policy.validate()?;
    let mut configured = BTreeMap::new();
    for target in targets {
        if !policy
            .audience(&target.host)
            .and_then(|a| registry.expected_for_audience(&a).ok())
            .is_some_and(|h| h.slug == target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "WORK_ROLE_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/field-service/work-roles",
            get(read::<C>).post(write::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/field-service/work-roles/workspace-preference",
            axum::routing::post(save_workspace::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/field-service/work-roles/preferences",
            get(read_preferences::<C>).post(select_company::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/field-service/work-roles/companies",
            get(read_companies::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/field-service/work-roles/access",
            get(read_access::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/field-service/work-roles/context",
            get(read_context::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/field-service/work-roles/current",
            get(current::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/field-service/work-roles/initialize",
            get(initialize::<C>),
        )
        .with_state(Arc::new(RoleState {
            registry,
            policy,
            scopes,
            clock,
            targets: configured,
        })))
}
fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn input() -> BusinessError {
    BusinessError::invalid(
        "VALIDATION_FAILED",
        "Use a role selection and its current revision.",
    )
}
async fn read<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<RoleState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    run(s, slug, headers, Ok(WorkRoleAction::Restore), false).await
}
fn preference_service<C: Clock + Send + Sync + 'static>(
    s: &RoleState<C>,
    t: &NativeWorkRoleTarget,
) -> crate::native_account_preferences::NativeAccountPreferenceService<C> {
    crate::native_account_preferences::NativeAccountPreferenceService {
        control: t.control.clone(),
        inspector: t.inspector.clone(),
        policy: s.policy.clone(),
        registry: s.registry.clone(),
        clock: s.clock.clone(),
    }
}
async fn save_workspace<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<RoleState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let headers = parts.headers;
    let result: Result<_, PgTransactionError<BusinessError>> = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = s
            .targets
            .get(&(host.into(), slug.clone()))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&s.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")? != Some(inbound.expected_origin()) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let bytes = to_bytes(body, 16384).await.map_err(|_| input())?;
        let raw: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| input())?;
        let path = raw.get("path").and_then(|v| v.as_str()).ok_or_else(input)?;
        let selector =
            erpx_server_work_projection::public::workspace_preference::workspace_preference_scope(
                path, &slug,
            )?;
        let requested = CapturedHoldingMutation::capture(
            &headers,
            selector.branch_slug,
            selector.department_slug,
        )
        .resolve()?;
        if raw
            .get("companyId")
            .is_some_and(|v| v.as_str() != Some(requested.company_id.as_str()))
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Review your current Company.",
            )
            .into());
        }
        preference_service(&s, target)
            .remember_workspace(host.into(), inbound.into_proof(), path.into(), requested)
            .await
    }
    .await;
    respond(result)
}
async fn read_preferences<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<RoleState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    let result: Result<_, PgTransactionError<BusinessError>> = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = s.targets.get(&(host.into(), slug)).ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&s.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        preference_service(&s, target)
            .read(host.into(), inbound.into_proof())
            .await
    }
    .await;
    respond(result)
}
async fn select_company<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<RoleState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let headers = parts.headers;
    let result: Result<_, PgTransactionError<BusinessError>> = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = s.targets.get(&(host.into(), slug)).ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&s.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")? != Some(inbound.expected_origin()) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let workspace = s.scopes.slugs(&headers)?;
        let lineage = if workspace != (None, None) {
            Some((
                workspace.0,
                workspace.1,
                unique_request_header(&headers, "x-erpx-context-company")?.map(str::to_owned),
            ))
        } else {
            None
        };
        if lineage.is_some() {
            return preference_service(&s, target)
                .select_company(host.into(), inbound.into_proof(), String::new(), lineage)
                .await;
        }
        let bytes = to_bytes(body, 16384).await.map_err(|_| input())?;
        let raw: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| input())?;
        if raw.get("path").is_some()
            || raw
                .get("operation")
                .and_then(|v| v.as_str())
                .is_some_and(|v| v != "company")
        {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "NATIVE_WORKSPACE_PREFERENCE_UNAVAILABLE",
                "This workspace preference command is not enabled yet.",
            )
            .into());
        }
        let company = raw
            .get("companyId")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty() && s.len() <= 128)
            .ok_or_else(input)?;
        preference_service(&s, target)
            .select_company(host.into(), inbound.into_proof(), company.into(), lineage)
            .await
    }
    .await;
    respond(result)
}
async fn read_companies<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<RoleState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    let result: Result<_, PgTransactionError<BusinessError>> = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = s.targets.get(&(host.into(), slug)).ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&s.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        native_work_roles::companies(
            &target.inspector,
            host,
            inbound.into_proof(),
            s.clock.clone(),
        )
        .await
    }
    .await;
    respond(result)
}
async fn read_access<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<RoleState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    read_workspace(s, slug, headers, false).await
}
async fn read_context<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<RoleState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    read_workspace(s, slug, headers, true).await
}
async fn read_workspace<C: Clock + Send + Sync + 'static>(
    s: Arc<RoleState<C>>,
    slug: String,
    headers: HeaderMap,
    context: bool,
) -> Response {
    let result: Result<_, PgTransactionError<BusinessError>> = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = s.targets.get(&(host.into(), slug)).ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&s.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let company = unique_request_header(&headers, "x-erpx-requested-company")?
            .map(CompanyId::new)
            .transpose()?;
        let front_company_slug =
            unique_request_header(&headers, "x-erpx-front-company")?.map(str::to_owned);
        let (branch_slug, department_slug) = s.scopes.slugs(&headers)?;
        let unsafe_method = unique_request_header(&headers, "x-erpx-method")?
            .is_some_and(|m| !matches!(m, "GET" | "HEAD" | "OPTIONS"));
        let captured = if context
            && (unsafe_method
                || [
                    "x-erpx-context-revision",
                    "x-erpx-context-assignment",
                    "x-erpx-context-assignment-version",
                ]
                .iter()
                .any(|k| headers.contains_key(*k)))
        {
            Some(
                CapturedHoldingMutation::capture(
                    &headers,
                    branch_slug.clone(),
                    department_slug.clone(),
                )
                .resolve_rendered_read()?,
            )
        } else {
            None
        };
        let input = native_work_roles::WorkRoleWorkspace {
            company,
            front_company_slug,
            branch_slug,
            department_slug,
            expected: captured.as_ref().and_then(|r| r.expected.clone()),
            captured_company: captured.map(|r| r.company_id),
        };
        native_work_roles::execute_workspace(
            &target.inspector,
            host,
            inbound.into_proof(),
            input,
            s.clock.clone(),
            if context {
                WorkRoleAction::Context {
                    destination: unique_request_header(&headers, "x-erpx-destination")?
                        .unwrap_or("")
                        .to_owned(),
                    api: unique_request_header(&headers, "x-erpx-api")? != Some("false"),
                }
            } else {
                WorkRoleAction::Inspect
            },
        )
        .await
    }
    .await;
    respond(result)
}
async fn current<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<RoleState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    run(s, slug, headers, Ok(WorkRoleAction::Inspect), false).await
}
async fn initialize<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<RoleState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    run(s, slug, headers, Ok(WorkRoleAction::Initialize), false).await
}
async fn write<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<RoleState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    // Bounded input; the role actor is always resolved from the native Account proof.
    let action = async {
        let bytes = to_bytes(body, 16384).await.map_err(|_| input())?;
        let parsed = erpx_server_workflow::public::commands::CommandJson::parse(
            std::str::from_utf8(&bytes).map_err(|_| input())?,
        )?;
        let raw: serde_json::Value = serde_json::from_str(parsed.as_str()).map_err(|_| input())?;
        if raw.get("action").and_then(|v| v.as_str()) == Some("initialize") {
            return Ok(WorkRoleAction::Initialize);
        }
        let assignment = raw
            .get("assignmentId")
            .and_then(|v| v.as_str())
            .ok_or_else(input)?;
        if assignment.len() > 128 {
            return Err(input());
        }
        let revision = raw
            .get("revision")
            .and_then(|v| v.as_i64())
            .filter(|n| (0..9_007_199_254_740_991).contains(n))
            .ok_or_else(input)?;
        let remember = match raw.get("remember") {
            None => true,
            Some(v) => v.as_bool().ok_or_else(input)?,
        };
        let expected_version = match raw.get("assignmentVersion") {
            None => None,
            Some(v) => Some(
                v.as_i64()
                    .filter(|n| (1..=9_007_199_254_740_991).contains(n))
                    .ok_or_else(input)?,
            ),
        };
        Ok(WorkRoleAction::Select {
            assignment: AssignmentId::new(assignment)?,
            expected_version,
            revision,
            remember,
        })
    }
    .await;
    run(s, slug, parts.headers, action, true).await
}
async fn run<C: Clock + Send + Sync + 'static>(
    s: Arc<RoleState<C>>,
    slug: String,
    headers: HeaderMap,
    action: Result<WorkRoleAction, BusinessError>,
    mutation: bool,
) -> Response {
    let result: Result<_, PgTransactionError<BusinessError>> = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = s.targets.get(&(host.into(), slug)).ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&s.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        let origin = unique_request_header(&headers, "origin")?;
        if (mutation && origin != Some(inbound.expected_origin()))
            || origin.is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let company =
            unique_request_header(&headers, "x-erpx-context-company")?.ok_or_else(|| {
                BusinessError::invalid("COMPANY_CONTEXT_REQUIRED", "Choose a Company.")
            })?;
        let (branch_slug, department_slug) = s.scopes.slugs(&headers)?;
        // Selectors are untrusted. Identity/Organization verify the exact current
        // Company and hierarchy inside the retained authenticated Holding unit.
        let request = HoldingWorkspaceRequest {
            company_id: CompanyId::new(company)?,
            branch_slug,
            department_slug,
            expected: None,
            mutation,
        };
        native_work_roles::execute(
            &target.inspector,
            host,
            inbound.into_proof(),
            request,
            s.clock.clone(),
            action?,
        )
        .await
    }
    .await;
    respond(result)
}
fn respond<T: serde::Serialize>(result: Result<T, PgTransactionError<BusinessError>>) -> Response {
    let mut response=match result{
  Ok(data)=>Json(serde_json::json!({"data":data})).into_response(),
  Err(PgTransactionError::Business(e))=>{let status=match e.kind{FailureKind::InvalidInput=>StatusCode::UNPROCESSABLE_ENTITY,FailureKind::Unauthenticated=>StatusCode::UNAUTHORIZED,FailureKind::Forbidden=>StatusCode::FORBIDDEN,FailureKind::NotFound=>StatusCode::NOT_FOUND,FailureKind::Conflict=>StatusCode::CONFLICT,FailureKind::Unavailable=>StatusCode::SERVICE_UNAVAILABLE};(status,Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()},
  Err(PgTransactionError::Persistence(_))=>(StatusCode::INTERNAL_SERVER_ERROR,Json(serde_json::json!({"error":{"code":"WORK_ROLE_UNAVAILABLE","message":"The work role selection is unavailable."}}))).into_response()
 };
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    response
}
