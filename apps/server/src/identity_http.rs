use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use std::collections::BTreeMap;
use std::{collections::HashMap, sync::Arc};

use chrono::{Duration, Utc};
use erpx_identity::development_access::{
    DEVELOPMENT_CLUSTER_MARKER, DEVELOPMENT_CONTEXT_HEADER, DEVELOPMENT_CONTROLLER_PRINCIPAL,
    DevelopmentControllerRoleConfig, DevelopmentPersona,
    development_controller_role_config_from_env, require_development_gates,
};
use erpx_identity::directory::sync::{ImmutableIdentityLink, immutable_identity_database_key};
use erpx_identity::{
    AccessTokenClaims, BrowserAssertionRequest, BrowserAuthenticationCeremonyStart,
    BrowserRegistrationCeremonyStart, BrowserRegistrationRequest, IdentityError,
    IdentityStoreError, LiveDirectoryError, PasskeyCeremonyService, RateLimitDecision,
    RateLimitPolicy, RoleOption, SecurityCommandContext, WebAuthnProvider,
    authenticate_directory_user_with_runtime, directory_authority_key,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::AuthContext;

const LOGIN_RATE_LIMIT: RateLimitPolicy = RateLimitPolicy::new(5, 300, 900);
const RECOVERY_ISSUE_RATE_LIMIT: RateLimitPolicy = RateLimitPolicy::new(3, 900, 1_800);
const RECOVERY_CONSUME_RATE_LIMIT: RateLimitPolicy = RateLimitPolicy::new(5, 900, 1_800);
const PASSKEY_RATE_LIMIT: RateLimitPolicy = RateLimitPolicy::new(5, 300, 900);
const TOTP_ENROLLMENT_START_RATE_LIMIT: RateLimitPolicy = RateLimitPolicy::new(3, 900, 1_800);
const TOTP_ENROLLMENT_VERIFY_RATE_LIMIT: RateLimitPolicy = RateLimitPolicy::new(5, 300, 900);
const RECOVERY_SESSION_INITIATE_RATE_LIMIT: RateLimitPolicy = RateLimitPolicy::new(3, 900, 1_800);
const RECOVERY_SESSION_CONSUME_RATE_LIMIT: RateLimitPolicy = RateLimitPolicy::new(5, 900, 1_800);
const TOTP_ENROLLMENT_LIFETIME: Duration = Duration::minutes(15);
const RECOVERY_SESSION_LIFETIME: Duration = Duration::minutes(15);

pub(crate) struct IdentityHttpResponse {
    pub(crate) status: &'static str,
    pub(crate) body: String,
}

#[derive(Debug, Deserialize)]
struct IssueRecoveryCodesRequest {
    count: usize,
    mfa_code: String,
    idempotency_key: String,
    purpose: String,
    locale: String,
}

#[derive(Debug, Deserialize)]
struct RemoveTotpRequest {
    mfa_code: String,
    idempotency_key: String,
    purpose: String,
    locale: String,
}

#[derive(Debug, Deserialize)]
struct ConsumeRecoveryCodeRequest {
    principal_ref: Option<String>,
    code: String,
}

#[derive(Debug, Serialize)]
struct DeviceResponse {
    id: String,
    label: String,
    created_at: String,
    last_seen_at: String,
    revoked_at: Option<String>,
    current: bool,
}

#[derive(Debug, Serialize)]
struct DeviceListResponse {
    devices: Vec<DeviceResponse>,
    next_cursor: Option<String>,
    measured_at: String,
}

#[derive(Debug, Serialize)]
struct RecoveryCodesIssuedResponse {
    issued: bool,
    replayed: bool,
    count: usize,
    codes: Vec<String>,
    issued_at: String,
    command_id: String,
    audit_reference: String,
}

#[derive(Debug, Serialize)]
struct SecurityFactorStateResponse {
    directory_managed: bool,
    totp_enabled: bool,
    totp_enabled_at: Option<String>,
    passkey_count: u16,
    recovery_codes_total: u16,
    recovery_codes_remaining: u16,
    recovery_codes_issued_at: Option<String>,
    measured_at: String,
}

#[derive(Debug, Serialize)]
struct TotpRemovedResponse {
    removed: bool,
    replayed: bool,
    removed_at: String,
    command_id: String,
    audit_reference: String,
}

#[derive(Debug, Serialize)]
struct RecoveryCodeConsumedResponse {
    consumed: bool,
}

#[derive(Debug, Deserialize)]
struct BeginTotpEnrollmentRequest {
    issuer: Option<String>,
    account: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VerifyTotpEnrollmentRequest {
    enrollment_id: Uuid,
    code: String,
}

#[derive(Debug, Serialize)]
struct TotpEnrollmentStartedResponse {
    enrollment_id: String,
    secret: String,
    otpauth_uri: String,
    issued_at: String,
    expires_at: String,
}

#[derive(Debug, Serialize)]
struct TotpEnrollmentVerifiedResponse {
    verified: bool,
}

#[derive(Debug, Deserialize)]
struct InitiateRecoverySessionRequest {
    principal_ref: Option<String>,
    code: String,
}

#[derive(Debug, Serialize)]
struct RecoverySessionStartedResponse {
    recovery_session_id: String,
    recovery_token: String,
    issued_at: String,
    expires_at: String,
}

#[derive(Debug, Deserialize)]
struct ConsumeRecoverySessionRequest {
    recovery_token: String,
    device_label: String,
}

#[derive(Debug, Deserialize)]
struct ResetPasswordWithRecoverySessionRequest {
    recovery_token: String,
    new_password: String,
    device_label: String,
}

#[derive(Debug, Deserialize)]
struct DirectoryLoginRequest {
    #[serde(default)]
    connector_id: Option<Uuid>,
    #[serde(default)]
    hostname: Option<String>,
    username: String,
    password: String,
    device_label: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RevokeDeviceRequest {
    idempotency_key: String,
    purpose: String,
    locale: String,
}

#[derive(Debug, Serialize)]
struct DeviceRevokedResponse {
    revoked: bool,
    replayed: bool,
    command_id: String,
    audit_reference: String,
    revoked_at: String,
}

#[derive(Debug, Deserialize)]
struct SelectRoleRequest {
    session_id: Uuid,
    selection_id: String,
}

#[derive(Debug, Serialize)]
struct RoleOptionsResponse {
    options: Vec<RoleOption>,
}

#[derive(Debug, Serialize)]
struct RoleSelectionResponse {
    session_id: Uuid,
    active_role: RoleOption,
    access_token: String,
    access_expires_at: String,
    authorization_context_version: u64,
}

#[derive(Debug, Deserialize)]
struct FinishPasskeyRegistrationRequest {
    challenge_id: Uuid,
    challenge_material: String,
    rp_id: String,
    origin: String,
    credential_id: Vec<u8>,
    response: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct BeginPasskeyAuthenticationRequest {
    principal_ref: Option<String>,
    #[serde(default)]
    credential_id: Vec<u8>,
}

#[derive(Debug, Deserialize)]
struct FinishPasskeyAuthenticationRequest {
    principal_ref: Option<String>,
    challenge_id: Uuid,
    challenge_material: String,
    rp_id: String,
    origin: String,
    credential_id: Vec<u8>,
    response: serde_json::Value,
    device_label: String,
}

#[derive(Debug, Serialize)]
struct PasskeyChallengeResponse {
    challenge_id: String,
    challenge: String,
    options: serde_json::Value,
    ceremony: &'static str,
    expires_at: String,
}

#[derive(Debug, Serialize)]
struct PasskeyCompletedResponse {
    completed: bool,
}

/// Dispatch passkey routes only after `main.rs` has constructed a provider-
/// backed ceremony service. The service must never be built around the
/// unsupported provider in a production process.
pub(crate) async fn handle_passkey<P>(
    method: &str,
    path: &str,
    headers: &HashMap<String, String>,
    body: &[u8],
    auth: Arc<AuthContext>,
    service: Arc<PasskeyCeremonyService<P>>,
) -> Option<IdentityHttpResponse>
where
    P: WebAuthnProvider,
{
    if !is_passkey_path(path) {
        return None;
    }
    let response = match (method, path) {
        ("POST", "/auth/passkeys/registration/begin") => {
            begin_passkey_registration(headers, auth, service).await
        }
        ("POST", "/auth/passkeys/registration/finish") => {
            finish_passkey_registration(headers, body, auth, service).await
        }
        ("POST", "/auth/passkeys/authentication/begin") => {
            begin_passkey_authentication(headers, body, auth, service).await
        }
        ("POST", "/auth/passkeys/authentication/finish") => {
            finish_passkey_authentication(headers, body, auth, service).await
        }
        _ => method_not_allowed(),
    };
    Some(response)
}

fn is_passkey_path(path: &str) -> bool {
    matches!(
        path,
        "/auth/passkeys/registration/begin"
            | "/auth/passkeys/registration/finish"
            | "/auth/passkeys/authentication/begin"
            | "/auth/passkeys/authentication/finish"
    )
}

async fn begin_passkey_registration<P>(
    headers: &HashMap<String, String>,
    auth: Arc<AuthContext>,
    service: Arc<PasskeyCeremonyService<P>>,
) -> IdentityHttpResponse
where
    P: WebAuthnProvider,
{
    let claims = match authenticate(headers, &auth).await {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    if let Err(response) = admit(
        &auth,
        "passkey_registration",
        &claims.principal_ref,
        PASSKEY_RATE_LIMIT,
    )
    .await
    {
        return response;
    }
    match service
        .begin_browser_registration(
            &claims.principal_ref,
            stable_webauthn_user_id(&claims.principal_ref),
            &claims.principal_ref,
            &claims.principal_ref,
            &[],
            Utc::now(),
        )
        .await
    {
        Ok(start) => browser_registration_start_response(start),
        Err(error) => identity_error_response(error, false),
    }
}

async fn finish_passkey_registration<P>(
    headers: &HashMap<String, String>,
    body: &[u8],
    auth: Arc<AuthContext>,
    service: Arc<PasskeyCeremonyService<P>>,
) -> IdentityHttpResponse
where
    P: WebAuthnProvider,
{
    let claims = match authenticate(headers, &auth).await {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    let input = match serde_json::from_slice::<FinishPasskeyRegistrationRequest>(body) {
        Ok(input) => input,
        Err(_) => return json_response("400 Bad Request", "invalid_json"),
    };
    if let Err(response) = admit(
        &auth,
        "passkey_registration",
        &claims.principal_ref,
        PASSKEY_RATE_LIMIT,
    )
    .await
    {
        return response;
    }
    let request = BrowserRegistrationRequest {
        rp_id: input.rp_id,
        origin: input.origin,
        credential_id: input.credential_id,
        response: serde_json::to_vec(&input.response).unwrap_or_default(),
        state: Vec::new(),
    };
    let material = input.challenge_material.into_bytes();
    match service
        .finish_browser_registration(
            &claims.principal_ref,
            input.challenge_id,
            &material,
            &request,
            Utc::now(),
        )
        .await
    {
        Ok(_) => response("200 OK", &PasskeyCompletedResponse { completed: true }),
        Err(error) => identity_error_response(error, false),
    }
}

async fn begin_passkey_authentication<P>(
    headers: &HashMap<String, String>,
    body: &[u8],
    auth: Arc<AuthContext>,
    service: Arc<PasskeyCeremonyService<P>>,
) -> IdentityHttpResponse
where
    P: WebAuthnProvider,
{
    let claims = match optional_authenticate(headers, &auth).await {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    let input = match serde_json::from_slice::<BeginPasskeyAuthenticationRequest>(body) {
        Ok(input) => input,
        Err(_) => return json_response("400 Bad Request", "invalid_json"),
    };
    let principal_ref = match (claims.as_ref(), input.principal_ref.as_deref()) {
        (Some(claims), Some(principal_ref)) if claims.principal_ref != principal_ref => {
            return json_response("403 Forbidden", "principal_mismatch");
        }
        (Some(claims), _) => claims.principal_ref.as_str(),
        (None, Some(principal_ref)) if !principal_ref.trim().is_empty() => principal_ref,
        (None, _) => return json_response("401 Unauthorized", "authentication_failed"),
    };
    if let Err(response) = admit(
        &auth,
        "passkey_authentication",
        principal_ref,
        PASSKEY_RATE_LIMIT,
    )
    .await
    {
        return response;
    }
    let credential = match if input.credential_id.is_empty() {
        auth.identity
            .most_recent_active_passkey(principal_ref)
            .await
    } else {
        auth.identity
            .find_passkey(principal_ref, &input.credential_id)
            .await
    } {
        Ok(Some(credential)) => credential,
        Ok(None) => return json_response("403 Forbidden", "invalid_credentials"),
        Err(_) => return json_response("503 Service Unavailable", "identity_unavailable"),
    };
    match service
        .begin_browser_authentication(principal_ref, &credential, Utc::now())
        .await
    {
        Ok(start) => browser_authentication_start_response(start),
        Err(error) => identity_error_response(error, false),
    }
}

async fn finish_passkey_authentication<P>(
    headers: &HashMap<String, String>,
    body: &[u8],
    auth: Arc<AuthContext>,
    service: Arc<PasskeyCeremonyService<P>>,
) -> IdentityHttpResponse
where
    P: WebAuthnProvider,
{
    let claims = match optional_authenticate(headers, &auth).await {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    let input = match serde_json::from_slice::<FinishPasskeyAuthenticationRequest>(body) {
        Ok(input) => input,
        Err(_) => return json_response("400 Bad Request", "invalid_json"),
    };
    let principal_ref = match (claims.as_ref(), input.principal_ref.as_deref()) {
        (Some(claims), Some(principal_ref)) if claims.principal_ref != principal_ref => {
            return json_response("403 Forbidden", "principal_mismatch");
        }
        (Some(claims), _) => claims.principal_ref.as_str(),
        (None, Some(principal_ref)) if !principal_ref.trim().is_empty() => principal_ref,
        (None, _) => return json_response("401 Unauthorized", "authentication_failed"),
    };
    if !valid_label(&input.device_label, 200) {
        return json_response("400 Bad Request", "invalid_device_label");
    }
    if let Err(response) = admit(
        &auth,
        "passkey_authentication",
        principal_ref,
        PASSKEY_RATE_LIMIT,
    )
    .await
    {
        return response;
    }
    let request = BrowserAssertionRequest {
        rp_id: input.rp_id,
        origin: input.origin,
        credential_id: input.credential_id,
        response: serde_json::to_vec(&input.response).unwrap_or_default(),
        state: Vec::new(),
    };
    let material = input.challenge_material.into_bytes();
    match service
        .finish_browser_authentication(
            principal_ref,
            input.challenge_id,
            &material,
            &request,
            Utc::now(),
        )
        .await
    {
        Ok(_) => match auth
            .identity
            .login_with_passkey(&auth.signer, principal_ref, &input.device_label, Utc::now())
            .await
        {
            Ok(session) => response("200 OK", &super::issued_session_body(&session)),
            Err(error) => identity_error_response(error, true),
        },
        Err(error) => identity_error_response(error, false),
    }
}

fn browser_registration_start_response(
    start: BrowserRegistrationCeremonyStart,
) -> IdentityHttpResponse {
    let challenge = match String::from_utf8(start.challenge().material().as_bytes().to_vec()) {
        Ok(challenge) => challenge,
        Err(_) => return json_response("500 Internal Server Error", "identity_record_invalid"),
    };
    let options = match serde_json::from_slice(start.browser().options()) {
        Ok(options) => options,
        Err(_) => return json_response("500 Internal Server Error", "identity_record_invalid"),
    };
    response(
        "200 OK",
        &PasskeyChallengeResponse {
            challenge_id: start.challenge().challenge().id.to_string(),
            challenge,
            options,
            ceremony: "registration",
            expires_at: start.challenge().challenge().expires_at.to_rfc3339(),
        },
    )
}

fn browser_authentication_start_response(
    start: BrowserAuthenticationCeremonyStart,
) -> IdentityHttpResponse {
    let challenge = match String::from_utf8(start.challenge().material().as_bytes().to_vec()) {
        Ok(challenge) => challenge,
        Err(_) => return json_response("500 Internal Server Error", "identity_record_invalid"),
    };
    let options = match serde_json::from_slice(start.browser().options()) {
        Ok(options) => options,
        Err(_) => return json_response("500 Internal Server Error", "identity_record_invalid"),
    };
    response(
        "200 OK",
        &PasskeyChallengeResponse {
            challenge_id: start.challenge().challenge().id.to_string(),
            challenge,
            options,
            ceremony: "authentication",
            expires_at: start.challenge().challenge().expires_at.to_rfc3339(),
        },
    )
}

fn stable_webauthn_user_id(principal_ref: &str) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(b"erpx-webauthn-user\0");
    digest.update(principal_ref.as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest.finalize()[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

/// Dispatch identity-only HTTP routes. The returned DTOs intentionally omit
/// password hashes, recovery-code hashes, and all persisted credential
/// material. Newly issued recovery codes are returned exactly once by their
/// dedicated authenticated issuance response.
pub(crate) async fn handle(
    method: &str,
    path: &str,
    headers: &HashMap<String, String>,
    body: &[u8],
    auth: Arc<AuthContext>,
) -> Result<Option<IdentityHttpResponse>, Box<dyn std::error::Error + Send + Sync>> {
    if is_development_path(path) {
        return Ok(Some(
            handle_development(method, path, headers, body, auth).await,
        ));
    }
    if is_passkey_path(path) {
        let Some(service) = auth.passkeys.clone() else {
            return Ok(Some(json_response(
                "503 Service Unavailable",
                "webauthn_provider_unavailable",
            )));
        };
        return Ok(handle_passkey(method, path, headers, body, auth, service).await);
    }
    if !is_identity_path(path) {
        return Ok(None);
    }

    let route_path = path.split_once('?').map_or(path, |(route, _)| route);

    let response = match (method, route_path) {
        ("POST", "/auth/login") => login(body, auth).await,
        ("POST", "/auth/directory-login") => directory_login(body, auth).await,
        ("GET", "/auth/role-options") => role_options(headers, auth).await,
        ("POST", "/auth/role-selection") => select_role(headers, body, auth).await,
        ("GET", "/auth/devices") => list_devices(path, headers, auth).await,
        ("GET", "/auth/security-factors") => security_factor_state(headers, auth).await,
        ("POST", route_path) if device_revoke_path(route_path).is_some() => {
            revoke_device(route_path, headers, body, auth).await
        }
        ("POST", "/auth/recovery-codes") => issue_recovery_codes(headers, body, auth).await,
        ("POST", "/auth/recovery-codes/consume") => {
            consume_recovery_code(headers, body, auth).await
        }
        ("POST", "/auth/totp/enrollment/start") => begin_totp_enrollment(headers, body, auth).await,
        ("POST", "/auth/totp/enrollment/verify") => {
            verify_totp_enrollment(headers, body, auth).await
        }
        ("POST", "/auth/totp/remove") => remove_totp(headers, body, auth).await,
        ("POST", "/auth/recovery-sessions/initiate") => {
            initiate_recovery_session(headers, body, auth).await
        }
        ("POST", "/auth/recovery-sessions/consume") => {
            consume_recovery_session(headers, body, auth).await
        }
        ("POST", "/auth/recovery-sessions/reset-password") => {
            reset_password_with_recovery_session(headers, body, auth).await
        }
        ("GET", "/auth/recovery-codes/consume")
        | ("GET", "/auth/recovery-codes")
        | ("GET", "/auth/totp/enrollment/start")
        | ("GET", "/auth/totp/enrollment/verify")
        | ("GET", "/auth/totp/remove")
        | ("GET", "/auth/recovery-sessions/initiate")
        | ("GET", "/auth/recovery-sessions/consume")
        | ("GET", "/auth/recovery-sessions/reset-password")
        | ("POST", "/auth/role-options")
        | ("GET", "/auth/role-selection")
        | ("POST", "/auth/devices")
        | ("POST", "/auth/security-factors")
        | ("GET", "/auth/login") => method_not_allowed(),
        ("GET", path) if device_revoke_path(path).is_some() => method_not_allowed(),
        _ => method_not_allowed(),
    };
    Ok(Some(response))
}

#[derive(Debug, Deserialize)]
struct DevelopmentControllerLoginRequest {
    password: String,
    device_label: String,
}

#[derive(Debug, Deserialize)]
struct BeginDevelopmentContextRequest {
    persona_key: String,
    feature: String,
    reason: String,
}

#[derive(Debug, Serialize)]
struct DevelopmentStatusResponse {
    enabled: bool,
    gates: Vec<&'static str>,
    cluster_marker: &'static str,
    controller_principal: &'static str,
    context_expires_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct DevelopmentPersonaResponse {
    persona_key: String,
    principal_ref: String,
    directory_username: String,
    holding_id: Option<String>,
    organization_scope: String,
    role_key: String,
    feature: String,
    permissions: Vec<String>,
    entitled: bool,
    directory_enabled: bool,
    requires_totp_for_high_risk: bool,
}

#[derive(Debug, Serialize)]
struct DevelopmentContextResponse {
    context_id: String,
    persona: DevelopmentPersonaResponse,
    reason: String,
    correlation_id: String,
    issued_at: String,
    expires_at: String,
    active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    context_token: Option<String>,
}

fn is_development_path(path: &str) -> bool {
    matches!(
        path,
        "/dev-access/login"
            | "/dev-access/status"
            | "/dev-access/personas"
            | "/dev-access/features"
            | "/dev-access/context/begin"
            | "/dev-access/context"
            | "/dev-access/context/end"
    )
}

async fn handle_development(
    method: &str,
    path: &str,
    headers: &HashMap<String, String>,
    body: &[u8],
    auth: Arc<AuthContext>,
) -> IdentityHttpResponse {
    let gates = development_gates();
    if gates.is_err() {
        // Do not advertise the existence of a development-only route unless
        // the local or explicitly configured tailnet gates are satisfied.
        return json_response("404 Not Found", "not_found");
    }
    match (method, path) {
        ("POST", "/dev-access/login") => development_controller_login(body, auth).await,
        ("GET", "/dev-access/status") => development_status(headers, auth).await,
        ("GET", "/dev-access/personas") => development_personas(headers, auth).await,
        ("GET", "/dev-access/features") => development_features(headers, auth).await,
        ("POST", "/dev-access/context/begin") => {
            begin_development_context(headers, body, auth).await
        }
        ("GET", "/dev-access/context") => get_development_context(headers, auth).await,
        ("POST", "/dev-access/context/end") => end_development_context(headers, body, auth).await,
        _ => method_not_allowed(),
    }
}

pub(crate) fn development_gates() -> Result<(), String> {
    let environment = std::env::vars().collect::<BTreeMap<_, _>>();
    let http_addr = environment
        .get("ERPX_HTTP_ADDR")
        .map_or("127.0.0.1:50052", String::as_str);
    let database_urls = [
        "ERPX_CONTROL_WRITE_DATABASE_URL",
        "ERPX_CONTROL_READ_DATABASE_URL",
        "ERPX_HOLDING_ADMIN_DATABASE_URL",
    ]
    .into_iter()
    .filter_map(|key| environment.get(key).cloned())
    .collect::<Vec<_>>();
    require_development_gates(
        cfg!(debug_assertions),
        http_addr,
        &database_urls,
        &environment,
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

pub(crate) fn development_controller_role_config()
-> Result<Option<DevelopmentControllerRoleConfig>, String> {
    let environment = std::env::vars().collect::<BTreeMap<_, _>>();
    let http_addr = environment
        .get("ERPX_HTTP_ADDR")
        .map_or("127.0.0.1:50052", String::as_str);
    let database_urls = [
        "ERPX_CONTROL_WRITE_DATABASE_URL",
        "ERPX_CONTROL_READ_DATABASE_URL",
        "ERPX_HOLDING_ADMIN_DATABASE_URL",
    ]
    .into_iter()
    .filter_map(|key| environment.get(key).cloned())
    .collect::<Vec<_>>();
    development_controller_role_config_from_env(
        cfg!(debug_assertions),
        http_addr,
        &database_urls,
        &environment,
    )
    .map_err(|error| error.to_string())
}

async fn development_controller_login(body: &[u8], auth: Arc<AuthContext>) -> IdentityHttpResponse {
    let input = match serde_json::from_slice::<DevelopmentControllerLoginRequest>(body) {
        Ok(input) if !input.password.is_empty() && !input.device_label.trim().is_empty() => input,
        _ => return json_response("400 Bad Request", "invalid_identity_request"),
    };
    match auth
        .identity
        .login(
            &auth.signer,
            DEVELOPMENT_CONTROLLER_PRINCIPAL,
            &input.password,
            &input.device_label,
            Utc::now(),
        )
        .await
    {
        Ok(session) => response("200 OK", &super::issued_session_body(&session)),
        Err(IdentityStoreError::InvalidCredentials | IdentityStoreError::PrincipalInactive) => {
            json_response("401 Unauthorized", "authentication_failed")
        }
        Err(_) => json_response("503 Service Unavailable", "identity_unavailable"),
    }
}

async fn development_controller_claims(
    headers: &HashMap<String, String>,
    auth: &AuthContext,
) -> Result<AccessTokenClaims, IdentityHttpResponse> {
    let token = headers
        .get("x-erpx-access-token")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| json_response("401 Unauthorized", "authentication_required"))?;
    let claims = auth
        .identity
        .authorize_access_token(&auth.signer, token, Utc::now())
        .await
        .map_err(|_| json_response("401 Unauthorized", "authentication_failed"))?;
    let actor = headers
        .get("x-erpx-actor")
        .map(String::as_str)
        .unwrap_or_default();
    let session = headers
        .get("x-erpx-session")
        .map(String::as_str)
        .unwrap_or_default();
    if claims.principal_ref != DEVELOPMENT_CONTROLLER_PRINCIPAL
        || actor != DEVELOPMENT_CONTROLLER_PRINCIPAL
        || session != claims.session_id.to_string()
    {
        return Err(json_response(
            "403 Forbidden",
            "development_controller_required",
        ));
    }
    Ok(claims)
}

async fn development_status(
    headers: &HashMap<String, String>,
    auth: Arc<AuthContext>,
) -> IdentityHttpResponse {
    let context_expires_at = match development_controller_claims(headers, &auth).await {
        Ok(claims) => auth
            .identity
            .get_development_persona_context(
                headers
                    .get(DEVELOPMENT_CONTEXT_HEADER)
                    .map(String::as_str)
                    .unwrap_or_default(),
                Utc::now(),
            )
            .await
            .ok()
            .filter(|context| context.controller_session_id == claims.session_id)
            .map(|context| context.expires_at.to_rfc3339()),
        Err(_) => None,
    };
    response(
        "200 OK",
        &DevelopmentStatusResponse {
            enabled: true,
            gates: Vec::new(),
            cluster_marker: DEVELOPMENT_CLUSTER_MARKER,
            controller_principal: DEVELOPMENT_CONTROLLER_PRINCIPAL,
            context_expires_at,
        },
    )
}

async fn development_personas(
    headers: &HashMap<String, String>,
    auth: Arc<AuthContext>,
) -> IdentityHttpResponse {
    if let Err(error) = development_controller_claims(headers, &auth).await {
        return error;
    }
    match auth.identity.list_development_personas(None).await {
        Ok(personas) => response(
            "200 OK",
            &personas
                .iter()
                .map(development_persona_response)
                .collect::<Vec<_>>(),
        ),
        Err(_) => json_response("503 Service Unavailable", "development_catalog_unavailable"),
    }
}

async fn development_features(
    headers: &HashMap<String, String>,
    auth: Arc<AuthContext>,
) -> IdentityHttpResponse {
    if let Err(error) = development_controller_claims(headers, &auth).await {
        return error;
    }
    let features = erpx_identity::permission_catalog()
        .into_iter()
        .map(|entry| {
            serde_json::json!({
                "role_key": entry.role_key,
                "target_scope": entry.target_scope.as_str(),
                "permissions": entry.permissions,
                "entitlement": "resolved_by_selected_holding"
            })
        })
        .collect::<Vec<_>>();
    response("200 OK", &features)
}

async fn begin_development_context(
    headers: &HashMap<String, String>,
    body: &[u8],
    auth: Arc<AuthContext>,
) -> IdentityHttpResponse {
    let claims = match development_controller_claims(headers, &auth).await {
        Ok(claims) => claims,
        Err(error) => return error,
    };
    let input = match serde_json::from_slice::<BeginDevelopmentContextRequest>(body) {
        Ok(input) => input,
        Err(_) => return json_response("400 Bad Request", "invalid_json"),
    };
    let device_id = match auth
        .identity
        .active_session_device_id(claims.session_id, Utc::now())
        .await
    {
        Ok(device_id) => device_id,
        Err(IdentityStoreError::SessionInactive) => {
            return json_response("401 Unauthorized", "session_inactive");
        }
        Err(_) => {
            return json_response("503 Service Unavailable", "development_context_unavailable");
        }
    };
    let token = erpx_identity::issue_context_token();
    match auth
        .identity
        .begin_development_persona_context(
            &token,
            DEVELOPMENT_CONTROLLER_PRINCIPAL,
            claims.session_id,
            device_id,
            &input.persona_key,
            &input.feature,
            &input.reason,
            Utc::now(),
        )
        .await
    {
        Ok(context) => response(
            "200 OK",
            &DevelopmentContextResponse {
                context_id: context.id.to_string(),
                persona: development_persona_response(&context.persona),
                reason: context.reason,
                correlation_id: context.correlation_id.to_string(),
                issued_at: context.issued_at.to_rfc3339(),
                expires_at: context.expires_at.to_rfc3339(),
                active: true,
                context_token: Some(token),
            },
        ),
        Err(IdentityStoreError::InvalidRecord(_)) => {
            json_response("403 Forbidden", "development_persona_denied")
        }
        Err(_) => json_response("503 Service Unavailable", "development_context_unavailable"),
    }
}

async fn get_development_context(
    headers: &HashMap<String, String>,
    auth: Arc<AuthContext>,
) -> IdentityHttpResponse {
    let claims = match development_controller_claims(headers, &auth).await {
        Ok(claims) => claims,
        Err(error) => return error,
    };
    let Some(token) = headers.get(DEVELOPMENT_CONTEXT_HEADER) else {
        return json_response("404 Not Found", "development_context_missing");
    };
    match auth
        .identity
        .get_development_persona_context(token, Utc::now())
        .await
    {
        Ok(context) if context.controller_session_id == claims.session_id => response(
            "200 OK",
            &DevelopmentContextResponse {
                context_id: context.id.to_string(),
                persona: development_persona_response(&context.persona),
                reason: context.reason,
                correlation_id: context.correlation_id.to_string(),
                issued_at: context.issued_at.to_rfc3339(),
                expires_at: context.expires_at.to_rfc3339(),
                active: true,
                context_token: None,
            },
        ),
        _ => json_response("404 Not Found", "development_context_inactive"),
    }
}

async fn end_development_context(
    headers: &HashMap<String, String>,
    body: &[u8],
    auth: Arc<AuthContext>,
) -> IdentityHttpResponse {
    let claims = match development_controller_claims(headers, &auth).await {
        Ok(claims) => claims,
        Err(error) => return error,
    };
    let Some(token) = headers.get(DEVELOPMENT_CONTEXT_HEADER) else {
        return json_response("404 Not Found", "development_context_missing");
    };
    let reason = serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("reason")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "controller_logout".into());
    let ended = auth
        .identity
        .end_development_persona_context(token, &reason, Utc::now())
        .await
        .unwrap_or(false);
    let _ = auth
        .identity
        .revoke_development_contexts_for_session(
            claims.session_id,
            "controller_context_end",
            Utc::now(),
        )
        .await;
    response("200 OK", &serde_json::json!({ "ended": ended }))
}

fn development_persona_response(persona: &DevelopmentPersona) -> DevelopmentPersonaResponse {
    DevelopmentPersonaResponse {
        persona_key: persona.persona_key.clone(),
        principal_ref: persona.principal_ref.clone(),
        directory_username: persona.directory_username.clone(),
        holding_id: persona.holding_id.clone(),
        organization_scope: persona.organization_scope.clone(),
        role_key: persona.role_key.clone(),
        feature: persona.feature.clone(),
        permissions: persona.permissions.clone(),
        entitled: persona.entitled,
        directory_enabled: persona.directory_enabled,
        requires_totp_for_high_risk: persona.requires_totp_for_high_risk,
    }
}

fn is_identity_path(path: &str) -> bool {
    let path = path.split_once('?').map_or(path, |(route, _)| route);
    matches!(
        path,
        "/auth/login"
            | "/auth/directory-login"
            | "/auth/role-options"
            | "/auth/role-selection"
            | "/auth/devices"
            | "/auth/security-factors"
            | "/auth/recovery-codes"
            | "/auth/recovery-codes/consume"
            | "/auth/totp/enrollment/start"
            | "/auth/totp/enrollment/verify"
            | "/auth/totp/remove"
            | "/auth/recovery-sessions/initiate"
            | "/auth/recovery-sessions/consume"
            | "/auth/recovery-sessions/reset-password"
    ) || device_revoke_path(path).is_some()
}

async fn role_options(
    headers: &HashMap<String, String>,
    auth: Arc<AuthContext>,
) -> IdentityHttpResponse {
    let claims = match authenticate(headers, &auth).await {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    match auth
        .identity
        .role_options(&claims.principal_ref, Utc::now())
        .await
    {
        Ok(options) => response("200 OK", &RoleOptionsResponse { options }),
        Err(_) => json_response("503 Service Unavailable", "role_options_unavailable"),
    }
}

async fn select_role(
    headers: &HashMap<String, String>,
    body: &[u8],
    auth: Arc<AuthContext>,
) -> IdentityHttpResponse {
    let claims = match authenticate(headers, &auth).await {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    let input = match serde_json::from_slice::<SelectRoleRequest>(body) {
        Ok(input)
            if input.session_id == claims.session_id
                && !input.selection_id.trim().is_empty()
                && input.selection_id.len() <= 512 =>
        {
            input
        }
        _ => return json_response("400 Bad Request", "invalid_role_selection"),
    };
    match auth
        .identity
        .select_active_role(
            &auth.signer,
            &claims.principal_ref,
            input.session_id,
            &input.selection_id,
            Utc::now(),
        )
        .await
    {
        Ok((active_role, token)) => response(
            "200 OK",
            &RoleSelectionResponse {
                session_id: claims.session_id,
                active_role,
                access_token: token.token().to_owned(),
                access_expires_at: token.claims().expires_at.to_rfc3339(),
                authorization_context_version: token
                    .claims()
                    .authorization_context_version
                    .expect("selected-role tokens always carry a context version"),
            },
        ),
        Err(IdentityStoreError::RoleSelectionInvalid) => {
            json_response("403 Forbidden", "role_selection_invalid")
        }
        Err(IdentityStoreError::SessionInactive) => {
            json_response("401 Unauthorized", "session_inactive")
        }
        Err(_) => json_response("503 Service Unavailable", "role_selection_unavailable"),
    }
}

fn device_revoke_path(path: &str) -> Option<Uuid> {
    let device_id = path
        .strip_prefix("/auth/devices/")?
        .strip_suffix("/revoke")?;
    if device_id.is_empty() || device_id.contains('/') {
        return None;
    }
    Uuid::parse_str(device_id).ok()
}

async fn login(body: &[u8], auth: Arc<AuthContext>) -> IdentityHttpResponse {
    let input = match serde_json::from_slice::<super::LoginBody>(body) {
        Ok(input) => input,
        Err(_) => return json_response("400 Bad Request", "invalid_json"),
    };
    if input.principal_ref.trim().is_empty() {
        return json_response("400 Bad Request", "invalid_identity_request");
    }
    if input.principal_ref == DEVELOPMENT_CONTROLLER_PRINCIPAL {
        // The controller is reachable only through the development-gated
        // route. Never let a retained local credential make it usable through
        // the ordinary production-shaped login surface.
        return json_response("401 Unauthorized", "authentication_failed");
    }
    if let Err(response) = admit(&auth, "login", &input.principal_ref, LOGIN_RATE_LIMIT).await {
        return response;
    }
    match auth
        .identity
        .principal_is_directory_managed(&input.principal_ref)
        .await
    {
        Ok(true) => return json_response("401 Unauthorized", "authentication_failed"),
        Ok(false) => {}
        Err(IdentityStoreError::Database(_)) => {
            return json_response("503 Service Unavailable", "identity_unavailable");
        }
        Err(_) => return json_response("401 Unauthorized", "authentication_failed"),
    }
    match auth
        .identity
        .login(
            &auth.signer,
            &input.principal_ref,
            &input.password,
            &input.device_label,
            Utc::now(),
        )
        .await
    {
        Ok(session) => {
            if auth
                .identity
                .clear_rate_limit("login", &input.principal_ref)
                .await
                .is_err()
            {
                return json_response("503 Service Unavailable", "identity_unavailable");
            }
            response("200 OK", &super::issued_session_body(&session))
        }
        Err(error) => {
            let (status, message) = super::identity_http_error(error);
            json_response(status, message)
        }
    }
}

async fn directory_login(body: &[u8], auth: Arc<AuthContext>) -> IdentityHttpResponse {
    let input = match serde_json::from_slice::<DirectoryLoginRequest>(body) {
        Ok(input) => input,
        Err(_) => return json_response("400 Bad Request", "invalid_json"),
    };
    if input.username.trim().is_empty() || input.password.is_empty() {
        return json_response("400 Bad Request", "invalid_identity_request");
    }
    let route_key = input
        .hostname
        .as_deref()
        .filter(|hostname| !hostname.trim().is_empty())
        .map(str::to_owned)
        .or_else(|| input.connector_id.map(|value| value.to_string()));
    let Some(route_key) = route_key else {
        return json_response("400 Bad Request", "directory_hostname_required");
    };
    let rate_key = format!("{}:{}", route_key.to_ascii_lowercase(), input.username);
    if let Err(response) = admit(&auth, "directory_login", &rate_key, LOGIN_RATE_LIMIT).await {
        return response;
    }
    let connector = match input.hostname.as_deref() {
        Some(hostname) if !hostname.trim().is_empty() => {
            match auth
                .directory
                .connector_for_verified_hostname(hostname)
                .await
            {
                Ok(Some(connector))
                    if input
                        .connector_id
                        .is_none_or(|connector_id| connector_id == connector.id) =>
                {
                    connector
                }
                _ => return json_response("401 Unauthorized", "invalid_credentials"),
            }
        }
        _ => match input.connector_id {
            Some(connector_id) if !connector_id.is_nil() => {
                match auth.directory.connector(connector_id).await {
                    Ok(connector) => connector,
                    Err(_) => return json_response("401 Unauthorized", "invalid_credentials"),
                }
            }
            _ => return json_response("400 Bad Request", "directory_hostname_required"),
        },
    };
    let settings = match auth
        .directory
        .connector_runtime_settings(connector.id)
        .await
    {
        Ok(settings) => settings,
        Err(_) => return json_response("503 Service Unavailable", "directory_unavailable"),
    };
    let primary = authenticate_directory_user_with_runtime(
        &connector,
        &settings,
        &input.username,
        &input.password,
        auth.directory_secrets.as_ref(),
    )
    .await;
    let authenticated = match primary {
        Ok(identity) => identity,
        Err(LiveDirectoryError::Transport)
            if connector.fallback_to_holding
                && matches!(
                    connector.scope,
                    erpx_identity::DirectoryScope::Branch { .. }
                ) =>
        {
            let Some(parent_id) = connector.parent_connector_id else {
                return json_response("401 Unauthorized", "invalid_credentials");
            };
            let parent = match auth.directory.connector(parent_id).await {
                Ok(parent)
                    if parent.directory_authority_fingerprint.is_some()
                        && parent.directory_authority_fingerprint
                            == connector.directory_authority_fingerprint =>
                {
                    parent
                }
                _ => {
                    let _ = auth
                        .directory
                        .record_holding_fallback(
                            &connector,
                            parent_id,
                            false,
                            Some("authority_verification_failed"),
                            Uuid::new_v4(),
                            Utc::now(),
                        )
                        .await;
                    return json_response("401 Unauthorized", "invalid_credentials");
                }
            };
            let parent_settings = match auth.directory.connector_runtime_settings(parent.id).await {
                Ok(settings) => settings,
                Err(_) => return json_response("503 Service Unavailable", "directory_unavailable"),
            };
            match authenticate_directory_user_with_runtime(
                &parent,
                &parent_settings,
                &input.username,
                &input.password,
                auth.directory_secrets.as_ref(),
            )
            .await
            {
                Ok(identity) => {
                    let _ = auth
                        .directory
                        .record_holding_fallback(
                            &connector,
                            parent.id,
                            true,
                            None,
                            Uuid::new_v4(),
                            Utc::now(),
                        )
                        .await;
                    identity
                }
                Err(_) => {
                    let _ = auth
                        .directory
                        .record_holding_fallback(
                            &connector,
                            parent.id,
                            false,
                            Some("holding_connector_unavailable"),
                            Uuid::new_v4(),
                            Utc::now(),
                        )
                        .await;
                    return json_response("401 Unauthorized", "invalid_credentials");
                }
            }
        }
        Err(LiveDirectoryError::InvalidCredentials | LiveDirectoryError::AccountDenied) => {
            return json_response("401 Unauthorized", "invalid_credentials");
        }
        Err(_) => return json_response("503 Service Unavailable", "directory_unavailable"),
    };
    let authority = match directory_authority_key(&connector) {
        Ok(authority) => authority,
        Err(_) => return json_response("401 Unauthorized", "invalid_credentials"),
    };
    let immutable = match ImmutableIdentityLink::new(
        &authority,
        authenticated.immutable_value().hyphenated().to_string(),
    ) {
        Ok(immutable) => immutable,
        Err(_) => return json_response("401 Unauthorized", "invalid_credentials"),
    };
    let identity_link = immutable_identity_database_key(&immutable);
    match auth
        .identity
        .login_with_directory_link(
            &auth.signer,
            connector.id,
            &identity_link,
            &input.device_label,
            Utc::now(),
        )
        .await
    {
        Ok(session) => {
            let _ = auth
                .identity
                .clear_rate_limit("directory_login", &rate_key)
                .await;
            response("200 OK", &super::issued_session_body(&session))
        }
        Err(IdentityStoreError::InvalidCredentials | IdentityStoreError::PrincipalInactive) => {
            json_response("401 Unauthorized", "invalid_credentials")
        }
        Err(_) => json_response("503 Service Unavailable", "identity_unavailable"),
    }
}

async fn list_devices(
    path: &str,
    headers: &HashMap<String, String>,
    auth: Arc<AuthContext>,
) -> IdentityHttpResponse {
    let claims = match authenticate(headers, &auth).await {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    let (after_device_id, page_size) = match device_list_parameters(path) {
        Ok(parameters) => parameters,
        Err(error) => return json_response("400 Bad Request", error),
    };
    let measured_at = Utc::now();
    let current_device_id = match auth
        .identity
        .active_session_device_id(claims.session_id, measured_at)
        .await
    {
        Ok(device_id) => device_id,
        Err(error) => return identity_error_response(error, false),
    };
    match auth
        .identity
        .list_devices_page(
            &claims.principal_ref,
            after_device_id,
            current_device_id,
            page_size,
        )
        .await
    {
        Ok((devices, next_device_id)) => response(
            "200 OK",
            &DeviceListResponse {
                devices: devices
                    .into_iter()
                    .map(|device| DeviceResponse {
                        id: device.id().to_string(),
                        label: device.label().to_owned(),
                        created_at: device.created_at().to_rfc3339(),
                        last_seen_at: device.last_seen_at().to_rfc3339(),
                        revoked_at: device.revoked_at().map(|at| at.to_rfc3339()),
                        current: device.id() == current_device_id,
                    })
                    .collect(),
                next_cursor: next_device_id.map(encode_device_cursor),
                measured_at: measured_at.to_rfc3339(),
            },
        ),
        Err(error) => identity_error_response(error, false),
    }
}

async fn security_factor_state(
    headers: &HashMap<String, String>,
    auth: Arc<AuthContext>,
) -> IdentityHttpResponse {
    let claims = match authenticate(headers, &auth).await {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    let measured_at = Utc::now();
    match auth
        .identity
        .security_factor_state(&claims.principal_ref, measured_at)
        .await
    {
        Ok(state) => response(
            "200 OK",
            &SecurityFactorStateResponse {
                directory_managed: state.directory_managed(),
                totp_enabled: state.totp_enabled(),
                totp_enabled_at: state.totp_enabled_at().map(|value| value.to_rfc3339()),
                passkey_count: state.passkey_count(),
                recovery_codes_total: state.recovery_codes_total(),
                recovery_codes_remaining: state.recovery_codes_remaining(),
                recovery_codes_issued_at: state
                    .recovery_codes_issued_at()
                    .map(|value| value.to_rfc3339()),
                measured_at: state.measured_at().to_rfc3339(),
            },
        ),
        Err(error) => identity_error_response(error, false),
    }
}

async fn revoke_device(
    path: &str,
    headers: &HashMap<String, String>,
    body: &[u8],
    auth: Arc<AuthContext>,
) -> IdentityHttpResponse {
    let Some(device_id) = device_revoke_path(path) else {
        return json_response("400 Bad Request", "device_id_must_be_uuid");
    };
    let claims = match authenticate(headers, &auth).await {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    let input = match serde_json::from_slice::<RevokeDeviceRequest>(body) {
        Ok(input) if valid_device_revocation_request(&input) => input,
        _ => return json_response("400 Bad Request", "invalid_device_revocation_request"),
    };
    let revoked_at = Utc::now();
    let current_device_id = match auth
        .identity
        .active_session_device_id(claims.session_id, revoked_at)
        .await
    {
        Ok(device_id) => device_id,
        Err(error) => return identity_error_response(error, false),
    };
    if current_device_id == device_id {
        return json_response("409 Conflict", "current_device_requires_sign_out");
    }
    let command = match security_command_context(
        &claims.principal_ref,
        "device_revoke",
        Some(device_id),
        &input.idempotency_key,
        &input.purpose,
        &input.locale,
        &device_id.to_string(),
    ) {
        Ok(command) => command,
        Err(response) => return response,
    };
    match auth
        .identity
        .revoke_device_command(&claims.principal_ref, device_id, &command, revoked_at)
        .await
    {
        Ok(Some(outcome)) => response(
            "200 OK",
            &DeviceRevokedResponse {
                revoked: true,
                replayed: outcome.replayed(),
                command_id: command.command_id().to_string(),
                audit_reference: command.audit_reference().to_string(),
                revoked_at: outcome.occurred_at().to_rfc3339(),
            },
        ),
        Ok(None) => json_response("404 Not Found", "device_not_found"),
        Err(error) => identity_error_response(error, false),
    }
}

fn device_list_parameters(path: &str) -> Result<(Option<Uuid>, u16), &'static str> {
    let mut cursor = None;
    let mut page_size = 25_u16;
    let mut page_size_seen = false;
    let Some((_, query)) = path.split_once('?') else {
        return Ok((cursor, page_size));
    };
    for pair in query.split('&').filter(|value| !value.is_empty()) {
        let (key, value) = pair.split_once('=').ok_or("invalid_device_list_query")?;
        match key {
            "cursor" if cursor.is_none() => cursor = Some(decode_device_cursor(value)?),
            "page_size" if !page_size_seen => {
                page_size = value
                    .parse::<u16>()
                    .ok()
                    .filter(|value| (1..=100).contains(value))
                    .ok_or("invalid_device_page_size")?;
                page_size_seen = true;
            }
            _ => return Err("invalid_device_list_query"),
        }
    }
    Ok((cursor, page_size))
}

fn encode_device_cursor(device_id: Uuid) -> String {
    URL_SAFE_NO_PAD.encode(device_id.as_bytes())
}

fn decode_device_cursor(value: &str) -> Result<Uuid, &'static str> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| "invalid_device_cursor")?;
    Uuid::from_slice(&bytes).map_err(|_| "invalid_device_cursor")
}

fn valid_device_revocation_request(request: &RevokeDeviceRequest) -> bool {
    valid_security_command_fields(&request.idempotency_key, &request.purpose, &request.locale)
}

#[cfg(test)]
fn stable_device_command_id(principal_ref: &str, device_id: Uuid, idempotency_key: &str) -> Uuid {
    stable_security_command_id(
        principal_ref,
        "device_revoke",
        Some(device_id),
        idempotency_key,
    )
}

#[cfg(test)]
fn stable_device_audit_reference(command_id: Uuid) -> Uuid {
    stable_security_audit_reference(command_id)
}

fn valid_security_command_fields(idempotency_key: &str, purpose: &str, locale: &str) -> bool {
    matches!(locale, "en" | "th")
        && !idempotency_key.trim().is_empty()
        && idempotency_key.len() <= 128
        && !idempotency_key.chars().any(char::is_control)
        && !purpose.trim().is_empty()
        && purpose.len() <= 256
        && !purpose.chars().any(char::is_control)
}

fn stable_security_command_id(
    principal_ref: &str,
    operation: &str,
    target_id: Option<Uuid>,
    idempotency_key: &str,
) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!(
            "erpx:identity-security:{principal_ref}:{operation}:{}:{idempotency_key}",
            target_id.map_or_else(|| "none".into(), |target| target.to_string())
        )
        .as_bytes(),
    )
}

fn stable_security_audit_reference(command_id: Uuid) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("erpx:identity-security-audit:{command_id}").as_bytes(),
    )
}

fn security_command_context(
    principal_ref: &str,
    operation: &str,
    target_id: Option<Uuid>,
    idempotency_key: &str,
    purpose: &str,
    locale: &str,
    payload_binding: &str,
) -> Result<SecurityCommandContext, IdentityHttpResponse> {
    if !valid_security_command_fields(idempotency_key, purpose, locale)
        || operation.trim().is_empty()
        || operation.len() > 64
        || payload_binding.len() > 512
        || payload_binding.chars().any(char::is_control)
    {
        return Err(json_response(
            "400 Bad Request",
            "invalid_security_command_request",
        ));
    }
    let command_id =
        stable_security_command_id(principal_ref, operation, target_id, idempotency_key);
    let audit_reference = stable_security_audit_reference(command_id);
    let digest = Sha256::digest(
        serde_json::to_vec(&(
            principal_ref,
            operation,
            target_id,
            idempotency_key,
            purpose,
            locale,
            payload_binding,
        ))
        .expect("bounded security command digest input is serializable"),
    );
    SecurityCommandContext::new(
        command_id,
        audit_reference,
        idempotency_key.to_owned(),
        digest.into(),
        purpose.to_owned(),
        locale.to_owned(),
    )
    .map_err(|_| json_response("400 Bad Request", "invalid_security_command_request"))
}

async fn issue_recovery_codes(
    headers: &HashMap<String, String>,
    body: &[u8],
    auth: Arc<AuthContext>,
) -> IdentityHttpResponse {
    let claims = match authenticate(headers, &auth).await {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    if let Err(response) = admit(
        &auth,
        "recovery_issue",
        &claims.principal_ref,
        RECOVERY_ISSUE_RATE_LIMIT,
    )
    .await
    {
        return response;
    }
    let input = match serde_json::from_slice::<IssueRecoveryCodesRequest>(body) {
        Ok(input)
            if valid_totp_code(&input.mfa_code)
                && valid_security_command_fields(
                    &input.idempotency_key,
                    &input.purpose,
                    &input.locale,
                ) =>
        {
            input
        }
        Ok(_) => {
            return json_response("400 Bad Request", "invalid_recovery_rotation_request");
        }
        Err(_) => return json_response("400 Bad Request", "invalid_json"),
    };
    let Some(keys) = auth.totp_keys.clone() else {
        return json_response("503 Service Unavailable", "totp_unavailable");
    };
    let command = match security_command_context(
        &claims.principal_ref,
        "recovery_codes_rotate",
        None,
        &input.idempotency_key,
        &input.purpose,
        &input.locale,
        &format!("count={}", input.count),
    ) {
        Ok(command) => command,
        Err(response) => return response,
    };
    match auth
        .identity
        .rotate_recovery_codes_with_totp(
            &claims.principal_ref,
            input.count,
            &input.mfa_code,
            keys.as_ref(),
            &command,
            Utc::now(),
        )
        .await
    {
        Ok(outcome) => {
            let codes = outcome
                .batch()
                .map(|batch| batch.codes().map(str::to_owned).collect())
                .unwrap_or_default();
            response(
                "200 OK",
                &RecoveryCodesIssuedResponse {
                    issued: true,
                    replayed: outcome.replayed(),
                    count: outcome
                        .batch()
                        .map_or(input.count, |batch| batch.set().records().len()),
                    codes,
                    issued_at: outcome.occurred_at().to_rfc3339(),
                    command_id: command.command_id().to_string(),
                    audit_reference: command.audit_reference().to_string(),
                },
            )
        }
        Err(error) => totp_error_response(error),
    }
}

async fn consume_recovery_code(
    headers: &HashMap<String, String>,
    body: &[u8],
    auth: Arc<AuthContext>,
) -> IdentityHttpResponse {
    let input = match serde_json::from_slice::<ConsumeRecoveryCodeRequest>(body) {
        Ok(input) => input,
        Err(_) => return json_response("400 Bad Request", "invalid_json"),
    };
    let claims = match optional_authenticate(headers, &auth).await {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    let principal_ref = match (claims, input.principal_ref.as_deref()) {
        (Some(claims), Some(principal_ref)) if claims.principal_ref != principal_ref => {
            return json_response("403 Forbidden", "principal_mismatch");
        }
        (Some(claims), _) => claims.principal_ref,
        (None, Some(principal_ref)) if !principal_ref.trim().is_empty() => principal_ref.to_owned(),
        (None, _) => return json_response("401 Unauthorized", "authentication_failed"),
    };
    if let Err(response) = admit(
        &auth,
        "recovery_consume",
        &principal_ref,
        RECOVERY_CONSUME_RATE_LIMIT,
    )
    .await
    {
        return response;
    }
    match auth
        .identity
        .consume_recovery_code(&principal_ref, &input.code, Utc::now())
        .await
    {
        Ok(_) => response("200 OK", &RecoveryCodeConsumedResponse { consumed: true }),
        Err(error) => identity_error_response(error, false),
    }
}

async fn begin_totp_enrollment(
    headers: &HashMap<String, String>,
    body: &[u8],
    auth: Arc<AuthContext>,
) -> IdentityHttpResponse {
    let claims = match authenticate(headers, &auth).await {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    if let Err(response) = admit(
        &auth,
        "totp_enrollment_start",
        &claims.principal_ref,
        TOTP_ENROLLMENT_START_RATE_LIMIT,
    )
    .await
    {
        return response;
    }
    let input = match serde_json::from_slice::<BeginTotpEnrollmentRequest>(body) {
        Ok(input) => input,
        Err(_) => return json_response("400 Bad Request", "invalid_json"),
    };
    let issuer = input.issuer.unwrap_or_else(|| "ERPX".to_owned());
    let account = input
        .account
        .unwrap_or_else(|| claims.principal_ref.clone());
    if !valid_label(&issuer, 128) || !valid_label(&account, 256) {
        return json_response("400 Bad Request", "invalid_totp_identity");
    }
    let Some(keys) = auth.totp_keys.clone() else {
        return json_response("503 Service Unavailable", "totp_unavailable");
    };
    let key_reference = keys.reference().to_owned();
    match auth
        .identity
        .begin_totp_enrollment(
            &claims.principal_ref,
            keys.as_ref(),
            &key_reference,
            &issuer,
            &account,
            Utc::now(),
            TOTP_ENROLLMENT_LIFETIME,
        )
        .await
    {
        Ok(start) => response(
            "200 OK",
            &TotpEnrollmentStartedResponse {
                enrollment_id: start.id().to_string(),
                secret: start.secret().as_str().to_owned(),
                otpauth_uri: start.otpauth_uri().to_owned(),
                issued_at: start.issued_at().to_rfc3339(),
                expires_at: start.expires_at().to_rfc3339(),
            },
        ),
        Err(error) => totp_error_response(error),
    }
}

async fn verify_totp_enrollment(
    headers: &HashMap<String, String>,
    body: &[u8],
    auth: Arc<AuthContext>,
) -> IdentityHttpResponse {
    let claims = match authenticate(headers, &auth).await {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    if let Err(response) = admit(
        &auth,
        "totp_enrollment_verify",
        &claims.principal_ref,
        TOTP_ENROLLMENT_VERIFY_RATE_LIMIT,
    )
    .await
    {
        return response;
    }
    let input = match serde_json::from_slice::<VerifyTotpEnrollmentRequest>(body) {
        Ok(input) => input,
        Err(_) => return json_response("400 Bad Request", "invalid_json"),
    };
    if !valid_totp_code(&input.code) {
        return json_response("400 Bad Request", "invalid_totp_code");
    }
    let Some(keys) = auth.totp_keys.clone() else {
        return json_response("503 Service Unavailable", "totp_unavailable");
    };
    match auth
        .identity
        .verify_totp_enrollment(
            &claims.principal_ref,
            input.enrollment_id,
            &input.code,
            keys.as_ref(),
            Utc::now(),
        )
        .await
    {
        Ok(()) => {
            let _ = auth
                .identity
                .clear_rate_limit("totp_enrollment_verify", &claims.principal_ref)
                .await;
            response("200 OK", &TotpEnrollmentVerifiedResponse { verified: true })
        }
        Err(error) => totp_error_response(error),
    }
}

async fn remove_totp(
    headers: &HashMap<String, String>,
    body: &[u8],
    auth: Arc<AuthContext>,
) -> IdentityHttpResponse {
    let claims = match authenticate(headers, &auth).await {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    if let Err(response) = admit(
        &auth,
        "totp_remove",
        &claims.principal_ref,
        TOTP_ENROLLMENT_VERIFY_RATE_LIMIT,
    )
    .await
    {
        return response;
    }
    let input = match serde_json::from_slice::<RemoveTotpRequest>(body) {
        Ok(input)
            if valid_totp_code(&input.mfa_code)
                && valid_security_command_fields(
                    &input.idempotency_key,
                    &input.purpose,
                    &input.locale,
                ) =>
        {
            input
        }
        Ok(_) => return json_response("400 Bad Request", "invalid_totp_removal_request"),
        Err(_) => return json_response("400 Bad Request", "invalid_json"),
    };
    let Some(keys) = auth.totp_keys.clone() else {
        return json_response("503 Service Unavailable", "totp_unavailable");
    };
    let command = match security_command_context(
        &claims.principal_ref,
        "totp_remove",
        None,
        &input.idempotency_key,
        &input.purpose,
        &input.locale,
        "remove-active-totp",
    ) {
        Ok(command) => command,
        Err(response) => return response,
    };
    match auth
        .identity
        .remove_totp_with_proof(
            &claims.principal_ref,
            &input.mfa_code,
            keys.as_ref(),
            &command,
            Utc::now(),
        )
        .await
    {
        Ok(outcome) => response(
            "200 OK",
            &TotpRemovedResponse {
                removed: true,
                replayed: outcome.replayed(),
                removed_at: outcome.occurred_at().to_rfc3339(),
                command_id: command.command_id().to_string(),
                audit_reference: command.audit_reference().to_string(),
            },
        ),
        Err(error) => totp_error_response(error),
    }
}

async fn initiate_recovery_session(
    headers: &HashMap<String, String>,
    body: &[u8],
    auth: Arc<AuthContext>,
) -> IdentityHttpResponse {
    let input = match serde_json::from_slice::<InitiateRecoverySessionRequest>(body) {
        Ok(input) => input,
        Err(_) => return json_response("400 Bad Request", "invalid_json"),
    };
    let claims = match optional_authenticate(headers, &auth).await {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    let principal_ref = match (claims, input.principal_ref.as_deref()) {
        (Some(claims), Some(principal_ref)) if claims.principal_ref != principal_ref => {
            return json_response("403 Forbidden", "principal_mismatch");
        }
        (Some(claims), _) => claims.principal_ref,
        (None, Some(principal_ref)) if !principal_ref.trim().is_empty() => principal_ref.to_owned(),
        (None, _) => return json_response("401 Unauthorized", "authentication_failed"),
    };
    if let Err(response) = admit(
        &auth,
        "recovery_session_initiate",
        &principal_ref,
        RECOVERY_SESSION_INITIATE_RATE_LIMIT,
    )
    .await
    {
        return response;
    }
    if input.code.trim().is_empty() {
        return json_response("400 Bad Request", "invalid_recovery_code");
    }
    match auth
        .identity
        .initiate_recovery_session(
            &principal_ref,
            &input.code,
            Utc::now(),
            RECOVERY_SESSION_LIFETIME,
        )
        .await
    {
        Ok(session) => response(
            "200 OK",
            &RecoverySessionStartedResponse {
                recovery_session_id: session.id().to_string(),
                recovery_token: session.token().as_str().to_owned(),
                issued_at: session.issued_at().to_rfc3339(),
                expires_at: session.expires_at().to_rfc3339(),
            },
        ),
        Err(error) => recovery_session_error_response(error),
    }
}

async fn consume_recovery_session(
    headers: &HashMap<String, String>,
    body: &[u8],
    auth: Arc<AuthContext>,
) -> IdentityHttpResponse {
    let input = match serde_json::from_slice::<ConsumeRecoverySessionRequest>(body) {
        Ok(input) => input,
        Err(_) => return json_response("400 Bad Request", "invalid_json"),
    };
    if input.recovery_token.trim().is_empty() {
        return json_response("401 Unauthorized", "recovery_session_invalid");
    }
    // A recovery bearer is the authentication credential for this exchange.
    // If a normal session is also supplied, validate it through AuthContext;
    // never treat an unvalidated access-token header as attribution.
    if headers.contains_key("x-erpx-access-token")
        && let Err(response) = authenticate(headers, &auth).await
    {
        return response;
    }
    let rate_identifier = opaque_rate_limit_identifier(&input.recovery_token);
    if let Err(response) = admit(
        &auth,
        "recovery_session_consume",
        &rate_identifier,
        RECOVERY_SESSION_CONSUME_RATE_LIMIT,
    )
    .await
    {
        return response;
    }
    match auth
        .identity
        .consume_recovery_session(
            &auth.signer,
            &input.recovery_token,
            &input.device_label,
            Utc::now(),
        )
        .await
    {
        Ok(session) => response("200 OK", &super::issued_session_body(&session)),
        Err(error) => recovery_session_error_response(error),
    }
}

async fn reset_password_with_recovery_session(
    headers: &HashMap<String, String>,
    body: &[u8],
    auth: Arc<AuthContext>,
) -> IdentityHttpResponse {
    let input = match serde_json::from_slice::<ResetPasswordWithRecoverySessionRequest>(body) {
        Ok(input) => input,
        Err(_) => return json_response("400 Bad Request", "invalid_json"),
    };
    if input.recovery_token.trim().is_empty()
        || input.new_password.is_empty()
        || input.new_password.len() > 1024
        || input.new_password.chars().any(char::is_control)
        || !valid_label(&input.device_label, 200)
    {
        return json_response("400 Bad Request", "invalid_recovery_password_request");
    }
    // A normal bearer, when supplied by a client, is validated but never used
    // as the authority for this recovery exchange. The one-time recovery
    // token remains the credential for the password replacement.
    if headers.contains_key("x-erpx-access-token")
        && let Err(response) = authenticate(headers, &auth).await
    {
        return response;
    }
    let rate_identifier = opaque_rate_limit_identifier(&input.recovery_token);
    if let Err(response) = admit(
        &auth,
        "recovery_password_reset",
        &rate_identifier,
        RECOVERY_SESSION_CONSUME_RATE_LIMIT,
    )
    .await
    {
        return response;
    }
    match auth
        .identity
        .reset_password_with_recovery_session(
            &auth.signer,
            &input.recovery_token,
            &input.new_password,
            &input.device_label,
            Utc::now(),
        )
        .await
    {
        Ok(session) => response("200 OK", &super::issued_session_body(&session)),
        Err(error) => recovery_session_error_response(error),
    }
}

fn valid_label(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max_bytes && !value.chars().any(char::is_control)
}

fn valid_totp_code(value: &str) -> bool {
    value.len() == 6 && value.as_bytes().iter().all(u8::is_ascii_digit)
}

fn opaque_rate_limit_identifier(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn totp_error_response(error: IdentityStoreError) -> IdentityHttpResponse {
    match error {
        IdentityStoreError::Database(_) => {
            json_response("503 Service Unavailable", "identity_unavailable")
        }
        IdentityStoreError::Primitive(IdentityError::InvalidIdentityReference) => {
            json_response("400 Bad Request", "invalid_totp_identity")
        }
        IdentityStoreError::Primitive(IdentityError::InvalidTotpCode) => {
            json_response("400 Bad Request", "invalid_totp_code")
        }
        IdentityStoreError::Primitive(IdentityError::TotpEnrollmentInvalid) => {
            json_response("400 Bad Request", "totp_enrollment_invalid")
        }
        IdentityStoreError::Primitive(IdentityError::TotpReplay)
        | IdentityStoreError::SecurityCommandConflict => {
            json_response("409 Conflict", "security_command_conflict")
        }
        IdentityStoreError::Primitive(
            IdentityError::InvalidKeyReference
            | IdentityError::KeyReferenceNotFound(_)
            | IdentityError::TotpEncryptionFailed
            | IdentityError::TotpDecryptionFailed,
        ) => json_response("503 Service Unavailable", "totp_unavailable"),
        IdentityStoreError::InvalidRecord(_) => {
            json_response("500 Internal Server Error", "identity_record_invalid")
        }
        other => identity_error_response(other, false),
    }
}

fn recovery_session_error_response(error: IdentityStoreError) -> IdentityHttpResponse {
    match error {
        IdentityStoreError::Database(_) => {
            json_response("503 Service Unavailable", "identity_unavailable")
        }
        IdentityStoreError::Primitive(IdentityError::InvalidRecoveryCode) => {
            json_response("401 Unauthorized", "recovery_code_invalid")
        }
        IdentityStoreError::Primitive(IdentityError::RecoveryCodeAlreadyConsumed) => {
            json_response("401 Unauthorized", "recovery_code_invalid")
        }
        IdentityStoreError::Primitive(IdentityError::RecoverySessionInvalid) => {
            json_response("401 Unauthorized", "recovery_session_invalid")
        }
        IdentityStoreError::InvalidRecord(_) => {
            json_response("500 Internal Server Error", "identity_record_invalid")
        }
        IdentityStoreError::SecurityCommandConflict => {
            json_response("409 Conflict", "security_command_conflict")
        }
        other => identity_error_response(other, false),
    }
}

async fn admit(
    auth: &AuthContext,
    namespace: &str,
    identifier: &str,
    policy: RateLimitPolicy,
) -> Result<(), IdentityHttpResponse> {
    match auth
        .identity
        .admit_rate_limit(namespace, identifier, policy, Utc::now())
        .await
    {
        Ok(RateLimitDecision::Allowed) => Ok(()),
        Ok(RateLimitDecision::Limited) => Err(json_response(
            "429 Too Many Requests",
            "authentication_temporarily_unavailable",
        )),
        Err(_) => Err(json_response(
            "503 Service Unavailable",
            "identity_unavailable",
        )),
    }
}

async fn authenticate(
    headers: &HashMap<String, String>,
    auth: &AuthContext,
) -> Result<AccessTokenClaims, IdentityHttpResponse> {
    let token = headers
        .get("x-erpx-access-token")
        .filter(|token| !token.is_empty())
        .ok_or_else(|| json_response("401 Unauthorized", "authentication_failed"))?;
    auth.identity
        .authorize_access_token(&auth.signer, token, Utc::now())
        .await
        .map_err(|error| {
            let (status, message) = super::identity_http_error(error);
            json_response(status, message)
        })
}

async fn optional_authenticate(
    headers: &HashMap<String, String>,
    auth: &AuthContext,
) -> Result<Option<AccessTokenClaims>, IdentityHttpResponse> {
    if !headers.contains_key("x-erpx-access-token") {
        return Ok(None);
    }
    authenticate(headers, auth).await.map(Some)
}

fn identity_error_response(error: IdentityStoreError, issuing: bool) -> IdentityHttpResponse {
    match error {
        IdentityStoreError::Database(_) => {
            json_response("503 Service Unavailable", "identity_unavailable")
        }
        IdentityStoreError::Primitive(IdentityError::InvalidRecoveryCodeCount) if issuing => {
            json_response("400 Bad Request", "invalid_recovery_code_count")
        }
        IdentityStoreError::Primitive(IdentityError::InvalidIdentityReference) if issuing => {
            json_response("400 Bad Request", "invalid_identity_reference")
        }
        IdentityStoreError::PrincipalInactive if issuing => {
            json_response("403 Forbidden", "principal_inactive")
        }
        IdentityStoreError::InvalidRecord(_) => {
            json_response("500 Internal Server Error", "identity_record_invalid")
        }
        IdentityStoreError::SecurityCommandConflict => {
            json_response("409 Conflict", "security_command_conflict")
        }
        IdentityStoreError::Primitive(_) | IdentityStoreError::PrincipalInactive => {
            let (status, message) = super::identity_http_error(error);
            json_response(status, message)
        }
        IdentityStoreError::InvalidCredentials
        | IdentityStoreError::DeviceRevoked
        | IdentityStoreError::PasskeyAlreadyRegistered
        | IdentityStoreError::PasskeyNotFound
        | IdentityStoreError::PasskeyRevoked
        | IdentityStoreError::SessionInactive
        | IdentityStoreError::RoleSelectionRequired
        | IdentityStoreError::RoleSelectionInvalid => {
            let (status, message) = super::identity_http_error(error);
            json_response(status, message)
        }
    }
}

fn method_not_allowed() -> IdentityHttpResponse {
    json_response("405 Method Not Allowed", "method_not_allowed")
}

fn json_response(status: &'static str, error: &'static str) -> IdentityHttpResponse {
    IdentityHttpResponse {
        status,
        body: serde_json::json!({ "error": error }).to_string(),
    }
}

fn response<T: Serialize>(status: &'static str, value: &T) -> IdentityHttpResponse {
    IdentityHttpResponse {
        status,
        body: serde_json::to_string(value).expect("identity response DTO is serializable"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_dtos_do_not_contain_secret_or_hash_fields() {
        let device = DeviceResponse {
            id: Uuid::new_v4().to_string(),
            label: "laptop".into(),
            created_at: "2026-07-24T12:00:00Z".into(),
            last_seen_at: "2026-07-24T12:00:00Z".into(),
            revoked_at: None,
            current: true,
        };
        let body = response(
            "200 OK",
            &DeviceListResponse {
                devices: vec![device],
                next_cursor: None,
                measured_at: "2026-07-24T12:00:00Z".into(),
            },
        );
        assert!(!body.body.contains("refresh_token"));
        assert!(!body.body.contains("code_hash"));
        assert!(!body.body.contains("password"));
        assert!(!body.body.contains("secret"));
    }

    #[test]
    fn identity_route_matching_is_scoped_to_the_declared_paths() {
        assert!(is_identity_path("/auth/devices"));
        assert!(is_identity_path("/auth/devices?page_size=25"));
        assert!(is_identity_path("/auth/security-factors"));
        assert!(is_identity_path("/auth/recovery-codes"));
        assert!(is_identity_path("/auth/recovery-codes/consume"));
        assert!(is_identity_path("/auth/totp/enrollment/start"));
        assert!(is_identity_path("/auth/totp/enrollment/verify"));
        assert!(is_identity_path("/auth/totp/remove"));
        assert!(is_identity_path("/auth/recovery-sessions/initiate"));
        assert!(is_identity_path("/auth/recovery-sessions/consume"));
        assert!(is_identity_path("/auth/recovery-sessions/reset-password"));
        assert!(is_identity_path(&format!(
            "/auth/devices/{}/revoke",
            Uuid::new_v4()
        )));
        assert!(!is_identity_path("/auth/devices/not-a-uuid/revoke"));
        assert!(is_identity_path("/auth/login"));
    }

    #[test]
    fn passkey_route_helpers_are_explicit_and_responses_redact_credential_material() {
        assert!(is_passkey_path("/auth/passkeys/registration/begin"));
        assert!(is_passkey_path("/auth/passkeys/registration/finish"));
        assert!(is_passkey_path("/auth/passkeys/authentication/begin"));
        assert!(is_passkey_path("/auth/passkeys/authentication/finish"));
        assert!(!is_passkey_path("/auth/passkeys/credential"));

        let body = response(
            "200 OK",
            &PasskeyChallengeResponse {
                challenge_id: Uuid::new_v4().to_string(),
                challenge: "public-ceremony-challenge".into(),
                options: serde_json::json!({"publicKey": {}}),
                ceremony: "registration",
                expires_at: "2026-07-24T12:05:00Z".into(),
            },
        );
        assert!(!body.body.contains("credential_id"));
        assert!(!body.body.contains("public_key"));
        assert!(!body.body.contains("access_token"));
        assert!(!body.body.contains("refresh_token"));
    }

    #[test]
    fn passkey_authentication_can_begin_without_disclosing_a_credential_identifier() {
        let request: BeginPasskeyAuthenticationRequest =
            serde_json::from_value(serde_json::json!({ "principal_ref": "principal-001" }))
                .expect("principal-only passkey request");
        assert_eq!(request.principal_ref.as_deref(), Some("principal-001"));
        assert!(request.credential_id.is_empty());
    }

    #[test]
    fn recovery_consumed_response_has_no_code_material() {
        let body = response("200 OK", &RecoveryCodeConsumedResponse { consumed: true });
        assert_eq!(body.body, r#"{"consumed":true}"#);
    }

    #[test]
    fn recovery_code_issuance_returns_plaintext_codes_only_in_the_one_time_issue_dto() {
        let body = response(
            "200 OK",
            &RecoveryCodesIssuedResponse {
                issued: true,
                replayed: false,
                count: 2,
                codes: vec!["ONE-TIME-A".into(), "ONE-TIME-B".into()],
                issued_at: "2026-07-24T12:00:00Z".into(),
                command_id: Uuid::new_v4().to_string(),
                audit_reference: Uuid::new_v4().to_string(),
            },
        );
        assert!(body.body.contains("ONE-TIME-A"));
        assert!(body.body.contains("ONE-TIME-B"));
        assert!(!body.body.contains("code_hash"));
        assert!(!body.body.contains("password"));
    }

    #[test]
    fn totp_start_response_allows_only_one_time_public_enrollment_material() {
        let body = response(
            "200 OK",
            &TotpEnrollmentStartedResponse {
                enrollment_id: Uuid::new_v4().to_string(),
                secret: "PUBLIC-ONCE".into(),
                otpauth_uri: "otpauth://totp/ERPX:principal?secret=PUBLIC-ONCE".into(),
                issued_at: "2026-07-24T12:00:00Z".into(),
                expires_at: "2026-07-24T12:15:00Z".into(),
            },
        );
        assert!(body.body.contains("PUBLIC-ONCE"));
        assert!(!body.body.contains("ciphertext"));
        assert!(!body.body.contains("code_hash"));
        assert!(!body.body.contains("password"));
        assert!(!body.body.contains("key_reference"));
    }

    #[test]
    fn recovery_session_response_contains_bearer_once_without_persisted_material() {
        let body = response(
            "200 OK",
            &RecoverySessionStartedResponse {
                recovery_session_id: Uuid::new_v4().to_string(),
                recovery_token: "ONE-TIME-BEARER".into(),
                issued_at: "2026-07-24T12:00:00Z".into(),
                expires_at: "2026-07-24T12:15:00Z".into(),
            },
        );
        assert!(body.body.contains("ONE-TIME-BEARER"));
        assert!(!body.body.contains("token_hash"));
        assert!(!body.body.contains("code_hash"));
        assert!(!body.body.contains("password"));
        assert!(!body.body.contains("ciphertext"));
    }

    #[test]
    fn identity_lifetimes_are_fixed_at_or_below_fifteen_minutes() {
        assert_eq!(TOTP_ENROLLMENT_LIFETIME, Duration::minutes(15));
        assert_eq!(RECOVERY_SESSION_LIFETIME, Duration::minutes(15));
        assert!(TOTP_ENROLLMENT_LIFETIME <= Duration::minutes(15));
        assert!(RECOVERY_SESSION_LIFETIME <= Duration::minutes(15));
    }

    #[test]
    fn device_revocation_success_is_not_encoded_as_an_error() {
        let command_id = Uuid::new_v4();
        let body = response(
            "200 OK",
            &DeviceRevokedResponse {
                revoked: true,
                replayed: false,
                command_id: command_id.to_string(),
                audit_reference: stable_device_audit_reference(command_id).to_string(),
                revoked_at: "2026-07-24T12:00:00Z".into(),
            },
        );
        assert_eq!(body.status, "200 OK");
        assert!(body.body.contains(r#""revoked":true"#));
        assert!(body.body.contains(r#""replayed":false"#));
        assert!(!body.body.contains("error"));
    }

    #[test]
    fn device_list_query_is_bounded_and_uses_an_opaque_cursor() {
        let device_id = Uuid::new_v4();
        let cursor = encode_device_cursor(device_id);
        assert!(!cursor.contains(&device_id.to_string()));
        assert_eq!(decode_device_cursor(&cursor), Ok(device_id));
        assert_eq!(
            device_list_parameters(&format!("/auth/devices?cursor={cursor}&page_size=40")),
            Ok((Some(device_id), 40))
        );
        for invalid in [
            "/auth/devices?page_size=0",
            "/auth/devices?page_size=101",
            "/auth/devices?page_size=25&page_size=25",
            "/auth/devices?unknown=value",
            "/auth/devices?cursor=not-a-cursor",
        ] {
            assert!(device_list_parameters(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn device_revocation_contract_requires_locale_idempotency_and_purpose() {
        let device_id = Uuid::new_v4();
        let valid = RevokeDeviceRequest {
            idempotency_key: "device-command-001".into(),
            purpose: "Revoke an unrecognized device".into(),
            locale: "en".into(),
        };
        assert!(valid_device_revocation_request(&valid));
        assert_eq!(
            stable_device_command_id("principal-001", device_id, &valid.idempotency_key),
            stable_device_command_id("principal-001", device_id, &valid.idempotency_key)
        );
        assert_ne!(
            stable_device_command_id("principal-001", device_id, &valid.idempotency_key),
            stable_device_command_id("principal-002", device_id, &valid.idempotency_key)
        );
        assert_ne!(
            stable_device_command_id("principal-001", device_id, &valid.idempotency_key),
            stable_device_command_id("principal-001", Uuid::new_v4(), &valid.idempotency_key)
        );
        assert!(!valid_device_revocation_request(&RevokeDeviceRequest {
            purpose: String::new(),
            ..valid
        }));
    }
}
