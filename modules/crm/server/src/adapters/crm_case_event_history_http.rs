//! Native CRM lead case-event history GET ingress.
//!
//! Transport facts only: a registered Host/Holding target, one bounded `leadId`
//! query, the exact browser Origin and business-error mapping. Current Account
//! session resolution, selected Company/branch/department scope, current IAM
//! authority, `sales.crm` purchase admission and the serializable owner unit
//! remain composition responsibilities behind
//! [`CrmCaseEventHistoryHttpBackend`]. The router never mints an actor, reads a
//! database or falls back to TypeScript: an unregistered target, an invalid
//! query or an invalid reply fails closed.
use erpx_platform_runtime_server::public::http_support::{
    Body, HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router,
    State, StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const QUERY_LIMIT: usize = 2 * 1024;

pub type CrmCaseEventHistoryHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct CrmCaseEventHistoryReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub lead_id: &'a str,
}

/// Owner read data plus the validated current context. Composition supplies
/// `context`; the router never mints it.
#[derive(Clone, Debug)]
pub struct CrmCaseEventHistoryHttpResponse {
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl CrmCaseEventHistoryHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, scope, authority, purchase and
/// serializable owner-unit adapter. The router never reads a database.
pub trait CrmCaseEventHistoryHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: CrmCaseEventHistoryReadRequest<'a>,
    ) -> CrmCaseEventHistoryHttpFuture<'a, CrmCaseEventHistoryHttpResponse>;
}

pub struct CrmCaseEventHistoryHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn CrmCaseEventHistoryHttpBackend>,
}

struct CrmCaseEventHistoryHttpState {
    targets: BTreeMap<(String, String), CrmCaseEventHistoryHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "CRM_HISTORY_REQUEST_INVALID",
        "Use one bounded CRM case-event history read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "CRM_HISTORY_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(malformed());
    }
    first
        .map(|value| value.to_str().map_err(|_| malformed()))
        .transpose()
}

fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (rest, None),
        };
        !host.is_empty()
            && !host
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
            && port.is_none_or(|port| {
                (1..=5).contains(&port.len()) && port.bytes().all(|byte| byte.is_ascii_digit())
            })
    })
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

fn valid_lead_id(value: &str) -> bool {
    !value.is_empty() && value.encode_utf16().count() <= 200 && !value.chars().any(char::is_control)
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
    state: &'a CrmCaseEventHistoryHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a CrmCaseEventHistoryHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn percent_decode(value: &str) -> Result<String, BusinessError> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).copied().and_then(hex);
            let low = bytes.get(index + 2).copied().and_then(hex);
            let (Some(high), Some(low)) = (high, low) else {
                return Err(malformed());
            };
            out.push(high * 16 + low);
            index += 3;
        } else if bytes[index] == b'+' {
            out.push(b' ');
            index += 1;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| malformed())
}

/// One `leadId` and no other query parameter on the history read.
fn history_query(request: &Request) -> Result<String, BusinessError> {
    let Some(raw) = request.uri().query() else {
        return Err(malformed());
    };
    if raw.len() > QUERY_LIMIT {
        return Err(malformed());
    }
    let mut lead_id: Option<String> = None;
    for pair in raw.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let name = percent_decode(name)?;
        let value = percent_decode(value)?;
        match name.as_str() {
            "leadId" if lead_id.is_none() && valid_lead_id(&value) => lead_id = Some(value),
            _ => return Err(malformed()),
        }
    }
    lead_id.ok_or_else(malformed)
}

pub fn crm_case_event_history_http_router(
    targets: Vec<CrmCaseEventHistoryHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_origin(&target.origin)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
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
            "/api/v1/holdings/{holding_slug}/crm/case-events",
            get(read_case_events),
        )
        .with_state(Arc::new(CrmCaseEventHistoryHttpState {
            targets: configured,
        })))
}

async fn read_case_events(
    State(state): State<Arc<CrmCaseEventHistoryHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let lead_id = history_query(&request)?;
        let headers = request.headers().clone();
        let _ = to_bytes(request.into_body(), 0)
            .await
            .map_err(|_| malformed())?;
        target
            .backend
            .read(CrmCaseEventHistoryReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                lead_id: &lead_id,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(host: &str, slug: &str, origin: &str) -> CrmCaseEventHistoryHttpTarget {
        CrmCaseEventHistoryHttpTarget {
            host: host.to_owned(),
            holding_slug: slug.to_owned(),
            origin: origin.to_owned(),
            backend: Arc::new(NeverBackend),
        }
    }

    #[test]
    fn router_accepts_exact_distinct_targets_only() {
        assert!(
            crm_case_event_history_http_router(vec![target(
                "acme.jesadakorn.com",
                "acme",
                "https://acme.jesadakorn.com"
            )])
            .is_ok()
        );
        assert!(crm_case_event_history_http_router(vec![]).is_err());
        assert!(
            crm_case_event_history_http_router(vec![
                target("acme.jesadakorn.com", "acme", "https://acme.jesadakorn.com"),
                target("acme.jesadakorn.com", "acme", "https://acme.jesadakorn.com"),
            ])
            .is_err()
        );
        assert!(
            crm_case_event_history_http_router(vec![target(
                "acme.jesadakorn.com",
                "acme",
                "http://acme.jesadakorn.com"
            )])
            .is_err()
        );
    }

    struct NeverBackend;
    impl CrmCaseEventHistoryHttpBackend for NeverBackend {
        fn read<'a>(
            &'a self,
            _request: CrmCaseEventHistoryReadRequest<'a>,
        ) -> CrmCaseEventHistoryHttpFuture<'a, CrmCaseEventHistoryHttpResponse> {
            Box::pin(async { Err(missing()) })
        }
    }

    #[test]
    fn query_accepts_one_lead_id_and_rejects_everything_else() {
        let request = |uri: &str| {
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("request")
        };
        let valid = request("/crm/case-events?leadId=lead-1");
        assert_eq!(history_query(&valid).expect("lead id"), "lead-1");
        let encoded = request("/crm/case-events?leadId=lead%2D1");
        assert_eq!(history_query(&encoded).expect("lead id"), "lead-1");
        assert!(history_query(&request("/crm/case-events")).is_err());
        assert!(history_query(&request("/crm/case-events?leadId=a&leadId=b")).is_err());
        assert!(history_query(&request("/crm/case-events?id=lead-1")).is_err());
    }

    #[test]
    fn envelope_carries_data_and_context_only() {
        let reply = CrmCaseEventHistoryHttpResponse {
            data: serde_json::json!([{"id":"event-1"}]),
            context: serde_json::json!({"companyId":"company"}),
        };
        assert_eq!(
            reply.value(),
            serde_json::json!({"data":[{"id":"event-1"}],"context":{"companyId":"company"}})
        );
    }
}
