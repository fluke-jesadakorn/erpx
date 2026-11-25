//! Owner-local axum read router for the lane 14 Commerce workflow order reads.
//!
//! The router owns the Holding target allow-list, header and Company isolation,
//! and error mapping. The Pg reader executes the owner SQL; TypeScript only
//! transports the bounded facts and fails closed when this route is absent.

use super::application::{CommerceLane14Read, read_document, read_page, read_references};
use super::domain::valid_id;
use super::postgres::PgCommerceLane14Read;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_RECORD: HeaderName = HeaderName::from_static("x-erpx-context-record");

/// Exact registered Holding targets. Composition supplies these; the router
/// never infers a Holding from a caller-supplied header.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn CommerceLane14Read>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_LANE14_READ_REQUEST_INVALID",
        "Use one bounded Commerce workflow order read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_LANE14_READ_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        })
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

fn target<'a>(
    state: &'a LaneState,
    slug: &str,
    request: &Request,
) -> Result<&'a Arc<dyn CommerceLane14Read>, BusinessError> {
    let host = unique_header(request.headers(), header::HOST)?
        .filter(|host| !host.is_empty() && host.len() <= 253)
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn scope(request: &Request) -> Result<(&str, &str), BusinessError> {
    let company = unique_header(request.headers(), CONTEXT_COMPANY)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)?;
    let record = unique_header(request.headers(), CONTEXT_RECORD)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)?;
    Ok((company, record))
}

/// Build the native read router. The lead mounts it; no composition or public
/// module is edited inside this lane.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgCommerceLane14Read::new(pool)), targets)
}

/// Reader-injected constructor used by the owner unit tests.
pub fn router_with_reader(
    reader: Arc<dyn CommerceLane14Read>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.host.len() > 253
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
    let state = Arc::new(LaneState {
        targets: configured,
    });
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/commerce/lane14/workflow-document",
            get(document),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/commerce/lane14/workflow-document-page",
            get(document_page),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/commerce/lane14/workflow-reference-rows",
            get(reference_rows),
        )
        .with_state(state))
}

async fn document(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let reader = target(&state, &slug, &request)?;
        let (company, id) = scope(&request)?;
        let row = read_document(reader.as_ref(), company, id).await?;
        Ok::<_, BusinessError>(serde_json::json!({"row": row}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn document_page(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let reader = target(&state, &slug, &request)?;
        let (company, after) = scope(&request)?;
        let rows = read_page(reader.as_ref(), company, after).await?;
        Ok::<_, BusinessError>(serde_json::json!({"rows": rows}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn reference_rows(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let reader = target(&state, &slug, &request)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?;
        let rows = read_references(reader.as_ref(), company).await?;
        Ok::<_, BusinessError>(serde_json::json!({"rows": rows}))
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
    use crate::adapters::native_lane_14::application::LaneReadFuture;
    use crate::adapters::native_lane_14::domain::{COLUMNS, ColumnKind};
    use erpx_server_runtime::public::http_support::body::Body;
    use erpx_server_runtime::public::http_support::serde_json::{self, Value};
    use std::sync::Mutex;

    fn good_row() -> Value {
        let mut object = serde_json::Map::new();
        for (index, column) in COLUMNS.iter().enumerate() {
            let value = match column.kind {
                ColumnKind::Text => Value::String(format!("{}-{}", column.name, index)),
                ColumnKind::Int => Value::from(index as i64),
            };
            object.insert(column.name.to_owned(), value);
        }
        Value::Object(object)
    }

    #[derive(Clone)]
    struct Probe {
        row: Option<Value>,
        seen: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl CommerceLane14Read for Probe {
        fn document<'a>(
            &'a self,
            company: &'a str,
            id: &'a str,
        ) -> LaneReadFuture<'a, Option<Value>> {
            let seen = self.seen.clone();
            let company = company.to_owned();
            let id = id.to_owned();
            let row = self.row.clone();
            Box::pin(async move {
                seen.lock().expect("lock").push((company, id));
                Ok(row)
            })
        }
        fn page<'a>(
            &'a self,
            _company: &'a str,
            _after: &'a str,
        ) -> LaneReadFuture<'a, Vec<Value>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn references<'a>(&'a self, _company: &'a str) -> LaneReadFuture<'a, Vec<Value>> {
            Box::pin(async { Ok(Vec::new()) })
        }
    }

    fn state(row: Option<Value>) -> (Arc<LaneState>, Arc<Mutex<Vec<(String, String)>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader = Probe {
            row,
            seen: seen.clone(),
        };
        let mut targets = BTreeMap::new();
        targets.insert(
            ("acme.jesadakorn.com".to_owned(), "acme".to_owned()),
            Arc::new(reader) as Arc<dyn CommerceLane14Read>,
        );
        (Arc::new(LaneState { targets }), seen)
    }

    fn request(host: &str, company: &str, record: &str, extra_company: bool) -> Request {
        let mut builder = Request::builder()
            .header(header::HOST, host)
            .header(CONTEXT_COMPANY, company)
            .header(CONTEXT_RECORD, record);
        if extra_company {
            builder = builder.header(CONTEXT_COMPANY, "co-2");
        }
        builder.body(Body::empty()).expect("request")
    }

    async fn body(response: Response) -> (StatusCode, Value) {
        let status = response.status();
        let bytes = erpx_server_runtime::public::http_support::body::to_bytes(
            response.into_body(),
            1024 * 1024,
        )
        .await
        .expect("body");
        let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, value)
    }

    #[tokio::test]
    async fn router_rejects_empty_and_duplicate_targets() {
        let reader: Arc<dyn CommerceLane14Read> = Arc::new(Probe {
            row: None,
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        assert!(router_with_reader(reader.clone(), Vec::new()).is_err());
        let duplicate = vec![
            LaneTarget {
                host: "acme.jesadakorn.com".to_owned(),
                holding_slug: "acme".to_owned(),
            },
            LaneTarget {
                host: "acme.jesadakorn.com".to_owned(),
                holding_slug: "acme".to_owned(),
            },
        ];
        assert!(router_with_reader(reader.clone(), duplicate).is_err());
        let valid = vec![LaneTarget {
            host: "acme.jesadakorn.com".to_owned(),
            holding_slug: "acme".to_owned(),
        }];
        assert!(router_with_reader(reader, valid).is_ok());
    }

    #[tokio::test]
    async fn handler_isolates_host_and_company() {
        let (state, seen) = state(Some(good_row()));
        let (status, value) = body(
            document(
                State(state.clone()),
                Path("acme".to_owned()),
                request("acme.jesadakorn.com", "co-1", "order-1", false),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value, serde_json::json!({"row": good_row()}));
        assert_eq!(
            *seen.lock().expect("lock"),
            vec![("co-1".to_owned(), "order-1".to_owned())]
        );
        let (status, _) = body(
            document(
                State(state.clone()),
                Path("acme".to_owned()),
                request("other.jesadakorn.com", "co-1", "order-1", false),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _) = body(
            document(
                State(state.clone()),
                Path("acme".to_owned()),
                request("acme.jesadakorn.com", "co-1", "order-1", true),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let (status, _) = body(
            document(
                State(state),
                Path("acme".to_owned()),
                request("acme.jesadakorn.com", "bad company!", "order-1", false),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn handler_maps_unavailable_to_503() {
        struct Down;
        impl CommerceLane14Read for Down {
            fn document<'a>(
                &'a self,
                _company: &'a str,
                _id: &'a str,
            ) -> LaneReadFuture<'a, Option<Value>> {
                Box::pin(async { Err(super::super::domain::unavailable()) })
            }
            fn page<'a>(
                &'a self,
                _company: &'a str,
                _after: &'a str,
            ) -> LaneReadFuture<'a, Vec<Value>> {
                Box::pin(async { Err(super::super::domain::unavailable()) })
            }
            fn references<'a>(&'a self, _company: &'a str) -> LaneReadFuture<'a, Vec<Value>> {
                Box::pin(async { Err(super::super::domain::unavailable()) })
            }
        }
        let mut targets = BTreeMap::new();
        targets.insert(
            ("acme.jesadakorn.com".to_owned(), "acme".to_owned()),
            Arc::new(Down) as Arc<dyn CommerceLane14Read>,
        );
        let state = Arc::new(LaneState { targets });
        let (status, value) = body(
            page_endpoint(
                state,
                request("acme.jesadakorn.com", "co-1", "cursor-1", false),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            value
                .get("error")
                .and_then(|error| error.get("code"))
                .and_then(Value::as_str),
            Some("COMMERCE_LANE14_READ_UNAVAILABLE")
        );
    }

    async fn page_endpoint(state: Arc<LaneState>, request: Request) -> Response {
        document_page(State(state), Path("acme".to_owned()), request).await
    }
}
