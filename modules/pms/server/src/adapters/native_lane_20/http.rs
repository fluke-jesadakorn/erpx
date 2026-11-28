//! Owner-local axum router for the native PMS property-readiness read.
//!
//! The route is isolated by exact Host + registered Holding slug, requires a
//! single Company and Property identity header, and returns only the bounded
//! boolean facts computed by the owner reader. There is no TS SQL fallback: a
//! missing/malformed native reply fails closed in the TS client with 503.

use super::application::{
    CalendarFuture, CalendarQuery, CalendarRead, ImportBatchFuture, ImportBatchRead,
    PropertyReadinessFuture, PropertyReadinessRead, RoomSummaryFuture, RoomSummaryQuery,
    RoomSummaryRead,
};
use super::domain::{
    CalendarRecord, ImportBatchRow, PropertyReadinessFacts, RoomSummaryFacts,
    configuration_unavailable, invalid_configuration, invalid_request, valid_filter, valid_id,
    valid_slug,
};
use super::postgres::{
    PgCalendarRead, PgImportBatchRead, PgPropertyReadinessRead, PgRoomSummaryRead,
};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Query, Request, Response, Router,
    State, StatusCode, header, routing::get, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_PROPERTY: HeaderName = HeaderName::from_static("x-erpx-context-property");

/// Registered Holding target. Both fields are exact: the Host header and the
/// path slug must match one configured target or the request is a 404.
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

/// Deterministic reader used by unit tests; production uses the Pg reader.
pub struct ProbePropertyReadinessRead<F> {
    load: F,
}

impl<F> ProbePropertyReadinessRead<F>
where
    F: Fn(&str, &str) -> PropertyReadinessFuture<'static, Option<PropertyReadinessFacts>>
        + Send
        + Sync,
{
    pub fn new(load: F) -> Self {
        Self { load }
    }
}

impl<F> PropertyReadinessRead for ProbePropertyReadinessRead<F>
where
    F: Fn(&str, &str) -> PropertyReadinessFuture<'static, Option<PropertyReadinessFacts>>
        + Send
        + Sync,
{
    fn load<'a>(
        &'a self,
        company: &'a str,
        property: &'a str,
    ) -> PropertyReadinessFuture<'a, Option<PropertyReadinessFacts>> {
        (self.load)(company, property)
    }
}

/// Deterministic room-summary reader used by unit tests.
pub struct ProbeRoomSummaryRead<F> {
    load: F,
}

impl<F> ProbeRoomSummaryRead<F>
where
    F: Fn(&str, &RoomSummaryQuery) -> RoomSummaryFuture<'static, Option<RoomSummaryFacts>>
        + Send
        + Sync,
{
    pub fn new(load: F) -> Self {
        Self { load }
    }
}

impl<F> RoomSummaryRead for ProbeRoomSummaryRead<F>
where
    F: Fn(&str, &RoomSummaryQuery) -> RoomSummaryFuture<'static, Option<RoomSummaryFacts>>
        + Send
        + Sync,
{
    fn load<'a>(
        &'a self,
        company: &'a str,
        query: &'a RoomSummaryQuery,
    ) -> RoomSummaryFuture<'a, Option<RoomSummaryFacts>> {
        (self.load)(company, query)
    }
}

/// Deterministic import-batch reader used by unit tests.
pub struct ProbeImportBatchRead<F> {
    load: F,
}

impl<F> ProbeImportBatchRead<F>
where
    F: Fn(&str, &str) -> ImportBatchFuture<'static, Option<Vec<ImportBatchRow>>> + Send + Sync,
{
    pub fn new(load: F) -> Self {
        Self { load }
    }
}

impl<F> ImportBatchRead for ProbeImportBatchRead<F>
where
    F: Fn(&str, &str) -> ImportBatchFuture<'static, Option<Vec<ImportBatchRow>>> + Send + Sync,
{
    fn load<'a>(
        &'a self,
        company: &'a str,
        property: &'a str,
    ) -> ImportBatchFuture<'a, Option<Vec<ImportBatchRow>>> {
        (self.load)(company, property)
    }
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn PropertyReadinessRead>>,
    summaries: BTreeMap<(String, String), Arc<dyn RoomSummaryRead>>,
    batches: BTreeMap<(String, String), Arc<dyn ImportBatchRead>>,
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

/// Build a state map keyed by exact `(host, holding_slug)`. Each entry carries
/// its own reader (and therefore its own pool); duplicate or malformed targets
/// are rejected so one Holding can never borrow another's reader.
fn reader_map<R>(
    targets: Vec<(LaneTarget, Arc<R>)>,
) -> Result<BTreeMap<(String, String), Arc<R>>, BusinessError>
where
    R: ?Sized,
{
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
            "/api/v1/holdings/{holding_slug}/pms/native-lane-20/property-readiness",
            get(readiness),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/pms/native-lane-20/room-summary",
            get(room_summary),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/pms/native-lane-20/import-batches",
            get(import_batches),
        )
        .with_state(state)
}

/// Composition-facing constructor for a single pool shared by all targets.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_for_holdings(targets.into_iter().map(|target| (target, pool.clone())).collect())
}

/// Composition-facing constructor: one reader of each family is built per
/// registered Holding from that Holding's own database pool and inserted into
/// the state maps keyed by `(host, holding_slug)`. The returned router serves
/// both native lane 20 read routes.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let mut readiness = Vec::with_capacity(targets.len());
    let mut summaries = Vec::with_capacity(targets.len());
    let mut batches = Vec::with_capacity(targets.len());
    for (target, pool) in targets {
        readiness.push((
            LaneTarget {
                host: target.host.clone(),
                holding_slug: target.holding_slug.clone(),
            },
            Arc::new(PgPropertyReadinessRead::new(pool.clone())) as Arc<dyn PropertyReadinessRead>,
        ));
        summaries.push((
            LaneTarget {
                host: target.host.clone(),
                holding_slug: target.holding_slug.clone(),
            },
            Arc::new(PgRoomSummaryRead::new(pool.clone())) as Arc<dyn RoomSummaryRead>,
        ));
        batches.push((
            target,
            Arc::new(PgImportBatchRead::new(pool)) as Arc<dyn ImportBatchRead>,
        ));
    }
    Ok(route(Arc::new(LaneState {
        targets: reader_map(readiness)?,
        summaries: reader_map(summaries)?,
        batches: reader_map(batches)?,
    })))
}

/// Test-facing constructor: inject one deterministic readiness reader per
/// target; the room-summary map stays empty so those requests fail closed.
pub fn router_with_readers(
    targets: Vec<(LaneTarget, Arc<dyn PropertyReadinessRead>)>,
) -> Result<Router, BusinessError> {
    Ok(route(Arc::new(LaneState {
        targets: reader_map(targets)?,
        summaries: BTreeMap::new(),
        batches: BTreeMap::new(),
    })))
}

/// Test-facing constructor: one injected reader shared by all targets.
pub fn router_with_reader(
    reader: Arc<dyn PropertyReadinessRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

/// Test-facing constructor for both families without a live database.
#[cfg(test)]
fn router_with_summary_readers(
    readiness: Vec<(LaneTarget, Arc<dyn PropertyReadinessRead>)>,
    summaries: Vec<(LaneTarget, Arc<dyn RoomSummaryRead>)>,
) -> Result<Router, BusinessError> {
    Ok(route(Arc::new(LaneState {
        targets: reader_map(readiness)?,
        summaries: reader_map(summaries)?,
        batches: BTreeMap::new(),
    })))
}

/// Test-facing constructor for the import-batch family without a live database.
#[cfg(test)]
fn router_with_batch_readers(
    batches: Vec<(LaneTarget, Arc<dyn ImportBatchRead>)>,
) -> Result<Router, BusinessError> {
    Ok(route(Arc::new(LaneState {
        targets: BTreeMap::new(),
        summaries: BTreeMap::new(),
        batches: reader_map(batches)?,
    })))
}

#[derive(Debug, Deserialize)]
pub struct ImportBatchParams {
    pub property: String,
}

#[derive(Debug, Deserialize)]
pub struct RoomSummaryParams {
    pub property: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub building: String,
    #[serde(default)]
    pub floor: String,
    #[serde(default, rename = "type")]
    pub type_id: String,
}

async fn readiness(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
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
        let property = unique_header(request.headers(), CONTEXT_PROPERTY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid_request)?;
        let facts = reader.load(company, property).await?;
        let facts = facts.map(|facts| {
            serde_json::json!({
                "hasRoomTypes": facts.has_room_types,
                "hasActiveRooms": facts.has_active_rooms,
                "hasActiveRates": facts.has_active_rates,
                "hasStays": facts.has_stays,
            })
        });
        Ok::<_, BusinessError>(serde_json::json!({"facts": facts}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn room_summary(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    Query(params): Query<RoomSummaryParams>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .summaries
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid_request)?;
        if !valid_id(&params.property)
            || !valid_filter(&params.status)
            || !valid_filter(&params.q)
            || !valid_filter(&params.building)
            || !valid_filter(&params.floor)
            || !valid_filter(&params.type_id)
        {
            return Err(invalid_request());
        }
        let query = RoomSummaryQuery {
            property: params.property,
            status: params.status,
            q: params.q,
            building: params.building,
            floor: params.floor,
            type_id: params.type_id,
        };
        let summary = reader.load(company, &query).await?;
        let summary = summary.map(|facts| {
            serde_json::json!({
                "total": facts.total,
                "active": facts.active,
                "dirty": facts.dirty,
                "clean": facts.clean,
                "inspected": facts.inspected,
                "dnd": facts.dnd,
                "occupied": facts.occupied,
            })
        });
        Ok::<_, BusinessError>(serde_json::json!({"summary": summary}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn import_batches(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    Query(params): Query<ImportBatchParams>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .batches
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid_request)?;
        if !valid_id(&params.property) {
            return Err(invalid_request());
        }
        let rows = reader.load(company, &params.property).await?;
        let rows = rows.map(|rows| {
            rows.into_iter()
                .map(|row| {
                    serde_json::json!({
                        "id": row.id,
                        "cutover_date": row.cutover_date,
                        "state": row.state,
                        "version": row.version,
                        "created_by": row.created_by,
                        "approved_by": row.approved_by,
                        "opening_journal_id": row.opening_journal_id,
                        "report_json": row.report_json,
                        "created_at": row.created_at,
                    })
                })
                .collect::<Vec<_>>()
        });
        Ok::<_, BusinessError>(serde_json::json!({"batches": rows}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

// Keep the availability error reachable from the router's public surface so the
// TS transport and the reader agree on the same fail-closed code.
pub fn unavailable() -> BusinessError {
    configuration_unavailable()
}

#[cfg(test)]
mod room_summary_tests {
    use super::*;
    use crate::adapters::native_lane_20::postgres::ROOM_SUMMARY_SQL;
    use erpx_platform_runtime_server::public::http_support::serve;
    use sha2::{Digest, Sha256};
    use std::sync::Mutex;

    fn summary() -> RoomSummaryFacts {
        RoomSummaryFacts {
            total: 4,
            active: 3,
            dirty: 1,
            clean: 2,
            inspected: 0,
            dnd: 1,
            occupied: 2,
        }
    }

    fn sha(value: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
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
        seen: Arc<Mutex<Vec<(String, RoomSummaryQuery)>>>,
        answer: Option<RoomSummaryFacts>,
    ) -> (u16, tokio::task::JoinHandle<()>) {
        let readiness = Arc::new(ProbePropertyReadinessRead::new(|_, _| {
            Box::pin(async { Ok::<Option<PropertyReadinessFacts>, BusinessError>(None) })
        }));
        let probe = Arc::new(ProbeRoomSummaryRead::new(move |company, query| {
            let company = company.to_owned();
            let query = query.clone();
            let seen = seen.clone();
            Box::pin(async move {
                seen.lock().unwrap().push((company, query));
                Ok::<Option<RoomSummaryFacts>, BusinessError>(answer)
            })
        }));
        let target = || LaneTarget {
            host: "acme.example.test".to_owned(),
            holding_slug: "acme".to_owned(),
        };
        let router = router_with_summary_readers(
            vec![(target(), readiness)],
            vec![(target(), probe)],
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = serve(listener, router).await;
        });
        (addr.port(), server)
    }

    fn request(host: &str, company: &str, query: &str) -> String {
        format!(
            "GET /api/v1/holdings/acme/pms/native-lane-20/room-summary?{query} HTTP/1.1\r\n\
             Host: {host}\r\n\
             Accept: application/json\r\n\
             Connection: close\r\n\
             x-erpx-context-company: {company}\r\n\r\n"
        )
    }

    const QUERY: &str = "property=property-1&status=dirty&q=101&building=Main&floor=1&type=room-1";

    #[tokio::test]
    async fn room_summary_parity_and_filter_witness() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (port, server) = serve_probe(seen.clone(), Some(summary())).await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("acme.example.test", "company", QUERY),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        assert_eq!(body["summary"]["total"], 4);
        assert_eq!(body["summary"]["active"], 3);
        assert_eq!(body["summary"]["dirty"], 1);
        assert_eq!(body["summary"]["clean"], 2);
        assert_eq!(body["summary"]["inspected"], 0);
        assert_eq!(body["summary"]["dnd"], 1);
        assert_eq!(body["summary"]["occupied"], 2);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, "company");
        assert_eq!(seen[0].1.property, "property-1");
        assert_eq!(seen[0].1.status, "dirty");
        assert_eq!(seen[0].1.q, "101");
        assert_eq!(seen[0].1.building, "Main");
        assert_eq!(seen[0].1.floor, "1");
        assert_eq!(seen[0].1.type_id, "room-1");
    }

    #[tokio::test]
    async fn foreign_property_is_null() {
        let (port, server) = serve_probe(Arc::new(Mutex::new(Vec::new())), None).await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("acme.example.test", "company", "property=foreign"),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        assert!(body["summary"].is_null());
    }

    #[tokio::test]
    async fn wrong_host_and_bad_property_fail_closed() {
        let (port, server) = serve_probe(Arc::new(Mutex::new(Vec::new())), Some(summary())).await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("attacker.example.test", "company", QUERY),
        )
        .await;
        assert_eq!(status, 404);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("acme.example.test", "company", "property=bad%20id!"),
        )
        .await;
        server.abort();
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "PMS_NATIVE_LANE_20_REQUEST_INVALID");
    }

    #[test]
    fn room_summary_oracles() {
        assert_eq!(
            sha(ROOM_SUMMARY_SQL),
            "676661c61ace9c3fd137bcce3be879776266906ce20d45199c00d918c4ee613e"
        );
        let result = serde_json::json!({
            "total": 4,
            "active": 3,
            "dirty": 1,
            "clean": 2,
            "inspected": 0,
            "dnd": 1,
            "occupied": 2,
        })
        .to_string();
        assert_eq!(
            sha(&result),
            "a73433d444f53f1d87215bfceed8efb5429c8e87f5f02dc5000c199d9120d859"
        );
    }
}

#[cfg(test)]
mod import_batch_tests {
    use super::*;
    use crate::adapters::native_lane_20::postgres::IMPORT_BATCHES_SQL;
    use erpx_platform_runtime_server::public::http_support::serve;
    use sha2::{Digest, Sha256};
    use std::sync::Mutex;

    fn row() -> ImportBatchRow {
        ImportBatchRow {
            id: "b1".to_owned(),
            cutover_date: "2026-11-01".to_owned(),
            state: "preview".to_owned(),
            version: 2,
            created_by: "user".to_owned(),
            approved_by: None,
            opening_journal_id: None,
            report_json: "{}".to_owned(),
            created_at: 99,
        }
    }

    fn sha(value: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
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
        answer: Option<Vec<ImportBatchRow>>,
    ) -> (u16, tokio::task::JoinHandle<()>) {
        let probe = Arc::new(ProbeImportBatchRead::new(move |company, property| {
            let company = company.to_owned();
            let property = property.to_owned();
            let seen = seen.clone();
            let answer = answer.clone();
            Box::pin(async move {
                seen.lock().unwrap().push((company, property));
                Ok::<Option<Vec<ImportBatchRow>>, BusinessError>(answer)
            })
        }));
        let router = router_with_batch_readers(vec![(
            LaneTarget {
                host: "acme.example.test".to_owned(),
                holding_slug: "acme".to_owned(),
            },
            probe,
        )])
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = serve(listener, router).await;
        });
        (addr.port(), server)
    }

    fn request(host: &str, company: &str, property: &str) -> String {
        format!(
            "GET /api/v1/holdings/acme/pms/native-lane-20/import-batches?property={property} HTTP/1.1\r\n\
             Host: {host}\r\n\
             Accept: application/json\r\n\
             Connection: close\r\n\
             x-erpx-context-company: {company}\r\n\r\n"
        )
    }

    #[tokio::test]
    async fn import_batches_parity_and_scope() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (port, server) = serve_probe(seen.clone(), Some(vec![row()])).await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("acme.example.test", "company", "property-1"),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        assert_eq!(body["batches"][0]["id"], "b1");
        assert_eq!(body["batches"][0]["cutover_date"], "2026-11-01");
        assert_eq!(body["batches"][0]["state"], "preview");
        assert_eq!(body["batches"][0]["version"], 2);
        assert_eq!(body["batches"][0]["created_by"], "user");
        assert!(body["batches"][0]["approved_by"].is_null());
        assert!(body["batches"][0]["opening_journal_id"].is_null());
        assert_eq!(body["batches"][0]["report_json"], "{}");
        assert_eq!(body["batches"][0]["created_at"], 99);
        assert_eq!(
            *seen.lock().unwrap(),
            vec![("company".to_owned(), "property-1".to_owned())]
        );
    }

    #[tokio::test]
    async fn foreign_property_is_null() {
        let (port, server) = serve_probe(Arc::new(Mutex::new(Vec::new())), None).await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("acme.example.test", "company", "foreign"),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        assert!(body["batches"].is_null());
    }

    #[tokio::test]
    async fn wrong_host_and_bad_property_fail_closed() {
        let (port, server) = serve_probe(Arc::new(Mutex::new(Vec::new())), Some(vec![row()])).await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("attacker.example.test", "company", "property-1"),
        )
        .await;
        assert_eq!(status, 404);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("acme.example.test", "company", "bad%20id!"),
        )
        .await;
        server.abort();
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "PMS_NATIVE_LANE_20_REQUEST_INVALID");
    }

    #[test]
    fn import_batch_oracles() {
        assert_eq!(
            sha(IMPORT_BATCHES_SQL),
            "67821c399421eaf1320683cc3aec083dcb69a7ffe38b5492d4c81bff3161bb5c"
        );
        let result = serde_json::json!({
            "id": "b1",
            "cutover_date": "2026-11-01",
            "state": "preview",
            "version": 2,
            "created_by": "user",
            "approved_by": null,
            "opening_journal_id": null,
            "report_json": "{}",
            "created_at": 99,
        })
        .to_string();
        assert_eq!(
            sha(&result),
            "bb9e21645dd88dc468bd9822b1121839afcbc29ff62d5b8bfaa5e915a50a2e1e"
        );
    }
}
