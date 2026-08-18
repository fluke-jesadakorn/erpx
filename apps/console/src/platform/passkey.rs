//! Typed WebAuthn ceremony validation and platform providers.

use crate::platform::{
    PasskeyCeremony, PasskeyError, PasskeyFuture, PasskeyProvider, PasskeyRequest,
    PasskeyServerOptions,
};

const MIN_CHALLENGE_BYTES: usize = 16;
const MAX_CHALLENGE_BYTES: usize = 1_024;

pub fn validate_passkey_request(request: &PasskeyRequest) -> Result<(), PasskeyError> {
    if !valid_relying_party_id(&request.relying_party_id)
        || !origin_allows_relying_party(&request.origin, &request.relying_party_id)
    {
        return Err(PasskeyError::InvalidOrigin);
    }

    let challenge = match (&request.ceremony, &request.server_options) {
        (PasskeyCeremony::Registration, PasskeyServerOptions::Registration(options)) => {
            if options.public_key.rp.id != request.relying_party_id {
                return Err(PasskeyError::InvalidOrigin);
            }
            options.public_key.challenge.as_slice()
        }
        (PasskeyCeremony::Authentication, PasskeyServerOptions::Authentication(options)) => {
            if options.public_key.rp_id != request.relying_party_id {
                return Err(PasskeyError::InvalidOrigin);
            }
            options.public_key.challenge.as_slice()
        }
        _ => return Err(PasskeyError::Platform),
    };
    if !(MIN_CHALLENGE_BYTES..=MAX_CHALLENGE_BYTES).contains(&challenge.len()) {
        return Err(PasskeyError::Platform);
    }
    Ok(())
}

fn valid_relying_party_id(relying_party_id: &str) -> bool {
    !relying_party_id.is_empty()
        && relying_party_id.len() <= 253
        && !relying_party_id.starts_with('.')
        && !relying_party_id.ends_with('.')
        && relying_party_id.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn origin_allows_relying_party(origin: &str, relying_party_id: &str) -> bool {
    let Ok(url) = url::Url::parse(origin) else {
        return false;
    };
    if url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let local_http =
        url.scheme() == "http" && matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1");
    (url.scheme() == "https" || local_http)
        && (host == relying_party_id || host.ends_with(&format!(".{relying_party_id}")))
}

#[derive(Debug, Clone, Default)]
pub struct MacOsPasskeyProvider;

impl PasskeyProvider for MacOsPasskeyProvider {
    fn register(&self, request: PasskeyRequest) -> PasskeyFuture<'_> {
        Box::pin(async move {
            validate_passkey_request(&request)?;
            Err(PasskeyError::Unsupported)
        })
    }

    fn authenticate(&self, request: PasskeyRequest) -> PasskeyFuture<'_> {
        Box::pin(async move {
            validate_passkey_request(&request)?;
            Err(PasskeyError::Unsupported)
        })
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Default)]
pub struct WebPasskeyProvider;

#[cfg(target_arch = "wasm32")]
impl WebPasskeyProvider {
    pub fn is_available() -> bool {
        use wasm_bindgen::JsValue;

        web_sys::window().is_some_and(|window| {
            window.is_secure_context()
                && js_sys::Reflect::has(&window.navigator(), &JsValue::from_str("credentials"))
                    .unwrap_or(false)
                && js_sys::Reflect::has(&window, &JsValue::from_str("PublicKeyCredential"))
                    .unwrap_or(false)
        })
    }
}

#[cfg(target_arch = "wasm32")]
fn current_origin_matches(request: &PasskeyRequest) -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let Ok(origin) = window.location().origin() else {
        return false;
    };
    let Ok(current) = url::Url::parse(&origin) else {
        return false;
    };
    let Ok(requested) = url::Url::parse(&request.origin) else {
        return false;
    };
    current.origin() == requested.origin()
        && current.host_str().is_some_and(|host| {
            host == request.relying_party_id
                || host.ends_with(&format!(".{}", request.relying_party_id))
        })
}

#[cfg(target_arch = "wasm32")]
fn browser_error(error: &wasm_bindgen::JsValue) -> PasskeyError {
    let name = js_sys::Reflect::get(error, &wasm_bindgen::JsValue::from_str("name"))
        .ok()
        .and_then(|value| value.as_string())
        .unwrap_or_default();
    match name.as_str() {
        "AbortError" | "NotAllowedError" => PasskeyError::Cancelled,
        "NotSupportedError" => PasskeyError::Unsupported,
        "SecurityError" => PasskeyError::InvalidOrigin,
        _ => PasskeyError::Platform,
    }
}

#[cfg(target_arch = "wasm32")]
impl PasskeyProvider for WebPasskeyProvider {
    fn register(&self, request: PasskeyRequest) -> PasskeyFuture<'_> {
        Box::pin(async move {
            use wasm_bindgen::JsCast;

            validate_passkey_request(&request)?;
            if !Self::is_available() {
                return Err(PasskeyError::Unsupported);
            }
            if !current_origin_matches(&request) {
                return Err(PasskeyError::InvalidOrigin);
            }
            let PasskeyServerOptions::Registration(server_options) = request.server_options else {
                return Err(PasskeyError::Platform);
            };
            let options: web_sys::CredentialCreationOptions = server_options.into();
            let promise = web_sys::window()
                .ok_or(PasskeyError::Unsupported)?
                .navigator()
                .credentials()
                .create_with_options(&options)
                .map_err(|error| browser_error(&error))?;
            let credential = wasm_bindgen_futures::JsFuture::from(promise)
                .await
                .map_err(|error| browser_error(&error))?
                .dyn_into::<web_sys::PublicKeyCredential>()
                .map_err(|_| PasskeyError::Platform)?;
            let response = webauthn_rs_proto::RegisterPublicKeyCredential::from(credential);
            let credential_id = response.raw_id.as_slice().to_vec();
            let response_json =
                serde_json::to_value(response).map_err(|_| PasskeyError::Platform)?;
            Ok(crate::platform::PasskeyResponse {
                credential_id,
                response_json,
            })
        })
    }

    fn authenticate(&self, request: PasskeyRequest) -> PasskeyFuture<'_> {
        Box::pin(async move {
            use wasm_bindgen::JsCast;

            validate_passkey_request(&request)?;
            if !Self::is_available() {
                return Err(PasskeyError::Unsupported);
            }
            if !current_origin_matches(&request) {
                return Err(PasskeyError::InvalidOrigin);
            }
            let PasskeyServerOptions::Authentication(server_options) = request.server_options
            else {
                return Err(PasskeyError::Platform);
            };
            let options: web_sys::CredentialRequestOptions = server_options.into();
            let promise = web_sys::window()
                .ok_or(PasskeyError::Unsupported)?
                .navigator()
                .credentials()
                .get_with_options(&options)
                .map_err(|error| browser_error(&error))?;
            let credential = wasm_bindgen_futures::JsFuture::from(promise)
                .await
                .map_err(|error| browser_error(&error))?
                .dyn_into::<web_sys::PublicKeyCredential>()
                .map_err(|_| PasskeyError::Platform)?;
            let response = webauthn_rs_proto::PublicKeyCredential::from(credential);
            let credential_id = response.raw_id.as_slice().to_vec();
            let response_json =
                serde_json::to_value(response).map_err(|_| PasskeyError::Platform)?;
            Ok(crate::platform::PasskeyResponse {
                credential_id,
                response_json,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registration_request(origin: &str, rp_id: &str, challenge: &str) -> PasskeyRequest {
        let options = serde_json::from_value(serde_json::json!({
            "publicKey": {
                "rp": { "name": "ERPX", "id": rp_id },
                "user": { "id": "dGVzdC11c2Vy", "name": "test", "displayName": "Test" },
                "challenge": challenge,
                "pubKeyCredParams": [{ "type": "public-key", "alg": -7 }]
            }
        }))
        .expect("valid registration options");
        PasskeyRequest {
            ceremony: PasskeyCeremony::Registration,
            relying_party_id: rp_id.to_owned(),
            origin: origin.to_owned(),
            server_options: PasskeyServerOptions::Registration(options),
        }
    }

    #[test]
    fn validates_origin_rp_and_typed_challenge() {
        let valid = registration_request(
            "https://console.erpx.example",
            "erpx.example",
            "MDEyMzQ1Njc4OWFiY2RlZg",
        );
        assert_eq!(validate_passkey_request(&valid), Ok(()));

        for invalid in [
            registration_request(
                "http://console.erpx.example",
                "erpx.example",
                "MDEyMzQ1Njc4OWFiY2RlZg",
            ),
            registration_request(
                "https://attacker.example",
                "erpx.example",
                "MDEyMzQ1Njc4OWFiY2RlZg",
            ),
            registration_request(
                "https://erpx.example/path",
                "erpx.example",
                "MDEyMzQ1Njc4OWFiY2RlZg",
            ),
        ] {
            assert_eq!(
                validate_passkey_request(&invalid),
                Err(PasskeyError::InvalidOrigin)
            );
        }
    }

    #[test]
    fn localhost_http_is_allowed_for_development() {
        let request = registration_request(
            "http://localhost:3004",
            "localhost",
            "MDEyMzQ1Njc4OWFiY2RlZg",
        );
        assert_eq!(validate_passkey_request(&request), Ok(()));
    }
}
