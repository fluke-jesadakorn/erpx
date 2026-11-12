//! Native HTTP ingress for the Collaboration handbook.
//!
//! Transport facts only: exact registered Holding host/slug, browser origin,
//! one idempotency key, bounded JSON and error mapping. Normal Account session
//! resolution, current Company membership/editor authority, purchase access and
//! the serializable owner unit belong to the composition backend, which must
//! implement [`HandbookHttpBackend`]. The router never mints an actor, Company
//! or permission.
use crate::adapters::handbook::{
    HandbookArticleRead, HandbookArticleSummary, HandbookEditorOptions, HandbookEditorRow,
    HandbookHistoryRow, HandbookListPage, HandbookListRow, HandbookRevisionRow,
};
use crate::adapters::handbook_wire::decode_handbook_command;
use crate::domain::handbook::{Change, ListCursor, Receipt};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Query, Request, Response, Router,
    State, StatusCode,
    body::to_bytes,
    header,
    routing::{get, post},
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 192_000;
const DEFAULT_PAGE: usize = 50;

pub type HandbookHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandbookListData {
    /// Composition-supplied current `{owner,editor,purchase?}` facts; never a
    /// replacement for the owner authorization performed in the backend.
    pub access: Value,
    pub articles: Vec<HandbookListRow>,
    #[serde(rename = "previousCursor")]
    pub previous_cursor: Option<String>,
    #[serde(rename = "nextCursor")]
    pub next_cursor: Option<String>,
    #[serde(rename = "hasMore")]
    pub has_more: bool,
}

impl HandbookListData {
    pub fn from_page(access: Value, page: HandbookListPage) -> Self {
        Self {
            access,
            articles: page.items,
            previous_cursor: page.previous_cursor,
            next_cursor: page.next_cursor,
            has_more: page.has_more,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandbookDetailData {
    pub access: Value,
    pub article: HandbookArticleSummary,
    pub revision: HandbookRevisionRow,
    pub history: Vec<HandbookHistoryRow>,
}

impl HandbookDetailData {
    pub fn from_read(access: Value, read: HandbookArticleRead) -> Self {
        Self {
            access,
            article: read.article,
            revision: read.revision,
            history: read.history,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct HandbookEditorsData {
    pub version: i64,
    pub editors: Vec<HandbookEditorRow>,
}

impl From<HandbookEditorOptions> for HandbookEditorsData {
    fn from(value: HandbookEditorOptions) -> Self {
        Self {
            version: value.version,
            editors: value.editors,
        }
    }
}

#[derive(Clone)]
pub enum HandbookReadKind {
    List {
        query: String,
        editing: bool,
        cursor: Option<String>,
        limit: usize,
    },
    Article {
        id: String,
        editing: bool,
        revision: Option<i64>,
    },
    Editors,
}

pub struct HandbookHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub kind: HandbookReadKind,
}

pub struct HandbookHttpCommandRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub change: Change,
    pub input: Value,
    pub key: CommandKey,
}

pub enum HandbookHttpReadReply {
    List(Box<HandbookListData>),
    Article(Box<HandbookDetailData>),
    Editors(Box<HandbookEditorsData>),
}

/// Composition-owned session, scope and owner-unit adapter. The router never
/// resolves a session or invents Company/editor authority.
pub trait HandbookHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: HandbookHttpReadRequest<'a>,
    ) -> HandbookHttpFuture<'a, HandbookHttpReadReply>;
    fn command<'a>(
        &'a self,
        request: HandbookHttpCommandRequest<'a>,
    ) -> HandbookHttpFuture<'a, Receipt>;
}

#[derive(Clone)]
pub struct HandbookHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn HandbookHttpBackend>,
}

struct HandbookHttpState {
    targets: BTreeMap<(String, String), HandbookHttpTarget>,
}

#[derive(Deserialize)]
struct ReadQuery {
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    edit: Option<String>,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    revision: Option<String>,
    #[serde(default)]
    limit: Option<String>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "HANDBOOK_REQUEST_INVALID",
        "Use one unambiguous request header value.",
    )
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(request_invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| request_invalid()))
        .transpose()
}

fn valid_origin(host: &str, origin: &str) -> bool {
    let expected = format!("https://{host}");
    if origin != expected || host.is_empty() {
        return false;
    }
    if !host
        .bytes()
        .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
    {
        return true;
    }
    // The keyed contributor workspace uses its exact local HTTPS port.
    host.strip_suffix(":3443")
        .is_some_and(|name| name.ends_with(".erpx.localhost"))
}

fn key_valid(value: &str) -> bool {
    let len = value.len();
    (8..=180).contains(&len)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'.' | b'-'))
}

pub fn handbook_router(targets: Vec<HandbookHttpTarget>) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.holding_slug.is_empty()
            || !valid_origin(&target.host, &target.origin)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "HANDBOOK_HTTP_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/handbook",
            get(read_list).post(create_article),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/handbook/editors",
            get(read_editors).post(set_editor),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/handbook/{record_id}",
            get(read_article).post(update_article),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/handbook/{record_id}/actions",
            post(article_action),
        )
        .with_state(Arc::new(HandbookHttpState {
            targets: configured,
        })))
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

fn bounded<T: Serialize>(body: T) -> Response {
    let mut response = (StatusCode::OK, Json(json!({ "data": body }))).into_response();
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
    state: &'a HandbookHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a HandbookHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn check_origin(target: &HandbookHttpTarget, headers: &HeaderMap) -> Result<(), BusinessError> {
    match unique_header(headers, header::ORIGIN)? {
        Some(value) if value == target.origin => Ok(()),
        _ => Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the application origin.",
        )),
    }
}

fn boolean(value: Option<&str>) -> bool {
    value == Some("1")
}

fn read_limit(value: Option<&str>) -> Result<usize, BusinessError> {
    match value {
        None => Ok(DEFAULT_PAGE),
        Some(raw) => raw
            .parse::<usize>()
            .ok()
            .filter(|value| (1..=100).contains(value))
            .ok_or_else(|| {
                BusinessError::invalid("HANDBOOK_LIMIT_INVALID", "Choose a smaller page.")
            }),
    }
}

fn revision(value: Option<&str>) -> Result<Option<i64>, BusinessError> {
    match value {
        None => Ok(None),
        Some(raw) => raw
            .parse::<i64>()
            .ok()
            .filter(|value| *value >= 1 && *value <= crate::domain::handbook::MAX_SAFE_VERSION)
            .map(Some)
            .ok_or_else(|| {
                BusinessError::new(
                    FailureKind::NotFound,
                    "ARTICLE_NOT_FOUND",
                    "Article revision not found.",
                )
            }),
    }
}

fn search(value: Option<&str>) -> Result<String, BusinessError> {
    let value = value.unwrap_or("");
    if value.encode_utf16().count() > 100 {
        return Err(BusinessError::invalid(
            "HANDBOOK_QUERY_INVALID",
            "Shorten the handbook search.",
        ));
    }
    Ok(value.to_owned())
}

fn cursor(value: Option<&str>) -> Result<Option<String>, BusinessError> {
    value
        .map(ListCursor::decode)
        .transpose()
        .map(|value| value.map(|cursor| cursor.encode()))
}

async fn read_list(
    State(state): State<Arc<HandbookHttpState>>,
    Path(slug): Path<String>,
    Query(query): Query<ReadQuery>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let target = target(&state, &headers, &slug)?;
        target
            .backend
            .read(HandbookHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                kind: HandbookReadKind::List {
                    query: search(query.q.as_deref())?,
                    editing: boolean(query.edit.as_deref()),
                    cursor: cursor(query.cursor.as_deref())?,
                    limit: read_limit(query.limit.as_deref())?,
                },
            })
            .await
    }
    .await;
    match result {
        Ok(HandbookHttpReadReply::List(reply)) => bounded(*reply),
        Ok(_) => error_response(BusinessError::invalid(
            "HANDBOOK_HTTP_RESPONSE_INVALID",
            "The native handbook reader returned the wrong shape.",
        )),
        Err(error) => error_response(error),
    }
}

async fn read_editors(
    State(state): State<Arc<HandbookHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let target = target(&state, &headers, &slug)?;
        target
            .backend
            .read(HandbookHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                kind: HandbookReadKind::Editors,
            })
            .await
    }
    .await;
    match result {
        Ok(HandbookHttpReadReply::Editors(reply)) => bounded(*reply),
        Ok(_) => error_response(BusinessError::invalid(
            "HANDBOOK_HTTP_RESPONSE_INVALID",
            "The native handbook reader returned the wrong shape.",
        )),
        Err(error) => error_response(error),
    }
}

async fn read_article(
    State(state): State<Arc<HandbookHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    Query(query): Query<ReadQuery>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let target = target(&state, &headers, &slug)?;
        target
            .backend
            .read(HandbookHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                kind: HandbookReadKind::Article {
                    id,
                    editing: boolean(query.edit.as_deref()),
                    revision: revision(query.revision.as_deref())?,
                },
            })
            .await
    }
    .await;
    match result {
        Ok(HandbookHttpReadReply::Article(reply)) => bounded(*reply),
        Ok(_) => error_response(BusinessError::invalid(
            "HANDBOOK_HTTP_RESPONSE_INVALID",
            "The native handbook reader returned the wrong shape.",
        )),
        Err(error) => error_response(error),
    }
}

struct PreparedCommand {
    target: HandbookHttpTarget,
    change: Change,
    input: Value,
    key: CommandKey,
}

async fn prepare_command(
    state: Arc<HandbookHttpState>,
    slug: String,
    request: Request,
    operation: String,
    id: Option<String>,
) -> Result<PreparedCommand, BusinessError> {
    let target = target(&state, request.headers(), &slug)?.clone();
    check_origin(&target, request.headers())?;
    let key = unique_header(
        request.headers(),
        HeaderName::from_static("idempotency-key"),
    )?
    .filter(|value| key_valid(value))
    .ok_or_else(|| {
        BusinessError::invalid(
            "COMMAND_KEY_REQUIRED",
            "A valid Idempotency-Key is required.",
        )
    })?;
    let key = CommandKey::new(key)?;
    let body = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| {
            BusinessError::invalid("INPUT_TOO_LARGE", "Shorten the content before saving.")
        })?;
    let raw: Value = serde_json::from_slice(&body)
        .map_err(|_| BusinessError::invalid("INVALID_JSON", "Provide a valid handbook command."))?;
    let decoded = decode_handbook_command(&operation, id.as_deref(), &raw)?;
    Ok(PreparedCommand {
        target,
        change: decoded.change,
        input: decoded.input,
        key,
    })
}

async fn execute_command(prepared: PreparedCommand, headers: HeaderMap) -> Response {
    let PreparedCommand {
        target,
        change,
        input,
        key,
    } = prepared;
    let result = target
        .backend
        .command(HandbookHttpCommandRequest {
            host: &target.host,
            holding_slug: &target.holding_slug,
            headers: &headers,
            change,
            input,
            key,
        })
        .await;
    match result {
        Ok(receipt) => bounded(receipt),
        Err(error) => error_response(error),
    }
}

fn action_operation(raw: &Value) -> String {
    raw.get("action")
        .and_then(Value::as_str)
        .map(|action| format!("handbook.article.{action}"))
        .unwrap_or_else(|| "handbook.article.unknown".to_owned())
}

async fn create_article(
    State(state): State<Arc<HandbookHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let prepared = prepare_command(
        state,
        slug,
        request,
        "handbook.article.create".to_owned(),
        None,
    )
    .await;
    match prepared {
        Ok(command) => execute_command(command, headers).await,
        Err(error) => error_response(error),
    }
}

async fn update_article(
    State(state): State<Arc<HandbookHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let prepared = prepare_command(
        state,
        slug,
        request,
        "handbook.article.update".to_owned(),
        Some(id),
    )
    .await;
    match prepared {
        Ok(command) => execute_command(command, headers).await,
        Err(error) => error_response(error),
    }
}

async fn article_action(
    State(state): State<Arc<HandbookHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let target = match target(&state, &headers, &slug) {
        Ok(target) => target.clone(),
        Err(error) => return error_response(error),
    };
    if let Err(error) = check_origin(&target, &headers) {
        return error_response(error);
    }
    let key = match unique_header(&headers, HeaderName::from_static("idempotency-key")) {
        Ok(Some(value)) if key_valid(value) => match CommandKey::new(value) {
            Ok(key) => key,
            Err(error) => return error_response(error),
        },
        _ => {
            return error_response(BusinessError::invalid(
                "COMMAND_KEY_REQUIRED",
                "A valid Idempotency-Key is required.",
            ));
        }
    };
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => {
            return error_response(BusinessError::invalid(
                "INPUT_TOO_LARGE",
                "Shorten the content before saving.",
            ));
        }
    };
    let raw: Value = match serde_json::from_slice(&body) {
        Ok(raw) => raw,
        Err(_) => {
            return error_response(BusinessError::invalid(
                "INVALID_JSON",
                "Provide a valid handbook command.",
            ));
        }
    };
    if !raw.is_object() {
        return error_response(BusinessError::invalid(
            "HANDBOOK_INPUT_INVALID",
            "Provide a valid handbook value.",
        ));
    }
    let decoded = match decode_handbook_command(&action_operation(&raw), Some(&id), &raw) {
        Ok(decoded) => decoded,
        Err(error) => return error_response(error),
    };
    execute_command(
        PreparedCommand {
            target,
            change: decoded.change,
            input: decoded.input,
            key,
        },
        headers,
    )
    .await
}

async fn set_editor(
    State(state): State<Arc<HandbookHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let prepared = prepare_command(
        state,
        slug,
        request,
        "handbook.editor.update".to_owned(),
        None,
    )
    .await;
    match prepared {
        Ok(command) => execute_command(command, headers).await,
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::handbook::Category;
    use erpx_server_runtime::public::http_support::body::Body;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeBackend {
        reads: Mutex<Vec<(String, String, String)>>,
        commands: Mutex<Vec<(String, String, Change)>>,
        failure: Option<BusinessError>,
    }

    impl FakeBackend {
        fn failing(kind: FailureKind, code: &'static str) -> Self {
            Self {
                failure: Some(BusinessError::new(kind, code, "denied")),
                ..Self::default()
            }
        }
    }

    impl HandbookHttpBackend for FakeBackend {
        fn read<'a>(
            &'a self,
            request: HandbookHttpReadRequest<'a>,
        ) -> HandbookHttpFuture<'a, HandbookHttpReadReply> {
            let kind = match request.kind {
                HandbookReadKind::List { .. } => "list",
                HandbookReadKind::Article { .. } => "article",
                HandbookReadKind::Editors => "editors",
            };
            self.reads.lock().unwrap().push((
                request.host.into(),
                request.holding_slug.into(),
                kind.into(),
            ));
            Box::pin(async move {
                if let Some(error) = &self.failure {
                    return Err(error.clone());
                }
                match request.kind {
                    HandbookReadKind::List { .. } => {
                        Ok(HandbookHttpReadReply::List(Box::new(HandbookListData {
                            access: json!({"editor": true}),
                            articles: vec![],
                            previous_cursor: None,
                            next_cursor: Some("1:article".into()),
                            has_more: true,
                        })))
                    }
                    HandbookReadKind::Article { id, .. } => Ok(HandbookHttpReadReply::Article(
                        Box::new(HandbookDetailData {
                            access: json!({"editor": true}),
                            article: HandbookArticleSummary {
                                id,
                                version: 2,
                                draft_revision: Some(2),
                                published_revision: Some(1),
                                archived: 0,
                            },
                            revision: HandbookRevisionRow {
                                revision: 2,
                                title: "Policy".into(),
                                category: "policy".into(),
                                body: "Body".into(),
                                created_by: "editor".into(),
                                created_at: 1,
                                published_by: None,
                                published_at: None,
                            },
                            history: vec![],
                        }),
                    )),
                    HandbookReadKind::Editors => Ok(HandbookHttpReadReply::Editors(Box::new(
                        HandbookEditorsData {
                            version: 0,
                            editors: vec![],
                        },
                    ))),
                }
            })
        }

        fn command<'a>(
            &'a self,
            request: HandbookHttpCommandRequest<'a>,
        ) -> HandbookHttpFuture<'a, Receipt> {
            self.commands.lock().unwrap().push((
                request.host.into(),
                request.holding_slug.into(),
                request.change.clone(),
            ));
            Box::pin(async move {
                if let Some(error) = &self.failure {
                    return Err(error.clone());
                }
                Ok(Receipt {
                    id: "article-key".into(),
                    version: 1,
                    revision: 1,
                    status: "draft",
                })
            })
        }
    }

    fn make_target(backend: Arc<FakeBackend>) -> HandbookHttpTarget {
        HandbookHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend,
        }
    }

    fn state(target: HandbookHttpTarget) -> Arc<HandbookHttpState> {
        Arc::new(HandbookHttpState {
            targets: BTreeMap::from([((target.host.clone(), target.holding_slug.clone()), target)]),
        })
    }

    fn query() -> ReadQuery {
        ReadQuery {
            q: None,
            edit: None,
            cursor: None,
            revision: None,
            limit: None,
        }
    }

    fn post(body: &str, origin: Option<&str>, key: Option<&str>) -> Request {
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

    fn get() -> Request {
        Request::builder()
            .method("GET")
            .header("host", "acme.example.test")
            .body(Body::empty())
            .unwrap()
    }

    async fn body(response: Response) -> (StatusCode, Value) {
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[test]
    fn router_rejects_ambiguous_or_unregistered_targets() {
        let backend = Arc::new(FakeBackend::default());
        let mut target = make_target(backend.clone());
        target.origin = "http://acme.example.test".into();
        assert!(handbook_router(vec![target]).is_err());
        let mut target = make_target(backend.clone());
        target.origin = "https://other.example.test".into();
        assert!(handbook_router(vec![target]).is_err());
        assert!(handbook_router(vec![make_target(backend.clone()), make_target(backend)]).is_err());
        assert!(handbook_router(vec![make_target(Arc::new(FakeBackend::default()))]).is_ok());
    }

    #[tokio::test]
    async fn reads_require_exact_host_and_holding_and_return_bounded_envelopes() {
        let backend = Arc::new(FakeBackend::default());
        let state = state(make_target(backend.clone()));
        let response = read_list(
            State(state.clone()),
            Path("acme".into()),
            Query(query()),
            get(),
        )
        .await;
        let (status, value) = body(response).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["data"]["access"]["editor"], true);
        assert_eq!(value["data"]["nextCursor"], "1:article");
        assert_eq!(
            backend.reads.lock().unwrap().as_slice(),
            [("acme.example.test".into(), "acme".into(), "list".into())]
        );

        let response = read_list(
            State(state.clone()),
            Path("other".into()),
            Query(query()),
            get(),
        )
        .await;
        assert_eq!(body(response).await.0, StatusCode::NOT_FOUND);
        let mut wrong = get();
        wrong
            .headers_mut()
            .insert("host", HeaderValue::from_static("other.test"));
        let response = read_editors(State(state), Path("acme".into()), wrong).await;
        assert_eq!(body(response).await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn invalid_cursor_and_limit_reject_before_the_backend() {
        let backend = Arc::new(FakeBackend::default());
        let state = state(make_target(backend.clone()));
        for (cursor, limit) in [
            (Some(""), None),
            (Some("bad"), None),
            (None, Some("0")),
            (None, Some("101")),
        ] {
            let response = read_list(
                State(state.clone()),
                Path("acme".into()),
                Query(ReadQuery {
                    cursor: cursor.map(str::to_owned),
                    limit: limit.map(str::to_owned),
                    ..query()
                }),
                get(),
            )
            .await;
            assert_eq!(body(response).await.0, StatusCode::UNPROCESSABLE_ENTITY);
        }
        assert!(backend.reads.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn create_requires_origin_and_a_bounded_idempotency_key_before_the_owner() {
        let backend = Arc::new(FakeBackend::default());
        let state = state(make_target(backend.clone()));
        let payload = r#"{"title":" Policy ","body":"Body"}"#;
        for request in [
            post(payload, None, Some("handbook-key-0001")),
            post(
                payload,
                Some("https://evil.test"),
                Some("handbook-key-0001"),
            ),
            post(payload, Some("https://acme.example.test"), None),
            post(payload, Some("https://acme.example.test"), Some("short")),
        ] {
            let response = create_article(State(state.clone()), Path("acme".into()), request).await;
            assert!(matches!(
                body(response).await.0,
                StatusCode::FORBIDDEN | StatusCode::UNPROCESSABLE_ENTITY
            ));
        }
        assert!(backend.commands.lock().unwrap().is_empty());
        let response = create_article(
            State(state),
            Path("acme".into()),
            post(
                payload,
                Some("https://acme.example.test"),
                Some("handbook-key-0001"),
            ),
        )
        .await;
        let (status, value) = body(response).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["data"]["id"], "article-key");
        let commands = backend.commands.lock().unwrap();
        assert_eq!(commands.len(), 1);
        match &commands[0].2 {
            Change::CreateArticle {
                title,
                category,
                body,
            } => {
                assert_eq!(title, "Policy");
                assert_eq!(*category, Category::Procedure);
                assert_eq!(body, "Body");
            }
            other => panic!("unexpected change {other:?}"),
        }
    }

    #[tokio::test]
    async fn owner_denial_and_version_conflict_map_to_stable_http_failures() {
        let denied = Arc::new(FakeBackend::failing(
            FailureKind::Forbidden,
            "HANDBOOK_EDITOR_REQUIRED",
        ));
        let state = state(make_target(denied));
        let response = update_article(
            State(state),
            Path(("acme".into(), "article-1".into())),
            post(
                r#"{"title":"Policy","body":"Body","expectedVersion":2}"#,
                Some("https://acme.example.test"),
                Some("handbook-key-0002"),
            ),
        )
        .await;
        let (status, value) = body(response).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(value["error"]["code"], "HANDBOOK_EDITOR_REQUIRED");

        let conflict = Arc::new(FakeBackend::failing(
            FailureKind::Conflict,
            "VERSION_CONFLICT",
        ));
        let response = update_article(
            State(state_target(conflict)),
            Path(("acme".into(), "article-1".into())),
            post(
                r#"{"title":"Policy","body":"Body","expectedVersion":2}"#,
                Some("https://acme.example.test"),
                Some("handbook-key-0003"),
            ),
        )
        .await;
        let (status, value) = body(response).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(value["error"]["code"], "VERSION_CONFLICT");
    }

    fn state_target(backend: Arc<FakeBackend>) -> Arc<HandbookHttpState> {
        state(make_target(backend))
    }

    #[tokio::test]
    async fn action_route_uses_the_same_action_as_the_operation_and_body_must_be_bounded() {
        let backend = Arc::new(FakeBackend::default());
        let state = state(make_target(backend.clone()));
        let response = article_action(
            State(state.clone()),
            Path(("acme".into(), "article-1".into())),
            post(
                r#"{"action":"archive","expectedVersion":2}"#,
                Some("https://acme.example.test"),
                Some("handbook-key-0004"),
            ),
        )
        .await;
        assert_eq!(body(response).await.0, StatusCode::OK);
        match &backend.commands.lock().unwrap()[0].2 {
            Change::ArticleAction {
                action,
                expected_version,
                ..
            } => {
                assert_eq!(*action, crate::domain::handbook::ArticleAction::Archive);
                assert_eq!(*expected_version, 2);
            }
            other => panic!("unexpected change {other:?}"),
        }
        let response = article_action(
            State(state.clone()),
            Path(("acme".into(), "article-1".into())),
            post(
                r#"{"action":"unknown","expectedVersion":2}"#,
                Some("https://acme.example.test"),
                Some("handbook-key-0005"),
            ),
        )
        .await;
        assert_eq!(body(response).await.0, StatusCode::UNPROCESSABLE_ENTITY);
        let oversized = format!(r#"{{"title":"{}","body":"Body"}}"#, "x".repeat(BODY_LIMIT));
        let response = create_article(
            State(state),
            Path("acme".into()),
            post(
                &oversized,
                Some("https://acme.example.test"),
                Some("handbook-key-0006"),
            ),
        )
        .await;
        assert_eq!(body(response).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn editor_and_detail_reads_forward_the_selected_path_without_authority() {
        let backend = Arc::new(FakeBackend::default());
        let state = state(make_target(backend.clone()));
        let response = read_article(
            State(state.clone()),
            Path(("acme".into(), "article-1".into())),
            Query(ReadQuery {
                edit: Some("1".into()),
                revision: Some("2".into()),
                ..query()
            }),
            get(),
        )
        .await;
        let (status, value) = body(response).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["data"]["article"]["id"], "article-1");
        assert_eq!(value["data"]["revision"]["revision"], 2);
        let response = read_editors(
            State(state),
            Path("acme".into()),
            Request::builder()
                .header("host", "acme.example.test")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(body(response).await.0, StatusCode::OK);
        let reads = backend.reads.lock().unwrap();
        assert_eq!(reads[0].2, "article");
        assert_eq!(reads[1].2, "editors");
    }
}
