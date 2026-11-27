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
#[derive(Clone, Copy)]
enum RoleResponse {
    Internal,
    ActiveAssignments,
    SelectedAssignment,
}
pub fn native_work_roles_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    clock: Arc<C>,
    targets: Vec<NativeWorkRoleTarget>,
) -> Result<Router, BusinessError> {
    native_work_roles_router_with_selection(policy, registry, scopes, clock, targets, false)
}
pub(crate) fn native_work_roles_router_with_selection<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    clock: Arc<C>,
    targets: Vec<NativeWorkRoleTarget>,
    public_selection: bool,
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
    let router = Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/field-service/work-roles",
            get(read::<C>).post(write::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/field-service/work-roles/workspace-preference",
            axum::routing::post(save_workspace::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/workspace-preference",
            axum::routing::post(save_browser_workspace::<C>),
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
        );
    let router = if public_selection {
        router
            .route(
                "/api/v1/holdings/{holding_slug}/assignments/active",
                get(read_active::<C>).post(select_active::<C>),
            )
            .route(
                "/api/v1/holdings/{holding_slug}/companies/active",
                axum::routing::post(select_active_company::<C>),
            )
    } else {
        router
    };
    Ok(router.with_state(Arc::new(RoleState {
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
fn company_required() -> BusinessError {
    BusinessError::invalid("COMPANY_CONTEXT_REQUIRED", "Choose a Company.")
}
fn selection_cookie(name: &str, value: &str, max_age: u32) -> Result<HeaderValue, BusinessError> {
    if !value.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')) {
        return Err(input());
    }
    HeaderValue::from_str(&format!(
        "{name}={value}; Path=/; Max-Age={max_age}; HttpOnly; Secure; SameSite=Lax; Priority=High"
    )).map_err(|_| input())
}
type RoleResult<T> = Result<T, PgTransactionError<BusinessError>>;

fn option_href(option: &native_work_roles::NativeWorkRoleOption) -> String {
    match (option.branch_slug.as_deref(), option.department_slug.as_deref()) {
        (Some(branch), Some(department)) => format!("/{branch}/{department}/sales"),
        (Some(branch), None) => format!("/{branch}/sales"),
        (None, Some(department)) => format!("/departments/{department}/sales"),
        (None, None) => "/sales".into(),
    }
}

fn active_assignments(state: native_work_roles::NativeWorkRoleState) -> RoleResult<serde_json::Value> {
    let options = state.options.iter().map(|option| {
        let mut data = serde_json::to_value(option).map_err(|_| input())?;
        data["href"] = serde_json::Value::String(option_href(option));
        Ok(data)
    }).collect::<Result<Vec<_>, BusinessError>>()?;
    let revision = state.selection.as_ref().map_or(0, |selection| selection.revision);
    let selected = state.selection.as_ref().filter(|_| state.valid);
    Ok(serde_json::json!({
        "valid": state.valid,
        "options": options,
        "revision": revision,
        "assignmentVersion": selected.and_then(|s| s.assignment_version),
        "assignmentId": selected.and_then(|s| s.assignment_id.as_deref()),
        "companyId": state.company_id,
        "userId": state.user_id,
    }))
}
fn selected_assignment(state: native_work_roles::NativeWorkRoleState) -> RoleResult<(serde_json::Value, String)> {
    let selected = state.selection.as_ref().ok_or_else(input)?;
    let option = state.options.iter()
        .find(|option| Some(option.id.as_str()) == selected.assignment_id.as_deref())
        .ok_or_else(input)?;
    Ok((serde_json::json!({
        "assignmentId": option.id,
        "revision": selected.revision,
        "href": option_href(option),
        "companyId": state.company_id,
        "userId": state.user_id,
        "appliesToWorkspace": state.valid,
    }), state.holding_id))
}
async fn read<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<RoleState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    run(s, slug, headers, Ok(WorkRoleAction::Restore), false, RoleResponse::Internal).await
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
    respond(remember_workspace(s, slug, request, false).await)
}

async fn save_browser_workspace<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<RoleState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    respond(
        remember_workspace(s, slug, request, true)
            .await
            .map(|_| serde_json::json!({"saved": true})),
    )
}

async fn remember_workspace<C: Clock + Send + Sync + 'static>(
    s: Arc<RoleState<C>>,
    slug: String,
    request: Request,
    browser: bool,
) -> RoleResult<crate::native_account_preferences::NativeAccountPreferences> {
    let (parts, body) = request.into_parts();
    let headers = parts.headers;
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
    let path = if browser {
        let base = raw
            .get("base")
            .and_then(|v| v.as_str())
            .filter(|base| base.len() <= 512)
            .ok_or_else(|| {
                BusinessError::invalid("INVALID_WORKSPACE", "Choose an available workspace.")
            })?;
        format!("{base}/home")
    } else {
        raw.get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(input)?
            .to_owned()
    };
    let selector =
        erpx_server_work_projection::public::workspace_preference::workspace_preference_scope(
            &path, &slug,
        )?;
    let requested =
        CapturedHoldingMutation::capture(&headers, selector.branch_slug, selector.department_slug)
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
        .remember_workspace(host.into(), inbound.into_proof(), path, requested)
        .await
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
    respond(select_company_state(s, slug, request).await)
}
async fn select_active_company<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<RoleState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = select_company_state(s, slug, request).await.and_then(|state| {
        let company = state.company.ok_or_else(input)?.company_id;
        let cookie = selection_cookie(&format!("erpx_company_{}", state.holding_id), &company, 31_536_000)?;
        Ok((company, cookie))
    });
    match result {
        Ok((company, cookie)) => {
            let mut response = respond(Ok(serde_json::json!({"companyId": company})));
            response.headers_mut().append(header::SET_COOKIE, cookie);
            response
        }
        Err(error) => respond::<serde_json::Value>(Err(error)),
    }
}
async fn select_company_state<C: Clock + Send + Sync + 'static>(
    s: Arc<RoleState<C>>,
    slug: String,
    request: Request,
) -> Result<crate::native_account_preferences::NativeAccountPreferences, PgTransactionError<BusinessError>> {
    let (parts, body) = request.into_parts();
    let headers = parts.headers;
    async {
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
    .await
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
    run(s, slug, headers, Ok(WorkRoleAction::Inspect), false, RoleResponse::Internal).await
}
async fn initialize<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<RoleState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    run(s, slug, headers, Ok(WorkRoleAction::Initialize), false, RoleResponse::Internal).await
}
async fn read_active<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<RoleState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    run(s, slug, headers, Ok(WorkRoleAction::Inspect), false, RoleResponse::ActiveAssignments).await
}
async fn select_active<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<RoleState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    write_role(s, slug, request, RoleResponse::SelectedAssignment).await
}
async fn write<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<RoleState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    write_role(s, slug, request, RoleResponse::Internal).await
}
async fn write_role<C: Clock + Send + Sync + 'static>(
    s: Arc<RoleState<C>>,
    slug: String,
    request: Request,
    response_kind: RoleResponse,
) -> Response {
    let (parts, body) = request.into_parts();
    // Bounded input; the role actor is always resolved from the native Account proof.
    let action = async {
        let bytes = to_bytes(body, 16384).await.map_err(|_| input())?;
        let parsed = erpx_server_workflow::public::commands::CommandJson::parse(
            std::str::from_utf8(&bytes).map_err(|_| input())?,
        )?;
        let raw: serde_json::Value = serde_json::from_str(parsed.as_str()).map_err(|_| input())?;
        if matches!(response_kind, RoleResponse::Internal)
            && raw.get("action").and_then(|v| v.as_str()) == Some("initialize")
        {
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
    run(s, slug, parts.headers, action, true, response_kind).await
}
async fn run<C: Clock + Send + Sync + 'static>(
    s: Arc<RoleState<C>>,
    slug: String,
    headers: HeaderMap,
    action: Result<WorkRoleAction, BusinessError>,
    mutation: bool,
    response_kind: RoleResponse,
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
        let proof = inbound.into_proof();
        let selected_company = unique_request_header(&headers, "x-erpx-context-company")?;
        let company = if let Some(company) = selected_company {
            CompanyId::new(company)?
        } else if matches!(response_kind, RoleResponse::Internal) {
            return Err(company_required().into());
        } else {
            let preferences = preference_service(&s, target)
                .read(host.into(), proof.clone())
                .await?;
            if let Some(company) = preferences.company {
                CompanyId::new(company.company_id)?
            } else {
                let companies = native_work_roles::companies(
                    &target.inspector, host, proof.clone(), s.clock.clone(),
                ).await?;
                let [company] = companies.companies.as_slice() else {
                    return Err(company_required().into());
                };
                CompanyId::new(company.id.clone())?
            }
        };
        let (branch_slug, department_slug) = s.scopes.slugs(&headers)?;
        // Selectors are untrusted. Identity/Organization verify the exact current
        // Company and hierarchy inside the retained authenticated Holding unit.
        let request = HoldingWorkspaceRequest {
            company_id: company,
            branch_slug,
            department_slug,
            expected: None,
            mutation,
        };
        native_work_roles::execute(
            &target.inspector,
            host,
            proof,
            request,
            s.clock.clone(),
            action?,
        )
        .await
    }
    .await;
    match response_kind {
        RoleResponse::Internal => respond(result),
        RoleResponse::ActiveAssignments => respond(result.and_then(active_assignments)),
        RoleResponse::SelectedAssignment => match result.and_then(selected_assignment) {
            Ok((data, holding)) => {
                let mut response = respond(Ok(data));
                for prefix in ["erpx_assignment_", "erpx_role_preference_"] {
                    match selection_cookie(&format!("{prefix}{holding}"), "", 0) {
                        Ok(cookie) => { response.headers_mut().append(header::SET_COOKIE, cookie); }
                        Err(error) => return respond::<serde_json::Value>(Err(error.into())),
                    }
                }
                response
            }
            Err(error) => respond::<serde_json::Value>(Err(error)),
        },
    }
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
