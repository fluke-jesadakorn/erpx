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
}
