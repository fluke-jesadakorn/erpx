//! Native axum command ingress for the Collaboration owner-write slice.
//!
//! Transport facts only: exact registered Holding host/origin (browser route),
//! one bounded Idempotency-Key, a typed write `kind` and caller-owned values.
//! The router never resolves a session or mints authority; the composition
//! backend resolves the current actor/Company and opens the serializable unit.
//! The returned SQL is executed by the caller in that same unit together with
//! receipt/audit/outbox effects, so no second transaction or TS SQL copy exists.
use crate::adapters::http_ingress::{
    bounded, command_key, error_response, json_body, unique_header, valid_origin,
};
use crate::adapters::notification_statement::NotificationStatement;
use crate::adapters::wf1a7::{supported, system_supported};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, Path, Request, Response, Router, State, header, routing::post,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

/// One owner write command. The `kind` is an owner allow-listed discriminator;
/// `values` are positional arguments and never SQL text.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnerWriteCommand {
    pub kind: String,
    pub values: Vec<Value>,
    #[serde(default)]
    pub now: i64,
    /// Typed table/column selector (room/team/social_draft). Never an identifier.
    #[serde(default)]
    pub target: Option<String>,
    /// Bounded owner-policy SQL fragment (feature rollout / source access).
    #[serde(default)]
    pub predicate: Option<String>,
}

impl OwnerWriteCommand {
    fn validate(&self) -> Result<(), BusinessError> {
        if !supported(&self.kind) || self.values.is_empty() || self.values.len() > 64 {
            return Err(invalid());
        }
        if let Some(target) = &self.target {
            if !matches!(target.as_str(), "room" | "team" | "social_draft") {
                return Err(invalid());
            }
        }
        if let Some(predicate) = &self.predicate {
            if predicate.is_empty()
                || predicate.len() > 4_000
                || predicate.contains(';')
                || predicate.contains("--")
                || predicate.contains("/*")
                || predicate.contains("*/")
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

pub type OwnerWriteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<NotificationStatement, BusinessError>> + Send + 'a>>;

pub struct OwnerWriteRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: &'a OwnerWriteCommand,
    pub key: &'a str,
    /// True only for the private system route (no browser WorkContext).
    pub system: bool,
}

/// Composition-owned session, Company and serializable-unit adapter.
pub trait OwnerWriteBackend: Send + Sync {
    fn statement<'a>(&'a self, request: OwnerWriteRequest<'a>) -> OwnerWriteFuture<'a>;
}

#[derive(Clone)]
pub struct OwnerWriteTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn OwnerWriteBackend>,
}

struct AppState {
    targets: BTreeMap<(String, String), OwnerWriteTarget>,
    company_targets: BTreeMap<String, Arc<dyn OwnerWriteBackend>>,
    system_backend: Option<Arc<dyn OwnerWriteBackend>>,
}

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "COLLABORATION_OWNER_WRITE_INVALID",
        "Review the owner write request.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn target<'a>(
    state: &'a AppState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a OwnerWriteTarget, BusinessError> {
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

fn decode(raw: Value) -> Result<OwnerWriteCommand, BusinessError> {
    let command: OwnerWriteCommand = serde_json::from_value(raw).map_err(|_| invalid())?;
    command.validate()?;
    Ok(command)
}

async fn write(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let target = target(&state, &headers, &slug)?;
        let key = command_key(&headers)?;
        let command = decode(json_body(request).await?)?;
        target
            .backend
            .statement(OwnerWriteRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: &command,
                key: key.as_str(),
                system: false,
            })
            .await
    }
    .await;
    match result {
        Ok(statement) => bounded(statement),
        Err(error) => error_response(error),
    }
}

/// Company-keyed system route for workers holding a Company id but no browser
/// WorkContext. Reachable only on the private native API.
async fn company_write(
    State(state): State<Arc<AppState>>,
    Path(company_id): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let backend = state.company_targets.get(&company_id).ok_or_else(missing)?;
        let key = command_key(&headers)?;
        let command = decode(json_body(request).await?)?;
        backend
            .statement(OwnerWriteRequest {
                host: "",
                holding_slug: "",
                headers: &headers,
                command: &command,
                key: key.as_str(),
                system: false,
            })
            .await
    }
    .await;
    match result {
        Ok(statement) => bounded(statement),
        Err(error) => error_response(error),
    }
}

/// Private system route for owner facts with no browser WorkContext. No Host
/// or Origin is checked (reachable only on the private native API), but the
/// Idempotency-Key is still mandatory and the kind is restricted to statements
/// that carry their own strict row/version guards.
async fn system_write(
    State(state): State<Arc<AppState>>,
    request: Request,
) -> Response {
    let result = async {
        let backend = state
            .system_backend
            .as_ref()
            .ok_or_else(|| {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "COLLABORATION_OWNER_WRITE_UNAVAILABLE",
                    "The system owner write is unavailable.",
                )
            })?;
        let key = command_key(request.headers())?;
        let headers = request.headers().clone();
        let command = decode(json_body(request).await?)?;
        if !system_supported(&command.kind) {
            return Err(invalid());
        }
        backend
            .statement(OwnerWriteRequest {
                host: "",
                holding_slug: "",
                headers: &headers,
                command: &command,
                key: key.as_str(),
                system: true,
            })
            .await
    }
    .await;
    match result {
        Ok(statement) => bounded(statement),
        Err(error) => error_response(error),
    }
}

/// Browser-facing router: exact registered Holding host/origin + one key.
pub fn owner_write_router(targets: Vec<OwnerWriteTarget>) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        let host = target.host.split(':').next().unwrap_or_default().to_owned();
        if host.is_empty()
            || target.holding_slug.is_empty()
            || !valid_origin(&host, &target.origin)
            || configured
                .insert((host, target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "COLLABORATION_OWNER_WRITE_CONFIG_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/collaboration/owner-write",
            post(write),
        )
        .with_state(Arc::new(AppState {
            targets: configured,
            company_targets: BTreeMap::new(),
            system_backend: None,
        })))
}

/// System router keyed by exact Company id.
pub fn owner_write_company_router(
    pairs: Vec<(String, Arc<dyn OwnerWriteBackend>)>,
) -> Result<Router, BusinessError> {
    let mut company_targets = BTreeMap::new();
    for (company, backend) in pairs {
        if company.is_empty() || company_targets.insert(company, backend).is_some() {
            return Err(BusinessError::invalid(
                "COLLABORATION_OWNER_WRITE_CONFIG_INVALID",
                "Use exact distinct companies.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/companies/{company_id}/collaboration/owner-write",
            post(company_write),
        )
        .with_state(Arc::new(AppState {
            targets: BTreeMap::new(),
            company_targets,
            system_backend: None,
        })))
}

/// Private system router. No Host/Origin/Company check; kind allow-list is
/// restricted to self-guarded facts writes and the Idempotency-Key is required.
pub fn owner_write_system_router(backend: Arc<dyn OwnerWriteBackend>) -> Router {
    Router::new()
        .route("/api/v1/system/collaboration/owner-write", post(system_write))
        .with_state(Arc::new(AppState {
            targets: BTreeMap::new(),
            company_targets: BTreeMap::new(),
            system_backend: Some(backend),
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_platform_runtime_server::public::http_support::{StatusCode, body::Body, body::to_bytes};
    use serde_json::json;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeBackend {
        seen: Mutex<Vec<(String, String)>>,
    }

    impl OwnerWriteBackend for FakeBackend {
        fn statement<'a>(&'a self, request: OwnerWriteRequest<'a>) -> OwnerWriteFuture<'a> {
            self.seen
                .lock()
                .unwrap()
                .push((request.command.kind.clone(), request.key.to_owned()));
            Box::pin(async {
                Ok(NotificationStatement {
                    sql: "UPDATE t SET v=? WHERE id=?".to_owned(),
                    values: vec![json!(1), json!("id")],
                })
            })
        }
    }

    fn state(target: OwnerWriteTarget) -> Arc<AppState> {
        Arc::new(AppState {
            targets: BTreeMap::from([((target.host.clone(), target.holding_slug.clone()), target)]),
            company_targets: BTreeMap::new(),
            system_backend: None,
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
        let backend: Arc<dyn OwnerWriteBackend> = Arc::new(FakeBackend::default());
        let good = OwnerWriteTarget {
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
        let state = state(OwnerWriteTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: backend.clone(),
        });
        let body = r#"{"kind":"social_post_version_step","values":[1,"id",2],"now":1}"#;
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
        assert_eq!(payload["data"]["sql"], "UPDATE t SET v=? WHERE id=?");
        assert_eq!(
            backend.seen.lock().unwrap().as_slice(),
            [(
                "social_post_version_step".to_owned(),
                "owner-key-0001".to_owned()
            )]
        );
    }

    #[tokio::test]
    async fn unknown_kinds_are_rejected_before_the_backend() {
        let backend = Arc::new(FakeBackend::default());
        let state = state(OwnerWriteTarget {
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
}
