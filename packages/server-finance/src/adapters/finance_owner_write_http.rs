//! Owner-local axum ingress for the native Finance owner-write slice.
//!
//! Transport facts only: exact registered Holding host/origin (browser route),
//! one bounded Idempotency-Key, a typed allow-listed `kind` and caller-owned
//! positional values. The router never resolves a session or mints authority;
//! composition resolves the current Company and validates the wrapped context.
//! The command executes the admitted DML in Rust on the caller-owned
//! serializable unit, with receipt/audit/outbox effects in that same unit. The
//! reply carries only an outcome and a trusted no-op sequencing statement for
//! the caller's existing batch order; no business SQL is returned or authored
//! in TypeScript.
use crate::application::finance_owner_write::{OwnerWriteFuture, OwnerWriteOutcome};
use crate::domain::finance_owner_write::{invalid, supported, validate};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, json, post,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

const BODY_LIMIT: usize = 512 * 1024;

/// One owner write command. `kind` is an owner allow-listed discriminator and
/// `values` are positional arguments, never SQL text.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnerWriteCommand {
    pub kind: String,
    pub values: Vec<Value>,
    #[serde(default)]
    pub now: i64,
}

pub type FinanceOwnerWriteFuture<'a> = OwnerWriteFuture<'a, OwnerWriteOutcome>;

/// Trusted no-op sequencing statement returned after the write has executed in
/// Rust. It carries no business SQL and exists only so the TS caller can keep
/// its receipt/audit/outbox batch in the original order.
pub const SEQUENCING_STATEMENT: &str = "SELECT 1";

pub struct FinanceOwnerWriteRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: &'a OwnerWriteCommand,
    pub key: &'a str,
}

/// Composition-owned session/context resolution. The owner router never mints
/// an actor, Company or authority.
pub trait FinanceOwnerWriteBackend: Send + Sync {
    fn execute<'a>(&'a self, request: FinanceOwnerWriteRequest<'a>) -> FinanceOwnerWriteFuture<'a>;
}

#[derive(Clone, Debug, Serialize)]
pub struct FinanceOwnerWriteResponse {
    pub data: OwnerWriteOutcome,
}

#[derive(Clone)]
pub struct FinanceOwnerWriteTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn FinanceOwnerWriteBackend>,
}

pub struct FinanceOwnerWriteHttpState {
    targets: BTreeMap<(String, String), FinanceOwnerWriteTarget>,
    company_targets: BTreeMap<String, Arc<dyn FinanceOwnerWriteBackend>>,
    /// Present only for a single registered Company. A static one-Company
    /// deployment keeps working when the private caller's Company path is a
    /// data fixture, while multiple targets stay fail-closed.
    default_company: Option<Arc<dyn FinanceOwnerWriteBackend>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unique_header<'a>(
    headers: &'a HeaderMap,
    name: HeaderName,
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

fn command_key(headers: &HeaderMap) -> Result<String, BusinessError> {
    let key = unique_header(headers, HeaderName::from_static("idempotency-key"))?
        .filter(|key| {
            (8..=180).contains(&key.len())
                && key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'.' | b'-'))
        })
        .ok_or_else(invalid)?
        .to_owned();
    Ok(key)
}

fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        !rest.is_empty()
            && !rest
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
    }) || origin
        .strip_prefix("http://127.0.0.1:")
        .is_some_and(|port| {
            !port.is_empty() && port.len() <= 5 && port.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn secured(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

fn json_response(status: StatusCode, value: Value) -> Response {
    let mut response = (status, Json(value)).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    secured(response)
}

fn error_response(error: BusinessError) -> Response {
    let status = match error.kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    json_response(
        status,
        json!({"error": {"code": error.code, "message": error.message}}),
    )
}

async fn decode(request: Request) -> Result<OwnerWriteCommand, BusinessError> {
    if request.uri().query().is_some() {
        return Err(invalid());
    }
    let content_type = unique_header(request.headers(), header::CONTENT_TYPE)?;
    if content_type != Some("application/json") {
        return Err(invalid());
    }
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| invalid())?;
    if bytes.is_empty() || bytes.len() > BODY_LIMIT {
        return Err(invalid());
    }
    let command: OwnerWriteCommand = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if !supported(&command.kind) {
        return Err(invalid());
    }
    validate(&command.kind, &command.values)?;
    Ok(command)
}

fn target<'a>(
    state: &'a FinanceOwnerWriteHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a FinanceOwnerWriteTarget, BusinessError> {
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
    match unique_header(headers, header::ORIGIN)? {
        Some(origin) if origin == target.origin => Ok(()),
        _ => Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the application origin.",
        )),
    }
    .map(|()| target)
}

async fn write(
    State(state): State<Arc<FinanceOwnerWriteHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default()
        .to_owned();
    let result = async {
        let target = target(&state, &headers, &slug)?;
        let key = command_key(&headers)?;
        let command = decode(request).await?;
        target
            .backend
            .execute(FinanceOwnerWriteRequest {
                host: &host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: &command,
                key: key.as_str(),
            })
            .await
    }
    .await;
    match result {
        Ok(outcome) => json_response(
            StatusCode::OK,
            json!({"data": {"ok": true, "kind": outcome.kind, "rows": outcome.rows,
                    "sequencing": {"sql": SEQUENCING_STATEMENT, "values": []}}}),
        ),
        Err(error) => error_response(error),
    }
}

/// Company-keyed system route for workers holding a Company id but no browser
/// WorkContext. Reachable only on the private native API.
async fn company_write(
    State(state): State<Arc<FinanceOwnerWriteHttpState>>,
    Path(company_id): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let backend = state
            .company_targets
            .get(&company_id)
            .or(state.default_company.as_ref())
            .ok_or_else(missing)?;
        let key = command_key(request.headers())?;
        let headers = request.headers().clone();
        let command = decode(request).await?;
        backend
            .execute(FinanceOwnerWriteRequest {
                host: "",
                holding_slug: "",
                headers: &headers,
                command: &command,
                key: key.as_str(),
            })
            .await
    }
    .await;
    match result {
        Ok(outcome) => json_response(
            StatusCode::OK,
            json!({"data": {"ok": true, "kind": outcome.kind, "rows": outcome.rows,
                    "sequencing": {"sql": SEQUENCING_STATEMENT, "values": []}}}),
        ),
        Err(error) => error_response(error),
    }
}

/// Browser-facing router: exact registered Holding host/origin + one key.
pub fn owner_write_router(targets: Vec<FinanceOwnerWriteTarget>) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        let host = target.host.split(':').next().unwrap_or_default().to_owned();
        if host.is_empty()
            || target.holding_slug.is_empty()
            || target.holding_slug.len() > 128
            || !target.holding_slug.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
            })
            || !valid_origin(&target.origin)
            || configured
                .insert((host, target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(invalid());
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/finance/owner-write",
            post(write),
        )
        .with_state(Arc::new(FinanceOwnerWriteHttpState {
            targets: configured,
            company_targets: BTreeMap::new(),
            default_company: None,
        })))
}

/// System router keyed by exact Company id.
pub fn owner_write_company_router(
    pairs: Vec<(String, Arc<dyn FinanceOwnerWriteBackend>)>,
) -> Result<Router, BusinessError> {
    let mut company_targets = BTreeMap::new();
    for (company, backend) in pairs {
        if company.is_empty() || company_targets.insert(company, backend).is_some() {
            return Err(invalid());
        }
    }
    let default_company = if company_targets.len() == 1 {
        company_targets.values().next().cloned()
    } else {
        None
    };
    Ok(Router::new()
        .route(
            "/api/v1/companies/{company_id}/finance/owner-write",
            post(company_write),
        )
        .with_state(Arc::new(FinanceOwnerWriteHttpState {
            targets: BTreeMap::new(),
            company_targets,
            default_company,
        })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_runtime::public::http_support::body::Body;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeBackend {
        seen: Mutex<Vec<(String, String)>>,
    }

    impl FinanceOwnerWriteBackend for FakeBackend {
        fn execute<'a>(
            &'a self,
            request: FinanceOwnerWriteRequest<'a>,
        ) -> FinanceOwnerWriteFuture<'a> {
            self.seen
                .lock()
                .unwrap()
                .push((request.command.kind.clone(), request.key.to_owned()));
            let kind = request.command.kind.clone();
            Box::pin(async { Ok(OwnerWriteOutcome { kind, rows: 1 }) })
        }
    }

    fn state(target: FinanceOwnerWriteTarget) -> Arc<FinanceOwnerWriteHttpState> {
        Arc::new(FinanceOwnerWriteHttpState {
            targets: BTreeMap::from([((target.host.clone(), target.holding_slug.clone()), target)]),
            company_targets: BTreeMap::new(),
            default_company: None,
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
    fn configuration_rejects_unregistered_or_duplicate_targets() {
        let backend: Arc<dyn FinanceOwnerWriteBackend> = Arc::new(FakeBackend::default());
        let good = FinanceOwnerWriteTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: backend.clone(),
        };
        assert!(owner_write_router(vec![good.clone()]).is_ok());
        let mut bad = good.clone();
        bad.origin = "http://acme.example.test".into();
        assert!(owner_write_router(vec![bad]).is_err());
        assert!(owner_write_router(vec![good.clone(), good]).is_err());
    }

    #[tokio::test]
    async fn write_requires_exact_origin_and_key_and_forwards_kind() {
        let backend = Arc::new(FakeBackend::default());
        let state = state(FinanceOwnerWriteTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: backend.clone(),
        });
        let body = r#"{"kind":"finance_stmt_1","values":[1,"id"],"now":1}"#;
        let response = write(
            State(state.clone()),
            Path("acme".into()),
            post(Some("https://evil.test"), Some("owner-key-0001"), body),
        )
        .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let response = write(
            State(state.clone()),
            Path("acme".into()),
            post(Some("https://acme.example.test"), None, body),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let response = write(
            State(state),
            Path("acme".into()),
            post(
                Some("https://acme.example.test"),
                Some("owner-key-0001"),
                body,
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["data"]["kind"], "finance_stmt_1");
        assert_eq!(payload["data"]["rows"], 1);
        assert_eq!(payload["data"]["sequencing"]["sql"], SEQUENCING_STATEMENT);
        assert_eq!(
            backend.seen.lock().unwrap().as_slice(),
            [("finance_stmt_1".to_owned(), "owner-key-0001".to_owned())]
        );
    }

    #[tokio::test]
    async fn unknown_kinds_are_rejected_before_the_backend() {
        let backend = Arc::new(FakeBackend::default());
        let state = state(FinanceOwnerWriteTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: backend.clone(),
        });
        let response = write(
            State(state),
            Path("acme".into()),
            post(
                Some("https://acme.example.test"),
                Some("owner-key-0002"),
                r#"{"kind":"not_an_owner_write","values":[1]}"#,
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(backend.seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn sql_text_in_values_is_rejected() {
        let backend = Arc::new(FakeBackend::default());
        let state = state(FinanceOwnerWriteTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: backend.clone(),
        });
        let response = write(
            State(state),
            Path("acme".into()),
            post(
                Some("https://acme.example.test"),
                Some("owner-key-0003"),
                r#"{"kind":"finance_stmt_1","values":[{"sql":"DELETE FROM finance_journals"}]}"#,
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(backend.seen.lock().unwrap().is_empty());
    }
}
