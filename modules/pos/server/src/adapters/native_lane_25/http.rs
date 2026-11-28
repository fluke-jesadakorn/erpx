//! Owner-local axum ingress for the native POS evidence search.
//!
//! Transport facts only: registered Host/Holding target, exact bounded Company
//! and branch headers, a strict JSON envelope and business-error mapping. The
//! pool-backed owner reader owns the SQL, the projection and the response row.

use crate::adapters::native_lane_25::application::{
    PosEvidenceReader, read, read_day_closes, read_detail,
};
use crate::adapters::native_lane_25::domain::{
    DETAIL_KINDS, DayClosesQuery, EvidenceDetailQuery, EvidenceQuery, invalid, validate,
    validate_day_closes, validate_detail,
};
use crate::adapters::native_lane_25::postgres::PgEvidenceReader;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};
use serde::Deserialize;
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: &str = "x-erpx-context-company";
const WORKSPACE_BRANCH: &str = "x-erpx-workspace-branch";
const MAX_BODY_BYTES: usize = 16 * 1024;

pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorBody {
    #[serde(rename = "eventDate")]
    event_date: String,
    id: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchBody {
    metric: String,
    from: String,
    to: String,
    currency: String,
    query: String,
    limit: usize,
    cursor: Option<CursorBody>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct DetailBody {
    kind: String,
    id: String,
    from: String,
    to: String,
    currency: String,
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn PosEvidenceReader>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "POS_EVIDENCE_READ_UNAVAILABLE",
        "The native POS evidence service is unavailable.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "POS_EVIDENCE_CONFIG_INVALID",
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

fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
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

fn bounded(value: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(value)).into_response();
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

impl LaneState {
    /// One reader per registered Holding target, keyed by `(host, slug)`.
    /// Each entry carries its own pool-backed reader, so Holdings never share
    /// a database connection pool.
    fn with_readers(
        entries: Vec<(LaneTarget, Arc<dyn PosEvidenceReader>)>,
    ) -> Result<Self, BusinessError> {
        let mut configured = BTreeMap::new();
        for (target, reader) in entries {
            if target.host.is_empty()
                || !valid_slug(&target.holding_slug)
                || configured
                    .insert(
                        (target.host.clone(), target.holding_slug.clone()),
                        reader,
                    )
                    .is_some()
            {
                return Err(configuration_invalid());
            }
        }
        if configured.is_empty() {
            return Err(configuration_invalid());
        }
        Ok(Self {
            targets: configured,
        })
    }

    /// Shared-reader form retained for the single-reader tests.
    #[cfg(test)]
    fn new(
        reader: Arc<dyn PosEvidenceReader>,
        targets: Vec<LaneTarget>,
    ) -> Result<Self, BusinessError> {
        Self::with_readers(
            targets
                .into_iter()
                .map(|target| (target, reader.clone()))
                .collect(),
        )
    }

    #[cfg(test)]
    fn reader_for(&self, host: &str, slug: &str) -> Option<&Arc<dyn PosEvidenceReader>> {
        self.targets.get(&(host.to_owned(), slug.to_owned()))
    }

    async fn search(
        &self,
        headers: &HeaderMap,
        slug: &str,
        body: &[u8],
    ) -> Response {
        match self.search_inner(headers, slug, body).await {
            Ok(rows) => bounded(serde_json::json!({ "rows": rows })),
            Err(error) => error_response(error),
        }
    }

    async fn search_inner(
        &self,
        headers: &HeaderMap,
        slug: &str,
        body: &[u8],
    ) -> Result<Vec<serde_json::Value>, BusinessError> {
        let host = unique_header(headers, header::HOST.as_str())?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let reader = self
            .targets
            .get(&(host.to_owned(), slug.to_owned()))
            .ok_or_else(missing)?;
        let company = unique_header(headers, CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?;
        let branch = unique_header(headers, WORKSPACE_BRANCH)?
            .filter(|value| !value.is_empty() && valid_id(value))
            .map(str::to_owned);
        if body.len() > MAX_BODY_BYTES {
            return Err(invalid());
        }
        let parsed: SearchBody = serde_json::from_slice(body).map_err(|_| invalid())?;
        let query = EvidenceQuery {
            company: company.to_owned(),
            branch,
            metric: parsed.metric,
            from: parsed.from,
            to: parsed.to,
            currency: parsed.currency,
            query: parsed.query,
            limit: parsed.limit,
            cursor: parsed.cursor.map(|cursor| {
                crate::adapters::native_lane_25::domain::EvidenceCursor {
                    event_date: cursor.event_date,
                    id: cursor.id,
                }
            }),
        };
        validate(&query)?;
        let rows = read(reader.as_ref(), &query).await?;
        Ok(rows.into_iter().map(|row| row.value()).collect())
    }

    async fn detail(&self, headers: &HeaderMap, slug: &str, body: &[u8]) -> Response {
        match self.detail_inner(headers, slug, body).await {
            Ok(row) => bounded(serde_json::json!({ "row": row })),
            Err(error) => error_response(error),
        }
    }

    async fn detail_inner(
        &self,
        headers: &HeaderMap,
        slug: &str,
        body: &[u8],
    ) -> Result<Option<serde_json::Value>, BusinessError> {
        let host = unique_header(headers, header::HOST.as_str())?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let reader = self
            .targets
            .get(&(host.to_owned(), slug.to_owned()))
            .ok_or_else(missing)?;
        let company = unique_header(headers, CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?;
        let branch = unique_header(headers, WORKSPACE_BRANCH)?
            .filter(|value| !value.is_empty() && valid_id(value))
            .map(str::to_owned);
        if body.len() > MAX_BODY_BYTES {
            return Err(invalid());
        }
        let parsed: DetailBody = serde_json::from_slice(body).map_err(|_| invalid())?;
        let query = EvidenceDetailQuery {
            company: company.to_owned(),
            branch,
            kind: parsed.kind,
            id: parsed.id,
            from: parsed.from,
            to: parsed.to,
            currency: parsed.currency,
        };
        validate_detail(&query)?;
        if !DETAIL_KINDS.contains(&query.kind.as_str()) {
            return Err(invalid());
        }
        let row = read_detail(reader.as_ref(), &query).await?;
        Ok(row.map(|row| row.value()))
    }

    async fn day_closes(&self, headers: &HeaderMap, slug: &str) -> Response {
        match self.day_closes_inner(headers, slug).await {
            Ok(rows) => bounded(serde_json::json!({ "rows": rows })),
            Err(error) => error_response(error),
        }
    }

    async fn day_closes_inner(
        &self,
        headers: &HeaderMap,
        slug: &str,
    ) -> Result<Vec<serde_json::Value>, BusinessError> {
        let host = unique_header(headers, header::HOST.as_str())?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let reader = self
            .targets
            .get(&(host.to_owned(), slug.to_owned()))
            .ok_or_else(missing)?;
        let company = unique_header(headers, CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?;
        let branch = unique_header(headers, WORKSPACE_BRANCH)?
            .filter(|value| !value.is_empty() && valid_id(value))
            .map(str::to_owned);
        let query = DayClosesQuery {
            company: company.to_owned(),
            branch,
        };
        validate_day_closes(&query)?;
        let rows = read_day_closes(reader.as_ref(), &query).await?;
        Ok(rows.into_iter().map(|row| row.value()).collect())
    }
}

async fn day_closes(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    state.day_closes(&headers, &slug).await
}

/// Build the native read router with a single pool shared by every target.
/// The lead mounts it; no composition edit is needed and no SQL or business rule
/// is added to TypeScript.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgEvidenceReader::new(pool)), targets)
}

/// Router over one injected owner reader, kept for single-reader tests.
pub fn router_with_reader(
    reader: Arc<dyn PosEvidenceReader>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

/// Router over one owner reader per registered Holding target. Composition
/// supplies a distinct `DatabasePool` for each Holding; each entry becomes its
/// own `PgEvidenceReader` and is keyed by `(host, holding_slug)` in the state
/// map. This is the per-holding constructor the lead mounts.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                (
                    target,
                    Arc::new(PgEvidenceReader::new(pool)) as Arc<dyn PosEvidenceReader>,
                )
            })
            .collect(),
    )
}

/// Injection path for the per-target map, used by `router_for_holdings` and by
/// the two-target isolation test. No pool or SQL is created here.
pub fn router_with_readers(
    entries: Vec<(LaneTarget, Arc<dyn PosEvidenceReader>)>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/pos/evidence/search",
            post(search),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/pos/evidence/detail",
            post(detail),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/pos/day-closes",
            post(day_closes),
        )
        .with_state(Arc::new(LaneState::with_readers(entries)?)))
}

async fn detail(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), MAX_BODY_BYTES + 1).await {
        Ok(body) => body,
        Err(_) => return error_response(unavailable()),
    };
    state.detail(&headers, &slug, &body).await
}

async fn search(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), MAX_BODY_BYTES + 1).await {
        Ok(body) => body,
        Err(_) => return error_response(unavailable()),
    };
    state.search(&headers, &slug, &body).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_25::application::EvidenceFuture;
    use crate::adapters::native_lane_25::domain::{EvidenceRow, ROW_KEYS};
    use std::{future::Future, task::{Context, Poll, Waker}};

    fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = Box::pin(future);
        let mut cx = Context::from_waker(Waker::noop());
        loop {
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(value) => return value,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    /// Distinct fake readers let a test observe which Holding's reader served
    /// a request without needing a live pool.
    struct Tagged(&'static str);
    impl PosEvidenceReader for Tagged {
        fn search<'a>(&'a self, _: &'a EvidenceQuery) -> EvidenceFuture<'a, Vec<EvidenceRow>> {
            let id = format!("sale:{}", self.0);
            Box::pin(async move {
                Ok(vec![EvidenceRow {
                    id,
                    source_id: self.0.into(),
                    source_type: "sale".into(),
                    sale_id: self.0.into(),
                    event_date: "2026-01-02".into(),
                    receipt_number: "R1".into(),
                    status: "completed".into(),
                    sale_status: "completed".into(),
                    currency: "USD".into(),
                    method: None,
                    amount_minor: "100".into(),
                    contribution_minor: "100".into(),
                    refunded_minor: "0".into(),
                    revision: "1".into(),
                    sale_version: "1".into(),
                }])
            })
        }
    }

    struct Fake(bool);
    impl PosEvidenceReader for Fake {
        fn search<'a>(&'a self, _: &'a EvidenceQuery) -> EvidenceFuture<'a, Vec<EvidenceRow>> {
            if self.0 {
                return Box::pin(async { Err(unavailable()) });
            }
            Box::pin(async {
                Ok(vec![EvidenceRow {
                    id: "sale:s1".into(),
                    source_id: "s1".into(),
                    source_type: "sale".into(),
                    sale_id: "s1".into(),
                    event_date: "2026-01-02".into(),
                    receipt_number: "R1".into(),
                    status: "completed".into(),
                    sale_status: "completed".into(),
                    currency: "USD".into(),
                    method: None,
                    amount_minor: "100".into(),
                    contribution_minor: "100".into(),
                    refunded_minor: "0".into(),
                    revision: "1".into(),
                    sale_version: "1".into(),
                }])
            })
        }
    }

    fn state(fails: bool) -> LaneState {
        LaneState::new(
            Arc::new(Fake(fails)),
            vec![LaneTarget {
                host: "acme.example.com".into(),
                holding_slug: "acme".into(),
            }],
        )
        .unwrap()
    }

    fn headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "acme.example.com".parse().unwrap());
        headers.insert(CONTEXT_COMPANY, "co".parse().unwrap());
        headers
    }

    fn body() -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "metric":"pos-sales","from":"2026-01-01","to":"2026-01-31",
            "currency":"USD","query":"R1","limit":25,"cursor":null
        }))
        .unwrap()
    }

    #[test]
    fn configuration_rejects_bad_and_duplicate_targets() {
        assert!(LaneState::new(
            Arc::new(Fake(false)),
            vec![LaneTarget { host: String::new(), holding_slug: "acme".into() }],
        )
        .is_err());
        assert!(LaneState::new(Arc::new(Fake(false)), vec![]).is_err());
        assert!(LaneState::new(
            Arc::new(Fake(false)),
            vec![
                LaneTarget { host: "h".into(), holding_slug: "acme".into() },
                LaneTarget { host: "h".into(), holding_slug: "acme".into() },
            ],
        )
        .is_err());
    }

    #[test]
    fn valid_evidence_request_returns_exact_row_shape() {
        let state = state(false);
        let response = block_on(state.search(&headers(), "acme", &body()));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "private, no-store");
        let bytes = block_on(to_bytes(response.into_body(), 1 << 20)).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let rows = value["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].as_object().unwrap().len(), ROW_KEYS.len());
        for key in ROW_KEYS {
            assert!(rows[0].get(key).is_some());
        }
    }

    #[test]
    fn missing_company_and_unknown_target_fail_closed() {
        let state = state(false);
        let mut no_company = headers();
        no_company.remove(CONTEXT_COMPANY);
        let response = block_on(state.search(&no_company, "acme", &body()));
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let response = block_on(state.search(&headers(), "other", &body()));
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn two_holdings_route_to_two_distinct_injected_readers() {
        let entries: Vec<(LaneTarget, Arc<dyn PosEvidenceReader>)> = vec![
            (
                LaneTarget {
                    host: "north.example.com".into(),
                    holding_slug: "north".into(),
                },
                Arc::new(Tagged("north")),
            ),
            (
                LaneTarget {
                    host: "acme.example.com".into(),
                    holding_slug: "acme".into(),
                },
                Arc::new(Tagged("acme")),
            ),
        ];
        // The router constructor uses this same per-target injection path.
        assert!(router_with_readers(entries).is_ok());
        let state = LaneState::with_readers(vec![
            (
                LaneTarget {
                    host: "north.example.com".into(),
                    holding_slug: "north".into(),
                },
                Arc::new(Tagged("north")) as Arc<dyn PosEvidenceReader>,
            ),
            (
                LaneTarget {
                    host: "acme.example.com".into(),
                    holding_slug: "acme".into(),
                },
                Arc::new(Tagged("acme")) as Arc<dyn PosEvidenceReader>,
            ),
        ])
        .unwrap();
        let north = state.reader_for("north.example.com", "north").unwrap();
        let acme = state.reader_for("acme.example.com", "acme").unwrap();
        assert!(!Arc::ptr_eq(north, acme), "each Holding must own its reader/pool");

        let host = |value: &str| {
            let mut headers = headers();
            headers.insert(header::HOST, value.parse().unwrap());
            headers
        };
        let id = |h: &HeaderMap, slug: &str| {
            let response = block_on(state.search(h, slug, &body()));
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = block_on(to_bytes(response.into_body(), 1 << 20)).unwrap();
            let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            value["rows"][0]["id"].as_str().unwrap().to_owned()
        };
        assert_eq!(id(&host("north.example.com"), "north"), "sale:north");
        assert_eq!(id(&host("acme.example.com"), "acme"), "sale:acme");
        // A host/slug pair that is not registered cannot reach either reader.
        assert_eq!(
            block_on(state.search(&host("north.example.com"), "acme", &body())).status(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn invalid_metric_and_reader_failure_fail_closed() {
        let valid = state(false);
        let mut bad = serde_json::to_vec(&serde_json::json!({
            "metric":"other","from":"2026-01-01","to":"2026-01-31",
            "currency":"USD","query":"","limit":25
        }))
        .unwrap();
        assert_eq!(
            block_on(valid.search(&headers(), "acme", &bad)).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        bad.push(b'}');
        assert_eq!(
            block_on(valid.search(&headers(), "acme", &bad)).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let failing = state(true);
        assert_eq!(
            block_on(failing.search(&headers(), "acme", &body())).status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    struct DetailFake(bool);
    impl PosEvidenceReader for DetailFake {
        fn search<'a>(&'a self, _: &'a EvidenceQuery) -> EvidenceFuture<'a, Vec<EvidenceRow>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn detail<'a>(
            &'a self,
            query: &'a EvidenceDetailQuery,
        ) -> EvidenceFuture<'a, Option<crate::adapters::native_lane_25::domain::EvidenceDetailRow>> {
            if self.0 {
                return Box::pin(async { Err(unavailable()) });
            }
            use crate::adapters::native_lane_25::domain::{
                PAYMENT_DETAIL_KEYS, REFUND_DETAIL_KEYS,
            };
            let keys: &[&str] = if query.kind == "refund" {
                &REFUND_DETAIL_KEYS
            } else {
                &PAYMENT_DETAIL_KEYS
            };
            let kind = query.kind.clone();
            Box::pin(async move {
                Ok(Some(crate::adapters::native_lane_25::domain::EvidenceDetailRow {
                    values: keys
                        .iter()
                        .map(|key| {
                            let value = match *key {
                                "source_type" => kind.clone(),
                                "currency" => "USD".to_owned(),
                                "amount_minor" => "100".to_owned(),
                                _ => "v".to_owned(),
                            };
                            ((*key).to_owned(), Some(value))
                        })
                        .collect(),
                }))
            })
        }
    }

    fn detail_state(fails: bool) -> LaneState {
        LaneState::with_readers(vec![(
            LaneTarget {
                host: "acme.example.com".into(),
                holding_slug: "acme".into(),
            },
            Arc::new(DetailFake(fails)) as Arc<dyn PosEvidenceReader>,
        )])
        .unwrap()
    }

    fn detail_body(kind: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "kind": kind, "id": "r1", "from": "2026-01-01",
            "to": "2026-01-31", "currency": "USD"
        }))
        .unwrap()
    }

    #[test]
    fn detail_route_returns_the_exact_row_shape() {
        let state = detail_state(false);
        let response = block_on(state.detail(&headers(), "acme", &detail_body("refund")));
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = block_on(to_bytes(response.into_body(), 1 << 20)).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let row = value["row"].as_object().unwrap();
        assert_eq!(row.len(), crate::adapters::native_lane_25::domain::REFUND_DETAIL_KEYS.len());
        assert_eq!(row["source_type"], "refund");
    }

    #[test]
    fn detail_route_fails_closed_on_bad_scope_kind_and_reader_error() {
        let state = detail_state(false);
        let mut no_company = headers();
        no_company.remove(CONTEXT_COMPANY);
        assert_eq!(
            block_on(state.detail(&no_company, "acme", &detail_body("refund"))).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            block_on(state.detail(&headers(), "other", &detail_body("refund"))).status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            block_on(state.detail(&headers(), "acme", &detail_body("other"))).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let mut malformed = detail_body("refund");
        malformed.push(b'}');
        assert_eq!(
            block_on(state.detail(&headers(), "acme", &malformed)).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let failing = detail_state(true);
        assert_eq!(
            block_on(failing.detail(&headers(), "acme", &detail_body("payment"))).status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    struct DayClosesFake(bool);
    impl PosEvidenceReader for DayClosesFake {
        fn search<'a>(&'a self, _: &'a EvidenceQuery) -> EvidenceFuture<'a, Vec<EvidenceRow>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn day_closes<'a>(&'a self, _: &'a DayClosesQuery) -> EvidenceFuture<'a, Vec<crate::adapters::native_lane_25::domain::DayClosesRow>> {
            use crate::adapters::native_lane_25::domain::DAY_CLOSES_KEYS;
            if self.0 {
                return Box::pin(async { Err(unavailable()) });
            }
            Box::pin(async move {
                Ok(vec![crate::adapters::native_lane_25::domain::DayClosesRow {
                    values: DAY_CLOSES_KEYS
                        .iter()
                        .map(|key| {
                            let value = match *key {
                                "id" => "dc1".to_owned(),
                                "business_date" => "2026-01-31".to_owned(),
                                "currency" => "USD".to_owned(),
                                _ => "v".to_owned(),
                            };
                            ((*key).to_owned(), Some(value))
                        })
                        .collect(),
                }])
            })
        }
    }

    fn day_closes_state(fails: bool) -> LaneState {
        LaneState::with_readers(vec![(
            LaneTarget {
                host: "acme.example.com".into(),
                holding_slug: "acme".into(),
            },
            Arc::new(DayClosesFake(fails)) as Arc<dyn PosEvidenceReader>,
        )])
        .unwrap()
    }

    #[test]
    fn day_closes_route_returns_the_exact_row_shape() {
        let state = day_closes_state(false);
        let response = block_on(state.day_closes(&headers(), "acme"));
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = block_on(to_bytes(response.into_body(), 1 << 20)).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let rows = value["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].as_object().unwrap().len(), crate::adapters::native_lane_25::domain::DAY_CLOSES_KEYS.len());
        assert_eq!(rows[0]["id"], "dc1");
    }

    #[test]
    fn day_closes_route_fails_closed_on_bad_scope_and_reader_error() {
        let state = day_closes_state(false);
        let mut no_company = headers();
        no_company.remove(CONTEXT_COMPANY);
        assert_eq!(
            block_on(state.day_closes(&no_company, "acme")).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            block_on(state.day_closes(&headers(), "other")).status(),
            StatusCode::NOT_FOUND
        );
        let failing = day_closes_state(true);
        assert_eq!(
            block_on(failing.day_closes(&headers(), "acme")).status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
}
