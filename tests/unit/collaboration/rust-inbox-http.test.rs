//! Owner-local HTTP adapter tests for the native Inbox read ingress.
use erpx_server_collaboration::public::inbox_http::{
    InboxCategoryCounts, InboxHttpBackend, InboxHttpFuture, InboxHttpReadReply,
    InboxHttpReadRequest, InboxHttpState, InboxHttpTarget, InboxItemData, InboxPageData,
    InboxQuery, InboxReadKind, InboxSummaryData, read_notice, read_notifications, read_summary,
};
use erpx_server_runtime::public::http_support::{
    Path, Query, Request, Response, State, StatusCode,
    body::{Body, to_bytes},
    header,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Probe {
    reads: Mutex<Vec<String>>,
}

impl InboxHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: InboxHttpReadRequest<'a>,
    ) -> InboxHttpFuture<'a, InboxHttpReadReply> {
        let kind = match &request.kind {
            InboxReadKind::Summary => "summary",
            InboxReadKind::Notifications { .. } => "notifications",
            InboxReadKind::People { .. } => "people",
            InboxReadKind::Notices => "notices",
            InboxReadKind::Notice { .. } => "notice",
        };
        self.reads
            .lock()
            .unwrap()
            .push(format!("{kind}|{}|{}", request.host, request.holding_slug));
        Box::pin(async move {
            Ok(match request.kind {
                InboxReadKind::Summary => InboxHttpReadReply::Summary(Box::new(InboxSummaryData {
                    unread: 2,
                    categories: InboxCategoryCounts {
                        mention: 1,
                        notice: 1,
                        ..InboxCategoryCounts::default()
                    },
                })),
                InboxReadKind::Notifications { .. } => {
                    InboxHttpReadReply::Page(Box::new(InboxPageData {
                        items: vec![InboxItemData {
                            id: "delivery-1".into(),
                            event_id: "event-1".into(),
                            category: "mention".into(),
                            message: "You were mentioned".into(),
                            href: Some("/feed".into()),
                            created_at: 5,
                            read_at: None,
                            archived_at: None,
                            source_type: "social_comment".into(),
                            source_id: "comment-1".into(),
                            source: None,
                        }],
                        next_cursor: Some("cursor-1".into()),
                        has_more: true,
                    }))
                }
                InboxReadKind::People { .. } => {
                    InboxHttpReadReply::People(json!([{"id":"user-1","name":"User"}]))
                }
                InboxReadKind::Notices => InboxHttpReadReply::Notices(json!([])),
                InboxReadKind::Notice { id } => {
                    InboxHttpReadReply::Notice(json!({"id":id,"acceptedCount":1}))
                }
            })
        })
    }
}

fn target(probe: Arc<Probe>) -> InboxHttpTarget {
    InboxHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: probe,
    }
}

fn state(probe: Arc<Probe>) -> Arc<InboxHttpState> {
    Arc::new(InboxHttpState::new(vec![target(probe)]).unwrap())
}

fn request() -> Request {
    Request::builder()
        .method("GET")
        .header("host", "acme.example.test")
        .body(Body::empty())
        .unwrap()
}

fn query() -> InboxQuery {
    InboxQuery {
        view: None,
        category: None,
        search: None,
        cursor: None,
        limit: None,
        q: None,
    }
}

async fn reply(response: Response) -> (StatusCode, Value) {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn summary_notifications_and_notices_read_the_exact_holding() {
    let probe = Arc::new(Probe::default());
    let state = state(probe.clone());

    let response = read_summary(State(state.clone()), Path("acme".into()), request()).await;
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "private, no-store"
    );
    let (status, value) = reply(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["data"]["unread"], 2);
    assert_eq!(value["data"]["categories"]["mention"], 1);

    let response = read_notifications(
        State(state.clone()),
        Path("acme".into()),
        Query(InboxQuery {
            category: Some("mention".into()),
            search: Some("mentioned".into()),
            ..query()
        }),
        request(),
    )
    .await;
    let (status, value) = reply(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["data"]["items"][0]["sourceType"], "social_comment");
    assert_eq!(value["data"]["nextCursor"], "cursor-1");

    let response = read_notice(
        State(state),
        Path(("acme".into(), "notice-1".into())),
        request(),
    )
    .await;
    let (status, value) = reply(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["data"]["id"], "notice-1");
    assert_eq!(
        probe.reads.lock().unwrap().as_slice(),
        [
            "summary|acme.example.test|acme",
            "notifications|acme.example.test|acme",
            "notice|acme.example.test|acme"
        ]
    );
}

#[tokio::test]
async fn invalid_inbox_filters_and_unknown_hosts_are_rejected() {
    let probe = Arc::new(Probe::default());
    let state = state(probe.clone());
    for query in [
        InboxQuery {
            category: Some("bogus".into()),
            ..query()
        },
        InboxQuery {
            limit: Some("0".into()),
            ..query()
        },
        InboxQuery {
            limit: Some("101".into()),
            ..query()
        },
        InboxQuery {
            cursor: Some("x".repeat(513)),
            ..query()
        },
    ] {
        let response = read_notifications(
            State(state.clone()),
            Path("acme".into()),
            Query(query),
            request(),
        )
        .await;
        assert_eq!(reply(response).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    }
    let mut wrong = request();
    wrong
        .headers_mut()
        .insert("host", "other.example.test".parse().unwrap());
    let response = read_summary(State(state), Path("acme".into()), wrong).await;
    assert_eq!(reply(response).await.0, StatusCode::NOT_FOUND);
    assert!(probe.reads.lock().unwrap().is_empty());
}
