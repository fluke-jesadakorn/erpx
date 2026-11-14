//! Owner-local HTTP ingress for the native Sales document list.
//!
//! Transport facts only: registered Host/Holding target, exact query shape,
//! bounded private JSON and business-error mapping. Normal Account session
//! resolution, current Company/branch/department scope, purchase admission and
//! the serializable owner read remain composition responsibilities supplied
//! through the backend trait.
use crate::domain::sales_documents::{
    SalesDocumentKind, normalize_status, request_invalid, validate_search,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub type SalesDocumentsHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct SalesDocumentsHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub kind: &'a str,
    pub search: &'a str,
    pub status: Option<&'a str>,
    pub cursor: Option<&'a str>,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct SalesDocumentsHttpResponse {
    /// Owner read data. The backend adds the validated current context.
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl SalesDocumentsHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, scope, purchase and owner-unit adapter.
/// The router never mints a session, actor, Company scope or purchase.
pub trait SalesDocumentsHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: SalesDocumentsHttpRequest<'a>,
    ) -> SalesDocumentsHttpFuture<'a, SalesDocumentsHttpResponse>;
}

pub struct SalesDocumentsHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn SalesDocumentsHttpBackend>,
}

struct SalesDocumentsHttpState {
    targets: BTreeMap<(String, String), SalesDocumentsHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(request_invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| request_invalid()))
        .transpose()
}

fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        !rest.is_empty()
            && !rest
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
    })
}

pub fn sales_documents_router(
    targets: Vec<SalesDocumentsHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.holding_slug.is_empty()
            || !valid_origin(&target.origin)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "SALES_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/sales/documents",
            get(list_documents),
        )
        .with_state(Arc::new(SalesDocumentsHttpState {
            targets: configured,
        })))
}

fn status_code(kind: FailureKind) -> StatusCode {
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
        status_code(error.kind),
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

fn target<'a>(
    state: &'a SalesDocumentsHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a SalesDocumentsHttpTarget, BusinessError> {
    let host = unique_header(headers, "host")?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn decode_component(value: &str) -> Result<String, BusinessError> {
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
                let high = bytes
                    .get(index + 1)
                    .copied()
                    .and_then(hex)
                    .ok_or_else(request_invalid)?;
                let low = bytes
                    .get(index + 2)
                    .copied()
                    .and_then(hex)
                    .ok_or_else(request_invalid)?;
                decoded.push(high * 16 + low);
                index += 3;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).map_err(|_| request_invalid())
}

fn query(raw: Option<&str>) -> Result<BTreeMap<String, String>, BusinessError> {
    let mut parsed = BTreeMap::new();
    if let Some(raw) = raw {
        for pair in raw.split('&') {
            if pair.is_empty() {
                continue;
            }
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let key = decode_component(key)?;
            let value = decode_component(value)?;
            if !matches!(key.as_str(), "kind" | "q" | "status" | "cursor")
                || parsed.insert(key, value).is_some()
            {
                return Err(request_invalid());
            }
        }
    }
    Ok(parsed)
}

async fn list_documents(
    State(state): State<Arc<SalesDocumentsHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let values = query(request.uri().query())?;
        let kind =
            SalesDocumentKind::parse(values.get("kind").map(String::as_str).unwrap_or_default())?;
        let search = values.get("q").map(String::as_str).unwrap_or("");
        validate_search(search)?;
        let status = normalize_status(values.get("status").map(String::as_str))?;
        let cursor = values
            .get("cursor")
            .map(String::as_str)
            .filter(|value| !value.is_empty());
        if cursor.is_some_and(|value| value.len() > 4_096) {
            return Err(request_invalid());
        }
        let target = target(&state, request.headers(), &slug)?;
        target
            .backend
            .read(SalesDocumentsHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                kind: kind.as_str(),
                search,
                status: status.as_deref(),
                cursor,
                headers: request.headers(),
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}
