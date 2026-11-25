//! Owner-local axum read router for the Learning evidence-detail family.
//!
//! Transport facts only: an exact registered Host/Holding target, one bounded
//! path, single-valued Company/branch/department headers and one strict JSON
//! request. The owner reader executes the SQL and projects the row; no SQL,
//! predicate or bind crosses the wire from the caller. Failures map to the
//! shared error envelope and a missing runtime fails closed with 503.

use super::application::{EvidenceReader, EvidenceService};
use super::domain::{EvidenceRow, LaneTarget, configuration_invalid, invalid, missing};
use super::postgres::PgEvidenceReader;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_BRANCH: HeaderName = HeaderName::from_static("x-erpx-context-branch");
const CONTEXT_DEPARTMENT: HeaderName = HeaderName::from_static("x-erpx-context-department");
const BODY_LIMIT: usize = 16 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadRequest {
    kind: String,
    id: String,
    #[serde(default)]
    from: Option<i64>,
    #[serde(default)]
    to_exclusive: Option<i64>,
}

impl From<ReadRequest> for super::domain::EvidenceQuery {
    fn from(value: ReadRequest) -> Self {
        Self {
            kind: value.kind,
            id: value.id,
            from: value.from,
            to_exclusive: value.to_exclusive,
        }
    }
}

/// Each registered Holding owns its own reader (and therefore its own pool).
/// The map is keyed by the exact `(Host, holding_slug)` pair from the request.
struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn EvidenceReader>>,
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
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn valid_scope(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

/// Validate distinct Holding targets and bind each to its reader. This is the
/// single place that fills the state map; every constructor below funnels
/// through it so a Holding can never silently use another Holding's reader.
fn configure_readers(
    targets: Vec<(LaneTarget, Arc<dyn EvidenceReader>)>,
) -> Result<BTreeMap<(String, String), Arc<dyn EvidenceReader>>, BusinessError> {
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
    Ok(configured)
}

fn into_router(
    targets: BTreeMap<(String, String), Arc<dyn EvidenceReader>>,
) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/learning/evidence-detail",
            post(read),
        )
        .with_state(Arc::new(LaneState { targets }))
}

/// Reader-injection constructor. Tests supply one reader per target and can
/// prove the map routes each Holding to its own instance without any database.
pub fn router_with_reader(
    targets: Vec<(LaneTarget, Arc<dyn EvidenceReader>)>,
) -> Result<Router, BusinessError> {
    Ok(into_router(configure_readers(targets)?))
}

/// Per-Holding constructor. One `PgEvidenceReader` (and therefore one pool) is
/// built for every `(LaneTarget, DatabasePool)` pair and inserted under that
/// target's `(host, holding_slug)` key; no Holding shares another's pool.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let readers = targets
        .into_iter()
        .map(|(target, pool)| {
            let reader: Arc<dyn EvidenceReader> = Arc::new(PgEvidenceReader::new(pool));
            (target, reader)
        })
        .collect();
    router_with_reader(readers)
}

/// Single-pool compatibility constructor. The lead mounts the router; no other
/// module is edited here. This shares one reader across all targets and is kept
/// for callers with a single Holding database.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let reader: Arc<dyn EvidenceReader> = Arc::new(PgEvidenceReader::new(pool));
    router_with_reader(targets.into_iter().map(|target| (target, reader.clone())).collect())
}

async fn read(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?
            .clone();
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_scope(value))
            .ok_or_else(invalid)?
            .to_owned();
        let branch = unique_header(request.headers(), CONTEXT_BRANCH)?
            .filter(|value| valid_scope(value))
            .map(str::to_owned);
        let department = unique_header(request.headers(), CONTEXT_DEPARTMENT)?
            .filter(|value| valid_scope(value))
            .map(str::to_owned);

        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| invalid())?;
        let payload: ReadRequest = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        let query: super::domain::EvidenceQuery = payload.into();
        let row: Option<EvidenceRow> = EvidenceService::new(reader)
            .read(&query, &company, branch.as_deref(), department.as_deref())
            .await?;
        Ok::<_, BusinessError>(serde_json::json!({"row": row}))
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
    use crate::adapters::native_lane_31::application::EvidenceFuture;
    use crate::adapters::native_lane_31::domain::{EvidenceKind, EvidenceQuery};

    /// Injectable reader that tags every projected row with its own identity,
    /// so a test can prove which Holding's reader answered a read.
    struct TaggedReader(&'static str);
    impl EvidenceReader for TaggedReader {
        fn read<'a>(
            &'a self,
            kind: EvidenceKind,
            _query: &'a EvidenceQuery,
            company: &'a str,
            _branch: Option<&'a str>,
            _department: Option<&'a str>,
        ) -> EvidenceFuture<'a, Option<EvidenceRow>> {
            let tag = self.0;
            let company = company.to_owned();
            Box::pin(async move {
                Ok(Some(EvidenceRow {
                    id: format!("{tag}:{company}"),
                    assignment_id: None,
                    event_version: None,
                    version: None,
                    status: kind.as_str().to_owned(),
                    created_at: None,
                    due_date: None,
                    updated_at: None,
                    course_title: tag.to_owned(),
                    required_for: None,
                }))
            })
        }
    }

    fn injected(host: &str, slug: &str, tag: &'static str) -> (LaneTarget, Arc<dyn EvidenceReader>) {
        (
            LaneTarget { host: host.to_owned(), holding_slug: slug.to_owned() },
            Arc::new(TaggedReader(tag)) as Arc<dyn EvidenceReader>,
        )
    }

    #[test]
    fn rejects_empty_or_duplicate_targets() {
        assert_eq!(configure_readers(vec![]).err(), Some(configuration_invalid()));
        let duplicate = vec![
            injected("acme.jesadakorn.com", "acme", "a"),
            injected("acme.jesadakorn.com", "acme", "b"),
        ];
        assert_eq!(configure_readers(duplicate).err(), Some(configuration_invalid()));
        let bad_slug = vec![injected("acme.jesadakorn.com", "ACME", "a")];
        assert_eq!(configure_readers(bad_slug).err(), Some(configuration_invalid()));
        let ok = configure_readers(vec![injected("acme.jesadakorn.com", "acme", "a")]).unwrap();
        assert_eq!(ok.len(), 1);
    }

    #[tokio::test]
    async fn two_targets_route_to_two_distinct_readers() {
        let first = injected("acme.jesadakorn.com", "acme", "first");
        let second = injected("northstar.jesadakorn.com", "northstar", "second");
        let map = configure_readers(vec![first, second]).unwrap();
        assert_eq!(map.len(), 2);

        let acme = &map[&("acme.jesadakorn.com".to_owned(), "acme".to_owned())];
        let northstar = &map[&("northstar.jesadakorn.com".to_owned(), "northstar".to_owned())];
        assert!(!Arc::ptr_eq(acme, northstar));

        let query = EvidenceQuery {
            kind: "overdue".to_owned(),
            id: "event-1".to_owned(),
            from: None,
            to_exclusive: None,
        };
        // The map key determines which injected reader answers the read, and
        // each reader carries its own pool identity in real constructors.
        let acme_row = EvidenceService::new(acme.clone())
            .read(&query, "company-1", None, None)
            .await
            .unwrap()
            .unwrap();
        let northstar_row = EvidenceService::new(northstar.clone())
            .read(&query, "company-2", None, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(acme_row.course_title, "first");
        assert_eq!(northstar_row.course_title, "second");
        assert_eq!(acme_row.id, "first:company-1");
        assert_eq!(northstar_row.id, "second:company-2");

        // The injection constructor builds the same per-target map into a router.
        assert!(router_with_reader(vec![
            injected("acme.jesadakorn.com", "acme", "first"),
            injected("northstar.jesadakorn.com", "northstar", "second"),
        ])
        .is_ok());
    }

    #[test]
    fn valid_slug_and_scope_are_bounded() {
        assert!(valid_slug("acme"));
        assert!(valid_slug("north-star"));
        assert!(!valid_slug("ACME"));
        assert!(!valid_slug(""));
        assert!(valid_scope("co-1"));
        assert!(valid_scope("branch:west"));
        assert!(!valid_scope(""));
        assert!(!valid_scope(&"x".repeat(201)));
    }
}
