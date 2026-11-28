//! Owner-local axum router for the native Legal runtime-health read.
//!
//! The exact registered `(host, holding_slug)` pair, the current Company and
//! the `legal.manage` authority arrive from composition as private facts. The
//! scheduled-feature predicate is a code-owned constructor argument, never a
//! request header. The owner executes both real statements and emits the exact
//! `{data: ...}` envelope with the same private Cache-Control response. No
//! TypeScript business fallback exists for this operation.

use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use erpx_platform_runtime_server::public::{
    http_support::{
        HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router,
        State, StatusCode, get, header, serde_json,
    },
    postgres::DatabasePool,
};
use erpx_platform_shared_server::public::{BusinessError, Clock, FailureKind, TimestampMillis};

use super::{
    application::{RuntimeHealthAuthority, RuntimeHealthSource, read_runtime_health},
    domain::{RuntimeHealthTarget, valid_feature_predicate, valid_slug},
    postgres::PgRuntimeHealthSource,
};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const AUTHORITY_MANAGE: HeaderName = HeaderName::from_static("x-erpx-legal-runtime-manage");

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn RuntimeHealthSource>>,
    feature_predicate: Arc<str>,
}

struct WallClock;

impl Clock for WallClock {
    fn now(&self) -> TimestampMillis {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or(0);
        TimestampMillis::new(millis)
    }
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_LEGAL_RUNTIME_HEALTH_UNAVAILABLE",
        "The native Legal runtime-health service is unavailable.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_LEGAL_RUNTIME_HEALTH_REQUEST_INVALID",
        "Use one bounded Legal runtime-health read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_LEGAL_RUNTIME_HEALTH_CONFIG_INVALID",
        "Use exact distinct registered Holding targets and one code-owned feature predicate.",
    )
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

fn scope(headers: &HeaderMap) -> Result<Option<&str>, BusinessError> {
    match unique_header(headers, CONTEXT_COMPANY)? {
        None | Some("") => Ok(None),
        Some(value) => Ok(Some(value)),
    }
}

fn authority(headers: &HeaderMap) -> Result<RuntimeHealthAuthority, BusinessError> {
    let flag = match unique_header(headers, AUTHORITY_MANAGE)? {
        Some("true") => true,
        Some("false") => false,
        _ => return Err(invalid()),
    };
    Ok(RuntimeHealthAuthority { can_manage: flag })
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

fn private_json(value: &serde_json::Value) -> Response {
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

/// Shared target configuration. Every public constructor funnels through this
/// map, so the `(host, holding_slug)` key, exact-target validation and the
/// code-owned predicate have one owner.
fn configure(
    targets: Vec<(RuntimeHealthTarget, Arc<dyn RuntimeHealthSource>)>,
    feature_predicate: &str,
) -> Result<LaneState, BusinessError> {
    if !valid_feature_predicate(feature_predicate) {
        return Err(configuration_invalid());
    }
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
    Ok(LaneState {
        targets: configured,
        feature_predicate: Arc::from(feature_predicate),
    })
}

fn into_router(state: LaneState) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/legal/runtime-health",
            get(current),
        )
        .with_state(Arc::new(state))
}

/// Build the native read router with one pool shared by all registered targets.
pub fn router(
    pool: DatabasePool,
    targets: Vec<RuntimeHealthTarget>,
    feature_predicate: impl AsRef<str>,
) -> Result<Router, BusinessError> {
    let reader: Arc<dyn RuntimeHealthSource> = Arc::new(PgRuntimeHealthSource::new(pool));
    let targets = targets.into_iter().map(|target| (target, reader.clone())).collect();
    Ok(into_router(configure(targets, feature_predicate.as_ref())?))
}

/// Per-Holding constructor. Each `(RuntimeHealthTarget, DatabasePool)` pair gets
/// its own reader, so every Holding reads only from its own database pool.
pub fn router_for_holdings(
    targets: Vec<(RuntimeHealthTarget, DatabasePool)>,
    feature_predicate: impl AsRef<str>,
) -> Result<Router, BusinessError> {
    let readers = targets
        .into_iter()
        .map(|(target, pool)| {
            let reader: Arc<dyn RuntimeHealthSource> = Arc::new(PgRuntimeHealthSource::new(pool));
            (target, reader)
        })
        .collect();
    Ok(into_router(configure(readers, feature_predicate.as_ref())?))
}

/// Reader-injection constructor used by owner tests.
pub fn router_with_source(
    source: Arc<dyn RuntimeHealthSource>,
    targets: Vec<RuntimeHealthTarget>,
    feature_predicate: impl AsRef<str>,
) -> Result<Router, BusinessError> {
    let targets = targets.into_iter().map(|target| (target, source.clone())).collect();
    Ok(into_router(configure(targets, feature_predicate.as_ref())?))
}

async fn current(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let source = state
            .targets
            .get(&(host.to_owned(), slug))
            .ok_or_else(missing)?;
        let company = scope(request.headers())?
            .filter(|value| !value.is_empty())
            .ok_or_else(invalid)?;
        let authority = authority(request.headers())?;
        let read = read_runtime_health(
            source.as_ref(),
            authority,
            company,
            state.feature_predicate.as_ref(),
            &WallClock,
        )
        .await?;
        let body = serde_json::to_value(read).map_err(|_| unavailable())?;
        Ok::<_, BusinessError>(serde_json::json!({"data": body}))
    }
    .await;
    match result {
        Ok(body) => private_json(&body),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::adapters::native_r3_runtime_health::application::RuntimeHealthFuture;
    use crate::adapters::native_r3_runtime_health::domain::RuntimeHealthTaskRow;
    use erpx_platform_runtime_server::public::http_support::body::Body;

    struct TaggedSource {
        tag: &'static str,
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl RuntimeHealthSource for TaggedSource {
        fn tasks<'a>(&'a self) -> RuntimeHealthFuture<'a, Vec<RuntimeHealthTaskRow>> {
            let seen = self.seen.clone();
            let tag = self.tag;
            Box::pin(async move {
                seen.lock().unwrap().push(format!("{tag}:tasks"));
                Ok(vec![RuntimeHealthTaskRow {
                    task: "legal_worker".to_owned(),
                    heartbeat_at: i64::MAX,
                    last_success_at: None,
                    failure_count: 0,
                    last_error: None,
                }])
            })
        }
        fn company_ready<'a>(
            &'a self,
            company: &'a str,
            predicate: &'a str,
        ) -> RuntimeHealthFuture<'a, bool> {
            let seen = self.seen.clone();
            let tag = self.tag;
            let company = company.to_owned();
            let predicate = predicate.to_owned();
            Box::pin(async move {
                seen.lock()
                    .unwrap()
                    .push(format!("{tag}:ready:{company}:{predicate}"));
                Ok(true)
            })
        }
    }

    fn request(host: &str, company: &str, manage: &str) -> Request {
        Request::builder()
            .header(header::HOST, host)
            .header(CONTEXT_COMPANY, company)
            .header(AUTHORITY_MANAGE, manage)
            .body(Body::empty())
            .expect("request")
    }

    fn target(host: &str, slug: &str) -> RuntimeHealthTarget {
        RuntimeHealthTarget {
            host: host.to_owned(),
            holding_slug: slug.to_owned(),
        }
    }

    #[test]
    fn error_mapping_and_configuration() {
        assert_eq!(status(FailureKind::Forbidden), StatusCode::FORBIDDEN);
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(
            invalid().code,
            "NATIVE_LEGAL_RUNTIME_HEALTH_REQUEST_INVALID"
        );
        assert_eq!(unavailable().kind, FailureKind::Unavailable);
        let source: Arc<dyn RuntimeHealthSource> = Arc::new(TaggedSource {
            tag: "source",
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        assert!(router_with_source(
            source.clone(),
            vec![target("acme.jesadakorn.com", "acme")],
            "TRUE"
        )
        .is_ok());
        assert_eq!(
            router_with_source(
                source.clone(),
                vec![target("acme.jesadakorn.com", "acme")],
                "TRUE; DROP TABLE legal_items"
            )
            .err()
            .map(|error| error.code),
            Some("NATIVE_LEGAL_RUNTIME_HEALTH_CONFIG_INVALID")
        );
        assert_eq!(
            router_with_source(
                source,
                vec![
                    target("acme.jesadakorn.com", "acme"),
                    target("acme.jesadakorn.com", "acme")
                ],
                "TRUE"
            )
            .err()
            .map(|error| error.code),
            Some("NATIVE_LEGAL_RUNTIME_HEALTH_CONFIG_INVALID")
        );
    }

    #[tokio::test]
    async fn two_holdings_route_to_two_distinct_readers() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let acme: Arc<dyn RuntimeHealthSource> = Arc::new(TaggedSource {
            tag: "source-acme",
            seen: seen.clone(),
        });
        let north: Arc<dyn RuntimeHealthSource> = Arc::new(TaggedSource {
            tag: "source-north",
            seen: seen.clone(),
        });
        let state = Arc::new(
            configure(
                vec![
                    (target("acme.jesadakorn.com", "acme"), acme),
                    (target("north.jesadakorn.com", "northstar"), north),
                ],
                "TRUE",
            )
            .expect("state"),
        );
        for (slug, host) in [
            ("acme", "acme.jesadakorn.com"),
            ("northstar", "north.jesadakorn.com"),
        ] {
            let response = current(
                State(state.clone()),
                Path(slug.to_owned()),
                request(host, "company-1", "true"),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response.headers().get(header::CACHE_CONTROL).unwrap(),
                "private, no-store"
            );
        }
        assert_eq!(
            *seen.lock().unwrap(),
            vec![
                "source-acme:tasks".to_owned(),
                "source-acme:ready:company-1:TRUE".to_owned(),
                "source-north:tasks".to_owned(),
                "source-north:ready:company-1:TRUE".to_owned(),
            ]
        );
    }

    #[tokio::test]
    async fn handlers_fail_closed_for_unregistered_or_unbounded_requests() {
        let source: Arc<dyn RuntimeHealthSource> = Arc::new(TaggedSource {
            tag: "source",
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        let state = Arc::new(
            configure(
                vec![(target("acme.jesadakorn.com", "acme"), source)],
                "TRUE",
            )
            .expect("state"),
        );
        for (host, company, manage, expected) in [
            ("other.jesadakorn.com", "company-1", "true", StatusCode::NOT_FOUND),
            ("acme.jesadakorn.com", "", "true", StatusCode::UNPROCESSABLE_ENTITY),
            ("acme.jesadakorn.com", "company-1", "false", StatusCode::FORBIDDEN),
            ("acme.jesadakorn.com", "company-1", "maybe", StatusCode::UNPROCESSABLE_ENTITY),
        ] {
            let response = current(
                State(state.clone()),
                Path("acme".to_owned()),
                request(host, company, manage),
            )
            .await;
            assert_eq!(response.status(), expected);
        }
    }
}
