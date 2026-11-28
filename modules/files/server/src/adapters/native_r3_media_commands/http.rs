//! Exact Holding axum routes for the three media-publication commands. The
//! router owns only transport: one bounded JSON body, the registered Host and
//! Origin, a bounded Idempotency-Key and bounded identity headers. The owner
//! application and SQLx store own every business decision and statement.

use std::{collections::BTreeMap, sync::Arc};

use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, CommandKey, FailureKind};
use serde_json::Value;

use super::application::{
    CommandContext, CommandTarget, PublicationCommandStore, approve as approve_command,
    reorder as reorder_command, revoke as revoke_command,
};
use super::domain::{parse_order, parse_reason};
use super::postgres::PgPublicationCommandStore;

const BODY_LIMIT: usize = 64 * 1024;
const KEY_HEADER: HeaderName = HeaderName::from_static("idempotency-key");
const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");
const CONTEXT_HOLDING: HeaderName = HeaderName::from_static("x-erpx-context-holding");
const CONTEXT_PERMISSIONS: HeaderName = HeaderName::from_static("x-erpx-context-permissions");

struct TargetState {
    origin: String,
    store: Arc<dyn PublicationCommandStore>,
}

struct AppState {
    targets: BTreeMap<(String, String), TargetState>,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
}

fn valid_origin(origin: &str) -> bool {
    let rest = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"));
    rest.is_some_and(|host| {
        !host.is_empty()
            && !host.bytes().any(|byte| {
                byte.is_ascii_whitespace()
                    || matches!(byte, b'/' | b'?' | b'#' | b'@' | b'\\')
            })
    })
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid(
            "MEDIA_PUBLICATIONS_INVALID",
            "Use one bounded request.",
        ));
    }
    first
        .map(|value| {
            value
                .to_str()
                .map_err(|_| invalid("MEDIA_PUBLICATIONS_INVALID", "Use one bounded request."))
        })
        .transpose()
}

fn identity(headers: &HeaderMap, name: HeaderName) -> Result<String, BusinessError> {
    let value = unique_header(headers, name)?
        .filter(|value| !value.is_empty() && value.len() <= 200)
        .ok_or_else(|| invalid("MEDIA_PUBLICATIONS_INVALID", "Use one bounded request."))?;
    Ok(value.to_owned())
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
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn payload(value: Value) -> Response {
    let mut response = (StatusCode::OK, Json(serde_json::json!({"data": value}))).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
}

fn target_for<'a>(
    state: &'a AppState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a TargetState, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|host| !host.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

async fn body(request: Request) -> Result<Value, BusinessError> {
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| invalid("VALIDATION_FAILED", "Request body must be valid JSON."))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| invalid("VALIDATION_FAILED", "Request body must be valid JSON."))
}

async fn command(
    request: Request,
    store: Arc<dyn PublicationCommandStore>,
    origin: &str,
    target_type: Option<&str>,
    target_id: Option<&str>,
    run: for<'a> fn(
        &'a dyn PublicationCommandStore,
        &'a CommandContext,
        &'a Value,
        Option<(&'a str, &'a str)>,
    ) -> super::application::CommandFuture<'a, Value>,
) -> Response {
    let result = async {
        let headers = request.headers().clone();
        if let Some(found) = unique_header(&headers, header::ORIGIN)? {
            if found != origin {
                return Err(invalid(
                    "ORIGIN_FORBIDDEN",
                    "Use the application origin.",
                ));
            }
        }
        let key = unique_header(&headers, KEY_HEADER)?
            .ok_or_else(|| {
                invalid(
                    "COMMAND_KEY_REQUIRED",
                    "A valid Idempotency-Key is required.",
                )
            })
            .and_then(CommandKey::new)?;
        let company = identity(&headers, CONTEXT_COMPANY)?;
        let user = identity(&headers, CONTEXT_USER)?;
        let holding_id = identity(&headers, CONTEXT_HOLDING)?;
        let permissions = unique_header(&headers, CONTEXT_PERMISSIONS)?
            .unwrap_or("")
            .split(',')
            .filter(|value| !value.is_empty())
            .map(|value| value.trim().to_owned())
            .collect::<Vec<_>>();
        let value = body(request).await?;
        let ctx = CommandContext {
            company,
            user,
            holding_id,
            permissions,
            key: key.as_str().to_owned(),
            now: now_millis(),
        };
        let resource = target_type.zip(target_id);
        run(store.as_ref(), &ctx, &value, resource).await
    }
    .await;
    match result {
        Ok(value) => payload(value),
        Err(error) => error_response(error),
    }
}

fn now_millis() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(0)
}

async fn order(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = (|| {
        let target = target_for(&state, request.headers(), &slug)?;
        Ok((target.store.clone(), target.origin.clone()))
    })();
    match result {
        Err(error) => error_response(error),
        Ok((store, origin)) => {
            command(
                request,
                store,
                &origin,
                None,
                None,
                |store, ctx, value, _| {
                    let input = parse_order(value);
                    Box::pin(async move {
                        reorder_command(store, ctx, &input?).await
                    })
                },
            )
            .await
        }
    }
}

async fn approve(
    State(state): State<Arc<AppState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = (|| {
        let target = target_for(&state, request.headers(), &slug)?;
        if !valid_resource_id(&id) {
            return Err(invalid("NOT_FOUND", "Not found."));
        }
        Ok((target.store.clone(), target.origin.clone()))
    })();
    match result {
        Err(error) => error_response(error),
        Ok((store, origin)) => {
            command(
                request,
                store,
                &origin,
                None,
                Some(&id),
                |store, ctx, _, resource| {
                    let id = resource
                        .and_then(|(_, id)| Some(id.to_owned()))
                        .unwrap_or_default();
                    Box::pin(async move { approve_command(store, ctx, &id).await })
                },
            )
            .await
        }
    }
}

async fn revoke(
    State(state): State<Arc<AppState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = (|| {
        let target = target_for(&state, request.headers(), &slug)?;
        if !valid_resource_id(&id) {
            return Err(invalid("NOT_FOUND", "Not found."));
        }
        Ok((target.store.clone(), target.origin.clone()))
    })();
    match result {
        Err(error) => error_response(error),
        Ok((store, origin)) => {
            command(
                request,
                store,
                &origin,
                None,
                Some(&id),
                |store, ctx, value, resource| {
                    let reason = parse_reason(value);
                    let id = resource
                        .and_then(|(_, id)| Some(id.to_owned()))
                        .unwrap_or_default();
                    Box::pin(async move { revoke_command(store, ctx, &id, &reason?).await })
                },
            )
            .await
        }
    }
}

fn valid_resource_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 180
}

fn configure(
    entries: impl IntoIterator<Item = (CommandTarget, Arc<dyn PublicationCommandStore>)>,
) -> Result<Arc<AppState>, BusinessError> {
    let mut targets = BTreeMap::new();
    for (target, store) in entries {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_origin(&target.origin)
            || targets
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    TargetState {
                        origin: target.origin,
                        store,
                    },
                )
                .is_some()
        {
            return Err(invalid(
                "MEDIA_PUBLICATIONS_CONFIG_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    if targets.is_empty() {
        return Err(invalid(
            "MEDIA_PUBLICATIONS_CONFIG_INVALID",
            "Use exact distinct registered Holding targets.",
        ));
    }
    Ok(Arc::new(AppState { targets }))
}

/// Router from pre-built owner ports. The lead mounts it once per Holding.
pub fn media_commands_router(
    entries: impl IntoIterator<Item = (CommandTarget, Arc<dyn PublicationCommandStore>)>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/media-publications/order",
            post(order),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/media-publications/{publication_id}/approve",
            post(approve),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/media-publications/{publication_id}/revoke",
            post(revoke),
        )
        .with_state(configure(entries)?))
}

/// Per-holding constructor: one Pg store per registered Holding pool.
pub fn media_commands_router_for_holdings(
    targets: Vec<(CommandTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    media_commands_router(targets.into_iter().map(|(target, pool)| {
        let store: Arc<dyn PublicationCommandStore> =
            Arc::new(PgPublicationCommandStore::new(pool));
        (target, store)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::adapters::native_r3_media_commands::{CommandFuture, OrderInput};

    struct Noop;
    impl PublicationCommandStore for Noop {
        fn reorder(&self, _: &CommandContext, _: &OrderInput) -> CommandFuture<'_, Value> {
            Box::pin(async { Ok(serde_json::json!({"orderVersion":2})) })
        }
        fn approve(&self, _: &CommandContext, _: &str) -> CommandFuture<'_, Value> {
            Box::pin(async { Ok(Value::Null) })
        }
        fn revoke(&self, _: &CommandContext, _: &str, _: &str) -> CommandFuture<'_, Value> {
            Box::pin(async { Ok(Value::Null) })
        }
    }

    #[test]
    fn configuration_rejects_duplicate_and_unregistered_targets() {
        let store = Arc::new(Noop) as Arc<dyn PublicationCommandStore>;
        let target = CommandTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
        };
        assert!(media_commands_router(vec![(target.clone(), store.clone())]).is_ok());
        assert!(media_commands_router(vec![
            (target.clone(), store.clone()),
            (target, store),
        ])
        .is_err());
        assert!(media_commands_router(Vec::new()).is_err());
    }

    #[test]
    fn validation_helpers_match_the_next_handlers() {
        assert!(valid_slug("acme-holding_1"));
        assert!(!valid_slug("Acme"));
        assert!(valid_origin("https://acme.example.test"));
        assert!(!valid_origin("https://acme.example.test/path"));
    }

    #[test]
    fn error_envelope_and_status_match_the_next_handlers() {
        let response = error_response(invalid("COMMAND_KEY_REQUIRED", "A valid Idempotency-Key is required."));
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let response = payload(serde_json::json!({"orderVersion":2}));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers().get(header::CACHE_CONTROL).unwrap(), "private, no-store");
    }
}
