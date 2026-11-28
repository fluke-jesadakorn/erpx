//! Owner-local axum router for the three shared public route families.
//!
//! Every request is resolved by its exact `(Host, holding_slug)` target, its
//! Origin, an allow-listed operation and a bounded JSON body. Execution happens
//! in the injected owner backend; this module never reads a plan, never builds
//! SQL and never calls a TypeScript route.

use super::application::{
    NativeRouteBackend, RouteFuture, RouteReply, RouteRequest, missing_key, valid_key,
};
use super::domain::{
    IamOperation, LaneTarget, RouteCookie, RouteFamily, RouteInput, RouteMethod, iam_operation,
    valid_cookie, valid_slug,
};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes,
    header, json,
    routing::{get, post},
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

struct LaneState {
    targets: BTreeMap<(String, String), (LaneTarget, Arc<dyn NativeRouteBackend>)>,
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_IAM_ROUTE_REQUEST_INVALID",
        "Use one bounded owner route request.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_IAM_ROUTE_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
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
        Json(json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn cookie_value(cookie: &RouteCookie) -> HeaderValue {
    let raw = if cookie.delete {
        format!(
            "{}=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax; Priority=High",
            cookie.name
        )
    } else {
        format!(
            "{}={}; Path=/; Max-Age={}; HttpOnly; Secure; SameSite=Lax; Priority=High",
            cookie.name, cookie.value, cookie.max_age
        )
    };
    HeaderValue::from_str(&raw).unwrap_or_else(|_| HeaderValue::from_static(""))
}

fn bounded(reply: RouteReply) -> Response {
    let mut body = json!({"data": reply.data});
    if let Some(context) = reply.context {
        body["context"] = context;
    }
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
    for cookie in reply.cookies.iter().filter(|cookie| valid_cookie(cookie)) {
        response
            .headers_mut()
            .append(header::SET_COOKIE, cookie_value(cookie));
    }
    response
}

/// One shared backend for a single registered Holding. The lead normally
/// mounts `router_for_holdings` so every Holding keeps its own owner service.
pub fn router(
    targets: Vec<LaneTarget>,
    backend: Arc<dyn NativeRouteBackend>,
) -> Result<Router, BusinessError> {
    router_for_holdings(
        targets
            .into_iter()
            .map(|target| (target, backend.clone()))
            .collect(),
    )
}

/// Per-Holding constructor. Duplicated or malformed targets fail closed before
/// any route is built, so one Holding can never reach another Holding's owner.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, Arc<dyn NativeRouteBackend>)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, backend) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_origin(&target.origin)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    (target, backend),
                )
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
            "/api/v1/holdings/{holding_slug}/iam",
            get(iam_root).post(iam_root),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/iam/{*path}",
            get(iam).post(iam),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/assignments/active",
            get(assignments_active).post(assignments_active),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/companies/active",
            post(companies_active),
        )
        .with_state(Arc::new(LaneState {
            targets: configured,
        })))
}

fn valid_origin(origin: &str) -> bool {
    origin
        .strip_prefix("https://")
        .is_some_and(|host| !host.is_empty() && !host.contains('/') && !host.contains('@'))
}

fn unique_header<'a>(
    headers: &'a HeaderMap,
    name: header::HeaderName,
) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

fn target<'a>(
    state: &'a LaneState,
    headers: &HeaderMap,
    slug: &str,
    family: RouteFamily,
    method: RouteMethod,
) -> Result<&'a (LaneTarget, Arc<dyn NativeRouteBackend>), BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|host| !host.is_empty() && host.len() <= 255)
        .ok_or_else(missing)?;
    let found = state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)?;
    let origin = unique_header(headers, header::ORIGIN)?;
    if let Some(origin) = origin {
        if origin != found.0.origin {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the registered Holding origin.",
            ));
        }
    } else if method == RouteMethod::Post
        && matches!(
            family,
            RouteFamily::AssignmentsActive | RouteFamily::CompaniesActive
        )
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the registered Holding origin.",
        ));
    }
    Ok(found)
}

fn method_of(request: &Request) -> RouteMethod {
    if request.method() == axum::http::Method::POST {
        RouteMethod::Post
    } else {
        RouteMethod::Get
    }
}

fn iam_segments(request: &Request) -> Vec<String> {
    request
        .uri()
        .path()
        .split("/iam")
        .nth(1)
        .unwrap_or("")
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
        .collect()
}

async fn run(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
    family: RouteFamily,
    operation: Option<IamOperation>,
) -> Response {
    let result = async move {
        let method = method_of(&request);
        let headers = request.headers().clone();
        let query = request.uri().query().unwrap_or("").to_owned();
        if family == RouteFamily::Iam && operation.is_none() {
            return Err(missing());
        }
        let (target, backend) = target(&state, &headers, &slug, family, method)?;
        if method == RouteMethod::Post && !query.is_empty() {
            return Err(invalid());
        }
        let bytes = to_bytes(request.into_body(), 64 * 1024)
            .await
            .map_err(|_| invalid())?;
        if bytes.len() > 64 * 1024 || (method == RouteMethod::Get && !bytes.is_empty()) {
            return Err(invalid());
        }
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).map_err(|_| invalid())?
        };
        // The active assignment read uses a CAS revision, not a command key.
        // Every other POST is a durable owner command and requires one.
        let key = match method {
            RouteMethod::Post => {
                let key =
                    unique_header(&headers, header::HeaderName::from_static("idempotency-key"))?;
                if family == RouteFamily::AssignmentsActive {
                    key.filter(|key| valid_key(key)).map(str::to_owned)
                } else {
                    let key = key.ok_or_else(missing_key)?;
                    if !valid_key(key) {
                        return Err(invalid());
                    }
                    Some(key.to_owned())
                }
            }
            RouteMethod::Get => None,
        };
        let input = RouteInput {
            method,
            body,
            key,
            query,
        };
        backend
            .execute(RouteRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                family,
                operation,
                method,
                headers: &headers,
                input,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply),
        Err(error) => error_response(error),
    }
}

async fn iam_root(
    state: State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let method = method_of(&request);
    let operation = iam_operation(method, &[]);
    run(state, Path(slug), request, RouteFamily::Iam, operation).await
}

async fn iam(
    state: State<Arc<LaneState>>,
    Path((slug, _wildcard)): Path<(String, String)>,
    request: Request,
) -> Response {
    let method = method_of(&request);
    let path = iam_segments(&request);
    let operation = iam_operation(method, &path);
    run(state, Path(slug), request, RouteFamily::Iam, operation).await
}

async fn assignments_active(
    state: State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    run(
        state,
        Path(slug),
        request,
        RouteFamily::AssignmentsActive,
        None,
    )
    .await
}

async fn companies_active(
    state: State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    run(
        state,
        Path(slug),
        request,
        RouteFamily::CompaniesActive,
        None,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_platform_runtime_server::public::http_support::serve;
    use std::net::SocketAddr;

    struct Probe;

    impl NativeRouteBackend for Probe {
        fn execute<'a>(&'a self, request: RouteRequest<'a>) -> RouteFuture<'a, RouteReply> {
            Box::pin(async move {
                let operation = match request.family {
                    RouteFamily::Iam => format!("{:?}", request.operation),
                    RouteFamily::AssignmentsActive => "assignments-active".to_owned(),
                    RouteFamily::CompaniesActive => "companies-active".to_owned(),
                };
                let mut reply = RouteReply::data(json!({
                    "operation": operation,
                    "host": request.host,
                    "holdingSlug": request.holding_slug,
                    "family": format!("{:?}", request.family),
                    "method": format!("{:?}", request.method),
                    "body": request.input.body,
                    "key": request.input.key,
                    "query": request.input.query,
                    "cookie": request.headers.get("cookie").and_then(|v| v.to_str().ok()),
                    "origin": request.headers.get("origin").and_then(|v| v.to_str().ok()),
                    "context": request.headers.get("x-erpx-context-company").and_then(|v| v.to_str().ok()),
                }));
                if matches!(request.family, RouteFamily::AssignmentsActive) {
                    reply = reply.with_cookie(RouteCookie {
                        name: "erpx_assignment_h001".into(),
                        value: "assignment-1".into(),
                        max_age: 31_536_000,
                        delete: false,
                    });
                }
                if request.family == RouteFamily::CompaniesActive {
                    reply = reply.with_cookie(RouteCookie {
                        name: "erpx_company_h001".into(),
                        value: String::new(),
                        max_age: 0,
                        delete: true,
                    });
                }
                Ok(reply)
            })
        }
    }

    fn target() -> LaneTarget {
        LaneTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
        }
    }

    async fn http(addr: SocketAddr, raw: String) -> (u16, Value, String) {
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
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
        (
            status,
            serde_json::from_str(body).unwrap_or(Value::Null),
            text,
        )
    }

    fn request(method: &str, path: &str, extra: &str) -> String {
        format!(
            "{method} {path} HTTP/1.1\r\nHost: acme.example.test\r\nCookie: __Host-erpx-product=session\r\nOrigin: https://acme.example.test\r\nX-Erpx-Context-Company: company-1\r\n{extra}Connection: close\r\n\r\n"
        )
    }

    fn post(path: &str, key: &str, body: &str) -> String {
        format!(
            "POST {path} HTTP/1.1\r\nHost: acme.example.test\r\nCookie: __Host-erpx-product=session\r\nOrigin: https://acme.example.test\r\nX-Erpx-Context-Company: company-1\r\nContent-Type: application/json\r\nIdempotency-Key: {key}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    async fn serve_probe() -> SocketAddr {
        let router = router(vec![target()], Arc::new(Probe)).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let _server = tokio::spawn(async move {
            let _ = serve(listener, router).await;
        });
        addr
    }

    #[tokio::test]
    async fn native_router_executes_all_three_families_with_payload_parity() {
        let addr = serve_probe().await;
        let (status, body, headers) = http(
            addr,
            request("GET", "/api/v1/holdings/acme/assignments/active", ""),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["data"]["family"], "AssignmentsActive");
        assert_eq!(body["data"]["method"], "Get");
        assert_eq!(body["data"]["host"], "acme.example.test");
        assert_eq!(body["data"]["cookie"], "__Host-erpx-product=session");
        assert!(headers.contains("set-cookie: erpx_assignment_h001=assignment-1;"));
        assert!(headers.contains("cache-control: private, no-store"));

        let (status, body, _) = http(
            addr,
            post(
                "/api/v1/holdings/acme/assignments/active",
                "active-role-key-1",
                r#"{"assignmentId":"assignment-1","revision":3}"#,
            ),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["data"]["method"], "Post");
        assert_eq!(body["data"]["body"]["assignmentId"], "assignment-1");
        assert_eq!(body["data"]["key"], "active-role-key-1");
        assert_eq!(body["data"]["context"], "company-1");

        let (status, body, headers) = http(
            addr,
            post(
                "/api/v1/holdings/acme/companies/active",
                "active-company-key-1",
                r#"{"companyId":"company-2"}"#,
            ),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["data"]["family"], "CompaniesActive");
        assert_eq!(body["data"]["body"]["companyId"], "company-2");
        assert!(headers.contains("set-cookie: erpx_company_h001=;"));
        assert!(headers.contains("Max-Age=0"));

        let (status, body, text) = http(
            addr,
            request("GET", "/api/v1/holdings/acme/iam/overview", ""),
        )
        .await;
        assert_eq!(status, 200, "raw response: {text:?}");
        assert_eq!(body["data"]["operation"], "Some(Overview)");
    }

    #[tokio::test]
    async fn native_router_fails_closed_on_unknown_paths_bodies_and_origins() {
        let addr = serve_probe().await;
        for (method, path, extra, expected) in [
            ("GET", "/api/v1/holdings/acme/iam/unknown", "", 404),
            (
                "POST",
                "/api/v1/holdings/acme/iam/unknown",
                "Idempotency-Key: unknown-key-1\r\n",
                404,
            ),
            ("POST", "/api/v1/holdings/acme/companies/active", "", 422),
            ("POST", "/api/v1/holdings/acme/assignments/active", "", 200),
        ] {
            let (status, body, _) = http(addr, request(method, path, extra)).await;
            assert_eq!(status, expected, "{method} {path}: {body}");
        }
        let (status, _, _) = http(
            addr,
            "GET /api/v1/holdings/acme/iam/overview HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://other.example.test\r\nConnection: close\r\n\r\n".to_string(),
        )
        .await;
        assert_eq!(status, 403);
        let (status, _, _) = http(
            addr,
            "GET /api/v1/holdings/other/iam/overview HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n".to_string(),
        )
        .await;
        assert_eq!(status, 404);
    }

    #[test]
    fn allow_list_has_no_execute_by_default_fallback() {
        for path in [
            vec!["unknown".to_owned()],
            vec![
                "assignments".to_owned(),
                "bad/id".to_owned(),
                "revoke".to_owned(),
            ],
            vec![
                "connections".to_owned(),
                "id".to_owned(),
                "restart".to_owned(),
            ],
        ] {
            assert!(iam_operation(RouteMethod::Get, &path).is_none());
            assert!(iam_operation(RouteMethod::Post, &path).is_none());
        }
        assert_eq!(
            iam_operation(RouteMethod::Post, &["roles".to_owned()]),
            Some(IamOperation::Roles)
        );
        assert_eq!(
            iam_operation(
                RouteMethod::Post,
                &[
                    "access-reviews".to_owned(),
                    "campaign-1".to_owned(),
                    "cancel".to_owned(),
                ]
            ),
            Some(IamOperation::AccessReviewsCancel("campaign-1".into()))
        );
        assert!(!valid_key("short"));
        assert!(valid_key("active-role-key-1"));
        assert!(router_for_holdings(Vec::new()).is_err());
    }
}
