use crate::{
    adapters::account_http::{
        ACCOUNT_COOKIE, DEVICE_COOKIE, SAVED_PREFIX, SignInSurface, central, clear_cookie, cookie,
        cors_response, failure, invalid, json, one_header, presented, private, sign_in_response,
    },
    application::account_auth::AccountAuthentication,
    domain::account_auth::RequestAudience,
    public::{account_contracts::*, account_device_http::DeviceAccountsResponse},
};
use axum::{
    Json, Router,
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use erpx_server_shared::public::BusinessError;
use std::sync::Arc;

pub(crate) fn device_accounts_router(service: Arc<AccountAuthentication>) -> Router {
    Router::new()
        .route(
            "/api/v1/account/device-accounts",
            get(devices).delete(remove).options(preflight),
        )
        .route(
            "/api/v1/account/device-accounts/initialize",
            post(initialize).options(preflight),
        )
        .route(
            "/api/v1/account/device-accounts/active",
            post(select).options(preflight),
        )
        .route(
            "/api/v1/account/device-accounts/sign-in",
            post(device_sign_in).options(preflight),
        )
        .layer(axum::middleware::from_fn_with_state(
            service.clone(),
            cors_response,
        ))
        .with_state(service)
}
#[cfg_attr(feature="contracts",utoipa::path(post,path="/api/v1/account/device-accounts/sign-in",request_body=EmailSignInInput,responses((status=200,body=DeviceAccountsResponse),(status=400,body=AccountAuthErrorResponse),(status=401,body=AccountAuthErrorResponse),(status=403,body=AccountAuthErrorResponse),(status=409,body=AccountAuthErrorResponse),(status=503,body=AccountAuthErrorResponse))))]
async fn device_sign_in(
    State(service): State<Arc<AccountAuthentication>>,
    request: Request,
) -> Response {
    sign_in_response(service, request, SignInSurface::Device).await
}

#[cfg_attr(feature="contracts",utoipa::path(get,path="/api/v1/account/device-accounts",responses((status=200,body=DeviceAccountsResponse),(status=403,body=AccountAuthErrorResponse))))]
async fn devices(State(service): State<Arc<AccountAuthentication>>, request: Request) -> Response {
    let headers = request.into_parts().0.headers;
    let result = async {
        let origin = central(&service, &headers, false, false)?;
        let proof = presented(&headers, &RequestAudience::Account)?;
        let devices = DeviceAccountsDto::try_from(service.devices(&proof).await?)?;
        Ok::<_, BusinessError>(private(
            Json(DeviceAccountsResponse { data: devices }).into_response(),
            origin.as_deref(),
        ))
    }
    .await;
    result.unwrap_or_else(failure)
}
#[cfg_attr(feature="contracts",utoipa::path(post,path="/api/v1/account/device-accounts/initialize",request_body=DeviceInitializeInput,responses((status=200,body=DeviceAccountsResponse),(status=403,body=AccountAuthErrorResponse))))]
async fn initialize(
    State(service): State<Arc<AccountAuthentication>>,
    request: Request,
) -> Response {
    let result = async {
        let origin = central(&service, request.headers(), true, false)?;
        let proof = presented(request.headers(), &RequestAudience::Account)?;
        let input: DeviceInitializeInput = json(request).await?;
        let initialized = service
            .initialize(&proof, input.revision.map(AccountRevision::get))
            .await?;
        let mut response = Json(DeviceAccountsResponse {
            data: DeviceAccountsDto::try_from(initialized.devices)?,
        })
        .into_response();
        if let Some(secret) = initialized.browser_secret {
            cookie(&mut response, DEVICE_COOKIE, secret.expose(), true);
        }
        Ok::<_, BusinessError>(private(response, origin.as_deref()))
    }
    .await;
    result.unwrap_or_else(failure)
}
#[cfg_attr(feature="contracts",utoipa::path(post,path="/api/v1/account/device-accounts/active",request_body=DeviceSelectionInput,responses((status=200,body=DeviceAccountsResponse),(status=403,body=AccountAuthErrorResponse),(status=409,body=AccountAuthErrorResponse))))]
async fn select(State(service): State<Arc<AccountAuthentication>>, request: Request) -> Response {
    let result = async {
        let origin = central(&service, request.headers(), true, false)?;
        let proof = presented(request.headers(), &RequestAudience::Account)?;
        let input: DeviceSelectionInput = json(request).await?;
        let selected = service
            .select(&proof, input.handle, input.revision.get())
            .await?;
        let mut response = Json(DeviceAccountsResponse {
            data: DeviceAccountsDto::try_from(selected.devices)?,
        })
        .into_response();
        cookie(
            &mut response,
            ACCOUNT_COOKIE,
            selected.session_secret.expose(),
            true,
        );
        Ok::<_, BusinessError>(private(response, origin.as_deref()))
    }
    .await;
    result.unwrap_or_else(failure)
}
#[cfg_attr(feature="contracts",utoipa::path(delete,path="/api/v1/account/device-accounts",request_body=DeviceSelectionInput,responses((status=200,body=DeviceAccountsResponse),(status=403,body=AccountAuthErrorResponse),(status=409,body=AccountAuthErrorResponse))))]
async fn remove(State(service): State<Arc<AccountAuthentication>>, request: Request) -> Response {
    let result = async {
        let origin = central(&service, request.headers(), true, false)?;
        let proof = presented(request.headers(), &RequestAudience::Account)?;
        let input: DeviceSelectionInput = json(request).await?;
        let handle = input.handle.clone();
        let before = service.devices(&proof).await?;
        let was_active = before.revision == input.revision.get()
            && before
                .accounts
                .iter()
                .any(|account| account.handle == handle && account.current);
        let removed = service
            .remove(&proof, input.handle, input.revision.get())
            .await?;
        let mut response = Json(DeviceAccountsResponse {
            data: DeviceAccountsDto::try_from(removed)?,
        })
        .into_response();
        clear_cookie(&mut response, &format!("{SAVED_PREFIX}{handle}"));
        if was_active {
            clear_cookie(&mut response, ACCOUNT_COOKIE);
        }
        Ok::<_, BusinessError>(private(response, origin.as_deref()))
    }
    .await;
    result.unwrap_or_else(failure)
}
async fn preflight(
    State(service): State<Arc<AccountAuthentication>>,
    request: Request,
) -> Response {
    let headers = request.into_parts().0.headers;
    let result = async {
        let origin = central(&service, &headers, true, false)?;
        let method = one_header(&headers, "access-control-request-method")?
            .ok_or_else(|| invalid("INVALID_PREFLIGHT"))?;
        if !["GET", "POST", "DELETE"].contains(&method) {
            return Err(invalid("INVALID_PREFLIGHT"));
        }
        if one_header(&headers, "access-control-request-headers")?.is_some_and(|headers| {
            headers
                .split(',')
                .map(str::trim)
                .any(|name| !name.eq_ignore_ascii_case("content-type"))
        }) {
            return Err(invalid("INVALID_PREFLIGHT"));
        }
        let mut response = private(StatusCode::NO_CONTENT.into_response(), origin.as_deref());
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, POST, DELETE, OPTIONS"),
        );
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("Content-Type"),
        );
        response.headers_mut().insert(
            header::ACCESS_CONTROL_MAX_AGE,
            HeaderValue::from_static("600"),
        );
        Ok::<_, BusinessError>(response)
    }
    .await;
    result.unwrap_or_else(failure)
}

#[cfg(feature = "contracts")]
pub(crate) fn device_openapi() -> utoipa::openapi::OpenApi {
    use utoipa::OpenApi;
    #[derive(OpenApi)]
    #[openapi(
        paths(devices, initialize, select, remove, device_sign_in),
        components(schemas(
            DeviceAccountsResponse,
            DeviceAccountsDto,
            DeviceAccountDto,
            DeviceSelectionInput,
            DeviceInitializeInput,
            EmailSignInInput,
            AccountRevision,
            AccountAuthErrorResponse,
            AccountAuthError
        ))
    )]
    struct DeviceApi;
    DeviceApi::openapi()
}
