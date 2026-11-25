//! Owner-local axum ingress for the native Sales customer-note read.
//!
//! The router transports only bounded Host/Holding target and scope headers.
//! It never accepts SQL, a company from the body, or a caller-supplied
//! projection. The owner reader executes the query and builds the response;
//! any unavailable backend fails closed with an explicit 503.

use super::application::{CustomerNoteRead, read_notes};
use super::domain::{NoteQuery, NotesResponse};
use super::postgres::PgCustomerNoteRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_BRANCH: HeaderName = HeaderName::from_static("x-erpx-context-branch");
const CONTEXT_DEPARTMENT: HeaderName = HeaderName::from_static("x-erpx-context-department");
const NOTE_SOURCE: HeaderName = HeaderName::from_static("x-erpx-note-source");
const NOTE_ID: HeaderName = HeaderName::from_static("x-erpx-note-id");

/// Registered Holding target. The lead mounts the router with the exact
/// registered host/slug pairs; no other target resolves.
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn CustomerNoteRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "SALES_CUSTOMER_NOTES_REQUEST_INVALID",
        "Use one bounded customer-note read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "SALES_CUSTOMER_NOTES_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
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

/// Build the state map from per-target readers. Each `(host, holding_slug)` key
/// owns its own reader, so a Holding can never reach another Holding's pool.
fn build_with(
    targets: Vec<(LaneTarget, Arc<dyn CustomerNoteRead>)>,
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
            "/api/v1/holdings/{holding_slug}/sales/customer-notes",
            get(current),
        )
        .with_state(Arc::new(LaneState {
            targets: configured,
        })))
}

/// Reader-injection path: one distinct reader per registered Holding target.
fn router_with_readers(
    targets: Vec<(LaneTarget, Arc<dyn CustomerNoteRead>)>,
) -> Result<Router, BusinessError> {
    build_with(targets)
}

/// Router contract used by composition: the owner supplies its Pg pool and the
/// exact registered Holding targets. Every target gets its own reader over the
/// supplied pool. No composition or ledger file is edited by this lane.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_for_holdings(
        targets
            .into_iter()
            .map(|target| (target, pool.clone()))
            .collect(),
    )
}

/// Per-Holding router contract. Each registered Holding supplies its own
/// database pool; the owner builds one `PgCustomerNoteRead` per target and
/// inserts it into the state map keyed by `(host, holding_slug)`. Composition
/// loops `config.holdings` with `pools[i + 1].clone()`.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                let reader: Arc<dyn CustomerNoteRead> = Arc::new(PgCustomerNoteRead::new(pool));
                (target, reader)
            })
            .collect(),
    )
}

/// Same route with one injected reader shared across targets; retained for
/// owner tests and composition variants that already hold a typed owner reader.
pub fn router_with_reader(
    reader: Arc<dyn CustomerNoteRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

async fn current(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async move {
        let host = unique_header(&headers, header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(&headers, CONTEXT_COMPANY)?
            .filter(|value| !value.is_empty())
            .ok_or_else(invalid)?;
        let branch = unique_header(&headers, CONTEXT_BRANCH)?.filter(|value| !value.is_empty());
        let department =
            unique_header(&headers, CONTEXT_DEPARTMENT)?.filter(|value| !value.is_empty());
        let source = unique_header(&headers, NOTE_SOURCE)?
            .filter(|value| !value.is_empty())
            .ok_or_else(invalid)?;
        let id = unique_header(&headers, NOTE_ID)?
            .filter(|value| !value.is_empty())
            .ok_or_else(invalid)?;
        let query = NoteQuery {
            company: company.to_owned(),
            branch: branch.map(str::to_owned),
            department: department.map(str::to_owned),
            source: source.to_owned(),
            id: id.to_owned(),
        };
        let response: NotesResponse = read_notes(reader.as_ref(), &query).await?;
        Ok::<_, BusinessError>(response.into_value())
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
    use crate::adapters::native_lane_30::application::CustomerNoteReadFuture;
    use std::sync::Mutex;

    struct Probe {
        response: Result<NotesResponse, ()>,
        seen: Arc<Mutex<Vec<NoteQuery>>>,
    }

    impl CustomerNoteRead for Probe {
        fn read<'a>(&'a self, query: &'a NoteQuery) -> CustomerNoteReadFuture<'a, NotesResponse> {
            let seen = self.seen.clone();
            let response = self.response.clone();
            let query = query.clone();
            Box::pin(async move {
                seen.lock().unwrap().push(query);
                response.map_err(|_| {
                    BusinessError::new(
                        FailureKind::Unavailable,
                        "SALES_CUSTOMER_NOTES_UNAVAILABLE",
                        "The native Sales customer-note read is unavailable.",
                    )
                })
            })
        }
    }

    fn target() -> LaneTarget {
        LaneTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
        }
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, serde_json::Value) {
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
            serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
        )
    }

    async fn serve_router(router: Router) -> (u16, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        (addr.port(), server)
    }

    async fn serve(
        reader: Arc<dyn CustomerNoteRead>,
    ) -> (u16, tokio::task::JoinHandle<()>) {
        serve_router(router_with_reader(reader, vec![target()]).unwrap()).await
    }

    fn request(host: &str, extra: &str) -> String {
        format!(
            "GET /api/v1/holdings/acme/sales/customer-notes HTTP/1.1\r\nHost: {host}\r\n{extra}Connection: close\r\n\r\n"
        )
    }

    const SCOPE: &str = "x-erpx-context-company: company-1\r\nx-erpx-note-source: order\r\nx-erpx-note-id: order-1\r\n";

    #[test]
    fn configuration_requires_exact_distinct_holdings() {
        let probe = Arc::new(Probe {
            response: Ok(NotesResponse::default()),
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        assert!(router_with_reader(probe.clone(), vec![target()]).is_ok());
        assert!(router_with_reader(probe.clone(), vec![target(), target()]).is_err());
        assert!(router_with_reader(probe.clone(), vec![]).is_err());
        assert!(router_with_reader(
            probe,
            vec![LaneTarget {
                holding_slug: "BAD".into(),
                ..target()
            }]
        )
        .is_err());
    }

    #[tokio::test]
    async fn read_returns_exact_shape_and_passes_scope() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let probe = Arc::new(Probe {
            response: Ok(NotesResponse::default()),
            seen: seen.clone(),
        });
        let (port, server) = serve(probe).await;
        let response = http(
            format!("127.0.0.1:{port}").parse().unwrap(),
            request("acme.example.test", SCOPE),
        )
        .await;
        assert_eq!(response.0, 200);
        assert_eq!(response.1, serde_json::json!({"notes":[],"history":[]}));
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].company, "company-1");
        assert_eq!(seen[0].source, "order");
        assert_eq!(seen[0].id, "order-1");
        assert_eq!(seen[0].branch, None);
        assert_eq!(seen[0].department, None);
        server.abort();
    }

    #[tokio::test]
    async fn unknown_target_and_missing_scope_fail_closed() {
        let probe = Arc::new(Probe {
            response: Ok(NotesResponse::default()),
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        let (port, server) = serve(probe).await;
        let unknown = http(
            format!("127.0.0.1:{port}").parse().unwrap(),
            request("other.example.test", SCOPE),
        )
        .await;
        assert_eq!(unknown.0, 404);
        let no_company = http(
            format!("127.0.0.1:{port}").parse().unwrap(),
            request(
                "acme.example.test",
                "x-erpx-note-source: order\r\nx-erpx-note-id: order-1\r\n",
            ),
        )
        .await;
        assert_eq!(no_company.0, 422);
        assert_eq!(no_company.1["error"]["code"], "SALES_CUSTOMER_NOTES_REQUEST_INVALID");
        server.abort();
    }

    /// Proves the per-target state map routes each Holding to its own reader
    /// (and therefore its own pool in `router_for_holdings`). Two injected
    /// readers record disjoint company scopes; neither observes the other's call.
    #[tokio::test]
    async fn two_holdings_route_to_distinct_readers() {
        let first_seen = Arc::new(Mutex::new(Vec::new()));
        let second_seen = Arc::new(Mutex::new(Vec::new()));
        let first: Arc<dyn CustomerNoteRead> = Arc::new(Probe {
            response: Ok(NotesResponse::default()),
            seen: first_seen.clone(),
        });
        let second: Arc<dyn CustomerNoteRead> = Arc::new(Probe {
            response: Ok(NotesResponse::default()),
            seen: second_seen.clone(),
        });
        let router = router_with_readers(vec![
            (target(), first),
            (
                LaneTarget {
                    host: "other.example.test".into(),
                    holding_slug: "other".into(),
                },
                second,
            ),
        ])
        .unwrap();
        let (port, server) = serve_router(router).await;
        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

        let acme = http(
            addr,
            "GET /api/v1/holdings/acme/sales/customer-notes HTTP/1.1\r\nHost: acme.example.test\r\nx-erpx-context-company: company-a\r\nx-erpx-note-source: order\r\nx-erpx-note-id: order-1\r\nConnection: close\r\n\r\n"
                .into(),
        )
        .await;
        let other = http(
            addr,
            "GET /api/v1/holdings/other/sales/customer-notes HTTP/1.1\r\nHost: other.example.test\r\nx-erpx-context-company: company-b\r\nx-erpx-note-source: order\r\nx-erpx-note-id: order-1\r\nConnection: close\r\n\r\n"
                .into(),
        )
        .await;
        assert_eq!(acme.0, 200);
        assert_eq!(other.0, 200);

        let first_seen = first_seen.lock().unwrap();
        let second_seen = second_seen.lock().unwrap();
        assert_eq!(first_seen.len(), 1);
        assert_eq!(first_seen[0].company, "company-a");
        assert_eq!(second_seen.len(), 1);
        assert_eq!(second_seen[0].company, "company-b");
        assert!(first_seen.iter().all(|query| query.company != "company-b"));
        assert!(second_seen.iter().all(|query| query.company != "company-a"));
        server.abort();
    }

    #[tokio::test]
    async fn unavailable_reader_fails_closed_with_503() {
        let probe = Arc::new(Probe {
            response: Err(()),
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        let (port, server) = serve(probe).await;
        let response = http(
            format!("127.0.0.1:{port}").parse().unwrap(),
            request("acme.example.test", SCOPE),
        )
        .await;
        assert_eq!(response.0, 503);
        assert_eq!(response.1["error"]["code"], "SALES_CUSTOMER_NOTES_UNAVAILABLE");
        server.abort();
    }
}
