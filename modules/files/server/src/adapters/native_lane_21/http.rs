//! Owner-local axum router for the Files commercial-usage read family.
//!
//! The route is registered by an exact `(host, holding_slug)` target and an
//! authorized `x-erpx-context-company` scope. It transports only a bounded
//! discriminator and parameters; the owner owns the SQL, the projection and
//! the response shape. Failures never fall back to TypeScript SQL.

use crate::adapters::native_lane_21::application::{
    CommercialUsageRead, ReviewFilesRead, TaxSignatureRead, handle, handle_review, handle_tax,
};
use crate::adapters::native_lane_21::domain::{
    ReviewFilesRequest, ReviewFilesResponse, TaxSignatureRequest, TaxSignatureResponse,
    UsageReadRequest, UsageResponse, invalid, response_witness, review_invalid,
    review_response_witness, tax_invalid, tax_response_witness, valid_id, valid_slug,
};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, json, routing::post, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const MAX_BODY_BYTES: usize = 16 * 1024;

/// One exact registered Holding target. Composition supplies the private base
/// host and the holding path slug; no wildcard or suffix match is accepted.
#[derive(Clone)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct ReadState {
    targets: BTreeMap<(String, String), Arc<dyn CommercialUsageRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "FILES_COMMERCIAL_USAGE_ROUTE_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
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

fn bounded<T: serde::Serialize>(body: T) -> Response {
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
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

type Readers = BTreeMap<(String, String), Arc<dyn CommercialUsageRead>>;

fn register(
    readers: &mut Readers,
    target: LaneTarget,
    reader: Arc<dyn CommercialUsageRead>,
) -> Result<(), BusinessError> {
    if target.host.is_empty()
        || !valid_slug(&target.holding_slug)
        || readers
            .insert((target.host.clone(), target.holding_slug.clone()), reader)
            .is_some()
    {
        return Err(configuration_invalid());
    }
    Ok(())
}

fn register_tax(
    readers: &mut BTreeMap<(String, String), Arc<dyn TaxSignatureRead>>,
    target: LaneTarget,
    reader: Arc<dyn TaxSignatureRead>,
) -> Result<(), BusinessError> {
    if target.host.is_empty()
        || !valid_slug(&target.holding_slug)
        || readers
            .insert((target.host.clone(), target.holding_slug.clone()), reader)
            .is_some()
    {
        return Err(configuration_invalid());
    }
    Ok(())
}

fn assemble(readers: Readers) -> Result<Router, BusinessError> {
    if readers.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/files/commercial-usage",
            post(read),
        )
        .with_state(Arc::new(ReadState { targets: readers })))
}

fn pg_reader(pool: DatabasePool) -> Arc<dyn CommercialUsageRead> {
    Arc::new(crate::adapters::native_lane_21::postgres::PgCommercialUsageRead::new(pool))
}

/// Build the owner read router from one owner Pg pool shared by every target.
/// The lead mounts it; this module never edits composition and introduces no
/// new TypeScript SQL or business rule.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let mut readers = BTreeMap::new();
    let mut tax_readers = BTreeMap::new();
    for target in targets {
        register(&mut readers, target.clone(), pg_reader(pool.clone()))?;
        register_tax(&mut tax_readers, target, tax_reader(pool.clone()))?;
    }
    Ok(assemble(readers)?.merge(assemble_tax(tax_readers)))
}

/// Build the same owner router with one injected reader shared by all targets.
/// Used by the owner's own HTTP contract tests and by composition when it
/// already holds a reader.
pub fn router_with_reader(
    reader: Arc<dyn CommercialUsageRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    let mut readers = BTreeMap::new();
    for target in targets {
        register(&mut readers, target, reader.clone())?;
    }
    assemble(readers)
}

/// Build the owner router with one reader per `(LaneTarget, DatabasePool)` pair.
/// Each registered Holding is served by its own pool; the state is still keyed
/// by the exact `(host, holding_slug)` pair, so a caller cannot cross a Holding
/// boundary even when two targets share a host or pool. Distinct targets and a
/// non-empty set are required, matching the other constructors.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let mut readers = BTreeMap::new();
    let mut tax_readers = BTreeMap::new();
    for (target, pool) in targets {
        register(&mut readers, target.clone(), pg_reader(pool.clone()))?;
        register_tax(&mut tax_readers, target, tax_reader(pool))?;
    }
    Ok(assemble(readers)?.merge(assemble_tax(tax_readers)))
}

async fn read(
    State(state): State<Arc<ReadState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        // These reads are holding-wide: the company header is optional. If a
        // caller supplies one it must be a single, bounded, valid record id.
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .map(|value| {
                if valid_id(value) {
                    Ok(value.to_owned())
                } else {
                    Err(invalid())
                }
            })
            .transpose()?;
        let (_, body) = request.into_parts();
        let bytes = to_bytes(body, MAX_BODY_BYTES)
            .await
            .map_err(|_| invalid())?;
        let request: UsageReadRequest = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        let data = handle(reader.as_ref(), company.as_deref(), request.clone()).await?;
        let witness = response_witness(company.as_deref(), &request, &data);
        Ok::<_, BusinessError>(UsageResponse { data, witness })
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

/// State for the company-scoped tax-signature attachment reads.
struct TaxReadState {
    targets: BTreeMap<(String, String), Arc<dyn TaxSignatureRead>>,
}

fn assemble_tax(readers: BTreeMap<(String, String), Arc<dyn TaxSignatureRead>>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/files/finance-tax-signatures",
            post(tax_read),
        )
        .with_state(Arc::new(TaxReadState { targets: readers }))
}

fn tax_reader(pool: DatabasePool) -> Arc<dyn TaxSignatureRead> {
    Arc::new(crate::adapters::native_lane_21::postgres::PgTaxSignatureRead::new(pool))
}

async fn tax_read(
    State(state): State<Arc<TaxReadState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        // Company is mandatory for this company-scoped family.
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(tax_invalid)?
            .to_owned();
        let (_, body) = request.into_parts();
        let bytes = to_bytes(body, MAX_BODY_BYTES)
            .await
            .map_err(|_| tax_invalid())?;
        let request: TaxSignatureRequest =
            serde_json::from_slice(&bytes).map_err(|_| tax_invalid())?;
        let data = handle_tax(reader.as_ref(), &company, request.clone()).await?;
        let witness = tax_response_witness(&company, &request, &data);
        Ok::<_, BusinessError>(TaxSignatureResponse { data, witness })
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

/// State for the company-scoped review-file list. Composition uses the exact
/// host+slug target; the company header selects the company inside it.
struct ReviewReadState {
    targets: BTreeMap<(String, String), Arc<dyn ReviewFilesRead>>,
}

fn assemble_review(readers: BTreeMap<(String, String), Arc<dyn ReviewFilesRead>>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/files/finance-review-files",
            post(review_read),
        )
        .with_state(Arc::new(ReviewReadState { targets: readers }))
}

fn review_reader(pool: DatabasePool) -> Arc<dyn ReviewFilesRead> {
    Arc::new(crate::adapters::native_lane_21::postgres::PgReviewFilesRead::new(pool))
}

async fn review_read(
    State(state): State<Arc<ReviewReadState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(review_invalid)?
            .to_owned();
        let (_, body) = request.into_parts();
        let bytes = to_bytes(body, MAX_BODY_BYTES)
            .await
            .map_err(|_| review_invalid())?;
        let request: ReviewFilesRequest = serde_json::from_slice(&bytes).map_err(|_| review_invalid())?;
        let data = handle_review(reader.as_ref(), &company, request.clone()).await?;
        let witness = review_response_witness(&company, &request, &data);
        Ok::<_, BusinessError>(ReviewFilesResponse { data, witness })
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
    use crate::adapters::native_lane_21::application::UsageReadFuture;
    use crate::adapters::native_lane_21::domain::{
        KIND_RECONCILIATION, TAX_WORKSPACE, TaxSignatureFacts, TaxSignatureReadKind, UsageFacts,
        UsageReadKind,
    };
    use std::sync::Mutex;

    struct Probe {
        calls: Mutex<Vec<(Option<String>, UsageReadKind)>>,
    }

    impl CommercialUsageRead for Probe {
        fn read<'a>(
            &'a self,
            company: Option<&'a str>,
            kind: UsageReadKind,
            _request: &'a UsageReadRequest,
        ) -> UsageReadFuture<'a, UsageFacts> {
            Box::pin(async move {
                self.calls
                    .lock()
                    .expect("mutex")
                    .push((company.map(str::to_owned), kind));
                Ok(UsageFacts::Reconciliation { ready: true })
            })
        }
    }

    fn probe() -> Arc<Probe> {
        Arc::new(Probe {
            calls: Mutex::new(Vec::new()),
        })
    }

    fn target() -> LaneTarget {
        LaneTarget {
            host: "acme.example.test".to_owned(),
            holding_slug: "acme".to_owned(),
        }
    }

    async fn serve_probe(reader: Arc<Probe>) -> (u16, tokio::task::JoinHandle<()>) {
        let router = router_with_reader(reader, vec![target()]).expect("router");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let port = listener.local_addr().expect("addr").port();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        (port, server)
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, serde_json::Value) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
        stream.write_all(raw.as_bytes()).await.expect("write");
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.expect("read");
        let text = String::from_utf8_lossy(&bytes);
        let status = text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok())
            .expect("status");
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
        (
            status,
            serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
        )
    }

    fn request(host: &str, company: Option<&str>, body: &str) -> String {
        let company_header = company
            .map(|value| format!("x-erpx-context-company: {value}\r\n"))
            .unwrap_or_default();
        format!(
            "POST /api/v1/holdings/acme/files/commercial-usage HTTP/1.1\r\n\
             Host: {host}\r\n\
             Accept: application/json\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             {company_header}\r\n\
             {body}",
            body.len()
        )
    }

    #[tokio::test]
    async fn complete_payload_and_company_scope() {
        let reader = probe();
        let (port, server) = serve_probe(reader.clone()).await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request(
                "acme.example.test",
                Some("company-1"),
                &format!("{{\"kind\":\"{KIND_RECONCILIATION}\"}}"),
            ),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        assert_eq!(body["data"]["kind"], KIND_RECONCILIATION);
        assert_eq!(body["data"]["ready"], true);
        assert!(body["witness"].as_str().is_some_and(|w| w.len() == 64));
        assert_eq!(
            *reader.calls.lock().expect("mutex"),
            vec![(Some("company-1".to_owned()), UsageReadKind::Reconciliation)]
        );
    }

    #[tokio::test]
    async fn holding_wide_read_needs_no_company_header() {
        let reader = probe();
        let (port, server) = serve_probe(reader.clone()).await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request(
                "acme.example.test",
                None,
                &format!("{{\"kind\":\"{KIND_RECONCILIATION}\"}}"),
            ),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        assert_eq!(body["data"]["ready"], true);
        assert_eq!(
            *reader.calls.lock().expect("mutex"),
            vec![(None, UsageReadKind::Reconciliation)]
        );
    }

    #[tokio::test]
    async fn wrong_host_fails_closed() {
        let reader = probe();
        let (port, server) = serve_probe(reader.clone()).await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request(
                "attacker.example.test",
                Some("company-1"),
                &format!("{{\"kind\":\"{KIND_RECONCILIATION}\"}}"),
            ),
        )
        .await;
        server.abort();
        assert_eq!(status, 404);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
        assert!(reader.calls.lock().expect("mutex").is_empty());
    }

    #[tokio::test]
    async fn invalid_scope_and_body_fail_closed() {
        let reader = probe();
        let (port, server) = serve_probe(reader.clone()).await;
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        for raw in [
            request("acme.example.test", Some("bad company!"), "{}"),
            request(
                "acme.example.test",
                Some("company-1"),
                &format!("{{\"kind\":\"{KIND_RECONCILIATION}\",\"end\":9}}"),
            ),
            request("acme.example.test", Some("company-1"), "not-json"),
        ] {
            let (status, body) = http(addr, raw).await;
            assert_eq!(status, 422);
            assert_eq!(body["error"]["code"], "FILES_COMMERCIAL_USAGE_READ_INVALID");
        }
        server.abort();
        assert!(reader.calls.lock().expect("mutex").is_empty());
    }

    struct TaxProbe {
        calls: Mutex<Vec<(String, TaxSignatureReadKind)>>,
    }

    impl TaxSignatureRead for TaxProbe {
        fn read<'a>(
            &'a self,
            company: &'a str,
            kind: TaxSignatureReadKind,
            _request: &'a TaxSignatureRequest,
        ) -> UsageReadFuture<'a, TaxSignatureFacts> {
            Box::pin(async move {
                self.calls
                    .lock()
                    .expect("mutex")
                    .push((company.to_owned(), kind));
                Ok(TaxSignatureFacts::Workspace { rows: Vec::new() })
            })
        }
    }

    async fn serve_tax_probe(reader: Arc<TaxProbe>) -> (u16, tokio::task::JoinHandle<()>) {
        let mut readers = BTreeMap::new();
        readers.insert(
            ("acme.example.test".to_owned(), "acme".to_owned()),
            reader as Arc<dyn TaxSignatureRead>,
        );
        let router = assemble_tax(readers);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let port = listener.local_addr().expect("addr").port();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        (port, server)
    }

    fn tax_request(host: &str, company: Option<&str>, body: &str) -> String {
        let company_header = company
            .map(|value| format!("x-erpx-context-company: {value}\r\n"))
            .unwrap_or_default();
        format!(
            "POST /api/v1/holdings/acme/files/finance-tax-signatures HTTP/1.1\r\n\
             Host: {host}\r\n\
             Accept: application/json\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             {company_header}\r\n\
             {body}",
            body.len()
        )
    }

    #[tokio::test]
    async fn tax_read_requires_scope_and_exact_target() {
        let reader = Arc::new(TaxProbe {
            calls: Mutex::new(Vec::new()),
        });
        let (port, server) = serve_tax_probe(reader.clone()).await;
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        let body = format!("{{\"kind\":\"{TAX_WORKSPACE}\"}}");
        let (status, body) = http(
            addr,
            tax_request("acme.example.test", Some("company-1"), &body),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["data"]["kind"], TAX_WORKSPACE);
        assert!(body["data"]["rows"].as_array().is_some_and(Vec::is_empty));
        assert!(body["witness"].as_str().is_some_and(|w| w.len() == 64));
        assert_eq!(
            *reader.calls.lock().expect("mutex"),
            vec![("company-1".to_owned(), TaxSignatureReadKind::Workspace)]
        );

        let (status, body) = http(
            addr,
            tax_request(
                "acme.example.test",
                None,
                &format!("{{\"kind\":\"{TAX_WORKSPACE}\"}}"),
            ),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "FILES_TAX_SIGNATURE_READ_INVALID");
        let (status, _) = http(
            addr,
            tax_request(
                "attacker.example.test",
                Some("company-1"),
                &format!("{{\"kind\":\"{TAX_WORKSPACE}\"}}"),
            ),
        )
        .await;
        assert_eq!(status, 404);
        server.abort();
        assert_eq!(reader.calls.lock().expect("mutex").len(), 1);
    }

    #[test]
    fn configuration_requires_exact_distinct_holdings() {
        let reader = probe();
        assert!(router_with_reader(reader.clone(), vec![target()]).is_ok());
        assert!(router_with_reader(reader.clone(), vec![target(), target()]).is_err());
        assert!(router_with_reader(reader.clone(), vec![]).is_err());
        assert!(
            router_with_reader(
                reader,
                vec![LaneTarget {
                    host: "a".to_owned(),
                    holding_slug: "BAD".to_owned(),
                }]
            )
            .is_err()
        );
    }

    #[test]
    fn maps_failure_kinds_to_http_status() {
        assert_eq!(
            status(FailureKind::InvalidInput),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(status(FailureKind::Forbidden), StatusCode::FORBIDDEN);
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(
            status(FailureKind::Unavailable),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug(""));
    }
}
