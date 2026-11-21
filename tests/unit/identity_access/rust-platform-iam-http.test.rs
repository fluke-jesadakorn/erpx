//! Actual owner-local HTTP router tests for the Platform IAM namespace.
//!
//! These prove Host-only target resolution, exact Origin, strict command
//! headers/bodies and Platform error mapping. They do not prove the Account
//! session or Platform assignment resolution owned by composition.
use erpx_server_identity_access::public::iam_roles_http::{
    IamPlatformRolesReviewsHttpRequest, IamPlatformRolesReviewsHttpTarget,
    IamRolesReviewsHttpBackend, IamRolesReviewsHttpCommand, IamRolesReviewsHttpFuture,
    IamRolesReviewsHttpRequest, IamRolesReviewsHttpResponse,
    iam_platform_roles_reviews_http_router,
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

    fn execute_platform<'a>(
        &'a self,
        request: IamPlatformRolesReviewsHttpRequest<'a>,
    ) -> IamRolesReviewsHttpFuture<'a, IamRolesReviewsHttpResponse> {
        Box::pin(async move {
            let cookie = request.headers.get("cookie").and_then(|v| v.to_str().ok());
            let seen = match &request.command {
                IamRolesReviewsHttpCommand::SaveRole {
                    input,
                    idempotency_key,
                } => format!(
                    "save|{}|{}|{}|{}|{cookie:?}",
                    request.host, input.name, input.expected_version, idempotency_key
                ),
                IamRolesReviewsHttpCommand::DecideReview {
                    review_id,
                    input,
                    idempotency_key,
                } => format!(
                    "review|{}|{}|{}|{}|{cookie:?}",
                    request.host, review_id, input.decision, idempotency_key
                ),
                IamRolesReviewsHttpCommand::StartAccessReview {
                    input,
                    idempotency_key,
                } => format!(
                    "start|{}|{}|{}|{cookie:?}",
                    request.host, input.name, idempotency_key
                ),
                IamRolesReviewsHttpCommand::DecideAccessReviewItem {
                    item_id,
                    input,
                    idempotency_key,
                } => format!(
                    "item|{}|{}|{}|{}|{cookie:?}",
                    request.host, item_id, input.decision, idempotency_key
                ),
                IamRolesReviewsHttpCommand::CancelAccessReview {
                    campaign_id,
                    input,
                    idempotency_key,
                } => format!(
                    "cancel|{}|{}|{}|{}|{cookie:?}",
                    request.host, campaign_id, input.expected_version, idempotency_key
                ),
            };
            self.seen.lock().unwrap().push(seen);
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "ROLE_CHANGED",
                    "Synthetic changed Platform role.",
                ));
            }
            let data = match &request.command {
                IamRolesReviewsHttpCommand::SaveRole { .. } => serde_json::json!({
                    "id": "platform-review-iam-role-key-1",
                    "status": "pending",
                    "roleId": "platform_custom_role",
                    "revision": 1
                }),
                IamRolesReviewsHttpCommand::DecideReview { review_id, .. } => {
                    serde_json::json!({"id": review_id, "status": "approved"})
                }
                IamRolesReviewsHttpCommand::StartAccessReview { .. } => serde_json::json!({
                    "id": "ar-campaign-platform-key-1",
                    "status": "open",
                    "version": 1,
                    "itemCount": 0
                }),
                IamRolesReviewsHttpCommand::DecideAccessReviewItem { item_id, .. } => {
                    serde_json::json!({
                        "id": item_id,
                        "campaignId": "ar-campaign-platform-key-1",
                        "decision": "keep",
                        "campaignStatus": "completed",
                        "campaignVersion": 2
                    })
                }
                IamRolesReviewsHttpCommand::CancelAccessReview { campaign_id, .. } => {
                    serde_json::json!({
                        "id": campaign_id,
                        "status": "cancelled",
                        "version": 2
                    })
                }
            };
            Ok(IamRolesReviewsHttpResponse {
                data,
                context: serde_json::json!({"userId": "platform-actor"}),
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

const HOST: &str = "erpx-internal.example.test";
const ORIGIN: &str = "https://erpx-internal.example.test";

fn target(
    host: &str,
    seen: Arc<Mutex<Vec<String>>>,
    fail: bool,
) -> IamPlatformRolesReviewsHttpTarget {
    IamPlatformRolesReviewsHttpTarget {
        host: host.into(),
        origin: format!("https://{host}"),
        backend: Arc::new(Probe { seen, fail }),
    }
}

#[test]
fn platform_target_configuration_requires_distinct_exact_hosts_and_origins() {
    let backend = Arc::new(Noop);
    let target = || IamPlatformRolesReviewsHttpTarget {
        host: HOST.into(),
        origin: ORIGIN.into(),
        backend: backend.clone(),
    };
    assert!(iam_platform_roles_reviews_http_router(vec![target()]).is_ok());
    assert!(iam_platform_roles_reviews_http_router(vec![target(), target()]).is_err());
    assert!(
        iam_platform_roles_reviews_http_router(vec![IamPlatformRolesReviewsHttpTarget {
            origin: "http://erpx-internal.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        iam_platform_roles_reviews_http_router(vec![IamPlatformRolesReviewsHttpTarget {
            origin: "https://other.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        iam_platform_roles_reviews_http_router(vec![IamPlatformRolesReviewsHttpTarget {
            host: String::new(),
            origin: String::new(),
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

fn post(path: &str, host: &str, origin: Option<&str>, key: Option<&str>, body: &str) -> String {
    let origin = origin
        .map(|value| format!("Origin: {value}\r\n"))
        .unwrap_or_default();
    let key = key
        .map(|value| format!("Idempotency-Key: {value}\r\n"))
        .unwrap_or_default();
    format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\n{origin}Cookie: erpx=session\r\nContent-Type: application/json\r\n{key}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

const ROLE: &str = r#"{"name":"Platform finance reader","expectedVersion":0,"permissions":["platform.iam.read"],"grantablePermissions":["platform.iam.read"],"scopes":["platform"],"reason":"Independent Platform role revision"}"#;
const REVIEW: &str =
    r#"{"expectedVersion":1,"decision":"approve","reason":"Independent Platform decision"}"#;
const START: &str =
    r#"{"name":"Platform quarterly certification","dueAt":86400500,"reason":"Quarterly review"}"#;
const ITEM: &str =
    r#"{"expectedCampaignVersion":2,"decision":"keep","reason":"Certify Platform access"}"#;
const CANCEL: &str = r#"{"expectedVersion":2,"reason":"Superseded Platform campaign"}"#;

#[tokio::test]
async fn platform_router_forwards_all_commands_with_exact_origin_and_private_headers() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = iam_platform_roles_reviews_http_router(vec![
        target(HOST, seen.clone(), false),
        target("fail.example.test", seen.clone(), true),
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    for (path, key, body, expected) in [
        (
            "/api/v1/platform/iam/roles",
            "platform-role-key-1",
            ROLE,
            serde_json::json!({"id":"platform-review-iam-role-key-1","status":"pending","roleId":"platform_custom_role","revision":1}),
        ),
        (
            "/api/v1/platform/iam/reviews/platform-review-iam-role-key-1/decide",
            "platform-review-key-1",
            REVIEW,
            serde_json::json!({"id":"platform-review-iam-role-key-1","status":"approved"}),
        ),
        (
            "/api/v1/platform/iam/access-reviews",
            "platform-campaign-key-1",
            START,
            serde_json::json!({"id":"ar-campaign-platform-key-1","status":"open","version":1,"itemCount":0}),
        ),
        (
            "/api/v1/platform/iam/access-review-items/ar-item-platform-key-1-0/decide",
            "platform-item-key-1",
            ITEM,
            serde_json::json!({"id":"ar-item-platform-key-1-0","campaignId":"ar-campaign-platform-key-1","decision":"keep","campaignStatus":"completed","campaignVersion":2}),
        ),
        (
            "/api/v1/platform/iam/access-reviews/ar-campaign-platform-key-1/cancel",
            "platform-cancel-key-1",
            CANCEL,
            serde_json::json!({"id":"ar-campaign-platform-key-1","status":"cancelled","version":2}),
        ),
    ] {
        let (status, value, headers) =
            http(addr, post(path, HOST, Some(ORIGIN), Some(key), body)).await;
        assert_eq!(status, 200, "{path}");
        assert_eq!(value["data"], expected, "{path}");
        assert_eq!(value["context"]["userId"], "platform-actor");
        assert!(headers.contains("cache-control: private, no-store"));
        assert!(headers.contains("referrer-policy: no-referrer"));
        assert!(headers.contains("x-content-type-options: nosniff"));
    }
    {
        let seen = seen.lock().unwrap();
        assert_eq!(
            seen[0],
            "save|erpx-internal.example.test|Platform finance reader|0|platform-role-key-1|Some(\"erpx=session\")"
        );
        assert_eq!(
            seen[1],
            "review|erpx-internal.example.test|platform-review-iam-role-key-1|approve|platform-review-key-1|Some(\"erpx=session\")"
        );
        assert_eq!(
            seen[2],
            "start|erpx-internal.example.test|Platform quarterly certification|platform-campaign-key-1|Some(\"erpx=session\")"
        );
        assert_eq!(
            seen[3],
            "item|erpx-internal.example.test|ar-item-platform-key-1-0|keep|platform-item-key-1|Some(\"erpx=session\")"
        );
        assert_eq!(
            seen[4],
            "cancel|erpx-internal.example.test|ar-campaign-platform-key-1|2|platform-cancel-key-1|Some(\"erpx=session\")"
        );
    }

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/platform/iam/roles",
            "fail.example.test",
            Some("https://fail.example.test"),
            Some("platform-role-key-2"),
            ROLE,
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "ROLE_CHANGED");
}

#[tokio::test]
async fn platform_router_rejects_unknown_hosts_origins_keys_bodies_and_queries() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router =
        iam_platform_roles_reviews_http_router(vec![target(HOST, seen.clone(), false)]).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/platform/iam/roles",
            "unknown.example.test",
            Some("https://unknown.example.test"),
            Some("platform-role-key-1"),
            ROLE,
        ),
    )
    .await;
    assert_eq!(status, 404);

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/platform/iam/roles",
            HOST,
            None,
            Some("platform-role-key-1"),
            ROLE,
        ),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(body["error"]["code"], "ORIGIN_FORBIDDEN");

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/platform/iam/roles",
            HOST,
            Some("https://other.example.test"),
            Some("platform-role-key-1"),
            ROLE,
        ),
    )
    .await;
    assert_eq!(status, 403);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/platform/iam/roles?scope=all",
            HOST,
            Some(ORIGIN),
            Some("platform-role-key-1"),
            ROLE,
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, _, _) = http(
        addr,
        post("/api/v1/platform/iam/roles", HOST, Some(ORIGIN), None, ROLE),
    )
    .await;
    assert_eq!(status, 422);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/platform/iam/roles",
            HOST,
            Some(ORIGIN),
            Some("short"),
            ROLE,
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, body, _) = http(
        addr,
        format!(
            "POST /api/v1/platform/iam/roles HTTP/1.1\r\nHost: {HOST}\r\nOrigin: {ORIGIN}\r\nContent-Type: text/plain\r\nIdempotency-Key: platform-role-key-3\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "IAM_ROLES_REVIEWS_REQUEST_INVALID");

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/platform/iam/roles",
            HOST,
            Some(ORIGIN),
            Some("platform-role-key-4"),
            r#"{"name":"Platform finance reader","expectedVersion":0,"permissions":["platform.iam.read"],"scopes":["platform"],"reason":"Independent Platform role revision","extra":true}"#,
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/platform/iam/reviews/bad/id/decide",
            HOST,
            Some(ORIGIN),
            Some("platform-review-key-2"),
            REVIEW,
        ),
    )
    .await;
    assert_eq!(status, 404);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/platform/iam/access-review-items/bad/id/decide",
            HOST,
            Some(ORIGIN),
            Some("platform-item-key-2"),
            ITEM,
        ),
    )
    .await;
    assert_eq!(status, 404);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/platform/iam/access-reviews/bad/id/cancel",
            HOST,
            Some(ORIGIN),
            Some("platform-cancel-key-2"),
            CANCEL,
        ),
    )
    .await;
    assert_eq!(status, 404);
}
