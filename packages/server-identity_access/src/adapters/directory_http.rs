//! Account-only credential ingress. Copied Directory observations never become authority.
use super::account_http::{
    ACCOUNT_COOKIE, DEVICE_COOKIE, SAVED_PREFIX, central, clear_cookie, cookie, failure, json,
    presented, private,
};
use crate::{
    application::{account_auth::AccountAuthentication, directory::DirectoryAuthentication},
    domain::account_auth::RequestAudience,
    public::account_contracts::{DirectorySignInInput, DirectorySignInResponse},
};
use axum::{
    Json, Router,
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use erpx_server_shared::public::BusinessError;
use std::{collections::BTreeSet, sync::Arc};
#[derive(Clone)]
struct DirectoryHttp {
    account: Arc<AccountAuthentication>,
    directory: Arc<DirectoryAuthentication>,
}
pub fn directory_account_router(
    account: Arc<AccountAuthentication>,
    directory: Arc<DirectoryAuthentication>,
) -> Router {
    Router::new()
        .route("/api/auth/directory/sign-in", post(sign_in))
        .with_state(DirectoryHttp { account, directory })
}
#[cfg_attr(feature="contracts",utoipa::path(post,path="/api/auth/directory/sign-in",request_body=DirectorySignInInput,responses((status=200,body=DirectorySignInResponse),(status=400,body=crate::public::account_contracts::AccountAuthErrorResponse),(status=401,body=crate::public::account_contracts::AccountAuthErrorResponse),(status=403,body=crate::public::account_contracts::AccountAuthErrorResponse),(status=409,body=crate::public::account_contracts::AccountAuthErrorResponse),(status=429,body=crate::public::account_contracts::AccountAuthErrorResponse),(status=503,body=crate::public::account_contracts::AccountAuthErrorResponse))))]
async fn sign_in(State(state): State<DirectoryHttp>, request: Request) -> Response {
    let result = async {
        central(&state.account, request.headers(), true, true)?;
        let proof = presented(request.headers(), &RequestAudience::Account)?;
        // B0 Request.json()/Zod keeps the last duplicate property and strips
        // unknown fields. Parse to Value first so Serde's struct duplicate-field
        // rejection does not change that wire behavior. Extra actor/grant/session
        // fields never participate in the typed credential intent.
        let value: serde_json::Value = json(request).await?;
        let input: DirectorySignInInput = serde_json::from_value(value).map_err(|_| {
            BusinessError::invalid("INVALID_REQUEST_BODY", "The Account request is invalid.")
        })?;
        if !connection_uuid(&input.connection_id) {
            return Err(BusinessError::invalid(
                "INVALID_DIRECTORY_CONNECTION",
                "Choose an approved Directory connection.",
            ));
        }
        let signed = state
            .directory
            .sign_in(
                &input.connection_id,
                &input.username,
                input.password,
                &proof,
                input.revision.map(|r| r.get()),
            )
            .await?;
        let retained = signed
            .result
            .devices
            .accounts
            .iter()
            .map(|a| a.handle.as_str())
            .collect::<BTreeSet<_>>();
        let mut response = Json(DirectorySignInResponse { success: true }).into_response();
        for (handle, _) in &proof.saved {
            if !retained.contains(handle.as_str()) {
                clear_cookie(&mut response, &format!("{SAVED_PREFIX}{handle}"));
            }
        }
        if let Some(secret) = signed.browser_secret {
            cookie(&mut response, DEVICE_COOKIE, secret.expose(), true);
        }
        cookie(
            &mut response,
            ACCOUNT_COOKIE,
            signed.session_secret.expose(),
            true,
        );
        cookie(
            &mut response,
            &format!("{SAVED_PREFIX}{}", signed.handle),
            signed.session_secret.expose(),
            true,
        );
        Ok::<_, BusinessError>(private(response, None))
    }
    .await;
    result.unwrap_or_else(|error| {
        let throttled = error.code == "IAM_RATE_LIMIT";
        let mut response = failure(error);
        if throttled {
            *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;
        }
        response
    })
}
fn connection_uuid(value: &str) -> bool {
    let b = value.as_bytes();
    if b.len() != 36 {
        return false;
    }
    if !b.iter().enumerate().all(|(i, b)| {
        if [8, 13, 18, 23].contains(&i) {
            *b == b'-'
        } else {
            b.is_ascii_hexdigit()
        }
    }) {
        return false;
    }
    // Exact Zod UUID shape: RFC variant/version1..8, plus nil/max UUID values.
    value.eq_ignore_ascii_case("00000000-0000-0000-0000-000000000000")
        || value == "ffffffff-ffff-ffff-ffff-ffffffffffff"
        || (matches!(b[14], b'1'..=b'8')
            && matches!(b[19], b'8' | b'9' | b'a' | b'A' | b'b' | b'B'))
}
#[cfg(feature = "contracts")]
pub fn directory_openapi() -> utoipa::openapi::OpenApi {
    #[derive(utoipa::OpenApi)]
    #[openapi(paths(sign_in))]
    struct Api;
    <Api as utoipa::OpenApi>::openapi()
}
