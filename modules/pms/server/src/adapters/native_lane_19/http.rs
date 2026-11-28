//! Owner-local axum read router for lane 19. The target table enforces exact
//! Holding host + slug isolation; every scope value arrives once as a bounded
//! header. The router owns no SQL and falls closed on any configuration error.

use std::{collections::BTreeMap, sync::Arc};

use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router,
    State, StatusCode, header, routing::get,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::json;

use super::application::{GuestAccountEvidenceRead, read_guest_account_evidence};
use super::domain::{EvidenceRequest, invalid, not_found, valid_branch, valid_company, valid_id};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_BRANCH: HeaderName = HeaderName::from_static("x-erpx-context-branch");
const CONTEXT_RECORD: HeaderName = HeaderName::from_static("x-erpx-context-record");

/// Exact registered Holding target for the mounted owner router.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct Lane19State {
    targets: BTreeMap<(String, String), Arc<dyn GuestAccountEvidenceRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PMS_LANE_19_CONFIG_INVALID",
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

/// Validate exact registered targets and index them by `(host, holding_slug)`.
/// Every constructor below funnels through this one check so isolation and
/// duplicate rejection cannot drift between them.
fn configured_targets(
    entries: impl IntoIterator<Item = (LaneTarget, Arc<dyn GuestAccountEvidenceRead>)>,
) -> Result<BTreeMap<(String, String), Arc<dyn GuestAccountEvidenceRead>>, BusinessError> {
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
    Ok(configured)
}

fn read_router_from(
    configured: BTreeMap<(String, String), Arc<dyn GuestAccountEvidenceRead>>,
) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/pms/guest-account-finance-evidence",
            get(evidence),
        )
        .with_state(Arc::new(Lane19State {
            targets: configured,
        }))
}

/// Reader-injection path for tests: one distinct reader may be supplied per
/// Holding, so routing can be proven without a database.
pub fn router_with_readers(
    entries: Vec<(LaneTarget, Arc<dyn GuestAccountEvidenceRead>)>,
) -> Result<Router, BusinessError> {
    Ok(read_router_from(configured_targets(entries)?))
}

/// Build the owner read router over one shared injected reader.
pub fn router_with_reader(
    reader: Arc<dyn GuestAccountEvidenceRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

/// Compatible alias retained for the existing unit tests.
pub fn read_router(
    reader: Arc<dyn GuestAccountEvidenceRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    router_with_reader(reader, targets)
}

/// Mount entry point with a single shared Pg pool.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_with_reader(super::postgres::reader(pool), targets)
}

/// Mount entry point with one Pg reader per Holding pool. `pools[i + 1]` is the
/// i-th registered Holding's database, so no Holding can reach another's data.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                let reader: Arc<dyn GuestAccountEvidenceRead> =
                    Arc::new(super::postgres::PgGuestAccountEvidenceRead::new(pool));
                (target, reader)
            })
            .collect(),
    )
}

async fn evidence(
    State(state): State<Arc<Lane19State>>,
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
            .filter(|value| valid_company(value))
            .ok_or_else(invalid)?
            .to_owned();
        let branch = match unique_header(request.headers(), CONTEXT_BRANCH)? {
            Some("") | None => None,
            Some(value) if valid_branch(value) => Some(value.to_owned()),
            Some(_) => Err(invalid())?,
        };
        let id = unique_header(request.headers(), CONTEXT_RECORD)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?
            .to_owned();
        let request = EvidenceRequest {
            company,
            branch,
            id,
        };
        let value = read_guest_account_evidence(reader.as_ref(), request)
            .await?
            .ok_or_else(not_found)?;
        Ok::<_, BusinessError>(json!({"evidence": value}))
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
    use crate::adapters::native_lane_19::domain::GuestAccountEvidence;
    use erpx_platform_runtime_server::public::http_support::body::{self, Body};
    use erpx_platform_shared_server::public::RecordId;
    use serde_json::json;

    struct FakeReader;

    impl GuestAccountEvidenceRead for FakeReader {
        fn evidence<'a>(
            &'a self,
            _request: &'a EvidenceRequest,
        ) -> super::super::application::Lane19ReadFuture<'a, Option<GuestAccountEvidence>> {
            Box::pin(async { Ok(None) })
        }
    }

    struct IdReader(&'static str);

    impl GuestAccountEvidenceRead for IdReader {
        fn evidence<'a>(
            &'a self,
            _request: &'a EvidenceRequest,
        ) -> super::super::application::Lane19ReadFuture<'a, Option<GuestAccountEvidence>> {
            let id = self.0.to_owned();
            Box::pin(async move {
                Ok(Some(GuestAccountEvidence {
                    id,
                    property_id: "prop-1".into(),
                    hotel: "Riverside".into(),
                    business_date: "2026-11-25".into(),
                    currency: "THB".into(),
                    account: serde_json::Value::Null,
                    entries: vec![],
                }))
            })
        }
    }

    fn target() -> LaneTarget {
        LaneTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
        }
    }

    #[test]
    fn rejects_invalid_target_configuration() {
        assert_eq!(
            read_router(Arc::new(FakeReader), vec![]).unwrap_err().code,
            "PMS_LANE_19_CONFIG_INVALID"
        );
        assert_eq!(
            read_router(
                Arc::new(FakeReader),
                vec![LaneTarget {
                    host: "acme.jesadakorn.com".into(),
                    holding_slug: "Acme".into(),
                }]
            )
            .unwrap_err()
            .code,
            "PMS_LANE_19_CONFIG_INVALID"
        );
        assert_eq!(
            read_router(Arc::new(FakeReader), vec![target(), target()])
                .unwrap_err()
                .code,
            "PMS_LANE_19_CONFIG_INVALID"
        );
    }

    #[test]
    fn accepts_exact_distinct_targets() {
        assert!(read_router(Arc::new(FakeReader), vec![target()]).is_ok());
        assert!(router_with_reader(Arc::new(FakeReader), vec![target()]).is_ok());
    }

    /// Proves the same map used by `router_for_holdings` holds one distinct
    /// reader per Holding and that the handler dispatches to the right one.
    #[test]
    fn two_holdings_route_to_two_distinct_injected_readers() {
        let acme = LaneTarget {
            host: "acme.example.com".into(),
            holding_slug: "acme".into(),
        };
        let northstar = LaneTarget {
            host: "northstar.example.com".into(),
            holding_slug: "northstar".into(),
        };
        let acme_reader: Arc<dyn GuestAccountEvidenceRead> = Arc::new(IdReader("acct-acme"));
        let northstar_reader: Arc<dyn GuestAccountEvidenceRead> = Arc::new(IdReader("acct-northstar"));
        let entries = vec![
            (acme.clone(), acme_reader.clone()),
            (northstar.clone(), northstar_reader.clone()),
        ];

        let configured = configured_targets(entries.clone()).expect("configured");
        assert_eq!(configured.len(), 2);
        let acme_key = ("acme.example.com".to_owned(), "acme".to_owned());
        let northstar_key = ("northstar.example.com".to_owned(), "northstar".to_owned());
        assert!(!Arc::ptr_eq(
            &configured[&acme_key],
            &configured[&northstar_key]
        ));
        assert!(router_with_readers(entries).is_ok());

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        for (host, slug, expected) in [
            ("acme.example.com", "acme", "acct-acme"),
            ("northstar.example.com", "northstar", "acct-northstar"),
        ] {
            let state = Arc::new(Lane19State {
                targets: configured.clone(),
            });
            let request = Request::builder()
                .header(header::HOST, host)
                .header(CONTEXT_COMPANY, "company-1")
                .header(CONTEXT_RECORD, "acct-1")
                .body(Body::empty())
                .expect("request");
            let response = runtime.block_on(evidence(State(state), Path(slug.to_owned()), request));
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = runtime
                .block_on(body::to_bytes(response.into_body(), 1024 * 1024))
                .expect("body");
            let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
            assert_eq!(value["evidence"]["id"], expected);
        }
    }

    #[test]
    fn public_surface_exposes_the_per_holding_constructor() {
        let _constructor = crate::public::native_lane_19::router_for_holdings;
        let _target = crate::public::native_lane_19::LaneTarget {
            host: "acme.example.com".into(),
            holding_slug: "acme".into(),
        };
    }

    #[test]
    fn validates_identifiers_and_slugs() {
        assert!(valid_slug("acme-1"));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug(""));
        assert!(valid_id("acct-1"));
        assert!(RecordId::new("acct-1").is_ok());
        assert!(!valid_id("acct/1"));
    }

    #[test]
    fn maps_failure_kinds_to_status() {
        assert_eq!(status(FailureKind::InvalidInput), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(status(FailureKind::Unauthenticated), StatusCode::UNAUTHORIZED);
        assert_eq!(status(FailureKind::Forbidden), StatusCode::FORBIDDEN);
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(status(FailureKind::Conflict), StatusCode::CONFLICT);
        assert_eq!(status(FailureKind::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn success_body_is_bounded_and_pin_shaped() {
        let response = bounded(json!({"evidence":{"id":"acct-1"}}));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok()),
            Some("private, no-store")
        );
        assert_eq!(
            response
                .headers()
                .get(header::REFERRER_POLICY)
                .and_then(|value| value.to_str().ok()),
            Some("no-referrer")
        );
        assert_eq!(
            response
                .headers()
                .get(header::X_CONTENT_TYPE_OPTIONS)
                .and_then(|value| value.to_str().ok()),
            Some("nosniff")
        );
        assert_eq!(
            error_response(invalid()).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}
