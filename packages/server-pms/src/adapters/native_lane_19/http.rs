//! Owner-local axum read router for lane 19. The target table enforces exact
//! Holding host + slug isolation; every scope value arrives once as a bounded
//! header. The router owns no SQL and falls closed on any configuration error.

use std::{collections::BTreeMap, sync::Arc};

use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router,
    State, StatusCode, header, routing::get,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
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

/// Build the owner read router over an injected reader. Used by unit tests and
/// by [`router`], which supplies the Pg reader.
pub fn read_router(
    reader: Arc<dyn GuestAccountEvidenceRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    reader.clone(),
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/pms/guest-account-finance-evidence",
            get(evidence),
        )
        .with_state(Arc::new(Lane19State {
            targets: configured,
        })))
}

/// Mount entry point: the lead supplies the shared Pg pool and exact targets.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    read_router(super::postgres::reader(pool), targets)
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
    use erpx_server_shared::public::RecordId;
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
