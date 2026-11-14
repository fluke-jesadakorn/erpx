//! Native Work Projection Home unread HTTP ingress on registered Holding targets.
use erpx_server_runtime::public::http_support::{HeaderMap, Response, StatusCode, body::to_bytes};
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_work_projection::public::work_home_unread_http::{
    WorkHomeUnreadHttpBackend, WorkHomeUnreadHttpFuture, WorkHomeUnreadHttpRequest,
    WorkHomeUnreadHttpResponse, WorkHomeUnreadHttpState, WorkHomeUnreadHttpTarget,
    work_home_unread_router,
};
use serde_json::json;
use std::sync::{Arc, Mutex};

struct Fake {
    seen: Mutex<Vec<String>>,
}

impl WorkHomeUnreadHttpBackend for Fake {
    fn read<'a>(
        &'a self,
        request: WorkHomeUnreadHttpRequest<'a>,
    ) -> WorkHomeUnreadHttpFuture<'a, WorkHomeUnreadHttpResponse> {
        Box::pin(async move {
            self.seen
                .lock()
                .unwrap()
                .push(format!("read:{}:{}", request.host, request.holding_slug));
            Ok(WorkHomeUnreadHttpResponse {
                count: 7,
                context: json!({
                    "userId": "user",
                    "holdingId": "h001",
                    "companyId": "acme-co",
                    "assignmentId": "assignment",
                    "revision": 7
                }),
            })
        })
    }
}

fn state() -> (WorkHomeUnreadHttpState, Arc<Fake>) {
    let fake = Arc::new(Fake {
        seen: Mutex::new(Vec::new()),
    });
    let state = WorkHomeUnreadHttpState::new(vec![WorkHomeUnreadHttpTarget {
        host: "holding.erpxbrowser.test:3102".into(),
        holding_slug: "acme".into(),
        origin: "http://127.0.0.1:3102".into(),
        backend: fake.clone(),
    }])
    .unwrap();
    (state, fake)
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Runtime::new().unwrap().block_on(future)
}

fn headers(origin: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("host", "holding.erpxbrowser.test:3102".parse().unwrap());
    if let Some(origin) = origin {
        headers.insert("origin", origin.parse().unwrap());
    }
    headers
}

async fn body(response: Response) -> serde_json::Value {
    let bytes = to_bytes(response.into_body(), 16 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn reads_the_exact_target_and_returns_only_the_count_and_context() {
    let (state, fake) = state();
    let response = block_on(state.read(&headers(Some("http://127.0.0.1:3102")), "acme", None));
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        fake.seen.lock().unwrap().as_slice(),
        ["read:holding.erpxbrowser.test:3102:acme"]
    );
    let value = block_on(body(response));
    assert_eq!(value["data"]["count"], 7);
    assert_eq!(value["context"]["companyId"], "acme-co");
}

#[test]
fn rejects_query_wrong_origin_and_unknown_holding() {
    let (state, _) = state();
    assert_eq!(
        block_on(state.read(
            &headers(Some("http://127.0.0.1:3102")),
            "acme",
            Some("since=1")
        ))
        .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        block_on(state.read(&headers(Some("https://evil.test")), "acme", None)).status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        block_on(state.read(&headers(Some("http://127.0.0.1:3102")), "other", None)).status(),
        StatusCode::NOT_FOUND
    );
}

#[test]
fn maps_the_collaboration_unavailable_error() {
    struct Unavailable;
    impl WorkHomeUnreadHttpBackend for Unavailable {
        fn read<'a>(
            &'a self,
            _: WorkHomeUnreadHttpRequest<'a>,
        ) -> WorkHomeUnreadHttpFuture<'a, WorkHomeUnreadHttpResponse> {
            Box::pin(async {
                Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "COLLABORATION_HOME_UNREAD_UNAVAILABLE",
                    "The current Collaboration Home unread count is unavailable.",
                ))
            })
        }
    }
    let state = WorkHomeUnreadHttpState::new(vec![WorkHomeUnreadHttpTarget {
        host: "holding.erpxbrowser.test:3102".into(),
        holding_slug: "acme".into(),
        origin: "http://127.0.0.1:3102".into(),
        backend: Arc::new(Unavailable),
    }])
    .unwrap();
    let response = block_on(state.read(&headers(Some("http://127.0.0.1:3102")), "acme", None));
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let value = block_on(body(response));
    assert_eq!(
        value["error"]["code"],
        "COLLABORATION_HOME_UNREAD_UNAVAILABLE"
    );
}

#[test]
fn router_builds_and_rejects_duplicate_targets() {
    let (_, fake) = state();
    let target = || WorkHomeUnreadHttpTarget {
        host: "holding.erpxbrowser.test:3102".into(),
        holding_slug: "acme".into(),
        origin: "http://127.0.0.1:3102".into(),
        backend: fake.clone(),
    };
    assert!(work_home_unread_router(vec![target()]).is_ok());
    assert!(work_home_unread_router(vec![target(), target()]).is_err());
}
