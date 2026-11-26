//! Owner-local axum ingress for the native POS customer-search read.
//!
//! Transport facts only: registered Host/Holding target, exact bounded Company
//! header, composition-supplied authority/package decisions and a strict query
//! parser. The pool-backed owner reader owns the SQL, the projection and the
//! complete response payload.

use crate::adapters::native_customer_search::application::{search, PosCustomerReader};
use crate::adapters::native_customer_search::domain::{
    CustomerActorDecision, CustomerPackageDecision, CustomerSearchQuery, invalid,
};
use crate::adapters::native_customer_search::postgres::PgCustomerSearchReader;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    get, header, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use serde::Deserialize;
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: &str = "x-erpx-context-company";
const ACTOR_DECISION: &str = "x-erpx-pos-customer-actor";
const PACKAGE_DECISION: &str = "x-erpx-pos-customer-package";
const MAX_QUERY_BYTES: usize = 1024;
const MAX_DECISION_BYTES: usize = 4096;

pub struct CustomerSearchTarget {
    pub host: String,
    pub holding_slug: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActorHeader {
    #[serde(default)]
    read: bool,
    #[serde(default)]
    department: bool,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageHeader {
    #[serde(default)]
    enforced: bool,
    #[serde(default)]
    history: bool,
}

struct CustomerSearchState {
    targets: BTreeMap<(String, String), Arc<dyn PosCustomerReader>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "POS_CUSTOMERS_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_POS_CUSTOMERS_UNAVAILABLE",
        "The native POS customer service is unavailable.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
}

fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

fn decode(value: &str) -> Result<String, BusinessError> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' => {
                let high = bytes.get(index + 1).copied().ok_or_else(invalid)?;
                let low = bytes.get(index + 2).copied().ok_or_else(invalid)?;
                let hex = |byte: u8| -> Option<u8> {
                    match byte {
                        b'0'..=b'9' => Some(byte - b'0'),
                        b'a'..=b'f' => Some(byte - b'a' + 10),
                        b'A'..=b'F' => Some(byte - b'A' + 10),
                        _ => None,
                    }
                };
                let byte = (hex(high).ok_or_else(invalid)? << 4) | hex(low).ok_or_else(invalid)?;
                decoded.push(byte);
                index += 3;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).map_err(|_| invalid())
}

/// Parse the single optional `q` filter, rejecting duplicates, unknown keys
/// and malformed percent escapes exactly like the retained Next query.
fn parse_query(query: Option<&str>) -> Result<String, BusinessError> {
    let Some(query) = query.filter(|value| !value.is_empty()) else {
        return Ok(String::new());
    };
    if query.len() > MAX_QUERY_BYTES {
        return Err(invalid());
    }
    let mut seen = false;
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (name, value) = pair.split_once('=').ok_or_else(invalid)?;
        if decode(name)? != "q" || seen {
            return Err(invalid());
        }
        seen = true;
        return decode(value);
    }
    Ok(String::new())
}

fn decision<T: for<'de> Deserialize<'de>>(
    headers: &HeaderMap,
    name: &str,
) -> Result<T, BusinessError> {
    let raw = unique_header(headers, name)?
        .filter(|value| !value.is_empty())
        .ok_or_else(invalid)?;
    if raw.len() > MAX_DECISION_BYTES {
        return Err(invalid());
    }
    serde_json::from_str(raw).map_err(|_| invalid())
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

impl CustomerSearchState {
    fn with_readers(
        entries: Vec<(CustomerSearchTarget, Arc<dyn PosCustomerReader>)>,
    ) -> Result<Self, BusinessError> {
        let mut configured = BTreeMap::new();
        for (target, reader) in entries {
            if target.host.is_empty()
                || !valid_slug(&target.holding_slug)
                || configured
                    .insert(
                        (target.host.clone(), target.holding_slug.clone()),
                        reader,
                    )
                    .is_some()
            {
                return Err(configuration_invalid());
            }
        }
        if configured.is_empty() {
            return Err(configuration_invalid());
        }
        Ok(Self {
            targets: configured,
        })
    }

    async fn customers(&self, headers: &HeaderMap, slug: &str, query: Option<&str>) -> Response {
        match self.customers_inner(headers, slug, query).await {
            Ok(rows) => bounded(serde_json::json!({"data": rows})),
            Err(error) => error_response(error),
        }
    }

    async fn customers_inner(
        &self,
        headers: &HeaderMap,
        slug: &str,
        query: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, BusinessError> {
        let host = unique_header(headers, header::HOST.as_str())?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let reader = self
            .targets
            .get(&(host.to_owned(), slug.to_owned()))
            .ok_or_else(missing)?;
        let company = unique_header(headers, CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?;
        let actor_header: ActorHeader = decision(headers, ACTOR_DECISION)?;
        let package_header: PackageHeader = decision(headers, PACKAGE_DECISION)?;
        let query = CustomerSearchQuery {
            company: company.to_owned(),
            q: parse_query(query)?,
            actor: CustomerActorDecision {
                read: actor_header.read,
                department: actor_header.department,
            },
            package: CustomerPackageDecision {
                enforced: package_header.enforced,
                history: package_header.history,
            },
        };
        search(reader.as_ref(), &query)
            .await
            .map_err(|error| {
                if matches!(error.kind, FailureKind::Unavailable) {
                    unavailable()
                } else {
                    error
                }
            })
    }
}

async fn customers(
    State(state): State<Arc<CustomerSearchState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let query = request.uri().query().map(str::to_owned);
    state.customers(&headers, &slug, query.as_deref()).await
}

/// Build the native customer-search router with one pool shared by every
/// target. The lead mounts it; no SQL or business rule is added to TypeScript.
pub fn router(
    pool: DatabasePool,
    targets: Vec<CustomerSearchTarget>,
) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgCustomerSearchReader::new(pool)), targets)
}

/// Router over one injected owner reader, kept for single-reader tests.
pub fn router_with_reader(
    reader: Arc<dyn PosCustomerReader>,
    targets: Vec<CustomerSearchTarget>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

/// Router over one owner reader per registered Holding target. Composition
/// supplies a distinct `DatabasePool` for each Holding.
pub fn router_for_holdings(
    targets: Vec<(CustomerSearchTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                (
                    target,
                    Arc::new(PgCustomerSearchReader::new(pool)) as Arc<dyn PosCustomerReader>,
                )
            })
            .collect(),
    )
}

/// Injection path for the per-target map. No pool or SQL is created here.
pub fn router_with_readers(
    entries: Vec<(CustomerSearchTarget, Arc<dyn PosCustomerReader>)>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/pos/customers/search",
            get(customers),
        )
        .with_state(Arc::new(CustomerSearchState::with_readers(entries)?)))
}
