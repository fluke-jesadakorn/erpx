//! Owner-local axum ingress for the native Finance root aggregate read.
//!
//! Composition supplies one pool per registered Holding. The exact
//! `(Host, holding_slug)` pair selects a single Holding reader and the Company
//! header is the only SQL bind; `chartId` selects a ledger inside that Company.

use crate::adapters::finance_root_aggregate::{
    application::{RootAggregateRead, handle},
    domain::{
        configuration_invalid, missing, invalid_request, valid_chart_id, valid_company_id,
        valid_host, valid_slug,
    },
    postgres::PgRootAggregateRead,
};
use erpx_server_runtime::public::{
    http_support::{
        HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Query, Request, Response,
        Router, State, StatusCode, header, routing::get, serde_json,
    },
    postgres::DatabasePool,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

#[derive(Clone, Debug, Deserialize)]
pub struct RootQuery {
    #[serde(rename = "chartId")]
    pub chart_id: Option<String>,
}

/// A registered Holding host/slug pair. Both the Host header and the path slug
/// must match exactly; no suffix or first-holding fallback exists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootTarget {
    pub host: String,
    pub holding_slug: String,
}

struct RootState {
    readers: BTreeMap<(String, String), Arc<dyn RootAggregateRead>>,
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

fn bounded(value: serde_json::Value) -> Response {
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

fn state_from<I>(entries: I) -> Result<Arc<RootState>, BusinessError>
where
    I: IntoIterator<Item = (RootTarget, Arc<dyn RootAggregateRead>)>,
{
    let mut readers = BTreeMap::new();
    for (target, reader) in entries {
        if !valid_host(&target.host)
            || !valid_slug(&target.holding_slug)
            || readers
                .insert((target.host.clone(), target.holding_slug.clone()), reader)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if readers.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Arc::new(RootState { readers }))
}

fn mounted(state: Arc<RootState>) -> Router {
    Router::new()
        .route("/api/v1/holdings/{holding_slug}/finance", get(read))
        .with_state(state)
}

/// Build the router with one shared reader. Retained for injectable tests.
pub fn router_with_reader(
    reader: Arc<dyn RootAggregateRead>,
    targets: Vec<RootTarget>,
) -> Result<Router, BusinessError> {
    Ok(mounted(state_from(
        targets.into_iter().map(|target| (target, reader.clone())),
    )?))
}

/// Build the router with one owner reader per Holding pool. `pools[0]` is the
/// control plane; entry `i` pairs `config.holdings[i]` with `pools[i + 1]`.
pub fn router_for_holdings(
    targets: Vec<(RootTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let state = state_from(targets.into_iter().map(|(target, pool)| {
        let reader: Arc<dyn RootAggregateRead> = Arc::new(PgRootAggregateRead::new(pool));
        (target, reader)
    }))?;
    Ok(mounted(state))
}

async fn read(
    State(state): State<Arc<RootState>>,
    Path(slug): Path<String>,
    Query(query): Query<RootQuery>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| valid_host(host))
            .ok_or_else(missing)?;
        let reader = state
            .readers
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|company| valid_company_id(company))
            .ok_or_else(invalid_request)?
            .to_owned();
        let chart = match query.chart_id {
            Some(id) if !valid_chart_id(&id) => return Err(invalid_request()),
            id => id,
        };
        let value = handle(reader.as_ref(), &company, chart.as_deref()).await?;
        Ok::<_, BusinessError>(value)
    }
    .await;
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::finance_root_aggregate::{
        application::AggregateFuture, domain::unavailable,
    };

    struct FakeReader;

    impl RootAggregateRead for FakeReader {
        fn aggregate<'a>(
            &'a self,
            _company_id: &'a str,
            _chart_id: Option<&'a str>,
        ) -> AggregateFuture<'a> {
            Box::pin(async {
                Ok(crate::adapters::finance_root_aggregate::application::RootAggregate {
                    charts: vec![],
                    journals: vec![],
                    invoices: vec![],
                    payments: vec![],
                    report: serde_json::json!({}),
                })
            })
        }
    }

    fn target(host: &str, slug: &str) -> RootTarget {
        RootTarget {
            host: host.to_owned(),
            holding_slug: slug.to_owned(),
        }
    }

    #[test]
    fn root_aggregate_router_requires_distinct_registered_targets() {
        let reader: Arc<dyn RootAggregateRead> = Arc::new(FakeReader);
        assert!(router_with_reader(reader.clone(), vec![]).is_err());
        assert!(
            router_with_reader(reader.clone(), vec![target("acme.example.com", "acme")]).is_ok()
        );
        assert!(router_with_reader(
            reader.clone(),
            vec![
                target("acme.example.com", "acme"),
                target("acme.example.com", "acme")
            ]
        )
        .is_err());
        assert!(router_with_reader(reader, vec![target("", "acme")]).is_err());
    }

    #[test]
    fn root_aggregate_error_mapping_and_bounded_headers() {
        assert_eq!(
            error_response(unavailable()).status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            error_response(invalid_request()).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(error_response(missing()).status(), StatusCode::NOT_FOUND);
        let response = bounded(serde_json::json!({"data": {}}));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
    }
}
