//! Owner-local axum ingress for the lane 1 expense posting projection reads.
//!
//! Follows `server-commerce/adapters/quality_product_read.rs`: exact registered
//! Holding targets, a single `Host` header, a routing-scoped Company header and
//! an explicit error envelope. The router never accepts SQL or a caller-chosen
//! Company; the SQL is owned in `postgres` and executed by the owner pool.

use super::application::{ExpenseProjectionReader, ExpenseProjectionService};
use super::domain::{CompanyScope, ProjectionKind, valid_identifier};
use super::postgres::PgExpenseProjectionReader;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, json, post,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const MAX_BODY: usize = 16 * 1024;

/// One owner reader instance. A trait object lets the state map hold a distinct
/// Pg-backed reader for each registered Holding while keeping a single router.
type LaneReader = Arc<dyn ExpenseProjectionReader>;

/// One exact registered Holding/Company route. Composition owns the list; a
/// duplicate or malformed target is a configuration error, never a fallback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
    pub company_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectionRequest {
    kind: String,
    #[serde(default)]
    journal_id: Option<String>,
}

/// The Company authority and the owner reader bound to one `(host, slug)`.
struct LaneBinding {
    company_id: String,
    reader: LaneReader,
}

struct LaneState {
    targets: BTreeMap<(String, String), LaneBinding>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "LANE_01_READ_REQUEST_INVALID",
        "Use one bounded Finance lane 1 read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "LANE_01_TARGETS_INVALID",
        "Use exact distinct registered Holding Company targets.",
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
    RecordId::new(value).is_ok() && valid_identifier(value)
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
        Json(json!({"error":{"code":error.code,"message":error.message}})),
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

/// Build the state map with one reader per Holding. Each pair is keyed by the
/// exact `(host, holding_slug)` identity; no reader is shared across Holdings.
fn state_with_readers(
    entries: Vec<(LaneTarget, LaneReader)>,
) -> Result<Arc<LaneState>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in entries {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_id(&target.company_id)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    LaneBinding {
                        company_id: target.company_id,
                        reader,
                    },
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Arc::new(LaneState {
        targets: configured,
    }))
}

/// Shared-reader variant retained for single-pool callers and tests.
fn state(targets: Vec<LaneTarget>, reader: LaneReader) -> Result<Arc<LaneState>, BusinessError> {
    state_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

fn mount(lane: Arc<LaneState>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/finance/expense-posting-projection",
            post(project),
        )
        .with_state(lane)
}

/// Single-pool owner-local router. The lead mounts it; no composition, ledger
/// or shared module is edited here.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgExpenseProjectionReader::new(pool)), targets)
}

/// Per-Holding mount: composition pairs each exact registered target with that
/// Holding's own pool, so each reader executes against its own database.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let entries = targets
        .into_iter()
        .map(|(target, pool)| {
            let reader: LaneReader = Arc::new(PgExpenseProjectionReader::new(pool));
            (target, reader)
        })
        .collect();
    Ok(mount(state_with_readers(entries)?))
}

/// Injectable reader variant retained for the owner's own tests.
pub fn router_with_reader(
    reader: LaneReader,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    Ok(mount(state(targets, reader)?))
}

async fn project(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?
            .to_owned();
        let context = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|company| valid_id(company))
            .ok_or_else(invalid)?
            .to_owned();
        let binding = state
            .targets
            .get(&(host, slug.clone()))
            .ok_or_else(missing)?;
        // The routing target is the Company authority. A header Company that
        // differs cannot query another Company's Finance data, and the bound
        // reader is the one created from that Holding's own pool.
        if context != binding.company_id {
            return Err(invalid());
        }
        let bytes = to_bytes(request.into_body(), MAX_BODY)
            .await
            .map_err(|_| invalid())?;
        let body: ProjectionRequest = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        let kind = ProjectionKind::parse(&body.kind)?;
        if kind.needs_journal() != body.journal_id.is_some() {
            return Err(invalid());
        }
        let scope = CompanyScope {
            company_id: context,
        };
        let service = ExpenseProjectionService::new(Arc::clone(&binding.reader));
        let projection = service
            .read(&scope, kind, body.journal_id.as_deref())
            .await?;
        Ok::<_, BusinessError>(json!({
            "data": {"kind": kind.as_str(), "rows": projection.into_rows()}
        }))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::super::application::LaneReadFuture;
    use super::super::domain::{JournalLineRow, PostingAccountRow};
    use super::*;

    #[derive(Default)]
    struct Probe;

    impl ExpenseProjectionReader for Probe {
        fn journal_lines<'a>(
            &'a self,
            _scope: &'a CompanyScope,
            _journal_id: &'a str,
        ) -> LaneReadFuture<'a, Vec<JournalLineRow>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn posting_accounts<'a>(
            &'a self,
            _scope: &'a CompanyScope,
        ) -> LaneReadFuture<'a, Vec<PostingAccountRow>> {
            Box::pin(async { Ok(Vec::new()) })
        }
    }

    /// Mirrors `router_for_holdings` with injected readers instead of pools:
    /// two registered targets must resolve to two distinct reader instances in
    /// the state map keyed by their own `(host, holding_slug)`.
    #[test]
    fn two_targets_route_to_two_distinct_readers() {
        let acme: LaneReader = Arc::new(Probe);
        let northstar: LaneReader = Arc::new(Probe);
        let lane = state_with_readers(vec![
            (
                LaneTarget {
                    host: "acme.example.test".into(),
                    holding_slug: "acme".into(),
                    company_id: "company-1".into(),
                },
                acme.clone(),
            ),
            (
                LaneTarget {
                    host: "northstar.example.test".into(),
                    holding_slug: "northstar".into(),
                    company_id: "company-2".into(),
                },
                northstar.clone(),
            ),
        ])
        .expect("two-target state");

        assert_eq!(lane.targets.len(), 2);
        let bound_acme = &lane.targets[&("acme.example.test".to_owned(), "acme".to_owned())];
        let bound_northstar =
            &lane.targets[&("northstar.example.test".to_owned(), "northstar".to_owned())];
        assert_eq!(bound_acme.company_id, "company-1");
        assert_eq!(bound_northstar.company_id, "company-2");
        assert!(Arc::ptr_eq(&bound_acme.reader, &acme));
        assert!(Arc::ptr_eq(&bound_northstar.reader, &northstar));
        assert!(!Arc::ptr_eq(&bound_acme.reader, &bound_northstar.reader));
        assert!(router_for_holdings(Vec::new()).is_err());
    }

    #[test]
    fn targets_are_exact_and_distinct() {
        let good = LaneTarget {
            host: "acme.example.com".into(),
            holding_slug: "acme".into(),
            company_id: "company-1".into(),
        };
        assert!(valid_slug(&good.holding_slug));
        assert!(valid_id(&good.company_id));
        assert!(!valid_slug("Acme"));
        assert!(!valid_id("company/1"));
    }

    #[test]
    fn error_mapping_fails_closed() {
        let unavailable = BusinessError::new(
            FailureKind::Unavailable,
            "LANE_01_NATIVE_READ_UNAVAILABLE",
            "The native Finance lane 1 read is unavailable.",
        );
        assert_eq!(
            error_response(invalid()).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(error_response(missing()).status(), StatusCode::NOT_FOUND);
        assert_eq!(
            error_response(unavailable).status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[test]
    fn duplicate_header_is_rejected() {
        let mut headers = HeaderMap::new();
        headers.append(header::HOST, HeaderValue::from_static("acme.example.com"));
        headers.append(header::HOST, HeaderValue::from_static("evil.example.com"));
        assert_eq!(
            unique_header(&headers, header::HOST).unwrap_err().code,
            "LANE_01_READ_REQUEST_INVALID"
        );
    }

    #[test]
    fn request_shape_is_strict() {
        let good = br#"{"kind":"journal-lines","journal_id":"journal-1"}"#;
        let parsed: ProjectionRequest = serde_json::from_slice(good).expect("bounded request");
        assert_eq!(parsed.kind, "journal-lines");
        assert_eq!(parsed.journal_id.as_deref(), Some("journal-1"));

        // `deny_unknown_fields` keeps caller SQL out of the request object.
        let injected = br#"{"kind":"journal-lines","journal_id":"journal-1","sql":"SELECT 1"}"#;
        assert!(serde_json::from_slice::<ProjectionRequest>(injected).is_err());
        let accounts = br#"{"kind":"posting-accounts"}"#;
        let parsed: ProjectionRequest = serde_json::from_slice(accounts).expect("bounded request");
        assert!(parsed.journal_id.is_none());
        assert_eq!(
            ProjectionKind::parse(&parsed.kind),
            Ok(ProjectionKind::PostingAccounts)
        );
    }
}
