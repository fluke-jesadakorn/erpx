//! Owner-local axum router for the native Commerce entrypoint read family.
//!
//! Mirrors `quality_product_read.rs`: exact registered Holding targets, unique
//! Host/Company headers, bounded identities, no-store responses and explicit
//! error mapping. No SQL is planned or executed on the TypeScript side.

use super::application::{CommerceEntrypointRead, read_facts};
use super::domain::{validate, valid_id};
use super::postgres::PgCommerceEntrypointRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, post,
    serde_json::{self, Value},
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

/// Exact registered Holding target; the same pair must be declared in the
/// caller's native transport configuration.
#[derive(Clone, Debug)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

pub type LaneReader = Arc<dyn CommerceEntrypointRead>;

struct LaneState {
    targets: BTreeMap<(String, String), LaneReader>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_ENTRYPOINT_READ_REQUEST_INVALID",
        "Use one bounded Commerce entrypoint read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_ENTRYPOINT_READ_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first.map(|value| value.to_str().map_err(|_| invalid())).transpose()
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

fn state(targets: Vec<LaneTarget>, reader: LaneReader) -> Result<Arc<LaneState>, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), reader.clone())
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Arc::new(LaneState { targets: configured }))
}

/// Mountable router. Composition supplies only the pool and exact targets; the
/// owner owns the reader, SQL and response projection.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgCommerceEntrypointRead::new(pool)), targets)
}

/// Injectable variant used by the owner's own tests.
pub fn router_with_reader(reader: LaneReader, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/commerce/entrypoint-facts",
            post(read),
        )
        .with_state(state(targets, reader)?))
}

/// Pure request processing shared by the router and the unit tests.
async fn process(
    headers: &HeaderMap,
    slug: &str,
    body: &[u8],
    lane: &LaneState,
) -> Result<Value, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    let reader = lane
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)?;
    let company = unique_header(headers, CONTEXT_COMPANY)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)?;
    let value: Value = serde_json::from_slice(body).map_err(|_| invalid())?;
    let (query, company) = validate(&value, company)?;
    let facts = read_facts(reader.as_ref(), &company, &query).await?;
    Ok(serde_json::json!({"facts": facts.map(|facts| facts.into_value())}))
}

async fn read(
    State(lane): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let headers = request.headers().clone();
        let bytes = to_bytes(request.into_body(), 64 * 1024)
            .await
            .map_err(|_| invalid())?;
        process(&headers, &slug, &bytes, &lane).await
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
    use super::super::application::EntrypointReadFuture;
    use super::super::domain::{EntrypointFacts, EntrypointKind};
    use std::{collections::BTreeMap, sync::Mutex};

    #[derive(Clone)]
    struct Probe {
        rows: BTreeMap<(String, EntrypointKind, String), EntrypointFacts>,
        seen: Arc<Mutex<Vec<(String, EntrypointKind, String)>>>,
    }

    impl CommerceEntrypointRead for Probe {
        fn read<'a>(
            &'a self,
            company: &'a str,
            kind: EntrypointKind,
            id: &'a str,
        ) -> EntrypointReadFuture<'a, Option<EntrypointFacts>> {
            let seen = self.seen.clone();
            let rows = self.rows.clone();
            let company = company.to_owned();
            let id = id.to_owned();
            Box::pin(async move {
                seen.lock().expect("lock").push((company.clone(), kind, id.clone()));
                Ok(rows.get(&(company, kind, id)).cloned())
            })
        }
    }

    fn probe() -> Arc<Probe> {
        let mut rows = BTreeMap::new();
        rows.insert(
            ("co-1".to_owned(), EntrypointKind::OrderFacts, "o-1".to_owned()),
            EntrypointFacts {
                kind: "sales".to_owned(),
                state: Some("draft".to_owned()),
            },
        );
        rows.insert(
            ("co-1".to_owned(), EntrypointKind::ReturnKind, "r-1".to_owned()),
            EntrypointFacts { kind: "sales".to_owned(), state: None },
        );
        Arc::new(Probe { rows, seen: Arc::new(Mutex::new(Vec::new())) })
    }

    fn headers(host: &str, company: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_str(host).expect("host"));
        headers.insert(CONTEXT_COMPANY, HeaderValue::from_str(company).expect("company"));
        headers
    }

    fn target() -> Vec<LaneTarget> {
        vec![LaneTarget {
            host: "acme.example.test".to_owned(),
            holding_slug: "acme".to_owned(),
        }]
    }

    #[tokio::test]
    async fn owner_facts_shape_and_company_scope() {
        let lane = state(target(), probe()).expect("state");
        let body = br#"{"kind":"orderFacts","id":"o-1"}"#;
        let value = process(
            &headers("acme.example.test", "co-1"),
            "acme",
            body,
            &lane,
        )
        .await
        .expect("facts");
        assert_eq!(value, serde_json::json!({"facts":{"kind":"sales","state":"draft"}}));
    }

    #[tokio::test]
    async fn missing_facts_are_null_not_authority() {
        let lane = state(target(), probe()).expect("state");
        let body = br#"{"kind":"invoiceKind","id":"missing"}"#;
        let value = process(
            &headers("acme.example.test", "co-1"),
            "acme",
            body,
            &lane,
        )
        .await
        .expect("facts");
        assert_eq!(value, serde_json::json!({"facts":null}));
    }

    #[tokio::test]
    async fn wrong_host_duplicate_headers_and_bad_company_fail_closed() {
        let lane = state(target(), probe()).expect("state");
        let body = br#"{"kind":"orderKind","id":"o-1"}"#;
        assert_eq!(
            process(&headers("attacker.example.test", "co-1"), "acme", body, &lane)
                .await
                .unwrap_err()
                .kind,
            FailureKind::NotFound
        );
        assert_eq!(
            process(&headers("acme.example.test", "bad company!"), "acme", body, &lane)
                .await
                .unwrap_err()
                .kind,
            FailureKind::InvalidInput
        );
        let mut duplicate = headers("acme.example.test", "co-1");
        duplicate.append(CONTEXT_COMPANY, HeaderValue::from_static("co-2"));
        assert_eq!(
            process(&duplicate, "acme", body, &lane).await.unwrap_err().kind,
            FailureKind::InvalidInput
        );
    }

    #[test]
    fn router_requires_exact_distinct_targets() {
        let reader: LaneReader = probe();
        assert!(router_with_reader(reader.clone(), target()).is_ok());
        assert!(router_with_reader(reader.clone(), vec![]).is_err());
        assert!(router_with_reader(reader.clone(), vec![target()[0].clone(), target()[0].clone()]).is_err());
        assert!(router_with_reader(
            reader,
            vec![LaneTarget { host: String::new(), holding_slug: "acme".to_owned() }],
        )
        .is_err());
    }
}
