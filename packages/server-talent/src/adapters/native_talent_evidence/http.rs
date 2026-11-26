//! Owner-local axum ingress for the native Talent evidence read.
//!
//! The handler never accepts SQL and never falls back to a TypeScript reader:
//! the injected owner reader executes the owner-authored SELECT and returns the
//! complete evidence row. Target, host and scope isolation follow the other
//! mounted Talent owner routes.

use crate::application::native_talent_evidence::{
    EvidenceHandler, EvidenceQuery, EvidenceRange, TalentEvidenceRead,
};
use crate::adapters::native_talent_evidence::postgres::PgTalentEvidenceRead;
use crate::domain::native_talent_evidence::{EvidenceMetric, MAX_IDENTIFIER, invalid};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_ACTOR: HeaderName = HeaderName::from_static("x-erpx-context-actor");
const CONTEXT_BRANCH: HeaderName = HeaderName::from_static("x-erpx-context-branch");
const CONTEXT_DEPARTMENT: HeaderName = HeaderName::from_static("x-erpx-context-department");
const EVIDENCE_FROM: HeaderName = HeaderName::from_static("x-erpx-evidence-from");
const EVIDENCE_TO: HeaderName = HeaderName::from_static("x-erpx-evidence-to");

/// Exact registered Holding target for this owner route.
pub struct TalentEvidenceTarget {
    pub host: String,
    pub holding_slug: String,
}

struct TargetHandlers {
    evidence: EvidenceHandler,
}

struct TalentEvidenceState {
    targets: BTreeMap<(String, String), TargetHandlers>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid_request() -> BusinessError {
    invalid()
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "TALENT_EVIDENCE_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
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

fn bounded(row: Option<Value>) -> Response {
    let mut response = (
        StatusCode::OK,
        Json(serde_json::json!({"data": row})),
    )
        .into_response();
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

fn assemble(targets: BTreeMap<(String, String), TargetHandlers>) -> Result<Router, BusinessError> {
    if targets.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/talent/executive-evidence/{metric}/{record_id}",
            get(read_evidence),
        )
        .with_state(Arc::new(TalentEvidenceState { targets })))
}

fn handlers(reader: Arc<dyn TalentEvidenceRead>) -> TargetHandlers {
    TargetHandlers {
        evidence: EvidenceHandler::new(reader),
    }
}

/// Build from one owner pool shared by all targets.
pub fn router(pool: DatabasePool, targets: Vec<TalentEvidenceTarget>) -> Result<Router, BusinessError> {
    router_for_holdings(targets.into_iter().map(|target| (target, pool.clone())).collect())
}

/// Build with one owner reader per target/pool, keyed by `(host, holding_slug)`.
pub fn router_for_holdings(
    targets: Vec<(TalentEvidenceTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, pool) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    handlers(Arc::new(PgTalentEvidenceRead::new(pool))),
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    assemble(configured)
}

/// Reader-injected variant used by unit tests and by a composition that already
/// owns the pooled reader instance. No TypeScript reader is ever consulted.
pub fn router_with_reader(
    reader: Arc<dyn TalentEvidenceRead>,
    targets: Vec<TalentEvidenceTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    handlers(reader.clone()),
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    assemble(configured)
}

fn resolve_target<'a>(
    state: &'a TalentEvidenceState,
    slug: &str,
    headers: &HeaderMap,
) -> Result<&'a TargetHandlers, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|host| !host.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn timestamp(value: Option<&str>) -> Result<Option<i64>, BusinessError> {
    match value {
        None => Ok(None),
        Some(value) => value
            .parse::<i64>()
            .map(Some)
            .map_err(|_| invalid_request()),
    }
}

async fn read_evidence(
    State(state): State<Arc<TalentEvidenceState>>,
    Path((slug, metric, record_id)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let has_query = request.uri().query().is_some();
    let result = async {
        let handlers = resolve_target(&state, &slug, &headers)?;
        if has_query {
            return Err(invalid_request());
        }
        let company = unique_header(&headers, CONTEXT_COMPANY)?
            .ok_or_else(invalid_request)?;
        let actor = unique_header(&headers, CONTEXT_ACTOR)?.ok_or_else(invalid_request)?;
        let branch = unique_header(&headers, CONTEXT_BRANCH)?;
        let department = unique_header(&headers, CONTEXT_DEPARTMENT)?;
        let from = timestamp(unique_header(&headers, EVIDENCE_FROM)?)?;
        let to_exclusive = timestamp(unique_header(&headers, EVIDENCE_TO)?)?;
        if company.is_empty()
            || company.len() > MAX_IDENTIFIER
            || actor.is_empty()
            || actor.len() > MAX_IDENTIFIER
        {
            return Err(invalid_request());
        }
        if from.is_some() != to_exclusive.is_some() {
            return Err(invalid_request());
        }
        let range = match (from, to_exclusive) {
            (Some(from), Some(to_exclusive)) => Some(EvidenceRange { from, to_exclusive }),
            _ => None,
        };
        let metric = EvidenceMetric::parse(&metric)?;
        let query = EvidenceQuery::new(
            metric,
            &record_id,
            company,
            branch,
            department,
            range,
        )?;
        let row = handlers.evidence.handle(query).await?;
        Ok::<_, BusinessError>(row)
    }
    .await;
    match result {
        Ok(row) => bounded(row),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::native_talent_evidence::EvidenceFuture;
    use erpx_server_runtime::public::http_support::serve;
    use serde_json::json;

    #[derive(Clone)]
    struct Probe;

    impl TalentEvidenceRead for Probe {
        fn evidence<'a>(&'a self, query: &'a EvidenceQuery) -> EvidenceFuture<'a, Option<Value>> {
            let id = query.record_id.clone();
            let metric = query.metric;
            let range = query.range;
            let missing = id == "app-missing";
            Box::pin(async move {
                if missing {
                    return Ok(None);
                }
                Ok(Some(if metric == EvidenceMetric::OpenRequisitions {
                    json!({
                        "id": id, "job_title": "Engineer", "target_start_date": "2026-12-01",
                        "status": "open", "version": 1,
                        "updated_at": 1_800_000_000_000i64,
                        "branch_id": "branch-1", "department_id": Value::Null
                    })
                } else {
                    json!({
                        "id": id, "requisition_id": "req-1", "status": "applied",
                        "version": 2, "updated_at": 1_800_000_000_000i64,
                        "job_title": "Engineer", "branch_id": "branch-1",
                        "department_id": Value::Null, "range_from": range.map(|value| value.from)
                    })
                }))
            })
        }
    }

    fn target() -> TalentEvidenceTarget {
        TalentEvidenceTarget {
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
        (status, serde_json::from_str(body).unwrap_or(serde_json::Value::Null))
    }

    async fn serve_probe() -> (u16, tokio::task::JoinHandle<()>) {
        let router = router_with_reader(Arc::new(Probe), vec![target()]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { let _ = serve(listener, router).await; });
        (addr.port(), server)
    }

    fn request(metric: &str, id: &str, extra: &str) -> String {
        format!(
            "GET /api/v1/holdings/acme/talent/executive-evidence/{metric}/{id} HTTP/1.1\r\n\
             Host: acme.example.test\r\nAccept: application/json\r\nConnection: close\r\n\
             x-erpx-context-company: company-1\r\n\
             x-erpx-context-actor: user-1\r\n\
             x-erpx-context-branch: branch-1\r\n{extra}\r\n"
        )
    }

    #[tokio::test]
    async fn evidence_payload_matches_every_owner_field() {
        let (port, server) = serve_probe().await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("open-requisitions", "req-1", ""),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        let data = &body["data"];
        assert_eq!(data["id"], "req-1");
        assert_eq!(data["job_title"], "Engineer");
        assert_eq!(data["target_start_date"], "2026-12-01");
        assert_eq!(data["status"], "open");
        assert_eq!(data["version"], 1);
        assert_eq!(data["updated_at"], 1_800_000_000_000i64);
        assert_eq!(data["branch_id"], "branch-1");
        assert!(data["department_id"].is_null());
    }

    #[tokio::test]
    async fn application_evidence_requires_a_bounded_range() {
        let (port, server) = serve_probe().await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("application-stage-counts", "app-1", ""),
        )
        .await;
        server.abort();
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "EVIDENCE_FILTER_REQUIRED");

        let (port, server) = serve_probe().await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request(
                "application-stage-counts",
                "app-1",
                "x-erpx-evidence-from: 1\r\nx-erpx-evidence-to: 2\r\n",
            ),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        assert_eq!(body["data"]["id"], "app-1");
        assert_eq!(body["data"]["range_from"], 1);
    }

    #[tokio::test]
    async fn missing_row_stays_null_and_wrong_host_fails_closed() {
        let (port, server) = serve_probe().await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("open-requisitions", "app-missing", ""),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        assert!(body["data"].is_null());

        let (port, server) = serve_probe().await;
        let raw = request("open-requisitions", "req-1", "")
            .replace("acme.example.test", "attacker.example.test");
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            raw,
        )
        .await;
        server.abort();
        assert_eq!(status, 404);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    #[test]
    fn configuration_requires_exact_distinct_holdings() {
        let probe = Arc::new(Probe);
        assert!(router_with_reader(probe.clone(), vec![target()]).is_ok());
        assert!(router_with_reader(probe.clone(), vec![target(), target()]).is_err());
        assert!(router_with_reader(probe, vec![]).is_err());
    }
}
