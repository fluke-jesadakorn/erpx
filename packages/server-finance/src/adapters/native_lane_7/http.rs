//! Owner-local axum ingress for the native Finance invoice-list read.
//!
//! Composition supplies one pool per registered Holding; this router accepts no
//! SQL and no caller predicate. The exact `(Host, holding_slug)` pair selects a
//! single Holding reader, the Company header and optional `limit` are the only
//! binds. Unknown query keys fail closed.

use crate::adapters::native_lane_7::{
    application::{InvoiceListRead, handle},
    domain::{
        DEFAULT_LIMIT, configuration_invalid, invalid_request, missing, valid_company_id,
        valid_host, valid_slug,
    },
    postgres::PgInvoiceListRead,
};
use erpx_server_runtime::public::{
    http_support::{
        Body, HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response,
        Router, State, StatusCode, header, routing::get, serde_json,
    },
    postgres::DatabasePool,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

/// A registered Holding host/slug pair. Both the Host header and the path slug
/// must match exactly; no suffix or first-holding fallback exists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LaneState {
    readers: BTreeMap<(String, String), Arc<dyn InvoiceListRead>>,
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

/// Only `limit=<integer>` is accepted; every other key or duplicate fails
/// closed so no caller predicate can reach the owner statement.
fn parse_limit(request: &Request) -> Result<i64, BusinessError> {
    let Some(raw) = request.uri().query() else {
        return Ok(DEFAULT_LIMIT);
    };
    let mut limit = None;
    for part in raw.split('&') {
        if part.is_empty() {
            continue;
        }
        let mut pair = part.splitn(2, '=');
        match pair.next() {
            Some("limit") => {
                if limit.is_some() {
                    return Err(invalid_request());
                }
                let value = pair.next().ok_or_else(invalid_request)?;
                limit = Some(value.parse::<i64>().map_err(|_| invalid_request())?);
            }
            _ => return Err(invalid_request()),
        }
    }
    Ok(limit.unwrap_or(DEFAULT_LIMIT))
}

fn state_from<I>(entries: I) -> Result<Arc<LaneState>, BusinessError>
where
    I: IntoIterator<Item = (LaneTarget, Arc<dyn InvoiceListRead>)>,
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
    Ok(Arc::new(LaneState { readers }))
}

fn mounted(state: Arc<LaneState>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/finance/native-lane-7/invoice-list",
            get(read),
        )
        .with_state(state)
}

/// Build the router with one shared reader. Retained for injectable tests.
pub fn router_with_reader(
    reader: Arc<dyn InvoiceListRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    Ok(mounted(state_from(
        targets.into_iter().map(|target| (target, reader.clone())),
    )?))
}

/// Build the router with one owner reader per Holding pool. `pools[0]` is the
/// control plane; entry `i` pairs `config.holdings[i]` with `pools[i + 1]`.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let state = state_from(targets.into_iter().map(|(target, pool)| {
        let reader: Arc<dyn InvoiceListRead> = Arc::new(PgInvoiceListRead::new(pool));
        (target, reader)
    }))?;
    Ok(mounted(state))
}

async fn read(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
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
        let limit = parse_limit(&request)?;
        let value = handle(reader.as_ref(), &company, limit).await?;
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
    use crate::adapters::native_lane_7::{
        application::InvoiceListFuture, domain::unavailable,
    };

    struct FakeReader;

    impl InvoiceListRead for FakeReader {
        fn invoice_list<'a>(
            &'a self,
            _company_id: &'a str,
            _limit: i64,
        ) -> InvoiceListFuture<'a> {
            Box::pin(async {
                Ok(vec![serde_json::json!({
                    "id": "i1",
                    "invoice_number": "INV-1",
                    "kind": "purchase",
                    "invoice_date": "2026-01-01",
                    "due_date": "2026-01-31",
                    "currency": "THB",
                    "total_minor": 1000,
                    "outstanding_minor": 0,
                    "status": "draft",
                    "party_name": "Acme",
                })])
            })
        }
    }

    fn target(host: &str, slug: &str) -> LaneTarget {
        LaneTarget {
            host: host.to_owned(),
            holding_slug: slug.to_owned(),
        }
    }

    fn request(query: Option<&str>) -> Request {
        let uri = match query {
            Some(query) => format!(
                "http://acme.example.com/api/v1/holdings/acme/finance/native-lane-7/invoice-list?{query}"
            ),
            None => "http://acme.example.com/api/v1/holdings/acme/finance/native-lane-7/invoice-list"
                .to_owned(),
        };
        Request::builder().uri(uri).body(Body::empty()).unwrap()
    }

    #[test]
    fn native_lane_7_router_requires_distinct_registered_targets() {
        let reader: Arc<dyn InvoiceListRead> = Arc::new(FakeReader);
        assert!(router_with_reader(reader.clone(), vec![]).is_err());
        assert!(router_with_reader(reader.clone(), vec![target("acme.example.com", "acme")]).is_ok());
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
    fn native_lane_7_accepts_only_the_bound_limit_query() {
        assert_eq!(parse_limit(&request(None)).unwrap(), DEFAULT_LIMIT);
        assert_eq!(parse_limit(&request(Some("limit=7"))).unwrap(), 7);
        assert!(parse_limit(&request(Some("limit=7&limit=8"))).is_err());
        assert!(parse_limit(&request(Some("kind=purchase"))).is_err());
        assert!(parse_limit(&request(Some("limit=abc"))).is_err());
    }

    #[test]
    fn native_lane_7_error_mapping_and_bounded_headers() {
        assert_eq!(
            error_response(unavailable()).status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            error_response(invalid_request()).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(error_response(missing()).status(), StatusCode::NOT_FOUND);
        let response = bounded(serde_json::json!({"data": []}));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
    }
}
