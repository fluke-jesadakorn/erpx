//! Owner-local Axum ingress for the native customer follow-up workspace read.
//!
//! Transport facts only: exact Host/Holding targets, workspace/company/user
//! headers supplied by the caller's authority and a bounded `orderId` query.
//! The owner store owns every statement and the exact `{data:...}` envelope.
use crate::adapters::native_r3_followup_workspace::application::WorkspaceFuture;
use crate::adapters::native_r3_followup_workspace::domain::{
    WORKSPACE_PATH, WorkspaceQuery, request_invalid,
};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router,
    State, StatusCode, get, header, to_bytes,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc};

pub const WORKSPACE_CONTEXT_COMPANY: HeaderName =
    HeaderName::from_static("x-erpx-context-company");
pub const WORKSPACE_CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");
pub const WORKSPACE_CONTEXT_PERMISSIONS: HeaderName =
    HeaderName::from_static("x-erpx-context-permissions");
pub const WORKSPACE_BRANCH: HeaderName = HeaderName::from_static("x-erpx-workspace-branch");
pub const WORKSPACE_DEPARTMENT: HeaderName = HeaderName::from_static("x-erpx-workspace-department");

pub struct FollowupWorkspaceTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    /// A composition-owned backend must authorize the session, selection and
    /// purchase and execute this read in its retained transaction. There is no
    /// direct pool/header fallback.
    pub backend: Arc<dyn FollowupWorkspaceBackend>,
}

pub trait FollowupWorkspaceBackend: Send + Sync {
    fn read<'a>(&'a self, host: &'a str, holding_slug: &'a str, headers: &'a HeaderMap, query: &'a WorkspaceQuery)
        -> WorkspaceFuture<'a, AuthorizedFollowupWorkspace>;
}

/// Produced by the composition backend after its session, selection and
/// purchase checks. `None` purchase is reserved for disabled package mode.
pub struct AuthorizedFollowupWorkspace {
    pub data: serde_json::Value,
    pub purchase: Option<serde_json::Value>,
}

type Targets = BTreeMap<(String, String), FollowupWorkspaceTarget>;

struct AppState {
    targets: Targets,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_FOLLOWUP_WORKSPACE_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':'))
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_'))
}

fn bounded_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn unique_header<'a>(
    headers: &'a HeaderMap,
    name: HeaderName,
) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(request_invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| request_invalid()))
        .transpose()
}

/// Register one exact `host` + `holding_slug` target per registered Holding.
pub fn router(targets: Vec<FollowupWorkspaceTarget>) -> Result<Router, BusinessError> {
    let mut registered = BTreeMap::new();
    for target in targets {
        if !valid_host(&target.host)
            || target.origin.strip_prefix("https://") != Some(target.host.as_str())
            || !valid_slug(&target.holding_slug)
            || registered
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if registered.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(WORKSPACE_PATH, get(read))
        .with_state(Arc::new(AppState { targets: registered })))
}

fn status(kind: FailureKind) -> StatusCode {
    match kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn error_response(error: BusinessError) -> Response {
    if error.code == "DATABASE_NUMBER_OUT_OF_RANGE" {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":{"code":"INTERNAL_ERROR","message":"The request could not be completed."}}))).into_response();
    }
    (
        status(error.kind),
        Json(json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn bounded(body: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(body)).into_response();
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    response
}

async fn read(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let headers = parts.headers.clone();
    let result = async move {
        let query = WorkspaceQuery::from_query(parts.uri.query());
        query.validate()?;
        to_bytes(body, 0).await.map_err(|_| request_invalid())?;

        let host = unique_header(&headers, header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.to_owned(), slug))
            .ok_or_else(missing)?;
        if unique_header(&headers, header::ORIGIN)?
            .is_some_and(|origin| origin != target.origin)
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "FOLLOWUP_WORKSPACE_ORIGIN",
                "The workspace is available from its Holding origin.",
            ));
        }
        for name in [WORKSPACE_BRANCH, WORKSPACE_DEPARTMENT] {
            if unique_header(&headers, name)?.is_some_and(|value| !bounded_id(value)) {
                return Err(request_invalid());
            }
        }
        let authorized = target.backend.read(host, &target.holding_slug, &headers, &query).await?;
        let mut data = authorized.data;
        let object = data.as_object_mut().ok_or_else(request_invalid)?;
        let can_create = object.get("canCreate").and_then(serde_json::Value::as_bool).ok_or_else(request_invalid)?;
        if let Some(purchase) = authorized.purchase {
            let new_work = purchase.get("newWork").and_then(serde_json::Value::as_bool).ok_or_else(request_invalid)?;
            object.insert("canCreate".into(), serde_json::Value::Bool(can_create && new_work));
            object.insert("purchase".into(), purchase);
        }
        Ok(json!({ "data": data }))
    }
    .await;
    result.map_or_else(error_response, bounded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn forged_permissions_cannot_replace_authority_and_bad_scope_fails_closed() {
        struct Deny;
        impl FollowupWorkspaceBackend for Deny {
            fn read<'a>(&'a self, _: &'a str, _: &'a str, _: &'a HeaderMap, _: &'a WorkspaceQuery) -> WorkspaceFuture<'a, AuthorizedFollowupWorkspace> {
                Box::pin(async { Err(BusinessError::new(FailureKind::Forbidden, "AUTHORITY_DENIED", "Denied.")) })
            }
        }
        let target = FollowupWorkspaceTarget { host: "acme.jesadakorn.com".into(), holding_slug: "acme".into(), origin: "https://acme.jesadakorn.com".into(), backend: Arc::new(Deny) };
        let state = Arc::new(AppState { targets: BTreeMap::from([((target.host.clone(), target.holding_slug.clone()), target)]) });
        let request = |scope: &str| Request::builder()
            .uri("/api/v1/holdings/acme/customer-followups")
            .header("host", "acme.jesadakorn.com")
            .header("x-erpx-context-permissions", "commerce.read,commerce.sales")
            .header("x-erpx-workspace-branch", scope)
            .body(erpx_platform_runtime_server::public::http_support::Body::empty()).unwrap();
        let forbidden = read(State(state.clone()), Path("acme".into()), request("branch")).await;
        assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
        let invalid = read(State(state), Path("acme".into()), request("../other")).await;
        assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn backend_purchase_controls_gate_creation() {
        struct Restricted;
        impl FollowupWorkspaceBackend for Restricted {
            fn read<'a>(&'a self, _: &'a str, _: &'a str, _: &'a HeaderMap, _: &'a WorkspaceQuery) -> WorkspaceFuture<'a, AuthorizedFollowupWorkspace> {
                Box::pin(async { Ok(AuthorizedFollowupWorkspace { data: json!({"canCreate": true}), purchase: Some(json!({"newWork": false, "history": true, "revision": "r1"})) }) })
            }
        }
        let target = FollowupWorkspaceTarget { host: "acme.jesadakorn.com".into(), holding_slug: "acme".into(), origin: "https://acme.jesadakorn.com".into(), backend: Arc::new(Restricted) };
        let state = Arc::new(AppState { targets: BTreeMap::from([((target.host.clone(), target.holding_slug.clone()), target)]) });
        let request = Request::builder().uri("/api/v1/holdings/acme/customer-followups").header("host", "acme.jesadakorn.com").body(erpx_platform_runtime_server::public::http_support::Body::empty()).unwrap();
        let response = read(State(state), Path("acme".into()), request).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["data"]["canCreate"], false);
        assert_eq!(value["data"]["purchase"]["revision"], "r1");
    }

    #[tokio::test]
    async fn unsafe_database_number_matches_typescript_internal_error() {
        let response = error_response(BusinessError::new(FailureKind::Unavailable, "DATABASE_NUMBER_OUT_OF_RANGE", "internal detail"));
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["error"]["code"], "INTERNAL_ERROR");
        assert_eq!(value["error"]["message"], "The request could not be completed.");
    }

    #[test]
    fn path_is_the_shared_handler_route() {
        assert_eq!(WORKSPACE_PATH, "/api/v1/holdings/{holding_slug}/customer-followups");
    }

    #[test]
    fn invalid_targets_fail_closed() {
        struct Backend;
        impl FollowupWorkspaceBackend for Backend {
            fn read<'a>(
                &'a self,
                _: &'a str, _: &'a str, _: &'a HeaderMap, _: &'a WorkspaceQuery,
            ) -> WorkspaceFuture<'a, AuthorizedFollowupWorkspace>
            {
                Box::pin(async { Err(BusinessError::new(FailureKind::Unavailable, "FOLLOWUP_AUTHORITY_REQUIRED", "Current authority is required.")) })
            }
        }
        let target = || FollowupWorkspaceTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.jesadakorn.com".into(),
            backend: Arc::new(Backend),
        };
        assert!(router(vec![target()]).is_ok());
        assert_eq!(
            router(vec![target(), target()]).unwrap_err().code,
            "CUSTOMER_FOLLOWUP_WORKSPACE_CONFIG_INVALID"
        );
        assert_eq!(status(FailureKind::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn query_ignores_unknown_and_preserves_first_empty_order_id() {
        let query = WorkspaceQuery::from_query(Some("other=1&orderId=&orderId=second"));
        assert_eq!(query.order_id.as_deref(), Some(""));
    }
}
