//! Web-only BFF boundary.
//!
//! This module may call application ports and native Tonic clients. Persistence
//! adapters remain in the server composition root.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use dioxus::server::axum::{
    Extension, Json,
    body::{Body, to_bytes},
    extract::{Path, Request},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use erpx_web_session::{
    SafeActiveRole, SafeWebSessionSnapshot, SensitiveToken, WEB_SESSION_COOKIE_NAME,
    WEB_SESSION_SELECTOR_BYTES, WebSessionAccount, WebSessionBundle, WebSessionError,
    WebSessionSelector, WebSessionStore,
};
use serde::{Deserialize, Serialize};
use std::{fmt, io, sync::Arc};
use tokio::sync::Mutex;
use uuid::Uuid;

const PRIVATE_NO_STORE: &str = "private, no-store";
const LEGACY_BUNDLE_COOKIE_NAME: &str = "erpx_session_bundle_v2";
const MAX_LEGACY_COOKIE_BYTES: usize = 3_800;

#[derive(Clone)]
pub(crate) struct BffState {
    sessions: Arc<dyn WebSessionStore>,
    auth_http_origin: Arc<str>,
    grpc_origin: Arc<str>,
    auth_http: reqwest::Client,
    refresh_lock: Arc<Mutex<()>>,
}

impl BffState {
    pub(crate) fn new(
        sessions: Arc<dyn WebSessionStore>,
        auth_http_origin: String,
        grpc_origin: String,
    ) -> Result<Self, io::Error> {
        let auth_http_origin = auth_http_origin.trim_end_matches('/');
        let grpc_origin = grpc_origin.trim_end_matches('/');
        if !(auth_http_origin.starts_with("http://") || auth_http_origin.starts_with("https://"))
            || !(grpc_origin.starts_with("http://") || grpc_origin.starts_with("https://"))
        {
            return Err(io::Error::other("identity HTTP origin is invalid"));
        }
        Ok(Self {
            sessions,
            auth_http_origin: Arc::from(auth_http_origin),
            grpc_origin: Arc::from(grpc_origin),
            auth_http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| io::Error::other("identity HTTP client is unavailable"))?,
            refresh_lock: Arc::new(Mutex::new(())),
        })
    }
}

impl fmt::Debug for BffState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BffState")
            .field("sessions", &"[configured]")
            .field("auth_http_origin", &self.auth_http_origin)
            .field("grpc_origin", &self.grpc_origin)
            .finish()
    }
}

#[derive(Clone)]
pub(crate) struct BffCredential {
    pub(crate) session_id: Uuid,
    pub(crate) principal_ref: String,
    pub(crate) access_token: SensitiveToken,
}

impl fmt::Debug for BffCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BffCredential")
            .field("session_id", &self.session_id)
            .field("principal_ref", &self.principal_ref)
            .field("access_token", &"[redacted]")
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
    let page_request = is_console_page_request(&request);
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
                    }),
                    bundle: Some(bundle),
                }
            }
            Ok(None) => BffSessionContext::signed_out(),
            Err(SessionLoadError::Unauthorized) if page_request => {
                clear_stale_cookie = true;
                BffSessionContext::signed_out()
            }
            Err(SessionLoadError::Unauthorized) => return unauthorized_session(),
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
    let private_response = context.snapshot.authenticated;
    let secure_cookie = request_is_https(request.headers());
    request.extensions_mut().insert(context);
    let mut response = next.run(request).await;
    if clear_stale_cookie {
        clear_session_cookie(&mut response, secure_cookie);
    }
    if private_response {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static(PRIVATE_NO_STORE),
        );
    }
    response
}

fn is_console_page_request(request: &Request) -> bool {
    (request.method() == Method::GET || request.method() == Method::HEAD)
        && (request.uri().path() == crate::LOGIN_PATH
            || crate::is_legacy_login_path(request.uri().path())
            || crate::validated_login_return_path(request.uri().path()).is_some())
}

fn console_page_redirect(request: &Request, context: &BffSessionContext) -> Option<String> {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return None;
    }
    let path = request.uri().path();
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

#[derive(Clone, Copy)]
enum SessionLoadError {
    Unauthorized,
    Unavailable,
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
        .map_err(|_| SessionLoadError::Unavailable)?
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
        .map_err(|_| SessionLoadError::Unavailable)?
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
    refreshed.development_context_token = current.development_context_token;
    if refreshed.active_role.is_none() {
        refreshed.active_role = current.active_role;
    }
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
                .map_err(|_| SessionLoadError::Unavailable)?
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
    let groups = match crate::fetch_navigation_manifest(
        credential.access_token.expose().to_owned(),
        credential.principal_ref,
        credential.session_id.to_string(),
        request.scope_key,
    )
    .await
    {
        Ok(groups) => groups,
        Err(error) if error.ends_with("InvalidArgument") => {
            return (StatusCode::BAD_REQUEST, "navigation scope invalid").into_response();
        }
        Err(error) if error.ends_with("Unauthenticated") || error.ends_with("PermissionDenied") => {
            return (StatusCode::FORBIDDEN, "navigation authorization denied").into_response();
        }
        Err(error) if error == "authorized navigation manifest is empty" => Vec::new(),
        Err(_) => return unavailable(),
    };
    let mut response = Json(crate::navigation_http_groups(&groups)).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(PRIVATE_NO_STORE),
    );
    response
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacySessionEntry {
    principal_ref: String,
    session_id: String,
    refresh_token: String,
    refresh_expires_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacySessionBundle {
    version: u8,
    active_session_id: String,
    mru_session_ids: Vec<String>,
    accounts: Vec<LegacySessionEntry>,
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
    if !same_origin_mutation(&headers)
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
    exchange_and_install(&state, context, &headers, path, body).await
}

pub(crate) async fn directory_login(
    Extension(state): Extension<BffState>,
    Extension(context): Extension<BffSessionContext>,
    headers: HeaderMap,
    Json(request): Json<DirectoryLoginRequest>,
) -> Response {
    if !same_origin_mutation(&headers)
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
    if !same_origin_mutation(&headers)
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
    if !same_origin_mutation(&headers) || !body.is_object() {
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
    if !same_origin_mutation(&headers)
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
            .map(|account| SafeAccountSummary {
                session_id: account.session_id,
                principal_ref: account.principal_ref,
                active: account.session_id == bundle.active_session_id,
                selected_role: account.active_role.and_then(|role| {
                    Some(SelectedRole {
                        selection_id: role.selection_id,
                        source: role.source,
                        role_key: role.role_key?,
                        scope_key: role.scope_key?,
                        expires_at: None,
                        requires_mfa: false,
                    })
                }),
                refresh_expires_at: account.refresh_expires_at,
                last_used_at: account.last_used_at,
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
    if !same_origin_mutation(&headers) {
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
    if !same_origin_mutation(&headers) {
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
    if !same_origin_mutation(&headers)
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
    if !same_origin_mutation(&headers) {
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

pub(crate) async fn grpc_proxy(
    Extension(state): Extension<BffState>,
    Extension(context): Extension<BffSessionContext>,
    Path(grpc_path): Path<String>,
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
    if !same_origin_mutation(&headers)
        || !grpc_route_allowed(&grpc_path)
        || headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_none_or(|value| !value.starts_with("application/grpc-web"))
    {
        return invalid_request();
    }
    let body = match to_bytes(request.into_body(), 16 * 1024 * 1024).await {
        Ok(body) => body,
        Err(_) => return invalid_request(),
    };
    let mut upstream = state
        .auth_http
        .post(format!("{}/{}", state.grpc_origin, grpc_path))
        .header(
            header::CONTENT_TYPE.as_str(),
            headers
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("application/grpc-web+proto"),
        )
        .header("x-erpx-access-token", credential.access_token.expose())
        .header("x-erpx-actor", &credential.principal_ref)
        .header("x-erpx-session", credential.session_id.to_string())
        .body(body.to_vec());
    if let Some(context_token) = context
        .bundle
        .as_ref()
        .and_then(WebSessionBundle::active_account)
        .and_then(|account| account.development_context_token.as_ref())
    {
        upstream = upstream.header("x-erpx-dev-persona-context", context_token.expose());
    }
    for name in ["x-grpc-web", "grpc-timeout", "te", "accept"] {
        if let Some(value) = headers.get(name).and_then(|value| value.to_str().ok()) {
            upstream = upstream.header(name, value);
        }
    }
    let upstream = match upstream.send().await {
        Ok(response) => response,
        Err(_) => return unavailable(),
    };
    let status = upstream.status();
    let response_headers = upstream.headers().clone();
    let body = match upstream.bytes().await {
        Ok(body) => body,
        Err(_) => return unavailable(),
    };
    let mut response = Response::builder()
        .status(status)
        .header(header::CACHE_CONTROL, PRIVATE_NO_STORE);
    for name in [
        header::CONTENT_TYPE.as_str(),
        "grpc-status",
        "grpc-message",
        "grpc-status-details-bin",
    ] {
        if let Some(value) = response_headers.get(name) {
            response = response.header(name, value);
        }
    }
    response
        .body(Body::from(body))
        .unwrap_or_else(|_| unavailable())
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
        || (method != Method::GET && !same_origin_mutation(&headers))
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
    if !allowed || (method != Method::GET && !same_origin_mutation(&headers)) {
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

pub(crate) async fn public_branding_proxy(Extension(state): Extension<BffState>) -> Response {
    relay_public_response(
        state
            .auth_http
            .get(format!("{}/branding/v1/bootstrap", state.auth_http_origin))
            .header("accept", "application/json"),
        32 * 1024,
    )
    .await
}

pub(crate) async fn public_marketing_proxy(
    Extension(state): Extension<BffState>,
    Path(public_path): Path<String>,
    method: Method,
    headers: HeaderMap,
    request: Request,
) -> Response {
    if !public_marketing_route_allowed(&method, &public_path)
        || (method == Method::POST && !same_origin_mutation(&headers))
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

fn grpc_route_allowed(path: &str) -> bool {
    let Some((service, method)) = path.split_once('/') else {
        return false;
    };
    if method.is_empty()
        || method.len() > 128
        || !method
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return false;
    }
    let service = service
        .strip_prefix("erpx.platform.v1.")
        .unwrap_or_default();
    matches!(
        service,
        "AssetMaintenanceService"
            | "AssetsCommandService"
            | "AssetsQueryService"
            | "BrandCommandService"
            | "BrandQueryService"
            | "CommercialCommandService"
            | "CommercialQueryService"
            | "CrmCommandService"
            | "CrmQueryService"
            | "CrmService"
            | "Customer360QueryService"
            | "CustomerAccessAuditQueryService"
            | "CustomerAuthorityCommandService"
            | "CustomerAuthorityQueryService"
            | "DataPlaneControlService"
            | "DirectoryService"
            | "EntitlementService"
            | "FinanceCommandService"
            | "FinanceQueryService"
            | "FinanceService"
            | "HrService"
            | "IdentityAccessCommandService"
            | "IdentityAccessQueryService"
            | "InventoryCommandService"
            | "InventoryQueryService"
            | "LegalCommandService"
            | "LegalQueryService"
            | "LegalService"
            | "MaintenanceCommandService"
            | "MaintenanceQueryService"
            | "MarketingCommandService"
            | "MarketingContentService"
            | "MarketingQueryService"
            | "MigrationService"
            | "OperationsCommandService"
            | "OperationsQueryService"
            | "OperationsService"
            | "OrganizationCommandService"
            | "OrganizationQueryService"
            | "OrganizationService"
            | "PeoplePayrollCommandService"
            | "PeoplePayrollQueryService"
            | "PlatformAdministrationCommandService"
            | "PlatformAdministrationQueryService"
            | "PlatformCustomerService"
            | "PlatformOperationsService"
            | "PosCommandService"
            | "PosQueryService"
            | "PosService"
            | "ProcurementCommandService"
            | "ProcurementQueryService"
            | "QualityCommandService"
            | "QualityQueryService"
            | "RoleLifecycleService"
            | "SaasBillingService"
            | "SalesCommandService"
            | "SalesQueryService"
            | "SalesService"
            | "ScmService"
            | "SupportCommandService"
            | "SupportQueryService"
            | "SupportService"
    )
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

fn same_origin_mutation(headers: &HeaderMap) -> bool {
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
    origin_authority.eq_ignore_ascii_case(host)
}

pub(crate) async fn upgrade_legacy_session(
    Extension(state): Extension<BffState>,
    headers: HeaderMap,
) -> Response {
    let Some(legacy) = legacy_bundle_from_headers(&headers) else {
        return (StatusCode::UNAUTHORIZED, "legacy session unavailable").into_response();
    };
    let now = Utc::now();
    let mut accounts = Vec::with_capacity(legacy.accounts.len());
    for session_id in &legacy.mru_session_ids {
        let Some(legacy_account) = legacy
            .accounts
            .iter()
            .find(|account| &account.session_id == session_id)
        else {
            return (StatusCode::BAD_REQUEST, "legacy session invalid").into_response();
        };
        let response = match state
            .auth_http
            .post(format!("{}/auth/refresh", state.auth_http_origin))
            .json(&serde_json::json!({
                "session_id": legacy_account.session_id,
                "refresh_token": legacy_account.refresh_token,
            }))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => response,
            Ok(_) => return (StatusCode::UNAUTHORIZED, "legacy session rejected").into_response(),
            Err(_) => return unavailable(),
        };
        let rotated: RotatedSession = match response.json().await {
            Ok(rotated) => rotated,
            Err(_) => return unavailable(),
        };
        let account = match rotated_account(rotated, now) {
            Ok(account) => account,
            Err(()) => return unavailable(),
        };
        accounts.push(account);
    }
    let Some(active_session_id) = accounts
        .iter()
        .find(|account| account.session_id.to_string() == legacy.active_session_id)
        .map(|account| account.session_id)
    else {
        return unavailable();
    };
    let Some(expires_at) = accounts
        .iter()
        .map(|account| account.refresh_expires_at)
        .max()
    else {
        return unavailable();
    };
    let selector = match WebSessionSelector::generate() {
        Ok(selector) => selector,
        Err(_) => return unavailable(),
    };
    let bundle = WebSessionBundle {
        id: Uuid::new_v4(),
        selector_digest: selector.digest(),
        active_session_id,
        accounts,
        version: 1,
        created_at: now,
        updated_at: now,
        expires_at,
    };
    if state.sessions.insert(&bundle).await.is_err() {
        return unavailable();
    }
    let snapshot = match SafeWebSessionSnapshot::from_bundle(&bundle) {
        Ok(snapshot) => snapshot,
        Err(_) => return unavailable(),
    };
    let mut response = Json(snapshot).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(PRIVATE_NO_STORE),
    );
    let secure = request_is_https(&headers);
    append_cookie(
        &mut response,
        &format!(
            "{WEB_SESSION_COOKIE_NAME}={}; Path=/; Max-Age={}; HttpOnly; SameSite=Strict{}",
            URL_SAFE_NO_PAD.encode(selector.as_bytes()),
            expires_at.signed_duration_since(now).num_seconds().max(0),
            if secure { "; Secure" } else { "" }
        ),
    );
    append_cookie(
        &mut response,
        &format!(
            "{LEGACY_BUNDLE_COOKIE_NAME}=; Path=/auth; Max-Age=0; HttpOnly; SameSite=Strict{}",
            if secure { "; Secure" } else { "" }
        ),
    );
    response
}

fn legacy_bundle_from_headers(headers: &HeaderMap) -> Option<LegacySessionBundle> {
    let encoded = cookie_value(headers, LEGACY_BUNDLE_COOKIE_NAME)?;
    if encoded.is_empty() || encoded.len() > MAX_LEGACY_COOKIE_BYTES {
        return None;
    }
    let decoded = URL_SAFE_NO_PAD.decode(encoded).ok()?;
    let bundle: LegacySessionBundle = serde_json::from_slice(&decoded).ok()?;
    if bundle.version != 2
        || bundle.accounts.is_empty()
        || bundle.accounts.len() > 5
        || bundle.mru_session_ids.len() != bundle.accounts.len()
        || bundle.accounts.iter().any(|account| {
            account.principal_ref.trim().is_empty()
                || account.principal_ref.len() > 256
                || Uuid::parse_str(&account.session_id).is_err()
                || account.refresh_token.is_empty()
                || account.refresh_token.len() > 256
                || DateTime::parse_from_rfc3339(&account.refresh_expires_at).is_err()
        })
    {
        return None;
    }
    let sessions = bundle
        .accounts
        .iter()
        .map(|account| account.session_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let mru = bundle
        .mru_session_ids
        .iter()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    (sessions.len() == bundle.accounts.len()
        && sessions == mru
        && sessions.contains(bundle.active_session_id.as_str()))
    .then_some(bundle)
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

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
            HeaderValue::from_static("erpx_web_session_v1=short"),
        );
        assert!(selector_from_headers(&headers).is_none());
    }

    #[test]
    fn console_pages_redirect_to_the_canonical_login_route() {
        let protected = Request::builder()
            .uri("/platform/tenants?ignored=1")
            .body(Body::empty())
            .unwrap();
        assert!(is_console_page_request(&protected));
        assert_eq!(
            console_page_redirect(&protected, &BffSessionContext::signed_out()),
            Some("/login?return_to=/platform/tenants".to_owned())
        );

        let legacy = Request::builder()
            .uri("/auth/sign-in")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            console_page_redirect(&legacy, &BffSessionContext::signed_out()),
            Some("/login".to_owned())
        );
    }

    #[test]
    fn authenticated_login_request_returns_to_a_validated_console_page() {
        let request = Request::builder()
            .uri("/login?return_to=/platform/tenants")
            .body(Body::empty())
            .unwrap();
        let mut context = BffSessionContext::signed_out();
        context.snapshot.authenticated = true;
        assert_eq!(
            console_page_redirect(&request, &context),
            Some("/platform/tenants".to_owned())
        );

        context.snapshot.role_selection_required = true;
        assert_eq!(console_page_redirect(&request, &context), None);
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
        assert!(same_origin_mutation(&headers));

        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://attacker.example:8443"),
        );
        assert!(!same_origin_mutation(&headers));
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://console.example:8443"),
        );
        assert!(!same_origin_mutation(&headers));
    }

    #[test]
    fn proxy_routes_are_closed_compile_time_allowlists() {
        assert!(grpc_route_allowed(
            "erpx.platform.v1.FinanceQueryService/ListFinanceCharts"
        ));
        assert!(!grpc_route_allowed(
            "erpx.identity.v1.InternalTokenService/DumpTokens"
        ));
        assert!(!grpc_route_allowed(
            "erpx.platform.v1.FinanceQueryService/List-Finance-Charts"
        ));
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
        assert!(cookie.starts_with("erpx_web_session_v1="));
        assert!(cookie.contains("Path=/"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(cookie.contains("Secure"));
        assert!(!cookie.contains("access"));
        assert!(!cookie.contains("refresh"));
    }
}
