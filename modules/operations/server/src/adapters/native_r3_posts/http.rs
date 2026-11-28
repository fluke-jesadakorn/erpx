//! Owner-local axum ingress for the three native L06 post sub-resources.
//!
//! Composition mounts this router on the private service. A request is routed
//! only to the Postgres pool registered for its exact `(Host, holding_slug)`
//! pair, and the injected work context must agree with that target's Company.
//! The retained Next transports and app re-exports are untouched by this module.

use crate::adapters::native_r3_posts::postgres::PgPosts;
use crate::application::native_r3_posts::{
    AiDraftHandler, AiProvider, TaskCommandHandler, TaskCreator, TaskSourceHandler,
};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body, header, routing::{get, post},
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");
const CONTEXT_BRANCH: HeaderName = HeaderName::from_static("x-erpx-workspace-branch");
const CONTEXT_DEPARTMENT: HeaderName = HeaderName::from_static("x-erpx-workspace-department");
const IDEMPOTENCY_KEY: HeaderName = HeaderName::from_static("idempotency-key");

/// One exact registered Holding target. The Company is part of the target so a
/// caller cannot select another Company's post by changing a header.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostsTarget {
    pub host: String,
    pub holding_slug: String,
    pub company_id: String,
}

struct PostsState {
    company_id: String,
    sources: TaskSourceHandler,
    commands: TaskCommandHandler,
    drafts: AiDraftHandler,
}

struct PostsRouterState {
    targets: BTreeMap<(String, String), PostsState>,
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "OPERATIONS_POSTS_INVALID",
        "Use one bounded native post request.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "OPERATIONS_POSTS_CONFIG_INVALID",
        "Use exact distinct registered Holding Company targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

fn required(headers: &HeaderMap, name: HeaderName) -> Result<String, BusinessError> {
    unique_header(headers, name)?
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(invalid)
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

/// `canonicalDestinationLinks` is identity on these payloads: no field is a
/// destination/href key, so the retained `{data}` envelope is exact.
fn bounded(body: Value) -> Response {
    let mut response = (StatusCode::OK, Json(body)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
}

async fn payload(request: Request) -> Result<Value, BusinessError> {
    let bytes = body::to_bytes(request.into_body(), 1_048_576)
        .await
        .map_err(|_| invalid())?;
    serde_json::from_slice(&bytes).map_err(|_| invalid())
}

fn resolve<'a>(
    state: &'a PostsRouterState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a PostsState, BusinessError> {
    let company = required(headers, CONTEXT_COMPANY)?;
    let user = required(headers, CONTEXT_USER)?;
    let host = unique_header(headers, header::HOST)?
        .filter(|host| !host.is_empty())
        .ok_or_else(not_found)?;
    let target = state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(not_found)?;
    if target.company_id != company {
        return Err(invalid());
    }
    let _ = user;
    Ok(target)
}

pub fn configured(
    targets: Vec<(PostsTarget, Arc<PgPosts>)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, owner) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || target.company_id.is_empty()
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    PostsState {
                        company_id: target.company_id,
                        sources: TaskSourceHandler::new(owner.clone()),
                        commands: TaskCommandHandler::new(owner.clone()),
                        drafts: AiDraftHandler::new(owner),
                    },
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/posts/{post_id}/task-source",
            get(task_source),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/posts/{post_id}/task",
            post(create_task),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/posts/{post_id}/ai-drafts",
            post(create_ai_draft),
        )
        .with_state(Arc::new(PostsRouterState {
            targets: configured,
        })))
}

async fn task_source(
    State(state): State<Arc<PostsRouterState>>,
    Path((slug, post)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let query = request.uri().query().unwrap_or_default().to_owned();
    let result = async {
        let target = resolve(&state, &headers, &slug)?;
        let user = required(&headers, CONTEXT_USER)?;
        let branch = unique_header(&headers, CONTEXT_BRANCH)?.map(str::to_owned);
        let department = unique_header(&headers, CONTEXT_DEPARTMENT)?.map(str::to_owned);
        let ids = query
            .split('&')
            .filter_map(|pair| pair.strip_prefix("commentId="))
            .map(|value| value.to_owned())
            .collect::<Vec<_>>();
        let body = target
            .sources
            .read(
                &target.company_id,
                &user,
                branch.as_deref(),
                department.as_deref(),
                &post,
                &ids,
                now_ms(),
            )
            .await?;
        Ok::<_, BusinessError>(bounded(body))
    }
    .await;
    result.unwrap_or_else(error_response)
}

async fn create_task(
    State(state): State<Arc<PostsRouterState>>,
    Path((slug, post)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let key = unique_header(&headers, IDEMPOTENCY_KEY)
        .and_then(|value| value.filter(|v| !v.is_empty()).map(str::to_owned).ok_or_else(invalid));
    let result = async {
        let target = resolve(&state, &headers, &slug)?;
        let key = key?;
        let input = payload(request).await?;
        let task = input.get("task").cloned().unwrap_or(Value::Null);
        let source = input.get("source").cloned().unwrap_or(Value::Null);
        let body = target.commands.create(&post, &key, &task, &source).await?;
        Ok::<_, BusinessError>(bounded(body))
    }
    .await;
    result.unwrap_or_else(error_response)
}

async fn create_ai_draft(
    State(state): State<Arc<PostsRouterState>>,
    Path((slug, post)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let key = unique_header(&headers, IDEMPOTENCY_KEY)
        .and_then(|value| value.filter(|v| !v.is_empty()).map(str::to_owned).ok_or_else(invalid));
    let result = async {
        let target = resolve(&state, &headers, &slug)?;
        let user = required(&headers, CONTEXT_USER)?;
        let key = key?;
        let input = payload(request).await?;
        let kind = input.get("kind").and_then(Value::as_str).unwrap_or_default().to_owned();
        let ids = input
            .get("commentIds")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let actor = json!({
            "assignmentId": unique_header(&headers, HeaderName::from_static("x-erpx-context-assignment"))?,
            "role": unique_header(&headers, HeaderName::from_static("x-erpx-context-role"))?,
            "authority": unique_header(&headers, HeaderName::from_static("x-erpx-context-revision"))?,
        })
        .to_string();
        let snapshot = target
            .sources
            .read(
                &target.company_id,
                &user,
                None,
                None,
                &post,
                &ids,
                now_ms(),
            )
            .await?;
        let sources: Vec<crate::domain::native_r3_posts::SourceItem> =
            serde_json::from_value(snapshot["data"]["sources"].clone()).unwrap_or_default();
        let hash = snapshot["data"]["hash"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let version = snapshot["data"]["post"]["version"]
            .as_i64()
            .unwrap_or_default();
        let body = target
            .drafts
            .create(
                &target.company_id,
                &user,
                &post,
                &key,
                &kind,
                &ids,
                &actor,
                now_ms(),
                &hash,
                version,
                &sources,
            )
            .await?;
        Ok::<_, BusinessError>(bounded(body))
    }
    .await;
    result.unwrap_or_else(error_response)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// One owner per Holding pool; a request can only reach the pool registered for
/// its exact target. `creator` and `provider` are composition-supplied ports.
pub fn router_for_holdings(
    targets: Vec<(PostsTarget, DatabasePool, Arc<dyn TaskCreator>, Arc<dyn AiProvider>)>,
) -> Result<Router, BusinessError> {
    let owners = targets
        .into_iter()
        .map(|(target, pool, creator, provider)| {
            (target, Arc::new(PgPosts::new(pool, creator, provider)))
        })
        .collect();
    configured(owners)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_must_be_exact_and_distinct() {
        let target = PostsTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            company_id: "acme:001".into(),
        };
        assert!(configured(vec![]).is_err());
        assert!(valid_slug(&target.holding_slug));
        assert!(!valid_slug("Acme"));
    }

    #[test]
    fn errors_keep_the_retained_status_codes() {
        assert_eq!(status(FailureKind::InvalidInput), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(status(FailureKind::Conflict), StatusCode::CONFLICT);
        assert_eq!(status(FailureKind::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn success_envelope_is_private_no_store() {
        let response = bounded(json!({"data": {"id": "t1"}}));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
    }

    #[test]
    fn context_headers_are_single_and_required() {
        let mut headers = HeaderMap::new();
        assert!(required(&headers, CONTEXT_COMPANY).is_err());
        headers.insert(CONTEXT_COMPANY, HeaderValue::from_static("acme:001"));
        assert_eq!(required(&headers, CONTEXT_COMPANY).unwrap(), "acme:001");
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("acme:001"));
        assert!(required(&headers, CONTEXT_COMPANY).is_err());
    }
}
