//! Owner-local axum read router for the Learning evidence-detail family.
//!
//! Transport facts only: an exact registered Host/Holding target, one bounded
//! path, single-valued Company/branch/department headers and one strict JSON
//! request. The owner reader executes the SQL and projects the row; no SQL,
//! predicate or bind crosses the wire from the caller. Failures map to the
//! shared error envelope and a missing runtime fails closed with 503.

use super::application::EvidenceService;
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

struct LaneState {
    targets: BTreeMap<(String, String), ()>,
    service: Arc<EvidenceService>,
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

fn configure(targets: Vec<LaneTarget>) -> Result<BTreeMap<(String, String), ()>, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), ())
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

/// Build the owner-local read router. The lead mounts it; no other module is
/// edited here. One reader is shared across all registered Holding targets.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let configured = configure(targets)?;
    let service = Arc::new(EvidenceService::new(Arc::new(PgEvidenceReader::new(pool))));
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/learning/evidence-detail",
            post(read),
        )
        .with_state(Arc::new(LaneState { targets: configured, service })))
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
        if !state.targets.contains_key(&(host.to_owned(), slug.clone())) {
            return Err(missing());
        }
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
        let row: Option<EvidenceRow> = state
            .service
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

    #[test]
    fn rejects_empty_or_duplicate_targets() {
        assert_eq!(configure(vec![]), Err(configuration_invalid()));
        let duplicate = vec![
            LaneTarget { host: "acme.jesadakorn.com".into(), holding_slug: "acme".into() },
            LaneTarget { host: "acme.jesadakorn.com".into(), holding_slug: "acme".into() },
        ];
        assert_eq!(configure(duplicate), Err(configuration_invalid()));
        let bad_slug = vec![LaneTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "ACME".into(),
        }];
        assert_eq!(configure(bad_slug), Err(configuration_invalid()));
        let ok = configure(vec![LaneTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
        }])
        .unwrap();
        assert_eq!(ok.len(), 1);
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
