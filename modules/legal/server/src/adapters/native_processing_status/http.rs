//! Owner-local axum router for the Legal processing-status read.
//!
//! This follows the same target/header/Company isolation and error mapping as
//! the other native Legal reads: an exact registered `(host, holding_slug)` pair
//! is required, the Company identity and the item-access authority arrive from
//! composition as unique private headers, and the owner executes all SQL. No
//! TypeScript business fallback exists for this operation.

use std::{collections::BTreeMap, sync::Arc};

use erpx_platform_runtime_server::public::{
    http_support::{
        HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router,
        State, StatusCode, header, routing::get, serde_json,
    },
    postgres::DatabasePool,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};

use super::{
    application::{ProcessingStatusAuthority, ProcessingStatusSource, read_processing_status},
    domain::{ProcessingStatusTarget, valid_id, valid_slug},
    postgres::PgProcessingStatusSource,
};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const AUTHORITY_ACCESSIBLE: HeaderName =
    HeaderName::from_static("x-erpx-legal-processing-accessible");

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn ProcessingStatusSource>>,
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_LEGAL_PROCESSING_STATUS_UNAVAILABLE",
        "The native Legal processing-status service is unavailable.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_LEGAL_PROCESSING_STATUS_REQUEST_INVALID",
        "Use one bounded Legal processing-status read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_LEGAL_PROCESSING_STATUS_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
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

fn scope(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    match unique_header(headers, name)? {
        None | Some("") => Ok(None),
        Some(value) if valid_id(value) => Ok(Some(value)),
        Some(_) => Err(invalid()),
    }
}

fn authority(headers: &HeaderMap) -> Result<ProcessingStatusAuthority, BusinessError> {
    let flag = match unique_header(headers, AUTHORITY_ACCESSIBLE)? {
        Some("true") => true,
        Some("false") => false,
        _ => return Err(invalid()),
    };
    Ok(ProcessingStatusAuthority {
        item_accessible: flag,
    })
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

/// Shared target configuration. Every public constructor funnels through this
/// map, so the `(host, holding_slug)` key and exact-target validation have one
/// owner. Each key holds one reader, which may be a distinct Pg reader for a
/// distinct Holding pool.
fn configure_targets(
    targets: Vec<(ProcessingStatusTarget, Arc<dyn ProcessingStatusSource>)>,
) -> Result<BTreeMap<(String, String), Arc<dyn ProcessingStatusSource>>, BusinessError> {
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
    targets: BTreeMap<(String, String), Arc<dyn ProcessingStatusSource>>,
) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/legal/processing/{id}",
            get(current),
        )
        .with_state(Arc::new(LaneState { targets }))
}

/// Build the native read router with one pool shared by all registered targets.
pub fn router(pool: DatabasePool, targets: Vec<ProcessingStatusTarget>) -> Result<Router, BusinessError> {
    let reader: Arc<dyn ProcessingStatusSource> = Arc::new(PgProcessingStatusSource::new(pool));
    let targets = targets.into_iter().map(|target| (target, reader.clone())).collect();
    Ok(into_router(configure_targets(targets)?))
}

/// Per-Holding constructor. Each `(ProcessingStatusTarget, DatabasePool)` pair
/// gets its own reader, so every Holding reads only from its own database pool
/// while the same `host`/`holding_slug` isolation applies to all of them.
pub fn router_for_holdings(
    targets: Vec<(ProcessingStatusTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let readers = targets
        .into_iter()
        .map(|(target, pool)| {
            let reader: Arc<dyn ProcessingStatusSource> = Arc::new(PgProcessingStatusSource::new(pool));
            (target, reader)
        })
        .collect();
    Ok(into_router(configure_targets(readers)?))
}

/// Reader-injection constructor used by owner tests.
pub fn router_with_reader(
    reader: Arc<dyn ProcessingStatusSource>,
    targets: Vec<ProcessingStatusTarget>,
) -> Result<Router, BusinessError> {
    let targets = targets.into_iter().map(|target| (target, reader.clone())).collect();
    Ok(into_router(configure_targets(targets)?))
}

async fn current(
    State(state): State<Arc<LaneState>>,
    Path((slug, id)): Path<(String, String)>,
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
        let company = scope(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| !value.is_empty())
            .ok_or_else(invalid)?;
        if !valid_id(&id) {
            return Err(invalid());
        }
        let authority = authority(request.headers())?;
        let read = read_processing_status(reader.as_ref(), authority, company, &id).await?;
        let body = serde_json::to_value(read).map_err(|_| unavailable())?;
        Ok::<_, BusinessError>(serde_json::json!({"data": body}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use erpx_platform_runtime_server::public::http_support::body::Body;
    use super::super::application::ProcessingStatusFuture;
    use super::super::domain::ProcessingStatusRead;

    struct TaggedReader {
        tag: &'static str,
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl ProcessingStatusSource for TaggedReader {
        fn status<'a>(
            &'a self,
            company: &'a str,
            id: &'a str,
        ) -> ProcessingStatusFuture<'a, Option<ProcessingStatusRead>> {
            let tag = self.tag;
            let seen = self.seen.clone();
            let company = company.to_owned();
            let id = id.to_owned();
            Box::pin(async move {
                seen.lock().unwrap().push(format!("{tag}:{company}:{id}"));
                Ok(Some(ProcessingStatusRead {
                    id: id.clone(),
                    item_id: "item-1".to_owned(),
                    document_id: "doc-1".to_owned(),
                    state: "queued".to_owned(),
                    error: None,
                    attempts: 1,
                    job_id: id,
                    result: serde_json::Value::Null,
                }))
            })
        }
    }

    fn request(host: &str, accessible: &str) -> Request {
        Request::builder()
            .header(header::HOST, host)
            .header(CONTEXT_COMPANY, "company-1")
            .header(AUTHORITY_ACCESSIBLE, accessible)
            .body(Body::empty())
            .expect("request")
    }

    fn target(host: &str, slug: &str) -> ProcessingStatusTarget {
        ProcessingStatusTarget {
            host: host.to_owned(),
            holding_slug: slug.to_owned(),
        }
    }

    #[test]
    fn error_mapping_and_configuration() {
        assert_eq!(status(FailureKind::Forbidden), StatusCode::FORBIDDEN);
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(invalid().code, "NATIVE_LEGAL_PROCESSING_STATUS_REQUEST_INVALID");
        assert_eq!(unavailable().kind, FailureKind::Unavailable);
        let reader: Arc<dyn ProcessingStatusSource> = Arc::new(TaggedReader {
            tag: "reader",
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        assert!(router_with_reader(reader.clone(), vec![target("acme.jesadakorn.com", "acme")]).is_ok());
        assert_eq!(
            router_with_reader(
                reader,
                vec![
                    target("acme.jesadakorn.com", "acme"),
                    target("acme.jesadakorn.com", "acme")
                ]
            )
            .err()
            .map(|error| error.code),
            Some("NATIVE_LEGAL_PROCESSING_STATUS_CONFIG_INVALID")
        );
    }

    #[tokio::test]
    async fn two_holdings_route_to_two_distinct_readers() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let acme: Arc<dyn ProcessingStatusSource> = Arc::new(TaggedReader {
            tag: "reader-acme",
            seen: seen.clone(),
        });
        let north: Arc<dyn ProcessingStatusSource> = Arc::new(TaggedReader {
            tag: "reader-north",
            seen: seen.clone(),
        });
        let state = Arc::new(LaneState {
            targets: configure_targets(vec![
                (target("acme.jesadakorn.com", "acme"), acme),
                (target("north.jesadakorn.com", "northstar"), north),
            ])
            .expect("targets"),
        });
        for (slug, host) in [
            ("acme", "acme.jesadakorn.com"),
            ("northstar", "north.jesadakorn.com"),
        ] {
            let response = current(
                State(state.clone()),
                Path((slug.to_owned(), "job-1".to_owned())),
                request(host, "true"),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
        }
        assert_eq!(
            *seen.lock().unwrap(),
            vec![
                "reader-acme:company-1:job-1".to_owned(),
                "reader-north:company-1:job-1".to_owned(),
            ]
        );
    }

    #[tokio::test]
    async fn handlers_fail_closed_for_unregistered_or_unbounded_requests() {
        let reader: Arc<dyn ProcessingStatusSource> = Arc::new(TaggedReader {
            tag: "reader",
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        let state = Arc::new(LaneState {
            targets: configure_targets(vec![(target("acme.jesadakorn.com", "acme"), reader)])
                .expect("targets"),
        });
        for (host, id, accessible, status_code) in [
            ("other.jesadakorn.com", "job-1", "true", StatusCode::NOT_FOUND),
            ("acme.jesadakorn.com", "bad id!", "true", StatusCode::UNPROCESSABLE_ENTITY),
            ("acme.jesadakorn.com", "job-1", "false", StatusCode::FORBIDDEN),
            ("acme.jesadakorn.com", "job-1", "maybe", StatusCode::UNPROCESSABLE_ENTITY),
            ("acme.jesadakorn.com", "job-1", "", StatusCode::UNPROCESSABLE_ENTITY),
        ] {
            let response = current(
                State(state.clone()),
                Path(("acme".to_owned(), id.to_owned())),
                request(host, accessible),
            )
            .await;
            assert_eq!(response.status(), status_code);
        }
    }
}
