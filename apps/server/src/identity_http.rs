use std::{collections::HashMap, sync::Arc};

use chrono::{Duration, Utc};
use erpx_identity::directory::sync::{ImmutableIdentityLink, immutable_identity_database_key};
use erpx_identity::{
    AccessTokenClaims, BrowserAssertionRequest, BrowserAuthenticationCeremonyStart,
    BrowserRegistrationCeremonyStart, BrowserRegistrationRequest, IdentityError,
    IdentityStoreError, PasskeyCeremonyService, RateLimitDecision, RateLimitPolicy,
    WebAuthnProvider,
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
}

#[derive(Debug, Serialize)]
struct DeviceListResponse {
    devices: Vec<DeviceResponse>,
}

#[derive(Debug, Serialize)]
struct RecoveryCodesIssuedResponse {
    issued: bool,
    count: usize,
    codes: Vec<String>,
    issued_at: String,
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
    connector_id: Uuid,
    username: String,
    password: String,
    device_label: String,
}

#[derive(Debug, Serialize)]
struct DeviceRevokedResponse {
    revoked: bool,
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
    let credential = match auth
        .identity
        .find_passkey(principal_ref, &input.credential_id)
        .await
    {
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

    let response = match (method, path) {
        ("POST", "/auth/login") => login(body, auth).await,
        ("POST", "/auth/directory-login") => directory_login(body, auth).await,
        ("GET", "/auth/devices") => list_devices(headers, auth).await,
        ("POST", path) if device_revoke_path(path).is_some() => {
            revoke_device(path, headers, auth).await
        }
        ("POST", "/auth/recovery-codes") => issue_recovery_codes(headers, body, auth).await,
        ("POST", "/auth/recovery-codes/consume") => {
            consume_recovery_code(headers, body, auth).await
        }
        ("POST", "/auth/totp/enrollment/start") => begin_totp_enrollment(headers, body, auth).await,
        ("POST", "/auth/totp/enrollment/verify") => {
            verify_totp_enrollment(headers, body, auth).await
        }
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
        | ("GET", "/auth/recovery-sessions/initiate")
        | ("GET", "/auth/recovery-sessions/consume")
        | ("GET", "/auth/recovery-sessions/reset-password")
        | ("POST", "/auth/devices")
        | ("GET", "/auth/login") => method_not_allowed(),
        ("GET", path) if device_revoke_path(path).is_some() => method_not_allowed(),
        _ => method_not_allowed(),
    };
    Ok(Some(response))
}

fn is_identity_path(path: &str) -> bool {
    matches!(
        path,
        "/auth/login"
            | "/auth/directory-login"
            | "/auth/devices"
            | "/auth/recovery-codes"
            | "/auth/recovery-codes/consume"
            | "/auth/totp/enrollment/start"
            | "/auth/totp/enrollment/verify"
            | "/auth/recovery-sessions/initiate"
            | "/auth/recovery-sessions/consume"
            | "/auth/recovery-sessions/reset-password"
    ) || device_revoke_path(path).is_some()
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
    if input.connector_id.is_nil() || input.username.trim().is_empty() || input.password.is_empty()
    {
        return json_response("400 Bad Request", "invalid_identity_request");
    }
    let rate_key = format!("{}:{}", input.connector_id, input.username);
    if let Err(response) = admit(&auth, "directory_login", &rate_key, LOGIN_RATE_LIMIT).await {
        return response;
    }
    let connector = match auth.directory.connector(input.connector_id).await {
        Ok(connector) => connector,
        Err(_) => return json_response("401 Unauthorized", "invalid_credentials"),
    };
    let authenticated = match erpx_worker::authenticate_directory_user(
        &connector,
        &input.username,
        &input.password,
    )
    .await
    {
        Ok(identity) => identity,
        Err(_) => return json_response("401 Unauthorized", "invalid_credentials"),
    };
    let authority = match authenticated.provider() {
        erpx_identity::directory::auth::DirectoryProvider::GenericLdap => "generic_ldap",
        erpx_identity::directory::auth::DirectoryProvider::ActiveDirectoryDomainServices => {
            "active_directory_domain_services"
        }
        erpx_identity::directory::auth::DirectoryProvider::UniventionCorporateServer => {
            "univention_corporate_server"
        }
    };
    let immutable = match ImmutableIdentityLink::new(
        authority,
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
    headers: &HashMap<String, String>,
    auth: Arc<AuthContext>,
) -> IdentityHttpResponse {
    let claims = match authenticate(headers, &auth).await {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    match auth.identity.list_devices(&claims.principal_ref).await {
        Ok(devices) => response(
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
                    })
                    .collect(),
            },
        ),
        Err(error) => identity_error_response(error, false),
    }
}

async fn revoke_device(
    path: &str,
    headers: &HashMap<String, String>,
    auth: Arc<AuthContext>,
) -> IdentityHttpResponse {
    let Some(device_id) = device_revoke_path(path) else {
        return json_response("400 Bad Request", "device_id_must_be_uuid");
    };
    let claims = match authenticate(headers, &auth).await {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    match auth
        .identity
        .revoke_device(&claims.principal_ref, device_id, Utc::now())
        .await
    {
        Ok(true) => response("200 OK", &DeviceRevokedResponse { revoked: true }),
        Ok(false) => json_response("404 Not Found", "device_not_found"),
        Err(error) => identity_error_response(error, false),
    }
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
        Ok(input) => input,
        Err(_) => return json_response("400 Bad Request", "invalid_json"),
    };
    match auth
        .identity
        .issue_recovery_codes(&claims.principal_ref, input.count, Utc::now())
        .await
    {
        Ok(batch) => response(
            "200 OK",
            &RecoveryCodesIssuedResponse {
                issued: true,
                count: batch.set().records().len(),
                codes: batch.codes().map(str::to_owned).collect(),
                issued_at: batch.set().issued_at().to_rfc3339(),
            },
        ),
        Err(error) => identity_error_response(error, true),
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
        IdentityStoreError::Primitive(_) | IdentityStoreError::PrincipalInactive => {
            let (status, message) = super::identity_http_error(error);
            json_response(status, message)
        }
        IdentityStoreError::InvalidCredentials
        | IdentityStoreError::DeviceRevoked
        | IdentityStoreError::PasskeyAlreadyRegistered
        | IdentityStoreError::PasskeyNotFound
        | IdentityStoreError::PasskeyRevoked
        | IdentityStoreError::SessionInactive => {
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
        };
        let body = response(
            "200 OK",
            &DeviceListResponse {
                devices: vec![device],
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
        assert!(is_identity_path("/auth/recovery-codes"));
        assert!(is_identity_path("/auth/recovery-codes/consume"));
        assert!(is_identity_path("/auth/totp/enrollment/start"));
        assert!(is_identity_path("/auth/totp/enrollment/verify"));
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
                count: 2,
                codes: vec!["ONE-TIME-A".into(), "ONE-TIME-B".into()],
                issued_at: "2026-07-24T12:00:00Z".into(),
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
        let body = response("200 OK", &DeviceRevokedResponse { revoked: true });
        assert_eq!(body.status, "200 OK");
        assert_eq!(body.body, r#"{"revoked":true}"#);
        assert!(!body.body.contains("error"));
    }
}
