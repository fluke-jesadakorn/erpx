use crate::{
    application::account_auth::{AccountAuthentication, PresentedSecrets},
    domain::account_auth::{OpaqueSecret, RequestAudience},
    public::account_contracts::*,
};
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::de::DeserializeOwned;
use std::{collections::BTreeSet, sync::Arc};

pub(crate) const ACCOUNT_COOKIE: &str = "__Secure-erpx-account.session_token";
pub(crate) const DEVICE_COOKIE: &str = "__Host-erpx-device";
pub(crate) const SAVED_PREFIX: &str = "__Secure-erpx-account.session_token_multi-";
const PRODUCT_COOKIE: &str = "__Host-erpx-product";
const BODY_LIMIT: usize = 8192;

/// Inbound HTTP only: policy/ports are assembled by the composition crate.
pub fn account_auth_router(service: Arc<AccountAuthentication>) -> Router {
    Router::new()
        .route("/api/auth/sign-in/email", post(sign_in))
        .route("/api/auth/get-session", get(get_session))
        .route("/api/auth/sign-out", post(sign_out))
        .layer(axum::middleware::from_fn_with_state(
            service.clone(),
            cors_response,
        ))
        .with_state(service.clone())
        .merge(super::account_device_http::device_accounts_router(service))
}

pub(crate) async fn cors_response(
    State(service): State<Arc<AccountAuthentication>>,
    request: Request,
    next: axum::middleware::Next,
) -> Response {
    let origin = if request
        .uri()
        .path()
        .starts_with("/api/v1/account/device-accounts")
    {
        central(&service, request.headers(), false, false)
            .ok()
            .flatten()
    } else {
        None
    };
    let response = next.run(request).await;
    private(response, origin.as_deref())
}

pub(crate) fn invalid(code: &'static str) -> BusinessError {
    BusinessError::invalid(code, "The Account request is invalid.")
}
pub(crate) fn one_header<'a>(
    headers: &'a HeaderMap,
    name: &'static str,
) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let value = values.next();
    if values.next().is_some() {
        return Err(invalid("AMBIGUOUS_REQUEST_AUTHORITY"));
    }
    value
        .map(|value| {
            value
                .to_str()
                .map_err(|_| invalid("INVALID_REQUEST_HEADER"))
        })
        .transpose()
}
fn authority(
    service: &AccountAuthentication,
    headers: &HeaderMap,
) -> Result<RequestAudience, BusinessError> {
    let host = one_header(headers, "host")?.ok_or_else(|| invalid("MISSING_REQUEST_AUTHORITY"))?;
    service
        .policy
        .audience(host)
        .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))
}
pub(crate) fn central(
    service: &AccountAuthentication,
    headers: &HeaderMap,
    mutation: bool,
    credentials: bool,
) -> Result<Option<String>, BusinessError> {
    if authority(service, headers)? != RequestAudience::Account {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "NOT_FOUND",
            "Not found.",
        ));
    }
    let origin = one_header(headers, "origin")?;
    if mutation && origin.is_none()
        || origin.is_some_and(|origin| {
            if credentials {
                origin != service.policy.account_origin
            } else {
                !service.policy.device_origin_allowed(origin)
            }
        })
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "INVALID_ORIGIN",
            "Open Account from an approved origin.",
        ));
    }
    Ok(origin.map(str::to_owned))
}
pub(crate) fn presented(
    headers: &HeaderMap,
    audience: &RequestAudience,
) -> Result<PresentedSecrets, BusinessError> {
    let mut names = BTreeSet::new();
    let mut proof = PresentedSecrets {
        browser: None,
        active: None,
        saved: Vec::new(),
    };
    let mut size = 0;
    for header in headers.get_all(header::COOKIE) {
        let value = header
            .to_str()
            .map_err(|_| invalid("INVALID_COOKIE_HEADER"))?;
        size += value.len();
        if size > BODY_LIMIT {
            return Err(invalid("COOKIE_HEADER_TOO_LARGE"));
        }
        for item in value
            .split(';')
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            let (name, value) = item
                .split_once('=')
                .ok_or_else(|| invalid("INVALID_COOKIE_HEADER"))?;
            if !names.insert(name.to_owned()) {
                return Err(invalid("AMBIGUOUS_COOKIE_HEADER"));
            }
            let active_name = match audience {
                RequestAudience::Account => ACCOUNT_COOKIE,
                RequestAudience::Holding { .. } => PRODUCT_COOKIE,
            };
            if name == active_name {
                proof.active = Some(OpaqueSecret::new(value.into())?);
            }
            if matches!(audience, RequestAudience::Account) {
                if name == DEVICE_COOKIE {
                    proof.browser = Some(OpaqueSecret::new(value.into())?);
                }
                if let Some(handle) = name.strip_prefix(SAVED_PREFIX) {
                    if handle.len() != 36
                        || !handle.bytes().all(|c| c.is_ascii_hexdigit() || c == b'-')
                        || proof.saved.len() >= 5
                    {
                        return Err(invalid("INVALID_SAVED_ACCOUNT_PROOF"));
                    }
                    proof
                        .saved
                        .push((handle.into(), OpaqueSecret::new(value.into())?));
                }
            }
        }
    }
    Ok(proof)
}
pub(crate) async fn json<T: DeserializeOwned>(request: Request) -> Result<T, BusinessError> {
    let content = one_header(request.headers(), "content-type")?.unwrap_or_default();
    if content.split(';').next().map(str::trim) != Some("application/json") {
        return Err(invalid("JSON_CONTENT_TYPE_REQUIRED"));
    }
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| invalid("REQUEST_BODY_TOO_LARGE"))?;
    serde_json::from_slice(&bytes).map_err(|_| invalid("INVALID_REQUEST_BODY"))
}
fn checked_return(path: Option<&str>) -> Result<(), BusinessError> {
    if path.is_some_and(|value| {
        !value.starts_with('/')
            || value.starts_with("//")
            || value.contains(['\\', '\r', '\n', '\0'])
    }) {
        return Err(invalid("INVALID_RETURN_PATH"));
    }
    Ok(())
}
pub(crate) fn cookie(response: &mut Response, name: &str, value: &str, persistent: bool) {
    let expiry = if persistent { "; Max-Age=43200" } else { "" };
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_str(&format!(
            "{name}={value}; Path=/; Secure; HttpOnly; SameSite=Lax{expiry}"
        ))
        .expect("generated opaque cookie is a valid header"),
    );
}
pub(crate) fn clear_cookie(response: &mut Response, name: &str) {
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_str(&format!(
            "{name}=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0"
        ))
        .expect("constant cookie name is a valid header"),
    );
}
pub(crate) fn private(mut response: Response, origin: Option<&str>) -> Response {
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
        .headers_mut()
        .insert(header::VARY, HeaderValue::from_static("Origin, Cookie"));
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    if let Some(origin) = origin
        && let Ok(value) = HeaderValue::from_str(origin)
    {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
            HeaderValue::from_static("true"),
        );
    }
    response
}
pub(crate) fn failure(error: BusinessError) -> Response {
    let status = match error.kind {
        FailureKind::InvalidInput => StatusCode::BAD_REQUEST,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    let response = if error.kind == FailureKind::NotFound {
        (status, "Not found.").into_response()
    } else {
        (
            status,
            Json(AccountAuthErrorResponse {
                error: AccountAuthError {
                    code: error.code.into(),
                    message: error.message.into(),
                },
            }),
        )
            .into_response()
    };
    private(response, None)
}
fn response_session(
    session: crate::domain::account_auth::AccountSession,
) -> Result<AccountSessionResponse, BusinessError> {
    let date = |millis: i64| {
        time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(millis) * 1_000_000)
            .ok()
            .and_then(|value| {
                value
                    .format(&time::format_description::well_known::Rfc3339)
                    .ok()
            })
            .ok_or_else(|| invalid("INVALID_SESSION_TIME"))
    };
    Ok(AccountSessionResponse {
        session: AccountSessionDto {
            id: session.id,
            user_id: session.user.id.clone(),
            expires_at: date(session.expires_at)?,
            created_at: date(session.authenticated_at)?,
            updated_at: date(session.authenticated_at)?,
        },
        user: session.user.into(),
    })
}

#[cfg_attr(feature="contracts",utoipa::path(post,path="/api/auth/sign-in/email",request_body=EmailSignInInput,responses((status=200,body=EmailSignInResponse),(status=400,body=AccountAuthErrorResponse),(status=401,body=AccountAuthErrorResponse),(status=403,body=AccountAuthErrorResponse),(status=409,body=AccountAuthErrorResponse),(status=503,body=AccountAuthErrorResponse))))]
async fn sign_in(State(service): State<Arc<AccountAuthentication>>, request: Request) -> Response {
    sign_in_response(service, request, SignInSurface::Account).await
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SignInSurface {
    Account,
    Device,
}
pub(crate) async fn sign_in_response(
    service: Arc<AccountAuthentication>,
    request: Request,
    surface: SignInSurface,
) -> Response {
    let result = async {
        let origin = central(
            &service,
            request.headers(),
            true,
            surface == SignInSurface::Account,
        )?;
        let proof = presented(request.headers(), &RequestAudience::Account)?;
        let input: EmailSignInInput = json(request).await?;
        if surface == SignInSurface::Account {
            checked_return(input.callback_url.as_deref())?;
        }
        let signed = service
            .sign_in(
                &input.email,
                input.password,
                &proof,
                input.revision.map(AccountRevision::get),
            )
            .await?;
        let retained = signed
            .result
            .devices
            .accounts
            .iter()
            .map(|account| account.handle.as_str())
            .collect::<BTreeSet<_>>();
        let removed = proof
            .saved
            .iter()
            .filter(|(handle, _)| !retained.contains(handle.as_str()))
            .map(|(handle, _)| format!("{SAVED_PREFIX}{handle}"))
            .collect::<Vec<_>>();
        let mut response = match surface {
            SignInSurface::Account => Json(EmailSignInResponse {
                user: signed.result.session.user.into(),
            })
            .into_response(),
            SignInSurface::Device => {
                Json(crate::public::account_device_http::DeviceAccountsResponse {
                    data: DeviceAccountsDto::try_from(signed.result.devices)?,
                })
                .into_response()
            }
        };
        for name in removed {
            clear_cookie(&mut response, &name);
        }
        if let Some(browser) = signed.browser_secret {
            cookie(&mut response, DEVICE_COOKIE, browser.expose(), true);
        }
        cookie(
            &mut response,
            ACCOUNT_COOKIE,
            signed.session_secret.expose(),
            input.remember_me != Some(false),
        );
        cookie(
            &mut response,
            &format!("{SAVED_PREFIX}{}", signed.handle),
            signed.session_secret.expose(),
            input.remember_me != Some(false),
        );
        Ok::<_, BusinessError>(private(response, origin.as_deref()))
    }
    .await;
    result.unwrap_or_else(|error| {
        if surface == SignInSurface::Device && error.code == "INVALID_EMAIL_OR_PASSWORD" {
            failure(BusinessError::new(
                error.kind,
                "SIGN_IN_FAILED",
                "Sign-in failed. Check your email, password or approved Directory sign-in.",
            ))
        } else {
            failure(error)
        }
    })
}
#[cfg_attr(feature="contracts",utoipa::path(get,path="/api/auth/get-session",responses((status=200,body=Option<AccountSessionResponse>),(status=400,body=AccountAuthErrorResponse))))]
async fn get_session(
    State(service): State<Arc<AccountAuthentication>>,
    request: Request,
) -> Response {
    let headers = request.into_parts().0.headers;
    let result = async {
        let audience = authority(&service, &headers)?;
        let proof = presented(&headers, &audience)?;
        let session = service
            .session(&proof, audience)
            .await?
            .map(response_session)
            .transpose()?;
        Ok::<_, BusinessError>(private(Json(session).into_response(), None))
    }
    .await;
    result.unwrap_or_else(failure)
}
#[cfg_attr(feature="contracts",utoipa::path(post,path="/api/auth/sign-out",responses((status=200,body=AccountOperationResponse),(status=403,body=AccountAuthErrorResponse))))]
async fn sign_out(State(service): State<Arc<AccountAuthentication>>, request: Request) -> Response {
    let headers = request.into_parts().0.headers;
    let result = async {
        let audience = authority(&service, &headers)?;
        let origin =
            one_header(&headers, "origin")?.ok_or_else(|| invalid("MISSING_REQUEST_ORIGIN"))?;
        let expected = match &audience {
            RequestAudience::Account => &service.policy.account_origin,
            RequestAudience::Holding { audience: target } => {
                &service
                    .policy
                    .holdings
                    .iter()
                    .find(|holding| &holding.audience == target)
                    .ok_or_else(|| invalid("INVALID_REQUEST_AUTHORITY"))?
                    .origin
            }
        };
        if origin != expected {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "INVALID_ORIGIN",
                "Sign out from the current Account or Holding origin.",
            ));
        }
        let proof = presented(&headers, &audience)?;
        service.sign_out(&proof, audience.clone()).await?;
        let mut response = Json(AccountOperationResponse { status: true }).into_response();
        clear_cookie(
            &mut response,
            if audience == RequestAudience::Account {
                ACCOUNT_COOKIE
            } else {
                PRODUCT_COOKIE
            },
        );
        Ok::<_, BusinessError>(private(response, None))
    }
    .await;
    result.unwrap_or_else(failure)
}
#[cfg(feature = "contracts")]
pub fn account_openapi() -> utoipa::openapi::OpenApi {
    use utoipa::OpenApi;
    #[derive(OpenApi)]
    #[openapi(
        paths(sign_in, get_session, sign_out),
        components(schemas(
            AccountRevision,
            EmailSignInInput,
            EmailSignInResponse,
            AccountUserDto,
            AccountSessionDto,
            AccountSessionResponse,
            DeviceSelectionInput,
            DeviceAccountsDto,
            DeviceAccountDto,
            AccountAuthError,
            AccountAuthErrorResponse,
            AccountOperationResponse
        ))
    )]
    struct AccountApi;
    let mut api = AccountApi::openapi();
    api.merge(super::account_device_http::device_openapi());
    api
}
