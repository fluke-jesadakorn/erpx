//! Owner-local axum ingress for the native R3 leave-requests routes.
//!
//! Transport facts only: exact registered Holding host/origin, bounded JSON
//! body, strict idempotency-key rules, the retired `{data: ...}` envelope,
//! page cursors and `Cache-Control`. The paged list SQL is executed by the
//! owner Postgres adapter; session authority, other-owner facts and the
//! retained native leave commands are supplied by composition through
//! `LeaveRequestsBackend`. No TypeScript SQL or business fallback remains in
//! this slice.

use super::application::{
    LeaveActor, LeaveFuture, LeaveRequestsBackend, LeaveRequestsRead,
};
use super::domain::{
    LeaveCommandKind, LeaveCursor, LeaveListEnvelope, LeaveListFilter, LeaveListRow, MAX_BODY,
    MAX_KEY, configuration_invalid, invalid, not_found, origin_forbidden, page_query, valid_id,
};
use super::postgres::PgLeaveRequestsRead;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

const ORIGIN_HEADER: HeaderName = HeaderName::from_static("origin");
const KEY_HEADER: HeaderName = HeaderName::from_static("idempotency-key");

/// One exact registered browser origin/Holding pair, as the retired handler
/// derived `requestPublicOrigin(request)` from the configured public origin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaveRequestsTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
}

struct AppState {
    targets: BTreeMap<(String, String), (String, Arc<dyn LeaveRequestsRead>)>,
    backend: Arc<dyn LeaveRequestsBackend>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}

fn valid_origin(origin: &str) -> bool {
    origin.starts_with("https://")
        && origin.len() > 8
        && !origin.ends_with('/')
        && !origin[8..].contains('/')
        && !origin.contains('@')
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid("VALIDATION_FAILED", "Duplicate header."));
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid("VALIDATION_FAILED", "Invalid header.")))
        .transpose()
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

fn data(code: StatusCode, value: Value) -> Response {
    (code, Json(serde_json::json!({"data": value}))).into_response()
}

fn cached(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response
}

fn configure(
    targets: Vec<(LeaveRequestsTarget, Arc<dyn LeaveRequestsRead>)>,
) -> Result<BTreeMap<(String, String), (String, Arc<dyn LeaveRequestsRead>)>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in targets {
        if target.host.is_empty()
            || !valid_origin(&target.origin)
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    (target.origin.clone(), reader),
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(configured)
}

fn finish(
    targets: Vec<(LeaveRequestsTarget, Arc<dyn LeaveRequestsRead>)>,
    backend: Arc<dyn LeaveRequestsBackend>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/leave-requests",
            routing::get(list).post(create),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/leave-requests/preview",
            routing::post(preview),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/leave-requests/{id}",
            routing::get(detail).patch(edit),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/leave-requests/{id}/transition",
            routing::post(transition),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/leave-requests/{id}/cancellation-review",
            routing::post(cancellation_review),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/leave-requests/{id}/approve",
            routing::post(approve),
        )
        .with_state(Arc::new(AppState {
            targets: configure(targets)?,
            backend,
        })))
}

/// Native router with one owner list reader per registered Holding pool and a
/// composition-owned backend for authority and the retained leave commands.
/// The lead mounts this on the private native People Payroll axum service.
pub fn router(
    targets: Vec<(LeaveRequestsTarget, DatabasePool)>,
    backend: Arc<dyn LeaveRequestsBackend>,
) -> Result<Router, BusinessError> {
    finish(
        targets
            .into_iter()
            .map(|(target, pool)| (target, Arc::new(PgLeaveRequestsRead::new(pool)) as Arc<dyn LeaveRequestsRead>))
            .collect(),
        backend,
    )
}

/// Reader-injection path used by owner-local contract tests.
pub fn router_with_reader(
    targets: Vec<(LeaveRequestsTarget, Arc<dyn LeaveRequestsRead>)>,
    backend: Arc<dyn LeaveRequestsBackend>,
) -> Result<Router, BusinessError> {
    finish(targets, backend)
}

fn target_for(
    state: &AppState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<(String, String, Arc<dyn LeaveRequestsRead>), BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|host| !host.is_empty())
        .ok_or_else(missing)?;
    let (origin, reader) = state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .cloned()
        .ok_or_else(missing)?;
    Ok((host.to_owned(), origin, reader))
}

fn check_origin(headers: &HeaderMap, expected: &str) -> Result<(), BusinessError> {
    match unique_header(headers, ORIGIN_HEADER)? {
        Some(origin) if origin != expected => Err(origin_forbidden()),
        _ => Ok(()),
    }
}

fn command_key(
    headers: &HeaderMap,
    kind: LeaveCommandKind,
) -> Result<Option<String>, BusinessError> {
    let Some(code) = kind.key_code() else { return Ok(None) };
    let key = unique_header(headers, KEY_HEADER)?
        .filter(|key| !key.is_empty() && key.len() <= MAX_KEY)
        .ok_or_else(|| invalid(code, "Idempotency-Key is required."))?;
    Ok(Some(key.to_owned()))
}

async fn body_value(request: Request) -> Result<Value, BusinessError> {
    let bytes = to_bytes(request.into_body(), MAX_BODY)
        .await
        .map_err(|_| invalid("VALIDATION_FAILED", "Invalid leave request body."))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| invalid("VALIDATION_FAILED", "Invalid leave request body."))
}

async fn list(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let query = request.uri().query().map(str::to_owned);
    let result = async {
        let (host, _origin, reader) = target_for(&state, &headers, &slug)?;
        let actor = state.backend.actor(&host, &slug, &headers).await?;
        match page_query(query.as_deref())? {
            Some((filter, limit, cursor)) => {
                let mut rows = reader
                    .list(&actor, filter, cursor.as_ref(), limit)
                    .await?;
                let has_more = rows.len() > limit;
                rows.truncate(limit);
                let next_cursor = if has_more {
                    rows.last().map(|row: &LeaveListRow| {
                        LeaveCursor {
                            at: row.created_at,
                            id: row.id.clone(),
                        }
                        .encode()
                    })
                } else {
                    None
                };
                Ok::<_, BusinessError>(serde_json::to_value(LeaveListEnvelope {
                    data: rows,
                    has_more,
                    next_cursor,
                })
                .map_err(|_| invalid("VALIDATION_FAILED", "Invalid leave list."))?)
            }
            None => {
                let history = state.backend.history(&actor).await?;
                Ok::<_, BusinessError>(serde_json::Value::Array(history))
            }
        }
    }
    .await;
    match result {
        Ok(value) => cached(data(StatusCode::OK, value)),
        Err(error) => error_response(error),
    }
}

async fn detail(
    State(state): State<Arc<AppState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        if !valid_id(&id) {
            return Err(invalid("VALIDATION_FAILED", "Invalid leave id."));
        }
        let (host, _origin, _) = target_for(&state, &headers, &slug)?;
        let actor = state.backend.actor(&host, &slug, &headers).await?;
        state
            .backend
            .detail(&actor, &id)
            .await?
            .ok_or_else(not_found)
    }
    .await;
    match result {
        Ok(value) => cached(data(StatusCode::OK, value)),
        Err(error) => error_response(error),
    }
}

async fn run_command(
    state: &Arc<AppState>,
    slug: &str,
    headers: &HeaderMap,
    kind: LeaveCommandKind,
    id: Option<&str>,
    request: Request,
    code: StatusCode,
) -> Response {
    let result = async {
        let (host, origin, _) = target_for(state, headers, slug)?;
        check_origin(headers, &origin)?;
        let key = command_key(headers, kind)?;
        if let Some(id) = id {
            if !valid_id(id) {
                return Err(invalid("VALIDATION_FAILED", "Invalid leave id."));
            }
        }
        let body = body_value(request).await?;
        if kind == LeaveCommandKind::CancellationReview
            && !matches!(body.get("decision").and_then(Value::as_str), Some("approve" | "reject"))
        {
            return Err(invalid("VALIDATION_FAILED", "Decision must be approve or reject."));
        }
        let actor = state.backend.actor(&host, slug, headers).await?;
        state.backend.command(&actor, kind, id, &body, key.as_deref()).await
    }
    .await;
    match result {
        Ok(value) => data(code, value),
        Err(error) => error_response(error),
    }
}

async fn create(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    run_command(&state, &slug, &headers, LeaveCommandKind::Create, None, request, StatusCode::CREATED).await
}

async fn preview(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    run_command(&state, &slug, &headers, LeaveCommandKind::Preview, None, request, StatusCode::OK).await
}

async fn edit(
    State(state): State<Arc<AppState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    run_command(&state, &slug, &headers, LeaveCommandKind::Edit, Some(&id), request, StatusCode::OK).await
}

async fn transition(
    State(state): State<Arc<AppState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    run_command(&state, &slug, &headers, LeaveCommandKind::Transition, Some(&id), request, StatusCode::OK).await
}

async fn cancellation_review(
    State(state): State<Arc<AppState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    run_command(&state, &slug, &headers, LeaveCommandKind::CancellationReview, Some(&id), request, StatusCode::OK).await
}

async fn approve(
    State(state): State<Arc<AppState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    run_command(&state, &slug, &headers, LeaveCommandKind::Approve, Some(&id), request, StatusCode::OK).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::domain::{LeaveListRow, LeaveListFilter};

    struct Reader {
        rows: Vec<LeaveListRow>,
    }

    impl LeaveRequestsRead for Reader {
        fn list<'a>(
            &'a self,
            _actor: &'a LeaveActor,
            _filter: LeaveListFilter,
            _cursor: Option<&'a LeaveCursor>,
            _limit: usize,
        ) -> LeaveFuture<'a, Vec<LeaveListRow>> {
            let rows = self.rows.clone();
            Box::pin(async move { Ok(rows) })
        }
    }

    struct Backend;

    impl LeaveRequestsBackend for Backend {
        fn actor<'a>(
            &'a self,
            _host: &'a str,
            _slug: &'a str,
            _headers: &'a HeaderMap,
        ) -> LeaveFuture<'a, LeaveActor> {
            Box::pin(async {
                Ok(LeaveActor {
                    company: "acme-co".into(),
                    user: "u-1".into(),
                    engagement: "e-1".into(),
                    branch: None,
                    department: None,
                    evidence_reader: false,
                })
            })
        }
        fn detail<'a>(&'a self, _actor: &'a LeaveActor, _id: &'a str) -> LeaveFuture<'a, Option<Value>> {
            Box::pin(async { Ok(Some(serde_json::json!({"id":"leave-1","permittedActions":["cancel"]}))) })
        }
        fn history<'a>(&'a self, _actor: &'a LeaveActor) -> LeaveFuture<'a, Vec<Value>> {
            Box::pin(async { Ok(vec![serde_json::json!({"id":"leave-1"})]) })
        }
        fn command<'a>(
            &'a self,
            _actor: &'a LeaveActor,
            _kind: LeaveCommandKind,
            _id: Option<&'a str>,
            body: &'a Value,
            _key: Option<&'a str>,
        ) -> LeaveFuture<'a, Value> {
            let body = body.clone();
            Box::pin(async move { Ok(body) })
        }
    }

    fn target() -> LeaveRequestsTarget {
        LeaveRequestsTarget {
            host: "acme.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.test".into(),
        }
    }

    fn reader() -> Arc<dyn LeaveRequestsRead> {
        Arc::new(Reader {
            rows: vec![LeaveListRow {
                id: "leave-1".into(),
                number: "L-1".into(),
                start_date: "2026-12-01".into(),
                end_date: None,
                start_segment: None,
                end_segment: None,
                calculated_units: None,
                days: 1,
                state: "draft".into(),
                cancellation_state: None,
                version: 1,
                created_at: 1_700_000_000_000,
                updated_at: 1_700_000_000_000,
                decision_note: None,
                leave_type_name: None,
            }],
        })
    }

    #[test]
    fn targets_must_be_exact_and_distinct() {
        assert!(router_with_reader(vec![(target(), reader())], Arc::new(Backend)).is_ok());
        assert!(router_with_reader(vec![], Arc::new(Backend)).is_err());
        assert!(router_with_reader(vec![(target(), reader()), (target(), reader())], Arc::new(Backend)).is_err());
    }

    #[test]
    fn envelope_matches_the_retired_envelopes() {
        let paged = serde_json::to_string(&LeaveListEnvelope {
            data: vec![],
            has_more: true,
            next_cursor: Some("cursor".into()),
        })
        .unwrap();
        assert_eq!(paged, r#"{"data":[],"hasMore":true,"nextCursor":"cursor"}"#);
        let detail = serde_json::to_string(&serde_json::json!({"data":{"id":"leave-1"}})).unwrap();
        assert_eq!(detail, r#"{"data":{"id":"leave-1"}}"#);
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, String, String) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(raw.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let status = text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok())
            .unwrap();
        let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
        (status, head.to_owned(), body.to_owned())
    }

    #[tokio::test]
    async fn router_enforces_transport_rules() {
        let router = router_with_reader(vec![(target(), reader())], Arc::new(Backend)).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        let paged = "GET /api/v1/holdings/acme/leave-requests?filter=drafts&limit=1 HTTP/1.1\r\nHost: acme.test\r\nConnection: close\r\n\r\n";
        let (code, head, body) = http(addr, paged.to_owned()).await;
        assert_eq!(code, 200);
        assert!(head.to_ascii_lowercase().contains("cache-control: private, no-store"));
        assert!(body.contains("\"hasMore\":false"));
        let bad = "GET /api/v1/holdings/acme/leave-requests?limit=51 HTTP/1.1\r\nHost: acme.test\r\nConnection: close\r\n\r\n";
        assert_eq!(http(addr, bad.to_owned()).await.0, 422);
        let create = "POST /api/v1/holdings/acme/leave-requests HTTP/1.1\r\nHost: acme.test\r\nConnection: close\r\nContent-Length: 2\r\nContent-Type: application/json\r\n\r\n{}";
        assert_eq!(http(addr, create.to_owned()).await.0, 422);
        let with_key = "POST /api/v1/holdings/acme/leave-requests HTTP/1.1\r\nHost: acme.test\r\nConnection: close\r\nIdempotency-Key: k1\r\nContent-Length: 2\r\nContent-Type: application/json\r\n\r\n{}";
        assert_eq!(http(addr, with_key.to_owned()).await.0, 201);
        let wrong_origin = "POST /api/v1/holdings/acme/leave-requests HTTP/1.1\r\nHost: acme.test\r\nConnection: close\r\nOrigin: https://evil.test\r\nIdempotency-Key: k1\r\nContent-Length: 2\r\nContent-Type: application/json\r\n\r\n{}";
        assert_eq!(http(addr, wrong_origin.to_owned()).await.0, 403);
        let other_host = "GET /api/v1/holdings/acme/leave-requests HTTP/1.1\r\nHost: other.test\r\nConnection: close\r\n\r\n";
        assert_eq!(http(addr, other_host.to_owned()).await.0, 404);
        server.abort();
    }
}
