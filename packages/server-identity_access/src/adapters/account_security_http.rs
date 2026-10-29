#[cfg(feature = "contracts")]
use crate::public::account_contracts::{AccountAuthError, AccountAuthErrorResponse};
use crate::{
    adapters::account_http::{failure, invalid, json, one_header, presented, private},
    application::account_security::AccountSecurity,
    domain::account_auth::RequestAudience,
    public::{account_contracts::AccountRevision, account_security_http::*},
};
use axum::{
    Json, Router,
    extract::{Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::Arc;

/// Explicit private composition only; this does not mount into the old runtime.
pub fn account_security_router(service: Arc<AccountSecurity>) -> Router {
    Router::new()
        .route("/api/v1/account/iam/mfa/enroll", post(enroll))
        .route("/api/v1/account/iam/mfa/verify", post(verify))
        .route("/api/v1/account/security", get(read).post(revoke))
        .layer(axum::middleware::from_fn(
            |request: Request, next: axum::middleware::Next| async move {
                let mut response = private(next.run(request).await, None);
                response.headers_mut().insert(
                    header::CACHE_CONTROL,
                    axum::http::HeaderValue::from_static("private, no-store"),
                );
                response.headers_mut().insert(
                    header::REFERRER_POLICY,
                    axum::http::HeaderValue::from_static("no-referrer"),
                );
                response
            },
        ))
        .with_state(service)
}
fn audience(
    service: &AccountSecurity,
    request: &Request,
    personal: bool,
    mutation: bool,
) -> Result<RequestAudience, BusinessError> {
    let host = one_header(request.headers(), "host")?
        .ok_or_else(|| invalid("MISSING_REQUEST_AUTHORITY"))?;
    let audience = service
        .policy
        .audience(host)
        .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))?;
    if personal && audience != RequestAudience::Account {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "NOT_FOUND",
            "Not found.",
        ));
    }
    let origin = one_header(request.headers(), "origin")?;
    // B0 personal POST requires an Origin; MFA keeps its existing missing-Origin
    // API behavior but rejects every supplied foreign origin on all audiences.
    if (personal && mutation && origin.is_none())
        || ((!personal || mutation) && origin.is_some_and(|o| o != format!("https://{host}")))
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the ERPX application to submit this request.",
        ));
    }
    Ok(audience)
}
fn failed(error: BusinessError) -> Response {
    let status = match error.code {
        "IAM_RATE_LIMIT" => Some(StatusCode::TOO_MANY_REQUESTS),
        "JSON_CONTENT_TYPE_REQUIRED" => Some(StatusCode::UNSUPPORTED_MEDIA_TYPE),
        "REQUEST_BODY_TOO_LARGE" => Some(StatusCode::PAYLOAD_TOO_LARGE),
        _ if error.kind == FailureKind::InvalidInput => Some(StatusCode::UNPROCESSABLE_ENTITY),
        _ => None,
    };
    let mut response = failure(error);
    if let Some(status) = status {
        *response.status_mut() = status;
    }
    response
}
#[cfg_attr(feature="contracts",utoipa::path(post,path="/api/v1/account/iam/mfa/enroll",request_body=AccountMfaEnrollInput,responses((status=415,body=AccountAuthErrorResponse),(status=413,body=AccountAuthErrorResponse),(status=503,body=AccountAuthErrorResponse),(status=422,body=AccountAuthErrorResponse),(status=409,body=AccountAuthErrorResponse),(status=401,body=AccountAuthErrorResponse),(status=200,body=AccountMfaEnrollmentResponse),(status=403,body=AccountAuthErrorResponse),(status=429,body=AccountAuthErrorResponse))))]
async fn enroll(State(service): State<Arc<AccountSecurity>>, request: Request) -> Response {
    let result = async {
        let audience = audience(&service, &request, false, true)?;
        let proof = presented(request.headers(), &audience)?;
        let _: AccountMfaEnrollInput = json(request).await?;
        let enrollment = service.enroll(&proof, audience).await?;
        Ok::<_, BusinessError>(
            Json(AccountMfaEnrollmentResponse {
                data: AccountMfaEnrollment {
                    secret: enrollment.secret,
                    uri: enrollment.uri,
                },
            })
            .into_response(),
        )
    }
    .await;
    result.unwrap_or_else(failed)
}
#[cfg_attr(feature="contracts",utoipa::path(post,path="/api/v1/account/iam/mfa/verify",request_body=AccountMfaVerifyInput,responses((status=415,body=AccountAuthErrorResponse),(status=413,body=AccountAuthErrorResponse),(status=503,body=AccountAuthErrorResponse),(status=409,body=AccountAuthErrorResponse),(status=401,body=AccountAuthErrorResponse),(status=200,body=AccountMfaVerifiedResponse),(status=403,body=AccountAuthErrorResponse),(status=422,body=AccountAuthErrorResponse),(status=429,body=AccountAuthErrorResponse))))]
async fn verify(State(service): State<Arc<AccountSecurity>>, request: Request) -> Response {
    let result = async {
        let audience = audience(&service, &request, false, true)?;
        let proof = presented(request.headers(), &audience)?;
        let input: AccountMfaVerifyInput = json(request).await?;
        let result = service.verify(&proof, audience, input.code).await?;
        Ok::<_, BusinessError>(
            Json(AccountMfaVerifiedResponse {
                data: AccountMfaVerified {
                    verified_until: AccountRevision::new(result.verified_until)?,
                },
            })
            .into_response(),
        )
    }
    .await;
    result.unwrap_or_else(failed)
}
#[cfg_attr(feature="contracts",utoipa::path(get,path="/api/v1/account/security",responses((status=503,body=AccountAuthErrorResponse),(status=422,body=AccountAuthErrorResponse),(status=409,body=AccountAuthErrorResponse),(status=403,body=AccountAuthErrorResponse),(status=200,body=AccountSecurityResponse),(status=401,body=AccountAuthErrorResponse))))]
async fn read(State(service): State<Arc<AccountSecurity>>, request: Request) -> Response {
    let result = async {
        let audience = audience(&service, &request, true, false)?;
        if request.uri().query().is_some_and(|q| !q.is_empty()) {
            return Err(invalid("ACCOUNT_SCOPE_NOT_ALLOWED"));
        }
        let proof = presented(request.headers(), &audience)?;
        drop(request);
        Ok::<_, BusinessError>(
            Json(AccountSecurityResponse {
                data: service.read(&proof).await?.try_into()?,
            })
            .into_response(),
        )
    }
    .await;
    result.unwrap_or_else(failed)
}
#[cfg_attr(feature="contracts",utoipa::path(post,path="/api/v1/account/security",request_body=AccountSecurityInput,responses((status=415,body=AccountAuthErrorResponse),(status=413,body=AccountAuthErrorResponse),(status=503,body=AccountAuthErrorResponse),(status=422,body=AccountAuthErrorResponse),(status=200,body=AccountSecurityRevokeResponse),(status=401,body=AccountAuthErrorResponse),(status=403,body=AccountAuthErrorResponse),(status=409,body=AccountAuthErrorResponse))))]
async fn revoke(State(service): State<Arc<AccountSecurity>>, request: Request) -> Response {
    let result = async {
        let audience = audience(&service, &request, true, true)?;
        let proof = presented(request.headers(), &audience)?;
        let key = one_header(request.headers(), "idempotency-key")?
            .unwrap_or_default()
            .to_owned();
        let input: AccountSecurityInput = json(request).await?;
        let result = service.revoke(&proof, input.into(), key).await?;
        let mut response = Json(AccountSecurityRevokeResponse {
            data: AccountSecurityRevoked {
                revoked: result.revoked,
                count: AccountRevision::new(
                    i64::try_from(result.count).map_err(|_| invalid("INVALID_SESSION_COUNT"))?,
                )?,
                reauthenticate: result.reauthenticate,
            },
        })
        .into_response();
        response.headers_mut().insert(
            header::REFERRER_POLICY,
            axum::http::HeaderValue::from_static("no-referrer"),
        );
        Ok::<_, BusinessError>(response)
    }
    .await;
    result.unwrap_or_else(failed)
}
#[cfg(feature = "contracts")]
pub fn security_openapi() -> utoipa::openapi::OpenApi {
    use utoipa::OpenApi;
    #[derive(OpenApi)]
    #[openapi(
        paths(enroll, verify, read, revoke),
        components(schemas(
            AccountRevision,
            AccountAuthError,
            AccountAuthErrorResponse,
            AccountMfaEnrollInput,
            AccountMfaVerifyInput,
            AccountMfaEnrollment,
            AccountMfaVerified,
            AccountMfaEnrollmentResponse,
            AccountMfaVerifiedResponse,
            AccountSecurityFamily,
            AccountSecuritySeed,
            AccountSecurityInput,
            AccountSecurityScope,
            AccountSecurityResponse,
            AccountSecurityRevoked,
            AccountSecurityRevokeResponse
        ))
    )]
    struct Api;
    Api::openapi()
}
