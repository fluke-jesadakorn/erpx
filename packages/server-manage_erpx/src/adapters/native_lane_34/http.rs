//! Owner-local axum router for the native beta feedback read.
//!
//! The Host + Holding slug pair selects one exact registered target and that
//! target's own reader/pool; the Company header supplies the only SQL bind.
//! Missing or duplicate headers and unknown targets fail closed with the same
//! status mapping as the other owner routers. No TypeScript business fallback
//! exists behind this route.

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

/// Reader-per-target map. The key is exactly `(host, holding_slug)` so one
/// Holding can never borrow another Holding's database pool.
type Targets = BTreeMap<(String, String), Arc<dyn BetaFeedbackQuery>>;

struct LaneState {
    targets: Targets,
}

/// Validate the exact distinct target set and insert each reader under its
/// own `(host, holding_slug)` key. Duplicate keys fail closed.
fn configure_targets<R>(
    entries: Vec<(LaneTarget, R)>,
) -> Result<BTreeMap<(String, String), R>, BusinessError> {
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

fn from_targets(targets: Targets) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/manage-erpx/beta-feedback",
            get(feedback),
        )
        .with_state(Arc::new(LaneState { targets }))
}

/// Reader-injection constructor retained for tests and for composition code
/// that already owns reader instances. One reader is shared across targets.
pub fn router_with_reader(
    reader: Arc<dyn BetaFeedbackQuery>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    Ok(from_targets(configure_targets(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )?))
}

/// Per-Holding constructor: every target carries its own database pool, so
/// composition binds one `PgBetaFeedbackQuery` per Holding. Cross-Holding
/// reads are impossible because the resolved reader comes from the same map
/// entry that matched the Host and slug.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let entries = targets
        .into_iter()
        .map(|(target, pool)| {
            let reader: Arc<dyn BetaFeedbackQuery> = Arc::new(PgBetaFeedbackQuery::new(pool));
            (target, reader)
        })
        .collect();
    Ok(from_targets(configure_targets(entries)?))
}

/// Single-pool compatibility constructor. The caller supplies the pool; the
/// same pool is cloned for each registered target, preserving prior behavior.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_for_holdings(
        targets
            .into_iter()
            .map(|target| (target, pool.clone()))
            .collect(),
    )
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

async fn feedback(
    State(state): State<Arc<LaneState>>,
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
            .filter(|value| valid_company_id(value))
            .ok_or_else(invalid_request)?;
        let page = load(reader.as_ref(), company).await?;
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
    use crate::adapters::native_lane_34::application::FeedbackFuture;
    use crate::adapters::native_lane_34::domain::{FeedbackItem, FeedbackPage};
    use erpx_server_runtime::public::http_support::{Body, to_bytes};

    struct StaticReader {
        id: &'static str,
    }

    impl BetaFeedbackQuery for StaticReader {
        fn list<'a>(&'a self, _company: &'a str) -> FeedbackFuture<'a> {
            let id = self.id.to_owned();
            Box::pin(async move {
                Ok(FeedbackPage {
                    items: vec![FeedbackItem {
                        id: format!("{id}-row"),
                        body: "b".to_owned(),
                        created_at: 1,
                    }],
                })
            })
        }
    }

    fn acme() -> LaneTarget {
        LaneTarget {
            host: "acme.example.test".to_owned(),
            holding_slug: "acme".to_owned(),
        }
    }

    fn northstar() -> LaneTarget {
        LaneTarget {
            host: "northstar.example.test".to_owned(),
            holding_slug: "northstar".to_owned(),
        }
    }

    fn reader(id: &'static str) -> Arc<dyn BetaFeedbackQuery> {
        Arc::new(StaticReader { id })
    }

    fn err<T>(result: Result<T, BusinessError>) -> BusinessError {
        result.err().expect("expected configuration error")
    }

    async fn call(state: Arc<LaneState>, host: &str, slug: &str) -> String {
        let request = Request::builder()
            .header(header::HOST, host)
            .header(CONTEXT_COMPANY, "acme:co")
            .body(Body::empty())
            .expect("request");
        let response = feedback(State(state), Path(slug.to_owned()), request).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1 << 20).await.expect("body");
        String::from_utf8(bytes.to_vec()).expect("utf8")
    }

    #[test]
    fn rejects_unconfigured_and_duplicate_targets() {
        assert_eq!(
            err(configure_targets(Vec::<(
                LaneTarget,
                Arc<dyn BetaFeedbackQuery>
            )>::new())),
            configuration_invalid()
        );
        let mut bad_host = acme();
        bad_host.host = String::new();
        assert_eq!(
            err(configure_targets(vec![(bad_host, reader("acme"))])),
            configuration_invalid()
        );
        let mut bad_slug = acme();
        bad_slug.holding_slug = "Acme".to_owned();
        assert_eq!(
            err(configure_targets(vec![(bad_slug, reader("acme"))])),
            configuration_invalid()
        );
        let duplicate = acme();
        assert_eq!(
            err(configure_targets(vec![
                (duplicate.clone(), reader("one")),
                (duplicate, reader("two")),
            ])),
            configuration_invalid()
        );
    }

    #[test]
    fn accepts_an_exact_distinct_target_set() {
        let configured = configure_targets(vec![
            (acme(), reader("acme")),
            (northstar(), reader("northstar")),
        ])
        .expect("targets");
        assert_eq!(configured.len(), 2);
        assert!(configured.contains_key(&("acme.example.test".to_owned(), "acme".to_owned())));
    }

    #[tokio::test]
    async fn two_targets_route_to_two_distinct_readers() {
        let acme_reader = reader("acme");
        let northstar_reader = reader("northstar");
        let targets = configure_targets(vec![
            (acme(), acme_reader.clone()),
            (northstar(), northstar_reader.clone()),
        ])
        .expect("targets");
        let first = targets
            .get(&("acme.example.test".to_owned(), "acme".to_owned()))
            .expect("acme reader");
        let second = targets
            .get(&("northstar.example.test".to_owned(), "northstar".to_owned()))
            .expect("northstar reader");
        assert!(!Arc::ptr_eq(first, second));
        assert_eq!(
            first.list("acme:co").await.expect("acme page").items[0].id,
            "acme-row"
        );
        assert_eq!(
            second.list("acme:co").await.expect("northstar page").items[0].id,
            "northstar-row"
        );

        let state = Arc::new(LaneState { targets });
        assert!(
            call(state.clone(), "acme.example.test", "acme")
                .await
                .contains("acme-row")
        );
        assert!(
            call(state, "northstar.example.test", "northstar")
                .await
                .contains("northstar-row")
        );
    }

    #[test]
    fn router_with_reader_keeps_the_single_reader_convenience() {
        assert!(router_with_reader(reader("acme"), vec![acme()]).is_ok());
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
