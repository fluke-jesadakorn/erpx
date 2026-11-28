//! Owner-local HTTP ingress for the verified-customer portal support-thread
//! read. The handler validates transport facts, delegates to the application
//! port and validates the complete payload. It never prepares SQL.

use crate::application::portal_support_threads::ThreadsFuture;
use crate::domain::portal_support_threads::{Thread, invalid};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    get, header, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Native route mounted by composition for the tickets family.
pub const CUSTOMER_THREADS_PATH: &str =
    "/api/v1/portals/{holding_slug}/customer-access/support/threads/{binding_id}";

pub struct CustomerThreadsRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub binding_id: String,
}

pub trait CustomerThreadBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: CustomerThreadsRequest<'a>,
    ) -> ThreadsFuture<'a, Vec<Thread>>;
}

pub struct CustomerThreadTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn CustomerThreadBackend>,
}

struct CustomerThreadState {
    targets: BTreeMap<(String, String), CustomerThreadTarget>,
}

fn invalid_input() -> BusinessError {
    invalid()
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}

fn origin_ok(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        !rest.is_empty()
            && !rest
                .bytes()
                .any(|b| matches!(b, b'/' | b'?' | b'#' | b'@' | b':'))
    })
}

fn unique<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid_input());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid_input()))
        .transpose()
}

fn target<'a>(
    targets: &'a BTreeMap<(String, String), CustomerThreadTarget>,
    headers: &'a HeaderMap,
    slug: &str,
) -> Result<(&'a CustomerThreadTarget, &'a str), BusinessError> {
    let host = unique(headers, "host")?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    let found = targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)?;
    if unique(headers, "origin")?.is_some_and(|value| value != found.origin) {
        return Err(forbidden());
    }
    Ok((found, host))
}

fn valid_response(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == 4
            && [
                "id",
                "version",
                "message",
                "publishedAt",
            ]
            .iter()
            .all(|key| object.contains_key(*key))
            && object["id"].as_str().is_some_and(|id| !id.is_empty())
            && object["version"].as_i64().is_some_and(|version| version >= 0)
            && object["message"].as_str().is_some()
            && object["publishedAt"].as_i64().is_some_and(|at| at >= 0)
    })
}

fn valid_thread(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == 6
            && [
                "requestId",
                "requestedAt",
                "note",
                "response",
                "acknowledgedAt",
                "closedAt",
            ]
            .iter()
            .all(|key| object.contains_key(*key))
            && object["requestId"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
            && object["requestedAt"].as_i64().is_some_and(|at| at >= 0)
            && object["note"].as_str().is_some()
            && (object["response"].is_null() || valid_response(&object["response"]))
            && (object["acknowledgedAt"].is_null()
                || object["acknowledgedAt"]
                    .as_i64()
                    .is_some_and(|at| at >= 0))
            && (object["closedAt"].is_null()
                || object["closedAt"].as_i64().is_some_and(|at| at >= 0))
    })
}

fn code(kind: FailureKind) -> StatusCode {
    match kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn err(error: BusinessError) -> Response {
    (
        code(error.kind),
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn bounded(value: Value) -> Response {
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

async fn run(
    state: &CustomerThreadState,
    headers: &HeaderMap,
    slug: &str,
    binding_id: &str,
    query: Option<&str>,
) -> Result<Value, BusinessError> {
    let (found, host) = target(&state.targets, headers, slug)?;
    if query.is_some_and(|value| !value.is_empty()) {
        return Err(invalid_input());
    }
    let binding_id = RecordId::new(binding_id)
        .map_err(|_| invalid_input())?
        .as_str()
        .to_owned();
    let threads = found
        .backend
        .execute(CustomerThreadsRequest {
            host,
            holding_slug: slug,
            headers,
            binding_id,
        })
        .await?;
    let wire: Vec<Value> = threads.iter().map(Thread::wire).collect();
    if !wire.iter().all(valid_thread) {
        return Err(invalid_input());
    }
    Ok(serde_json::json!({"data":wire}))
}

async fn customer_threads_handler(
    State(state): State<Arc<CustomerThreadState>>,
    Path((slug, binding_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let parts = request.into_parts();
    match run(
        &state,
        &parts.0.headers,
        &slug,
        &binding_id,
        parts.0.uri.query(),
    )
    .await
    {
        Ok(value) => bounded(value),
        Err(error) => err(error),
    }
}

/// Build the customer-facing thread router from exact registered targets.
pub fn portal_support_threads_router(
    targets: Vec<CustomerThreadTarget>,
) -> Result<Router, BusinessError> {
    let mut map = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.holding_slug.is_empty()
            || target.holding_slug.len() > 128
            || !target.holding_slug.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
            })
            || !origin_ok(&target.origin)
            || map
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "PORTAL_SUPPORT_THREADS_HTTP_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            CUSTOMER_THREADS_PATH,
            get(customer_threads_handler),
        )
        .with_state(Arc::new(CustomerThreadState { targets: map })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_platform_runtime_server::public::http_support::serve;
    use std::sync::Mutex;

    struct Probe {
        seen: Arc<Mutex<Vec<String>>>,
    }
    impl CustomerThreadBackend for Probe {
        fn execute<'a>(
            &'a self,
            request: CustomerThreadsRequest<'a>,
        ) -> ThreadsFuture<'a, Vec<Thread>> {
            let label = format!(
                "{}|{}|{}",
                request.host, request.holding_slug, request.binding_id
            );
            self.seen.lock().unwrap().push(label);
            let thread = crate::domain::portal_support_threads::thread_from_columns(
                "request-1".into(),
                1000,
                "note".into(),
                Some("response-1".into()),
                Some(1),
                Some("message".into()),
                Some(2000),
                None,
                None,
            );
            Box::pin(async move { Ok(vec![thread]) })
        }
    }

    fn router() -> (Router, Arc<Mutex<Vec<String>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let target = CustomerThreadTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(Probe { seen: seen.clone() }),
        };
        (
            portal_support_threads_router(vec![target]).unwrap(),
            seen,
        )
    }

    async fn get_request(path: &str) -> (u16, String) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (router, _) = router();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = serve(listener, router).await;
        });
        let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
        socket
            .write_all(
                format!(
                    "GET {path} HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n"
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let mut bytes = Vec::new();
        socket.read_to_end(&mut bytes).await.unwrap();
        server.abort();
        let text = String::from_utf8_lossy(&bytes).to_string();
        let status = text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok())
            .unwrap();
        (status, text)
    }

    #[tokio::test]
    async fn serves_the_complete_thread_payload() {
        let (status, text) = get_request(
            "/api/v1/portals/acme/customer-access/support/threads/binding-1",
        )
        .await;
        assert_eq!(status, 200, "{text}");
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
        let value: Value = serde_json::from_str(body).unwrap();
        let thread = &value["data"][0];
        assert_eq!(thread.as_object().unwrap().len(), 6);
        assert_eq!(thread["requestId"], "request-1");
        assert_eq!(thread["response"]["id"], "response-1");
        assert_eq!(thread["response"].as_object().unwrap().len(), 4);
        assert!(text.contains("cache-control: private, no-store"));
    }

    #[tokio::test]
    async fn rejects_query_selectors() {
        let (status, text) = get_request(
            "/api/v1/portals/acme/customer-access/support/threads/binding-1?all=1",
        )
        .await;
        assert_eq!(status, 422, "{text}");
    }
}
