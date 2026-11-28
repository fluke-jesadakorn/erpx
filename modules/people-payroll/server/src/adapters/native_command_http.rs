//! Owner-local axum ingress for native people-payroll write commands.
//!
//! Transport facts only: exact registered Holding host/origin, one bounded
//! Idempotency-Key, a typed allow-listed command kind with a full-parity
//! positional payload and a bounded JSON body. The router never resolves a
//! session, mints authority, opens a transaction or holds SQL; those remain
//! composition responsibilities behind [`NativeCommandHttpBackend`]. Writes are
//! executed by the owner Postgres adapter inside the composition unit together
//! with the receipt/audit/outbox ledger, so no TypeScript SQL path exists.
use crate::adapters::native_command_sql::statement;
use crate::domain::native_command::NativeCommand;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, CommandKey, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;
const ORIGIN_HEADER: HeaderName = HeaderName::from_static("origin");
const KEY_HEADER: HeaderName = HeaderName::from_static("idempotency-key");

pub type NativeCommandHttpFuture<'a> =
    Pin<Box<dyn Future<Output = Result<NativeCommandHttpResponse, BusinessError>> + Send + 'a>>;

/// The admitted command handed to composition. `command.kind` has already
/// passed the owner allow-list and `command.values` matches its bind arity.
pub struct NativeCommandHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: &'a NativeCommand,
    pub key: &'a CommandKey,
}

#[derive(Clone, Debug)]
pub struct NativeCommandHttpResponse {
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl NativeCommandHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned actor/authority/transaction adapter.
pub trait NativeCommandHttpBackend: Send + Sync {
    fn execute<'a>(&'a self, request: NativeCommandHttpRequest<'a>) -> NativeCommandHttpFuture<'a>;
}

#[derive(Clone)]
pub struct NativeCommandHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn NativeCommandHttpBackend>,
}

struct AppState {
    targets: BTreeMap<(String, String), NativeCommandHttpTarget>,
    companies: BTreeMap<String, Arc<dyn NativeCommandHttpBackend>>,
}

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "PEOPLE_PAYROLL_NATIVE_COMMAND_INVALID",
        "Use one bounded native people-payroll command.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the registered application origin.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_NATIVE_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

fn valid_origin(origin: &str) -> bool {
    origin.starts_with("https://")
        && origin.len() > 8
        && !origin.ends_with('/')
        && !origin[8..].contains('/')
        && !origin.contains('@')
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
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

fn target<'a>(
    state: &'a AppState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a NativeCommandHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?
        .split(':')
        .next()
        .unwrap_or_default()
        .to_owned();
    let target = state
        .targets
        .get(&(host, slug.to_owned()))
        .ok_or_else(missing)?;
    match unique_header(headers, ORIGIN_HEADER)? {
        Some(origin) if origin == target.origin => Ok(()),
        _ => Err(forbidden()),
    }
    .map(|()| target)
}

fn mutation_key(headers: &HeaderMap) -> Result<CommandKey, BusinessError> {
    unique_header(headers, KEY_HEADER)?
        .ok_or_else(|| {
            BusinessError::invalid(
                "COMMAND_KEY_REQUIRED",
                "Use a valid Idempotency-Key for this action.",
            )
        })
        .and_then(CommandKey::new)
}

async fn bounded_body(request: Request) -> Result<NativeCommand, BusinessError> {
    if request.uri().query().is_some() {
        return Err(invalid());
    }
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| invalid())?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if !value.is_object() {
        return Err(invalid());
    }
    let command = NativeCommand::parse(&value)?;
    if statement(&command.kind).is_none() {
        return Err(invalid());
    }
    Ok(command)
}

async fn command(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let target = target(&state, &headers, &slug)?;
        let key = mutation_key(&headers)?;
        let command = bounded_body(request).await?;
        target
            .backend
            .execute(NativeCommandHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: &command,
                key: &key,
            })
            .await
            .map(|reply| reply.value())
    }
    .await;
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

/// Browser-facing caller-unit read plan. The kind is resolved against the
/// owner's native read registry; the exact SQL is returned for the caller to
/// prepare and run on its own already-open unit. No SQL is accepted from the
/// wire, no write ledger is touched and nothing is executed here.
async fn plan_read(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let _target = target(&state, &headers, &slug)?;
        if request.uri().query().is_some() {
            return Err(invalid());
        }
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| invalid())?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        let command = NativeCommand::parse_read(&value)?;
        if !crate::adapters::native_read_sql::is_read(&command.kind) {
            return Err(invalid());
        }
        let statement = crate::adapters::native_read_sql::read_statement(&command.kind)
            .ok_or_else(invalid)?;
        if command.values.len() != statement.bind_count {
            return Err(invalid());
        }
        Ok(serde_json::json!({
            "statements": [{ "sql": statement.sql, "values": command.values }]
        }))
    }
    .await;
    match result {
        Ok(data) => bounded(serde_json::json!({ "data": data })),
        Err(error) => error_response(error),
    }
}

/// Company-keyed system route for workers holding a Company id but no browser
/// WorkContext. Reachable only on the private native API.
async fn company_command(
    State(state): State<Arc<AppState>>,
    Path(company_id): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let backend = state
            .companies
            .get(&company_id)
            .ok_or_else(missing)?
            .clone();
        let headers = request.headers().clone();
        let key = mutation_key(&headers)?;
        let command = bounded_body(request).await?;
        backend
            .execute(NativeCommandHttpRequest {
                host: "",
                holding_slug: "",
                headers: &headers,
                command: &command,
                key: &key,
            })
            .await
            .map(|reply| reply.value())
    }
    .await;
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

/// Browser-facing router: exact registered Holding host/origin + one key.
pub fn native_command_router(
    targets: Vec<NativeCommandHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        let host = target.host.split(':').next().unwrap_or_default().to_owned();
        if host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_origin(&target.origin)
            || configured
                .insert((host, target.holding_slug.clone()), target)
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
            "/api/v1/holdings/{holding_slug}/people-payroll/native-command",
            post(command),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/people-payroll/command-plan",
            post(plan_read),
        )
        .with_state(Arc::new(AppState {
            targets: configured,
            companies: BTreeMap::new(),
        })))
}

/// Private system router keyed by exact Company id.
pub fn native_command_company_router(
    pairs: Vec<(String, Arc<dyn NativeCommandHttpBackend>)>,
) -> Result<Router, BusinessError> {
    let mut companies = BTreeMap::new();
    for (company, backend) in pairs {
        if company.is_empty() || companies.insert(company, backend).is_some() {
            return Err(configuration_invalid());
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/companies/{company_id}/people-payroll/native-command",
            post(company_command),
        )
        .with_state(Arc::new(AppState {
            targets: BTreeMap::new(),
            companies,
        })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_platform_runtime_server::public::http_support::{StatusCode as Code, body::Body};
    use serde_json::json;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Probe {
        seen: Mutex<Vec<(String, String)>>,
    }
    impl NativeCommandHttpBackend for Probe {
        fn execute<'a>(
            &'a self,
            request: NativeCommandHttpRequest<'a>,
        ) -> NativeCommandHttpFuture<'a> {
            self.seen.lock().unwrap().push((
                request.command.kind.clone(),
                request.key.as_str().to_owned(),
            ));
            Box::pin(async {
                Ok(NativeCommandHttpResponse {
                    data: json!({"kind":"tech_provision_stmt_4","rows":1}),
                    context: json!({}),
                })
            })
        }
    }

    fn state(target: NativeCommandHttpTarget) -> Arc<AppState> {
        Arc::new(AppState {
            targets: BTreeMap::from([((target.host.clone(), target.holding_slug.clone()), target)]),
            companies: BTreeMap::new(),
        })
    }

    fn post(origin: Option<&str>, key: Option<&str>, body: &str) -> Request {
        let mut builder = Request::builder()
            .method("POST")
            .header("host", "acme.example.test")
            .header("content-type", "application/json");
        if let Some(origin) = origin {
            builder = builder.header("origin", origin);
        }
        if let Some(key) = key {
            builder = builder.header("idempotency-key", key);
        }
        builder.body(Body::from(body.to_owned())).unwrap()
    }

    #[test]
    fn configuration_rejects_unregistered_or_invalid_targets() {
        let backend: Arc<dyn NativeCommandHttpBackend> = Arc::new(Probe::default());
        let good = NativeCommandHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: backend.clone(),
        };
        assert!(native_command_router(vec![good.clone()]).is_ok());
        let mut bad = good.clone();
        bad.origin = "http://acme.example.test".into();
        assert!(native_command_router(vec![bad]).is_err());
        assert!(native_command_router(vec![good.clone(), good]).is_err());
    }

    #[tokio::test]
    async fn requires_exact_origin_and_key_and_forwards_admitted_kind() {
        let probe = Arc::new(Probe::default());
        let state = state(NativeCommandHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: probe.clone(),
        });
        let body = r#"{"kind":"tech_provision_stmt_4","values":["id",[]],"now":1}"#;
        let response = command(
            State(state.clone()),
            Path("acme".into()),
            post(Some("https://evil.test"), Some("pp-key-0001"), body),
        )
        .await;
        assert_eq!(response.status(), Code::FORBIDDEN);
        let response = command(
            State(state.clone()),
            Path("acme".into()),
            post(Some("https://acme.example.test"), None, body),
        )
        .await;
        assert_eq!(response.status(), Code::UNPROCESSABLE_ENTITY);
        let response = command(
            State(state),
            Path("acme".into()),
            post(Some("https://acme.example.test"), Some("pp-key-0001"), body),
        )
        .await;
        assert_eq!(response.status(), Code::OK);
        assert_eq!(
            probe.seen.lock().unwrap().as_slice(),
            &[("tech_provision_stmt_4".to_owned(), "pp-key-0001".to_owned())]
        );
    }

    #[tokio::test]
    async fn unknown_kind_is_rejected_before_the_backend() {
        let probe = Arc::new(Probe::default());
        let state = state(NativeCommandHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: probe.clone(),
        });
        let response = command(
            State(state),
            Path("acme".into()),
            post(
                Some("https://acme.example.test"),
                Some("pp-key-0002"),
                r#"{"kind":"not_a_statement","values":[1]}"#,
            ),
        )
        .await;
        assert_eq!(response.status(), Code::UNPROCESSABLE_ENTITY);
        assert!(probe.seen.lock().unwrap().is_empty());
    }
}
