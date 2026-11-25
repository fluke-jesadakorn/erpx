//! Lane 35 owner-local axum ingress for the Records work-report read family.
//!
//! Transport facts only: one exact registered Host/Holding target, a Company
//! header that must equal the body scope, strict body validation and owner
//! error mapping. The handler and Postgres reader below own the query and the
//! projection. No fallback SQL exists on the TypeScript side.

use crate::adapters::native_lane_35::application::{LaneFuture, LaneQueryHandler, WorkReportSource};
use crate::adapters::native_lane_35::domain::{
    CandidateRow, EventRow, LaneQuery, LaneTarget, SubmitTime, configuration_invalid, invalid,
    valid_id, valid_slug,
};
use crate::adapters::native_lane_35::postgres::PgWorkReportSource;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

const COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const MAX_BODY: usize = 64 * 1024;

/// Object-safe bridge so a distinct owner reader (and therefore a distinct
/// Holding pool) can be installed per target, while tests inject fake readers.
struct SharedSource(Arc<dyn WorkReportSource>);

impl WorkReportSource for SharedSource {
    fn candidate_rows<'a>(&'a self, company: &'a str) -> LaneFuture<'a, Vec<CandidateRow>> {
        self.0.candidate_rows(company)
    }
    fn submit_times<'a>(
        &'a self,
        company: &'a str,
        ids: &'a [String],
    ) -> LaneFuture<'a, Vec<SubmitTime>> {
        self.0.submit_times(company, ids)
    }
    fn event_rows<'a>(&'a self, company: &'a str, ids: &'a str) -> LaneFuture<'a, Vec<EventRow>> {
        self.0.event_rows(company, ids)
    }
}

type Handler = LaneQueryHandler<SharedSource>;

struct LaneHttpState {
    targets: BTreeMap<(String, String), Arc<Handler>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
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
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn bounded(body: Value) -> Response {
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

fn target<'a>(
    state: &'a LaneHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a Arc<Handler>, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn valid_target(target: &LaneTarget) -> bool {
    !target.host.is_empty() && valid_slug(&target.holding_slug)
}

/// Pure target check, exercised without a live pool.
pub fn validate_targets(targets: &[LaneTarget]) -> Result<(), BusinessError> {
    let mut seen = BTreeMap::new();
    for target in targets {
        if !valid_target(target)
            || seen.insert((&target.host, &target.holding_slug), ()).is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if targets.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(())
}

/// Install one reader per exact target key. Each entry owns its own handler and
/// therefore its own injected reader/pool; no reader is shared across targets.
fn build_handlers(
    targets: Vec<(LaneTarget, Arc<dyn WorkReportSource>)>,
) -> Result<BTreeMap<(String, String), Arc<Handler>>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in targets {
        if !valid_target(&target)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    Arc::new(LaneQueryHandler::new(SharedSource(reader))),
                )
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

fn router_from(configured: BTreeMap<(String, String), Arc<Handler>>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/records/native-lane-35/work-report",
            post(read_report),
        )
        .with_state(Arc::new(LaneHttpState {
            targets: configured,
        }))
}

/// Single-pool constructor kept for tests and the original local setup.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgWorkReportSource::new(pool)), targets)
}

/// Reader-injection constructor kept for tests. The same reader is cloned into
/// every target, so a fake can assert route/target isolation.
pub fn router_with_reader(
    reader: Arc<dyn WorkReportSource>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    validate_targets(&targets)?;
    let configured = build_handlers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )?;
    Ok(router_from(configured))
}

/// Per-Holding constructor. Composition pairs each registered Holding with its
/// own pool; every target gets a distinct `PgWorkReportSource` over that pool.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let configured = build_handlers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                (
                    target,
                    Arc::new(PgWorkReportSource::new(pool)) as Arc<dyn WorkReportSource>,
                )
            })
            .collect(),
    )?;
    Ok(router_from(configured))
}

async fn read_report(
    State(state): State<Arc<LaneHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let handler = target(&state, request.headers(), &slug)?;
        let company = unique_header(request.headers(), COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?
            .to_owned();
        match request.uri().query() {
            None | Some("") => Ok(()),
            Some(_) => Err(invalid()),
        }?;
        let bytes = to_bytes(request.into_body(), MAX_BODY)
            .await
            .map_err(|_| invalid())?;
        let query: LaneQuery = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        // A caller cannot read a Company other than the one in the signed scope.
        if query.company_id() != company {
            return Err(invalid());
        }
        let body = handler.execute(query).await?.value();
        Ok::<_, BusinessError>(body)
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

    #[derive(Clone, Copy)]
    struct TaggedSource(&'static str);

    impl WorkReportSource for TaggedSource {
        fn candidate_rows<'a>(&'a self, company: &'a str) -> LaneFuture<'a, Vec<CandidateRow>> {
            let row = CandidateRow {
                approval_json: None,
                items_json: None,
                id: self.0.to_owned(),
                kind: "purchase".to_owned(),
                title: self.0.to_owned(),
                state: "queued".to_owned(),
                requester_user_id: company.to_owned(),
                branch_id: None,
                department_id: None,
                currency: None,
                created_at: 1,
                updated_at: 2,
            };
            Box::pin(async move { Ok(vec![row]) })
        }
        fn submit_times<'a>(
            &'a self,
            _company: &'a str,
            _ids: &'a [String],
        ) -> LaneFuture<'a, Vec<SubmitTime>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn event_rows<'a>(&'a self, _company: &'a str, _ids: &'a str) -> LaneFuture<'a, Vec<EventRow>> {
            Box::pin(async { Ok(vec![]) })
        }
    }

    #[tokio::test]
    async fn two_targets_route_to_two_distinct_readers() {
        // The reader-injection path is the same one `router_for_holdings` uses
        // with one PgWorkReportSource per Holding pool.
        let alpha: Arc<dyn WorkReportSource> = Arc::new(TaggedSource("alpha"));
        let beta: Arc<dyn WorkReportSource> = Arc::new(TaggedSource("beta"));
        assert!(!Arc::ptr_eq(&alpha, &beta));

        let handlers = build_handlers(vec![
            (
                LaneTarget {
                    host: "alpha.jesadakorn.com".to_owned(),
                    holding_slug: "acme".to_owned(),
                },
                alpha,
            ),
            (
                LaneTarget {
                    host: "beta.jesadakorn.com".to_owned(),
                    holding_slug: "acme".to_owned(),
                },
                beta,
            ),
        ])
        .unwrap();

        assert_eq!(handlers.len(), 2);
        let key_alpha = ("alpha.jesadakorn.com".to_owned(), "acme".to_owned());
        let key_beta = ("beta.jesadakorn.com".to_owned(), "acme".to_owned());
        let handler_alpha = handlers.get(&key_alpha).unwrap();
        let handler_beta = handlers.get(&key_beta).unwrap();
        assert!(!Arc::ptr_eq(handler_alpha, handler_beta));

        let query = || LaneQuery::Candidates {
            company_id: "acme".to_owned(),
        };
        let from_alpha = handler_alpha.execute(query()).await.unwrap();
        let from_beta = handler_beta.execute(query()).await.unwrap();
        assert_ne!(from_alpha, from_beta);
        assert!(!handlers.contains_key(&("other.jesadakorn.com".to_owned(), "acme".to_owned())));
    }

    #[test]
    fn target_configuration_requires_exact_distinct_hosts() {
        let good = LaneTarget {
            host: "acme.jesadakorn.com".to_owned(),
            holding_slug: "acme".to_owned(),
        };
        assert!(validate_targets(&[good.clone()]).is_ok());
        let bad = LaneTarget {
            host: good.host.clone(),
            holding_slug: "BAD SLUG".to_owned(),
        };
        assert_eq!(
            validate_targets(&[bad]).unwrap_err().code,
            "NATIVE_RECORDS_WORK_REPORT_CONFIG_INVALID"
        );
        assert_eq!(
            validate_targets(&[good.clone(), good]).unwrap_err().code,
            "NATIVE_RECORDS_WORK_REPORT_CONFIG_INVALID"
        );
        assert_eq!(
            validate_targets(&[]).unwrap_err().code,
            "NATIVE_RECORDS_WORK_REPORT_CONFIG_INVALID"
        );
    }

    #[test]
    fn query_envelope_is_tagged() {
        let candidates: LaneQuery = serde_json::from_str(
            r#"{"kind":"candidates","companyId":"acme"}"#,
        )
        .unwrap();
        assert_eq!(candidates.company_id(), "acme");
        let events: LaneQuery =
            serde_json::from_str(r#"{"kind":"events","companyId":"acme","ids":"[]"}"#).unwrap();
        assert_eq!(events.company_id(), "acme");
        assert!(serde_json::from_str::<LaneQuery>(r#"{"kind":"other"}"#).is_err());
    }
}
