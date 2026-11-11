//! Native HTTP ingress for Social posts, topics, comments, reactions and
//! follows.
//!
//! Transport facts only: exact registered Holding host/slug, browser origin,
//! one idempotency key, bounded JSON and error mapping. Session resolution,
//! current Company membership/editor authority and the serializable owner unit
//! belong to the composition backend, which must implement
//! [`SocialHttpBackend`]. The router never mints an actor, Company or permission.
use crate::adapters::http_ingress::{
    bounded, command_key, content_invalid, error_response, json_body, unique_header, valid_origin,
};
use crate::adapters::social::{FeedFilter, FeedPost};
use crate::adapters::social_http_wire::decode_social_command;
use crate::domain::social::{Change, FeedCursor, Receipt};
use erpx_server_runtime::public::http_support::{
    HeaderMap, Path, Query, Request, Response, Router, State, header,
    routing::{get, post},
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const DEFAULT_PAGE: usize = 50;

pub type SocialHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SocialFeedData {
    pub items: Vec<FeedPost>,
    #[serde(rename = "nextCursor")]
    pub next_cursor: Option<String>,
    #[serde(rename = "hasMore")]
    pub has_more: bool,
}

impl SocialFeedData {
    pub fn new(items: Vec<FeedPost>, has_more: bool) -> Self {
        let next_cursor = if has_more {
            items.last().map(|post| {
                FeedCursor {
                    at: post.created_at,
                    id: post.id.clone(),
                }
                .encode()
            })
        } else {
            None
        };
        Self {
            items,
            next_cursor,
            has_more,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SocialThreadData {
    pub posts: Vec<FeedPost>,
}

#[derive(Clone)]
pub enum SocialReadKind {
    Feed {
        filter: FeedFilter,
        cursor: Option<FeedCursor>,
        limit: usize,
    },
    Thread {
        post_id: String,
    },
}

pub struct SocialHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub kind: SocialReadKind,
}

pub struct SocialHttpCommandRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub change: Change,
    pub input: Value,
    pub key: CommandKey,
}

pub enum SocialHttpReadReply {
    Feed(Box<SocialFeedData>),
    Thread(Box<SocialThreadData>),
}

/// Composition-owned session, scope and owner-unit adapter. The router never
/// resolves a session or invents Company/member authority.
pub trait SocialHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: SocialHttpReadRequest<'a>,
    ) -> SocialHttpFuture<'a, SocialHttpReadReply>;
    fn command<'a>(
        &'a self,
        request: SocialHttpCommandRequest<'a>,
    ) -> SocialHttpFuture<'a, Receipt>;
}

#[derive(Clone)]
pub struct SocialHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn SocialHttpBackend>,
}

pub struct SocialHttpState {
    targets: BTreeMap<(String, String), SocialHttpTarget>,
}

impl SocialHttpState {
    /// Validate exact distinct registered Holding targets.
    pub fn new(targets: Vec<SocialHttpTarget>) -> Result<Self, BusinessError> {
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
                    "SOCIAL_HTTP_CONFIGURATION_INVALID",
                    "Use exact distinct registered Holding targets.",
                ));
            }
        }
        Ok(Self {
            targets: configured,
        })
    }
}

#[derive(Deserialize)]
pub struct FeedQuery {
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<String>,
}

#[derive(Deserialize)]
pub struct ThreadQuery {
    #[serde(default)]
    pub limit: Option<String>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub fn social_router(targets: Vec<SocialHttpTarget>) -> Result<Router, BusinessError> {
    let state = SocialHttpState::new(targets)?;
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/feed",
            get(read_feed).post(create_post),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/feed/{post_id}",
            get(read_thread),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/feed/{post_id}/comments",
            post(create_comment),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/feed/{post_id}/comments/{comment_id}/actions",
            post(comment_action),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/feed/{post_id}/reactions",
            post(set_reaction),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/feed/{post_id}/follows",
            post(set_post_follow),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/topics/{tag}/follows",
            post(set_topic_follow),
        )
        .with_state(Arc::new(state)))
}

fn target<'a>(
    state: &'a SocialHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a SocialHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn check_origin(target: &SocialHttpTarget, headers: &HeaderMap) -> Result<(), BusinessError> {
    match unique_header(headers, header::ORIGIN)? {
        Some(value) if value == target.origin => Ok(()),
        _ => Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the application origin.",
        )),
    }
}

fn read_limit(value: Option<&str>) -> Result<usize, BusinessError> {
    match value {
        None => Ok(DEFAULT_PAGE),
        Some(raw) => raw
            .parse::<usize>()
            .ok()
            .filter(|value| (1..=100).contains(value))
            .ok_or_else(|| {
                BusinessError::invalid("SOCIAL_LIMIT_INVALID", "Choose a smaller page.")
            }),
    }
}

fn cursor(value: Option<&str>) -> Result<Option<FeedCursor>, BusinessError> {
    value.map(FeedCursor::decode).transpose()
}

pub async fn read_feed(
    State(state): State<Arc<SocialHttpState>>,
    Path(slug): Path<String>,
    Query(query): Query<FeedQuery>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let target = target(&state, &headers, &slug)?;
        target
            .backend
            .read(SocialHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                kind: SocialReadKind::Feed {
                    filter: FeedFilter::parse(query.filter.as_deref().unwrap_or("all"))?,
                    cursor: cursor(query.cursor.as_deref())?,
                    limit: read_limit(query.limit.as_deref())?,
                },
            })
            .await
    }
    .await;
    match result {
        Ok(SocialHttpReadReply::Feed(reply)) => bounded(*reply),
        Ok(_) => error_response(BusinessError::invalid(
            "SOCIAL_HTTP_RESPONSE_INVALID",
            "The native Social reader returned the wrong shape.",
        )),
        Err(error) => error_response(error),
    }
}

pub async fn read_thread(
    State(state): State<Arc<SocialHttpState>>,
    Path((slug, post_id)): Path<(String, String)>,
    Query(query): Query<ThreadQuery>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let target = target(&state, &headers, &slug)?;
        let _ = read_limit(query.limit.as_deref())?;
        target
            .backend
            .read(SocialHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                kind: SocialReadKind::Thread { post_id },
            })
            .await
    }
    .await;
    match result {
        Ok(SocialHttpReadReply::Thread(reply)) => bounded(*reply),
        Ok(_) => error_response(BusinessError::invalid(
            "SOCIAL_HTTP_RESPONSE_INVALID",
            "The native Social reader returned the wrong shape.",
        )),
        Err(error) => error_response(error),
    }
}

struct PreparedSocialCommand {
    target: SocialHttpTarget,
    change: Change,
    input: Value,
    key: CommandKey,
}

async fn prepare_command(
    state: Arc<SocialHttpState>,
    slug: String,
    request: Request,
    operation: &str,
    post_id: Option<String>,
    comment_id: Option<String>,
) -> Result<PreparedSocialCommand, BusinessError> {
    let target = target(&state, request.headers(), &slug)?.clone();
    check_origin(&target, request.headers())?;
    let headers = request.headers().clone();
    let key = command_key(&headers)?;
    let raw = json_body(request).await?;
    if !raw.is_object() {
        return Err(content_invalid());
    }
    let decoded =
        decode_social_command(operation, post_id.as_deref(), comment_id.as_deref(), &raw)?;
    Ok(PreparedSocialCommand {
        target,
        change: decoded.change,
        input: decoded.input,
        key,
    })
}

async fn execute_command(prepared: PreparedSocialCommand, headers: HeaderMap) -> Response {
    let PreparedSocialCommand {
        target,
        change,
        input,
        key,
    } = prepared;
    match target
        .backend
        .command(SocialHttpCommandRequest {
            host: &target.host,
            holding_slug: &target.holding_slug,
            headers: &headers,
            change,
            input,
            key,
        })
        .await
    {
        Ok(receipt) => bounded(receipt),
        Err(error) => error_response(error),
    }
}

async fn command_response(
    prepared: Result<PreparedSocialCommand, BusinessError>,
    headers: HeaderMap,
) -> Response {
    match prepared {
        Ok(command) => execute_command(command, headers).await,
        Err(error) => error_response(error),
    }
}

pub async fn create_post(
    State(state): State<Arc<SocialHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let prepared = prepare_command(state, slug, request, "social.post.publish", None, None).await;
    command_response(prepared, headers).await
}

pub async fn create_comment(
    State(state): State<Arc<SocialHttpState>>,
    Path((slug, post_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let prepared = prepare_command(
        state,
        slug,
        request,
        "social.comment.publish",
        Some(post_id),
        None,
    )
    .await;
    command_response(prepared, headers).await
}

pub async fn comment_action(
    State(state): State<Arc<SocialHttpState>>,
    Path((slug, post_id, comment_id)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let prepared = async {
        let target = target(&state, &headers, &slug)?.clone();
        check_origin(&target, &headers)?;
        let key = command_key(&headers)?;
        let raw = json_body(request).await?;
        let action = raw
            .get("action")
            .and_then(Value::as_str)
            .ok_or_else(content_invalid)?;
        let operation = match action {
            "edit" => "social.comment.edit",
            "remove" => "social.comment.remove",
            _ => return Err(content_invalid()),
        };
        let decoded = decode_social_command(operation, Some(&post_id), Some(&comment_id), &raw)?;
        Ok(PreparedSocialCommand {
            target,
            change: decoded.change,
            input: decoded.input,
            key,
        })
    }
    .await;
    command_response(prepared, headers).await
}

pub async fn set_reaction(
    State(state): State<Arc<SocialHttpState>>,
    Path((slug, post_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let prepared = prepare_command(
        state,
        slug,
        request,
        "social.post.appreciate",
        Some(post_id),
        None,
    )
    .await;
    command_response(prepared, headers).await
}

pub async fn set_post_follow(
    State(state): State<Arc<SocialHttpState>>,
    Path((slug, post_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let prepared = prepare_command(
        state,
        slug,
        request,
        "social.post.follow",
        Some(post_id),
        None,
    )
    .await;
    command_response(prepared, headers).await
}

pub async fn set_topic_follow(
    State(state): State<Arc<SocialHttpState>>,
    Path((slug, tag)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let prepared =
        prepare_command(state, slug, request, "social.topic.follow", Some(tag), None).await;
    command_response(prepared, headers).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::social::FeedComment;
    use erpx_server_runtime::public::http_support::{
        StatusCode,
        body::{Body, to_bytes},
    };
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeBackend {
        reads: Mutex<Vec<(String, String, &'static str)>>,
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

    fn post_row(id: &str) -> FeedPost {
        FeedPost {
            id: id.into(),
            kind: "team".into(),
            author_id: "user".into(),
            author_name: "User".into(),
            body: "Body".into(),
            audience: "Holding",
            branch_id: None,
            linked_record: None,
            version: 1,
            created_at: 10,
            appreciation_count: 0,
            comment_count: 0,
            appreciated: false,
            followed: false,
            topics: vec![],
            comments: vec![FeedComment {
                id: "comment-1".into(),
                post_id: id.into(),
                author_id: "user".into(),
                author_name: "User".into(),
                body: "Reply".into(),
                created_at: 11,
            }],
        }
    }

    impl SocialHttpBackend for FakeBackend {
        fn read<'a>(
            &'a self,
            request: SocialHttpReadRequest<'a>,
        ) -> SocialHttpFuture<'a, SocialHttpReadReply> {
            let kind = match &request.kind {
                SocialReadKind::Feed { .. } => "feed",
                SocialReadKind::Thread { .. } => "thread",
            };
            self.reads.lock().unwrap().push((
                request.host.into(),
                request.holding_slug.into(),
                kind,
            ));
            Box::pin(async move {
                if let Some(error) = &self.failure {
                    return Err(error.clone());
                }
                match request.kind {
                    SocialReadKind::Feed { .. } => Ok(SocialHttpReadReply::Feed(Box::new(
                        SocialFeedData::new(vec![post_row("post-1")], true),
                    ))),
                    SocialReadKind::Thread { post_id } => {
                        Ok(SocialHttpReadReply::Thread(Box::new(SocialThreadData {
                            posts: vec![post_row(&post_id)],
                        })))
                    }
                }
            })
        }

        fn command<'a>(
            &'a self,
            request: SocialHttpCommandRequest<'a>,
        ) -> SocialHttpFuture<'a, Receipt> {
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
                    id: "post-key".into(),
                    version: 1,
                    status: "published",
                })
            })
        }
    }

    fn make_target(backend: Arc<FakeBackend>) -> SocialHttpTarget {
        SocialHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend,
        }
    }

    fn http_state(target: SocialHttpTarget) -> Arc<SocialHttpState> {
        Arc::new(SocialHttpState {
            targets: BTreeMap::from([((target.host.clone(), target.holding_slug.clone()), target)]),
        })
    }

    fn query(filter: Option<&str>) -> FeedQuery {
        FeedQuery {
            filter: filter.map(str::to_owned),
            cursor: None,
            limit: None,
        }
    }

    fn post_request(body: &str, origin: Option<&str>, key: Option<&str>) -> Request {
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

    fn get_request() -> Request {
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
        assert!(social_router(vec![target]).is_err());
        assert!(social_router(vec![make_target(backend.clone()), make_target(backend)]).is_err());
        assert!(social_router(vec![make_target(Arc::new(FakeBackend::default()))]).is_ok());
    }

    #[tokio::test]
    async fn feed_reads_bind_the_exact_host_and_holding() {
        let backend = Arc::new(FakeBackend::default());
        let state = http_state(make_target(backend.clone()));
        let response = read_feed(
            State(state.clone()),
            Path("acme".into()),
            Query(query(Some("following"))),
            get_request(),
        )
        .await;
        let (status, value) = body(response).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["data"]["items"][0]["id"], "post-1");
        assert_eq!(value["data"]["nextCursor"], "10:post-1");
        assert_eq!(value["data"]["hasMore"], true);
        assert_eq!(
            backend.reads.lock().unwrap().as_slice(),
            [("acme.example.test".into(), "acme".into(), "feed".into())]
        );
        let response = read_feed(
            State(state.clone()),
            Path("other".into()),
            Query(query(None)),
            get_request(),
        )
        .await;
        assert_eq!(body(response).await.0, StatusCode::NOT_FOUND);
        let response = read_feed(
            State(state.clone()),
            Path("acme".into()),
            Query(query(Some("bogus"))),
            get_request(),
        )
        .await;
        assert_eq!(body(response).await.0, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(backend.reads.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn commands_require_origin_and_a_bounded_key_before_the_owner() {
        let backend = Arc::new(FakeBackend::default());
        let state = http_state(make_target(backend.clone()));
        let payload = r#"{"body":"Hello","audience":"holding"}"#;
        for request in [
            post_request(payload, None, Some("social-key-0001")),
            post_request(payload, Some("https://evil.test"), Some("social-key-0001")),
            post_request(payload, Some("https://acme.example.test"), None),
            post_request(payload, Some("https://acme.example.test"), Some("short")),
        ] {
            let response = create_post(State(state.clone()), Path("acme".into()), request).await;
            assert!(matches!(
                body(response).await.0,
                StatusCode::FORBIDDEN | StatusCode::UNPROCESSABLE_ENTITY
            ));
        }
        assert!(backend.commands.lock().unwrap().is_empty());
        let response = create_post(
            State(state),
            Path("acme".into()),
            post_request(
                payload,
                Some("https://acme.example.test"),
                Some("social-key-0001"),
            ),
        )
        .await;
        let (status, value) = body(response).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["data"]["id"], "post-key");
        assert_eq!(
            backend.commands.lock().unwrap()[0].2,
            Change::PublishPost {
                body: "Hello".into(),
                audience: crate::domain::social::Audience::Holding,
                tags: vec![],
                mentions: vec![],
                linked_record: None,
            }
        );
    }

    #[tokio::test]
    async fn comment_actions_follow_the_path_action_and_owner_failures_pass_through() {
        let backend = Arc::new(FakeBackend::default());
        let state = http_state(make_target(backend.clone()));
        let response = comment_action(
            State(state.clone()),
            Path(("acme".into(), "post-1".into(), "comment-1".into())),
            post_request(
                r#"{"action":"remove","expectedVersion":2}"#,
                Some("https://acme.example.test"),
                Some("social-key-0002"),
            ),
        )
        .await;
        assert_eq!(body(response).await.0, StatusCode::OK);
        assert!(matches!(
            backend.commands.lock().unwrap()[0].2,
            Change::TombstoneComment {
                expected_version: 2,
                ..
            }
        ));
        let response = comment_action(
            State(state),
            Path(("acme".into(), "post-1".into(), "comment-1".into())),
            post_request(
                r#"{"action":"unknown","expectedVersion":2}"#,
                Some("https://acme.example.test"),
                Some("social-key-0003"),
            ),
        )
        .await;
        assert_eq!(body(response).await.0, StatusCode::UNPROCESSABLE_ENTITY);

        let denied = Arc::new(FakeBackend::failing(
            FailureKind::Forbidden,
            "COMMENT_AUTHOR_REQUIRED",
        ));
        let response = set_reaction(
            State(http_state(make_target(denied))),
            Path(("acme".into(), "post-1".into())),
            post_request(
                r#"{"active":true}"#,
                Some("https://acme.example.test"),
                Some("social-key-0004"),
            ),
        )
        .await;
        let (status, value) = body(response).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(value["error"]["code"], "COMMENT_AUTHOR_REQUIRED");
    }

    #[tokio::test]
    async fn responses_are_private_and_thread_reads_select_the_path_post() {
        let backend = Arc::new(FakeBackend::default());
        let state = http_state(make_target(backend.clone()));
        let response = read_thread(
            State(state),
            Path(("acme".into(), "post-9".into())),
            Query(ThreadQuery { limit: None }),
            get_request(),
        )
        .await;
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "private, no-store"
        );
        let (status, value) = body(response).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["data"]["posts"][0]["id"], "post-9");
        assert_eq!(backend.reads.lock().unwrap()[0].2, "thread");
    }
}
