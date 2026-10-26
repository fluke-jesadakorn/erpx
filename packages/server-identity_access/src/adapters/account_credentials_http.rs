use crate::{
    adapters::account_http::{central, clear_cookie, cookie, failure, json, presented, private},
    application::{
        account_auth::{AccountAuthentication, PresentedSecrets, SignedIn},
        account_credentials::AccountCredentials,
    },
    domain::{
        account_auth::RequestAudience,
        account_credentials::{CredentialProofKind, account_return_path},
    },
    public::account_contracts::*,
};
use axum::{
    Json, Router,
    extract::{Query, Request, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use erpx_server_shared::public::BusinessError;
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone)]
struct CredentialHttpState {
    authentication: Arc<AccountAuthentication>,
    credentials: Arc<AccountCredentials>,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VerificationLinkInput {
    token: String,
    #[serde(rename = "callbackURL")]
    callback_url: Option<String>,
    revision: Option<String>,
}
pub fn account_credentials_router(
    authentication: Arc<AccountAuthentication>,
    credentials: Arc<AccountCredentials>,
) -> Result<Router, BusinessError> {
    if authentication.policy.account_origin != credentials.policy.account_origin
        || authentication.policy.account_host != credentials.policy.account_host
        || authentication.policy.environment != credentials.policy.environment
    {
        return Err(BusinessError::invalid(
            "ACCOUNT_AUTHORITY_MISMATCH",
            "Assemble Account services with the same exact authority.",
        ));
    }
    Ok(Router::new()
        .route("/api/auth/sign-up/email", post(sign_up))
        .route("/api/auth/request-password-reset", post(request_reset))
        .route(
            "/api/auth/send-verification-email",
            post(request_verification),
        )
        .route("/api/auth/reset-password", post(reset_password))
        .route("/api/auth/change-password", post(change_password))
        .route(
            "/api/auth/verify-email",
            get(verify_link).post(verify_email),
        )
        .with_state(CredentialHttpState {
            authentication,
            credentials,
        }))
}
fn signed(signed: SignedIn, presented: &PresentedSecrets) -> Response {
    let retained = signed
        .result
        .devices
        .accounts
        .iter()
        .map(|account| account.handle.as_str())
        .collect::<BTreeSet<_>>();
    let removed = presented
        .saved
        .iter()
        .filter(|(handle, _)| !retained.contains(handle.as_str()))
        .map(|(handle, _)| format!("__Secure-erpx-account.session_token_multi-{handle}"))
        .collect::<Vec<_>>();
    let mut response = Json(EmailSignInResponse {
        user: signed.result.session.user.into(),
    })
    .into_response();
    for name in removed {
        clear_cookie(&mut response, &name);
    }
    if let Some(browser) = signed.browser_secret {
        cookie(&mut response, "__Host-erpx-device", browser.expose(), true);
    }
    cookie(
        &mut response,
        "__Secure-erpx-account.session_token",
        signed.session_secret.expose(),
        true,
    );
    cookie(
        &mut response,
        &format!(
            "__Secure-erpx-account.session_token_multi-{}",
            signed.handle
        ),
        signed.session_secret.expose(),
        true,
    );
    private(response, None)
}
fn status() -> Response {
    private(
        Json(AccountOperationResponse { status: true }).into_response(),
        None,
    )
}

#[cfg_attr(feature="contracts",utoipa::path(post,path="/api/auth/sign-up/email",request_body=EmailSignUpInput,responses((status=200,body=AccountOperationResponse),(status=400,body=AccountAuthErrorResponse),(status=403,body=AccountAuthErrorResponse),(status=503,body=AccountAuthErrorResponse))))]
async fn sign_up(State(state): State<CredentialHttpState>, request: Request) -> Response {
    let result = async {
        central(&state.authentication, request.headers(), true, true)?;
        let input: EmailSignUpInput = json(request).await?;
        state
            .credentials
            .sign_up(
                input.name,
                &input.email,
                input.password,
                input.invitation_token,
                input.callback_url.as_deref(),
            )
            .await?;
        Ok::<_, BusinessError>(status())
    }
    .await;
    result.unwrap_or_else(failure)
}
#[cfg_attr(feature="contracts",utoipa::path(post,path="/api/auth/request-password-reset",request_body=PasswordResetRequestInput,responses((status=200,body=AccountOperationResponse),(status=400,body=AccountAuthErrorResponse),(status=403,body=AccountAuthErrorResponse),(status=503,body=AccountAuthErrorResponse))))]
async fn request_reset(State(state): State<CredentialHttpState>, request: Request) -> Response {
    let result = async {
        central(&state.authentication, request.headers(), true, true)?;
        let input: PasswordResetRequestInput = json(request).await?;
        state
            .credentials
            .request(
                CredentialProofKind::PasswordReset,
                &input.email,
                input.redirect_to.as_deref(),
            )
            .await?;
        Ok::<_, BusinessError>(status())
    }
    .await;
    result.unwrap_or_else(failure)
}
#[cfg_attr(feature="contracts",utoipa::path(post,path="/api/auth/send-verification-email",request_body=EmailVerificationRequestInput,responses((status=200,body=AccountOperationResponse),(status=400,body=AccountAuthErrorResponse),(status=403,body=AccountAuthErrorResponse),(status=503,body=AccountAuthErrorResponse))))]
async fn request_verification(
    State(state): State<CredentialHttpState>,
    request: Request,
) -> Response {
    let result = async {
        central(&state.authentication, request.headers(), true, true)?;
        let input: EmailVerificationRequestInput = json(request).await?;
        state
            .credentials
            .request(
                CredentialProofKind::EmailVerification,
                &input.email,
                input.callback_url.as_deref(),
            )
            .await?;
        Ok::<_, BusinessError>(status())
    }
    .await;
    result.unwrap_or_else(failure)
}
#[cfg_attr(feature="contracts",utoipa::path(post,path="/api/auth/reset-password",request_body=PasswordResetInput,responses((status=200,body=AccountOperationResponse),(status=400,body=AccountAuthErrorResponse),(status=401,body=AccountAuthErrorResponse),(status=403,body=AccountAuthErrorResponse),(status=503,body=AccountAuthErrorResponse))))]
async fn reset_password(State(state): State<CredentialHttpState>, request: Request) -> Response {
    let result = async {
        central(&state.authentication, request.headers(), true, true)?;
        let input: PasswordResetInput = json(request).await?;
        state
            .credentials
            .reset_password(input.token, input.new_password)
            .await?;
        Ok::<_, BusinessError>(status())
    }
    .await;
    result.unwrap_or_else(failure)
}
#[cfg_attr(feature="contracts",utoipa::path(post,path="/api/auth/change-password",request_body=PasswordChangeInput,responses((status=200,body=AccountOperationResponse),(status=400,body=AccountAuthErrorResponse),(status=401,body=AccountAuthErrorResponse),(status=403,body=AccountAuthErrorResponse),(status=503,body=AccountAuthErrorResponse))))]
async fn change_password(State(state): State<CredentialHttpState>, request: Request) -> Response {
    let result = async {
        central(&state.authentication, request.headers(), true, true)?;
        let proof = presented(request.headers(), &RequestAudience::Account)?;
        let input: PasswordChangeInput = json(request).await?;
        state
            .credentials
            .change_password(&proof, input.current_password, input.new_password)
            .await?;
        let mut response = status();
        clear_cookie(&mut response, "__Secure-erpx-account.session_token");
        for (handle, secret) in &proof.saved {
            if proof
                .active
                .as_ref()
                .is_some_and(|active| active.expose() == secret.expose())
            {
                clear_cookie(
                    &mut response,
                    &format!("__Secure-erpx-account.session_token_multi-{handle}"),
                );
            }
        }
        Ok::<_, BusinessError>(response)
    }
    .await;
    result.unwrap_or_else(failure)
}
#[cfg_attr(feature="contracts",utoipa::path(post,path="/api/auth/verify-email",request_body=EmailVerificationInput,responses((status=200,body=EmailSignInResponse),(status=400,body=AccountAuthErrorResponse),(status=401,body=AccountAuthErrorResponse),(status=403,body=AccountAuthErrorResponse),(status=409,body=AccountAuthErrorResponse),(status=503,body=AccountAuthErrorResponse))))]
async fn verify_email(State(state): State<CredentialHttpState>, request: Request) -> Response {
    let result = async {
        central(&state.authentication, request.headers(), true, true)?;
        let proof = presented(request.headers(), &RequestAudience::Account)?;
        let input: EmailVerificationInput = json(request).await?;
        account_return_path(input.callback_url.as_deref())?;
        let verified = state
            .credentials
            .verify_email(
                input.token,
                &proof,
                input.revision.map(AccountRevision::get),
            )
            .await?;
        Ok::<_, BusinessError>(signed(verified, &proof))
    }
    .await;
    result.unwrap_or_else(failure)
}
#[cfg_attr(feature="contracts",utoipa::path(get,path="/api/auth/verify-email",params(("token"=String,Query,description="Single-use email proof"),("callbackURL"=Option<String>,Query),("revision"=Option<i64>,Query)),responses((status=303,description="Verified and redirected within Account; session cookies are host-only"),(status=400,body=AccountAuthErrorResponse),(status=401,body=AccountAuthErrorResponse),(status=409,body=AccountAuthErrorResponse),(status=503,body=AccountAuthErrorResponse))))]
async fn verify_link(State(state): State<CredentialHttpState>, request: Request) -> Response {
    let (parts, _) = request.into_parts();
    let headers = parts.headers;
    let uri = parts.uri;

    let result = async {
        central(&state.authentication, &headers, false, true)?;
        let proof = presented(&headers, &RequestAudience::Account)?;
        if uri.query().is_some_and(|value| value.len() > 8192) {
            return Err(BusinessError::invalid(
                "INVALID_VERIFICATION_LINK",
                "Open a valid Account verification link.",
            ));
        }
        let Query(input) = Query::<VerificationLinkInput>::try_from_uri(&uri).map_err(|_| {
            BusinessError::invalid(
                "INVALID_VERIFICATION_LINK",
                "Open a valid Account verification link.",
            )
        })?;
        let revision = input
            .revision
            .as_deref()
            .map(serde_json::from_str::<AccountRevision>)
            .transpose()
            .map_err(|_| {
                BusinessError::invalid(
                    "INVALID_DEVICE_REVISION",
                    "Review the current saved account revision.",
                )
            })?;
        let path = account_return_path(input.callback_url.as_deref())?;
        let location = HeaderValue::from_str(&path).map_err(|_| {
            BusinessError::invalid("INVALID_RETURN_PATH", "Choose a valid Account return path.")
        })?;
        let verified = state
            .credentials
            .verify_email(input.token, &proof, revision.map(AccountRevision::get))
            .await?;
        let mut response = signed(verified, &proof);
        *response.status_mut() = StatusCode::SEE_OTHER;
        response.headers_mut().insert(header::LOCATION, location);
        *response.body_mut() = axum::body::Body::empty();
        Ok::<_, BusinessError>(response)
    }
    .await;
    result.unwrap_or_else(failure)
}

#[cfg(feature = "contracts")]
pub fn credentials_openapi() -> utoipa::openapi::OpenApi {
    use utoipa::OpenApi;
    #[derive(OpenApi)]
    #[openapi(
        paths(
            sign_up,
            request_reset,
            request_verification,
            reset_password,
            change_password,
            verify_email,
            verify_link
        ),
        components(schemas(
            EmailSignUpInput,
            PasswordResetRequestInput,
            PasswordResetInput,
            PasswordChangeInput,
            EmailVerificationRequestInput,
            EmailVerificationInput
        ))
    )]
    struct CredentialsApi;
    CredentialsApi::openapi()
}
