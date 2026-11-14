//! Actual owner-local HTTP router tests for native IAM roles/reviews.
use erpx_server_identity_access::public::iam_roles_http::{
    IamRolesReviewsHttpBackend, IamRolesReviewsHttpCommand, IamRolesReviewsHttpFuture,
    IamRolesReviewsHttpRequest, IamRolesReviewsHttpResponse, IamRolesReviewsHttpTarget,
    iam_roles_reviews_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: bool,
}

impl IamRolesReviewsHttpBackend for Probe {
    fn execute<'a>(
        &'a self,
        request: IamRolesReviewsHttpRequest<'a>,
    ) -> IamRolesReviewsHttpFuture<'a, IamRolesReviewsHttpResponse> {
        Box::pin(async move {
            let cookie = request.headers.get("cookie").and_then(|v| v.to_str().ok());
            let seen = match &request.command {
                IamRolesReviewsHttpCommand::SaveRole {
                    input,
                    idempotency_key,
                } => format!(
                    "save|{}|{}|{}|{}|{}|{cookie:?}",
                    request.host,
                    request.holding_slug,
                    input.name,
                    input.expected_version,
                    idempotency_key
                ),
                IamRolesReviewsHttpCommand::DecideReview {
                    review_id,
                    input,
                    idempotency_key,
                } => format!(
                    "review|{}|{}|{}|{}|{}|{cookie:?}",
                    request.host, request.holding_slug, review_id, input.decision, idempotency_key
                ),
                IamRolesReviewsHttpCommand::StartAccessReview {
                    input,
                    idempotency_key,
                } => format!(
                    "start|{}|{}|{}|{}|{cookie:?}",
                    request.host, request.holding_slug, input.name, idempotency_key
                ),
                IamRolesReviewsHttpCommand::DecideAccessReviewItem {
                    item_id,
                    input,
                    idempotency_key,
                } => format!(
                    "item|{}|{}|{}|{}|{}|{cookie:?}",
                    request.host, request.holding_slug, item_id, input.decision, idempotency_key
                ),
                IamRolesReviewsHttpCommand::CancelAccessReview {
                    campaign_id,
                    input,
                    idempotency_key,
                } => format!(
                    "cancel|{}|{}|{}|{}|{}|{cookie:?}",
                    request.host,
                    request.holding_slug,
                    campaign_id,
                    input.expected_version,
                    idempotency_key
                ),
            };
            self.seen.lock().unwrap().push(seen);
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "ROLE_CHANGED",
                    "Synthetic changed role.",
                ));
            }
            let data = match &request.command {
                IamRolesReviewsHttpCommand::SaveRole { .. } => serde_json::json!({
                    "id": "iam-review-iam-role-key-1",
                    "status": "pending",
                    "roleId": "custom_role",
                    "revision": 2
                }),
                IamRolesReviewsHttpCommand::DecideReview { review_id, .. } => {
                    serde_json::json!({"id": review_id, "status": "approved"})
                }
                IamRolesReviewsHttpCommand::StartAccessReview { .. } => serde_json::json!({
                    "id": "ar-campaign-iam-role-key-1",
                    "status": "open",
                    "version": 1,
                    "itemCount": 1
                }),
                IamRolesReviewsHttpCommand::DecideAccessReviewItem { item_id, .. } => {
                    serde_json::json!({
                        "id": item_id,
                        "campaignId": "ar-campaign-iam-role-key-1",
                        "decision": "keep",
                        "campaignStatus": "completed",
                        "campaignVersion": 3
                    })
                }
                IamRolesReviewsHttpCommand::CancelAccessReview { campaign_id, .. } => {
                    serde_json::json!({
                        "id": campaign_id,
                        "status": "cancelled",
                        "version": 3
                    })
                }
            };
            Ok(IamRolesReviewsHttpResponse {
                data,
                context: serde_json::json!({"userId": "actor"}),
            })
        })
    }
}

struct Noop;

impl IamRolesReviewsHttpBackend for Noop {
    fn execute<'a>(
        &'a self,
        _: IamRolesReviewsHttpRequest<'a>,
    ) -> IamRolesReviewsHttpFuture<'a, IamRolesReviewsHttpResponse> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(Noop);
    let target = || IamRolesReviewsHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(iam_roles_reviews_http_router(vec![target()]).is_ok());
    assert!(iam_roles_reviews_http_router(vec![target(), target()]).is_err());
    assert!(
        iam_roles_reviews_http_router(vec![IamRolesReviewsHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        iam_roles_reviews_http_router(vec![IamRolesReviewsHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        iam_roles_reviews_http_router(vec![IamRolesReviewsHttpTarget {
            holding_slug: "Acme".into(),
            ..target()
        }])
        .is_err()
    );
}

async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, serde_json::Value, String) {
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

fn post(path: &str, host: &str, key: Option<&str>, body: &str) -> String {
    let key = key
        .map(|value| format!("Idempotency-Key: {value}\r\n"))
        .unwrap_or_default();
    format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nCookie: erpx=session\r\nContent-Type: application/json\r\n{key}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn target(
    host: &str,
    slug: &str,
    seen: Arc<Mutex<Vec<String>>>,
    fail: bool,
) -> IamRolesReviewsHttpTarget {
    IamRolesReviewsHttpTarget {
        host: host.into(),
        holding_slug: slug.into(),
        origin: format!("https://{host}"),
        backend: Arc::new(Probe { seen, fail }),
    }
}

const ROLE: &str = r#"{"name":"Finance reader","expectedVersion":0,"permissions":["iam.read"],"grantablePermissions":["iam.read"],"scopes":["holding"],"reason":"Independent role revision"}"#;
const REVIEW: &str =
    r#"{"expectedVersion":1,"decision":"approve","reason":"Independent decision"}"#;
const START: &str =
    r#"{"name":"Quarterly certification","dueAt":86400500,"reason":"Quarterly review"}"#;
const ITEM: &str = r#"{"expectedCampaignVersion":2,"decision":"keep","reason":"Certify access"}"#;
const CANCEL: &str = r#"{"expectedVersion":2,"reason":"Superseded campaign"}"#;

#[tokio::test]
async fn actual_router_forwards_all_identity_commands_with_private_headers() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = iam_roles_reviews_http_router(vec![
        target("acme.example.test", "acme", seen.clone(), false),
        target("fail.example.test", "fail", seen.clone(), true),
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    for (path, key, body, expected) in [
        (
            "/api/v1/holdings/acme/iam/roles",
            "iam-role-key-1",
            ROLE,
            serde_json::json!({"id":"iam-review-iam-role-key-1","status":"pending","roleId":"custom_role","revision":2}),
        ),
        (
            "/api/v1/holdings/acme/iam/reviews/iam-review-iam-role-key-1/decide",
            "iam-review-key-1",
            REVIEW,
            serde_json::json!({"id":"iam-review-iam-role-key-1","status":"approved"}),
        ),
        (
            "/api/v1/holdings/acme/iam/access-reviews",
            "iam-campaign-key-1",
            START,
            serde_json::json!({"id":"ar-campaign-iam-role-key-1","status":"open","version":1,"itemCount":1}),
        ),
        (
            "/api/v1/holdings/acme/iam/access-review-items/ar-item-iam-role-key-1-0/decide",
            "iam-item-key-1",
            ITEM,
            serde_json::json!({"id":"ar-item-iam-role-key-1-0","campaignId":"ar-campaign-iam-role-key-1","decision":"keep","campaignStatus":"completed","campaignVersion":3}),
        ),
        (
            "/api/v1/holdings/acme/iam/access-reviews/ar-campaign-iam-role-key-1/cancel",
            "iam-cancel-key-1",
            CANCEL,
            serde_json::json!({"id":"ar-campaign-iam-role-key-1","status":"cancelled","version":3}),
        ),
    ] {
        let (status, value, headers) =
            http(addr, post(path, "acme.example.test", Some(key), body)).await;
        assert_eq!(status, 200, "{path}");
        assert_eq!(value["data"], expected, "{path}");
        assert_eq!(value["context"]["userId"], "actor");
        assert!(headers.contains("cache-control: private, no-store"));
        assert!(headers.contains("referrer-policy: no-referrer"));
        assert!(headers.contains("x-content-type-options: nosniff"));
    }
    {
        let seen = seen.lock().unwrap();
        assert_eq!(
            seen[0],
            "save|acme.example.test|acme|Finance reader|0|iam-role-key-1|Some(\"erpx=session\")"
        );
        assert_eq!(
            seen[1],
            "review|acme.example.test|acme|iam-review-iam-role-key-1|approve|iam-review-key-1|Some(\"erpx=session\")"
        );
        assert_eq!(
            seen[2],
            "start|acme.example.test|acme|Quarterly certification|iam-campaign-key-1|Some(\"erpx=session\")"
        );
        assert_eq!(
            seen[3],
            "item|acme.example.test|acme|ar-item-iam-role-key-1-0|keep|iam-item-key-1|Some(\"erpx=session\")"
        );
        assert_eq!(
            seen[4],
            "cancel|acme.example.test|acme|ar-campaign-iam-role-key-1|2|iam-cancel-key-1|Some(\"erpx=session\")"
        );
    }

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/fail/iam/roles",
            "fail.example.test",
            Some("iam-role-key-2"),
            ROLE,
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "ROLE_CHANGED");
}

#[tokio::test]
async fn router_rejects_unknown_scope_bad_keys_bodies_and_queries() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = iam_roles_reviews_http_router(vec![target(
        "acme.example.test",
        "acme",
        seen.clone(),
        false,
    )])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/other/iam/roles",
            "acme.example.test",
            Some("iam-role-key-1"),
            ROLE,
        ),
    )
    .await;
    assert_eq!(status, 404);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/iam/roles?scope=all",
            "acme.example.test",
            Some("iam-role-key-1"),
            ROLE,
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/iam/roles",
            "acme.example.test",
            None,
            ROLE,
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/iam/roles",
            "acme.example.test",
            Some("short"),
            ROLE,
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, body, _) = http(
        addr,
        "POST /api/v1/holdings/acme/iam/roles HTTP/1.1\r\nHost: acme.example.test\r\nContent-Type: text/plain\r\nIdempotency-Key: iam-role-key-3\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_string(),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "IAM_ROLES_REVIEWS_REQUEST_INVALID");

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/iam/roles",
            "acme.example.test",
            Some("iam-role-key-4"),
            r#"{"name":"Finance reader","expectedVersion":0,"permissions":["iam.read"],"scopes":["holding"],"reason":"Independent role revision","extra":true}"#,
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/iam/reviews/bad/id/decide",
            "acme.example.test",
            Some("iam-review-key-2"),
            REVIEW,
        ),
    )
    .await;
    assert_eq!(status, 404);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/iam/access-review-items/bad/id/decide",
            "acme.example.test",
            Some("iam-item-key-2"),
            ITEM,
        ),
    )
    .await;
    assert_eq!(status, 404);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/iam/access-reviews/bad/id/cancel",
            "acme.example.test",
            Some("iam-cancel-key-2"),
            CANCEL,
        ),
    )
    .await;
    assert_eq!(status, 404);

    assert!(seen.lock().unwrap().is_empty());
}
