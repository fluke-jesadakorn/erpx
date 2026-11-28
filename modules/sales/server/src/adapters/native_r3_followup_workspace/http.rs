//! Owner-local Axum ingress for the native customer follow-up workspace read.
//!
//! Transport facts only: exact Host/Holding targets, workspace/company/user
//! headers supplied by the caller's authority and a bounded `orderId` query.
//! The owner store owns every statement and the exact `{data:...}` envelope.
use crate::adapters::native_r3_followup_workspace::application::{
    FollowupWorkspaceStore, WorkspaceInput, read_workspace,
};
use crate::adapters::native_r3_followup_workspace::domain::{
    WORKSPACE_PATH, WorkspaceQuery, request_invalid,
};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Query, Request, Response, Router,
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

const QUERY_LIMIT: usize = 4096;

pub struct FollowupWorkspaceTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub store: Arc<dyn FollowupWorkspaceStore>,
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
    let bytes = parts.uri.query().unwrap_or("").as_bytes().to_vec();
    let result = async move {
        if bytes.len() > QUERY_LIMIT
            || bytes.iter().enumerate().any(|(index, byte)| {
                *byte == b'%'
                    && !bytes
                        .get(index + 1..index + 3)
                        .is_some_and(|hex| hex.iter().all(u8::is_ascii_hexdigit))
            })
        {
            return Err(request_invalid());
        }
        let Query(query) = Query::<WorkspaceQuery>::try_from_uri(&parts.uri)
            .map_err(|_| request_invalid())?;
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
        let company = unique_header(&headers, WORKSPACE_CONTEXT_COMPANY)?
            .filter(|value| bounded_id(value))
            .ok_or_else(request_invalid)?;
        let user = unique_header(&headers, WORKSPACE_CONTEXT_USER)?
            .filter(|value| bounded_id(value))
            .ok_or_else(request_invalid)?;
        let branch = unique_header(&headers, WORKSPACE_BRANCH)?.filter(|value| bounded_id(value));
        let department =
            unique_header(&headers, WORKSPACE_DEPARTMENT)?.filter(|value| bounded_id(value));
        let permissions = unique_header(&headers, WORKSPACE_CONTEXT_PERMISSIONS)?
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .collect::<Vec<_>>();
        let input = WorkspaceInput {
            company_id: company,
            branch_id: branch,
            department_id: department,
            order_id: query.order_id.as_deref(),
            user_id: user,
            can_read: permissions.contains(&"commerce.read"),
            can_sell: permissions.contains(&"commerce.sales"),
            can_manage: permissions.contains(&"commerce.approve"),
        };
        let data = read_workspace(target.store.as_ref(), &query, input).await?;
        Ok(json!({ "data": data }))
    }
    .await;
    result.map_or_else(error_response, bounded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_is_the_shared_handler_route() {
        assert_eq!(WORKSPACE_PATH, "/api/v1/holdings/{holding_slug}/customer-followups");
    }

    #[test]
    fn invalid_targets_fail_closed() {
        struct Store;
        impl FollowupWorkspaceStore for Store {
            fn read<'a>(
                &'a self,
                _: WorkspaceInput<'a>,
            ) -> crate::adapters::native_r3_followup_workspace::application::WorkspaceFuture<'a, crate::adapters::native_r3_followup_workspace::application::WorkspaceRows>
            {
                Box::pin(async { Ok(Default::default()) })
            }
        }
        let target = || FollowupWorkspaceTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.jesadakorn.com".into(),
            store: Arc::new(Store),
        };
        assert!(router(vec![target()]).is_ok());
        assert_eq!(
            router(vec![target(), target()]).unwrap_err().code,
            "CUSTOMER_FOLLOWUP_WORKSPACE_CONFIG_INVALID"
        );
        assert_eq!(status(FailureKind::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn query_rejects_unknown_keys_and_bad_order_ids() {
        let bad = Query::<WorkspaceQuery>::try_from_uri(
            &"/api/v1/holdings/acme/customer-followups?orderId=../x".parse().unwrap(),
        )
        .expect("parses");
        assert!(bad.0.validate().is_err());
        let unknown = Query::<WorkspaceQuery>::try_from_uri(
            &"/api/v1/holdings/acme/customer-followups?other=1".parse().unwrap(),
        );
        assert!(unknown.is_err());
    }
}
