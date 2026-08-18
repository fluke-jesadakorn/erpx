//! Typed HTTP and gRPC transport policy for Web and native console clients.

use std::{fmt, future::Future, pin::Pin, time::Duration};

use super::{CapabilityStatus, ClientTarget, NativeEndpointConfig};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
pub const DEFAULT_RESPONSE_LIMIT: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl HttpMethod {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpEndpoint {
    Auth,
    Public,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub endpoint: HttpEndpoint,
    pub method: HttpMethod,
    pub path: String,
    pub headers: Vec<HttpHeader>,
    pub body: Vec<u8>,
    pub timeout: Duration,
    pub response_limit: usize,
}

impl HttpRequest {
    pub fn new(endpoint: HttpEndpoint, method: HttpMethod, path: impl Into<String>) -> Self {
        Self {
            endpoint,
            method,
            path: path.into(),
            headers: Vec::new(),
            body: Vec::new(),
            timeout: DEFAULT_TIMEOUT,
            response_limit: DEFAULT_RESPONSE_LIMIT,
        }
    }

    pub fn json(mut self, body: Vec<u8>) -> Self {
        self.headers.push(HttpHeader {
            name: "content-type".to_owned(),
            value: "application/json".to_owned(),
        });
        self.body = body;
        self
    }

    pub fn validate(&self) -> Result<(), HttpError> {
        validate_relative_path(&self.path)?;
        if self.timeout.is_zero() || self.response_limit == 0 {
            return Err(HttpError::InvalidPolicy);
        }
        for header in &self.headers {
            validate_header(header)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<HttpHeader>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpError {
    InvalidEndpoint,
    EndpointNotAllowed,
    InvalidPath,
    InvalidHeader,
    InvalidPolicy,
    RedirectDenied { status: u16 },
    ResponseTooLarge { limit: usize },
    Timeout,
    Offline,
    Protocol,
    Deferred(ClientTarget),
}

impl fmt::Display for HttpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEndpoint => formatter.write_str("the endpoint is not a valid origin"),
            Self::EndpointNotAllowed => {
                formatter.write_str("the endpoint is not allowed for this build")
            }
            Self::InvalidPath => {
                formatter.write_str("the request path is not a safe relative path")
            }
            Self::InvalidHeader => formatter.write_str("the request contains an invalid header"),
            Self::InvalidPolicy => {
                formatter.write_str("the request policy must have non-zero bounds")
            }
            Self::RedirectDenied { status } => {
                write!(formatter, "HTTP redirect {status} was denied")
            }
            Self::ResponseTooLarge { limit } => {
                write!(formatter, "HTTP response exceeded {limit} bytes")
            }
            Self::Timeout => formatter.write_str("HTTP request timed out"),
            Self::Offline => formatter.write_str("HTTP service is unavailable"),
            Self::Protocol => formatter.write_str("HTTP protocol error"),
            Self::Deferred(target) => {
                write!(formatter, "HTTP transport for {target:?} is deferred")
            }
        }
    }
}

impl std::error::Error for HttpError {}

pub type HttpFuture<'a> = Pin<Box<dyn Future<Output = Result<HttpResponse, HttpError>> + 'a>>;

pub trait HttpHost {
    fn execute(&self, request: HttpRequest) -> HttpFuture<'_>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EndpointOrigin {
    value: String,
}

impl EndpointOrigin {
    fn parse(value: &str, release: bool) -> Result<Self, HttpError> {
        let value = value.trim();
        let (scheme, authority) = value.split_once("://").ok_or(HttpError::InvalidEndpoint)?;
        if authority.is_empty()
            || authority.contains(['/', '?', '#', '\\', '@'])
            || !matches!(scheme, "http" | "https")
        {
            return Err(HttpError::InvalidEndpoint);
        }
        let host = authority_host(authority).ok_or(HttpError::InvalidEndpoint)?;
        let loopback = is_loopback(host);
        if scheme != "https" && (release || !loopback) {
            return Err(HttpError::EndpointNotAllowed);
        }
        Ok(Self {
            value: value.trim_end_matches('/').to_owned(),
        })
    }

    fn join(&self, path: &str) -> Result<String, HttpError> {
        validate_relative_path(path)?;
        Ok(format!("{}{path}", self.value))
    }
}

fn authority_host(authority: &str) -> Option<&str> {
    if let Some(rest) = authority.strip_prefix('[') {
        let close = rest.find(']')?;
        let host = &rest[..close];
        let suffix = &rest[close + 1..];
        if !suffix.is_empty()
            && (!suffix.starts_with(':')
                || suffix[1..].is_empty()
                || !suffix[1..].bytes().all(|b| b.is_ascii_digit()))
        {
            return None;
        }
        return Some(host);
    }
    let mut parts = authority.split(':');
    let host = parts.next()?;
    let port = parts.next();
    if parts.next().is_some()
        || host.is_empty()
        || port.is_some_and(|value| value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    Some(host)
}

fn is_loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1"
}

fn validate_relative_path(path: &str) -> Result<(), HttpError> {
    let lower = path.to_ascii_lowercase();
    if path.is_empty()
        || !path.starts_with('/')
        || path.starts_with("//")
        || path.contains(['\\', '\0', '\r', '\n', '#'])
        || lower.contains("://")
        || lower.contains("%2f")
        || lower.contains("%5c")
    {
        return Err(HttpError::InvalidPath);
    }
    Ok(())
}

fn validate_header(header: &HttpHeader) -> Result<(), HttpError> {
    let valid_name = !header.name.is_empty()
        && header
            .name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
    if !valid_name || header.value.contains(['\r', '\n', '\0']) {
        return Err(HttpError::InvalidHeader);
    }
    Ok(())
}

/// Validated native endpoint allowlist. Requests select a role rather than an
/// arbitrary URL, so callers cannot escape the configured auth/public origins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeHttpPolicy {
    auth: EndpointOrigin,
    public: EndpointOrigin,
    grpc: EndpointOrigin,
}

impl NativeHttpPolicy {
    pub fn from_config(config: &NativeEndpointConfig) -> Result<Self, HttpError> {
        Self::with_release_mode(config, !cfg!(debug_assertions))
    }

    pub fn with_release_mode(
        config: &NativeEndpointConfig,
        release: bool,
    ) -> Result<Self, HttpError> {
        Ok(Self {
            auth: EndpointOrigin::parse(&config.auth_http, release)?,
            public: EndpointOrigin::parse(&config.public_http, release)?,
            grpc: EndpointOrigin::parse(&config.grpc, release)?,
        })
    }

    pub fn request_url(&self, request: &HttpRequest) -> Result<String, HttpError> {
        request.validate()?;
        match request.endpoint {
            HttpEndpoint::Auth => self.auth.join(&request.path),
            HttpEndpoint::Public => self.public.join(&request.path),
        }
    }

    pub fn grpc_endpoint(&self) -> &str {
        &self.grpc.value
    }
}

/// Web requests are always relative same-origin BFF requests. Absolute and
/// protocol-relative URLs are rejected before reaching Fetch.
#[derive(Debug, Clone, Default)]
pub struct WebSameOriginPolicy;

impl WebSameOriginPolicy {
    pub fn request_path(&self, request: &HttpRequest) -> Result<String, HttpError> {
        request.validate()?;
        if request.path.starts_with("/api/v2/") {
            return Ok(request.path.clone());
        }
        let prefix = match request.endpoint {
            HttpEndpoint::Auth => "/api/v2/auth",
            HttpEndpoint::Public => "/api/v2/public",
        };
        Ok(format!("{prefix}{}", request.path))
    }
}

/// Browser Fetch adapter. It deliberately uses typed Web APIs: credentials
/// remain `include` for the same-origin HttpOnly BFF cookie and redirects are
/// rejected by Fetch itself as well as by response validation.
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Default)]
pub struct WebFetchHttpHost {
    policy: WebSameOriginPolicy,
}

#[cfg(target_arch = "wasm32")]
impl HttpHost for WebFetchHttpHost {
    fn execute(&self, request: HttpRequest) -> HttpFuture<'_> {
        use wasm_bindgen::{JsCast, JsValue};
        use wasm_bindgen_futures::JsFuture;

        Box::pin(async move {
            let path = self.policy.request_path(&request)?;
            let signal = web_sys::AbortSignal::timeout_with_u32(
                request.timeout.as_millis().min(u32::MAX as u128) as u32,
            );
            let init = web_sys::RequestInit::new();
            init.set_method(request.method.as_str());
            init.set_mode(web_sys::RequestMode::SameOrigin);
            init.set_credentials(web_sys::RequestCredentials::Include);
            init.set_redirect(web_sys::RequestRedirect::Error);
            init.set_signal(Some(&signal));
            if !request.body.is_empty() {
                let bytes = js_sys::Uint8Array::from(request.body.as_slice());
                init.set_body(&JsValue::from(bytes));
            }
            let fetch_request = web_sys::Request::new_with_str_and_init(&path, &init)
                .map_err(|_| HttpError::Protocol)?;
            for header in &request.headers {
                fetch_request
                    .headers()
                    .set(&header.name, &header.value)
                    .map_err(|_| HttpError::InvalidHeader)?;
            }
            let result = web_sys::window()
                .ok_or(HttpError::Offline)?
                .fetch_with_request(&fetch_request);
            let response = JsFuture::from(result).await.map_err(|_| {
                if signal.aborted() {
                    HttpError::Timeout
                } else {
                    HttpError::Offline
                }
            })?;
            let response: web_sys::Response =
                response.dyn_into().map_err(|_| HttpError::Protocol)?;
            let status = response.status();
            validate_response_status(status)?;
            if response
                .headers()
                .get("content-length")
                .ok()
                .flatten()
                .and_then(|value| value.parse::<usize>().ok())
                .is_some_and(|size| size > request.response_limit)
            {
                return Err(HttpError::ResponseTooLarge {
                    limit: request.response_limit,
                });
            }
            let buffer = JsFuture::from(response.array_buffer().map_err(|_| HttpError::Protocol)?)
                .await
                .map_err(|_| HttpError::Protocol)?;
            let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
            let mut body = Vec::with_capacity(bytes.len().min(request.response_limit));
            append_response_chunk(&mut body, &bytes, request.response_limit)?;
            Ok(HttpResponse {
                status,
                headers: Vec::new(),
                body,
            })
        })
    }
}

pub fn validate_response_status(status: u16) -> Result<(), HttpError> {
    if (300..400).contains(&status) {
        Err(HttpError::RedirectDenied { status })
    } else {
        Ok(())
    }
}

pub fn append_response_chunk(
    body: &mut Vec<u8>,
    chunk: &[u8],
    limit: usize,
) -> Result<(), HttpError> {
    if chunk.len() > limit.saturating_sub(body.len()) {
        return Err(HttpError::ResponseTooLarge { limit });
    }
    body.extend_from_slice(chunk);
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub struct DeferredHttpHost {
    target: ClientTarget,
}

impl DeferredHttpHost {
    pub const fn for_target(target: ClientTarget) -> (Self, CapabilityStatus) {
        (Self { target }, CapabilityStatus::Deferred)
    }

    pub const fn windows() -> (Self, CapabilityStatus) {
        Self::for_target(ClientTarget::Windows)
    }

    pub const fn linux() -> (Self, CapabilityStatus) {
        Self::for_target(ClientTarget::Linux)
    }

    pub const fn ios() -> (Self, CapabilityStatus) {
        Self::for_target(ClientTarget::Ios)
    }

    pub const fn android() -> (Self, CapabilityStatus) {
        Self::for_target(ClientTarget::Android)
    }
}

impl HttpHost for DeferredHttpHost {
    fn execute(&self, _request: HttpRequest) -> HttpFuture<'_> {
        let target = self.target;
        Box::pin(async move { Err(HttpError::Deferred(target)) })
    }
}

#[cfg(all(
    not(target_arch = "wasm32"),
    any(feature = "server", feature = "native", feature = "web")
))]
pub struct ReqwestHttpHost {
    client: reqwest::Client,
    policy: NativeHttpPolicy,
}

#[cfg(all(
    not(target_arch = "wasm32"),
    any(feature = "server", feature = "native", feature = "web")
))]
impl ReqwestHttpHost {
    pub fn new(config: &NativeEndpointConfig) -> Result<Self, HttpError> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| HttpError::Protocol)?;
        Ok(Self {
            client,
            policy: NativeHttpPolicy::from_config(config)?,
        })
    }
}

#[cfg(all(
    not(target_arch = "wasm32"),
    any(feature = "server", feature = "native", feature = "web")
))]
impl HttpHost for ReqwestHttpHost {
    fn execute(&self, request: HttpRequest) -> HttpFuture<'_> {
        Box::pin(async move {
            let url = self.policy.request_url(&request)?;
            let method = reqwest::Method::from_bytes(request.method.as_str().as_bytes())
                .map_err(|_| HttpError::Protocol)?;
            let mut builder = self.client.request(method, url).timeout(request.timeout);
            for header in &request.headers {
                builder = builder.header(&header.name, &header.value);
            }
            let response = builder
                .body(request.body)
                .send()
                .await
                .map_err(map_reqwest_error)?;
            let status = response.status().as_u16();
            validate_response_status(status)?;
            if response
                .content_length()
                .is_some_and(|size| size > request.response_limit as u64)
            {
                return Err(HttpError::ResponseTooLarge {
                    limit: request.response_limit,
                });
            }
            let headers = response
                .headers()
                .iter()
                .filter_map(|(name, value)| {
                    value.to_str().ok().map(|value| HttpHeader {
                        name: name.as_str().to_owned(),
                        value: value.to_owned(),
                    })
                })
                .collect();
            let mut response = response;
            let mut body = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(map_reqwest_error)? {
                append_response_chunk(&mut body, &chunk, request.response_limit)?;
            }
            Ok(HttpResponse {
                status,
                headers,
                body,
            })
        })
    }
}

#[cfg(all(
    not(target_arch = "wasm32"),
    any(feature = "server", feature = "native", feature = "web")
))]
fn map_reqwest_error(error: reqwest::Error) -> HttpError {
    if error.is_timeout() {
        HttpError::Timeout
    } else if error.is_connect() {
        HttpError::Offline
    } else {
        HttpError::Protocol
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn native_grpc_channel(
    policy: &NativeHttpPolicy,
) -> Result<tonic::transport::Channel, HttpError> {
    tonic::transport::Endpoint::from_shared(policy.grpc_endpoint().to_owned())
        .map(|endpoint| endpoint.connect_lazy())
        .map_err(|_| HttpError::InvalidEndpoint)
}

#[cfg(target_arch = "wasm32")]
pub fn web_grpc_channel(origin: &str) -> Result<tonic_web_wasm_client::Client, HttpError> {
    let origin = EndpointOrigin::parse(origin, !cfg!(debug_assertions))?;
    Ok(tonic_web_wasm_client::Client::new(format!(
        "{}/api/v2/internal/grpc",
        origin.value
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoints(origin: &str) -> NativeEndpointConfig {
        NativeEndpointConfig {
            auth_http: origin.to_owned(),
            public_http: origin.to_owned(),
            grpc: origin.to_owned(),
        }
    }

    #[test]
    fn release_endpoints_require_https() {
        for origin in [
            "http://127.0.0.1:50052",
            "http://localhost:50052",
            "http://example.test",
        ] {
            assert!(matches!(
                NativeHttpPolicy::with_release_mode(&endpoints(origin), true),
                Err(HttpError::EndpointNotAllowed)
            ));
        }
        assert!(
            NativeHttpPolicy::with_release_mode(&endpoints("https://console.example.test"), true)
                .is_ok()
        );
    }

    #[test]
    fn debug_http_is_limited_to_loopback() {
        assert!(
            NativeHttpPolicy::with_release_mode(&endpoints("http://127.0.0.1:50052"), false)
                .is_ok()
        );
        assert!(
            NativeHttpPolicy::with_release_mode(&endpoints("http://[::1]:50052"), false).is_ok()
        );
        assert!(matches!(
            NativeHttpPolicy::with_release_mode(&endpoints("http://192.168.1.5:50052"), false),
            Err(HttpError::EndpointNotAllowed)
        ));
    }

    #[test]
    fn native_requests_cannot_escape_the_endpoint_allowlist() {
        let policy =
            NativeHttpPolicy::with_release_mode(&endpoints("https://console.example.test"), true)
                .expect("policy");
        let good = HttpRequest::new(HttpEndpoint::Auth, HttpMethod::Post, "/login");
        assert_eq!(
            policy.request_url(&good).as_deref(),
            Ok("https://console.example.test/login")
        );
        for path in [
            "https://evil.test/login",
            "//evil.test/login",
            "/ok%2f..%2fsecret",
            "/bad\\path",
        ] {
            let request = HttpRequest::new(HttpEndpoint::Auth, HttpMethod::Post, path);
            assert_eq!(policy.request_url(&request), Err(HttpError::InvalidPath));
        }
    }

    #[test]
    fn web_policy_only_produces_same_origin_bff_paths() {
        let policy = WebSameOriginPolicy;
        let auth = HttpRequest::new(HttpEndpoint::Auth, HttpMethod::Post, "/login");
        let public = HttpRequest::new(HttpEndpoint::Public, HttpMethod::Get, "/branding");
        assert_eq!(
            policy.request_path(&auth).as_deref(),
            Ok("/api/v2/auth/login")
        );
        assert_eq!(
            policy.request_path(&public).as_deref(),
            Ok("/api/v2/public/branding")
        );
        let cross_origin = HttpRequest::new(
            HttpEndpoint::Public,
            HttpMethod::Get,
            "https://evil.test/data",
        );
        assert_eq!(
            policy.request_path(&cross_origin),
            Err(HttpError::InvalidPath)
        );
    }

    #[test]
    fn redirects_are_denied() {
        assert_eq!(
            validate_response_status(302),
            Err(HttpError::RedirectDenied { status: 302 })
        );
        assert!(validate_response_status(200).is_ok());
    }

    #[test]
    fn streaming_response_limit_is_enforced_before_append() {
        let mut body = b"1234".to_vec();
        assert_eq!(
            append_response_chunk(&mut body, b"567", 6),
            Err(HttpError::ResponseTooLarge { limit: 6 })
        );
        assert_eq!(body, b"1234");
    }

    #[test]
    fn zero_timeout_and_response_limit_are_rejected() {
        let mut request = HttpRequest::new(HttpEndpoint::Auth, HttpMethod::Get, "/session");
        request.timeout = Duration::ZERO;
        assert_eq!(request.validate(), Err(HttpError::InvalidPolicy));
        request.timeout = DEFAULT_TIMEOUT;
        request.response_limit = 0;
        assert_eq!(request.validate(), Err(HttpError::InvalidPolicy));
    }

    #[test]
    fn future_target_factories_are_truthfully_deferred() {
        for (host, status) in [
            DeferredHttpHost::windows(),
            DeferredHttpHost::linux(),
            DeferredHttpHost::ios(),
            DeferredHttpHost::android(),
        ] {
            assert_eq!(status, CapabilityStatus::Deferred);
            assert!(matches!(
                host.target,
                ClientTarget::Windows
                    | ClientTarget::Linux
                    | ClientTarget::Ios
                    | ClientTarget::Android
            ));
        }
    }

    #[cfg(feature = "server")]
    fn one_shot_server(
        response: &'static [u8],
        delay: Duration,
    ) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("test server address");
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut request = [0_u8; 1_024];
            let _ = stream.read(&mut request);
            std::thread::sleep(delay);
            let _ = stream.write_all(response);
        });
        (format!("http://{address}"), thread)
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn reqwest_adapter_denies_redirects() {
        let (origin, thread) = one_shot_server(
            b"HTTP/1.1 302 Found\r\nLocation: https://example.test/\r\nContent-Length: 0\r\n\r\n",
            Duration::ZERO,
        );
        let host = ReqwestHttpHost::new(&endpoints(&origin)).expect("debug loopback host");
        let result = host
            .execute(HttpRequest::new(
                HttpEndpoint::Auth,
                HttpMethod::Get,
                "/session",
            ))
            .await;
        assert!(matches!(
            result,
            Err(HttpError::RedirectDenied { status: 302 })
        ));
        thread.join().expect("test server");
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn reqwest_adapter_enforces_timeout() {
        let (origin, thread) = one_shot_server(
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n",
            Duration::from_millis(100),
        );
        let host = ReqwestHttpHost::new(&endpoints(&origin)).expect("debug loopback host");
        let mut request = HttpRequest::new(HttpEndpoint::Auth, HttpMethod::Get, "/session");
        request.timeout = Duration::from_millis(10);
        assert_eq!(host.execute(request).await, Err(HttpError::Timeout));
        thread.join().expect("test server");
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn reqwest_adapter_reports_offline_connect_failure() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve unused port");
        let origin = format!(
            "http://{}",
            listener.local_addr().expect("reserved address")
        );
        drop(listener);
        let host = ReqwestHttpHost::new(&endpoints(&origin)).expect("debug loopback host");
        let mut request = HttpRequest::new(HttpEndpoint::Auth, HttpMethod::Get, "/session");
        request.timeout = Duration::from_secs(1);
        assert_eq!(host.execute(request).await, Err(HttpError::Offline));
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn reqwest_adapter_rejects_oversized_response() {
        let (origin, thread) = one_shot_server(
            b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\n\r\n1234567",
            Duration::ZERO,
        );
        let host = ReqwestHttpHost::new(&endpoints(&origin)).expect("debug loopback host");
        let mut request = HttpRequest::new(HttpEndpoint::Public, HttpMethod::Get, "/branding");
        request.response_limit = 6;
        assert_eq!(
            host.execute(request).await,
            Err(HttpError::ResponseTooLarge { limit: 6 })
        );
        thread.join().expect("test server");
    }
}
