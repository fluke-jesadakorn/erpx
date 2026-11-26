//! Owner-local axum router for the native PMS document entrypoint reads.
//!
//! Both routes are isolated by an exact Host + registered Holding slug and a
//! single Company identity header. The owner executes the SQL with sqlx and
//! returns the complete retired payload; a missing/malformed native reply fails
//! closed in the TypeScript transport instead of falling back to TS SQL.

use crate::application::native_documents::{
    DocumentFuture, DocumentQuery, DocumentRead, PostingQuery,
};
use crate::adapters::native_documents_postgres::PgDocumentRead;
use crate::domain::native_documents::{
    DocumentRow, RelatedPosting, invalid_configuration, invalid_request, unavailable, valid_id,
    valid_slug,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Query, Request, Response, Router,
    State, StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

/// Registered Holding target. Host and slug must both match one config entry.
pub struct DocumentTarget {
    pub host: String,
    pub holding_slug: String,
}

/// Deterministic reader used by unit tests. Production uses the Pg reader.
pub struct ProbeDocumentRead<L, P> {
    load: L,
    postings: P,
}

impl<L, P> ProbeDocumentRead<L, P>
where
    L: Fn(&str, &DocumentQuery) -> DocumentFuture<'static, Option<DocumentRow>> + Send + Sync,
    P: Fn(&str, &PostingQuery) -> DocumentFuture<'static, Option<Vec<RelatedPosting>>> + Send + Sync,
{
    pub fn new(load: L, postings: P) -> Self {
        Self { load, postings }
    }
}

impl<L, P> DocumentRead for ProbeDocumentRead<L, P>
where
    L: Fn(&str, &DocumentQuery) -> DocumentFuture<'static, Option<DocumentRow>> + Send + Sync,
    P: Fn(&str, &PostingQuery) -> DocumentFuture<'static, Option<Vec<RelatedPosting>>> + Send + Sync,
{
    fn load<'a>(
        &'a self,
        company: &'a str,
        query: &'a DocumentQuery,
    ) -> DocumentFuture<'a, Option<DocumentRow>> {
        (self.load)(company, query)
    }

    fn postings<'a>(
        &'a self,
        company: &'a str,
        query: &'a PostingQuery,
    ) -> DocumentFuture<'a, Option<Vec<RelatedPosting>>> {
        (self.postings)(company, query)
    }
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn DocumentRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
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

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid_request());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid_request()))
        .transpose()
}

fn reader_map(
    targets: Vec<(DocumentTarget, Arc<dyn DocumentRead>)>,
) -> Result<BTreeMap<(String, String), Arc<dyn DocumentRead>>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), reader)
                .is_some()
        {
            return Err(invalid_configuration());
        }
    }
    if configured.is_empty() {
        return Err(invalid_configuration());
    }
    Ok(configured)
}

fn route(state: Arc<LaneState>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/pms/native-documents/document",
            get(document),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/pms/native-documents/postings",
            get(postings),
        )
        .with_state(state)
}

/// Composition-facing constructor: one owner reader per registered Holding,
/// built from that Holding's own database pool.
pub fn router_for_holdings(
    targets: Vec<(DocumentTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let readers = targets
        .into_iter()
        .map(|(target, pool)| {
            (
                target,
                Arc::new(PgDocumentRead::new(pool)) as Arc<dyn DocumentRead>,
            )
        })
        .collect();
    Ok(route(Arc::new(LaneState {
        targets: reader_map(readers)?,
    })))
}

/// Test-facing constructor: inject one deterministic reader per target.
pub fn router_with_readers(
    targets: Vec<(DocumentTarget, Arc<dyn DocumentRead>)>,
) -> Result<Router, BusinessError> {
    Ok(route(Arc::new(LaneState {
        targets: reader_map(targets)?,
    })))
}

/// Test-facing constructor: one injected reader shared by all targets.
pub fn router_with_reader(
    reader: Arc<dyn DocumentRead>,
    targets: Vec<DocumentTarget>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

/// Keep the fail-closed code reachable from the owner's public surface.
pub fn unavailable_error() -> BusinessError {
    unavailable()
}

#[derive(Debug, Deserialize)]
pub struct DocumentParams {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct PostingParams {
    pub property: String,
    pub id: String,
}

async fn document(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    Query(params): Query<DocumentParams>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid_request)?;
        if !valid_id(&params.id) {
            return Err(invalid_request());
        }
        let row = reader
            .load(company, &DocumentQuery { id: params.id })
            .await?;
        let row = row.map(|row| {
            serde_json::json!({
                "id": row.id,
                "property_id": row.property_id,
                "guest_account_id": row.guest_account_id,
                "kind": row.kind,
                "finance_id": row.finance_id,
                "snapshot_json": row.snapshot_json,
                "created_at": row.created_at,
            })
        });
        Ok::<_, BusinessError>(serde_json::json!({"document": row}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn postings(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    Query(params): Query<PostingParams>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid_request)?;
        if !valid_id(&params.property) || !valid_id(&params.id) {
            return Err(invalid_request());
        }
        let rows = reader
            .postings(
                company,
                &PostingQuery {
                    property: params.property,
                    document: params.id,
                },
            )
            .await?;
        let rows = rows.map(|rows| {
            rows.into_iter()
                .map(|row| serde_json::json!({"journal_id": row.journal_id}))
                .collect::<Vec<_>>()
        });
        Ok::<_, BusinessError>(serde_json::json!({"postings": rows}))
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
    use erpx_server_runtime::public::http_support::serve;
    use std::sync::Mutex;

    const HOST: &str = "acme.example.test";

    fn row() -> DocumentRow {
        DocumentRow {
            id: "doc-1".to_owned(),
            property_id: "property-1".to_owned(),
            guest_account_id: "guest-1".to_owned(),
            kind: "receipt".to_owned(),
            finance_id: Some("finance-1".to_owned()),
            snapshot_json: "{\"amount\":100}".to_owned(),
            created_at: 99,
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
        (status, serde_json::from_str(body).unwrap_or(serde_json::Value::Null))
    }

    async fn serve_probe(
        seen: Arc<Mutex<Vec<(String, String)>>>,
        answer: Option<DocumentRow>,
        postings: Option<Vec<RelatedPosting>>,
    ) -> (u16, tokio::task::JoinHandle<()>) {
        let seen_load = seen.clone();
        let seen_postings = seen.clone();
        let probe = Arc::new(ProbeDocumentRead::new(
            move |company, query| {
                let company = company.to_owned();
                let id = query.id.clone();
                let seen = seen_load.clone();
                let answer = answer.clone();
                Box::pin(async move {
                    seen.lock().unwrap().push((company, id));
                    Ok::<Option<DocumentRow>, BusinessError>(answer)
                })
            },
            move |company, query| {
                let company = company.to_owned();
                let key = format!("{}:{}", query.property, query.document);
                let seen = seen_postings.clone();
                let postings = postings.clone();
                Box::pin(async move {
                    seen.lock().unwrap().push((company, key));
                    Ok::<Option<Vec<RelatedPosting>>, BusinessError>(postings)
                })
            },
        ));
        let router = router_with_reader(
            probe,
            vec![DocumentTarget {
                host: HOST.to_owned(),
                holding_slug: "acme".to_owned(),
            }],
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = serve(listener, router).await;
        });
        (addr.port(), server)
    }

    fn document_request(host: &str, company: &str, id: &str) -> String {
        format!(
            "GET /api/v1/holdings/acme/pms/native-documents/document?id={id} HTTP/1.1\r\n\
             Host: {host}\r\n\
             Accept: application/json\r\n\
             Connection: close\r\n\
             x-erpx-context-company: {company}\r\n\r\n"
        )
    }

    fn postings_request(host: &str, company: &str, property: &str, id: &str) -> String {
        format!(
            "GET /api/v1/holdings/acme/pms/native-documents/postings?property={property}&id={id} HTTP/1.1\r\n\
             Host: {host}\r\n\
             Accept: application/json\r\n\
             Connection: close\r\n\
             x-erpx-context-company: {company}\r\n\r\n"
        )
    }

    #[tokio::test]
    async fn document_payload_parity_and_company_scope() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (port, server) = serve_probe(seen.clone(), Some(row()), None).await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            document_request(HOST, "company", "doc-1"),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        assert_eq!(body["document"]["id"], "doc-1");
        assert_eq!(body["document"]["property_id"], "property-1");
        assert_eq!(body["document"]["guest_account_id"], "guest-1");
        assert_eq!(body["document"]["kind"], "receipt");
        assert_eq!(body["document"]["finance_id"], "finance-1");
        assert_eq!(body["document"]["snapshot_json"], "{\"amount\":100}");
        assert_eq!(body["document"]["created_at"], 99);
        assert_eq!(
            *seen.lock().unwrap(),
            vec![("company".to_owned(), "doc-1".to_owned())]
        );
    }

    #[tokio::test]
    async fn postings_payload_parity() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let postings = Some(vec![
            RelatedPosting { journal_id: "j-1".to_owned() },
            RelatedPosting { journal_id: "j-2".to_owned() },
        ]);
        let (port, server) = serve_probe(seen.clone(), None, postings).await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            postings_request(HOST, "company", "property-1", "doc-1"),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        assert_eq!(body["postings"][0]["journal_id"], "j-1");
        assert_eq!(body["postings"][1]["journal_id"], "j-2");
        assert_eq!(body["postings"].as_array().unwrap().len(), 2);
        assert_eq!(
            *seen.lock().unwrap(),
            vec![("company".to_owned(), "property-1:doc-1".to_owned())]
        );
    }

    #[tokio::test]
    async fn absent_document_is_null() {
        let (port, server) = serve_probe(Arc::new(Mutex::new(Vec::new())), None, None).await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            document_request(HOST, "company", "doc-1"),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        assert!(body["document"].is_null());
    }

    #[tokio::test]
    async fn wrong_host_fails_closed() {
        let (port, server) = serve_probe(Arc::new(Mutex::new(Vec::new())), Some(row()), None).await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            document_request("attacker.example.test", "company", "doc-1"),
        )
        .await;
        server.abort();
        assert_eq!(status, 404);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn invalid_identity_fails_closed() {
        let (port, server) = serve_probe(Arc::new(Mutex::new(Vec::new())), Some(row()), None).await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            document_request(HOST, "bad company id!", "doc-1"),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "PMS_NATIVE_DOCUMENTS_REQUEST_INVALID");
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            document_request(HOST, "company", "bad%20id!"),
        )
        .await;
        server.abort();
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "PMS_NATIVE_DOCUMENTS_REQUEST_INVALID");
    }

    #[test]
    fn configuration_requires_exact_distinct_targets() {
        let reader = Arc::new(ProbeDocumentRead::new(
            |_, _| Box::pin(async { Ok::<Option<DocumentRow>, BusinessError>(None) }),
            |_, _| Box::pin(async { Ok::<Option<Vec<RelatedPosting>>, BusinessError>(None) }),
        ));
        let target = || DocumentTarget {
            host: HOST.to_owned(),
            holding_slug: "acme".to_owned(),
        };
        assert!(router_with_reader(reader.clone(), vec![target()]).is_ok());
        assert!(router_with_reader(reader.clone(), vec![target(), target()]).is_err());
        assert!(router_with_reader(reader, vec![]).is_err());
    }
}
