//! Web-only BFF boundary.
//!
//! This module may call application ports and native Tonic clients. Persistence
//! adapters remain in the server composition root.

// Historical G11/G14 source-bound acceptance tests still exercise the retired
// façade helpers. The v2 server router does not register those handlers.
#![allow(dead_code)]

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use dioxus::server::axum::{
    Extension, Json,
    body::{Body, to_bytes},
    extract::{Form, Path, Query, Request},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use erpx_domain::corporate_identity::{
    BrandAssetKind, ResolvedCorporateIdentity, validate_brand_asset_bytes,
};
#[cfg(test)]
use erpx_ui_core::WorkItemStage;
use erpx_ui_core::{
    MAX_WORK_PAGE_SIZE, NavGroup, SafeUiText, WorkDomain, WorkItemKind, WorkItemReadModel,
    WorkItemSeverity, WorkQueueMeta, WorkQueueReadModel, WorkQueueView, WorkSlaState,
    WorkSourceContribution, valid_work_cursor,
};
use erpx_ui_kit::{
    DashboardBreakdown, DashboardBreakdownSegment, DashboardMetric, DashboardRange,
    DashboardReadModel, ReadModelAvailability, ReadModelFreshness, ReadModelMeta,
};
use erpx_web_session::{
    SafeActiveRole, SafeWebSessionSnapshot, SensitiveToken, WEB_SESSION_COOKIE_NAME,
    WEB_SESSION_SELECTOR_BYTES, WebSessionAccount, WebSessionBundle, WebSessionError,
    WebSessionSelector, WebSessionStore,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fmt, io, sync::Arc};
use tokio::sync::Mutex;
use uuid::Uuid;

const PRIVATE_NO_STORE: &str = "private, no-store";

#[derive(Clone)]
pub(crate) struct BffState {
    sessions: Arc<dyn WebSessionStore>,
    auth_http_origin: Arc<str>,
    grpc_origin: Arc<str>,
    trusted_public_origins: Arc<[Arc<str>]>,
    auth_http: reqwest::Client,
    refresh_lock: Arc<Mutex<()>>,
    production_profile: bool,
}

impl BffState {
    pub(crate) fn new(
        sessions: Arc<dyn WebSessionStore>,
        auth_http_origin: String,
        grpc_origin: String,
        trusted_public_origins: String,
        production_profile: bool,
    ) -> Result<Self, io::Error> {
        let auth_http_origin = auth_http_origin.trim_end_matches('/');
        let grpc_origin = grpc_origin.trim_end_matches('/');
        if !(auth_http_origin.starts_with("http://") || auth_http_origin.starts_with("https://"))
            || !(grpc_origin.starts_with("http://") || grpc_origin.starts_with("https://"))
        {
            return Err(io::Error::other("identity HTTP origin is invalid"));
        }
        let trusted_public_origins = parse_trusted_public_origins(&trusted_public_origins)?;
        erpx_console_bff::validate_runtime_origins(
            auth_http_origin,
            grpc_origin,
            &trusted_public_origins,
            production_profile,
        )?;
        Ok(Self {
            sessions,
            auth_http_origin: Arc::from(auth_http_origin),
            grpc_origin: Arc::from(grpc_origin),
            trusted_public_origins,
            auth_http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| io::Error::other("identity HTTP client is unavailable"))?,
            refresh_lock: Arc::new(Mutex::new(())),
            production_profile,
        })
    }
}

#[cfg(test)]
fn validate_web_runtime_profile(
    auth_http_origin: &str,
    grpc_origin: &str,
    trusted_public_origins: &[Arc<str>],
    production_profile: bool,
) -> Result<(), io::Error> {
    erpx_console_bff::validate_runtime_origins(
        auth_http_origin,
        grpc_origin,
        trusted_public_origins,
        production_profile,
    )
}

impl fmt::Debug for BffState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BffState")
            .field("sessions", &"[configured]")
            .field("auth_http_origin", &self.auth_http_origin)
            .field("grpc_origin", &self.grpc_origin)
            .field("trusted_public_origins", &self.trusted_public_origins)
            .field("production_profile", &self.production_profile)
            .finish()
    }
}

#[derive(Clone)]
pub(crate) struct BffCredential {
    pub(crate) session_id: Uuid,
    pub(crate) principal_ref: String,
    pub(crate) access_token: SensitiveToken,
    pub(crate) development_context_token: Option<SensitiveToken>,
}

impl fmt::Debug for BffCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BffCredential")
            .field("session_id", &self.session_id)
            .field("principal_ref", &self.principal_ref)
            .field("access_token", &"[redacted]")
            .field(
                "development_context_token",
                &self
                    .development_context_token
                    .as_ref()
                    .map(|_| "[redacted]"),
            )
            .finish()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct BffSessionContext {
    pub(crate) snapshot: SafeWebSessionSnapshot,
    pub(crate) credential: Option<BffCredential>,
    pub(crate) bundle: Option<WebSessionBundle>,
}

impl BffSessionContext {
    fn signed_out() -> Self {
        Self {
            snapshot: SafeWebSessionSnapshot::signed_out(),
            credential: None,
            bundle: None,
        }
    }
}

pub(crate) async fn session_middleware(
    Extension(state): Extension<BffState>,
    mut request: Request,
    next: Next,
) -> Response {
    if state.production_profile && !request_is_https(request.headers()) {
        return Response::builder()
            .status(StatusCode::UPGRADE_REQUIRED)
            .header(header::CACHE_CONTROL, PRIVATE_NO_STORE)
            .body(Body::empty())
            .unwrap_or_else(|_| unavailable());
    }
    let page_request = is_console_page_request(&request);
    let mut route_status = console_document_route_status(&request);
    let mut clear_stale_cookie = false;
    let context = match selector_from_headers(request.headers()) {
        None => BffSessionContext::signed_out(),
        Some(selector) => match load_and_refresh_bundle(&state, &selector).await {
            Ok(Some(bundle)) => {
                let snapshot = match SafeWebSessionSnapshot::from_bundle(&bundle) {
                    Ok(snapshot) => snapshot,
                    Err(_) => return unavailable(),
                };
                let Some(active) = bundle.active_account() else {
                    return unavailable();
                };
                BffSessionContext {
                    snapshot,
                    credential: Some(BffCredential {
                        session_id: active.session_id,
                        principal_ref: active.principal_ref.clone(),
                        access_token: active.access_token.clone(),
                        development_context_token: active.development_context_token.clone(),
                    }),
                    bundle: Some(bundle),
                }
            }
            Ok(None) => {
                clear_stale_cookie = true;
                BffSessionContext::signed_out()
            }
            Err(SessionLoadError::Unauthorized) if page_request => {
                clear_stale_cookie = true;
                BffSessionContext::signed_out()
            }
            Err(SessionLoadError::Unauthorized) => {
                return unauthorized_session_with_cleared_cookie(request_is_https(
                    request.headers(),
                ));
            }
            Err(SessionLoadError::Unavailable) => return unavailable(),
        },
    };
    if let Some(location) = console_page_redirect(&request, &context) {
        let mut response = temporary_redirect(&location);
        if clear_stale_cookie {
            clear_session_cookie(&mut response, request_is_https(request.headers()));
        }
        return response;
    }
    if route_status == Some(StatusCode::OK) && page_request && context.snapshot.authenticated {
        route_status =
            Some(authenticated_document_route_status(&state, &context, request.uri().path()).await);
    }
    let private_response = context.snapshot.authenticated;
    let secure_cookie = request_is_https(request.headers());
    request.extensions_mut().insert(context);
    let mut response = next.run(request).await;
    if let Some(status) = route_status
        && status != StatusCode::OK
        && response.status().is_success()
    {
        *response.status_mut() = status;
    }
    if clear_stale_cookie {
        clear_session_cookie(&mut response, secure_cookie);
    }
    if private_response {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static(PRIVATE_NO_STORE),
        );
    }
    apply_web_security_headers(&mut response, secure_cookie);
    response
}

fn apply_web_security_headers(response: &mut Response, https: bool) {
    let headers = response.headers_mut();
    headers.insert(
        "content-security-policy",
        HeaderValue::from_static(
            "default-src 'self'; base-uri 'self'; object-src 'none'; frame-ancestors 'none'; form-action 'self'; img-src 'self' data:; font-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline' 'unsafe-eval' 'wasm-unsafe-eval'; connect-src 'self'; manifest-src 'self'",
        ),
    );
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    headers.insert(
        "cross-origin-opener-policy",
        HeaderValue::from_static("same-origin"),
    );
    if https {
        headers.insert(
            "strict-transport-security",
            HeaderValue::from_static("max-age=63072000; includeSubDomains"),
        );
    }
}

async fn authenticated_document_route_status(
    _state: &BffState,
    context: &BffSessionContext,
    path: &str,
) -> StatusCode {
    if crate::navigation_authorizes_console_path(path, &[]) {
        return StatusCode::OK;
    }
    let Some(credential) = context.credential.as_ref() else {
        return StatusCode::UNAUTHORIZED;
    };
    let Some(scope_key) = context
        .snapshot
        .active_role
        .as_ref()
        .and_then(|role| role.scope_key.as_ref())
    else {
        return StatusCode::FORBIDDEN;
    };
    let development_context_token = credential
        .development_context_token
        .as_ref()
        .map(|token| token.expose().to_owned());
    match crate::fetch_navigation_manifest(
        credential.access_token.expose().to_owned(),
        credential.principal_ref.clone(),
        credential.session_id.to_string(),
        scope_key.clone(),
        development_context_token,
        false,
    )
    .await
    {
        Ok(groups) if crate::navigation_authorizes_console_path(path, &groups) => StatusCode::OK,
        Ok(_) => StatusCode::FORBIDDEN,
        Err(error) if error.ends_with("Unauthenticated") || error.ends_with("PermissionDenied") => {
            StatusCode::FORBIDDEN
        }
        Err(error) if error == "authorized navigation manifest is empty" => StatusCode::FORBIDDEN,
        Err(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn console_document_route_status(request: &Request) -> Option<StatusCode> {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return None;
    }
    let path = request.uri().path();
    if path.starts_with("/api/")
        || path.starts_with("/assets/")
        || path.starts_with("/_")
        || path.starts_with("/public/")
        || path.starts_with("/.well-known/")
        || path
            .rsplit('/')
            .next()
            .is_some_and(|segment| segment.contains('.'))
    {
        return None;
    }
    StatusCode::from_u16(crate::console_route_http_status(path)).ok()
}

fn is_console_page_request(request: &Request) -> bool {
    (request.method() == Method::GET || request.method() == Method::HEAD)
        && (request.uri().path() == "/"
            || request.uri().path() == crate::LOGIN_PATH
            || crate::is_legacy_login_path(request.uri().path())
            || crate::validated_login_return_path(request.uri().path()).is_some())
}

fn console_page_redirect(request: &Request, context: &BffSessionContext) -> Option<String> {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return None;
    }
    let path = request.uri().path();
    if path == "/" {
        return Some(
            if context.snapshot.authenticated && context.snapshot.role_selection_required {
                "/auth/accounts".to_owned()
            } else if context.snapshot.authenticated {
                crate::authenticated_landing_path().to_owned()
            } else {
                crate::login_redirect_location(crate::authenticated_landing_path())
            },
        );
    }
    if crate::is_legacy_login_path(path) {
        return Some(crate::LOGIN_PATH.to_owned());
    }
    if path == crate::LOGIN_PATH {
        if context.snapshot.authenticated && !context.snapshot.role_selection_required {
            return Some(
                request
                    .uri()
                    .path_and_query()
                    .and_then(|value| crate::login_return_path_from_url(value.as_str()))
                    .unwrap_or_else(|| crate::authenticated_landing_path().to_owned()),
            );
        }
        return None;
    }
    if context.snapshot.authenticated {
        if context.snapshot.role_selection_required && !path.starts_with("/auth/") {
            return Some("/auth/accounts".to_owned());
        }
        return None;
    }
    crate::validated_login_return_path(path)
        .map(|return_to| crate::login_redirect_location(&return_to))
}

fn temporary_redirect(location: &str) -> Response {
    Response::builder()
        .status(StatusCode::TEMPORARY_REDIRECT)
        .header(header::LOCATION, location)
        .header(header::CACHE_CONTROL, PRIVATE_NO_STORE)
        .body(Body::empty())
        .unwrap_or_else(|_| unavailable())
}

fn see_other(location: &str) -> Response {
    Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, location)
        .header(header::CACHE_CONTROL, PRIVATE_NO_STORE)
        .body(Body::empty())
        .unwrap_or_else(|_| unavailable())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SessionLoadError {
    Unauthorized,
    Unavailable,
}

fn classify_session_load_error(error: WebSessionError) -> SessionLoadError {
    match error {
        WebSessionError::Invalid | WebSessionError::Protection => {
            tracing::warn!(
                error = %error,
                "rejecting an unreadable stored Web session"
            );
            SessionLoadError::Unauthorized
        }
        WebSessionError::Entropy | WebSessionError::Conflict | WebSessionError::Unavailable => {
            tracing::warn!(
                error = %error,
                "Web session persistence is unavailable"
            );
            SessionLoadError::Unavailable
        }
    }
}

async fn load_and_refresh_bundle(
    state: &BffState,
    selector: &WebSessionSelector,
) -> Result<Option<WebSessionBundle>, SessionLoadError> {
    let now = Utc::now();
    let Some(bundle) = state
        .sessions
        .load(selector.digest(), now)
        .await
        .map_err(classify_session_load_error)?
    else {
        return Ok(None);
    };
    let active = bundle
        .active_account()
        .ok_or(SessionLoadError::Unavailable)?;
    if active.access_expires_at > now + Duration::seconds(30) {
        return Ok(Some(bundle));
    }

    // One refresh is allowed at a time in this process. Durable bundle CAS below
    // is the second fence for racing workers and rotating refresh credentials.
    let _guard = state.refresh_lock.lock().await;
    let now = Utc::now();
    let Some(mut bundle) = state
        .sessions
        .load(selector.digest(), now)
        .await
        .map_err(classify_session_load_error)?
    else {
        return Ok(None);
    };
    let current = bundle
        .active_account()
        .cloned()
        .ok_or(SessionLoadError::Unavailable)?;
    if current.access_expires_at > now + Duration::seconds(30) {
        return Ok(Some(bundle));
    }
    if current.refresh_expires_at <= now {
        let _ = state.sessions.delete(selector.digest()).await;
        return Err(SessionLoadError::Unauthorized);
    }
    let response = state
        .auth_http
        .post(format!("{}/auth/refresh", state.auth_http_origin))
        .json(&serde_json::json!({
            "session_id": current.session_id,
            "refresh_token": current.refresh_token.expose(),
        }))
        .send()
        .await
        .map_err(|_| SessionLoadError::Unavailable)?;
    if matches!(response.status().as_u16(), 401 | 403) {
        let _ = state.sessions.delete(selector.digest()).await;
        return Err(SessionLoadError::Unauthorized);
    }
    if !response.status().is_success() {
        return Err(SessionLoadError::Unavailable);
    }
    let mut refreshed = rotated_account(
        response
            .json::<RotatedSession>()
            .await
            .map_err(|_| SessionLoadError::Unavailable)?,
        now,
    )
    .map_err(|_| SessionLoadError::Unavailable)?;
    if refreshed.session_id != current.session_id
        || refreshed.principal_ref != current.principal_ref
    {
        return Err(SessionLoadError::Unavailable);
    }
    preserve_refreshed_active_role(&mut refreshed, &current);
    refreshed.development_context_token = current.development_context_token;
    refreshed.development_active_role = current.development_active_role;
    if refreshed.authorization_context_version.is_none() {
        refreshed.authorization_context_version = current.authorization_context_version;
    }
    let expected_version = bundle.version;
    let Some(account) = bundle
        .accounts
        .iter_mut()
        .find(|account| account.session_id == current.session_id)
    else {
        return Err(SessionLoadError::Unavailable);
    };
    *account = refreshed;
    bundle.version += 1;
    bundle.updated_at = now;
    bundle.expires_at = bundle
        .accounts
        .iter()
        .map(|account| account.refresh_expires_at)
        .max()
        .ok_or(SessionLoadError::Unavailable)?;
    match state.sessions.replace(&bundle, expected_version).await {
        Ok(()) => Ok(Some(bundle)),
        Err(WebSessionError::Conflict) => {
            let winner = state
                .sessions
                .load(selector.digest(), Utc::now())
                .await
                .map_err(classify_session_load_error)?
                .ok_or(SessionLoadError::Unauthorized)?;
            let active = winner
                .active_account()
                .ok_or(SessionLoadError::Unavailable)?;
            if active.access_expires_at <= Utc::now() + Duration::seconds(30) {
                Err(SessionLoadError::Unavailable)
            } else {
                Ok(Some(winner))
            }
        }
        Err(_) => Err(SessionLoadError::Unavailable),
    }
}

pub(crate) async fn session_snapshot(Extension(context): Extension<BffSessionContext>) -> Response {
    if context.snapshot.authenticated
        && context.credential.as_ref().is_none_or(|credential| {
            credential.access_token.expose().is_empty()
                || Some(credential.session_id) != context.snapshot.session_id
                || Some(credential.principal_ref.as_str())
                    != context.snapshot.principal_ref.as_deref()
        })
    {
        return unavailable();
    }
    let mut response = Json(context.snapshot).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(PRIVATE_NO_STORE),
    );
    response
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NavigationRequest {
    scope_key: String,
    #[serde(default)]
    include_inaccessible: bool,
}

pub(crate) async fn navigation_manifest(
    Extension(context): Extension<BffSessionContext>,
    Json(request): Json<NavigationRequest>,
) -> Response {
    let Some(credential) = context.credential else {
        return (
            StatusCode::UNAUTHORIZED,
            "authenticated web session required",
        )
            .into_response();
    };
    if !context.snapshot.authenticated
        || request.scope_key.is_empty()
        || request.scope_key.len() > 2_048
        || request.scope_key.chars().any(char::is_control)
    {
        return (StatusCode::BAD_REQUEST, "navigation request invalid").into_response();
    }
    let development_context_token = credential
        .development_context_token
        .as_ref()
        .map(|token| token.expose().to_owned());
    let groups = match crate::fetch_navigation_manifest(
        credential.access_token.expose().to_owned(),
        credential.principal_ref.clone(),
        credential.session_id.to_string(),
        request.scope_key,
        development_context_token,
        request.include_inaccessible,
    )
    .await
    {
        Ok(groups) => groups,
        Err(error) if error.ends_with("InvalidArgument") => {
            tracing::warn!(
                event = "navigation_manifest_failed",
                category = "invalid_scope",
                "authorized navigation manifest request failed"
            );
            return (StatusCode::BAD_REQUEST, "navigation scope invalid").into_response();
        }
        Err(error) if error.ends_with("Unauthenticated") || error.ends_with("PermissionDenied") => {
            tracing::warn!(
                event = "navigation_manifest_failed",
                category = "authorization_denied",
                "authorized navigation manifest request failed"
            );
            return (StatusCode::FORBIDDEN, "navigation authorization denied").into_response();
        }
        Err(error) if error == "authorized navigation manifest is empty" => Vec::new(),
        Err(_) => {
            tracing::warn!(
                event = "navigation_manifest_failed",
                category = "transport_unavailable",
                "authorized navigation manifest request failed"
            );
            return unavailable();
        }
    };
    tracing::info!(
        event = "navigation_manifest_loaded",
        group_count = groups.len(),
        route_count = groups.iter().map(|group| group.items.len()).sum::<usize>(),
        "authorized navigation manifest loaded"
    );
    let mut response = Json(crate::navigation_http_groups(&groups)).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(PRIVATE_NO_STORE),
    );
    response
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DashboardQuery {
    range: Option<String>,
    compare: Option<String>,
}

pub(crate) async fn dashboard_read_model(
    Extension(state): Extension<BffState>,
    Extension(context): Extension<BffSessionContext>,
    Query(query): Query<DashboardQuery>,
) -> Response {
    let Some(credential) = context.credential else {
        return (
            StatusCode::UNAUTHORIZED,
            "authenticated web session required",
        )
            .into_response();
    };
    let range = match query.range.as_deref().unwrap_or("30d") {
        value if DashboardRange::parse(value).is_some() => {
            DashboardRange::parse(value).unwrap_or_default()
        }
        _ => return (StatusCode::BAD_REQUEST, "dashboard range invalid").into_response(),
    };
    let compared_to_previous = match query.compare.as_deref() {
        None => false,
        Some("previous") => true,
        Some(_) => {
            return (StatusCode::BAD_REQUEST, "dashboard comparison invalid").into_response();
        }
    };
    let Some(scope_key) = context
        .bundle
        .as_ref()
        .and_then(WebSessionBundle::active_account)
        .and_then(WebSessionAccount::effective_active_role)
        .and_then(|role| role.scope_key.clone())
    else {
        return (StatusCode::FORBIDDEN, "active role scope required").into_response();
    };
    let development_context_token = credential
        .development_context_token
        .as_ref()
        .map(|token| token.expose().to_owned());
    let groups = match crate::fetch_navigation_manifest(
        credential.access_token.expose().to_owned(),
        credential.principal_ref.clone(),
        credential.session_id.to_string(),
        scope_key.clone(),
        development_context_token,
        false,
    )
    .await
    {
        Ok(groups) => groups,
        Err(error) if error.ends_with("InvalidArgument") => {
            return (StatusCode::BAD_REQUEST, "dashboard scope invalid").into_response();
        }
        Err(error) if error.ends_with("Unauthenticated") || error.ends_with("PermissionDenied") => {
            return (StatusCode::FORBIDDEN, "dashboard authorization denied").into_response();
        }
        Err(_) => return unavailable(),
    };
    let sources = fetch_platform_work_sources(&state, &credential, Utc::now()).await;
    let model = dashboard_from_authorized_sources(
        range,
        compared_to_previous,
        scope_key,
        &groups,
        &sources,
        Utc::now(),
    );
    if !model.is_bounded() {
        return unavailable();
    }
    private_json(model)
}

fn dashboard_from_authorized_sources(
    range: DashboardRange,
    compared_to_previous: bool,
    scope: String,
    groups: &[NavGroup],
    sources: &AuthorizedWorkSources,
    as_of: DateTime<Utc>,
) -> DashboardReadModel {
    let successful_sources = sources
        .contributions
        .iter()
        .filter(|source| {
            matches!(
                source.availability,
                ReadModelAvailability::Available | ReadModelAvailability::Empty
            )
        })
        .count();
    let availability = if groups.is_empty() || successful_sources == 0 {
        if groups.is_empty() {
            ReadModelAvailability::Empty
        } else {
            ReadModelAvailability::Unavailable
        }
    } else if successful_sources == sources.contributions.len() {
        if sources.items.is_empty() {
            ReadModelAvailability::Empty
        } else {
            ReadModelAvailability::Available
        }
    } else {
        ReadModelAvailability::Partial
    };
    let counters = WorkQueueReadModel::counters_for(&sources.items, as_of);
    let metrics = if successful_sources == 0 {
        Vec::new()
    } else {
        [
            ("active_work", "Active authorized work", counters.total),
            (
                "approvals",
                "Approvals requiring action",
                counters.approvals,
            ),
            (
                "exceptions",
                "Open exceptions and incidents",
                counters.exceptions,
            ),
            ("overdue", "Items beyond SLA", counters.overdue),
        ]
        .into_iter()
        .map(|(key, label, value)| DashboardMetric {
            key: key.into(),
            label: label.into(),
            value: value.to_string(),
            unit: "items".into(),
            previous_period_delta: None,
            trend: Vec::new(),
            drill_target: None,
        })
        .collect()
    };
    let breakdowns = (successful_sources > 0)
        .then(|| DashboardBreakdown {
            key: "authorized_work_by_source".into(),
            label: "Authorized active work by source".into(),
            segments: sources
                .contributions
                .iter()
                .filter(|source| {
                    matches!(
                        source.availability,
                        ReadModelAvailability::Available | ReadModelAvailability::Empty
                    )
                })
                .map(|source| DashboardBreakdownSegment {
                    key: format!("{}:{}", source.domain.as_str(), source.label),
                    label: source.label.clone(),
                    value: f64::from(source.item_count),
                })
                .collect(),
        })
        .into_iter()
        .collect();
    DashboardReadModel {
        range,
        compared_to_previous,
        meta: ReadModelMeta {
            scope,
            as_of: as_of.to_rfc3339(),
            freshness: ReadModelFreshness::Live,
            availability,
        },
        metrics,
        breakdowns,
        work_items: Vec::new(),
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkQueueQuery {
    view: Option<String>,
    domain: Option<String>,
    severity: Option<String>,
    cursor: Option<String>,
    page_size: Option<u32>,
}

pub(crate) async fn work_queue_read_model(
    Extension(state): Extension<BffState>,
    Extension(context): Extension<BffSessionContext>,
    Query(query): Query<WorkQueueQuery>,
) -> Response {
    let Some(credential) = context.credential else {
        return (
            StatusCode::UNAUTHORIZED,
            "authenticated web session required",
        )
            .into_response();
    };
    let Some(scope_key) = context
        .bundle
        .as_ref()
        .and_then(WebSessionBundle::active_account)
        .and_then(|account| account.active_role.as_ref())
        .and_then(|role| role.scope_key.clone())
    else {
        return (StatusCode::FORBIDDEN, "active role scope required").into_response();
    };
    let view = match query.view.as_deref().unwrap_or("all") {
        value if WorkQueueView::parse(value).is_some() => WorkQueueView::parse(value).unwrap(),
        _ => return (StatusCode::BAD_REQUEST, "work queue view invalid").into_response(),
    };
    let domain = match query.domain.as_deref() {
        None | Some("") | Some("all") => None,
        Some(value) => match parse_work_domain(value) {
            Some(domain) => Some(domain),
            None => return (StatusCode::BAD_REQUEST, "work queue domain invalid").into_response(),
        },
    };
    let severity = match query.severity.as_deref() {
        None | Some("") | Some("all") => None,
        Some(value) => match parse_work_severity(value) {
            Some(severity) => Some(severity),
            None => {
                return (StatusCode::BAD_REQUEST, "work queue severity invalid").into_response();
            }
        },
    };
    let page_size = query.page_size.unwrap_or(24);
    if page_size == 0 || page_size > MAX_WORK_PAGE_SIZE {
        return (StatusCode::BAD_REQUEST, "work queue page size invalid").into_response();
    }
    let offset = match query.cursor.as_deref() {
        None | Some("") => 0,
        Some(value) if valid_work_cursor(value) => value
            .strip_prefix("offset:")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or_default(),
        Some(_) => return (StatusCode::BAD_REQUEST, "work queue cursor invalid").into_response(),
    };

    let now = Utc::now();
    let mut sources = fetch_platform_work_sources(&state, &credential, now).await;
    sources.items.sort_by(|left, right| {
        work_sla_rank(left.sla)
            .cmp(&work_sla_rank(right.sla))
            .then_with(|| left.severity.rank().cmp(&right.severity.rank()))
            .then_with(|| right.updated_at.cmp(&left.updated_at))
            .then_with(|| left.key.cmp(&right.key))
    });
    let counters = WorkQueueReadModel::counters_for(&sources.items, now);
    let principal = credential.principal_ref.as_str();
    let filtered = sources
        .items
        .into_iter()
        .filter(|item| domain.is_none_or(|domain| item.domain == domain))
        .filter(|item| severity.is_none_or(|severity| item.severity == severity))
        .filter(|item| match view {
            WorkQueueView::All => true,
            WorkQueueView::Mine => item.assignee.as_deref() == Some(principal),
            WorkQueueView::Team => item.assignee.as_deref() != Some(principal),
            WorkQueueView::Overdue => item.sla == WorkSlaState::Overdue,
            WorkQueueView::Approvals => item.kind == WorkItemKind::Approval,
        })
        .collect::<Vec<_>>();
    let has_more = offset.saturating_add(page_size as usize) < filtered.len();
    let items = filtered
        .into_iter()
        .skip(offset)
        .take(page_size as usize)
        .collect::<Vec<_>>();
    let available_sources = sources
        .contributions
        .iter()
        .filter(|source| {
            matches!(
                source.availability,
                ReadModelAvailability::Available | ReadModelAvailability::Empty
            )
        })
        .count();
    let all_sources_responded = sources.contributions.iter().all(|source| {
        matches!(
            source.availability,
            ReadModelAvailability::Available | ReadModelAvailability::Empty
        )
    });
    let availability = if available_sources == 0 {
        ReadModelAvailability::Unavailable
    } else if all_sources_responded {
        if counters.total == 0 {
            ReadModelAvailability::Empty
        } else {
            ReadModelAvailability::Available
        }
    } else {
        ReadModelAvailability::Partial
    };
    let model = WorkQueueReadModel {
        view,
        meta: WorkQueueMeta {
            scope: SafeUiText::new(scope_key).as_str().to_owned(),
            as_of: now.to_rfc3339(),
            freshness: if available_sources == 0 {
                ReadModelFreshness::Unknown
            } else {
                ReadModelFreshness::Current
            },
            availability,
        },
        counters,
        returned_count: items.len() as u32,
        items,
        contributions: sources.contributions,
        page_size,
        next_cursor: has_more.then(|| format!("offset:{}", offset + page_size as usize)),
    };
    if !model.is_bounded() {
        return unavailable();
    }
    private_json(model)
}

struct AuthorizedWorkSources {
    items: Vec<WorkItemReadModel>,
    contributions: Vec<WorkSourceContribution>,
}

async fn fetch_platform_work_sources(
    _state: &BffState,
    _credential: &BffCredential,
    _now: DateTime<Utc>,
) -> AuthorizedWorkSources {
    let mut contributions = Vec::new();
    for (domain, label) in [
        (WorkDomain::Support, "Platform incidents"),
        (WorkDomain::Support, "Support tickets"),
        (WorkDomain::Operations, "Operations exceptions"),
        (WorkDomain::Finance, "Finance close reviews"),
        (WorkDomain::People, "People approvals"),
        (WorkDomain::Maintenance, "Maintenance work orders"),
        (WorkDomain::Procurement, "Procurement approvals"),
    ] {
        contributions.push(unavailable_contribution(
            domain,
            label,
            "This source needs an explicit server-authorized tenant routing context.",
        ));
    }
    AuthorizedWorkSources {
        items: Vec::new(),
        contributions,
    }
}

#[cfg(test)]
fn work_stage(value: &str) -> WorkItemStage {
    match value.trim().to_ascii_lowercase().as_str() {
        "resolved" | "customer_confirmed" => WorkItemStage::Resolved,
        "closed" | "postmortem_complete" => WorkItemStage::Closed,
        "waiting" | "waiting_for_customer" | "access_requested" => WorkItemStage::Waiting,
        "in_progress" | "investigating" | "identified" | "monitoring" | "triaged" => {
            WorkItemStage::InProgress
        }
        _ => WorkItemStage::Open,
    }
}

#[cfg(test)]
fn work_severity(value: &str) -> WorkItemSeverity {
    match value.trim().to_ascii_lowercase().as_str() {
        "critical" | "sev0" | "sev1" => WorkItemSeverity::Critical,
        "high" | "sev2" => WorkItemSeverity::High,
        "medium" | "normal" | "sev3" => WorkItemSeverity::Medium,
        "low" | "sev4" => WorkItemSeverity::Low,
        _ => WorkItemSeverity::Informational,
    }
}

fn parse_work_domain(value: &str) -> Option<WorkDomain> {
    match value {
        "operations" => Some(WorkDomain::Operations),
        "support" => Some(WorkDomain::Support),
        "finance" => Some(WorkDomain::Finance),
        "people" => Some(WorkDomain::People),
        "maintenance" => Some(WorkDomain::Maintenance),
        "procurement" => Some(WorkDomain::Procurement),
        _ => None,
    }
}

fn parse_work_severity(value: &str) -> Option<WorkItemSeverity> {
    match value {
        "critical" => Some(WorkItemSeverity::Critical),
        "high" => Some(WorkItemSeverity::High),
        "medium" => Some(WorkItemSeverity::Medium),
        "low" => Some(WorkItemSeverity::Low),
        "informational" => Some(WorkItemSeverity::Informational),
        _ => None,
    }
}

fn work_sla_rank(value: WorkSlaState) -> u8 {
    match value {
        WorkSlaState::Overdue => 0,
        WorkSlaState::DueToday => 1,
        WorkSlaState::Healthy => 2,
        WorkSlaState::NoDueDate => 3,
    }
}

#[cfg(test)]
fn available_contribution(
    domain: WorkDomain,
    label: &str,
    item_count: usize,
) -> WorkSourceContribution {
    WorkSourceContribution {
        domain,
        label: label.into(),
        availability: if item_count == 0 {
            ReadModelAvailability::Empty
        } else {
            ReadModelAvailability::Available
        },
        freshness: ReadModelFreshness::Current,
        item_count: item_count as u32,
        message: if item_count == 0 {
            "The authoritative source returned no active work items.".into()
        } else {
            "Live authorized work items are included in this queue.".into()
        },
    }
}

fn unavailable_contribution(
    domain: WorkDomain,
    label: &str,
    message: &str,
) -> WorkSourceContribution {
    WorkSourceContribution {
        domain,
        label: label.into(),
        availability: ReadModelAvailability::Unavailable,
        freshness: ReadModelFreshness::Unknown,
        item_count: 0,
        message: message.into(),
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RotatedSession {
    principal_ref: String,
    session_id: String,
    access_token: String,
    refresh_token: String,
    access_expires_at: String,
    refresh_expires_at: String,
    #[serde(default)]
    active_role_source: Option<String>,
    #[serde(default)]
    active_role_selection_id: Option<String>,
    #[serde(default)]
    authorization_context_version: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PasswordLoginRequest {
    principal_ref: String,
    password: String,
    device_label: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DirectoryLoginRequest {
    hostname: String,
    username: String,
    password: String,
    device_label: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecoveryLoginRequest {
    principal_ref: String,
    recovery_code: String,
    new_password: String,
    device_label: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryStarted {
    recovery_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ActivateAccountRequest {
    session_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ActivateRoleRequest {
    selection_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectedRoleResponse {
    session_id: Uuid,
    active_role: SelectedRole,
    access_token: String,
    access_expires_at: String,
    authorization_context_version: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SelectedRole {
    selection_id: String,
    source: String,
    role_key: String,
    scope_key: String,
    expires_at: Option<String>,
    requires_mfa: bool,
}

#[derive(Debug, Serialize)]
struct SafeAccountSummary {
    session_id: Uuid,
    principal_ref: String,
    active: bool,
    selected_role: Option<SelectedRole>,
    refresh_expires_at: DateTime<Utc>,
    last_used_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct SafeAccountsResponse {
    accounts: Vec<SafeAccountSummary>,
}

pub(crate) async fn password_login(
    Extension(state): Extension<BffState>,
    Extension(context): Extension<BffSessionContext>,
    headers: HeaderMap,
    Json(request): Json<PasswordLoginRequest>,
) -> Response {
    exchange_password_login(&state, context, &headers, request, false).await
}

pub(crate) async fn password_login_form_entry() -> Response {
    see_other(crate::LOGIN_PATH)
}

pub(crate) async fn password_login_form(
    Extension(state): Extension<BffState>,
    Extension(context): Extension<BffSessionContext>,
    headers: HeaderMap,
    Form(request): Form<PasswordLoginRequest>,
) -> Response {
    let response = exchange_password_login(&state, context, &headers, request, true).await;
    password_form_response(response, &headers)
}

async fn exchange_password_login(
    state: &BffState,
    context: BffSessionContext,
    headers: &HeaderMap,
    request: PasswordLoginRequest,
    password_form_navigation: bool,
) -> Response {
    let trusted_origin = if password_form_navigation {
        trusted_password_form_submission(
            headers,
            &state.trusted_public_origins,
            state.production_profile,
        )
    } else {
        same_origin_mutation(headers, &state.trusted_public_origins)
    };
    if !trusted_origin
        || request.principal_ref.trim().is_empty()
        || request.principal_ref.len() > 256
        || request.password.is_empty()
        || request.password.len() > 1_024
        || request.device_label.trim().is_empty()
        || request.device_label.len() > 200
    {
        return invalid_request();
    }
    let path = if cfg!(debug_assertions) && request.principal_ref == "local:dev-controller" {
        "/dev-access/login"
    } else {
        "/auth/login"
    };
    let body = if path == "/dev-access/login" {
        serde_json::json!({
            "password": request.password,
            "device_label": request.device_label,
        })
    } else {
        serde_json::json!({
            "principal_ref": request.principal_ref,
            "password": request.password,
            "device_label": request.device_label,
        })
    };
    exchange_and_install(state, context, headers, path, body).await
}

fn password_form_response(login_response: Response, headers: &HeaderMap) -> Response {
    let return_to = headers
        .get(header::REFERER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| reqwest::Url::parse(value).ok())
        .and_then(|url| {
            let mut path_and_query = url.path().to_owned();
            if let Some(query) = url.query() {
                path_and_query.push('?');
                path_and_query.push_str(query);
            }
            crate::login_return_path_from_url(&path_and_query)
        });
    let location = return_to
        .map(|path| crate::login_redirect_location(&path))
        .unwrap_or_else(|| crate::LOGIN_PATH.to_owned());
    if !login_response.status().is_success() {
        let error = match login_response.status() {
            StatusCode::BAD_REQUEST => "invalid_request",
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => "authentication_rejected",
            _ => "authentication_unavailable",
        };
        let separator = if location.contains('?') { '&' } else { '?' };
        return see_other(&format!("{location}{separator}auth_error={error}"));
    }
    let cookies = login_response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    let mut response = see_other(&location);
    for cookie in cookies {
        response.headers_mut().append(header::SET_COOKIE, cookie);
    }
    response
}

pub(crate) async fn directory_login(
    Extension(state): Extension<BffState>,
    Extension(context): Extension<BffSessionContext>,
    headers: HeaderMap,
    Json(request): Json<DirectoryLoginRequest>,
) -> Response {
    if !same_origin_mutation(&headers, &state.trusted_public_origins)
        || !valid_hostname(&request.hostname)
        || request.username.trim().is_empty()
        || request.username.len() > 256
        || request.password.is_empty()
        || request.password.len() > 1_024
        || request.device_label.trim().is_empty()
        || request.device_label.len() > 200
    {
        return invalid_request();
    }
    exchange_and_install(
        &state,
        context,
        &headers,
        "/auth/directory-login",
        serde_json::json!({
            "hostname": request.hostname,
            "username": request.username,
            "password": request.password,
            "device_label": request.device_label,
        }),
    )
    .await
}

pub(crate) async fn passkey_authentication_begin(
    Extension(state): Extension<BffState>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if !same_origin_mutation(&headers, &state.trusted_public_origins)
        || body
            .get("principal_ref")
            .and_then(serde_json::Value::as_str)
            .is_none_or(|value| value.trim().is_empty() || value.len() > 256)
    {
        return invalid_request();
    }
    relay_public_auth_json(&state, "/auth/passkeys/authentication/begin", body).await
}

pub(crate) async fn passkey_authentication_finish(
    Extension(state): Extension<BffState>,
    Extension(context): Extension<BffSessionContext>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if !same_origin_mutation(&headers, &state.trusted_public_origins) || !body.is_object() {
        return invalid_request();
    }
    exchange_and_install(
        &state,
        context,
        &headers,
        "/auth/passkeys/authentication/finish",
        body,
    )
    .await
}

pub(crate) async fn recovery_login(
    Extension(state): Extension<BffState>,
    Extension(context): Extension<BffSessionContext>,
    headers: HeaderMap,
    Json(request): Json<RecoveryLoginRequest>,
) -> Response {
    if !same_origin_mutation(&headers, &state.trusted_public_origins)
        || request.principal_ref.trim().is_empty()
        || request.principal_ref.len() > 256
        || request.recovery_code.trim().is_empty()
        || request.recovery_code.len() > 256
        || !(12..=1_024).contains(&request.new_password.len())
        || request.device_label.trim().is_empty()
        || request.device_label.len() > 200
    {
        return invalid_request();
    }
    let response = match state
        .auth_http
        .post(format!(
            "{}/auth/recovery-sessions/initiate",
            state.auth_http_origin
        ))
        .json(&serde_json::json!({
            "principal_ref": request.principal_ref,
            "code": request.recovery_code,
        }))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => response,
        Ok(response) => return auth_error(response.status()),
        Err(_) => return unavailable(),
    };
    let started: RecoveryStarted = match response.json::<RecoveryStarted>().await {
        Ok(started) if !started.recovery_token.is_empty() => started,
        _ => return unavailable(),
    };
    exchange_and_install(
        &state,
        context,
        &headers,
        "/auth/recovery-sessions/reset-password",
        serde_json::json!({
            "recovery_token": started.recovery_token,
            "new_password": request.new_password,
            "device_label": request.device_label,
        }),
    )
    .await
}

pub(crate) async fn accounts(Extension(context): Extension<BffSessionContext>) -> Response {
    let Some(bundle) = context.bundle else {
        return (
            StatusCode::UNAUTHORIZED,
            "authenticated web session required",
        )
            .into_response();
    };
    private_json(SafeAccountsResponse {
        accounts: bundle
            .accounts
            .into_iter()
            .map(|account| {
                let selected_role = account.effective_active_role().cloned().and_then(|role| {
                    Some(SelectedRole {
                        selection_id: role.selection_id,
                        source: role.source,
                        role_key: role.role_key?,
                        scope_key: role.scope_key?,
                        expires_at: None,
                        requires_mfa: false,
                    })
                });
                SafeAccountSummary {
                    session_id: account.session_id,
                    principal_ref: account.principal_ref,
                    active: account.session_id == bundle.active_session_id,
                    selected_role,
                    refresh_expires_at: account.refresh_expires_at,
                    last_used_at: account.last_used_at,
                }
            })
            .collect(),
    })
}

pub(crate) async fn activate_account(
    Extension(state): Extension<BffState>,
    Extension(context): Extension<BffSessionContext>,
    headers: HeaderMap,
    Json(request): Json<ActivateAccountRequest>,
) -> Response {
    if !same_origin_mutation(&headers, &state.trusted_public_origins) {
        return invalid_request();
    }
    let Some(mut bundle) = context.bundle else {
        return (
            StatusCode::UNAUTHORIZED,
            "authenticated web session required",
        )
            .into_response();
    };
    if !bundle
        .accounts
        .iter()
        .any(|account| account.session_id == request.session_id)
    {
        return (StatusCode::NOT_FOUND, "account not found").into_response();
    }
    let expected = bundle.version;
    let position = bundle
        .accounts
        .iter()
        .position(|account| account.session_id == request.session_id)
        .unwrap_or_default();
    let mut activated = bundle.accounts.remove(position);
    activated.last_used_at = Utc::now();
    bundle.accounts.insert(0, activated);
    bundle.active_session_id = request.session_id;
    bundle.version += 1;
    bundle.updated_at = Utc::now();
    if state.sessions.replace(&bundle, expected).await.is_err() {
        return unavailable();
    }
    match SafeWebSessionSnapshot::from_bundle(&bundle) {
        Ok(snapshot) => private_json(snapshot),
        Err(_) => unavailable(),
    }
}

pub(crate) async fn delete_account(
    Extension(state): Extension<BffState>,
    Extension(context): Extension<BffSessionContext>,
    Path(session_id): Path<Uuid>,
    headers: HeaderMap,
) -> Response {
    if !same_origin_mutation(&headers, &state.trusted_public_origins) {
        return invalid_request();
    }
    let Some(selector) = selector_from_headers(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            "authenticated web session required",
        )
            .into_response();
    };
    let Some(mut bundle) = context.bundle else {
        return (
            StatusCode::UNAUTHORIZED,
            "authenticated web session required",
        )
            .into_response();
    };
    let Some(position) = bundle
        .accounts
        .iter()
        .position(|account| account.session_id == session_id)
    else {
        return (StatusCode::NOT_FOUND, "account not found").into_response();
    };
    let removed = bundle.accounts.remove(position);
    if bundle.accounts.is_empty() {
        if state.sessions.delete(selector.digest()).await.is_err() {
            return unavailable();
        }
        revoke_account_upstream(&state, &removed).await;
        let mut response = private_json(SafeWebSessionSnapshot::signed_out());
        clear_session_cookie(&mut response, request_is_https(&headers));
        return response;
    }
    if bundle.active_session_id == session_id {
        bundle.active_session_id = bundle.accounts[0].session_id;
    }
    let expected_version = bundle.version;
    let now = Utc::now();
    bundle.version += 1;
    bundle.updated_at = now;
    bundle.expires_at = bundle
        .accounts
        .iter()
        .map(|account| account.refresh_expires_at)
        .max()
        .unwrap_or(now);
    if state
        .sessions
        .replace(&bundle, expected_version)
        .await
        .is_err()
    {
        return unavailable();
    }
    revoke_account_upstream(&state, &removed).await;
    let snapshot = match SafeWebSessionSnapshot::from_bundle(&bundle) {
        Ok(snapshot) => snapshot,
        Err(_) => return unavailable(),
    };
    let mut response = private_json(snapshot);
    append_session_cookie(
        &mut response,
        &selector,
        bundle.expires_at,
        now,
        request_is_https(&headers),
    );
    response
}

pub(crate) async fn role_options(
    Extension(state): Extension<BffState>,
    Extension(context): Extension<BffSessionContext>,
) -> Response {
    relay_authenticated_auth(
        &state,
        &context,
        reqwest::Method::GET,
        "/auth/role-options",
        None,
    )
    .await
}

pub(crate) async fn activate_role(
    Extension(state): Extension<BffState>,
    Extension(context): Extension<BffSessionContext>,
    headers: HeaderMap,
    Json(request): Json<ActivateRoleRequest>,
) -> Response {
    if !same_origin_mutation(&headers, &state.trusted_public_origins)
        || request.selection_id.trim().is_empty()
        || request.selection_id.len() > 256
    {
        return invalid_request();
    }
    let Some(credential) = context.credential.as_ref() else {
        return (
            StatusCode::UNAUTHORIZED,
            "authenticated web session required",
        )
            .into_response();
    };
    let response = match state
        .auth_http
        .post(format!("{}/auth/role-selection", state.auth_http_origin))
        .header("x-erpx-access-token", credential.access_token.expose())
        .json(&serde_json::json!({
            "session_id": credential.session_id,
            "selection_id": request.selection_id,
        }))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => response,
        Ok(response) => return auth_error(response.status()),
        Err(_) => return unavailable(),
    };
    let selected: SelectedRoleResponse = match response.json().await {
        Ok(selected) => selected,
        Err(_) => return unavailable(),
    };
    let Some(mut bundle) = context.bundle else {
        return unavailable();
    };
    let expected = bundle.version;
    let Some(account) = bundle
        .accounts
        .iter_mut()
        .find(|account| account.session_id == credential.session_id)
    else {
        return unavailable();
    };
    if selected.session_id != account.session_id || selected.authorization_context_version == 0 {
        return unavailable();
    }
    let access_expires_at = match DateTime::parse_from_rfc3339(&selected.access_expires_at) {
        Ok(value) => value.with_timezone(&Utc),
        Err(_) => return unavailable(),
    };
    let access_token = match SensitiveToken::new(selected.access_token) {
        Ok(token) => token,
        Err(_) => return unavailable(),
    };
    account.access_token = access_token;
    account.access_expires_at = access_expires_at;
    account.active_role = Some(SafeActiveRole {
        source: selected.active_role.source,
        selection_id: selected.active_role.selection_id,
        role_key: Some(selected.active_role.role_key),
        scope_key: Some(selected.active_role.scope_key),
    });
    account.authorization_context_version = Some(selected.authorization_context_version);
    account.last_used_at = Utc::now();
    bundle.version += 1;
    bundle.updated_at = Utc::now();
    if state.sessions.replace(&bundle, expected).await.is_err() {
        return unavailable();
    }
    match SafeWebSessionSnapshot::from_bundle(&bundle) {
        Ok(snapshot) => private_json(snapshot),
        Err(_) => unavailable(),
    }
}

pub(crate) async fn delete_session(
    Extension(state): Extension<BffState>,
    Extension(context): Extension<BffSessionContext>,
    headers: HeaderMap,
) -> Response {
    if !same_origin_mutation(&headers, &state.trusted_public_origins) {
        return invalid_request();
    }
    if let Some(credential) = context.credential {
        let _ = state
            .auth_http
            .post(format!("{}/auth/revoke", state.auth_http_origin))
            .header("x-erpx-access-token", credential.access_token.expose())
            .json(&serde_json::json!({ "session_id": credential.session_id }))
            .send()
            .await;
    }
    if let Some(selector) = selector_from_headers(&headers) {
        let _ = state.sessions.delete(selector.digest()).await;
    }
    let mut response = private_json(SafeWebSessionSnapshot::signed_out());
    append_cookie(
        &mut response,
        &format!(
            "{WEB_SESSION_COOKIE_NAME}=; Path=/; Max-Age=0; HttpOnly; SameSite=Strict{}",
            if request_is_https(&headers) {
                "; Secure"
            } else {
                ""
            }
        ),
    );
    response
}

pub(crate) async fn identity_proxy(
    Extension(state): Extension<BffState>,
    Extension(context): Extension<BffSessionContext>,
    Path(identity_path): Path<String>,
    method: Method,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let Some(credential) = context.credential else {
        return (
            StatusCode::UNAUTHORIZED,
            "authenticated web session required",
        )
            .into_response();
    };
    if !identity_route_allowed(&method, &identity_path)
        || (method != Method::GET && !same_origin_mutation(&headers, &state.trusted_public_origins))
    {
        return invalid_request();
    }
    let body = match to_bytes(request.into_body(), 64 * 1024).await {
        Ok(body) => body,
        Err(_) => return invalid_request(),
    };
    let upstream_method = match reqwest::Method::from_bytes(method.as_str().as_bytes()) {
        Ok(method) => method,
        Err(_) => return invalid_request(),
    };
    let mut upstream = state
        .auth_http
        .request(
            upstream_method,
            format!("{}/auth/{}", state.auth_http_origin, identity_path),
        )
        .header("accept", "application/json")
        .header("x-erpx-access-token", credential.access_token.expose());
    if method != Method::GET {
        upstream = upstream
            .header(header::CONTENT_TYPE.as_str(), "application/json")
            .body(body.to_vec());
    }
    let upstream = match upstream.send().await {
        Ok(response) => response,
        Err(_) => return unavailable(),
    };
    let status = upstream.status();
    if !status.is_success() {
        return auth_error(status);
    }
    match upstream.json::<serde_json::Value>().await {
        Ok(value) => private_json(value),
        Err(_) => unavailable(),
    }
}

fn identity_route_allowed(method: &Method, path: &str) -> bool {
    matches!(
        (method, path),
        (&Method::GET, "devices")
            | (&Method::POST, "recovery-codes")
            | (&Method::POST, "recovery-codes/consume")
            | (&Method::POST, "totp/enrollment/start")
            | (&Method::POST, "totp/enrollment/verify")
            | (&Method::POST, "passkeys/registration/begin")
            | (&Method::POST, "passkeys/registration/finish")
    ) || (method == Method::POST
        && path
            .strip_prefix("devices/")
            .and_then(|value| value.strip_suffix("/revoke"))
            .is_some_and(|value| Uuid::parse_str(value).is_ok()))
}

pub(crate) async fn development_proxy(
    Extension(state): Extension<BffState>,
    Extension(context): Extension<BffSessionContext>,
    Path(development_path): Path<String>,
    method: Method,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let Some(credential) = context.credential else {
        return (
            StatusCode::UNAUTHORIZED,
            "authenticated web session required",
        )
            .into_response();
    };
    let allowed = matches!(
        (method.clone(), development_path.as_str()),
        (Method::GET, "personas") | (Method::POST, "context/begin") | (Method::POST, "context/end")
    );
    if !allowed
        || (method != Method::GET && !same_origin_mutation(&headers, &state.trusted_public_origins))
    {
        return invalid_request();
    }
    let body = match to_bytes(request.into_body(), 64 * 1024).await {
        Ok(body) => body,
        Err(_) => return invalid_request(),
    };
    let upstream_method = match reqwest::Method::from_bytes(method.as_str().as_bytes()) {
        Ok(method) => method,
        Err(_) => return invalid_request(),
    };
    let mut upstream = state
        .auth_http
        .request(
            upstream_method,
            format!("{}/dev-access/{}", state.auth_http_origin, development_path),
        )
        .header("accept", "application/json")
        .header("x-erpx-access-token", credential.access_token.expose())
        .header("x-erpx-actor", &credential.principal_ref)
        .header("x-erpx-session", credential.session_id.to_string());
    if let Some(context_token) = context
        .bundle
        .as_ref()
        .and_then(WebSessionBundle::active_account)
        .and_then(|account| account.development_context_token.as_ref())
    {
        upstream = upstream.header("x-erpx-dev-persona-context", context_token.expose());
    }
    if method != Method::GET {
        upstream = upstream
            .header(header::CONTENT_TYPE.as_str(), "application/json")
            .body(body.to_vec());
    }
    let upstream = match upstream.send().await {
        Ok(response) => response,
        Err(_) => return unavailable(),
    };
    let status = upstream.status();
    if !status.is_success() {
        return auth_error(status);
    }
    let mut value = match upstream.json::<serde_json::Value>().await {
        Ok(value) => value,
        Err(_) => return unavailable(),
    };
    if development_path == "personas" {
        return match safe_development_persona_list(value) {
            Some(personas) => private_json(personas),
            None => unavailable(),
        };
    }
    let development_context_token = if development_path == "context/begin" {
        let Some(token) = value
            .as_object_mut()
            .and_then(|object| object.remove("context_token"))
            .and_then(|value| value.as_str().map(str::to_owned))
            .and_then(|value| SensitiveToken::new(value).ok())
        else {
            return unavailable();
        };
        Some(token)
    } else if development_path == "context/end" {
        None
    } else {
        return private_json(value);
    };
    let development_active_role = if development_path == "context/begin" {
        let Some(persona) = value.get("persona") else {
            return unavailable();
        };
        let Some(selection_id) = persona
            .get("persona_key")
            .and_then(serde_json::Value::as_str)
        else {
            return unavailable();
        };
        let Some(role_key) = persona.get("role_key").and_then(serde_json::Value::as_str) else {
            return unavailable();
        };
        let Some(scope_key) = persona
            .get("organization_scope")
            .and_then(serde_json::Value::as_str)
        else {
            return unavailable();
        };
        let role = SafeActiveRole {
            source: "local".into(),
            selection_id: selection_id.into(),
            role_key: Some(role_key.into()),
            scope_key: Some(scope_key.into()),
        };
        if role.validate().is_err() {
            return unavailable();
        }
        Some(role)
    } else {
        None
    };
    let Some(mut bundle) = context.bundle else {
        return unavailable();
    };
    let expected_version = bundle.version;
    let Some(account) = bundle
        .accounts
        .iter_mut()
        .find(|account| account.session_id == credential.session_id)
    else {
        return unavailable();
    };
    account.development_context_token = development_context_token;
    account.development_active_role = development_active_role;
    bundle.version += 1;
    bundle.updated_at = Utc::now();
    if state
        .sessions
        .replace(&bundle, expected_version)
        .await
        .is_err()
    {
        return unavailable();
    }
    private_json(value)
}

fn safe_development_persona_list(value: serde_json::Value) -> Option<serde_json::Value> {
    const MAX_PERSONAS: usize = 256;
    const MAX_FIELD_BYTES: usize = 256;

    let values = value.as_array()?;
    if values.len() > MAX_PERSONAS {
        return None;
    }
    let mut safe = Vec::with_capacity(values.len());
    for value in values {
        let object = value.as_object()?;
        let string = |key: &str| {
            object
                .get(key)?
                .as_str()
                .filter(|value| {
                    !value.trim().is_empty()
                        && value.len() <= MAX_FIELD_BYTES
                        && !value.chars().any(char::is_control)
                })
                .map(str::to_owned)
        };
        let tenant_id = match object.get("tenant_id")? {
            serde_json::Value::Null => None,
            serde_json::Value::String(value)
                if !value.trim().is_empty()
                    && value.len() <= MAX_FIELD_BYTES
                    && !value.chars().any(char::is_control) =>
            {
                Some(value.clone())
            }
            _ => return None,
        };
        safe.push(serde_json::json!({
            "persona_key": string("persona_key")?,
            "principal_ref": string("principal_ref")?,
            "tenant_id": tenant_id,
            "organization_scope": string("organization_scope")?,
            "role_key": string("role_key")?,
            "feature": string("feature")?,
            "entitled": object.get("entitled")?.as_bool()?,
            "directory_enabled": object.get("directory_enabled")?.as_bool()?,
        }));
    }
    Some(serde_json::Value::Array(safe))
}

pub(crate) async fn public_branding_proxy(
    Extension(state): Extension<BffState>,
    headers: HeaderMap,
) -> Response {
    let request = forwarded_brand_request(
        state
            .auth_http
            .get(format!("{}/branding/v1/bootstrap", state.auth_http_origin))
            .header("accept", "application/json"),
        &headers,
    );
    relay_public_response(request, 32 * 1024).await
}

pub(crate) async fn public_brand_asset_proxy(
    Extension(state): Extension<BffState>,
    Path(kind): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Some(kind) = brand_asset_kind(&kind) else {
        return (StatusCode::NOT_FOUND, "brand asset not found").into_response();
    };
    let bootstrap = forwarded_brand_request(
        state
            .auth_http
            .get(format!("{}/branding/v1/bootstrap", state.auth_http_origin))
            .header("accept", "application/json"),
        &headers,
    )
    .send()
    .await;
    let identity = match bootstrap {
        Ok(response) if response.status().is_success() => response
            .json::<ResolvedCorporateIdentity>()
            .await
            .ok()
            .filter(|identity| identity.validate().is_ok())
            .unwrap_or_else(ResolvedCorporateIdentity::neutral_erpx),
        _ => ResolvedCorporateIdentity::neutral_erpx(),
    };
    let reference = match kind {
        BrandAssetKind::LogoMark => identity.assets.logo_mark.as_ref(),
        BrandAssetKind::Wordmark => identity.assets.wordmark.as_ref(),
        BrandAssetKind::Favicon => identity.assets.favicon.as_ref(),
        BrandAssetKind::LoginArtwork => identity.assets.login_artwork.as_ref(),
    };
    if let Some(reference) = reference {
        let upstream = forwarded_brand_request(
            state.auth_http.get(format!(
                "{}/branding/v1/assets/{}",
                state.auth_http_origin,
                brand_asset_kind_path(kind)
            )),
            &headers,
        )
        .header("accept", &reference.media_type)
        .send()
        .await;
        if let Ok(upstream) = upstream
            && upstream.status().is_success()
            && upstream
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.starts_with(&reference.media_type))
            && let Ok(bytes) = upstream.bytes().await
            && u64::try_from(bytes.len()).ok() == Some(reference.byte_length)
            && validate_brand_asset_bytes(kind, &reference.media_type, &bytes)
                .is_ok_and(|digest| digest.eq_ignore_ascii_case(&reference.sha256))
        {
            return public_brand_asset_response(
                bytes.to_vec(),
                &reference.media_type,
                &reference.sha256,
            );
        }
    }
    let (bytes, digest) = fallback_brand_svg(&identity);
    public_brand_asset_response(bytes, "image/svg+xml", &digest)
}

fn forwarded_brand_request(
    mut request: reqwest::RequestBuilder,
    headers: &HeaderMap,
) -> reqwest::RequestBuilder {
    if let Some(host) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .filter(|value| {
            !value.is_empty() && value.len() <= 260 && !value.chars().any(char::is_control)
        })
    {
        request = request.header(header::HOST.as_str(), host);
    }
    request
}

fn brand_asset_kind(value: &str) -> Option<BrandAssetKind> {
    match value {
        "logo_mark" => Some(BrandAssetKind::LogoMark),
        "wordmark" => Some(BrandAssetKind::Wordmark),
        "favicon" => Some(BrandAssetKind::Favicon),
        "login_artwork" => Some(BrandAssetKind::LoginArtwork),
        _ => None,
    }
}

const fn brand_asset_kind_path(kind: BrandAssetKind) -> &'static str {
    match kind {
        BrandAssetKind::LogoMark => "logo_mark",
        BrandAssetKind::Wordmark => "wordmark",
        BrandAssetKind::Favicon => "favicon",
        BrandAssetKind::LoginArtwork => "login_artwork",
    }
}

fn fallback_brand_svg(identity: &ResolvedCorporateIdentity) -> (Vec<u8>, String) {
    let monogram = identity
        .short_name
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_uppercase)
        .take(2)
        .collect::<String>();
    let monogram = if monogram.is_empty() { "E" } else { &monogram };
    let primary = identity.tokens.light.primary.as_str();
    let accent = identity.tokens.light.accent.as_str();
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" role="img" aria-label="Brand mark"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop stop-color="{primary}"/><stop offset="1" stop-color="{accent}"/></linearGradient></defs><rect width="64" height="64" rx="18" fill="url(#g)"/><text x="32" y="39" text-anchor="middle" font-family="system-ui,sans-serif" font-size="23" font-weight="700" fill="#fff">{monogram}</text></svg>"##
    );
    let digest = format!("{:x}", Sha256::digest(svg.as_bytes()));
    (svg.into_bytes(), digest)
}

fn public_brand_asset_response(bytes: Vec<u8>, media_type: &str, digest: &str) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, media_type)
        .header(header::CONTENT_LENGTH, bytes.len().to_string())
        .header(header::ETAG, format!("\"{digest}\""))
        .header(
            header::CACHE_CONTROL,
            "public, max-age=300, stale-if-error=86400",
        )
        .header("x-content-type-options", "nosniff")
        .header("content-security-policy", "default-src 'none'; sandbox")
        .header(header::VARY, "Host")
        .body(Body::from(bytes))
        .unwrap_or_else(|_| unavailable())
}

pub(crate) async fn public_marketing_proxy(
    Extension(state): Extension<BffState>,
    Path(public_path): Path<String>,
    method: Method,
    headers: HeaderMap,
    request: Request,
) -> Response {
    if !public_marketing_route_allowed(&method, &public_path)
        || (method == Method::POST
            && !same_origin_mutation(&headers, &state.trusted_public_origins))
    {
        return invalid_request();
    }
    let body = match to_bytes(request.into_body(), 64 * 1024).await {
        Ok(body) => body,
        Err(_) => return invalid_request(),
    };
    let upstream_method = if method == Method::GET {
        reqwest::Method::GET
    } else {
        reqwest::Method::POST
    };
    let mut upstream = state
        .auth_http
        .request(
            upstream_method,
            format!("{}/public/marketing/{public_path}", state.auth_http_origin),
        )
        .header("accept", "application/json");
    for name in [
        header::HOST.as_str(),
        "x-erpx-tenant-id",
        "idempotency-key",
        "x-erpx-preview-token",
    ] {
        if let Some(value) = headers.get(name).and_then(|value| value.to_str().ok())
            && value.len() <= 4_096
        {
            upstream = upstream.header(name, value);
        }
    }
    if method == Method::POST {
        upstream = upstream
            .header(header::CONTENT_TYPE.as_str(), "application/json")
            .body(body.to_vec());
    }
    relay_public_response(upstream, 64 * 1024).await
}

fn public_marketing_route_allowed(method: &Method, path: &str) -> bool {
    let segments = path.split('/').collect::<Vec<_>>();
    match segments.as_slice() {
        ["forms", id, "submissions"] if method == Method::POST => Uuid::parse_str(id).is_ok(),
        ["previews", id] if method == Method::GET => Uuid::parse_str(id).is_ok(),
        _ => false,
    }
}

async fn relay_public_response(request: reqwest::RequestBuilder, limit: usize) -> Response {
    let upstream = match request.send().await {
        Ok(response) => response,
        Err(_) => return unavailable(),
    };
    let status = upstream.status();
    let content_type = upstream.headers().get(header::CONTENT_TYPE).cloned();
    let body = match upstream.bytes().await {
        Ok(body) if body.len() <= limit => body,
        _ => return unavailable(),
    };
    let mut response = Response::builder()
        .status(status)
        .header(header::CACHE_CONTROL, PRIVATE_NO_STORE);
    if let Some(content_type) = content_type {
        response = response.header(header::CONTENT_TYPE, content_type);
    }
    response
        .body(Body::from(body))
        .unwrap_or_else(|_| unavailable())
}

async fn exchange_and_install(
    state: &BffState,
    context: BffSessionContext,
    headers: &HeaderMap,
    path: &str,
    body: serde_json::Value,
) -> Response {
    let response = match state
        .auth_http
        .post(format!("{}{path}", state.auth_http_origin))
        .json(&body)
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => response,
        Ok(response) => return auth_error(response.status()),
        Err(_) => return unavailable(),
    };
    let rotated: RotatedSession = match response.json().await {
        Ok(rotated) => rotated,
        Err(_) => return unavailable(),
    };
    let now = Utc::now();
    let account = match rotated_account(rotated, now) {
        Ok(account) => account,
        Err(()) => return unavailable(),
    };
    install_account(state, context.bundle, headers, account, now).await
}

async fn install_account(
    state: &BffState,
    existing: Option<WebSessionBundle>,
    headers: &HeaderMap,
    account: WebSessionAccount,
    now: DateTime<Utc>,
) -> Response {
    let (bundle, selector, removed_accounts) = match existing {
        Some(mut bundle) => {
            let expected_version = bundle.version;
            let mut removed_accounts = Vec::new();
            bundle.accounts.retain(|candidate| {
                if candidate.principal_ref == account.principal_ref {
                    removed_accounts.push(candidate.clone());
                    false
                } else {
                    true
                }
            });
            let active_session_id = account.session_id;
            bundle.accounts.insert(0, account);
            if bundle.accounts.len() > 5 {
                removed_accounts.extend(bundle.accounts.drain(5..));
            }
            bundle.active_session_id = active_session_id;
            bundle.version += 1;
            bundle.updated_at = now;
            bundle.expires_at = match bundle
                .accounts
                .iter()
                .map(|candidate| candidate.refresh_expires_at)
                .max()
            {
                Some(expires_at) => expires_at,
                None => return unavailable(),
            };
            if state
                .sessions
                .replace(&bundle, expected_version)
                .await
                .is_err()
            {
                return unavailable();
            }
            (bundle, None, removed_accounts)
        }
        None => {
            let selector = match WebSessionSelector::generate() {
                Ok(selector) => selector,
                Err(_) => return unavailable(),
            };
            let expires_at = account.refresh_expires_at;
            let bundle = WebSessionBundle {
                id: Uuid::new_v4(),
                selector_digest: selector.digest(),
                active_session_id: account.session_id,
                accounts: vec![account],
                version: 1,
                created_at: now,
                updated_at: now,
                expires_at,
            };
            if state.sessions.insert(&bundle).await.is_err() {
                return unavailable();
            }
            (bundle, Some(selector), Vec::new())
        }
    };
    for removed in &removed_accounts {
        revoke_account_upstream(state, removed).await;
    }
    let snapshot = match SafeWebSessionSnapshot::from_bundle(&bundle) {
        Ok(snapshot) => snapshot,
        Err(_) => return unavailable(),
    };
    let mut response = private_json(snapshot);
    if let Some(selector) = selector {
        append_session_cookie(
            &mut response,
            &selector,
            bundle.expires_at,
            now,
            request_is_https(headers),
        );
    }
    response
}

async fn revoke_account_upstream(state: &BffState, account: &WebSessionAccount) {
    let _ = state
        .auth_http
        .post(format!("{}/auth/revoke", state.auth_http_origin))
        .header("x-erpx-access-token", account.access_token.expose())
        .json(&serde_json::json!({ "session_id": account.session_id }))
        .send()
        .await;
}

async fn relay_public_auth_json(state: &BffState, path: &str, body: serde_json::Value) -> Response {
    let response = match state
        .auth_http
        .post(format!("{}{path}", state.auth_http_origin))
        .json(&body)
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => return unavailable(),
    };
    let status = response.status();
    if !status.is_success() {
        return auth_error(status);
    }
    match response.json::<serde_json::Value>().await {
        Ok(value) => private_json(value),
        Err(_) => unavailable(),
    }
}

async fn relay_authenticated_auth(
    state: &BffState,
    context: &BffSessionContext,
    method: reqwest::Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> Response {
    let Some(credential) = context.credential.as_ref() else {
        return (
            StatusCode::UNAUTHORIZED,
            "authenticated web session required",
        )
            .into_response();
    };
    let mut request = state
        .auth_http
        .request(method, format!("{}{path}", state.auth_http_origin))
        .header("x-erpx-access-token", credential.access_token.expose());
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = match request.send().await {
        Ok(response) => response,
        Err(_) => return unavailable(),
    };
    let status = response.status();
    if !status.is_success() {
        return auth_error(status);
    }
    match response.json::<serde_json::Value>().await {
        Ok(value) => private_json(value),
        Err(_) => unavailable(),
    }
}

fn private_json<T: Serialize>(value: T) -> Response {
    let mut response = Json(value).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(PRIVATE_NO_STORE),
    );
    response
}

fn invalid_request() -> Response {
    (StatusCode::BAD_REQUEST, "invalid BFF request").into_response()
}

fn auth_error(status: reqwest::StatusCode) -> Response {
    let status = match status.as_u16() {
        400 | 405 => StatusCode::BAD_REQUEST,
        401 => StatusCode::UNAUTHORIZED,
        403 => StatusCode::FORBIDDEN,
        404 => StatusCode::NOT_FOUND,
        409 => StatusCode::CONFLICT,
        429 => StatusCode::TOO_MANY_REQUESTS,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };
    let mut response = (status, "authentication request was not accepted").into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(PRIVATE_NO_STORE),
    );
    response
}

fn valid_hostname(value: &str) -> bool {
    let hostname = value.trim();
    !hostname.is_empty()
        && hostname.len() <= 253
        && hostname
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
        && hostname.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
        })
}

fn parse_trusted_public_origins(value: &str) -> Result<Arc<[Arc<str>]>, io::Error> {
    let mut origins = Vec::new();
    for candidate in value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let url = reqwest::Url::parse(candidate)
            .map_err(|_| io::Error::other("trusted Web origin is invalid"))?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
            || url.host_str().is_none()
        {
            return Err(io::Error::other("trusted Web origin is invalid"));
        }
        let origin: Arc<str> = Arc::from(url.origin().ascii_serialization());
        if !origins.iter().any(|existing| existing == &origin) {
            origins.push(origin);
        }
    }
    Ok(Arc::from(origins.into_boxed_slice()))
}

fn loopback_authority(authority: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(&format!("http://{authority}")) else {
        return false;
    };
    url.username().is_empty()
        && url.password().is_none()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
        && url.host_str().is_some_and(|host| {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|address| address.is_loopback())
        })
}

fn same_origin_mutation(headers: &HeaderMap, trusted_public_origins: &[Arc<str>]) -> bool {
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| reqwest::Url::parse(value).ok())
    else {
        return false;
    };
    let Some(origin_host) = origin.host_str() else {
        return false;
    };
    if !matches!(origin.scheme(), "http" | "https")
        || headers
            .get("x-forwarded-proto")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|scheme| !scheme.eq_ignore_ascii_case(origin.scheme()))
    {
        return false;
    }
    let origin_authority = match origin.port_or_known_default() {
        Some(port) if !matches!((origin.scheme(), port), ("http", 80) | ("https", 443)) => {
            format!("{origin_host}:{port}")
        }
        _ => origin_host.to_owned(),
    };
    if origin_authority.eq_ignore_ascii_case(host) {
        return true;
    }

    loopback_authority(host)
        && trusted_public_origins
            .iter()
            .any(|trusted| trusted.as_ref() == origin.origin().ascii_serialization())
}

fn trusted_password_form_submission(
    headers: &HeaderMap,
    trusted_public_origins: &[Arc<str>],
    production_profile: bool,
) -> bool {
    if same_origin_mutation(headers, trusted_public_origins) {
        return true;
    }
    // Dioxus' local full-stack proxy can turn a native form navigation into an opaque
    // `Origin: null` request to an ephemeral loopback authority. Keep that development-only
    // compatibility path closed unless browser-controlled Fetch Metadata still proves that the
    // request is a same-origin top-level document navigation.
    if production_profile || !cfg!(debug_assertions) {
        return false;
    }

    headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|origin| origin == "null")
        && headers
            .get(header::HOST)
            .and_then(|value| value.to_str().ok())
            .is_some_and(loopback_authority)
        && headers
            .get("sec-fetch-site")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|site| site.eq_ignore_ascii_case("same-origin"))
        && headers
            .get("sec-fetch-mode")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|mode| mode.eq_ignore_ascii_case("navigate"))
        && headers
            .get("sec-fetch-dest")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|destination| destination.eq_ignore_ascii_case("document"))
}

fn rotated_account(rotated: RotatedSession, now: DateTime<Utc>) -> Result<WebSessionAccount, ()> {
    let active_role = match (rotated.active_role_source, rotated.active_role_selection_id) {
        (Some(source), Some(selection_id)) => Some(SafeActiveRole {
            source,
            selection_id,
            role_key: None,
            scope_key: None,
        }),
        (None, None) => None,
        _ => return Err(()),
    };
    let account = WebSessionAccount {
        session_id: Uuid::parse_str(&rotated.session_id).map_err(|_| ())?,
        principal_ref: rotated.principal_ref,
        access_token: SensitiveToken::new(rotated.access_token).map_err(|_| ())?,
        refresh_token: SensitiveToken::new(rotated.refresh_token).map_err(|_| ())?,
        development_context_token: None,
        development_active_role: None,
        access_expires_at: DateTime::parse_from_rfc3339(&rotated.access_expires_at)
            .map_err(|_| ())?
            .with_timezone(&Utc),
        refresh_expires_at: DateTime::parse_from_rfc3339(&rotated.refresh_expires_at)
            .map_err(|_| ())?
            .with_timezone(&Utc),
        last_used_at: now,
        active_role,
        authorization_context_version: rotated.authorization_context_version,
    };
    account.validate().map_err(|_| ())?;
    Ok(account)
}

fn preserve_refreshed_active_role(refreshed: &mut WebSessionAccount, current: &WebSessionAccount) {
    match (&mut refreshed.active_role, current.active_role.as_ref()) {
        (None, Some(current_role)) => {
            refreshed.active_role = Some(current_role.clone());
        }
        (Some(refreshed_role), Some(current_role))
            if refreshed_role.source == current_role.source
                && refreshed_role.selection_id == current_role.selection_id
                && refreshed_role.role_key.is_none()
                && refreshed_role.scope_key.is_none() =>
        {
            // The refresh contract carries the selected role identity but not the
            // credential-free role/scope labels used by the BFF. Retain those safe
            // labels only when the upstream role identity is unchanged.
            refreshed_role.role_key.clone_from(&current_role.role_key);
            refreshed_role.scope_key.clone_from(&current_role.scope_key);
        }
        _ => {}
    }
}

fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|part| {
            let (candidate, value) = part.trim().split_once('=')?;
            (candidate == name).then_some(value)
        })
}

fn request_is_https(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("https"))
}

fn append_cookie(response: &mut Response, value: &str) {
    if let Ok(value) = HeaderValue::from_str(value) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
}

fn clear_session_cookie(response: &mut Response, secure: bool) {
    append_cookie(
        response,
        &format!(
            "{WEB_SESSION_COOKIE_NAME}=; Path=/; Max-Age=0; HttpOnly; SameSite=Strict{}",
            if secure { "; Secure" } else { "" }
        ),
    );
}

fn append_session_cookie(
    response: &mut Response,
    selector: &WebSessionSelector,
    expires_at: DateTime<Utc>,
    now: DateTime<Utc>,
    secure: bool,
) {
    append_cookie(
        response,
        &format!(
            "{WEB_SESSION_COOKIE_NAME}={}; Path=/; Max-Age={}; HttpOnly; SameSite=Strict{}",
            URL_SAFE_NO_PAD.encode(selector.as_bytes()),
            expires_at.signed_duration_since(now).num_seconds().max(0),
            if secure { "; Secure" } else { "" }
        ),
    );
}

fn selector_from_headers(headers: &HeaderMap) -> Option<WebSessionSelector> {
    let encoded = cookie_value(headers, WEB_SESSION_COOKIE_NAME)?;
    let decoded = URL_SAFE_NO_PAD.decode(encoded).ok()?;
    let bytes: [u8; WEB_SESSION_SELECTOR_BYTES] = decoded.try_into().ok()?;
    Some(WebSessionSelector::from_bytes(bytes))
}

fn unavailable() -> Response {
    Response::builder()
        .status(StatusCode::SERVICE_UNAVAILABLE)
        .header(header::CACHE_CONTROL, PRIVATE_NO_STORE)
        .body(Body::from("web session unavailable"))
        .unwrap_or_else(|_| StatusCode::SERVICE_UNAVAILABLE.into_response())
}

fn unauthorized_session() -> Response {
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(header::CACHE_CONTROL, PRIVATE_NO_STORE)
        .body(Body::from("web session expired"))
        .unwrap_or_else(|_| StatusCode::UNAUTHORIZED.into_response())
}

fn unauthorized_session_with_cleared_cookie(secure: bool) -> Response {
    let mut response = unauthorized_session();
    clear_session_cookie(&mut response, secure);
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use erpx_ui_core::{WorkItemTarget, WorkRoute};

    #[test]
    fn selector_cookie_is_strictly_parsed() {
        let selector = [9_u8; WEB_SESSION_SELECTOR_BYTES];
        let encoded = URL_SAFE_NO_PAD.encode(selector);
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("theme=dark; {WEB_SESSION_COOKIE_NAME}={encoded}"))
                .unwrap(),
        );
        assert_eq!(
            selector_from_headers(&headers).unwrap().as_bytes(),
            &selector
        );

        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("erpx_web_session_v2=short"),
        );
        assert!(selector_from_headers(&headers).is_none());
    }

    #[test]
    fn unreadable_stored_session_requires_reauthentication_instead_of_reporting_an_outage() {
        assert_eq!(
            classify_session_load_error(WebSessionError::Protection),
            SessionLoadError::Unauthorized
        );
        assert_eq!(
            classify_session_load_error(WebSessionError::Invalid),
            SessionLoadError::Unauthorized
        );
        assert_eq!(
            classify_session_load_error(WebSessionError::Unavailable),
            SessionLoadError::Unavailable
        );
    }

    #[test]
    fn stale_api_session_response_clears_the_browser_cookie() {
        let response = unauthorized_session_with_cleared_cookie(false);

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers().get(header::SET_COOKIE).unwrap(),
            "erpx_web_session_v2=; Path=/; Max-Age=0; HttpOnly; SameSite=Strict"
        );
    }

    #[test]
    fn development_persona_proxy_exposes_only_ui_safe_fields() {
        let safe = safe_development_persona_list(serde_json::json!([{
            "persona_key": "demo:finance_operator",
            "principal_ref": "dev:demo:finance_operator",
            "directory_username": "must-not-cross-the-bff@example.test",
            "tenant_id": "demo",
            "organization_scope": "branch:00000000-0000-0000-0000-000000000001:00000000-0000-0000-0000-000000000002:00000000-0000-0000-0000-000000000003",
            "role_key": "finance_operator",
            "feature": "finance",
            "permissions": ["finance.read", "finance.write"],
            "entitled": true,
            "directory_enabled": true,
            "requires_totp_for_high_risk": false
        }]))
        .expect("safe persona list");
        let encoded = serde_json::to_string(&safe).unwrap();
        assert!(encoded.contains("finance_operator"));
        assert!(!encoded.contains("directory_username"));
        assert!(!encoded.contains("permissions"));
        assert!(!encoded.contains("requires_totp"));
    }

    #[test]
    fn refresh_preserves_scope_metadata_for_the_unchanged_active_role() {
        let now = Utc::now();
        let session_id = Uuid::new_v4().to_string();
        let rotated = |selection_id: &str| RotatedSession {
            principal_ref: "local:platform-admin".into(),
            session_id: session_id.clone(),
            access_token: "refreshed-access-token".into(),
            refresh_token: "refreshed-refresh-token".into(),
            access_expires_at: (now + Duration::minutes(10)).to_rfc3339(),
            refresh_expires_at: (now + Duration::hours(8)).to_rfc3339(),
            active_role_source: Some("local".into()),
            active_role_selection_id: Some(selection_id.into()),
            authorization_context_version: Some(2),
        };
        let mut current = rotated_account(rotated("platform-admin"), now).unwrap();
        current.active_role = Some(SafeActiveRole {
            source: "local".into(),
            selection_id: "platform-admin".into(),
            role_key: Some("platform_admin".into()),
            scope_key: Some("platform".into()),
        });

        let mut refreshed = rotated_account(rotated("platform-admin"), now).unwrap();
        preserve_refreshed_active_role(&mut refreshed, &current);
        let refreshed_role = refreshed.active_role.expect("unchanged active role");
        assert_eq!(refreshed_role.role_key.as_deref(), Some("platform_admin"));
        assert_eq!(refreshed_role.scope_key.as_deref(), Some("platform"));

        let mut changed = rotated_account(rotated("holding-owner"), now).unwrap();
        preserve_refreshed_active_role(&mut changed, &current);
        let changed_role = changed.active_role.expect("changed role identity");
        assert!(changed_role.role_key.is_none());
        assert!(changed_role.scope_key.is_none());
    }

    #[test]
    fn console_pages_redirect_to_the_canonical_login_route() {
        let root = Request::builder().uri("/").body(Body::empty()).unwrap();
        assert!(is_console_page_request(&root));
        assert_eq!(
            console_page_redirect(&root, &BffSessionContext::signed_out()),
            Some("/auth/sign-in?return_to=%2Fwork".to_owned())
        );

        let protected = Request::builder()
            .uri("/finance?ignored=1")
            .body(Body::empty())
            .unwrap();
        assert!(is_console_page_request(&protected));
        assert_eq!(
            console_page_redirect(&protected, &BffSessionContext::signed_out()),
            Some("/auth/sign-in?return_to=%2Ffinance".to_owned())
        );

        let legacy = Request::builder()
            .uri("/login")
            .body(Body::empty())
            .unwrap();
        assert!(!is_console_page_request(&legacy));
        assert_eq!(
            console_page_redirect(&legacy, &BffSessionContext::signed_out()),
            None
        );
    }

    #[test]
    fn document_route_status_distinguishes_registered_and_unknown_paths() {
        let registered = Request::builder()
            .uri("/finance")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            console_document_route_status(&registered),
            Some(StatusCode::OK)
        );

        let legacy = Request::builder()
            .uri("/platform/tenants/demo-tenant/workspace/finance")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            console_document_route_status(&legacy),
            Some(StatusCode::NOT_FOUND)
        );

        let missing = Request::builder()
            .uri("/this-route-does-not-exist")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            console_document_route_status(&missing),
            Some(StatusCode::NOT_FOUND)
        );

        let asset = Request::builder()
            .uri("/assets/console.css")
            .body(Body::empty())
            .unwrap();
        assert_eq!(console_document_route_status(&asset), None);
    }

    #[test]
    fn authenticated_login_request_returns_to_a_validated_console_page() {
        let request = Request::builder()
            .uri("/auth/sign-in?return_to=/finance")
            .body(Body::empty())
            .unwrap();
        let mut context = BffSessionContext::signed_out();
        context.snapshot.authenticated = true;
        assert_eq!(
            console_page_redirect(&request, &context),
            Some("/finance".to_owned())
        );

        context.snapshot.role_selection_required = true;
        assert_eq!(console_page_redirect(&request, &context), None);

        let root = Request::builder().uri("/").body(Body::empty()).unwrap();
        assert_eq!(
            console_page_redirect(&root, &context),
            Some("/auth/accounts".to_owned())
        );

        let protected = Request::builder().uri("/work").body(Body::empty()).unwrap();
        assert_eq!(
            console_page_redirect(&protected, &context),
            Some("/auth/accounts".to_owned())
        );
    }

    #[test]
    fn native_password_form_redirect_preserves_cookie_and_safe_return_path() {
        let mut login_response = StatusCode::OK.into_response();
        append_cookie(
            &mut login_response,
            "erpx_web_session_v2=opaque; Path=/; HttpOnly; SameSite=Strict",
        );
        let mut headers = HeaderMap::new();
        headers.insert(
            header::REFERER,
            HeaderValue::from_static("https://console.example/auth/sign-in?return_to=/finance"),
        );

        let response = password_form_response(login_response, &headers);

        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap(),
            "/auth/sign-in?return_to=%2Ffinance"
        );
        assert_eq!(
            response.headers().get(header::SET_COOKIE).unwrap(),
            "erpx_web_session_v2=opaque; Path=/; HttpOnly; SameSite=Strict"
        );

        headers.insert(
            header::REFERER,
            HeaderValue::from_static(
                "https://console.example/auth/sign-in?return_to=//attacker.example",
            ),
        );
        let response = password_form_response(StatusCode::OK.into_response(), &headers);
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap(),
            "/auth/sign-in"
        );

        headers.insert(
            header::REFERER,
            HeaderValue::from_static("https://console.example/auth/sign-in?return_to=/finance"),
        );
        let response = password_form_response(StatusCode::BAD_REQUEST.into_response(), &headers);
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap(),
            "/auth/sign-in?return_to=%2Ffinance&auth_error=invalid_request"
        );
        assert!(response.headers().get(header::SET_COOKIE).is_none());

        let response =
            password_form_response(StatusCode::UNAUTHORIZED.into_response(), &HeaderMap::new());
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap(),
            "/auth/sign-in?auth_error=authentication_rejected"
        );
    }

    #[test]
    fn mutation_origin_must_match_host_and_forwarded_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::HOST,
            HeaderValue::from_static("console.example:8443"),
        );
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://console.example:8443"),
        );
        headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));
        assert!(same_origin_mutation(&headers, &[]));

        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://attacker.example:8443"),
        );
        assert!(!same_origin_mutation(&headers, &[]));
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://console.example:8443"),
        );
        assert!(!same_origin_mutation(&headers, &[]));
    }

    #[test]
    fn mutation_origin_accepts_only_configured_origin_through_loopback_dev_proxy() {
        let trusted =
            parse_trusted_public_origins("http://100.97.9.56:3004,http://127.0.0.1:3004").unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("127.0.0.1:60805"));
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://100.97.9.56:3004"),
        );
        assert!(same_origin_mutation(&headers, &trusted));

        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://attacker.example"),
        );
        assert!(!same_origin_mutation(&headers, &trusted));

        headers.insert(
            header::HOST,
            HeaderValue::from_static("internal-proxy.example:60805"),
        );
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://100.97.9.56:3004"),
        );
        assert!(!same_origin_mutation(&headers, &trusted));
    }

    #[test]
    fn development_password_form_accepts_only_browser_same_origin_null_origin_navigation() {
        let trusted = parse_trusted_public_origins("http://localhost:3004").unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("127.0.0.1:60805"));
        headers.insert(header::ORIGIN, HeaderValue::from_static("null"));
        headers.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
        headers.insert("sec-fetch-mode", HeaderValue::from_static("navigate"));
        headers.insert("sec-fetch-dest", HeaderValue::from_static("document"));

        assert_eq!(
            trusted_password_form_submission(&headers, &trusted, false),
            cfg!(debug_assertions)
        );
        assert!(!trusted_password_form_submission(&headers, &trusted, true));

        headers.insert("sec-fetch-site", HeaderValue::from_static("cross-site"));
        assert!(!trusted_password_form_submission(&headers, &trusted, false));
        headers.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
        headers.insert(
            header::HOST,
            HeaderValue::from_static("internal.example:60805"),
        );
        assert!(!trusted_password_form_submission(&headers, &trusted, false));
    }

    #[test]
    fn trusted_public_origins_reject_non_origin_urls() {
        assert!(parse_trusted_public_origins("https://console.example/login").is_err());
        assert!(parse_trusted_public_origins("https://user@console.example").is_err());
        assert!(parse_trusted_public_origins("*").is_err());
    }

    #[test]
    fn production_profile_requires_https_and_a_bounded_public_origin() {
        let secure = parse_trusted_public_origins("https://console.example").unwrap();
        assert!(
            validate_web_runtime_profile(
                "https://identity.internal",
                "https://grpc.internal",
                &secure,
                true,
            )
            .is_ok()
        );
        let insecure = parse_trusted_public_origins("http://console.example").unwrap();
        assert!(
            validate_web_runtime_profile(
                "https://identity.internal",
                "https://grpc.internal",
                &insecure,
                true,
            )
            .is_err()
        );
        assert!(
            validate_web_runtime_profile(
                "http://identity.internal",
                "https://grpc.internal",
                &secure,
                true,
            )
            .is_err()
        );
        assert!(validate_web_runtime_profile("http://local", "http://local", &[], false).is_ok());
    }

    #[test]
    fn retained_identity_and_public_routes_are_closed_compile_time_allowlists() {
        assert!(identity_route_allowed(&Method::GET, "devices"));
        assert!(identity_route_allowed(
            &Method::POST,
            "passkeys/registration/finish"
        ));
        assert!(!identity_route_allowed(&Method::GET, "sessions"));
        assert!(!identity_route_allowed(&Method::POST, "tokens/export"));
        assert!(public_marketing_route_allowed(
            &Method::POST,
            &format!("forms/{}/submissions", Uuid::new_v4())
        ));
        assert!(!public_marketing_route_allowed(
            &Method::POST,
            "forms/not-a-uuid/submissions"
        ));
    }

    #[test]
    fn web_session_cookie_is_http_only_strict_and_secure_on_https() {
        let selector = WebSessionSelector::from_bytes([7; WEB_SESSION_SELECTOR_BYTES]);
        let now = Utc::now();
        let mut response = StatusCode::OK.into_response();
        append_session_cookie(
            &mut response,
            &selector,
            now + Duration::hours(1),
            now,
            true,
        );
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|value| value.to_str().ok())
            .unwrap();
        assert!(cookie.starts_with("erpx_web_session_v2="));
        assert!(cookie.contains("Path=/"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(cookie.contains("Secure"));
        assert!(!cookie.contains("access"));
        assert!(!cookie.contains("refresh"));
    }

    #[test]
    fn web_security_headers_are_closed_and_hsts_is_https_only() {
        let mut https_response = StatusCode::OK.into_response();
        apply_web_security_headers(&mut https_response, true);
        let headers = https_response.headers();
        assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
        assert_eq!(headers.get("x-frame-options").unwrap(), "DENY");
        assert_eq!(headers.get("referrer-policy").unwrap(), "no-referrer");
        assert_eq!(
            headers.get("cross-origin-opener-policy").unwrap(),
            "same-origin"
        );
        let csp = headers
            .get("content-security-policy")
            .and_then(|value| value.to_str().ok())
            .unwrap();
        assert!(csp.contains("default-src 'self'"));
        assert!(csp.contains("frame-ancestors 'none'"));
        // Dioxus Web's interpreter compiles framework-owned evaluator snippets with
        // `Function`; blocking `unsafe-eval` prevents hydration before the session
        // request can run. External script origins remain closed by `script-src`.
        assert!(csp.contains("'unsafe-eval'"));
        assert!(csp.contains("'wasm-unsafe-eval'"));
        assert!(headers.contains_key("strict-transport-security"));

        let mut http_response = StatusCode::OK.into_response();
        apply_web_security_headers(&mut http_response, false);
        assert!(
            !http_response
                .headers()
                .contains_key("strict-transport-security")
        );
    }

    #[test]
    fn dashboard_read_model_never_converts_navigation_into_business_kpis() {
        let groups = vec![
            NavGroup::new(
                "Finance",
                vec![
                    erpx_ui_core::NavItem::new("Customers", "/platform/customers", "customers")
                        .with_action_capabilities(vec!["customer.read".to_owned()]),
                    erpx_ui_core::NavItem::new("Unknown", "/internal/raw", "unknown"),
                ],
            ),
            NavGroup::new(
                "access_token=must-not-render",
                vec![erpx_ui_core::NavItem::new(
                    "Marketing",
                    "/marketing",
                    "marketing",
                )],
            ),
        ];
        let sources = AuthorizedWorkSources {
            items: Vec::new(),
            contributions: vec![unavailable_contribution(
                WorkDomain::Finance,
                "Finance projection",
                "not available",
            )],
        };
        let model = dashboard_from_authorized_sources(
            DashboardRange::ThirtyDays,
            true,
            "platform".to_owned(),
            &groups,
            &sources,
            Utc::now(),
        );

        assert!(model.is_bounded());
        assert_eq!(model.meta.availability, ReadModelAvailability::Unavailable);
        assert!(model.metrics.is_empty());
        assert!(model.work_items.is_empty());
        assert!(model.breakdowns.is_empty());
        let encoded = serde_json::to_string(&model).expect("dashboard JSON");
        assert!(!encoded.contains("must-not-render"));
        assert!(!encoded.contains("/internal/raw"));
    }

    #[test]
    fn dashboard_kpis_are_derived_only_from_authoritative_source_items() {
        let item = WorkItemReadModel {
            key: "operations:exception-1".into(),
            domain: WorkDomain::Operations,
            kind: WorkItemKind::Exception,
            title: "Inventory allocation exception".into(),
            summary: "Requires an authorized operator".into(),
            stage: WorkItemStage::Open,
            status_label: "open".into(),
            severity: WorkItemSeverity::High,
            assignee: None,
            due_at: None,
            updated_at: Some(Utc::now().to_rfc3339()),
            sla: WorkSlaState::Overdue,
            freshness: ReadModelFreshness::Current,
            target: WorkItemTarget {
                route: WorkRoute::OperationsExceptions,
                record_ref: Some("exception-1".into()),
            },
            action_capabilities: vec!["operations.exception.read".into()],
        };
        let sources = AuthorizedWorkSources {
            items: vec![item],
            contributions: vec![available_contribution(
                WorkDomain::Operations,
                "Operations exceptions",
                1,
            )],
        };
        let groups = vec![NavGroup::new(
            "Operations",
            vec![erpx_ui_core::NavItem::new(
                "Operations",
                "/operations",
                "operations",
            )],
        )];
        let model = dashboard_from_authorized_sources(
            DashboardRange::ThirtyDays,
            false,
            "holding:test".into(),
            &groups,
            &sources,
            Utc::now(),
        );

        assert_eq!(model.meta.availability, ReadModelAvailability::Available);
        assert_eq!(model.metrics.len(), 4);
        assert_eq!(model.metrics[0].value, "1");
        assert_eq!(model.metrics[2].value, "1");
        assert_eq!(model.metrics[3].value, "1");
        assert_eq!(model.breakdowns[0].segments[0].value, 1.0);
    }

    #[test]
    fn work_queue_normalizes_only_active_stages_and_known_severity() {
        assert_eq!(work_stage("waiting_for_customer"), WorkItemStage::Waiting);
        assert_eq!(work_stage("postmortem_complete"), WorkItemStage::Closed);
        assert!(!work_stage("closed").active());
        assert_eq!(work_severity("sev1"), WorkItemSeverity::Critical);
        assert_eq!(
            work_severity("unpublished"),
            WorkItemSeverity::Informational
        );
        assert_eq!(
            WorkRoute::OperationsExceptions.path(),
            "/operations/exceptions"
        );
    }

    #[test]
    fn cached_navigation_clients_default_to_authorized_only_manifests() {
        let legacy: NavigationRequest =
            serde_json::from_str(r#"{"scope_key":"platform"}"#).expect("legacy request");
        assert!(!legacy.include_inaccessible);

        let discovery: NavigationRequest =
            serde_json::from_str(r#"{"scope_key":"platform","include_inaccessible":true}"#)
                .expect("discovery request");
        assert!(discovery.include_inaccessible);
    }

    #[test]
    fn brand_asset_endpoint_is_allowlisted_hashed_and_never_embeds_object_keys() {
        assert_eq!(
            brand_asset_kind("logo_mark"),
            Some(BrandAssetKind::LogoMark)
        );
        assert_eq!(brand_asset_kind("../../secret"), None);
        let mut identity = ResolvedCorporateIdentity::neutral_erpx();
        identity.short_name = "ไทย ERPX".to_owned();
        let (bytes, digest) = fallback_brand_svg(&identity);
        let svg = String::from_utf8(bytes).expect("fallback SVG");
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("aria-label=\"Brand mark\""));
        assert_eq!(digest.len(), 64);
        assert!(!svg.contains("object_key"));
        assert!(!svg.contains("branding/"));
    }
}
