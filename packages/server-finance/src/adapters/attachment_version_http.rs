//! Owner-local axum ingress for the native Finance attachment version guard.
//!
//! This is the transport counterpart of the retired TypeScript `versionGuard`
//! assertions in the Files attachment service. A company-keyed system caller
//! posts only an allow-listed target type, an identifier, an expected version
//! and already-resolved actor facts. The owning backend executes
//! `assert_attachment_version` or `bump_attachment_version` on one serializable
//! unit; the reply carries a trusted no-op sequencing statement so the caller
//! can keep its existing batch order. No SQL is accepted from the wire and no
//! TypeScript assertion substitutes for this port.
use crate::adapters::attachment_version::{
    AttachmentVersionActor, AttachmentVersionStep, assert_attachment_version,
    bump_attachment_version,
};
use crate::domain::attachment_version::{attachment_version_target, unavailable};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, json, post,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::Deserialize;
use sqlx::PgConnection;
use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::Arc,
};

const BODY_LIMIT: usize = 16 * 1024;
const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

/// Trusted sequencing statement returned after the native guard executes. It
/// carries no business SQL and exists only to preserve the caller's batch order.
pub const SEQUENCING_STATEMENT: &str = "SELECT 1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentVersionOperation {
    Assert,
    Step,
}

pub type AttachmentVersionFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AttachmentVersionStep, BusinessError>> + Send + 'a>>;

/// One owner-owned operation. The composition supplies the Company and the
/// already-authorized actor facts; the owner crate revalidates scope and the
/// allow-listed target before preparing any SQL.
pub struct AttachmentVersionRequest {
    pub operation: AttachmentVersionOperation,
    pub actor: AttachmentVersionActor,
    pub command: CommandKey,
    pub company_id: String,
    pub target_type: String,
    pub target_id: String,
    pub expected_version: i64,
    pub now: i64,
}

pub trait AttachmentVersionBackend: Send + Sync {
    fn execute(&self, request: AttachmentVersionRequest) -> AttachmentVersionFuture<'_>;
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

/// Default Postgres backend: one serializable unit per operation, owned by the
/// caller's pool. It never opens a second pool or mints authority.
pub struct PgAttachmentVersionBackend {
    pool: DatabasePool,
}

impl PgAttachmentVersionBackend {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl AttachmentVersionBackend for PgAttachmentVersionBackend {
    fn execute(&self, request: AttachmentVersionRequest) -> AttachmentVersionFuture<'_> {
        Box::pin(async move {
            let operation = request.operation;
            let actor = request.actor;
            let command = request.command;
            let target_type = request.target_type;
            let target_id = request.target_id;
            let expected_version = request.expected_version;
            let now = request.now;
            PgUnitOfWork::new(self.pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let actor = actor.clone();
                    let command = command.clone();
                    let target_type = target_type.clone();
                    let target_id = target_id.clone();
                    Box::pin(async move {
                        match operation {
                            AttachmentVersionOperation::Assert => {
                                assert_attachment_version(
                                    connection,
                                    &actor,
                                    &target_type,
                                    &target_id,
                                    expected_version,
                                )
                                .await?;
                                Ok(AttachmentVersionStep {
                                    target_type,
                                    target_id,
                                    changed: false,
                                    version: expected_version,
                                })
                            }
                            AttachmentVersionOperation::Step => {
                                bump_attachment_version(
                                    connection,
                                    &actor,
                                    &command,
                                    &target_type,
                                    &target_id,
                                    expected_version,
                                    now,
                                )
                                .await
                            }
                        }
                    })
                })
                .await
                .map_err(business)
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VersionRequest {
    target_type: String,
    target_id: String,
    expected_version: i64,
    user_id: String,
    assignment_id: String,
    role: String,
    scope_kind: String,
    #[serde(default)]
    now: i64,
}

pub struct AttachmentVersionHttpState {
    companies: BTreeMap<String, Arc<dyn AttachmentVersionBackend>>,
    default_company: Option<Arc<dyn AttachmentVersionBackend>>,
}

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "FINANCE_ATTACHMENT_VERSION_REQUEST_INVALID",
        "Use one bounded Finance attachment version operation.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b":_.-".contains(&byte))
}

fn unique_header<'a>(headers: &'a HeaderMap, name: HeaderName) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

fn secured(mut response: Response) -> Response {
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

fn json_response(status: StatusCode, value: serde_json::Value) -> Response {
    let mut response = (status, Json(value)).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
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
    json_response(status, json!({"error":{"code":error.code,"message":error.message}}))
}

async fn decode(request: Request) -> Result<VersionRequest, BusinessError> {
    if request.uri().query().is_some() {
        return Err(invalid());
    }
    if unique_header(request.headers(), header::CONTENT_TYPE)? != Some("application/json") {
        return Err(invalid());
    }
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| invalid())?;
    if bytes.is_empty() || bytes.len() > BODY_LIMIT {
        return Err(invalid());
    }
    let body: VersionRequest = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if attachment_version_target(&body.target_type).is_none()
        || !valid_identifier(&body.target_id)
        || !valid_identifier(&body.user_id)
        || !valid_identifier(&body.assignment_id)
        || body.role.is_empty()
        || body.role.len() > 128
        || !matches!(
            body.scope_kind.as_str(),
            "company" | "holding" | "branch" | "department"
        )
        || body.expected_version <= 0
        || body.now <= 0
    {
        return Err(invalid());
    }
    Ok(body)
}

async fn execute(
    State(state): State<Arc<AttachmentVersionHttpState>>,
    Path(company_id): Path<String>,
    operation: AttachmentVersionOperation,
    request: Request,
) -> Response {
    let result = async {
        if !valid_identifier(&company_id) {
            return Err(invalid());
        }
        let header_company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|company| *company == company_id)
            .ok_or_else(invalid)?
            .to_owned();
        let key = unique_header(request.headers(), HeaderName::from_static("idempotency-key"))?
            .ok_or_else(invalid)?;
        let command = CommandKey::new(key).map_err(|_| invalid())?;
        let backend = state
            .companies
            .get(&header_company)
            .or(state.default_company.as_ref())
            .ok_or_else(missing)?
            .clone();
        let body = decode(request).await?;
        let actor = AttachmentVersionActor {
            user_id: body.user_id,
            assignment_id: body.assignment_id,
            company_id: header_company.clone(),
            role: body.role,
            scope_kind: body.scope_kind,
        };
        backend
            .execute(AttachmentVersionRequest {
                operation,
                actor,
                command,
                company_id: header_company,
                target_type: body.target_type,
                target_id: body.target_id,
                expected_version: body.expected_version,
                now: body.now,
            })
            .await
    }
    .await;
    match result {
        Ok(step) => json_response(
            StatusCode::OK,
            json!({"data":{"ok":true,"targetType":step.target_type,"targetId":step.target_id,
                "changed":step.changed,"version":step.version,
                "sequencing":{"sql":SEQUENCING_STATEMENT,"values":[]}}}),
        ),
        Err(error) => error_response(error),
    }
}

async fn guard(
    state: State<Arc<AttachmentVersionHttpState>>,
    path: Path<String>,
    request: Request,
) -> Response {
    execute(state, path, AttachmentVersionOperation::Assert, request).await
}

async fn step(
    state: State<Arc<AttachmentVersionHttpState>>,
    path: Path<String>,
    request: Request,
) -> Response {
    execute(state, path, AttachmentVersionOperation::Step, request).await
}

/// Company-keyed system router. Composition mounts this on the private native
/// API and supplies one backend per exact registered Company.
pub fn attachment_version_router(
    pairs: Vec<(String, Arc<dyn AttachmentVersionBackend>)>,
) -> Result<Router, BusinessError> {
    let mut companies = BTreeMap::new();
    for (company, backend) in pairs {
        if !valid_identifier(&company) || companies.insert(company, backend).is_some() {
            return Err(invalid());
        }
    }
    let default_company = if companies.len() == 1 {
        companies.values().next().cloned()
    } else {
        None
    };
    Ok(Router::new()
        .route(
            "/api/v1/companies/{company_id}/finance/attachment-version/guard",
            post(guard),
        )
        .route(
            "/api/v1/companies/{company_id}/finance/attachment-version/step",
            post(step),
        )
        .with_state(Arc::new(AttachmentVersionHttpState {
            companies,
            default_company,
        })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_runtime::public::http_support::body::Body;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Probe {
        seen: Mutex<Vec<(AttachmentVersionOperation, String)>>,
    }

    impl AttachmentVersionBackend for Probe {
        fn execute(&self, request: AttachmentVersionRequest) -> AttachmentVersionFuture<'_> {
            self.seen.lock().unwrap().push((request.operation, request.target_type.clone()));
            Box::pin(async move {
                Ok(AttachmentVersionStep {
                    target_type: request.target_type,
                    target_id: request.target_id,
                    changed: false,
                    version: request.expected_version,
                })
            })
        }
    }

    fn state(backend: Arc<dyn AttachmentVersionBackend>) -> Arc<AttachmentVersionHttpState> {
        Arc::new(AttachmentVersionHttpState {
            companies: BTreeMap::from([("company-1".to_owned(), backend)]),
            default_company: None,
        })
    }

    fn request(company: &str, body: &str) -> Request {
        Request::builder()
            .method("POST")
            .header("content-type", "application/json")
            .header("x-erpx-context-company", company)
            .header("idempotency-key", "attachment-key-0001")
            .body(Body::from(body.to_owned()))
            .unwrap()
    }

    const BODY: &str = r#"{"targetType":"finance_journal","targetId":"journal-1","expectedVersion":4,"userId":"user-1","assignmentId":"assignment-1","role":"finance.manager","scopeKind":"holding","now":100}"#;

    #[test]
    fn configuration_rejects_duplicate_and_invalid_companies() {
        let backend: Arc<dyn AttachmentVersionBackend> = Arc::new(Probe::default());
        assert!(attachment_version_router(vec![(
            "company-1".to_owned(),
            backend.clone()
        )])
        .is_ok());
        assert!(attachment_version_router(vec![
            ("company-1".to_owned(), backend.clone()),
            ("company-1".to_owned(), backend)
        ])
        .is_err());
        assert!(attachment_version_router(vec![("company/1".to_owned(), Arc::new(Probe::default()))]).is_err());
    }

    #[tokio::test]
    async fn guard_executes_native_assert_and_returns_sequencing_statement() {
        let backend = Arc::new(Probe::default());
        let state = state(backend.clone());
        let response = guard(
            State(state),
            Path("company-1".to_owned()),
            request("company-1", BODY),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["data"]["ok"], true);
        assert_eq!(payload["data"]["version"], 4);
        assert_eq!(payload["data"]["sequencing"]["sql"], SEQUENCING_STATEMENT);
        assert_eq!(backend.seen.lock().unwrap().as_slice(), [(AttachmentVersionOperation::Assert, "finance_journal".to_owned())]);
    }

    #[tokio::test]
    async fn unregistered_target_and_foreign_company_fail_closed() {
        let backend = Arc::new(Probe::default());
        let state = state(backend.clone());
        let injected = BODY.replace("finance_journal", "finance_journal;DROP TABLE finance_journals");
        let response = guard(
            State(state.clone()),
            Path("company-1".to_owned()),
            request("company-1", &injected),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let response = guard(
            State(state),
            Path("company-1".to_owned()),
            request("company-2", BODY),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(backend.seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn step_executes_native_bump() {
        let backend = Arc::new(Probe::default());
        let state = state(backend.clone());
        let response = step(
            State(state),
            Path("company-1".to_owned()),
            request("company-1", BODY),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(backend.seen.lock().unwrap().as_slice(), [(AttachmentVersionOperation::Step, "finance_journal".to_owned())]);
    }
}
