//! Owner-local HTTP router tests for the native Projects attachment bind.
use erpx_server_projects::public::project_attachment_http::{
    ProjectAttachmentBindHttpBackend, ProjectAttachmentBindHttpRequest,
    ProjectAttachmentHttpFuture, ProjectAttachmentHttpResponse, ProjectAttachmentHttpTarget,
    project_attachment_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl ProjectAttachmentBindHttpBackend for Noop {
    fn bind<'a>(
        &'a self,
        _: ProjectAttachmentBindHttpRequest<'a>,
    ) -> ProjectAttachmentHttpFuture<'a, ProjectAttachmentHttpResponse> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
}

#[derive(Default)]
struct Probe {
    seen: Mutex<Vec<String>>,
}

impl ProjectAttachmentBindHttpBackend for Probe {
    fn bind<'a>(
        &'a self,
        request: ProjectAttachmentBindHttpRequest<'a>,
    ) -> ProjectAttachmentHttpFuture<'a, ProjectAttachmentHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(format!(
                "{}:{}:{}:{}:{}",
                request.host,
                request.holding_slug,
                request.set_id,
                request.expected_version,
                request.key
            ));
            Ok(ProjectAttachmentHttpResponse {
                data: serde_json::json!({
                    "id": "attachment-1",
                    "version": 5,
                    "status": "bound",
                    "ownerVersion": 5,
                    "attachmentSetId": request.set_id,
                    "attachmentLinkId": "link-1",
                    "setId": request.set_id,
                    "manifest": [],
                }),
                context: serde_json::json!({"userId": "user"}),
            })
        })
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(Noop);
    let target = || ProjectAttachmentHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(project_attachment_router(vec![target()]).is_ok());
    assert!(project_attachment_router(vec![target(), target()]).is_err());
    assert!(
        project_attachment_router(vec![ProjectAttachmentHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
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

fn post(path: &str, origin: &str, key: &str, body: &str) -> String {
    format!(
        "POST {path} HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: {origin}\r\n\
         Idempotency-Key: {key}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[tokio::test]
async fn actual_router_retains_target_set_version_and_key() {
    let probe = Arc::new(Probe::default());
    let router = project_attachment_router(vec![ProjectAttachmentHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: probe.clone(),
    }])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let path = "/api/v1/holdings/acme/project-attachments/set-1/bind";
    let (status, body) = http(
        addr,
        post(
            path,
            "https://acme.example.test",
            "bind-key-0001",
            r#"{"expectedTargetVersion":4}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["attachmentSetId"], "set-1");
    assert_eq!(body["data"]["attachmentLinkId"], "link-1");
    assert_eq!(body["data"]["ownerVersion"], 5);
    assert_eq!(body["context"]["userId"], "user");
    assert_eq!(
        probe.seen.lock().unwrap().as_slice(),
        ["acme.example.test:acme:set-1:4:bind-key-0001"]
    );

    for (raw, expected) in [
        (
            post(
                path,
                "https://other.example.test",
                "bind-key-0001",
                r#"{"expectedTargetVersion":4}"#,
            ),
            403,
        ),
        (
            format!(
                "POST {path} HTTP/1.1\r\nHost: acme.example.test\r\n\
                 Origin: https://acme.example.test\r\n\
                 Content-Type: application/json\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{}",
                r#"{"expectedTargetVersion":4}"#.len(),
                r#"{"expectedTargetVersion":4}"#
            ),
            422,
        ),
        (
            post(
                "/api/v1/holdings/acme/project-attachments/set-1/bind?set=2",
                "https://acme.example.test",
                "bind-key-0001",
                r#"{"expectedTargetVersion":4}"#,
            ),
            422,
        ),
        (
            post(
                path,
                "https://acme.example.test",
                "bind-key-0001",
                r#"{"expectedTargetVersion":4,"linkIds":["link-1"]}"#,
            ),
            422,
        ),
        (
            post(
                path,
                "https://acme.example.test",
                "short",
                r#"{"expectedTargetVersion":4}"#,
            ),
            422,
        ),
    ] {
        let (status, _) = http(addr, raw).await;
        assert_eq!(status, expected);
    }

    server.abort();
}
