//! Private HTTP ingress for the native Support session writer. Account/Holding
//! cookies and the rendered work-context CAS headers are verified again inside
//! the command's own transaction; this layer supplies no authority.
use crate::http_scope::{BusinessScopeSlugs, selected_company_target};
use crate::support_session_command::SupportSessionCommand;
use axum::{
    Router,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json,
};
use erpx_server_identity_access::public::{
    AccountPolicy,
    account_inbound_http::{
        CapturedHoldingMutation, parse_inbound_account_request, unique_request_header,
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind};
use erpx_server_support::public::session::{SessionAction, SessionRequest};
use erpx_server_workflow::public::commands::CommandJson;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::Arc,
};

#[derive(Clone)]
pub struct SupportSessionHttpTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub command: Arc<SupportSessionCommand<C>>,
}

#[derive(Clone)]
struct SupportSessionHttpState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String, String), Arc<SupportSessionCommand<C>>>,
}

pub fn support_session_http_router<C: Clock + Clone + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<SupportSessionHttpTarget<C>>,
) -> Result<Router, BusinessError> {
    policy.validate()?;
    let mut registered = BTreeMap::new();
    for target in targets {
        let key = (
            target.host.clone(),
            target.holding_slug.clone(),
            target.command.company.as_str().to_owned(),
        );
        if registered.insert(key, target.command).is_some() {
            return Err(BusinessError::invalid(
                "SUPPORT_SESSION_HTTP_CONFIGURATION_INVALID",
                "Configure distinct Support Holding targets.",
            ));
        }
    }
    if registered.is_empty() {
        return Err(BusinessError::invalid(
            "SUPPORT_SESSION_HTTP_CONFIGURATION_INVALID",
            "Configure at least one Support Holding target.",
        ));
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/support/sessions",
            post(post_session::<C>),
        )
        .with_state(SupportSessionHttpState {
            policy,
            scopes,
            targets: registered,
        }))
}

async fn post_session<C: Clock + Clone + Send + Sync + 'static>(
    State(state): State<SupportSessionHttpState<C>>,
    Path(holding_slug): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let result = async {
        let host = headers
            .get("host")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(not_found)?;
        let target = selected_company_target(&state.targets, host, &holding_slug, &headers)?;
        if let Some(origin) = unique_request_header(&headers, "origin")?
            && origin != format!("https://{host}")
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            ));
        }
        let key = unique_request_header(&headers, "idempotency-key")?.ok_or_else(|| {
            BusinessError::invalid("COMMAND_KEY_REQUIRED", "Idempotency-Key is required.")
        })?;
        if key.is_empty() || key.len() > 256 {
            return Err(BusinessError::invalid(
                "COMMAND_KEY_REQUIRED",
                "Idempotency-Key is required.",
            ));
        }
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        let (branch, department) = {
            let (branch, department) = state.scopes.slugs(&headers)?;
            (branch, department)
        };
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let value: Value = serde_json::from_slice(&body).map_err(|_| {
            BusinessError::invalid("INVALID_JSON", "Provide a valid Support session body.")
        })?;
        let wire = CommandJson::parse(std::str::from_utf8(&body).map_err(|_| {
            BusinessError::invalid("INVALID_JSON", "Provide a valid Support session body.")
        })?)?;
        let key = CommandKey::new(key)?;
        let proof = inbound.into_proof();
        let outcome = if value.get("id").is_none() {
            let input: SessionRequest = serde_json::from_value(value.clone()).map_err(|_| {
                BusinessError::invalid(
                    "SUPPORT_SESSION_INPUT_INVALID",
                    "Provide a valid session request.",
                )
            })?;
            target
                .request(host, proof, move || captured.resolve(), input, wire, key)
                .await
        } else {
            let id = value["id"].as_str().ok_or_else(|| {
                BusinessError::invalid(
                    "SUPPORT_SESSION_INPUT_INVALID",
                    "Provide the session id.",
                )
            })?;
            let mut change = value.clone();
            if let Some(object) = change.as_object_mut() {
                object.remove("id");
            }
            let input: SessionAction = serde_json::from_value(change).map_err(|_| {
                BusinessError::invalid(
                    "SUPPORT_SESSION_INPUT_INVALID",
                    "Provide a valid action.",
                )
            })?;
            let id = id.to_owned();
            target
                .change(
                    host,
                    proof,
                    move || captured.resolve(),
                    id,
                    input,
                    wire,
                    key,
                )
                .await
        };
        let outcome = outcome.map_err(|error| match error {
            PgTransactionError::Business(error) => error,
            PgTransactionError::Persistence(_) => BusinessError::new(
                FailureKind::Unavailable,
                "SUPPORT_SESSION_UNAVAILABLE",
                "The Support session writer is unavailable.",
            ),
        })?;
        let data: Value = serde_json::from_str(outcome.as_str()).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "SUPPORT_SESSION_UNAVAILABLE",
                "The Support session writer is unavailable.",
            )
        })?;
        Ok::<_, BusinessError>(Json(json!({"data": data})))
    }
    .await;
    match result {
        Ok(response) => response.into_response(),
        Err(error) => business_error(error),
    }
}

fn business_error(error: BusinessError) -> Response {
    let status = match error.kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    (
        status,
        Json(json!({"error": {"code": error.code, "message": error.message}})),
    )
        .into_response()
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
