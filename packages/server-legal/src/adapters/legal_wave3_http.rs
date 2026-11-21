//! Native axum command ingress for the Legal wf3a13 write slice.
//!
//! Transport facts only: typed owner write `kind`, bounded positional values,
//! one Idempotency-Key and (for browser writes) the exact registered Holding
//! Host/Origin. Holding- and Company-keyed variants are private-API system
//! routes for workers that have no browser WorkContext. The composition
//! backend resolves the current actor/Company and the serializable unit; this
//! router never mints session, permission or purchase authority and never
//! executes SQL itself.
use crate::domain::legal_wave3::{LegalWave3Statement, supported};
use erpx_server_runtime::public::http_support::{
    HeaderMap, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;
const KEY_HEADER: header::HeaderName = header::HeaderName::from_static("idempotency-key");
const MAX_VALUES: usize = 64;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LegalWave3CommandBody {
    pub kind: String,
    pub values: Vec<Value>,
    #[serde(default)]
    pub now: i64,
}

pub type LegalWave3Future<'a> =
    Pin<Box<dyn Future<Output = Result<LegalWave3Statement, BusinessError>> + Send + 'a>>;

pub struct LegalWave3Request<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub kind: &'a str,
    pub values: &'a [Value],
    pub now: i64,
    pub key: &'a str,
}

/// Composition-owned session, Company and serializable-unit adapter.
pub trait LegalWave3Backend: Send + Sync {
    fn statement<'a>(&'a self, request: LegalWave3Request<'a>) -> LegalWave3Future<'a>;
}

#[derive(Clone)]
pub struct LegalWave3Target {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn LegalWave3Backend>,
}

struct AppState {
    targets: BTreeMap<(String, String), LegalWave3Target>,
    holding_targets: BTreeMap<String, Arc<dyn LegalWave3Backend>>,
    company_targets: BTreeMap<String, Arc<dyn LegalWave3Backend>>,
}

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "LEGAL_WAVE3_REQUEST_INVALID",
        "Use one bounded Legal owner write request.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "LEGAL_WAVE3_CONFIG_INVALID",
        "Use exact distinct registered targets.",
    )
}

fn unique_header(
    headers: &HeaderMap,
    name: header::HeaderName,
) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

fn valid_origin(value: &str) -> bool {
    value.starts_with("https://")
        && value.len() <= 512
        && !value
            .bytes()
            .any(|byte| matches!(byte, b' ' | b'<' | b'>' | b'\r' | b'\n'))
}

fn decode(bytes: &[u8]) -> Result<LegalWave3CommandBody, BusinessError> {
    let body: LegalWave3CommandBody = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    if !supported(&body.kind) || body.values.is_empty() || body.values.len() > MAX_VALUES {
        return Err(invalid());
    }
    Ok(body)
}

async fn body(request: Request) -> Result<LegalWave3CommandBody, BusinessError> {
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| invalid())?;
    decode(&bytes)
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

fn success(statement: LegalWave3Statement) -> Response {
    let mut response =
        (StatusCode::OK, Json(serde_json::json!({"data": statement}))).into_response();
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, header::HeaderValue::from_static(value));
    }
    response
}

fn browser_target<'a>(
    state: &'a AppState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a LegalWave3Target, BusinessError> {
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
            "Use the registered application origin.",
        )),
    }
    .map(|()| target)
}

async fn browser_write(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let target = browser_target(&state, &headers, &slug)?;
        let key = unique_header(&headers, KEY_HEADER.clone())?
            .filter(|value| !value.is_empty() && value.len() <= 200)
            .ok_or_else(invalid)?;
        let command = body(request).await?;
        target
            .backend
            .statement(LegalWave3Request {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                kind: &command.kind,
                values: &command.values,
                now: command.now,
                key,
            })
            .await
    }
    .await;
    match result {
        Ok(statement) => success(statement),
        Err(error) => error_response(error),
    }
}

async fn holding_write(
    State(state): State<Arc<AppState>>,
    Path(holding_id): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let backend = state.holding_targets.get(&holding_id).ok_or_else(missing)?;
        let key = unique_header(&headers, KEY_HEADER.clone())?
            .filter(|value| !value.is_empty() && value.len() <= 200)
            .ok_or_else(invalid)?;
        let command = body(request).await?;
        backend
            .statement(LegalWave3Request {
                host: "",
                holding_slug: "",
                headers: &headers,
                kind: &command.kind,
                values: &command.values,
                now: command.now,
                key,
            })
            .await
    }
    .await;
    match result {
        Ok(statement) => success(statement),
        Err(error) => error_response(error),
    }
}

async fn company_write(
    State(state): State<Arc<AppState>>,
    Path(company_id): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let backend = state.company_targets.get(&company_id).ok_or_else(missing)?;
        let key = unique_header(&headers, KEY_HEADER.clone())?
            .filter(|value| !value.is_empty() && value.len() <= 200)
            .ok_or_else(invalid)?;
        let command = body(request).await?;
        backend
            .statement(LegalWave3Request {
                host: "",
                holding_slug: "",
                headers: &headers,
                kind: &command.kind,
                values: &command.values,
                now: command.now,
                key,
            })
            .await
    }
    .await;
    match result {
        Ok(statement) => success(statement),
        Err(error) => error_response(error),
    }
}

/// Browser-facing router: exact registered Holding Host/Origin + one key.
pub fn legal_wave3_browser_router(targets: Vec<LegalWave3Target>) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        let host = target.host.split(':').next().unwrap_or_default().to_owned();
        if host.is_empty()
            || target.holding_slug.is_empty()
            || !valid_origin(&target.origin)
            || configured
                .insert((host, target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/legal/wave3-command",
            post(browser_write),
        )
        .with_state(Arc::new(AppState {
            targets: configured,
            holding_targets: BTreeMap::new(),
            company_targets: BTreeMap::new(),
        })))
}

/// Holding-keyed private-API system router for workers without a Company id.
pub fn legal_wave3_holding_router(
    pairs: Vec<(String, Arc<dyn LegalWave3Backend>)>,
) -> Result<Router, BusinessError> {
    let mut holding_targets = BTreeMap::new();
    for (holding, backend) in pairs {
        if holding.is_empty() || holding_targets.insert(holding, backend).is_some() {
            return Err(configuration_invalid());
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_id}/legal/wave3-command-by-holding",
            post(holding_write),
        )
        .with_state(Arc::new(AppState {
            targets: BTreeMap::new(),
            holding_targets,
            company_targets: BTreeMap::new(),
        })))
}

/// Company-keyed private-API system router for workers holding a Company id.
pub fn legal_wave3_company_router(
    pairs: Vec<(String, Arc<dyn LegalWave3Backend>)>,
) -> Result<Router, BusinessError> {
    let mut company_targets = BTreeMap::new();
    for (company, backend) in pairs {
        if company.is_empty() || company_targets.insert(company, backend).is_some() {
            return Err(configuration_invalid());
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/companies/{company_id}/legal/wave3-command",
            post(company_write),
        )
        .with_state(Arc::new(AppState {
            targets: BTreeMap::new(),
            holding_targets: BTreeMap::new(),
            company_targets,
        })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_runtime::public::http_support::body::Body;
    use serde_json::json;

    #[derive(Default)]
    struct FakeBackend;

    impl LegalWave3Backend for FakeBackend {
        fn statement<'a>(&'a self, request: LegalWave3Request<'a>) -> LegalWave3Future<'a> {
            assert_eq!(request.kind, "legal_source_recover");
            Box::pin(async {
                Ok(LegalWave3Statement {
                    sql: "UPDATE legal_law_source_checks SET state='queued'".to_owned(),
                    values: vec![json!(1)],
                })
            })
        }
    }

    fn request(origin: Option<&str>, key: Option<&str>, body: &str) -> Request {
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
    fn configuration_rejects_duplicate_or_invalid_targets() {
        let backend: Arc<dyn LegalWave3Backend> = Arc::new(FakeBackend);
        let target = LegalWave3Target {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: backend.clone(),
        };
        assert!(legal_wave3_browser_router(vec![target.clone()]).is_ok());
        let mut bad = target.clone();
        bad.origin = "http://acme.example.test".into();
        assert!(legal_wave3_browser_router(vec![bad]).is_err());
        assert!(legal_wave3_browser_router(vec![target.clone(), target]).is_err());
        assert!(legal_wave3_holding_router(vec![("acme".into(), backend.clone())]).is_ok());
        assert!(legal_wave3_company_router(vec![("company-1".into(), backend)]).is_ok());
    }

    #[tokio::test]
    async fn browser_write_requires_exact_origin_and_key_and_owns_the_kind() {
        let backend = Arc::new(FakeBackend);
        let state = Arc::new(AppState {
            targets: BTreeMap::from([(
                ("acme.example.test".to_owned(), "acme".to_owned()),
                LegalWave3Target {
                    host: "acme.example.test".into(),
                    holding_slug: "acme".into(),
                    origin: "https://acme.example.test".into(),
                    backend: backend.clone(),
                },
            )]),
            holding_targets: BTreeMap::new(),
            company_targets: BTreeMap::new(),
        });
        let body = r#"{"kind":"legal_source_recover","values":[1],"now":1}"#;
        let response = browser_write(
            State(state.clone()),
            Path("acme".into()),
            request(Some("https://evil.test"), Some("legal-key-0001"), body),
        )
        .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let response = browser_write(
            State(state.clone()),
            Path("acme".into()),
            request(Some("https://acme.example.test"), None, body),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let response = browser_write(
            State(state),
            Path("acme".into()),
            request(
                Some("https://acme.example.test"),
                Some("legal-key-0001"),
                body,
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn unknown_kind_is_rejected_before_the_backend() {
        let backend = Arc::new(FakeBackend);
        let state = Arc::new(AppState {
            targets: BTreeMap::new(),
            holding_targets: BTreeMap::from([(
                "acme".to_owned(),
                backend as Arc<dyn LegalWave3Backend>,
            )]),
            company_targets: BTreeMap::new(),
        });
        let response = holding_write(
            State(state),
            Path("acme".into()),
            request(
                None,
                Some("legal-key-0002"),
                r#"{"kind":"not_a_kind","values":[1]}"#,
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
