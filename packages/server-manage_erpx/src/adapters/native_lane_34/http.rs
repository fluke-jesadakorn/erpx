//! Owner-local axum router for the native beta feedback read.
//!
//! The Host + Holding slug pair selects one exact registered target; the
//! Company header supplies the only SQL bind. Missing or duplicate headers and
//! unknown targets fail closed with the same status mapping as the other
//! owner routers. No TypeScript business fallback exists behind this route.

use crate::adapters::native_lane_34::application::{BetaFeedbackQuery, load};
use crate::adapters::native_lane_34::domain::{
    FeedbackResponse, configuration_invalid, invalid_request, missing, valid_company_id, valid_slug,
};
use crate::adapters::native_lane_34::postgres::PgBetaFeedbackQuery;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, get, header, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

/// One exact registered Holding target. Duplicate or malformed pairs are
/// rejected at router construction; no prefix or wildcard resolution occurs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

type Targets = BTreeMap<(String, String), ()>;

struct LaneState {
    targets: Targets,
    reader: Arc<dyn BetaFeedbackQuery>,
}

fn configure_targets(targets: Vec<LaneTarget>) -> Result<Targets, BusinessError> {
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

fn bounded(body: FeedbackResponse) -> Response {
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

/// Build the native read router. The lead mounts it; composition is not edited
/// by this lane. No new TypeScript SQL or business rule is introduced.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let configured = configure_targets(targets)?;
    let reader: Arc<dyn BetaFeedbackQuery> = Arc::new(PgBetaFeedbackQuery::new(pool));
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/manage-erpx/beta-feedback",
            get(feedback),
        )
        .with_state(Arc::new(LaneState {
            targets: configured,
            reader,
        })))
}

async fn feedback(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        if !state.targets.contains_key(&(host.to_owned(), slug.clone())) {
            return Err(missing());
        }
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_company_id(value))
            .ok_or_else(invalid_request)?;
        let page = load(state.reader.as_ref(), company).await?;
        Ok::<_, BusinessError>(FeedbackResponse::from(page))
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
    fn rejects_unconfigured_and_duplicate_targets() {
        assert_eq!(
            configure_targets(Vec::new()).unwrap_err(),
            configuration_invalid()
        );
        assert_eq!(
            configure_targets(vec![LaneTarget {
                host: String::new(),
                holding_slug: "acme".to_owned(),
            }])
            .unwrap_err(),
            configuration_invalid()
        );
        assert_eq!(
            configure_targets(vec![LaneTarget {
                host: "acme.example.test".to_owned(),
                holding_slug: "Acme".to_owned(),
            }])
            .unwrap_err(),
            configuration_invalid()
        );
        let duplicate = LaneTarget {
            host: "acme.example.test".to_owned(),
            holding_slug: "acme".to_owned(),
        };
        assert_eq!(
            configure_targets(vec![duplicate.clone(), duplicate]).unwrap_err(),
            configuration_invalid()
        );
    }

    #[test]
    fn accepts_an_exact_distinct_target_set() {
        let configured = configure_targets(vec![
            LaneTarget {
                host: "acme.example.test".to_owned(),
                holding_slug: "acme".to_owned(),
            },
            LaneTarget {
                host: "northstar.example.test".to_owned(),
                holding_slug: "northstar".to_owned(),
            },
        ])
        .expect("targets");
        assert_eq!(configured.len(), 2);
        assert!(configured.contains_key(&("acme.example.test".to_owned(), "acme".to_owned())));
    }

    #[test]
    fn maps_failure_kinds_to_the_owner_status_contract() {
        assert_eq!(
            status(FailureKind::InvalidInput),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(
            status(FailureKind::Unavailable),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
}
