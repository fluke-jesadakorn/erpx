//! Owner-local HTTP ingress for the native Legal record slice.
//!
//! Transport facts only: registered Host/Holding targets, Origin, idempotency,
//! bounded JSON and error mapping. Normal Account session resolution, current
//! Company/branch/department scope and the serializable owner unit are supplied
//! by the composition backend, which must implement `LegalHttpBackend`.
use crate::domain::legal_records::{
    LegalDetail, LegalRegister, LegalRegisterQuery, REGISTER_MAX_PAGE,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Query, Request, Response, Router,
    State, StatusCode, body::to_bytes, get, header, post,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 512 * 1024;

pub type LegalHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub enum LegalHttpRead {
    Register(LegalRegisterQuery),
    Detail(String),
}

pub struct LegalHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub read: LegalHttpRead,
}

pub struct LegalHttpCommandRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub body: &'a Value,
    pub key: &'a str,
    pub request_hash: &'a str,
}

pub trait LegalHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: LegalHttpReadRequest<'a>,
    ) -> LegalHttpFuture<'a, LegalHttpReadReply>;
    fn command<'a>(
        &'a self,
        request: LegalHttpCommandRequest<'a>,
    ) -> LegalHttpFuture<'a, LegalCommandReply>;
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegalRenderedContext {
    pub user_id: String,
    pub holding_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub assignment_id: String,
    pub assignment_version: i64,
    pub revision: i64,
    pub role: String,
    pub modules: Vec<String>,
    pub permissions: Vec<String>,
    pub expires_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct LegalRegisterReply {
    pub data: LegalRegister,
    pub context: LegalRenderedContext,
}

#[derive(Clone, Debug, Serialize)]
pub struct LegalDetailReply {
    pub data: LegalDetail,
    pub context: LegalRenderedContext,
}

#[derive(Clone, Debug, Serialize)]
pub struct LegalCommandReply {
    pub data: Value,
    pub context: LegalRenderedContext,
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum LegalHttpReadReply {
    Register(Box<LegalRegisterReply>),
    Detail(Box<LegalDetailReply>),
}

pub struct LegalHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn LegalHttpBackend>,
}

pub struct LegalHttpService {
    targets: BTreeMap<(String, String), LegalHttpTarget>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LegalRegisterQueryWire {
    pub kind: Option<String>,
    pub search: Option<String>,
    pub state: Option<String>,
    pub page: Option<i64>,
    pub mine: Option<String>,
}

struct LegalHttpState {
    service: Arc<LegalHttpService>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed(message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, "LEGAL_REQUEST_INVALID", message)
}

fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (rest, None),
        };
        !host.is_empty()
            && !host
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
            && port.is_none_or(|port| {
                (1..=5).contains(&port.len())
                    && port.bytes().all(|byte| byte.is_ascii_digit())
                    && port.parse::<u16>().is_ok_and(|value| value > 0)
            })
    })
}

fn unique_header(
    headers: &HeaderMap,
    name: header::HeaderName,
) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(malformed("Use one unambiguous request header value."));
    }
    first
        .map(|value| {
            value
                .to_str()
                .map_err(|_| malformed("Use a valid request header."))
        })
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
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn bounded<T: Serialize>(value: &T) -> Response {
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

impl LegalHttpService {
    pub fn new(targets: Vec<LegalHttpTarget>) -> Result<Self, BusinessError> {
        let mut configured = BTreeMap::new();
        for target in targets {
            let key = (target.host.clone(), target.holding_slug.clone());
            if target.host.is_empty()
                || target.holding_slug.is_empty()
                || !valid_origin(&target.origin)
                || configured.contains_key(&key)
            {
                return Err(BusinessError::invalid(
                    "LEGAL_HTTP_CONFIGURATION_INVALID",
                    "Use exact distinct registered Holding targets.",
                ));
            }
            configured.insert(key, target);
        }
        Ok(Self {
            targets: configured,
        })
    }

    fn target<'a>(
        &'a self,
        headers: &HeaderMap,
        slug: &str,
    ) -> Result<&'a LegalHttpTarget, BusinessError> {
        let host = unique_header(headers, header::HOST)?.ok_or_else(missing)?;
        self.targets
            .get(&(host.to_owned(), slug.to_owned()))
            .ok_or_else(missing)
    }

    pub async fn register(
        &self,
        host: &str,
        holding_slug: &str,
        headers: &HeaderMap,
        query: LegalRegisterQuery,
    ) -> Response {
        let target = match self.target_for_host(headers, holding_slug, host) {
            Ok(target) => target,
            Err(error) => return error_response(error),
        };
        match target
            .backend
            .read(LegalHttpReadRequest {
                host,
                holding_slug,
                headers,
                read: LegalHttpRead::Register(query),
            })
            .await
        {
            Ok(LegalHttpReadReply::Register(reply)) => bounded(&serde_json::json!({
                "data": reply.data,
                "context": reply.context,
            })),
            Ok(_) => error_response(BusinessError::new(
                FailureKind::Unavailable,
                "LEGAL_REPLY_INVALID",
                "The Legal register reply was invalid.",
            )),
            Err(error) => error_response(error),
        }
    }

    pub async fn detail(
        &self,
        host: &str,
        holding_slug: &str,
        headers: &HeaderMap,
        id: &str,
    ) -> Response {
        let target = match self.target_for_host(headers, holding_slug, host) {
            Ok(target) => target,
            Err(error) => return error_response(error),
        };
        match target
            .backend
            .read(LegalHttpReadRequest {
                host,
                holding_slug,
                headers,
                read: LegalHttpRead::Detail(id.to_owned()),
            })
            .await
        {
            Ok(LegalHttpReadReply::Detail(reply)) => bounded(&serde_json::json!({
                "data": reply.data,
                "context": reply.context,
            })),
            Ok(_) => error_response(BusinessError::new(
                FailureKind::Unavailable,
                "LEGAL_REPLY_INVALID",
                "The Legal detail reply was invalid.",
            )),
            Err(error) => error_response(error),
        }
    }

    pub async fn command(
        &self,
        host: &str,
        holding_slug: &str,
        headers: &HeaderMap,
        body: &[u8],
    ) -> Response {
        let target = match self.target_for_host(headers, holding_slug, host) {
            Ok(target) => target,
            Err(error) => return error_response(error),
        };
        let origin = match unique_header(headers, header::ORIGIN) {
            Ok(origin) => origin,
            Err(error) => return error_response(error),
        };
        if origin != Some(target.origin.as_str()) {
            return error_response(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            ));
        }
        let key = match unique_header(headers, HeaderName::from_static("idempotency-key")) {
            Ok(Some(key)) => match CommandKey::new(key) {
                Ok(key) => key,
                Err(error) => return error_response(error),
            },
            Ok(None) => {
                return error_response(BusinessError::new(
                    FailureKind::InvalidInput,
                    "COMMAND_KEY_REQUIRED",
                    "Idempotency-Key is required.",
                ));
            }
            Err(error) => return error_response(error),
        };
        if body.len() > BODY_LIMIT {
            return error_response(BusinessError::new(
                FailureKind::InvalidInput,
                "BODY_TOO_LARGE",
                "Use a smaller Legal request.",
            ));
        }
        let request_hash = crate::domain::legal_records::sha256_hex(body);
        let body: Value = match serde_json::from_slice(body) {
            Ok(value @ Value::Object(_)) => value,
            _ => {
                return error_response(BusinessError::new(
                    FailureKind::InvalidInput,
                    "INVALID_JSON",
                    "Use a JSON object body.",
                ));
            }
        };
        match target
            .backend
            .command(LegalHttpCommandRequest {
                host,
                holding_slug,
                headers,
                body: &body,
                key: key.as_str(),
                request_hash: &request_hash,
            })
            .await
        {
            Ok(reply) => bounded(&serde_json::json!({
                "data": reply.data,
                "context": reply.context,
            })),
            Err(error) => error_response(error),
        }
    }

    fn target_for_host<'a>(
        &'a self,
        headers: &HeaderMap,
        slug: &str,
        host: &str,
    ) -> Result<&'a LegalHttpTarget, BusinessError> {
        if host.is_empty() {
            return self.target(headers, slug);
        }
        let registered = unique_header(headers, header::HOST)?.ok_or_else(missing)?;
        if registered != host {
            return Err(missing());
        }
        self.targets
            .get(&(host.to_owned(), slug.to_owned()))
            .ok_or_else(missing)
    }

    pub fn request_hash(body: &[u8]) -> String {
        crate::domain::legal_records::sha256_hex(body)
    }
}

pub fn legal_router(targets: Vec<LegalHttpTarget>) -> Result<Router, BusinessError> {
    let service = Arc::new(LegalHttpService::new(targets)?);
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/legal/records",
            get(read_register),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/legal/records/{id}",
            get(read_detail),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/legal/commands",
            post(command),
        )
        .with_state(Arc::new(LegalHttpState { service })))
}

fn query(slug: &str, wire: LegalRegisterQueryWire) -> Result<LegalRegisterQuery, BusinessError> {
    if slug.is_empty() {
        return Err(missing());
    }
    let page = wire.page.unwrap_or(1);
    if !(1..=REGISTER_MAX_PAGE).contains(&page) {
        return Err(malformed("Choose a current Legal page."));
    }
    let mine = match wire.mine.as_deref() {
        None | Some("false") => false,
        Some("true") => true,
        _ => return Err(malformed("Choose a current Legal scope.")),
    };
    LegalRegisterQuery::parse(
        wire.kind.as_deref(),
        wire.search.as_deref().unwrap_or(""),
        wire.state.as_deref().unwrap_or(""),
        page,
        mine,
    )
}

async fn read_register(
    State(state): State<Arc<LegalHttpState>>,
    Path(holding_slug): Path<String>,
    Query(wire): Query<LegalRegisterQueryWire>,
    headers: HeaderMap,
) -> Response {
    let query = match query(&holding_slug, wire) {
        Ok(query) => query,
        Err(error) => return error_response(error),
    };
    let host = unique_header(&headers, header::HOST)
        .ok()
        .flatten()
        .unwrap_or_default();
    state
        .service
        .register(host, &holding_slug, &headers, query)
        .await
}

async fn read_detail(
    State(state): State<Arc<LegalHttpState>>,
    Path((holding_slug, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let host = unique_header(&headers, header::HOST)
        .ok()
        .flatten()
        .unwrap_or_default();
    state
        .service
        .detail(host, &holding_slug, &headers, &id)
        .await
}

async fn command(
    State(state): State<Arc<LegalHttpState>>,
    Path(holding_slug): Path<String>,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => {
            return error_response(BusinessError::new(
                FailureKind::InvalidInput,
                "BODY_TOO_LARGE",
                "Use a smaller Legal request.",
            ));
        }
    };
    let host = unique_header(&headers, header::HOST)
        .ok()
        .flatten()
        .unwrap_or_default();
    state
        .service
        .command(host, &holding_slug, &headers, &body)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::legal_records::LegalRegisterQuery;
    use erpx_server_runtime::public::http_support::body::to_bytes as read_body;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Backend {
        reads: Mutex<Vec<String>>,
        command: Mutex<Option<Value>>,
        fail: Mutex<Option<BusinessError>>,
    }

    impl LegalHttpBackend for Backend {
        fn read<'a>(
            &'a self,
            request: LegalHttpReadRequest<'a>,
        ) -> LegalHttpFuture<'a, LegalHttpReadReply> {
            self.reads.lock().unwrap().push(match &request.read {
                LegalHttpRead::Register(query) => format!("register:{}", query.page),
                LegalHttpRead::Detail(id) => format!("detail:{id}"),
            });
            let fail = self.fail.lock().unwrap().clone();
            Box::pin(async move {
                if let Some(error) = fail {
                    return Err(error);
                }
                Ok(match &request.read {
                    LegalHttpRead::Register(_) => {
                        LegalHttpReadReply::Register(Box::new(LegalRegisterReply {
                            data: LegalRegister {
                                records: Vec::new(),
                                total: 0,
                                page: 1,
                                counts: Vec::new(),
                                enabled: true,
                                can_edit: true,
                                can_hold: true,
                                can_release_hold: true,
                                can_share: true,
                            },
                            context: test_context(),
                        }))
                    }
                    LegalHttpRead::Detail(_) => {
                        LegalHttpReadReply::Detail(Box::new(LegalDetailReply {
                            data: LegalDetail {
                                item: crate::domain::legal_records::LegalRecord {
                                    id: "item".into(),
                                    company_id: "company".into(),
                                    branch_id: None,
                                    department_id: None,
                                    kind: "contract".into(),
                                    title: "Contract".into(),
                                    description: String::new(),
                                    classification: "internal".into(),
                                    country: "TH".into(),
                                    jurisdiction: "Thailand".into(),
                                    language: "th".into(),
                                    currency: "THB".into(),
                                    timezone: "Asia/Bangkok".into(),
                                    priority: "normal".into(),
                                    due_date: None,
                                    owner_id: "user".into(),
                                    created_by: "user".into(),
                                    state: "draft".into(),
                                    version: 1,
                                    created_at: 0,
                                    updated_at: 0,
                                    data: serde_json::json!({}),
                                },
                                documents: Vec::new(),
                                events: Vec::new(),
                                members: Vec::new(),
                                holds: Vec::new(),
                                links: Vec::new(),
                                comments: Vec::new(),
                                envelopes: Vec::new(),
                                shares: Vec::new(),
                                submissions: Vec::new(),
                                access: Vec::new(),
                                can_edit: true,
                                can_hold: true,
                                can_release_hold: true,
                                can_share: true,
                            },
                            context: test_context(),
                        }))
                    }
                })
            })
        }

        fn command<'a>(
            &'a self,
            request: LegalHttpCommandRequest<'a>,
        ) -> LegalHttpFuture<'a, LegalCommandReply> {
            self.reads
                .lock()
                .unwrap()
                .push(format!("command:{}", request.key));
            let fail = self.fail.lock().unwrap().clone();
            let result =
                self.command.lock().unwrap().clone().unwrap_or_else(
                    || serde_json::json!({"id":"item","version":2,"state":"draft"}),
                );
            Box::pin(async move {
                if let Some(error) = fail {
                    return Err(error);
                }
                Ok(LegalCommandReply {
                    data: result,
                    context: test_context(),
                })
            })
        }
    }

    fn test_context() -> LegalRenderedContext {
        LegalRenderedContext {
            user_id: "user".into(),
            holding_id: "holding".into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            assignment_id: "assignment".into(),
            assignment_version: 2,
            revision: 4,
            role: "legal_manager".into(),
            modules: vec!["legal".into()],
            permissions: vec!["legal.read".into()],
            expires_at: 9_007_199_254_740_991,
        }
    }

    fn target(backend: Arc<Backend>) -> LegalHttpTarget {
        LegalHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend,
        }
    }

    fn headers(origin: Option<&str>, key: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "acme.example.test".parse().unwrap());
        if let Some(origin) = origin {
            headers.insert(header::ORIGIN, origin.parse().unwrap());
        }
        if let Some(key) = key {
            headers.insert(
                HeaderName::from_static("idempotency-key"),
                key.parse().unwrap(),
            );
        }
        headers
    }

    fn body<T: Serialize>(value: &T) -> String {
        serde_json::to_string(value).unwrap()
    }

    fn response_body(response: Response) -> serde_json::Value {
        let bytes = block_on(read_body(response.into_body(), 1 << 20)).unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn configuration_rejects_duplicate_or_unsafe_targets() {
        let backend = Arc::new(Backend::default());
        assert!(LegalHttpService::new(vec![target(backend.clone())]).is_ok());
        let mut contributor_target = target(backend.clone());
        contributor_target.origin = "https://acme.example.test:3443".into();
        assert!(LegalHttpService::new(vec![contributor_target]).is_ok());
        assert!(
            LegalHttpService::new(vec![target(backend.clone()), target(backend.clone())]).is_err()
        );
        for origin in [
            "http://acme.example.test",
            "https://acme.example.test:0",
            "https://acme.example.test:65536",
            "https://acme.example.test:abc",
            "https://acme.example.test:3443:1",
        ] {
            let mut unsafe_target = target(backend.clone());
            unsafe_target.origin = origin.into();
            assert!(LegalHttpService::new(vec![unsafe_target]).is_err());
        }
    }

    #[test]
    fn register_requires_the_registered_host_and_returns_bounded_json() {
        let backend = Arc::new(Backend::default());
        let service = LegalHttpService::new(vec![target(backend.clone())]).unwrap();
        let response = block_on(service.register(
            "acme.example.test",
            "acme",
            &headers(None, None),
            LegalRegisterQuery::parse(Some("contract"), "", "", 1, false).unwrap(),
        ));
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_body(response);
        assert_eq!(body["data"]["records"], serde_json::json!([]));
        assert_eq!(body["context"]["companyId"], serde_json::json!("company"));
        assert_eq!(body["context"]["revision"], serde_json::json!(4));
        assert_eq!(backend.reads.lock().unwrap().as_slice(), ["register:1"]);

        let mut foreign = headers(None, None);
        foreign.insert(header::HOST, "other.example.test".parse().unwrap());
        let response = block_on(service.register(
            "",
            "acme",
            &foreign,
            LegalRegisterQuery::parse(None, "", "", 1, false).unwrap(),
        ));
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn command_requires_the_exact_origin_and_idempotency_key() {
        let backend = Arc::new(Backend::default());
        let service = LegalHttpService::new(vec![target(backend.clone())]).unwrap();
        let raw = serde_json::json!({"action":"document","id":"item","expectedVersion":1,"name":"Agreement","body":"text"});
        let bytes = body(&raw);
        let response = block_on(service.command(
            "acme.example.test",
            "acme",
            &headers(None, Some("legal-command-key-0001")),
            bytes.as_bytes(),
        ));
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let response = block_on(service.command(
            "acme.example.test",
            "acme",
            &headers(Some("https://acme.example.test"), None),
            bytes.as_bytes(),
        ));
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let response = block_on(service.command(
            "acme.example.test",
            "acme",
            &headers(
                Some("https://acme.example.test"),
                Some("legal-command-key-0001"),
            ),
            bytes.as_bytes(),
        ));
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_body(response);
        assert_eq!(body["data"]["version"], serde_json::json!(2));
        assert_eq!(
            body["context"]["assignmentId"],
            serde_json::json!("assignment")
        );
    }

    #[test]
    fn command_rejects_oversized_and_non_object_bodies_before_the_backend() {
        let backend = Arc::new(Backend::default());
        let service = LegalHttpService::new(vec![target(backend.clone())]).unwrap();
        let mut headers = headers(
            Some("https://acme.example.test"),
            Some("legal-command-key-0001"),
        );
        let oversized = vec![b'a'; BODY_LIMIT + 1];
        let response = block_on(service.command("acme.example.test", "acme", &headers, &oversized));
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        headers.remove(HeaderName::from_static("idempotency-key"));
        let response = block_on(service.command("acme.example.test", "acme", &headers, b"[]"));
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(backend.reads.lock().unwrap().is_empty());
    }

    #[test]
    fn backend_failures_map_to_retained_statuses() {
        let backend = Arc::new(Backend::default());
        *backend.fail.lock().unwrap() = Some(BusinessError::new(
            FailureKind::Conflict,
            "LEGAL_VERSION_CONFLICT",
            "This record changed. Refresh before saving.",
        ));
        let service = LegalHttpService::new(vec![target(backend)]).unwrap();
        let response =
            block_on(service.detail("acme.example.test", "acme", &headers(None, None), "item"));
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(
            response_body(response)["error"]["code"],
            serde_json::json!("LEGAL_VERSION_CONFLICT")
        );
    }

    #[test]
    fn query_parser_keeps_page_and_scope_rules() {
        assert_eq!(
            query(
                "acme",
                LegalRegisterQueryWire {
                    page: Some(2),
                    ..Default::default()
                }
            )
            .unwrap()
            .page,
            2
        );
        assert!(
            query(
                "acme",
                LegalRegisterQueryWire {
                    page: Some(0),
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            query(
                "acme",
                LegalRegisterQueryWire {
                    mine: Some("maybe".into()),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }

    fn block_on<T>(future: impl Future<Output = T>) -> T {
        let mut future = Box::pin(future);
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(value) => value,
            std::task::Poll::Pending => panic!("test future unexpectedly pending"),
        }
    }
}
