//! Owner-local axum ingress for the native R3 expenses catch-all and preview.
//! Exact Host/Holding/Origin targets, bounded paths, Idempotency-Key, payload
//! envelope, status codes and headers are owned here. Owner SQL is in
//! `postgres`; cross-owner facts are injected as ports, never stubbed.

use super::application::{
    ExpenseAiPort, ExpenseCommandPort, ExpenseOptionsPort, ExpenseOwnerRead, ExpensePreviewPort,
    OwnerFuture,
};
use super::domain::{
    ExpenseActor, ExpensePath, ExpenseTarget, command_key_required, invalid, not_found,
    origin_forbidden, valid_id, valid_slug,
};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::{get, post}, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");
const CONTEXT_ASSIGNMENT: HeaderName = HeaderName::from_static("x-erpx-context-assignment");
const CONTEXT_BRANCH: HeaderName = HeaderName::from_static("x-erpx-context-branch");
const CONTEXT_DEPARTMENT: HeaderName = HeaderName::from_static("x-erpx-context-department");
const CONTEXT_REVIEWER: HeaderName = HeaderName::from_static("x-erpx-context-expense-reviewer");
const KEY_HEADER: HeaderName = HeaderName::from_static("idempotency-key");
const BODY_LIMIT: usize = 64 * 1024;

/// Everything the owner needs from other owners. Composition supplies real
/// implementations; the router fails closed when a target is not configured.
pub struct ExpensePorts {
    pub owner: Arc<dyn ExpenseOwnerRead>,
    pub options: Arc<dyn ExpenseOptionsPort>,
    pub preview: Arc<dyn ExpensePreviewPort>,
    pub commands: Arc<dyn ExpenseCommandPort>,
    pub ai: Arc<dyn ExpenseAiPort>,
}

struct AppState {
    targets: BTreeMap<(String, String), (String, Arc<ExpensePorts>)>,
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
    ).into_response()
}

fn bounded(mut response: Response) -> Response {
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response.headers_mut().insert(name, HeaderValue::from_static(value));
    }
    response
}

fn ok(code: StatusCode, data: Value) -> Response {
    bounded((code, Json(json!({"data": data}))).into_response())
}

fn unique(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first.map(|value| value.to_str().map_err(|_| invalid())).transpose()
}

fn required(headers: &HeaderMap, name: HeaderName) -> Result<String, BusinessError> {
    let value = unique(headers, name)?.filter(|v| valid_id(v)).ok_or_else(invalid)?;
    Ok(value.to_owned())
}

fn optional(headers: &HeaderMap, name: HeaderName) -> Result<Option<String>, BusinessError> {
    match unique(headers, name)? {
        Some(value) if !valid_id(value) => Err(invalid()),
        Some(value) => Ok(Some(value.to_owned())),
        None => Ok(None),
    }
}

fn actor(headers: &HeaderMap) -> Result<ExpenseActor, BusinessError> {
    Ok(ExpenseActor {
        company_id: required(headers, CONTEXT_COMPANY)?,
        user_id: required(headers, CONTEXT_USER)?,
        assignment_id: required(headers, CONTEXT_ASSIGNMENT)?,
        branch_id: optional(headers, CONTEXT_BRANCH)?,
        department_id: optional(headers, CONTEXT_DEPARTMENT)?,
        reviewer_read: matches!(unique(headers, CONTEXT_REVIEWER)?, Some("1" | "true")),
    })
}

fn target<'a>(
    state: &'a AppState,
    headers: &HeaderMap,
    slug: &str,
    check_origin: bool,
) -> Result<&'a Arc<ExpensePorts>, BusinessError> {
    let host = unique(headers, header::HOST)?
        .filter(|v| !v.is_empty() && v.len() <= 253)
        .ok_or_else(not_found)?;
    let (origin, ports) = state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(not_found)?;
    if check_origin && unique(headers, header::ORIGIN)?.is_some_and(|value| value != origin) {
        return Err(origin_forbidden());
    }
    Ok(ports)
}

fn key(headers: &HeaderMap) -> Result<String, BusinessError> {
    let value = unique(headers, KEY_HEADER)?
        .filter(|v| !v.is_empty() && v.len() <= 200 && valid_id(v))
        .ok_or_else(command_key_required)?;
    Ok(value.to_owned())
}

async fn body(request: Request) -> Result<Value, BusinessError> {
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| invalid())?;
    serde_json::from_slice(&bytes).map_err(|_| invalid())
}

/// Catch-all subpaths are derived from the URI so the axum handler needs only
/// the `Request` extractor; path segments are never trusted as identifiers.
fn catch_all(request: &Request) -> (&str, &str) {
    let rest = request.uri().path().strip_prefix("/api/v1/holdings/").unwrap_or("");
    let (slug, tail) = rest.split_once('/').unwrap_or((rest, ""));
    let sub = tail.strip_prefix("expenses").unwrap_or("").trim_start_matches('/');
    (slug, sub)
}

fn segments(path: &str) -> Result<Vec<String>, BusinessError> {
    let parts = path
        .split('/')
        .filter(|part| !part.is_empty())
        .map(|part| part.to_owned())
        .collect::<Vec<_>>();
    if parts.len() > super::domain::MAX_PATH_SEGMENTS || parts.iter().any(|p| !super::domain::valid_segment(p)) {
        return Err(invalid());
    }
    Ok(parts)
}

fn configure(
    entries: impl IntoIterator<Item = (ExpenseTarget, Arc<ExpensePorts>)>,
) -> Result<BTreeMap<(String, String), (String, Arc<ExpensePorts>)>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, ports) in entries {
        if target.host.is_empty()
            || target.host.len() > 253
            || !valid_slug(&target.holding_slug)
            || target.origin.is_empty()
            || target.origin.len() > 512
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    (target.origin.clone(), ports),
                )
                .is_some()
        {
            return Err(BusinessError::invalid(
                "PEOPLE_PAYROLL_EXPENSES_CONFIG_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    if configured.is_empty() {
        return Err(BusinessError::invalid(
            "PEOPLE_PAYROLL_EXPENSES_CONFIG_INVALID",
            "Use exact distinct registered Holding targets.",
        ));
    }
    Ok(configured)
}

fn finish(targets: BTreeMap<(String, String), (String, Arc<ExpensePorts>)>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/expenses/preview",
            post(preview),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/expenses",
            get(root_get).post(root_post),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/expenses/{*path}",
            get(path_get).post(path_post),
        )
        .with_state(Arc::new(AppState { targets }))
}

/// Router for the lead to mount on the private native People Payroll axum
/// service. Composition supplies a `PgExpenseOwnerRead` in `ExpensePorts`.
pub fn router(
    targets: Vec<(ExpenseTarget, Arc<ExpensePorts>)>,
) -> Result<Router, BusinessError> {
    Ok(finish(configure(targets)?))
}

/// Same constructor name used by the other native R3 owner slices; the owner
/// reader and every cross-owner port travel in one `ExpensePorts` bundle.
pub fn router_with_ports(
    targets: Vec<(ExpenseTarget, Arc<ExpensePorts>)>,
) -> Result<Router, BusinessError> {
    router(targets)
}

async fn root_get(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async move {
        let ports = target(&state, &headers, &slug, false)?;
        let who = actor(&headers)?;
        let expenses = ports.owner.list(&who).await?;
        let options = ports.options.options(&who).await?;
        Ok::<_, BusinessError>(json!({"expenses": expenses, "options": options}))
    }
    .await;
    match result {
        Ok(data) => ok(StatusCode::OK, data),
        Err(error) => error_response(error),
    }
}

async fn root_post(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async move {
        let ports = target(&state, &headers, &slug, true)?;
        let who = actor(&headers)?;
        let key = key(&headers)?;
        let payload = body(request).await?;
        let path = ExpensePath::Root;
        let data = ports.commands.execute(&who, &path, &payload, &key).await?;
        Ok::<_, BusinessError>(data)
    }
    .await;
    match result {
        Ok(data) => ok(StatusCode::CREATED, data),
        Err(error) => error_response(error),
    }
}

async fn path_get(
    State(state): State<Arc<AppState>>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let (slug, sub) = {
        let (slug, sub) = catch_all(&request);
        (slug.to_owned(), sub.to_owned())
    };
    let result = async move {
        let ports = target(&state, &headers, &slug, false)?;
        let who = actor(&headers)?;
        let parsed = ExpensePath::parse(&segments(&sub)?)?;
        match parsed {
            ExpensePath::Root => Err(not_found()),
            ExpensePath::Detail(id) => Ok(json!(ports.owner.detail(&who, &id).await?)),
            ExpensePath::AiStatus(id) => {
                let payment = payment_query(request.uri().query())?;
                Ok(ports.ai.status(&who, &id, payment.as_deref()).await?)
            }
            _ => Err(not_found()),
        }
    }
    .await;
    match result {
        Ok(data) => ok(StatusCode::OK, data),
        Err(error) => error_response(error),
    }
}

async fn path_post(
    State(state): State<Arc<AppState>>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let (slug, sub) = {
        let (slug, sub) = catch_all(&request);
        (slug.to_owned(), sub.to_owned())
    };
    let result = async move {
        let ports = target(&state, &headers, &slug, true)?;
        let who = actor(&headers)?;
        let key = key(&headers)?;
        let parsed = ExpensePath::from_post(&segments(&sub)?)?;
        if parsed == ExpensePath::Root {
            return Err(not_found());
        }
        let payload = body(request).await?;
        let data = if let ExpensePath::RequestAi(id) = &parsed {
            ports.ai.request(&who, id, &payload, &key).await?
        } else {
            ports.commands.execute(&who, &parsed, &payload, &key).await?
        };
        Ok::<_, BusinessError>(data)
    }
    .await;
    match result {
        Ok(data) => ok(StatusCode::OK, data),
        Err(error) => error_response(error),
    }
}

async fn preview(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async move {
        let ports = target(&state, &headers, &slug, true)?;
        let who = actor(&headers)?;
        let payload = body(request).await?;
        let id = payload.get("id").and_then(Value::as_str);
        ports.preview.preview(&who, &payload, id).await
    }
    .await;
    match result {
        Ok(data) => ok(StatusCode::OK, data),
        Err(error) => error_response(error),
    }
}

/// The retired route read only `payment` from the AI query string.
fn payment_query(query: Option<&str>) -> Result<Option<String>, BusinessError> {
    let mut found = None;
    for pair in query.unwrap_or("").split('&').filter(|p| !p.is_empty()) {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        if name != "payment" || found.replace(value.to_owned()).is_some() {
            return Err(invalid());
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::domain::Decision;
    use crate::domain::expense::ExpenseRow;
    use std::sync::Mutex;

    fn row() -> ExpenseRow {
        ExpenseRow {
            id: "e-1".into(),
            company_id: "acme-co".into(),
            request_kind: "expense".into(),
            payee_party_id: None,
            employee_id: None,
            engagement_id: None,
            branch_id: None,
            department_id: None,
            claimant_user_id: Some("u-1".into()),
            claimant_name: Some("Ada".into()),
            legal_name: Some("Ada".into()),
            expense_date: None,
            merchant: None,
            currency: Some("USD".into()),
            amount_minor: Some(1200),
            tax_minor: 0,
            category_account_id: None,
            receipt_reference: None,
            business_purpose: "trip".into(),
            prepared_by: None,
            approved_by: None,
            status: "draft".into(),
            workflow_version: 2,
            workflow_stage: "draft".into(),
            accounting_json: json!({}),
            invoice_id: None,
            journal_id: None,
            rejection_reason: None,
            reversal_journal_id: None,
            cancelled_at: None,
            submitted_at: None,
            posted_at: None,
            reimbursed_at: None,
            version: 1,
            created_by: "u-1".into(),
            created_at: 1,
            updated_at: 1,
        }
    }

    struct OwnerProbe;
    impl ExpenseOwnerRead for OwnerProbe {
        fn list<'a>(&'a self, _: &'a ExpenseActor) -> OwnerFuture<'a, Vec<ExpenseRow>> {
            Box::pin(async { Ok(vec![row()]) })
        }
        fn detail<'a>(&'a self, _: &'a ExpenseActor, id: &'a str) -> OwnerFuture<'a, ExpenseRow> {
            let mut value = row();
            value.id = id.to_owned();
            Box::pin(async move { Ok(value) })
        }
    }

    macro_rules! probe {
        ($name:ident: $port:ident :: fn $method:ident<'a>($($arg:tt)*)) => {
            struct $name;
            impl $port for $name {
                fn $method<'a>($($arg)*) -> OwnerFuture<'a, Value> {
                    Box::pin(async { Ok(json!({"ok": true})) })
                }
            }
        };
    }

    probe!(OptionsProbe: ExpenseOptionsPort::fn options<'a>(&'a self, _: &'a ExpenseActor));
    probe!(PreviewProbe: ExpensePreviewPort::fn preview<'a>(&'a self, _: &'a ExpenseActor, _: &'a Value, _: Option<&'a str>));
    probe!(CommandProbe: ExpenseCommandPort::fn execute<'a>(&'a self, _: &'a ExpenseActor, _: &'a ExpensePath, _: &'a Value, _: &'a str));
    struct AiProbe;
    impl ExpenseAiPort for AiProbe {
        fn status<'a>(&'a self, _: &'a ExpenseActor, _: &'a str, _: Option<&'a str>) -> OwnerFuture<'a, Value> {
            Box::pin(async { Ok(json!({"ok": true})) })
        }
        fn request<'a>(&'a self, _: &'a ExpenseActor, _: &'a str, _: &'a Value, _: &'a str) -> OwnerFuture<'a, Value> {
            Box::pin(async { Ok(json!({"ok": true})) })
        }
    }

    fn ports() -> Arc<ExpensePorts> {
        Arc::new(ExpensePorts {
            owner: Arc::new(OwnerProbe),
            options: Arc::new(OptionsProbe),
            preview: Arc::new(PreviewProbe),
            commands: Arc::new(CommandProbe),
            ai: Arc::new(AiProbe),
        })
    }

    fn target() -> ExpenseTarget {
        ExpenseTarget {
            host: "acme.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.test".into(),
        }
    }

    fn test_router() -> Router {
        router_with_ports(vec![(target(), ports())]).unwrap()
    }

    #[test]
    fn targets_must_be_exact_and_distinct() {
        assert!(router_with_ports(vec![(target(), ports())]).is_ok());
        assert!(router_with_ports(vec![(target(), ports()), (target(), ports())]).is_err());
        assert!(router_with_ports(vec![]).is_err());
    }

    #[test]
    fn envelopes_and_paths_are_stable() {
        let data = ok(StatusCode::CREATED, json!({"id":"e-1"}));
        assert_eq!(data.status(), StatusCode::CREATED);
        assert_eq!(data.headers()[header::CACHE_CONTROL], "private, no-store");
        assert_eq!(
            ExpensePath::parse(&["e-1".to_owned(), "approve".to_owned()]).unwrap(),
            ExpensePath::Decision("e-1".into(), Decision::Approve)
        );
        assert!(ExpensePath::parse(&["e-1".to_owned(), "nope".to_owned()]).is_err());
        assert_eq!(payment_query(Some("payment=p-1")).unwrap(), Some("p-1".into()));
        assert!(payment_query(Some("payment=a&payment=b")).is_err());
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, String, serde_json::Value) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(raw.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let text = String::from_utf8_lossy(&bytes).to_string();
        let status = text.lines().next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|v| v.parse().ok()).unwrap();
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("").to_owned();
        (status, text, serde_json::from_str(&body).unwrap_or(Value::Null))
    }

    #[tokio::test]
    async fn router_serves_every_route_shape() {
        let router = test_router();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        let headers = "Host: acme.test\r\nOrigin: https://acme.test\r\nx-erpx-context-company: acme-co\r\nx-erpx-context-user: u-1\r\nx-erpx-context-assignment: a-1\r\n";
        let req = |method: &str, path: &str, extra: &str, body: &str| format!(
            "{method} {path} HTTP/1.1\r\n{headers}{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );

        let (code, text, body) = http(addr, req("GET", "/api/v1/holdings/acme/expenses", "", "")).await;
        assert_eq!(code, 200);
        assert!(text.to_ascii_lowercase().contains("cache-control: private, no-store"));
        assert_eq!(body["data"]["expenses"][0]["id"], json!("e-1"));
        assert_eq!(body["data"]["options"]["ok"], json!(true));

        let (code, _, body) = http(addr, req("GET", "/api/v1/holdings/acme/expenses/e-2", "", "")).await;
        assert_eq!(code, 200);
        assert_eq!(body["data"]["id"], json!("e-2"));

        let (code, _, body) = http(addr, req("GET", "/api/v1/holdings/acme/expenses/e-2/ai?payment=p-1", "", "")).await;
        assert_eq!(code, 200);
        assert_eq!(body["data"]["ok"], json!(true));

        let (code, _, body) = http(addr, req("GET", "/api/v1/holdings/acme/expenses/e-2/unknown", "", "")).await;
        assert_eq!(code, 404);
        assert_eq!(body["error"]["code"], "NOT_FOUND");

        let (code, _, body) = http(addr, req("POST", "/api/v1/holdings/acme/expenses", "", "{}")).await;
        assert_eq!(code, 422);
        assert_eq!(body["error"]["code"], "COMMAND_KEY_REQUIRED");

        let (code, _, body) = http(addr, req("POST", "/api/v1/holdings/acme/expenses", "Idempotency-Key: k-1\r\n", "{}")).await;
        assert_eq!(code, 201);
        assert_eq!(body["data"]["ok"], json!(true));

        let (code, _, _) = http(addr, req("POST", "/api/v1/holdings/acme/expenses/e-1/approve", "Idempotency-Key: k-1\r\n", "{}")).await;
        assert_eq!(code, 200);

        let (code, _, body) = http(addr, req("POST", "/api/v1/holdings/acme/expenses/preview", "", "{}")).await;
        assert_eq!(code, 200);
        assert_eq!(body["data"]["ok"], json!(true));

        server.abort();
    }

    #[tokio::test]
    async fn origin_is_checked_on_writes() {
        let router = test_router();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        let raw = "POST /api/v1/holdings/acme/expenses/preview HTTP/1.1\r\nHost: acme.test\r\nOrigin: https://evil.test\r\nx-erpx-context-company: acme-co\r\nx-erpx-context-user: u-1\r\nx-erpx-context-assignment: a-1\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}";
        let (code, _, body) = http(addr, raw.to_owned()).await;
        assert_eq!(code, 403);
        assert_eq!(body["error"]["code"], "ORIGIN_FORBIDDEN");
        server.abort();
    }

}
