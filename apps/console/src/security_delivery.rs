use dioxus::prelude::dioxus_fullstack;
#[cfg(feature = "server")]
use dioxus::prelude::dioxus_server;
use erpx_delivery_contract::{DeliveryError, DeliveryErrorMeta};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg_attr(not(feature = "server"), allow(dead_code))]
const MAX_DEVICE_PAGE_SIZE: u16 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceListRequest {
    pub cursor: Option<String>,
    pub page_size: u16,
    pub locale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityDevice {
    pub id: Uuid,
    pub label: String,
    pub created_at: String,
    pub last_seen_at: String,
    pub revoked_at: Option<String>,
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevicePage {
    pub items: Vec<SecurityDevice>,
    pub next_cursor: Option<String>,
    pub measured_at: String,
    pub freshness: String,
    pub redacted_fields: Vec<String>,
    pub correlation_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevokeDeviceRequest {
    pub device_id: Uuid,
    pub idempotency_key: String,
    pub locale: String,
    pub purpose: String,
    pub confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceCommandReceipt {
    pub command_id: Uuid,
    pub device_id: Uuid,
    pub replayed: bool,
    pub revoked_at: String,
    pub invalidation_tags: Vec<String>,
    pub audit_reference: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevokeDeviceOutcome {
    pub receipt: DeviceCommandReceipt,
    pub device: Option<SecurityDevice>,
    pub authoritative_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityStateRequest {
    pub locale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityFactorPage {
    pub directory_managed: bool,
    pub totp_enabled: bool,
    pub totp_enabled_at: Option<String>,
    pub passkey_count: u16,
    pub recovery_codes_total: u16,
    pub recovery_codes_remaining: u16,
    pub recovery_codes_issued_at: Option<String>,
    pub measured_at: String,
    pub freshness: String,
    pub redacted_fields: Vec<String>,
    pub correlation_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BeginTotpRequest {
    pub locale: String,
    pub purpose: String,
    pub confirmed: bool,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TotpEnrollmentView {
    pub enrollment_id: Uuid,
    pub secret: String,
    pub otpauth_uri: String,
    pub issued_at: String,
    pub expires_at: String,
}

impl std::fmt::Debug for TotpEnrollmentView {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TotpEnrollmentView")
            .field("enrollment_id", &self.enrollment_id)
            .field("secret", &"[REDACTED]")
            .field("otpauth_uri", &"[REDACTED]")
            .field("issued_at", &self.issued_at)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyTotpRequest {
    pub enrollment_id: Uuid,
    pub code: String,
    pub locale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TotpVerifiedOutcome {
    pub verified: bool,
    pub state: SecurityFactorPage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HighRiskSecurityRequest {
    pub mfa_code: String,
    pub idempotency_key: String,
    pub locale: String,
    pub purpose: String,
    pub confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityCommandReceipt {
    pub command_id: Uuid,
    pub audit_reference: Uuid,
    pub replayed: bool,
    pub occurred_at: String,
    pub invalidation_tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveTotpOutcome {
    pub receipt: SecurityCommandReceipt,
    pub state: SecurityFactorPage,
    pub authoritative_state: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryCodeRotationView {
    pub receipt: SecurityCommandReceipt,
    pub codes: Vec<String>,
    pub state: SecurityFactorPage,
    pub authoritative_state: String,
}

impl std::fmt::Debug for RecoveryCodeRotationView {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RecoveryCodeRotationView")
            .field("receipt", &self.receipt)
            .field("codes", &format_args!("[REDACTED; {}]", self.codes.len()))
            .field("state", &self.state)
            .field("authoritative_state", &self.authoritative_state)
            .finish()
    }
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn error_meta(retryable: bool, retry_after_ms: Option<u64>) -> DeliveryErrorMeta {
    DeliveryErrorMeta {
        correlation_id: Uuid::new_v4(),
        retry_after_ms,
        retryable,
    }
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn valid_locale(locale: &str) -> bool {
    matches!(locale, "en" | "th")
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn validate_list_request(request: &DeviceListRequest) -> Result<(), DeliveryError> {
    if !valid_locale(&request.locale)
        || request.page_size == 0
        || request.page_size > MAX_DEVICE_PAGE_SIZE
        || request.cursor.as_ref().is_some_and(|cursor| {
            cursor.is_empty()
                || cursor.len() > 128
                || cursor.chars().any(|value| value.is_control())
        })
    {
        return Err(DeliveryError::Validation(error_meta(false, None)));
    }
    Ok(())
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn validate_revoke_request(request: &RevokeDeviceRequest) -> Result<(), DeliveryError> {
    if request.device_id.is_nil()
        || !request.confirmed
        || !valid_locale(&request.locale)
        || request.idempotency_key.trim().is_empty()
        || request.idempotency_key.len() > 128
        || request.idempotency_key.chars().any(char::is_control)
        || request.purpose.trim().is_empty()
        || request.purpose.len() > 256
        || request.purpose.chars().any(char::is_control)
    {
        return Err(DeliveryError::Validation(error_meta(false, None)));
    }
    Ok(())
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn valid_purpose(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn valid_totp_code(value: &str) -> bool {
    value.len() == 6 && value.as_bytes().iter().all(u8::is_ascii_digit)
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn validate_security_state_request(request: &SecurityStateRequest) -> Result<(), DeliveryError> {
    if !valid_locale(&request.locale) {
        return Err(DeliveryError::Validation(error_meta(false, None)));
    }
    Ok(())
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn validate_begin_totp_request(request: &BeginTotpRequest) -> Result<(), DeliveryError> {
    if !request.confirmed || !valid_locale(&request.locale) || !valid_purpose(&request.purpose) {
        return Err(DeliveryError::Validation(error_meta(false, None)));
    }
    Ok(())
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn validate_verify_totp_request(request: &VerifyTotpRequest) -> Result<(), DeliveryError> {
    if request.enrollment_id.is_nil()
        || !valid_locale(&request.locale)
        || !valid_totp_code(&request.code)
    {
        return Err(DeliveryError::Validation(error_meta(false, None)));
    }
    Ok(())
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn validate_high_risk_request(request: &HighRiskSecurityRequest) -> Result<(), DeliveryError> {
    if !request.confirmed
        || !valid_locale(&request.locale)
        || !valid_totp_code(&request.mfa_code)
        || request.idempotency_key.trim().is_empty()
        || request.idempotency_key.len() > 128
        || request.idempotency_key.chars().any(char::is_control)
        || !valid_purpose(&request.purpose)
    {
        return Err(DeliveryError::Validation(error_meta(false, None)));
    }
    Ok(())
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/auth/devices/list")]
pub async fn list_security_devices(
    request: DeviceListRequest,
) -> dioxus::prelude::ServerFnResult<Result<DevicePage, DeliveryError>> {
    if let Err(error) = validate_list_request(&request) {
        return Ok(Err(error));
    }
    #[cfg(feature = "server")]
    {
        return Ok(server::list_devices(request).await);
    }
    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/auth/devices/revoke")]
pub async fn revoke_security_device(
    request: RevokeDeviceRequest,
) -> dioxus::prelude::ServerFnResult<Result<RevokeDeviceOutcome, DeliveryError>> {
    if let Err(error) = validate_revoke_request(&request) {
        return Ok(Err(error));
    }
    #[cfg(feature = "server")]
    {
        crate::delivery::validate_mutation_request()?;
        return Ok(server::revoke_device(request).await);
    }
    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/auth/security/state")]
pub async fn get_security_factor_state(
    request: SecurityStateRequest,
) -> dioxus::prelude::ServerFnResult<Result<SecurityFactorPage, DeliveryError>> {
    if let Err(error) = validate_security_state_request(&request) {
        return Ok(Err(error));
    }
    #[cfg(feature = "server")]
    {
        return Ok(server::security_state().await);
    }
    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/auth/security/totp/start")]
pub async fn start_totp_enrollment(
    request: BeginTotpRequest,
) -> dioxus::prelude::ServerFnResult<Result<TotpEnrollmentView, DeliveryError>> {
    if let Err(error) = validate_begin_totp_request(&request) {
        return Ok(Err(error));
    }
    #[cfg(feature = "server")]
    {
        crate::delivery::validate_mutation_request()?;
        return Ok(server::start_totp().await);
    }
    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/auth/security/totp/verify")]
pub async fn confirm_totp_enrollment(
    request: VerifyTotpRequest,
) -> dioxus::prelude::ServerFnResult<Result<TotpVerifiedOutcome, DeliveryError>> {
    if let Err(error) = validate_verify_totp_request(&request) {
        return Ok(Err(error));
    }
    #[cfg(feature = "server")]
    {
        crate::delivery::validate_mutation_request()?;
        return Ok(server::verify_totp(request).await);
    }
    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/auth/security/totp/remove")]
pub async fn remove_totp_factor(
    request: HighRiskSecurityRequest,
) -> dioxus::prelude::ServerFnResult<Result<RemoveTotpOutcome, DeliveryError>> {
    if let Err(error) = validate_high_risk_request(&request) {
        return Ok(Err(error));
    }
    #[cfg(feature = "server")]
    {
        crate::delivery::validate_mutation_request()?;
        return Ok(server::remove_totp(request).await);
    }
    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/auth/recovery/rotate")]
pub async fn rotate_recovery_codes(
    request: HighRiskSecurityRequest,
) -> dioxus::prelude::ServerFnResult<Result<RecoveryCodeRotationView, DeliveryError>> {
    if let Err(error) = validate_high_risk_request(&request) {
        return Ok(Err(error));
    }
    #[cfg(feature = "server")]
    {
        crate::delivery::validate_mutation_request()?;
        return Ok(server::rotate_recovery(request).await);
    }
    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[cfg(feature = "server")]
mod server {
    use dioxus::fullstack::FullstackContext;
    use dioxus::server::axum::http::{HeaderName, HeaderValue, header};
    use serde::de::DeserializeOwned;

    use super::*;
    use crate::bff::{BffCredential, BffSessionContext, BffState};

    #[derive(Debug, Deserialize)]
    struct OwnerDevice {
        id: String,
        label: String,
        created_at: String,
        last_seen_at: String,
        revoked_at: Option<String>,
        current: bool,
    }

    #[derive(Debug, Deserialize)]
    struct OwnerDevicePage {
        devices: Vec<OwnerDevice>,
        next_cursor: Option<String>,
        measured_at: String,
    }

    #[derive(Debug, Deserialize)]
    struct OwnerRevokeReceipt {
        revoked: bool,
        replayed: bool,
        command_id: String,
        audit_reference: String,
        revoked_at: String,
    }

    #[derive(Debug, Deserialize)]
    struct OwnerSecurityState {
        directory_managed: bool,
        totp_enabled: bool,
        totp_enabled_at: Option<String>,
        passkey_count: u16,
        recovery_codes_total: u16,
        recovery_codes_remaining: u16,
        recovery_codes_issued_at: Option<String>,
        measured_at: String,
    }

    #[derive(Deserialize)]
    struct OwnerTotpEnrollment {
        enrollment_id: String,
        secret: String,
        otpauth_uri: String,
        issued_at: String,
        expires_at: String,
    }

    impl std::fmt::Debug for OwnerTotpEnrollment {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("OwnerTotpEnrollment")
                .field("enrollment_id", &self.enrollment_id)
                .field("secret", &"[REDACTED]")
                .field("otpauth_uri", &"[REDACTED]")
                .field("issued_at", &self.issued_at)
                .field("expires_at", &self.expires_at)
                .finish()
        }
    }

    #[derive(Debug, Deserialize)]
    struct OwnerTotpVerified {
        verified: bool,
    }

    #[derive(Debug, Deserialize)]
    struct OwnerSecurityReceipt {
        replayed: bool,
        command_id: String,
        audit_reference: String,
    }

    #[derive(Debug, Deserialize)]
    struct OwnerTotpRemoved {
        removed: bool,
        removed_at: String,
        #[serde(flatten)]
        receipt: OwnerSecurityReceipt,
    }

    #[derive(Deserialize)]
    struct OwnerRecoveryRotation {
        issued: bool,
        count: usize,
        codes: Vec<String>,
        issued_at: String,
        #[serde(flatten)]
        receipt: OwnerSecurityReceipt,
    }

    impl std::fmt::Debug for OwnerRecoveryRotation {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("OwnerRecoveryRotation")
                .field("issued", &self.issued)
                .field("count", &self.count)
                .field("codes", &format_args!("[REDACTED; {}]", self.codes.len()))
                .field("issued_at", &self.issued_at)
                .field("receipt", &self.receipt)
                .finish()
        }
    }

    #[derive(Clone)]
    struct SecurityAuthority {
        state: BffState,
        credential: BffCredential,
    }

    fn authority() -> Result<SecurityAuthority, DeliveryError> {
        let context = FullstackContext::current()
            .ok_or_else(|| DeliveryError::Unavailable(error_meta(true, Some(500))))?;
        context.add_response_header(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, no-store"),
        );
        let state = context
            .extension::<BffState>()
            .ok_or_else(|| DeliveryError::Unavailable(error_meta(true, Some(500))))?;
        let session = context
            .extension::<BffSessionContext>()
            .ok_or_else(|| DeliveryError::Unauthenticated(error_meta(false, None)))?;
        let credential = session
            .credential
            .ok_or_else(|| DeliveryError::Unauthenticated(error_meta(false, None)))?;
        Ok(SecurityAuthority { state, credential })
    }

    fn upstream_error(status: u16, correlation_id: Uuid) -> DeliveryError {
        let retryable = matches!(status, 502..=504);
        let meta = DeliveryErrorMeta {
            correlation_id,
            retry_after_ms: retryable.then_some(500),
            retryable,
        };
        match status {
            401 => DeliveryError::Unauthenticated(meta),
            403 => DeliveryError::Forbidden(meta),
            404 => DeliveryError::NotFound(meta),
            409 => DeliveryError::Conflict(meta),
            400 | 405 | 422 => DeliveryError::Validation(meta),
            429 => DeliveryError::RateLimited(meta),
            502..=504 => DeliveryError::Unavailable(meta),
            _ => DeliveryError::Internal(meta),
        }
    }

    fn response_correlation(response: &reqwest::Response) -> Uuid {
        response
            .headers()
            .get("x-erpx-correlation-id")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| Uuid::parse_str(value).ok())
            .unwrap_or_else(Uuid::new_v4)
    }

    fn add_response_correlation(correlation_id: Uuid) {
        if let Some(context) = FullstackContext::current()
            && let Ok(value) = HeaderValue::from_str(&correlation_id.to_string())
        {
            context.add_response_header(HeaderName::from_static("x-erpx-correlation-id"), value);
        }
    }

    async fn decode_owner<T: DeserializeOwned>(
        response: reqwest::Response,
    ) -> Result<(T, Uuid), DeliveryError> {
        let status = response.status().as_u16();
        let correlation_id = response_correlation(&response);
        add_response_correlation(correlation_id);
        if !(200..300).contains(&status) {
            return Err(upstream_error(status, correlation_id));
        }
        response
            .json::<T>()
            .await
            .map(|value| (value, correlation_id))
            .map_err(|_| {
                DeliveryError::Internal(DeliveryErrorMeta {
                    correlation_id,
                    retry_after_ms: None,
                    retryable: false,
                })
            })
    }

    async fn owner_get<T: DeserializeOwned>(
        authority: &SecurityAuthority,
        path: &str,
    ) -> Result<(T, Uuid), DeliveryError> {
        let response = authority
            .state
            .auth_http()
            .get(format!("{}{}", authority.state.auth_http_origin(), path))
            .header("accept", "application/json")
            .header(
                "x-erpx-access-token",
                authority.credential.access_token.expose(),
            )
            .header("x-erpx-actor", &authority.credential.principal_ref)
            .header(
                "x-erpx-session",
                authority.credential.session_id.to_string(),
            )
            .send()
            .await
            .map_err(|_| DeliveryError::Unavailable(error_meta(true, Some(500))))?;
        decode_owner(response).await
    }

    async fn owner_post<T: DeserializeOwned>(
        authority: &SecurityAuthority,
        path: &str,
        body: serde_json::Value,
    ) -> Result<(T, Uuid), DeliveryError> {
        let response = authority
            .state
            .auth_http()
            .post(format!("{}{}", authority.state.auth_http_origin(), path))
            .header("accept", "application/json")
            .header(
                "x-erpx-access-token",
                authority.credential.access_token.expose(),
            )
            .header("x-erpx-actor", &authority.credential.principal_ref)
            .header(
                "x-erpx-session",
                authority.credential.session_id.to_string(),
            )
            .json(&body)
            .send()
            .await
            .map_err(|_| DeliveryError::Unavailable(error_meta(true, Some(500))))?;
        decode_owner(response).await
    }

    fn security_page(owner: OwnerSecurityState, correlation_id: Uuid) -> SecurityFactorPage {
        SecurityFactorPage {
            directory_managed: owner.directory_managed,
            totp_enabled: owner.totp_enabled,
            totp_enabled_at: owner.totp_enabled_at,
            passkey_count: owner.passkey_count,
            recovery_codes_total: owner.recovery_codes_total,
            recovery_codes_remaining: owner.recovery_codes_remaining,
            recovery_codes_issued_at: owner.recovery_codes_issued_at,
            measured_at: owner.measured_at,
            freshness: "current".into(),
            redacted_fields: vec![
                "access_token".into(),
                "credential_id".into(),
                "credential_public_key".into(),
                "totp_secret".into(),
                "recovery_code_hash".into(),
            ],
            correlation_id,
        }
    }

    async fn owner_security_state(
        authority: &SecurityAuthority,
    ) -> Result<SecurityFactorPage, DeliveryError> {
        let (owner, correlation_id) =
            owner_get::<OwnerSecurityState>(authority, "/auth/security-factors").await?;
        Ok(security_page(owner, correlation_id))
    }

    fn security_receipt(
        owner: &OwnerSecurityReceipt,
        occurred_at: String,
        invalidation_tags: Vec<String>,
    ) -> Result<SecurityCommandReceipt, DeliveryError> {
        Ok(SecurityCommandReceipt {
            command_id: Uuid::parse_str(&owner.command_id)
                .map_err(|_| DeliveryError::Internal(error_meta(false, None)))?,
            audit_reference: Uuid::parse_str(&owner.audit_reference)
                .map_err(|_| DeliveryError::Internal(error_meta(false, None)))?,
            replayed: owner.replayed,
            occurred_at,
            invalidation_tags,
        })
    }

    async fn owner_device_page(
        authority: &SecurityAuthority,
        cursor: Option<&str>,
        page_size: u16,
    ) -> Result<DevicePage, DeliveryError> {
        let mut path = format!(
            "{}/auth/devices?page_size={page_size}",
            authority.state.auth_http_origin()
        );
        if let Some(cursor) = cursor {
            path.push_str("&cursor=");
            path.push_str(cursor);
        }
        let response = authority
            .state
            .auth_http()
            .get(path)
            .header("accept", "application/json")
            .header(
                "x-erpx-access-token",
                authority.credential.access_token.expose(),
            )
            .header("x-erpx-actor", &authority.credential.principal_ref)
            .header(
                "x-erpx-session",
                authority.credential.session_id.to_string(),
            )
            .send()
            .await
            .map_err(|_| DeliveryError::Unavailable(error_meta(true, Some(500))))?;
        let (page, correlation_id) = decode_owner::<OwnerDevicePage>(response).await?;
        let items = page
            .devices
            .into_iter()
            .map(|device| {
                Ok(SecurityDevice {
                    id: Uuid::parse_str(&device.id)
                        .map_err(|_| DeliveryError::Internal(error_meta(false, None)))?,
                    label: device.label,
                    created_at: device.created_at,
                    last_seen_at: device.last_seen_at,
                    revoked_at: device.revoked_at,
                    current: device.current,
                })
            })
            .collect::<Result<Vec<_>, DeliveryError>>()?;
        Ok(DevicePage {
            items,
            next_cursor: page.next_cursor,
            measured_at: page.measured_at,
            freshness: "current".into(),
            redacted_fields: vec![
                "access_token".into(),
                "refresh_token".into(),
                "credential_material".into(),
                "session_identifier".into(),
            ],
            correlation_id,
        })
    }

    pub(super) async fn list_devices(
        request: DeviceListRequest,
    ) -> Result<DevicePage, DeliveryError> {
        let authority = authority()?;
        owner_device_page(&authority, request.cursor.as_deref(), request.page_size).await
    }

    pub(super) async fn revoke_device(
        request: RevokeDeviceRequest,
    ) -> Result<RevokeDeviceOutcome, DeliveryError> {
        let authority = authority()?;
        let response = authority
            .state
            .auth_http()
            .post(format!(
                "{}/auth/devices/{}/revoke",
                authority.state.auth_http_origin(),
                request.device_id
            ))
            .header("accept", "application/json")
            .header(
                "x-erpx-access-token",
                authority.credential.access_token.expose(),
            )
            .header("x-erpx-actor", &authority.credential.principal_ref)
            .header(
                "x-erpx-session",
                authority.credential.session_id.to_string(),
            )
            .json(&serde_json::json!({
                "idempotency_key": request.idempotency_key,
                "purpose": request.purpose,
                "locale": request.locale,
            }))
            .send()
            .await
            .map_err(|_| DeliveryError::Unavailable(error_meta(true, Some(500))))?;
        let (receipt, _) = decode_owner::<OwnerRevokeReceipt>(response).await?;
        if !receipt.revoked {
            return Err(DeliveryError::Internal(error_meta(false, None)));
        }
        let command_id = Uuid::parse_str(&receipt.command_id)
            .map_err(|_| DeliveryError::Internal(error_meta(false, None)))?;
        let audit_reference = Uuid::parse_str(&receipt.audit_reference)
            .map_err(|_| DeliveryError::Internal(error_meta(false, None)))?;
        let page = owner_device_page(&authority, None, MAX_DEVICE_PAGE_SIZE).await?;
        let device = page
            .items
            .into_iter()
            .find(|device| device.id == request.device_id);
        let authoritative_state = if device
            .as_ref()
            .is_some_and(|device| device.revoked_at.is_some())
        {
            "confirmed"
        } else {
            "pending"
        };
        Ok(RevokeDeviceOutcome {
            receipt: DeviceCommandReceipt {
                command_id,
                device_id: request.device_id,
                replayed: receipt.replayed,
                revoked_at: receipt.revoked_at,
                invalidation_tags: vec![
                    "auth:devices".into(),
                    "auth:sessions".into(),
                    format!("auth:device:{}", request.device_id),
                ],
                audit_reference,
            },
            device,
            authoritative_state: authoritative_state.into(),
        })
    }

    pub(super) async fn security_state() -> Result<SecurityFactorPage, DeliveryError> {
        let authority = authority()?;
        owner_security_state(&authority).await
    }

    pub(super) async fn start_totp() -> Result<TotpEnrollmentView, DeliveryError> {
        let authority = authority()?;
        let (owner, _) = owner_post::<OwnerTotpEnrollment>(
            &authority,
            "/auth/totp/enrollment/start",
            serde_json::json!({}),
        )
        .await?;
        Ok(TotpEnrollmentView {
            enrollment_id: Uuid::parse_str(&owner.enrollment_id)
                .map_err(|_| DeliveryError::Internal(error_meta(false, None)))?,
            secret: owner.secret,
            otpauth_uri: owner.otpauth_uri,
            issued_at: owner.issued_at,
            expires_at: owner.expires_at,
        })
    }

    pub(super) async fn verify_totp(
        request: VerifyTotpRequest,
    ) -> Result<TotpVerifiedOutcome, DeliveryError> {
        let authority = authority()?;
        let (owner, _) = owner_post::<OwnerTotpVerified>(
            &authority,
            "/auth/totp/enrollment/verify",
            serde_json::json!({
                "enrollment_id": request.enrollment_id,
                "code": request.code,
            }),
        )
        .await?;
        if !owner.verified {
            return Err(DeliveryError::Internal(error_meta(false, None)));
        }
        let state = owner_security_state(&authority).await?;
        Ok(TotpVerifiedOutcome {
            verified: state.totp_enabled,
            state,
        })
    }

    pub(super) async fn remove_totp(
        request: HighRiskSecurityRequest,
    ) -> Result<RemoveTotpOutcome, DeliveryError> {
        let authority = authority()?;
        let (owner, _) = owner_post::<OwnerTotpRemoved>(
            &authority,
            "/auth/totp/remove",
            serde_json::json!({
                "mfa_code": request.mfa_code,
                "idempotency_key": request.idempotency_key,
                "purpose": request.purpose,
                "locale": request.locale,
            }),
        )
        .await?;
        if !owner.removed {
            return Err(DeliveryError::Internal(error_meta(false, None)));
        }
        let receipt = security_receipt(
            &owner.receipt,
            owner.removed_at,
            vec!["auth:security-factors".into(), "auth:totp".into()],
        )?;
        let state = owner_security_state(&authority).await?;
        let authoritative_state = if state.totp_enabled {
            "pending"
        } else {
            "confirmed"
        };
        Ok(RemoveTotpOutcome {
            receipt,
            state,
            authoritative_state: authoritative_state.into(),
        })
    }

    pub(super) async fn rotate_recovery(
        request: HighRiskSecurityRequest,
    ) -> Result<RecoveryCodeRotationView, DeliveryError> {
        let authority = authority()?;
        let (owner, _) = owner_post::<OwnerRecoveryRotation>(
            &authority,
            "/auth/recovery-codes",
            serde_json::json!({
                "count": 8,
                "mfa_code": request.mfa_code,
                "idempotency_key": request.idempotency_key,
                "purpose": request.purpose,
                "locale": request.locale,
            }),
        )
        .await?;
        if !owner.issued || (!owner.receipt.replayed && owner.codes.len() != owner.count) {
            return Err(DeliveryError::Internal(error_meta(false, None)));
        }
        let expected_count = u16::try_from(owner.count)
            .map_err(|_| DeliveryError::Internal(error_meta(false, None)))?;
        let receipt = security_receipt(
            &owner.receipt,
            owner.issued_at,
            vec!["auth:security-factors".into(), "auth:recovery-codes".into()],
        )?;
        let state = owner_security_state(&authority).await?;
        let authoritative_state = if state.recovery_codes_total == expected_count
            && state.recovery_codes_remaining == expected_count
        {
            "confirmed"
        } else {
            "pending"
        };
        Ok(RecoveryCodeRotationView {
            receipt,
            codes: owner.codes,
            state,
            authoritative_state: authoritative_state.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_query_rejects_unbounded_or_unsafe_input() {
        for request in [
            DeviceListRequest {
                cursor: None,
                page_size: 0,
                locale: "en".into(),
            },
            DeviceListRequest {
                cursor: None,
                page_size: 101,
                locale: "th".into(),
            },
            DeviceListRequest {
                cursor: Some("bad\nvalue".into()),
                page_size: 25,
                locale: "en".into(),
            },
        ] {
            assert!(matches!(
                validate_list_request(&request),
                Err(DeliveryError::Validation(_))
            ));
        }
    }

    #[test]
    fn revoke_requires_explicit_confirmation_and_audit_purpose() {
        let mut request = RevokeDeviceRequest {
            device_id: Uuid::new_v4(),
            idempotency_key: Uuid::new_v4().to_string(),
            locale: "th".into(),
            purpose: "เพิกถอนอุปกรณ์ที่ไม่รู้จัก".into(),
            confirmed: false,
        };
        assert!(matches!(
            validate_revoke_request(&request),
            Err(DeliveryError::Validation(_))
        ));
        request.confirmed = true;
        assert_eq!(validate_revoke_request(&request), Ok(()));
        request.purpose.clear();
        assert!(matches!(
            validate_revoke_request(&request),
            Err(DeliveryError::Validation(_))
        ));
    }

    #[test]
    fn device_contract_cannot_represent_credentials() {
        let encoded = serde_json::to_string(&SecurityDevice {
            id: Uuid::new_v4(),
            label: "ERPX console".into(),
            created_at: "2026-08-20T00:00:00Z".into(),
            last_seen_at: "2026-08-20T00:00:00Z".into(),
            revoked_at: None,
            current: true,
        })
        .expect("serialize safe device");
        for forbidden in ["access_token", "refresh_token", "credential_id", "secret"] {
            assert!(!encoded.contains(forbidden));
        }
    }

    #[test]
    fn security_commands_require_confirmation_current_mfa_and_purpose() {
        let mut request = HighRiskSecurityRequest {
            mfa_code: "123456".into(),
            idempotency_key: Uuid::new_v4().to_string(),
            locale: "th".into(),
            purpose: "หมุนเวียนรหัสกู้คืน".into(),
            confirmed: true,
        };
        assert_eq!(validate_high_risk_request(&request), Ok(()));
        request.mfa_code = "12345".into();
        assert!(matches!(
            validate_high_risk_request(&request),
            Err(DeliveryError::Validation(_))
        ));
        request.mfa_code = "123456".into();
        request.confirmed = false;
        assert!(matches!(
            validate_high_risk_request(&request),
            Err(DeliveryError::Validation(_))
        ));
    }

    #[test]
    fn one_time_security_dtos_redact_debug_and_do_not_model_credentials() {
        let enrollment = TotpEnrollmentView {
            enrollment_id: Uuid::new_v4(),
            secret: "ONE-TIME-TOTP".into(),
            otpauth_uri: "otpauth://one-time".into(),
            issued_at: "2026-08-21T00:00:00Z".into(),
            expires_at: "2026-08-21T00:15:00Z".into(),
        };
        let debug = format!("{enrollment:?}");
        assert!(!debug.contains("ONE-TIME-TOTP"));
        assert!(!debug.contains("otpauth://one-time"));
        let state = SecurityFactorPage {
            directory_managed: false,
            totp_enabled: true,
            totp_enabled_at: Some("2026-08-21T00:00:00Z".into()),
            passkey_count: 0,
            recovery_codes_total: 8,
            recovery_codes_remaining: 8,
            recovery_codes_issued_at: Some("2026-08-21T00:00:00Z".into()),
            measured_at: "2026-08-21T00:00:00Z".into(),
            freshness: "current".into(),
            redacted_fields: vec!["credential_id".into()],
            correlation_id: Uuid::new_v4(),
        };
        let encoded = serde_json::to_string(&state).expect("safe factor state");
        for forbidden in ["public_key", "recovery_code_hash", "access_token"] {
            assert!(!encoded.contains(forbidden));
        }
    }
}
