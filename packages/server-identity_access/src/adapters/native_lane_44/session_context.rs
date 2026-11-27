//! Loopback-only context-header derivation for the dev gateway.
//!
//! `GET /internal/session-context?holding={slug}` is served on the private API.
//! It accepts only the inbound Cookie/Host/Origin, resolves the current Account
//! session and selected work role through the existing owner ports, and returns
//! the private context values the gateway injects as headers. It never runs a
//! new SQL plan, never executes TypeScript, and never trusts a supplied context
//! header.

use super::application::RouteFuture;
use super::assignments_active::{
    ActiveWorkFn, ActiveWorkRoleView, WorkScopeLease, active_work_role_payload,
};
use crate::adapters::account_work_roles::read_account_work_roles;
use crate::adapters::holding_work_context::HoldingAccountWorkScope;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, json, routing::get,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, TimestampMillis};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// One exact `(Host, Holding slug)` pair. The private API owns the mapping;
/// an unknown pair is a 404 and never reaches an owner read.
#[derive(Clone, Debug)]
pub struct SessionContextTarget {
    pub host: String,
    pub holding_slug: String,
}

pub type WorkFactory = Arc<dyn Fn(i64) -> ActiveWorkFn + Send + Sync>;

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => BusinessError::new(
            FailureKind::Unavailable,
            "ACCOUNT_ACCESS_UNAVAILABLE",
            "Unable to read current account access.",
        ),
    }
}

fn pg_work(now: i64) -> ActiveWorkFn {
    Arc::new(move |connection, scope: &HoldingAccountWorkScope| {
        Box::pin(async move {
            let roles = read_account_work_roles(connection, scope, now, true)
                .await
                .map_err(business)?;
            Ok(ActiveWorkRoleView::from_owner(&roles))
        })
    })
}

/// Owner service for context derivation. The work closure is the only place
/// that can execute SQL; it is created after the session lease succeeds.
pub struct SessionContextService {
    lease: Arc<dyn WorkScopeLease>,
    work: WorkFactory,
    clock: Arc<dyn Clock + Send + Sync>,
}

impl SessionContextService {
    pub fn new(lease: Arc<dyn WorkScopeLease>, clock: Arc<dyn Clock + Send + Sync>) -> Self {
        Self::with_work(lease, clock, Arc::new(pg_work))
    }

    pub fn with_work(
        lease: Arc<dyn WorkScopeLease>,
        clock: Arc<dyn Clock + Send + Sync>,
        work: WorkFactory,
    ) -> Self {
        Self { lease, clock, work }
    }

    fn derive(
        &self,
        host: &str,
        headers: &HeaderMap,
        holding_slug: &str,
    ) -> RouteFuture<'_, SessionContext> {
        let lease = self.lease.clone();
        let work = (self.work)(self.clock.now().get());
        let now = self.clock.now().get();
        let holding_slug = holding_slug.to_owned();
        let host = host.to_owned();
        let headers = headers.clone();
        Box::pin(async move {
            let view = lease.load_active_roles(&host, &headers, work).await?;
            let data = active_work_role_payload(&view, &holding_slug, now)?;
            if data.get("valid").and_then(|value| value.as_bool()) != Some(true) {
                return Err(work_context_changed());
            }
            let assignment_id = data
                .get("assignmentId")
                .and_then(|value| value.as_str())
                .ok_or_else(work_context_changed)?
                .to_owned();
            let assignment_version = data
                .get("assignmentVersion")
                .and_then(|value| value.as_i64())
                .ok_or_else(work_context_changed)?;
            let revision = data
                .get("revision")
                .and_then(|value| value.as_i64())
                .ok_or_else(work_context_changed)?;
            let company_id = data
                .get("companyId")
                .and_then(|value| value.as_str())
                .ok_or_else(work_context_changed)?
                .to_owned();
            let user_id = data
                .get("userId")
                .and_then(|value| value.as_str())
                .ok_or_else(work_context_changed)?
                .to_owned();
            let (branch_slug, department_slug, modules) = data
                .get("options")
                .and_then(|value| value.as_array())
                .and_then(|options| {
                    options.iter().find(|option| {
                        option.get("id").and_then(|value| value.as_str())
                            == Some(assignment_id.as_str())
                            && option.get("version").and_then(|value| value.as_i64())
                                == Some(assignment_version)
                    })
                })
                .map(|option| {
                    let modules = option
                        .get("modules")
                        .and_then(|value| value.as_array())
                        .and_then(|values| {
                            values
                                .iter()
                                .map(|value| value.as_str().map(str::to_owned))
                                .collect::<Option<Vec<_>>>()
                        });
                    (
                        option
                            .get("branchSlug")
                            .and_then(|value| value.as_str())
                            .map(str::to_owned),
                        option
                            .get("departmentSlug")
                            .and_then(|value| value.as_str())
                            .map(str::to_owned),
                        modules,
                    )
                })
                .unwrap_or((None, None, None));
            let modules = modules.ok_or_else(work_context_changed)?;
            Ok(SessionContext {
                company_id,
                assignment_id,
                revision,
                assignment_version,
                branch_slug,
                department_slug,
                user_id,
                modules,
            })
        })
    }
}

/// The exact private context values the gateway injects as headers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionContext {
    pub company_id: String,
    pub assignment_id: String,
    pub revision: i64,
    pub assignment_version: i64,
    pub branch_slug: Option<String>,
    pub department_slug: Option<String>,
    pub user_id: String,
    pub modules: Vec<String>,
}

impl SessionContext {
    fn value(&self) -> serde_json::Value {
        json!({
            "companyId": self.company_id,
            "assignmentId": self.assignment_id,
            "revision": self.revision,
            "assignmentVersion": self.assignment_version,
            "branchSlug": self.branch_slug,
            "departmentSlug": self.department_slug,
            "userId": self.user_id,
            "modules": self.modules,
        })
    }
}

fn work_context_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_CONTEXT_CHANGED",
        "Your work role changed. Reload and review before retrying.",
    )
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "SESSION_CONTEXT_REQUEST_INVALID",
        "Use one bounded Holding context query.",
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

struct StateData {
    targets: BTreeMap<String, String>,
    service: SessionContextService,
}

/// Build the loopback-only router. Targets are exact Host -> Holding slug pairs;
/// duplicates and malformed pairs fail closed before any route is mounted.
pub fn router(
    targets: Vec<SessionContextTarget>,
    lease: Arc<dyn WorkScopeLease>,
    clock: Arc<dyn Clock + Send + Sync>,
) -> Result<Router, BusinessError> {
    router_with_service(targets, SessionContextService::new(lease, clock))
}

pub fn router_with_service(
    targets: Vec<SessionContextTarget>,
    service: SessionContextService,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.host.len() > 255
            || target.holding_slug.is_empty()
            || target.holding_slug.len() > 128
            || !target.holding_slug.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
            })
            || configured
                .insert(target.host.clone(), target.holding_slug.clone())
                .is_some()
        {
            return Err(BusinessError::invalid(
                "SESSION_CONTEXT_CONFIG_INVALID",
                "Use exact distinct loopback Holding targets.",
            ));
        }
    }
    if configured.is_empty() {
        return Err(BusinessError::invalid(
            "SESSION_CONTEXT_CONFIG_INVALID",
            "Use exact distinct loopback Holding targets.",
        ));
    }
    Ok(Router::new()
        .route("/internal/session-context", get(context))
        .with_state(Arc::new(StateData {
            targets: configured,
            service,
        })))
}

fn holding_query(request: &Request) -> Result<String, BusinessError> {
    let query = request.uri().query().ok_or_else(invalid)?;
    if query.len() > 128 {
        return Err(invalid());
    }
    let mut parts = query.split('&');
    let first = parts.next().ok_or_else(invalid)?;
    if parts.next().is_some() {
        return Err(invalid());
    }
    let (name, value) = first.split_once('=').ok_or_else(invalid)?;
    if name != "holding" || value.is_empty() || value.len() > 128 {
        return Err(invalid());
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
    }) {
        return Err(invalid());
    }
    Ok(value.to_owned())
}

async fn context(State(state): State<Arc<StateData>>, request: Request) -> Response {
    let result = async move {
        let slug = holding_query(&request)?;
        let headers = request.headers().clone();
        let host = headers
            .get_all(header::HOST)
            .iter()
            .next()
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))?;
        if headers.get_all(header::HOST).iter().count() != 1 {
            return Err(invalid());
        }
        let expected = state
            .targets
            .get(host)
            .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))?;
        if expected != &slug {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ));
        }
        if let Some(origin) = headers
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok())
        {
            if !loopback_origin(origin) {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "ORIGIN_FORBIDDEN",
                    "The context bridge is loopback only.",
                ));
            }
        }
        let bytes = to_bytes(request.into_body(), 16)
            .await
            .map_err(|_| invalid())?;
        if !bytes.is_empty() {
            return Err(invalid());
        }
        state.service.derive(host, &headers, &slug).await
    }
    .await;
    match result {
        Ok(context) => {
            let mut response =
                (StatusCode::OK, Json(json!({"data": context.value()}))).into_response();
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
        Err(error) => error_response(error),
    }
}

fn loopback_origin(origin: &str) -> bool {
    for prefix in ["http://127.0.0.1:", "http://[::1]:", "http://localhost:"] {
        if let Some(rest) = origin.strip_prefix(prefix) {
            if !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit()) && rest.len() <= 5
            {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_runtime::public::http_support::serve;
    use erpx_server_shared::public::TimestampMillis;
    use std::net::SocketAddr;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Clock9;
    impl Clock for Clock9 {
        fn now(&self) -> TimestampMillis {
            TimestampMillis::new(9_000)
        }
    }

    #[derive(Debug, Clone, Copy)]
    enum Session {
        Valid,
        Missing,
        Expired,
        NoSelection,
    }

    struct Lease {
        sessions: Arc<AtomicUsize>,
        reads: Arc<AtomicUsize>,
        session: Session,
    }

    impl WorkScopeLease for Lease {
        fn load_active_roles(
            &self,
            _host: &str,
            _headers: &HeaderMap,
            work: ActiveWorkFn,
        ) -> RouteFuture<'_, ActiveWorkRoleView> {
            self.sessions.fetch_add(1, Ordering::SeqCst);
            let reads = self.reads.clone();
            let session = match self.session {
                Session::Valid => 0,
                Session::Missing => 1,
                Session::Expired => 2,
                Session::NoSelection => 3,
            };
            Box::pin(async move {
                match session {
                    1 => {
                        return Err(BusinessError::new(
                            FailureKind::Unauthenticated,
                            "UNAUTHENTICATED",
                            "Sign in to continue.",
                        ));
                    }
                    2 => {
                        return Err(BusinessError::new(
                            FailureKind::Unauthenticated,
                            "SESSION_EXPIRED",
                            "The session expired.",
                        ));
                    }
                    _ => {}
                }
                // The only SQL-bearing closure is `work`; calling it is recorded.
                let _ = work.clone();
                reads.fetch_add(1, Ordering::SeqCst);
                let selection = if session == 3 {
                    None
                } else {
                    Some(super::super::assignments_active::ActiveWorkSelection {
                        assignment_id: Some("assignment-1".into()),
                        assignment_version: Some(3),
                        revision: 7,
                        expires_at: 9_500,
                    })
                };
                Ok(ActiveWorkRoleView {
                    company_id: "company-1".into(),
                    user_id: "alpha".into(),
                    options: vec![super::super::assignments_active::ActiveWorkOption {
                        id: "assignment-1".into(),
                        version: 3,
                        role: "ledger_reader".into(),
                        company_id: "company-1".into(),
                        branch_id: None,
                        modules: vec!["work.get".into(), "finance.read".into()],
                        scope_kind: crate::domain::work_context::WorkScopeKind::Branch,
                        scope_node_id: None,
                        label: "Ledger reader".into(),
                        valid_until: 9_500,
                        branch_slug: Some("bangkok".into()),
                        department_slug: None,
                        applies: true,
                    }],
                    selection,
                })
            })
        }
    }

    async fn start(session: Session) -> (SocketAddr, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let sessions = Arc::new(AtomicUsize::new(0));
        let reads = Arc::new(AtomicUsize::new(0));
        let work_reads = reads.clone();
        let work: WorkFactory = Arc::new(move |_now| {
            let reads = work_reads.clone();
            Arc::new(move |_connection, _scope| {
                let reads = reads.clone();
                Box::pin(async move {
                    reads.fetch_add(1, Ordering::SeqCst);
                    Err(BusinessError::new(
                        FailureKind::Unavailable,
                        "SQL_SHOULD_NOT_RUN",
                        "Owner SQL was reached.",
                    ))
                })
            })
        });
        let lease = Arc::new(Lease {
            sessions: sessions.clone(),
            reads: reads.clone(),
            session,
        });
        let service = SessionContextService::with_work(lease, Arc::new(Clock9), work);
        let router = router_with_service(
            vec![SessionContextTarget {
                host: "127.0.0.1:44999".into(),
                holding_slug: "acme".into(),
            }],
            service,
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let _server = tokio::spawn(async move {
            let _ = serve(listener, router).await;
        });
        (addr, sessions, reads)
    }

    async fn http(addr: SocketAddr, raw: String) -> (u16, serde_json::Value, String) {
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
            serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
            text,
        )
    }

    fn get(path: &str) -> String {
        format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:44999\r\nCookie: __Host-erpx-product=session\r\nConnection: close\r\n\r\n"
        )
    }

    #[tokio::test]
    async fn exact_context_shape_is_returned_for_the_current_session() {
        let (addr, sessions, _) = start(Session::Valid).await;
        let (status, body, headers) =
            http(addr, get("/internal/session-context?holding=acme")).await;
        assert_eq!(status, 200);
        assert_eq!(
            body["data"],
            json!({
                "companyId": "company-1",
                "assignmentId": "assignment-1",
                "revision": 7,
                "assignmentVersion": 3,
                "branchSlug": "bangkok",
                "departmentSlug": null,
                "userId": "alpha",
                "modules": ["work.get", "finance.read"],
            })
        );
        assert_eq!(body["data"]["modules"], json!(["work.get", "finance.read"]));
        assert_eq!(body["data"].as_object().unwrap().len(), 8);
        assert!(headers.contains("cache-control: private, no-store"));
        assert_eq!(sessions.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn invalid_sessions_fail_closed_without_owner_sql_and_unselected_role_is_409() {
        for session in [Session::Missing, Session::Expired] {
            let (addr, _, reads) = start(session).await;
            let (status, body, _) = http(addr, get("/internal/session-context?holding=acme")).await;
            assert_eq!(status, 401);
            assert_eq!(
                reads.load(Ordering::SeqCst),
                0,
                "no SQL may run for invalid session {session:?}"
            );
            assert!(body["error"]["code"].is_string());
        }
        let (addr, _, reads) = start(Session::NoSelection).await;
        let (status, body, _) = http(addr, get("/internal/session-context?holding=acme")).await;
        assert_eq!(status, 409);
        assert_eq!(body["error"]["code"], "WORK_CONTEXT_CHANGED");
        assert_eq!(reads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn query_body_and_host_are_bounded() {
        let (addr, sessions, _) = start(Session::Valid).await;
        for path in [
            "/internal/session-context",
            "/internal/session-context?holding=acme&extra=1",
            "/internal/session-context?holding=UPPER",
            &format!("/internal/session-context?holding={}", "a".repeat(129)),
        ] {
            let (status, _, _) = http(addr, get(path)).await;
            assert_eq!(status, 422, "{path}");
        }
        let (status, _, _) = http(
            addr,
            "GET /internal/session-context?holding=acme HTTP/1.1\r\nHost: other.example.test\r\nConnection: close\r\n\r\n".to_string(),
        )
        .await;
        assert_eq!(status, 404);
        let (status, _, _) = http(
            addr,
            "POST /internal/session-context?holding=acme HTTP/1.1\r\nHost: 127.0.0.1:44999\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
        )
        .await;
        assert_eq!(status, 405);
        assert_eq!(sessions.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn loopback_origin_and_target_configuration_are_strict() {
        assert!(loopback_origin("http://127.0.0.1:44999"));
        assert!(loopback_origin("http://[::1]:44999"));
        assert!(loopback_origin("http://localhost:44999"));
        assert!(!loopback_origin("https://acme.example.com"));
        assert!(!loopback_origin("http://127.0.0.1:44999/path"));
        assert!(
            router(
                Vec::new(),
                Arc::new(Lease {
                    sessions: Arc::new(AtomicUsize::new(0)),
                    reads: Arc::new(AtomicUsize::new(0)),
                    session: Session::Valid,
                }),
                Arc::new(Clock9),
            )
            .is_err()
        );
    }
}
