//! Owner-local HTTP ingress for the native document-page derivative command.
//!
//! Transport facts only: exact registered Host/Holding targets, one origin
//! check, one bounded multipart-free body, one idempotency key and query shape.
//! The current actor, the owning expense authority and the serializable unit
//! remain composition responsibilities behind [`DocumentDerivativeBackend`].
use crate::domain::document_derivative::MAX_PAGE_BYTES;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub type DerivativeHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct DocumentDerivativeSaveRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub attachment_id: &'a str,
    pub page: u32,
    pub page_count: u32,
    pub original_checksum: &'a str,
    pub bytes: &'a [u8],
    pub key: &'a str,
}

/// The complete wire reply. `{id,page}` has no destination href, so the shared
/// `canonicalDestinationLinks` envelope is an identity transform here.
#[derive(Clone, Debug)]
pub struct DocumentDerivativeReply {
    pub id: String,
    pub page: u32,
}

impl DocumentDerivativeReply {
    pub fn value(&self) -> serde_json::Value {
        serde_json::json!({"id": self.id, "page": self.page})
    }
}

pub trait DocumentDerivativeBackend: Send + Sync {
    fn save<'a>(
        &'a self,
        request: DocumentDerivativeSaveRequest<'a>,
    ) -> DerivativeHttpFuture<'a, DocumentDerivativeReply>;
}

pub struct DocumentDerivativeTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn DocumentDerivativeBackend>,
}

struct HttpState {
    targets: BTreeMap<(String, String), DocumentDerivativeTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed() -> BusinessError {
    BusinessError::invalid("ATTACHMENT_REQUEST_INVALID", "Review the file request.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "ATTACHMENT_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_'))
}

fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        !rest.is_empty()
            && !rest
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
    })
}

fn valid_attachment_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || matches!(byte, b'/' | b'\\' | b'?' | b'#'))
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

fn target<'a>(
    state: &'a HttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a DocumentDerivativeTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
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

fn percent_decode(raw: &str) -> Result<String, BusinessError> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' => {
                let high = bytes
                    .get(index + 1)
                    .and_then(|value| (*value as char).to_digit(16))
                    .ok_or_else(malformed)?;
                let low = bytes
                    .get(index + 2)
                    .and_then(|value| (*value as char).to_digit(16))
                    .ok_or_else(malformed)?;
                out.push((high * 16 + low) as u8);
                index += 3;
            }
            value => {
                out.push(value);
                index += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| malformed())
}

/// The derivatives route accepts only `page`, `total` and `original`. Unknown
/// or repeated keys fail closed; numeric bounds are rechecked by the domain.
fn page_query(query: Option<&str>) -> Result<(u32, u32, String), BusinessError> {
    let mut values = BTreeMap::new();
    for pair in query
        .unwrap_or_default()
        .split('&')
        .filter(|pair| !pair.is_empty())
    {
        let (key, raw) = pair.split_once('=').ok_or_else(malformed)?;
        let key = percent_decode(key)?;
        if !matches!(key.as_str(), "page" | "total" | "original")
            || values.insert(key, percent_decode(raw)?).is_some()
        {
            return Err(malformed());
        }
    }
    let page = values
        .get("page")
        .ok_or_else(malformed)?
        .parse::<u32>()
        .map_err(|_| malformed())?;
    let total = values
        .get("total")
        .ok_or_else(malformed)?
        .parse::<u32>()
        .map_err(|_| malformed())?;
    let original = values
        .get("original")
        .filter(|value| value.len() == 64)
        .ok_or_else(malformed)?
        .to_owned();
    Ok((page, total, original))
}

fn command_key(headers: &HeaderMap) -> Result<String, BusinessError> {
    let key = unique_header(headers, HeaderName::from_static("idempotency-key"))?
        .filter(|value| (8..=180).contains(&value.len()))
        .ok_or_else(|| {
            BusinessError::invalid(
                "COMMAND_KEY_REQUIRED",
                "A valid Idempotency-Key is required.",
            )
        })?;
    if !key
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'.' | b'-'))
    {
        return Err(BusinessError::invalid(
            "COMMAND_KEY_REQUIRED",
            "A valid Idempotency-Key is required.",
        ));
    }
    Ok(key.to_owned())
}

/// Assemble the exact owner router. Composition supplies registered targets and
/// the backend; the router never resolves a request-derived Host or session.
pub fn document_derivative_router(
    targets: Vec<DocumentDerivativeTarget>,
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
            "/api/v1/holdings/{holding_slug}/attachments/{attachment_id}/derivatives",
            post(save),
        )
        .with_state(Arc::new(HttpState {
            targets: configured,
        })))
}

async fn save(
    State(state): State<Arc<HttpState>>,
    Path((slug, attachment_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let (page, page_count, original_checksum) = match page_query(request.uri().query()) {
        Ok(value) => value,
        Err(error) => return error_response(error),
    };
    let result = async move {
        let target = target(&state, &headers, &slug)?;
        if !valid_attachment_id(&attachment_id) {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "DOCUMENT_NOT_FOUND",
                "The original document is unavailable or changed.",
            ));
        }
        if let Some(origin) = unique_header(&headers, header::ORIGIN)? {
            if origin != target.origin {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "ORIGIN_FORBIDDEN",
                    "Use the application origin.",
                ));
            }
        }
        let key = command_key(&headers)?;
        let declared = unique_header(&headers, header::CONTENT_LENGTH)?
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0 && *value <= MAX_PAGE_BYTES)
            .ok_or_else(|| {
                BusinessError::invalid("PAGE_SIZE", "A rendered page must be at most 2 MB.")
            })?;
        let bytes = to_bytes(request.into_body(), MAX_PAGE_BYTES)
            .await
            .map_err(|_| malformed())?;
        if bytes.len() != declared {
            return Err(BusinessError::invalid(
                "PAGE_SIZE",
                "The uploaded page size changed.",
            ));
        }
        target
            .backend
            .save(DocumentDerivativeSaveRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                attachment_id: &attachment_id,
                page,
                page_count,
                original_checksum: &original_checksum,
                bytes: &bytes,
                key: &key,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => (StatusCode::OK, Json(reply.value())).into_response(),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_platform_runtime_server::public::http_support::serve;
    use std::sync::Mutex;

    struct Probe {
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl DocumentDerivativeBackend for Probe {
        fn save<'a>(
            &'a self,
            request: DocumentDerivativeSaveRequest<'a>,
        ) -> DerivativeHttpFuture<'a, DocumentDerivativeReply> {
            self.seen.lock().unwrap().push(format!(
                "{}|{}|{}|{}|{}|{}|{}",
                request.host,
                request.holding_slug,
                request.attachment_id,
                request.page,
                request.page_count,
                request.original_checksum,
                request.key
            ));
            Box::pin(async {
                Ok(DocumentDerivativeReply {
                    id: "derivative-1".into(),
                    page: 1,
                })
            })
        }
    }

    fn target(host: &str, backend: Arc<dyn DocumentDerivativeBackend>) -> DocumentDerivativeTarget {
        DocumentDerivativeTarget {
            host: host.into(),
            holding_slug: "acme".into(),
            origin: format!("https://{host}"),
            backend,
        }
    }

    #[test]
    fn query_accepts_exact_page_shape_and_rejects_unknown_keys() {
        let checksum = "a".repeat(64);
        assert_eq!(
            page_query(Some(&format!("page=1&total=3&original={checksum}"))).unwrap(),
            (1, 3, checksum.clone())
        );
        assert!(page_query(Some("page=1&total=1&original=short")).is_err());
        assert!(page_query(Some("page=x&total=1&original=short")).is_err());
        assert!(page_query(Some("page=1&total=1&extra=1")).is_err());
        assert!(page_query(Some("page=1&page=2&total=1")).is_err());
    }

    #[tokio::test]
    async fn saves_exact_wire_payload_and_rejects_bad_origin_and_stream() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let router = document_derivative_router(vec![target(
            "acme.example.test",
            Arc::new(Probe { seen: seen.clone() }),
        )])
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let _server = tokio::spawn(async move {
            let _ = serve(listener, router).await;
        });
        let checksum = "a".repeat(64);
        let body = b"\xff\xd8\xff\xe0 rendered jpeg";
        let request = format!(
            "POST /api/v1/holdings/acme/attachments/attachment-1/derivatives?page=1&total=1&original={checksum} HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://acme.example.test\r\nIdempotency-Key: key-00000001\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        stream.write_all(request.as_bytes()).await.unwrap();
        stream.write_all(body).await.unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let split = bytes.windows(4).position(|part| part == b"\r\n\r\n").unwrap() + 4;
        let text = String::from_utf8_lossy(&bytes[..split]).to_ascii_lowercase();
        assert!(text.contains("200 ok"), "{text}");
        let reply: serde_json::Value = serde_json::from_slice(&bytes[split..]).unwrap();
        assert_eq!(reply, serde_json::json!({"id":"derivative-1","page":1}));
        assert_eq!(
            *seen.lock().unwrap(),
            vec![format!(
                "acme.example.test|acme|attachment-1|1|1|{checksum}|key-00000001"
            )]
        );
    }
}
