//! Owner-local HTTP adapter tests for native Social ingress. The exact owner
//! dispatch is exercised without opening a socket; mounting the router into
//! composition remains a lead integration step.
use erpx_server_collaboration::public::social::{FeedComment, FeedPost, Receipt};
use erpx_server_collaboration::public::social_http::{
    FeedQuery, SocialFeedData, SocialHttpBackend, SocialHttpCommandRequest, SocialHttpFuture,
    SocialHttpReadReply, SocialHttpReadRequest, SocialHttpState, SocialHttpTarget, SocialReadKind,
    SocialThreadData, ThreadQuery, comment_action, create_post, read_feed, read_thread,
    set_reaction,
};
use erpx_server_runtime::public::http_support::{
    Path, Query, Request, Response, State, StatusCode,
    body::{Body, to_bytes},
    header,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Probe {
    reads: Mutex<Vec<String>>,
    commands: Mutex<Vec<String>>,
    failure: Option<BusinessError>,
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

impl SocialHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: SocialHttpReadRequest<'a>,
    ) -> SocialHttpFuture<'a, SocialHttpReadReply> {
        let kind = match &request.kind {
            SocialReadKind::Feed { .. } => "feed",
            SocialReadKind::Thread { .. } => "thread",
        };
        self.reads
            .lock()
            .unwrap()
            .push(format!("{kind}|{}|{}", request.host, request.holding_slug));
        Box::pin(async move {
            if let Some(error) = &self.failure {
                return Err(error.clone());
            }
            Ok(match request.kind {
                SocialReadKind::Feed { .. } => SocialHttpReadReply::Feed(Box::new(
                    SocialFeedData::new(vec![post_row("post-1")], true),
                )),
                SocialReadKind::Thread { post_id } => {
                    SocialHttpReadReply::Thread(Box::new(SocialThreadData {
                        posts: vec![post_row(&post_id)],
                    }))
                }
            })
        })
    }

    fn command<'a>(
        &'a self,
        request: SocialHttpCommandRequest<'a>,
    ) -> SocialHttpFuture<'a, Receipt> {
        self.commands.lock().unwrap().push(format!(
            "{:?}|{}|{}",
            request.change, request.host, request.holding_slug
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

fn target(probe: Arc<Probe>) -> SocialHttpTarget {
    SocialHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: probe,
    }
}

fn state(probe: Arc<Probe>) -> Arc<SocialHttpState> {
    Arc::new(SocialHttpState::new(vec![target(probe)]).unwrap())
}

fn request(method: &str, body: &str, key: bool) -> Request {
    let mut builder = Request::builder()
        .method(method)
        .header("host", "acme.example.test")
        .header("content-type", "application/json");
    if method == "POST" {
        builder = builder.header("origin", "https://acme.example.test");
    }
    if key {
        builder = builder.header("idempotency-key", "social-http-key-0001");
    }
    builder.body(Body::from(body.to_owned())).unwrap()
}

fn feed_query(filter: Option<&str>, cursor: Option<&str>, limit: Option<&str>) -> FeedQuery {
    FeedQuery {
        filter: filter.map(str::to_owned),
        cursor: cursor.map(str::to_owned),
        limit: limit.map(str::to_owned),
    }
}

async fn reply(response: Response) -> (StatusCode, Value) {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn feed_and_thread_reads_bind_the_exact_registered_holding() {
    let probe = Arc::new(Probe::default());
    let state = state(probe.clone());

    let response = read_feed(
        State(state.clone()),
        Path("acme".into()),
        Query(feed_query(Some("following"), None, None)),
        request("GET", "", false),
    )
    .await;
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "private, no-store"
    );
    let (status, value) = reply(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["data"]["items"][0]["id"], "post-1");
    assert_eq!(value["data"]["nextCursor"], "10:post-1");

    let response = read_thread(
        State(state.clone()),
        Path(("acme".into(), "post-9".into())),
        Query(ThreadQuery { limit: None }),
        request("GET", "", false),
    )
    .await;
    let (status, value) = reply(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["data"]["posts"][0]["id"], "post-9");

    let mut wrong = request("GET", "", false);
    wrong
        .headers_mut()
        .insert("host", "other.example.test".parse().unwrap());
    let response = read_feed(
        State(state),
        Path("acme".into()),
        Query(feed_query(None, None, None)),
        wrong,
    )
    .await;
    assert_eq!(reply(response).await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        probe.reads.lock().unwrap().as_slice(),
        [
            "feed|acme.example.test|acme",
            "thread|acme.example.test|acme"
        ]
    );
}

#[tokio::test]
async fn post_comment_and_reaction_commands_require_origin_and_key() {
    let probe = Arc::new(Probe::default());
    let state = state(probe.clone());

    let body = json!({
        "body":"Hello #Ops",
        "audience":"holding",
        "tags":["policy"],
        "mentions":[{"kind":"person","targetId":"user-1"}]
    })
    .to_string();
    for req in [request("POST", &body, false), {
        let mut req = request("POST", &body, true);
        req.headers_mut()
            .insert("origin", "https://evil.test".parse().unwrap());
        req
    }] {
        let response = create_post(State(state.clone()), Path("acme".into()), req).await;
        assert!(matches!(
            reply(response).await.0,
            StatusCode::FORBIDDEN | StatusCode::UNPROCESSABLE_ENTITY
        ));
    }
    assert!(probe.commands.lock().unwrap().is_empty());

    let response = create_post(
        State(state.clone()),
        Path("acme".into()),
        request("POST", &body, true),
    )
    .await;
    let (status, value) = reply(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["data"]["id"], "post-key");

    let response = comment_action(
        State(state.clone()),
        Path(("acme".into(), "post-1".into(), "comment-1".into())),
        request("POST", r#"{"action":"remove","expectedVersion":2}"#, true),
    )
    .await;
    assert_eq!(reply(response).await.0, StatusCode::OK);

    let response = set_reaction(
        State(state),
        Path(("acme".into(), "post-1".into())),
        request("POST", r#"{"active":true}"#, true),
    )
    .await;
    assert_eq!(reply(response).await.0, StatusCode::OK);

    let commands = probe.commands.lock().unwrap();
    assert!(commands[0].contains("PublishPost"));
    assert!(commands[1].contains("TombstoneComment"));
    assert!(commands[2].contains("SetReaction"));
}

#[tokio::test]
async fn invalid_filters_cursors_and_owner_failures_are_stable() {
    let probe = Arc::new(Probe {
        failure: Some(BusinessError::new(
            FailureKind::Forbidden,
            "AUDIENCE_FORBIDDEN",
            "denied",
        )),
        ..Probe::default()
    });
    let state = state(probe);
    for query in [
        feed_query(Some("bogus"), None, None),
        feed_query(None, None, Some("0")),
        feed_query(None, Some("bad"), None),
    ] {
        let response = read_feed(
            State(state.clone()),
            Path("acme".into()),
            Query(query),
            request("GET", "", false),
        )
        .await;
        assert_eq!(reply(response).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    }
    let response = create_post(
        State(state),
        Path("acme".into()),
        request("POST", r#"{"body":"x","audience":"branch"}"#, true),
    )
    .await;
    let (status, value) = reply(response).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(value["error"]["code"], "AUDIENCE_FORBIDDEN");
}
