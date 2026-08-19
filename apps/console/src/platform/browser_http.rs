//! Same-origin browser transport for session-scoped Console surfaces.

use serde::de::DeserializeOwned;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

const RESPONSE_LIMIT: usize = 2 * 1024 * 1024;

pub async fn json_request<T>(method: &str, path: &str, body: Option<String>) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let init = web_sys::RequestInit::new();
    init.set_method(method);
    init.set_mode(web_sys::RequestMode::SameOrigin);
    init.set_credentials(web_sys::RequestCredentials::Include);
    init.set_redirect(web_sys::RequestRedirect::Error);
    if let Some(body) = body.as_deref() {
        init.set_body(&JsValue::from_str(body));
    }

    let request = web_sys::Request::new_with_str_and_init(path, &init)
        .map_err(|_| "Unable to create the session request.".to_owned())?;
    if body.is_some() {
        request
            .headers()
            .set("content-type", "application/json")
            .map_err(|_| "Unable to prepare the session request.".to_owned())?;
    }

    let window =
        web_sys::window().ok_or_else(|| "The browser window is unavailable.".to_owned())?;
    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| "The session service is unavailable.".to_owned())?
        .dyn_into::<web_sys::Response>()
        .map_err(|_| "The session service returned an invalid response.".to_owned())?;
    let status = response.status();
    if !(200..300).contains(&status) {
        return Err(match status {
            401 | 403 => "Your session is no longer active. Sign in again.".to_owned(),
            404 => "The requested session surface was not found.".to_owned(),
            _ => "The session service rejected the request. Try again.".to_owned(),
        });
    }

    let buffer = JsFuture::from(
        response
            .array_buffer()
            .map_err(|_| "The session service returned an unreadable response.".to_owned())?,
    )
    .await
    .map_err(|_| "The session service returned an unreadable response.".to_owned())?;
    let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
    if bytes.len() > RESPONSE_LIMIT {
        return Err("The session service returned an oversized response.".to_owned());
    }

    serde_json::from_slice(&bytes)
        .map_err(|_| "The session service returned an invalid payload.".to_owned())
}

pub fn replace_location(path: &str) -> Result<(), String> {
    web_sys::window()
        .ok_or_else(|| "The browser window is unavailable.".to_owned())?
        .location()
        .replace(path)
        .map_err(|_| "The workspace could not be opened.".to_owned())
}
