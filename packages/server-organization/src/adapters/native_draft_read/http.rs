//! Owner-local axum ingress for the native Organization draft read.
//!
//! Transport facts only: an exact registered Host/Holding pair, bounded verified
//! actor headers (Company, user, permissions and Holding scope) and an optional
//! bounded draft id. The pool, SQL and projection stay inside the owner crate;
//! a missing or malformed native runtime fails closed instead of reaching a
//! TypeScript authority.

use crate::adapters::native_draft_read::application::{DraftRead, handle};
use crate::adapters::native_draft_read::domain::{DraftReadQuery, valid_id};
use crate::adapters::native_draft_read::postgres::PgDraftRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Query, Request, Response, Router,
    State, StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");
const CONTEXT_PERMISSIONS: HeaderName = HeaderName::from_static("x-erpx-context-permissions");
const CONTEXT_SCOPE: HeaderName = HeaderName::from_static("x-erpx-context-scope");
const CONTEXT_BRANCH: HeaderName = HeaderName::from_static("x-erpx-context-branch");
const CONTEXT_DEPARTMENT: HeaderName = HeaderName::from_static("x-erpx-context-department");

/// One exact registered browser origin/Holding pair. Loose patterns and
/// duplicates are rejected so one Holding cannot read another Holding's drafts.
#[derive(Clone, Debug)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DraftQuery {
    id: Option<String>,
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn DraftRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "ORGANIZATION_DRAFT_READ_INVALID",
        "Use one bounded Organization draft read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "ORGANIZATION_DRAFT_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn response_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ORGANIZATION_DRAFT_READ_RESPONSE_INVALID",
        "The native Organization draft read could not be encoded.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_'))
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

fn actor(headers: &HeaderMap, draft_id: Option<String>) -> Result<DraftReadQuery, BusinessError> {
    let company = unique_header(headers, CONTEXT_COMPANY)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)?
        .to_owned();
    let user = unique_header(headers, CONTEXT_USER)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)?
        .to_owned();
    let permissions = match unique_header(headers, CONTEXT_PERMISSIONS)? {
        Some(value) => value
            .split(',')
            .map(|permission| permission.trim().to_owned())
            .collect(),
        None => Vec::new(),
    };
    let scope = unique_header(headers, CONTEXT_SCOPE)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)?
        .to_owned();
    let branch = unique_header(headers, CONTEXT_BRANCH)?.map(str::to_owned);
    let department = unique_header(headers, CONTEXT_DEPARTMENT)?.map(str::to_owned);
    Ok(DraftReadQuery {
        company,
        user,
        permissions,
        scope,
        branch,
        department,
        draft_id,
    })
}

fn router_with_readers(
    targets: Vec<(LaneTarget, Arc<dyn DraftRead>)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), reader)
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
            "/api/v1/holdings/{holding_slug}/people/organization/drafts",
            get(read),
        )
        .with_state(Arc::new(LaneState {
            targets: configured,
        })))
}

/// Build the native draft-read router. The lead mounts the returned router;
/// composition is not edited here. One owner reader is shared across all exact
/// distinct registered targets.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let reader: Arc<dyn DraftRead> = Arc::new(PgDraftRead::new(pool));
    router_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

/// Build the router with one owner reader and database pool per Holding. Each
/// `(LaneTarget, DatabasePool)` pair is keyed by `(host, holding_slug)` so a
/// request can only reach the pool registered for its Holding.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                let reader: Arc<dyn DraftRead> = Arc::new(PgDraftRead::new(pool));
                (target, reader)
            })
            .collect(),
    )
}

async fn read(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    Query(query): Query<DraftQuery>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let host = unique_header(&headers, header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let query = actor(&headers, query.id)?;
        let envelope = handle(reader.as_ref(), &query).await?;
        serde_json::to_value(envelope).map_err(|_| response_invalid())
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_draft_read::application::ReadFuture;
    use crate::adapters::native_draft_read::domain::{
        DraftReadResult, DraftRow, RecentRow, RequestRow,
    };

    #[derive(Clone)]
    struct Stub {
        result: Result<DraftReadResult, BusinessError>,
    }

    impl DraftRead for Stub {
        fn read<'a>(&'a self, _: &'a DraftReadQuery) -> ReadFuture<'a, DraftReadResult> {
            let result = self.result.clone();
            Box::pin(async move { result })
        }
    }

    fn facts(id: &str) -> DraftReadResult {
        DraftReadResult {
            draft: Some(DraftRow {
                id: id.into(),
                company_id: "acme-co".into(),
                owner_id: "user-1".into(),
                state: "draft".into(),
                version: 1,
                definition: serde_json::json!({"items": [], "reason": "Move"}),
                created_at: 1,
                updated_at: 2,
                base_snapshot: serde_json::json!({}),
            }),
            requests: vec![RequestRow {
                id: "request-1".into(),
                item_keys: serde_json::json!(["person:eng-1"]),
                state: "scheduled".into(),
                last_error: None,
                change_id: None,
            }],
            recent: vec![RecentRow {
                id: "recent-1".into(),
                updated_at: 1,
                count: 2,
            }],
        }
    }

    fn target() -> LaneTarget {
        LaneTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
        }
    }

    fn router(reader: Arc<dyn DraftRead>) -> Router {
        router_with_readers(vec![(target(), reader)]).unwrap()
    }

    #[test]
    fn target_configuration_requires_distinct_exact_holdings() {
        let reader: Arc<dyn DraftRead> = Arc::new(Stub { result: Ok(facts("d1")) });
        assert!(router_with_readers(vec![(target(), reader.clone())]).is_ok());
        assert!(router_with_readers(vec![(target(), reader.clone()), (target(), reader.clone())]).is_err());
        assert!(router_with_readers(vec![]).is_err());
        assert!(
            router_with_readers(vec![(
                LaneTarget {
                    holding_slug: "Acme".into(),
                    ..target()
                },
                reader,
            )])
            .is_err()
        );
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, String, serde_json::Value) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(raw.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let status = text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok())
            .unwrap();
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
        (
            status,
            text.to_string(),
            serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
        )
    }

    fn get(host: &str, slug: &str, query: &str) -> String {
        format!(
            "GET /api/v1/holdings/{slug}/people/organization/drafts{query} HTTP/1.1\r\nHost: {host}\r\nx-erpx-context-company: acme-co\r\nx-erpx-context-user: user-1\r\nx-erpx-context-permissions: people.manage,organization.manage\r\nx-erpx-context-scope: holding\r\nConnection: close\r\n\r\n"
        )
    }

    #[tokio::test]
    async fn actual_router_returns_complete_payload_and_bounded_headers() {
        let reader: Arc<dyn DraftRead> = Arc::new(Stub { result: Ok(facts("draft-1")) });
        let router = router(reader);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });

        let ok = http(addr, get("acme.example.test", "acme", "")).await;
        assert_eq!(ok.0, 200);
        assert_eq!(ok.2["data"]["draft"]["id"], "draft-1");
        assert_eq!(ok.2["data"]["requests"][0]["item_keys"][0], "person:eng-1");
        assert_eq!(ok.2["data"]["recent"][0]["count"], 2);
        assert!(ok.1.to_ascii_lowercase().contains("cache-control: private, no-store"));

        let by_id = http(addr, get("acme.example.test", "acme", "?id=draft-2")).await;
        assert_eq!(by_id.0, 200);

        let crossed = http(addr, get("acme.example.test", "northstar", "")).await;
        assert_eq!(crossed.0, 404);
        assert_eq!(crossed.2["error"]["code"], "NOT_FOUND");
        let _ = server.abort();
    }

    #[tokio::test]
    async fn unauthorized_and_malformed_requests_fail_closed() {
        let reader: Arc<dyn DraftRead> = Arc::new(Stub { result: Ok(facts("draft-1")) });
        let router = router(reader);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });

        let missing_permission = format!(
            "GET /api/v1/holdings/acme/people/organization/drafts HTTP/1.1\r\nHost: acme.example.test\r\nx-erpx-context-company: acme-co\r\nx-erpx-context-user: user-1\r\nx-erpx-context-permissions: people.manage\r\nx-erpx-context-scope: holding\r\nConnection: close\r\n\r\n"
        );
        let denied = http(addr, missing_permission).await;
        assert_eq!(denied.0, 403);
        assert_eq!(denied.2["error"]["code"], "COMPANY_DRAFT_AUTHORITY_REQUIRED");

        let bad_id = http(addr, get("acme.example.test", "acme", "?id=bad%20id")).await;
        assert_eq!(bad_id.0, 422);
        let _ = server.abort();
    }

    #[tokio::test]
    async fn reader_failure_fails_closed_with_503() {
        let reader: Arc<dyn DraftRead> = Arc::new(Stub {
            result: Err(BusinessError::new(
                FailureKind::Unavailable,
                "DRAFT_READ_DOWN",
                "The native draft reader is unavailable.",
            )),
        });
        let router = router(reader);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        let failed = http(addr, get("acme.example.test", "acme", "")).await;
        assert_eq!(failed.0, 503);
        assert_eq!(failed.2["error"]["code"], "DRAFT_READ_DOWN");
        let _ = server.abort();
    }
}
