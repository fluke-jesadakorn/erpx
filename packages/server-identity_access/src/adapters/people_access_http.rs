//! Native People relationship HTTP ingress.
//!
//! Transport facts only: registered Host/Holding targets, a bounded JSON body,
//! strict Idempotency-Key on apply and business-error mapping. Normal Account
//! session resolution, the selected Company scope, current IAM authority and
//! the serializable owner unit remain composition responsibilities behind
//! [`PeopleRelationshipsHttpBackend`].
use crate::domain::people_access::request_invalid;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde_json::Value;
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;

pub type PeopleRelationshipsHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// The reviewed relationship command. `Preview` carries the submitted change;
/// `Apply` carries the reviewed token and the caller's Idempotency-Key.
#[derive(Clone, Debug)]
pub enum PeopleRelationshipsHttpCommand {
    Preview { change: Value },
    Apply { token: String, idempotency_key: String },
}

pub struct PeopleRelationshipsHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: PeopleRelationshipsHttpCommand,
}

#[derive(Clone, Debug)]
pub struct PeopleRelationshipsHttpResponse {
    /// Owner data. The backend adds the validated current context.
    pub data: Value,
    pub context: Value,
}

impl PeopleRelationshipsHttpResponse {
    pub fn value(self) -> Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, scope, authority and owner unit adapter.
/// The router never mints a session, actor, Company or permission.
pub trait PeopleRelationshipsHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: PeopleRelationshipsHttpRequest<'a>,
    ) -> PeopleRelationshipsHttpFuture<'a, PeopleRelationshipsHttpResponse>;
}

pub struct PeopleRelationshipsHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn PeopleRelationshipsHttpBackend>,
}

struct PeopleRelationshipsHttpState {
    targets: BTreeMap<(String, String), PeopleRelationshipsHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    request_invalid()
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_RELATIONSHIPS_HTTP_CONFIGURATION_INVALID",
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
                (1..=5).contains(&port.len())
                    && port.bytes().all(|byte| byte.is_ascii_digit())
                    && port.parse::<u16>().is_ok_and(|value| value > 0)
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
    state: &'a PeopleRelationshipsHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a PeopleRelationshipsHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}
fn ensure_no_query(request: &Request) -> Result<(), BusinessError> {
    match request.uri().query() {
        None | Some("") => Ok(()),
        Some(_) => Err(malformed()),
    }
}
async fn strict_body(request: Request) -> Result<Value, BusinessError> {
    let content_type =
        unique_header(request.headers(), header::CONTENT_TYPE)?.ok_or_else(malformed)?;
    if content_type.split(';').next().map(str::trim) != Some("application/json") {
        return Err(malformed());
    }
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| malformed())?;
    if bytes.is_empty() {
        return Err(malformed());
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| malformed())?;
    if !value.is_object() {
        return Err(malformed());
    }
    Ok(value)
}
fn strict_key(headers: &HeaderMap) -> Result<String, BusinessError> {
    let value =
        unique_header(headers, HeaderName::from_static("idempotency-key"))?.ok_or_else(|| {
            BusinessError::invalid(
                "COMMAND_KEY_REQUIRED",
                "Idempotency-Key is required for this identity command.",
            )
        })?;
    CommandKey::new(value.to_owned())?;
    Ok(value.to_owned())
}

/// Holding ingress for the reviewed relationship commands. Platform callers
/// have no route here; relationship metadata is Company-scoped by contract.
pub fn people_relationships_http_router(
    targets: Vec<PeopleRelationshipsHttpTarget>,
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
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/iam/people-relationships/preview",
            post(preview),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/iam/people-relationships/apply",
            post(apply),
        )
        .with_state(Arc::new(PeopleRelationshipsHttpState {
            targets: configured,
        })))
}

async fn preview(
    State(state): State<Arc<PeopleRelationshipsHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        let change = body
            .as_object()
            .and_then(|object| {
                if object.len() == 1 {
                    object.get("change")
                } else {
                    None
                }
            })
            .cloned()
            .ok_or_else(malformed)?;
        target
            .backend
            .execute(PeopleRelationshipsHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: PeopleRelationshipsHttpCommand::Preview { change },
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn apply(
    State(state): State<Arc<PeopleRelationshipsHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        let key = strict_key(request.headers())?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        let token = body
            .as_object()
            .and_then(|object| {
                if object.len() == 1 {
                    object.get("previewToken").and_then(Value::as_str)
                } else {
                    None
                }
            })
            .map(str::to_owned)
            .ok_or_else(malformed)?;
        if !crate::domain::people_access::valid_token(&token) {
            return Err(crate::domain::people_access::preview_expired());
        }
        target
            .backend
            .execute(PeopleRelationshipsHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: PeopleRelationshipsHttpCommand::Apply {
                    token,
                    idempotency_key: key,
                },
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
    use super::{
        PeopleRelationshipsHttpBackend, PeopleRelationshipsHttpCommand,
        PeopleRelationshipsHttpFuture, PeopleRelationshipsHttpRequest,
        PeopleRelationshipsHttpResponse, PeopleRelationshipsHttpTarget,
        people_relationships_http_router,
    };
    use erpx_server_runtime::public::http_support::{serde_json, serve};
    use erpx_server_shared::public::BusinessError;
    use std::sync::{Arc, Mutex};

    struct Probe {
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl PeopleRelationshipsHttpBackend for Probe {
        fn execute<'a>(
            &'a self,
            request: PeopleRelationshipsHttpRequest<'a>,
        ) -> PeopleRelationshipsHttpFuture<'a, PeopleRelationshipsHttpResponse> {
            Box::pin(async move {
                let cookie = request
                    .headers
                    .get("cookie")
                    .and_then(|value| value.to_str().ok());
                let seen = match &request.command {
                    PeopleRelationshipsHttpCommand::Preview { change } => format!(
                        "preview|{}|{}|{}|{cookie:?}",
                        request.host,
                        request.holding_slug,
                        change["operation"].as_str().unwrap_or("")
                    ),
                    PeopleRelationshipsHttpCommand::Apply {
                        token,
                        idempotency_key,
                    } => format!(
                        "apply|{}|{}|{}|{}|{cookie:?}",
                        request.host, request.holding_slug, token, idempotency_key
                    ),
                };
                self.seen.lock().unwrap().push(seen);
                let data = match &request.command {
                    PeopleRelationshipsHttpCommand::Preview { .. } => {
                        serde_json::json!({"token":"3f2504e0-4f89-11d3-9a0c-0305e82c3301","summary":"organization: Partner"})
                    }
                    PeopleRelationshipsHttpCommand::Apply { .. } => {
                        serde_json::json!({"id":"iam-organization-1","version":1})
                    }
                };
                Ok(PeopleRelationshipsHttpResponse {
                    data,
                    context: serde_json::json!({"userId":"actor"}),
                })
            })
        }
    }

    struct Noop;

    impl PeopleRelationshipsHttpBackend for Noop {
        fn execute<'a>(
            &'a self,
            _: PeopleRelationshipsHttpRequest<'a>,
        ) -> PeopleRelationshipsHttpFuture<'a, PeopleRelationshipsHttpResponse> {
            Box::pin(async {
                Err(BusinessError::new(
                    erpx_server_shared::public::FailureKind::NotFound,
                    "NOT_FOUND",
                    "Not found.",
                ))
            })
        }
    }

    fn target() -> PeopleRelationshipsHttpTarget {
        PeopleRelationshipsHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(Noop),
        }
    }

    #[test]
    fn target_configuration_requires_distinct_exact_holdings() {
        assert!(people_relationships_http_router(vec![target()]).is_ok());
        assert!(people_relationships_http_router(vec![target(), target()]).is_err());
        assert!(
            people_relationships_http_router(vec![PeopleRelationshipsHttpTarget {
                origin: "http://acme.example.test".into(),
                ..target()
            }])
            .is_err()
        );
        assert!(
            people_relationships_http_router(vec![PeopleRelationshipsHttpTarget {
                holding_slug: "Acme".into(),
                ..target()
            }])
            .is_err()
        );
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, serde_json::Value) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(raw.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let text = String::from_utf8_lossy(&bytes).to_string();
        let status = text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok())
            .unwrap();
        (
            status,
            serde_json::from_str(text.split("\r\n\r\n").nth(1).unwrap_or(""))
                .unwrap_or(serde_json::Value::Null),
        )
    }

    fn post(path: &str, host: &str, key: Option<&str>, body: &str) -> String {
        let key = key
            .map(|value| format!("Idempotency-Key: {value}\r\n"))
            .unwrap_or_default();
        format!(
            "POST {path} HTTP/1.1\r\nHost: {host}\r\nCookie: erpx=session\r\n\
             Content-Type: application/json\r\n{key}Content-Length: {}\r\n\
             Connection: close\r\n\r\n{body}",
            body.len()
        )
    }

    #[tokio::test]
    async fn router_forwards_reviewed_relationship_commands_and_rejects_malformed_input() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let router = people_relationships_http_router(vec![PeopleRelationshipsHttpTarget {
            backend: Arc::new(Probe { seen: seen.clone() }),
            ..target()
        }])
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let _server = tokio::spawn(async move {
            let _ = serve(listener, router).await;
        });

        let change = r#"{"change":{"operation":"organization","expectedVersion":0,"name":"Partner","kind":"independent","reason":"Record the contractor relationship"}}"#;
        let (status, value) = http(
            addr,
            post(
                "/api/v1/holdings/acme/iam/people-relationships/preview",
                "acme.example.test",
                None,
                change,
            ),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(
            value["data"]["token"],
            "3f2504e0-4f89-11d3-9a0c-0305e82c3301"
        );

        let (status, _) = http(
            addr,
            post(
                "/api/v1/holdings/acme/iam/people-relationships/apply",
                "acme.example.test",
                Some("command-1"),
                r#"{"previewToken":"3f2504e0-4f89-11d3-9a0c-0305e82c3301"}"#,
            ),
        )
        .await;
        assert_eq!(status, 200);

        // Missing Idempotency-Key never reaches the owner backend.
        let (status, value) = http(
            addr,
            post(
                "/api/v1/holdings/acme/iam/people-relationships/apply",
                "acme.example.test",
                None,
                r#"{"previewToken":"3f2504e0-4f89-11d3-9a0c-0305e82c3301"}"#,
            ),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(value["error"]["code"], "COMMAND_KEY_REQUIRED");

        // Unknown Host is not a registered target.
        let (status, _) = http(
            addr,
            post(
                "/api/v1/holdings/acme/iam/people-relationships/preview",
                "elsewhere.example.test",
                None,
                change,
            ),
        )
        .await;
        assert_eq!(status, 404);

        // The preview envelope is exact: one `change` key, nothing else.
        let (status, value) = http(
            addr,
            post(
                "/api/v1/holdings/acme/iam/people-relationships/preview",
                "acme.example.test",
                None,
                r#"{"change":{"operation":"organization"},"previewToken":"3f2504e0-4f89-11d3-9a0c-0305e82c3301"}"#,
            ),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(value["error"]["code"], "PEOPLE_ACCESS_REQUEST_INVALID");

        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 2, "{seen:?}");
        assert!(seen[0].starts_with("preview|acme.example.test|acme|organization|"));
        assert_eq!(
            seen[1],
            "apply|acme.example.test|acme|3f2504e0-4f89-11d3-9a0c-0305e82c3301|command-1|Some(\"erpx=session\")"
        );
    }
}
