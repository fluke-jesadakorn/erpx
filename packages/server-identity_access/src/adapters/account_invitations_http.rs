use crate::{
    adapters::account_http::{central, failure, json, private},
    application::{account_auth::AccountAuthentication, account_invitations::AccountInvitations},
    public::account_contracts::*,
};
use axum::{
    Json, Router,
    extract::{Query, Request, State},
    response::{IntoResponse, Response},
    routing::get,
};
use erpx_server_shared::public::BusinessError;
use std::sync::Arc;
#[derive(Clone)]
struct InvitationHttpState {
    authentication: Arc<AccountAuthentication>,
    invitations: Arc<AccountInvitations>,
}
pub fn account_invitations_router(
    authentication: Arc<AccountAuthentication>,
    invitations: Arc<AccountInvitations>,
) -> Result<Router, BusinessError> {
    let policy = invitations.policy();
    if authentication.policy.account_origin != policy.account_origin
        || authentication.policy.account_host != policy.account_host
        || authentication.policy.environment != policy.environment
    {
        return Err(BusinessError::invalid(
            "ACCOUNT_AUTHORITY_MISMATCH",
            "Assemble Account services with the same exact authority.",
        ));
    }
    Ok(Router::new()
        .route("/api/v1/account-invitations", get(inspect).post(accept))
        .with_state(InvitationHttpState {
            authentication,
            invitations,
        }))
}
#[cfg_attr(feature="contracts",utoipa::path(get,path="/api/v1/account-invitations",params(("token"=String,Query,description="Private invitation bearer proof")),responses((status=200,body=AccountInvitationInspectResponse),(status=403,body=AccountAuthErrorResponse),(status=503,body=AccountAuthErrorResponse))))]
async fn inspect(State(state): State<InvitationHttpState>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    drop(body);
    let result = async {
        central(&state.authentication, &parts.headers, false, false)?;
        if parts.uri.query().is_some_and(|q| q.len() > 4096) {
            return Err(BusinessError::invalid(
                "INVALID_INVITATION_QUERY",
                "Use the current invitation link.",
            ));
        }
        let Query(input) = Query::<AccountInvitationInspectInput>::try_from_uri(&parts.uri)
            .map_err(|_| {
                BusinessError::invalid(
                    "INVALID_INVITATION_QUERY",
                    "Use the current invitation link.",
                )
            })?;
        let state = state.invitations.inspect(input.token).await?;
        Ok::<_, BusinessError>(private(
            Json(AccountInvitationInspectResponse {
                data: AccountInvitationInfo {
                    name: state.name,
                    email: state.email,
                    existing_account: false,
                },
            })
            .into_response(),
            None,
        ))
    }
    .await;
    result.unwrap_or_else(failure)
}
#[cfg_attr(feature="contracts",utoipa::path(post,path="/api/v1/account-invitations",request_body=AccountInvitationAcceptInput,responses((status=200,body=AccountInvitationAcceptResponse),(status=400,body=AccountAuthErrorResponse),(status=403,body=AccountAuthErrorResponse),(status=409,body=AccountAuthErrorResponse),(status=503,body=AccountAuthErrorResponse))))]
async fn accept(State(state): State<InvitationHttpState>, request: Request) -> Response {
    let result = async {
        central(&state.authentication, request.headers(), true, true)?;
        let input: AccountInvitationAcceptInput = json(request).await?;
        state
            .invitations
            .accept(input.token, input.password)
            .await?;
        Ok::<_, BusinessError>(private(
            Json(AccountInvitationAcceptResponse {
                data: AccountInvitationAccepted { activated: true },
            })
            .into_response(),
            None,
        ))
    }
    .await;
    result.unwrap_or_else(failure)
}
#[cfg(feature = "contracts")]
pub fn invitation_openapi() -> utoipa::openapi::OpenApi {
    use utoipa::OpenApi;
    #[derive(utoipa::OpenApi)]
    #[openapi(
        paths(inspect, accept),
        components(schemas(
            AccountInvitationInspectInput,
            AccountInvitationAcceptInput,
            AccountInvitationInfo,
            AccountInvitationInspectResponse,
            AccountInvitationAccepted,
            AccountInvitationAcceptResponse,
            AccountAuthError,
            AccountAuthErrorResponse
        ))
    )]
    struct Document;
    Document::openapi()
}
