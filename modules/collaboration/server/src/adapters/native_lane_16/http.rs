//! Owner-local axum ingress for the collaboration sent-notice reads. The
//! router validates the exact registered Holding host/slug/Company, one bounded
//! request and the two context headers, then returns the complete native
//! projection. It accepts no SQL and no caller-chosen Company.
//!
//! `router_for_holdings` is the composition contract: each registered Holding
//! supplies its own pool and gets its own owner reader in the state map.

use super::application::{
    PushReadRequest, PushSettingsQuery, PushSettingsQueryService, PushSettingsRead,
    PushSettingsService, ReadRequest, SentNoticeRead, SentNoticeService, configured_public_key,
};
use super::domain::{PushReadKind, ReadKind, valid_identifier};
use super::postgres::{PgPushSettingsRead, PgSentNoticeRead};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, get, header, json, post,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Deserialize;
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");
/// Public GET has no request body: the thin edge injects the authenticated work
/// session from the Account cookie. No business logic lives in that edge.
const CONTEXT_SESSION: HeaderName = HeaderName::from_static("x-erpx-context-session");
const MAX_BODY: usize = 8 * 1024;

/// One exact registered Holding target. Composition owns the list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
    pub company_id: String,
}

/// Wire shapes are the exact camelCase bodies emitted by the TS bridge. The
/// Company and user headers remain authoritative; the optional body copies are
/// cross-checked so a stale or forged caller selection fails closed.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PushProjectionRequest {
    kind: String,
    #[serde(default)]
    company_id: Option<String>,
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    endpoint_hash: Option<String>,
}

/// Wire shape of the composite `GET /push` settings read. The Company and user
/// headers remain authoritative; the body copies are cross-checked so a stale
/// or forged caller selection fails closed. VAPID config is never caller-supplied.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PushSettingsViewRequest {
    #[serde(default)]
    company_id: Option<String>,
    user_id: String,
    session_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectionRequest {
    kind: String,
    #[serde(default)]
    company_id: Option<String>,
    #[serde(default)]
    sender_id: Option<String>,
    #[serde(default)]
    notice_id: Option<String>,
    #[serde(default)]
    event_key: Option<String>,
    #[serde(default)]
    task_id: Option<String>,
}

/// A registered target and the owner reader bound to its pool.
struct LaneTargetState {
    company_id: String,
    service: SentNoticeService<Arc<dyn SentNoticeRead>>,
    push_service: Option<PushSettingsService<Arc<dyn PushSettingsRead>>>,
    push_view_service: Option<PushSettingsQueryService<Arc<dyn PushSettingsRead>>>,
}

struct LaneState {
    targets: BTreeMap<(String, String), LaneTargetState>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "LANE_16_READ_REQUEST_INVALID",
        "Use one bounded Collaboration sent-notice read.",
    )
}

fn push_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LANE_16_PUSH_READ_UNAVAILABLE",
        "The native Collaboration push read is unavailable.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "LANE_16_TARGETS_INVALID",
        "Use exact distinct registered Holding Company targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
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

fn bounded(body: serde_json::Value) -> Response {
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
    response
}

/// Validate the exact distinct Holding targets and bind one reader to each.
fn build_state(
    targets: Vec<(LaneTarget, Arc<dyn SentNoticeRead>)>,
) -> Result<Arc<LaneState>, BusinessError> {
    build_state_with_push(targets, Vec::new(), None)
}

fn build_state_with_push(
    targets: Vec<(LaneTarget, Arc<dyn SentNoticeRead>)>,
    push_readers: Vec<Arc<dyn PushSettingsRead>>,
    public_key: Option<String>,
) -> Result<Arc<LaneState>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (index, (target, reader)) in targets.into_iter().enumerate() {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_identifier(&target.company_id)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    LaneTargetState {
                        company_id: target.company_id,
                        service: SentNoticeService::new(reader),
                        push_service: push_readers
                            .get(index)
                            .map(|reader| PushSettingsService::new(reader.clone())),
                        push_view_service: push_readers.get(index).map(|reader| {
                            PushSettingsQueryService::new(reader.clone(), public_key.clone())
                        }),
                    },
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Arc::new(LaneState {
        targets: configured,
    }))
}

fn build_with(
    targets: Vec<(LaneTarget, Arc<dyn SentNoticeRead>)>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/collaboration/native-lane-16/sent-notices",
            post(read),
        )
        .with_state(build_state(targets)?))
}

fn build_with_push(
    targets: Vec<(LaneTarget, Arc<dyn SentNoticeRead>)>,
    push_readers: Vec<Arc<dyn PushSettingsRead>>,
    public_key: Option<String>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/collaboration/native-lane-16/sent-notices",
            post(read),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/collaboration/native-lane-16/push-settings",
            post(push_settings),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/collaboration/native-lane-16/push-settings-view",
            post(push_settings_view),
        )
        // Public-path alias: byte-compatible with the retired shared GET. The
        // edge only injects the authenticated context headers; it carries no
        // owner SQL, fallback or payload shaping.
        .route(
            "/api/v1/holdings/{holding_slug}/push",
            get(push_settings_public),
        )
        .with_state(build_state_with_push(targets, push_readers, public_key)?))
}

/// Reader-injection path: one distinct reader per registered Holding target.
fn router_with_readers(
    targets: Vec<(LaneTarget, Arc<dyn SentNoticeRead>)>,
) -> Result<Router, BusinessError> {
    build_with(targets)
}

/// Per-Holding router contract. Composition supplies its own Pg pool for each
/// registered Holding; the owner builds one `PgSentNoticeRead` per target and
/// stores it keyed by `(host, holding_slug)`. No composition file is edited here.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let mut notices = Vec::with_capacity(targets.len());
    let mut push = Vec::with_capacity(targets.len());
    for (target, pool) in targets {
        let reader: Arc<dyn SentNoticeRead> = Arc::new(PgSentNoticeRead::new(pool.clone()));
        let push_reader: Arc<dyn PushSettingsRead> = Arc::new(PgPushSettingsRead::new(pool));
        notices.push((target, reader));
        push.push(push_reader);
    }
    build_with_push(notices, push, configured_public_key())
}

/// Single-pool router retained for owner tests and composition variants that
/// use one shared pool across registered targets.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_for_holdings(
        targets
            .into_iter()
            .map(|target| (target, pool.clone()))
            .collect(),
    )
}

/// Same route with one injected reader shared across targets; retained for
/// owner tests and composition variants that already hold a typed reader.
pub fn router_with_reader(
    reader: Arc<dyn SentNoticeRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

async fn read(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?
            .to_owned();
        let target = state
            .targets
            .get(&(host, slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_identifier(value))
            .ok_or_else(invalid)?
            .to_owned();
        if company != target.company_id {
            return Err(invalid());
        }
        let header_sender = unique_header(request.headers(), CONTEXT_USER)?
            .filter(|value| valid_identifier(value))
            .map(str::to_owned);
        let bytes = to_bytes(request.into_body(), MAX_BODY)
            .await
            .map_err(|_| invalid())?;
        let body: ProjectionRequest = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if body
            .company_id
            .as_deref()
            .is_some_and(|value| value != company.as_str())
        {
            return Err(invalid());
        }
        let kind = ReadKind::parse(&body.kind).ok_or_else(invalid)?;
        let (has_notice, has_event, has_task) = kind.shape();
        // Only the notice reads carry a sender. The count and origin reads are
        // Company-keyed and the TS bridge deliberately omits the user header.
        let sender = if matches!(kind, ReadKind::Notices | ReadKind::Notice) {
            header_sender.filter(|value| {
                body.sender_id
                    .as_deref()
                    .is_none_or(|copy| copy == value.as_str())
            })
            .ok_or_else(invalid)?
        } else {
            if body.sender_id.is_some() {
                return Err(invalid());
            }
            String::new()
        };
        let request = ReadRequest {
            company,
            sender,
            kind,
            notice_id: body.notice_id,
            event_key: body.event_key,
            task_id: body.task_id,
        };
        if (has_notice || has_event || has_task)
            && request.notice_id.is_none()
            && request.event_key.is_none()
            && request.task_id.is_none()
        {
            return Err(invalid());
        }
        let data = target.service.read(request).await?;
        Ok::<_, BusinessError>(json!({"data": data}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn push_settings(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?
            .to_owned();
        let target = state
            .targets
            .get(&(host, slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_identifier(value))
            .ok_or_else(invalid)?
            .to_owned();
        if company != target.company_id {
            return Err(invalid());
        }
        let bytes = to_bytes(request.into_body(), MAX_BODY)
            .await
            .map_err(|_| invalid())?;
        let body: PushProjectionRequest = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if body
            .company_id
            .as_deref()
            .is_some_and(|value| value != company.as_str())
        {
            return Err(invalid());
        }
        let kind = PushReadKind::parse(&body.kind).ok_or_else(invalid)?;
        let user = match kind {
            PushReadKind::EndpointOwner => String::new(),
            _ => body.user_id.filter(|v| valid_identifier(v)).ok_or_else(invalid)?,
        };
        let service = target.push_service.as_ref().ok_or_else(push_unavailable)?;
        let data = service
            .read(PushReadRequest {
                company,
                user,
                kind,
                session: body.session_id,
                endpoint_hash: body.endpoint_hash,
            })
            .await?;
        Ok::<_, BusinessError>(json!({"data": data}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

/// Composite `GET /push` settings read over the lane path. The owner validates
/// the exact Holding target and context headers, then the application query
/// executes the real preference/session-count SQL through `PgPushSettingsRead`.
async fn push_settings_view(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?
            .to_owned();
        let target = state
            .targets
            .get(&(host, slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_identifier(value))
            .ok_or_else(invalid)?
            .to_owned();
        if company != target.company_id {
            return Err(invalid());
        }
        let header_user = unique_header(request.headers(), CONTEXT_USER)?
            .filter(|value| valid_identifier(value))
            .ok_or_else(invalid)?
            .to_owned();
        let bytes = to_bytes(request.into_body(), MAX_BODY)
            .await
            .map_err(|_| invalid())?;
        let body: PushSettingsViewRequest =
            serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if body
            .company_id
            .as_deref()
            .is_some_and(|value| value != company.as_str())
            || body.user_id != header_user
        {
            return Err(invalid());
        }
        let service = target
            .push_view_service
            .as_ref()
            .ok_or_else(push_unavailable)?;
        let data = service
            .read(PushSettingsQuery {
                company,
                user: body.user_id,
                session: body.session_id,
            })
            .await?;
        Ok::<_, BusinessError>(json!({"data": data}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

/// Public-path alias of the retired shared `GET /push` handler. It returns the
/// same `{data: ...}` envelope and `private, no-store` header; the payload comes
/// from the same application query and `PgPushSettingsRead` SQL adapter.
///
/// Handler shape note: GET has no body, so the final extractor is `HeaderMap`,
/// not `Request`. State and Path match every other handler in this router
/// (`State<Arc<LaneState>>`, `Path<String>`), so axum's `Handler<T, S>` bound is
/// satisfied by the same state type the router is built with.
async fn push_settings_public(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_header(&headers, header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?
            .to_owned();
        let target = state
            .targets
            .get(&(host, slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(&headers, CONTEXT_COMPANY)?
            .filter(|value| valid_identifier(value))
            .ok_or_else(invalid)?
            .to_owned();
        if company != target.company_id {
            return Err(invalid());
        }
        let user = unique_header(&headers, CONTEXT_USER)?
            .filter(|value| valid_identifier(value))
            .ok_or_else(invalid)?
            .to_owned();
        let session = unique_header(&headers, CONTEXT_SESSION)?
            .filter(|value| valid_identifier(value))
            .ok_or_else(invalid)?
            .to_owned();
        let service = target
            .push_view_service
            .as_ref()
            .ok_or_else(push_unavailable)?;
        let data = service
            .read(PushSettingsQuery {
                company,
                user,
                session,
            })
            .await?;
        Ok::<_, BusinessError>(json!({"data": data}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::application::LaneReadFuture;
    use erpx_platform_runtime_server::public::http_support::StatusCode as Code;
    use erpx_platform_runtime_server::public::http_support::body::Body;
    use std::sync::Mutex;

    struct FakeReader;

    impl SentNoticeRead for FakeReader {
        fn notices<'a>(
            &'a self,
            _company: &'a str,
            _sender: &'a str,
        ) -> LaneReadFuture<'a, Vec<super::super::domain::SentNoticeRow>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn notice<'a>(
            &'a self,
            _company: &'a str,
            _sender: &'a str,
            _id: &'a str,
        ) -> LaneReadFuture<'a, Option<super::super::domain::SentNoticeDetail>> {
            Box::pin(async { Ok(None) })
        }
        fn delivery_count<'a>(
            &'a self,
            _company: &'a str,
            _key: &'a str,
        ) -> LaneReadFuture<'a, i64> {
            Box::pin(async { Ok(0) })
        }
        fn task_origin<'a>(
            &'a self,
            _company: &'a str,
            _task: &'a str,
        ) -> LaneReadFuture<'a, Option<super::super::domain::TaskOrigin>> {
            Box::pin(async { Ok(None) })
        }
    }

    /// Records the Company each reader was asked to load, proving the per-target
    /// state map never crosses a Holding boundary.
    struct Probe {
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl SentNoticeRead for Probe {
        fn notices<'a>(
            &'a self,
            company: &'a str,
            _sender: &'a str,
        ) -> LaneReadFuture<'a, Vec<super::super::domain::SentNoticeRow>> {
            self.seen.lock().unwrap().push(company.to_owned());
            Box::pin(async { Ok(vec![]) })
        }
        fn notice<'a>(
            &'a self,
            _company: &'a str,
            _sender: &'a str,
            _id: &'a str,
        ) -> LaneReadFuture<'a, Option<super::super::domain::SentNoticeDetail>> {
            Box::pin(async { Ok(None) })
        }
        fn delivery_count<'a>(
            &'a self,
            _company: &'a str,
            _key: &'a str,
        ) -> LaneReadFuture<'a, i64> {
            Box::pin(async { Ok(0) })
        }
        fn task_origin<'a>(
            &'a self,
            _company: &'a str,
            _task: &'a str,
        ) -> LaneReadFuture<'a, Option<super::super::domain::TaskOrigin>> {
            Box::pin(async { Ok(None) })
        }
    }

    struct FakePushReader;

    impl PushSettingsRead for FakePushReader {
        fn preferences<'a>(
            &'a self,
            _company: &'a str,
            _user: &'a str,
        ) -> LaneReadFuture<'a, Option<serde_json::Value>> {
            Box::pin(async { Ok(Some(json!({"enabled": true, "work_enabled": true}))) })
        }
        fn session_count<'a>(
            &'a self,
            _company: &'a str,
            _user: &'a str,
            _session: &'a str,
        ) -> LaneReadFuture<'a, Option<i64>> {
            Box::pin(async { Ok(Some(1)) })
        }
        fn endpoint_owner<'a>(
            &'a self,
            _company: &'a str,
            _hash: &'a str,
        ) -> LaneReadFuture<'a, Option<serde_json::Value>> {
            Box::pin(async { Ok(None) })
        }
    }

    fn state_with_push() -> Arc<LaneState> {
        build_state_with_push(
            vec![(target(), Arc::new(FakeReader) as Arc<dyn SentNoticeRead>)],
            vec![Arc::new(FakePushReader) as Arc<dyn PushSettingsRead>],
            Some("vapid-public".into()),
        )
        .unwrap()
    }

    fn target() -> LaneTarget {
        LaneTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            company_id: "company-1".into(),
        }
    }

    fn state() -> Arc<LaneState> {
        build_state(vec![(target(), Arc::new(FakeReader) as Arc<dyn SentNoticeRead>)]).unwrap()
    }

    fn request(kind: &str, extra: Option<(&str, &str)>) -> Request {
        let mut body = json!({"kind": kind});
        if let Some((key, value)) = extra {
            body[key] = json!(value);
        }
        Request::builder()
            .method("POST")
            .header("host", "acme.example.test")
            .header("x-erpx-context-company", "company-1")
            .header("x-erpx-context-user", "user-1")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, serde_json::Value) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(raw.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let text = String::from_utf8_lossy(&bytes);
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
        )
    }

    async fn serve_router(router: Router) -> (u16, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        (addr.port(), server)
    }

    #[test]
    fn targets_are_exact_and_distinct() {
        assert!(!valid_slug("Acme"));
        assert!(!valid_identifier("company/1"));
        let mut bad = target();
        bad.holding_slug = String::new();
        assert!(configuration_invalid().code == "LANE_16_TARGETS_INVALID");
        assert_eq!(bad.holding_slug, "");
        let reader = Arc::new(FakeReader) as Arc<dyn SentNoticeRead>;
        assert!(router_with_reader(reader.clone(), vec![target()]).is_ok());
        assert!(router_with_reader(reader.clone(), vec![target(), target()]).is_err());
        assert!(router_with_reader(reader.clone(), vec![]).is_err());
        assert!(router_with_reader(
            reader,
            vec![LaneTarget {
                holding_slug: "BAD".into(),
                ..target()
            }]
        )
        .is_err());
    }

    #[test]
    fn request_shape_is_strict() {
        let good: ProjectionRequest =
            serde_json::from_str(r#"{"kind":"notice","noticeId":"notice-1"}"#).unwrap();
        assert_eq!(good.kind, "notice");
        assert_eq!(good.notice_id.as_deref(), Some("notice-1"));
        assert!(serde_json::from_str::<ProjectionRequest>(
            r#"{"kind":"notice","notice_id":"notice-1"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<ProjectionRequest>(
            r#"{"kind":"notice","noticeId":"notice-1","sql":"SELECT 1"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<ProjectionRequest>(r#"{"kind":"???"}"#).is_ok());
        assert_eq!(ReadKind::parse("???"), None);
    }

    #[test]
    fn push_request_shape_matches_the_ts_bridge_and_closes_snake_case() {
        let good: PushProjectionRequest = serde_json::from_str(
            r#"{"kind":"push-session-count","companyId":"company-1","userId":"user-1","sessionId":"session-1"}"#,
        )
        .unwrap();
        assert_eq!(good.kind, "push-session-count");
        assert_eq!(good.user_id.as_deref(), Some("user-1"));
        assert_eq!(good.session_id.as_deref(), Some("session-1"));
        assert!(serde_json::from_str::<PushProjectionRequest>(
            r#"{"kind":"push-preferences","user_id":"user-1"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<PushProjectionRequest>(
            r#"{"kind":"push-preferences","userId":"user-1","sql":"SELECT 1"}"#
        )
        .is_err());
    }

    #[test]
    fn duplicate_header_is_rejected() {
        let mut headers = HeaderMap::new();
        headers.append(header::HOST, HeaderValue::from_static("acme.example.test"));
        headers.append(header::HOST, HeaderValue::from_static("evil.example.test"));
        assert_eq!(
            unique_header(&headers, header::HOST).unwrap_err().code,
            "LANE_16_READ_REQUEST_INVALID"
        );
    }

    #[test]
    fn error_mapping_fails_closed() {
        assert_eq!(error_response(invalid()).status(), Code::UNPROCESSABLE_ENTITY);
        assert_eq!(error_response(missing()).status(), Code::NOT_FOUND);
        assert_eq!(
            error_response(BusinessError::new(
                FailureKind::Unavailable,
                "LANE_16_NATIVE_READ_UNAVAILABLE",
                "unavailable"
            ))
            .status(),
            Code::SERVICE_UNAVAILABLE
        );
    }

    #[tokio::test]
    async fn company_header_must_match_the_registered_target() {
        let mut req = request("notices", None);
        req.headers_mut().insert(
            CONTEXT_COMPANY,
            HeaderValue::from_static("other-company"),
        );
        let response = read(State(state()), Path("acme".into()), req).await;
        assert_eq!(response.status(), Code::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn bounded_read_returns_the_native_envelope() {
        let response = read(State(state()), Path("acme".into()), request("notices", None)).await;
        assert_eq!(response.status(), Code::OK);
        assert!(response.headers().get(header::CACHE_CONTROL).is_some());
        let response = read(
            State(state()),
            Path("acme".into()),
            request("delivery-count", Some(("eventKey", "notice:notice-1"))),
        )
        .await;
        assert_eq!(response.status(), Code::OK);
    }

    #[tokio::test]
    async fn push_read_returns_the_native_projection_and_fails_closed_without_reader() {
        let response = push_settings(
            State(state_with_push()),
            Path("acme".into()),
            request("push-preferences", Some(("userId", "user-1"))),
        )
        .await;
        assert_eq!(response.status(), Code::OK);

        let response = push_settings(
            State(state()),
            Path("acme".into()),
            request("push-preferences", Some(("userId", "user-1"))),
        )
        .await;
        assert_eq!(response.status(), Code::SERVICE_UNAVAILABLE);
    }

    fn view_request(body: &str) -> Request {
        Request::builder()
            .method("POST")
            .header("host", "acme.example.test")
            .header("x-erpx-context-company", "company-1")
            .header("x-erpx-context-user", "user-1")
            .header("content-type", "application/json")
            .body(Body::from(body.to_owned()))
            .unwrap()
    }

    fn public_headers(company: &str, user: &str, session: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("acme.example.test"));
        headers.insert(CONTEXT_COMPANY, HeaderValue::try_from(company).unwrap());
        headers.insert(CONTEXT_USER, HeaderValue::try_from(user).unwrap());
        if let Some(session) = session {
            headers.insert(CONTEXT_SESSION, HeaderValue::try_from(session).unwrap());
        }
        headers
    }

    async fn json_body(response: Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn push_settings_view_returns_complete_parity_payload() {
        let response = push_settings_view(
            State(state_with_push()),
            Path("acme".into()),
            view_request(r#"{"userId":"user-1","sessionId":"session-1"}"#),
        )
        .await;
        assert_eq!(response.status(), Code::OK);
        assert_eq!(
            json_body(response).await,
            json!({"data": {
                "available": true,
                "publicKey": "vapid-public",
                "preferences": {"enabled": true, "work_enabled": true},
                "deviceCount": 1,
            }})
        );
    }

    #[tokio::test]
    async fn public_push_get_returns_exact_shared_route_parity() {
        // No Origin header and no body: the retired shared GET asserted neither.
        let response = push_settings_public(
            State(state_with_push()),
            Path("acme".into()),
            public_headers("company-1", "user-1", Some("session-1")),
        )
        .await;
        assert_eq!(response.status(), Code::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok()),
            Some("private, no-store")
        );
        // canonicalDestinationLinks is identity for this payload: no key ends
        // in Href/href, so the native envelope is byte-identical to the old one.
        assert_eq!(
            json_body(response).await,
            json!({"data": {
                "available": true,
                "publicKey": "vapid-public",
                "preferences": {"enabled": true, "work_enabled": true},
                "deviceCount": 1,
            }})
        );
    }

    #[tokio::test]
    async fn public_push_get_fails_closed_on_scope_mismatch_and_missing_session() {
        // User/session ownership is enforced by the owner session-count SQL,
        // whose zero result is the documented empty-device response. The route
        // itself must reject a foreign Company and a missing session header.
        for headers in [
            public_headers("other-company", "user-1", Some("session-1")),
            public_headers("company-1", "user-1", None),
        ] {
            let response =
                push_settings_public(State(state_with_push()), Path("acme".into()), headers).await;
            assert_eq!(response.status(), Code::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn public_push_get_fails_closed_without_owner_reader() {
        let response = push_settings_public(
            State(state()),
            Path("acme".into()),
            public_headers("company-1", "user-1", Some("session-1")),
        )
        .await;
        assert_eq!(response.status(), Code::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn sender_header_is_required_only_for_notice_reads() {
        let mut count = request("delivery-count", Some(("eventKey", "notice:notice-1")));
        count.headers_mut().remove(CONTEXT_USER);
        assert_eq!(
            read(State(state()), Path("acme".into()), count).await.status(),
            Code::OK
        );
        let mut notices = request("notices", None);
        notices.headers_mut().remove(CONTEXT_USER);
        assert_eq!(
            read(State(state()), Path("acme".into()), notices).await.status(),
            Code::UNPROCESSABLE_ENTITY
        );
    }

    #[tokio::test]
    async fn body_company_copy_cannot_override_the_registered_target() {
        let response = read(
            State(state()),
            Path("acme".into()),
            request("notices", Some(("companyId", "other-company"))),
        )
        .await;
        assert_eq!(response.status(), Code::UNPROCESSABLE_ENTITY);

        let response = push_settings(
            State(state_with_push()),
            Path("acme".into()),
            request("push-preferences", Some(("companyId", "other-company"))),
        )
        .await;
        assert_eq!(response.status(), Code::UNPROCESSABLE_ENTITY);
    }

    /// Proves the per-target state map routes each Holding to its own reader
    /// (and therefore its own pool in `router_for_holdings`). Two injected
    /// readers record disjoint Company scopes; neither observes the other's call.
    #[tokio::test]
    async fn two_holdings_route_to_distinct_readers() {
        let first_seen = Arc::new(Mutex::new(Vec::new()));
        let second_seen = Arc::new(Mutex::new(Vec::new()));
        let first: Arc<dyn SentNoticeRead> = Arc::new(Probe {
            seen: first_seen.clone(),
        });
        let second: Arc<dyn SentNoticeRead> = Arc::new(Probe {
            seen: second_seen.clone(),
        });
        let router = router_with_readers(vec![
            (
                LaneTarget {
                    company_id: "company-a".into(),
                    ..target()
                },
                first,
            ),
            (
                LaneTarget {
                    host: "other.example.test".into(),
                    holding_slug: "other".into(),
                    company_id: "company-b".into(),
                },
                second,
            ),
        ])
        .unwrap();
        let (port, server) = serve_router(router).await;
        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

        let acme = http(
            addr,
            "POST /api/v1/holdings/acme/collaboration/native-lane-16/sent-notices HTTP/1.1\r\nHost: acme.example.test\r\nx-erpx-context-company: company-a\r\nx-erpx-context-user: user-1\r\ncontent-type: application/json\r\ncontent-length: 18\r\nConnection: close\r\n\r\n{\"kind\":\"notices\"}"
                .into(),
        )
        .await;
        let other = http(
            addr,
            "POST /api/v1/holdings/other/collaboration/native-lane-16/sent-notices HTTP/1.1\r\nHost: other.example.test\r\nx-erpx-context-company: company-b\r\nx-erpx-context-user: user-2\r\ncontent-type: application/json\r\ncontent-length: 18\r\nConnection: close\r\n\r\n{\"kind\":\"notices\"}"
                .into(),
        )
        .await;
        assert_eq!(acme.0, 200);
        assert_eq!(other.0, 200);

        let first_seen = first_seen.lock().unwrap();
        let second_seen = second_seen.lock().unwrap();
        assert_eq!(first_seen.len(), 1);
        assert_eq!(first_seen[0], "company-a");
        assert_eq!(second_seen.len(), 1);
        assert_eq!(second_seen[0], "company-b");
        assert!(first_seen.iter().all(|company| company != "company-b"));
        assert!(second_seen.iter().all(|company| company != "company-a"));
        server.abort();
    }
}
