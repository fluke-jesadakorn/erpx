//! Owner-local axum ingress for the native Executive Quality evidence search.
//!
//! Transport facts only: registered Host/Holding target, bounded Company and
//! branch headers, strict JSON envelope and business-error mapping. The pool-
//! backed owner reader owns the SQL, projection and response row.

use crate::application::quality_evidence::{
    QualityEvidenceReader, read_inspection_evidence,
};
use crate::domain::quality_evidence::{decode_cursor, invalid};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Deserialize;
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: &str = "x-erpx-context-company";
const WORKSPACE_BRANCH: &str = "x-erpx-workspace-branch";
const MAX_BODY_BYTES: usize = 16 * 1024;

pub struct QualityEvidenceTarget {
    pub host: String,
    pub holding_slug: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct FiltersBody {
    from: String,
    to: String,
    currency: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchBody {
    metric: String,
    filters: FiltersBody,
    query: String,
    cursor: Option<String>,
    limit: usize,
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn QualityEvidenceReader>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "QUALITY_EVIDENCE_READ_UNAVAILABLE",
        "The native Quality evidence service is unavailable.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "QUALITY_EVIDENCE_CONFIG_INVALID",
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

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
        })
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
    fn with_readers(
        entries: Vec<(QualityEvidenceTarget, Arc<dyn QualityEvidenceReader>)>,
    ) -> Result<Self, BusinessError> {
        let mut configured = BTreeMap::new();
        for (target, reader) in entries {
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
        Ok(Self {
            targets: configured,
        })
    }

    #[cfg(test)]
    fn new(
        reader: Arc<dyn QualityEvidenceReader>,
        targets: Vec<QualityEvidenceTarget>,
    ) -> Result<Self, BusinessError> {
        Self::with_readers(
            targets
                .into_iter()
                .map(|target| (target, reader.clone()))
                .collect(),
        )
    }

    #[cfg(test)]
    fn reader_for(&self, host: &str, slug: &str) -> Option<&Arc<dyn QualityEvidenceReader>> {
        self.targets.get(&(host.to_owned(), slug.to_owned()))
    }

    async fn search_inner(
        &self,
        headers: &HeaderMap,
        slug: &str,
        body: &[u8],
    ) -> Result<serde_json::Value, BusinessError> {
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
        if parsed.metric != crate::domain::quality_evidence::METRIC_INSPECTION_PASS_RATE {
            return Err(invalid());
        }
        let query = crate::domain::quality_evidence::InspectionEvidenceQuery {
            company: company.to_owned(),
            branch,
            from: parsed.filters.from,
            to: parsed.filters.to,
            currency: parsed.filters.currency,
            query: parsed.query,
            limit: parsed.limit,
            cursor: parsed.cursor.as_deref().map(decode_cursor).transpose()?,
        };
        let rows = read_inspection_evidence(reader.as_ref(), &query).await?;
        Ok(serde_json::json!({
            "rows": rows.into_iter().map(|row| row.value()).collect::<Vec<_>>(),
        }))
    }

    async fn search(&self, headers: &HeaderMap, slug: &str, body: &[u8]) -> Response {
        match self.search_inner(headers, slug, body).await {
            Ok(value) => bounded(value),
            Err(error) => error_response(error),
        }
    }
}

/// Build the native read router with one pool shared by every target. The lead
/// mounts it; no composition edit is made by the owner.
pub fn router(
    pool: DatabasePool,
    targets: Vec<QualityEvidenceTarget>,
) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(crate::adapters::quality_evidence::PgQualityEvidenceReader::new(pool)), targets)
}

/// Router over one injected owner reader, kept for single-reader tests.
pub fn router_with_reader(
    reader: Arc<dyn QualityEvidenceReader>,
    targets: Vec<QualityEvidenceTarget>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

/// Router over one owner reader per registered Holding. Composition supplies a
/// distinct `DatabasePool` for each Holding; this is the per-holding mount.
pub fn router_for_holdings(
    targets: Vec<(QualityEvidenceTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                (
                    target,
                    Arc::new(
                        crate::adapters::quality_evidence::PgQualityEvidenceReader::new(pool),
                    ) as Arc<dyn QualityEvidenceReader>,
                )
            })
            .collect(),
    )
}

pub fn router_with_readers(
    entries: Vec<(QualityEvidenceTarget, Arc<dyn QualityEvidenceReader>)>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/quality/evidence/inspections",
            post(search),
        )
        .with_state(Arc::new(LaneState::with_readers(entries)?)))
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
    use crate::application::quality_evidence::QualityEvidenceFuture;
    use crate::domain::quality_evidence::InspectionEvidenceRow;
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

    struct Fake(bool);
    impl QualityEvidenceReader for Fake {
        fn inspection_evidence<'a>(
            &'a self,
            _: &'a crate::domain::quality_evidence::InspectionEvidenceQuery,
        ) -> QualityEvidenceFuture<'a, Vec<InspectionEvidenceRow>> {
            if self.0 {
                return Box::pin(async { Err(unavailable()) });
            }
            Box::pin(async {
                Ok(vec![InspectionEvidenceRow {
                    id: "i1".into(),
                    hold_id: "h1".into(),
                    result: "pass".into(),
                    created_at: 1_767_225_600_000,
                    reviewed_version: 3,
                    product_name: "Rice".into(),
                    warehouse_name: "Main".into(),
                }])
            })
        }
    }

    fn state(fails: bool) -> LaneState {
        LaneState::new(
            Arc::new(Fake(fails)),
            vec![QualityEvidenceTarget {
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
            "metric":"inspection-pass-rate",
            "filters":{"from":"2026-01-01","to":"2026-01-31","currency":"THB"},
            "query":"","cursor":null,"limit":25
        }))
        .unwrap()
    }

    #[test]
    fn configuration_rejects_bad_and_duplicate_targets() {
        assert!(LaneState::new(
            Arc::new(Fake(false)),
            vec![QualityEvidenceTarget {
                host: String::new(),
                holding_slug: "acme".into()
            }],
        )
        .is_err());
        assert!(LaneState::new(Arc::new(Fake(false)), vec![]).is_err());
        assert!(LaneState::new(
            Arc::new(Fake(false)),
            vec![
                QualityEvidenceTarget { host: "h".into(), holding_slug: "acme".into() },
                QualityEvidenceTarget { host: "h".into(), holding_slug: "acme".into() },
            ],
        )
        .is_err());
    }

    #[test]
    fn valid_request_returns_the_exact_seven_field_row() {
        let state = state(false);
        let response = block_on(state.search(&headers(), "acme", &body()));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "private, no-store");
        let bytes = block_on(to_bytes(response.into_body(), 1 << 20)).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let rows = value["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].as_object().unwrap().len(), 7);
        assert_eq!(rows[0]["created_at"], 1_767_225_600_000_u64);
        assert_eq!(rows[0]["reviewed_version"], 3);
    }

    #[test]
    fn missing_scope_unknown_target_and_bad_metric_fail_closed() {
        let state = state(false);
        let mut no_company = headers();
        no_company.remove(CONTEXT_COMPANY);
        assert_eq!(
            block_on(state.search(&no_company, "acme", &body())).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            block_on(state.search(&headers(), "other", &body())).status(),
            StatusCode::NOT_FOUND
        );
        let mut bad = serde_json::to_vec(&serde_json::json!({
            "metric":"quality-holds",
            "filters":{"from":"2026-01-01","to":"2026-01-31","currency":"THB"},
            "query":"","cursor":null,"limit":25
        }))
        .unwrap();
        assert_eq!(
            block_on(state.search(&headers(), "acme", &bad)).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        bad.push(b'}');
        assert_eq!(
            block_on(state.search(&headers(), "acme", &bad)).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[test]
    fn encoded_cursor_is_decoded_by_the_owner() {
        let state = state(false);
        let payload = serde_json::to_vec(&serde_json::json!({
            "metric":"inspection-pass-rate",
            "filters":{"from":"2026-01-01","to":"2026-01-31","currency":"THB"},
            "query":"",
            "cursor":"eyJjcmVhdGVkQXQiOjE3NjcyMjU2MDAwMDAsImlkIjoiaTEifQ",
            "limit":25
        }))
        .unwrap();
        assert_eq!(
            block_on(state.search(&headers(), "acme", &payload)).status(),
            StatusCode::OK
        );
    }

    #[test]
    fn two_holdings_route_to_two_distinct_injected_readers() {
        let entries: Vec<(QualityEvidenceTarget, Arc<dyn QualityEvidenceReader>)> = vec![
            (
                QualityEvidenceTarget {
                    host: "north.example.com".into(),
                    holding_slug: "north".into(),
                },
                Arc::new(Fake(false)),
            ),
            (
                QualityEvidenceTarget {
                    host: "acme.example.com".into(),
                    holding_slug: "acme".into(),
                },
                Arc::new(Fake(true)),
            ),
        ];
        let state = LaneState::with_readers(entries).unwrap();
        assert!(!Arc::ptr_eq(
            state.reader_for("north.example.com", "north").unwrap(),
            state.reader_for("acme.example.com", "acme").unwrap()
        ));
        let mut north = headers();
        north.insert(header::HOST, "north.example.com".parse().unwrap());
        assert_eq!(
            block_on(state.search(&north, "north", &body())).status(),
            StatusCode::OK
        );
        assert_eq!(
            block_on(state.search(&headers(), "acme", &body())).status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert!(router_with_readers(vec![(
            QualityEvidenceTarget {
                host: "acme.example.com".into(),
                holding_slug: "acme".into()
            },
            Arc::new(Fake(false)) as Arc<dyn QualityEvidenceReader>,
        )])
        .is_ok());
    }
}
