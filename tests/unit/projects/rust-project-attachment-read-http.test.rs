//! Owner-local HTTP router tests for native Projects attachment reads.
use erpx_server_projects::public::project_attachment_read_http::{
    ProjectAttachmentContentHttpResponse, ProjectAttachmentReadContentHttpRequest,
    ProjectAttachmentReadHttpBackend, ProjectAttachmentReadHttpFuture,
    ProjectAttachmentReadHttpResponse, ProjectAttachmentReadHttpTarget,
    ProjectAttachmentReadLinksHttpRequest, ProjectAttachmentReadSetsHttpRequest,
    ProjectAttachmentReadStatusHttpRequest, project_attachment_read_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Probe {
    seen: Mutex<Vec<String>>,
    fail: Option<FailureKind>,
}

impl Probe {
    fn record(&self, value: String) -> Result<(), BusinessError> {
        self.seen.lock().unwrap().push(value);
        match self.fail {
            Some(kind) => Err(BusinessError::new(
                kind,
                "SYNTHETIC_READ_FAILURE",
                "Synthetic read failure.",
            )),
            None => Ok(()),
        }
    }
}

impl ProjectAttachmentReadHttpBackend for Probe {
    fn sets<'a>(
        &'a self,
        request: ProjectAttachmentReadSetsHttpRequest<'a>,
    ) -> ProjectAttachmentReadHttpFuture<'a, ProjectAttachmentReadHttpResponse> {
        Box::pin(async move {
            self.record(format!(
                "sets|{}|{}|{}",
                request.host, request.holding_slug, request.project_id
            ))?;
            Ok(ProjectAttachmentReadHttpResponse {
                data: serde_json::json!([]),
                context: serde_json::json!({"userId": "actor"}),
            })
        })
    }

    fn links<'a>(
        &'a self,
        request: ProjectAttachmentReadLinksHttpRequest<'a>,
    ) -> ProjectAttachmentReadHttpFuture<'a, ProjectAttachmentReadHttpResponse> {
        Box::pin(async move {
            self.record(format!(
                "links|{}|{}|{}|{}",
                request.host, request.holding_slug, request.project_id, request.set_id
            ))?;
            Ok(ProjectAttachmentReadHttpResponse {
                data: serde_json::json!([]),
                context: serde_json::json!({"userId": "actor"}),
            })
        })
    }

    fn status<'a>(
        &'a self,
        request: ProjectAttachmentReadStatusHttpRequest<'a>,
    ) -> ProjectAttachmentReadHttpFuture<'a, ProjectAttachmentReadHttpResponse> {
        Box::pin(async move {
            self.record(format!(
                "status|{}|{}|{}|{}",
                request.host, request.holding_slug, request.project_id, request.link_id
            ))?;
            Ok(ProjectAttachmentReadHttpResponse {
                data: serde_json::json!({"linkId": request.link_id}),
                context: serde_json::json!({"userId": "actor"}),
            })
        })
    }

    fn content<'a>(
        &'a self,
        request: ProjectAttachmentReadContentHttpRequest<'a>,
    ) -> ProjectAttachmentReadHttpFuture<'a, ProjectAttachmentContentHttpResponse> {
        Box::pin(async move {
            self.record(format!(
                "content|{}|{}|{}|{}|{}",
                request.host,
                request.holding_slug,
                request.project_id,
                request.link_id,
                request.variant
            ))?;
            Ok(ProjectAttachmentContentHttpResponse {
                media_type: "application/pdf".into(),
                filename: "plan .pdf".into(),
                inline: request.variant == "preview",
                body: b"hello".to_vec(),
            })
        })
    }
}

struct Noop;

impl ProjectAttachmentReadHttpBackend for Noop {
    fn sets<'a>(
        &'a self,
        _: ProjectAttachmentReadSetsHttpRequest<'a>,
    ) -> ProjectAttachmentReadHttpFuture<'a, ProjectAttachmentReadHttpResponse> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
    fn links<'a>(
        &'a self,
        _: ProjectAttachmentReadLinksHttpRequest<'a>,
    ) -> ProjectAttachmentReadHttpFuture<'a, ProjectAttachmentReadHttpResponse> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
    fn status<'a>(
        &'a self,
        _: ProjectAttachmentReadStatusHttpRequest<'a>,
    ) -> ProjectAttachmentReadHttpFuture<'a, ProjectAttachmentReadHttpResponse> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
    fn content<'a>(
        &'a self,
        _: ProjectAttachmentReadContentHttpRequest<'a>,
    ) -> ProjectAttachmentReadHttpFuture<'a, ProjectAttachmentContentHttpResponse> {
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
    let target = || ProjectAttachmentReadHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(project_attachment_read_router(vec![target()]).is_ok());
    assert!(project_attachment_read_router(vec![target(), target()]).is_err());
    assert!(
        project_attachment_read_router(vec![ProjectAttachmentReadHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        project_attachment_read_router(vec![ProjectAttachmentReadHttpTarget {
            holding_slug: "Acme".into(),
            ..target()
        }])
        .is_err()
    );
}

async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, String, Vec<u8>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream.write_all(raw.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let split = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap();
    let head = String::from_utf8_lossy(&bytes[..split]).to_string();
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
        .unwrap();
    (status, head, bytes[split + 4..].to_vec())
}

fn get(path: &str, host: &str) -> String {
    format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nCookie: erpx=session\r\nConnection: close\r\n\r\n"
    )
}

#[tokio::test]
async fn actual_router_forwards_exact_project_reads_with_private_headers() {
    let probe = Arc::new(Probe::default());
    let router = project_attachment_read_router(vec![ProjectAttachmentReadHttpTarget {
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

    let (status, head, body) = http(
        addr,
        get(
            "/api/v1/holdings/acme/projects/project-1/attachment-sets",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap()["context"]["userId"],
        "actor"
    );
    assert!(
        head.to_ascii_lowercase()
            .contains("cache-control: private, no-store")
    );
    assert!(
        head.to_ascii_lowercase()
            .contains("referrer-policy: no-referrer")
    );
    assert!(
        head.to_ascii_lowercase()
            .contains("x-content-type-options: nosniff")
    );

    let (status, _, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/projects/project-1/attachment-sets/set-1/links",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);

    let (status, _, body) = http(
        addr,
        get(
            "/api/v1/holdings/acme/projects/project-1/attachment-links/link-1/status",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap()["data"]["linkId"],
        "link-1"
    );

    let (status, head, body) = http(
        addr,
        get(
            "/api/v1/holdings/acme/projects/project-1/attachment-links/link-1/content?variant=preview",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body, b"hello");
    let lower = head.to_ascii_lowercase();
    assert!(lower.contains("content-type: application/pdf"));
    assert!(lower.contains("content-disposition: inline; filename*=utf-8''plan%20.pdf"));
    assert!(lower.contains("cache-control: private, no-store"));
    assert!(lower.contains("x-content-type-options: nosniff"));
    assert!(lower.contains("accept-ranges: none"));
    assert!(lower.contains("content-security-policy: sandbox; default-src 'none'"));

    assert_eq!(
        probe.seen.lock().unwrap().as_slice(),
        [
            "sets|acme.example.test|acme|project-1",
            "links|acme.example.test|acme|project-1|set-1",
            "status|acme.example.test|acme|project-1|link-1",
            "content|acme.example.test|acme|project-1|link-1|preview",
        ]
    );
    server.abort();
}

#[tokio::test]
async fn actual_router_rejects_query_bodies_and_maps_errors() {
    let probe = Arc::new(Probe::default());
    let router = project_attachment_read_router(vec![ProjectAttachmentReadHttpTarget {
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

    for path in [
        "/api/v1/holdings/acme/projects/project-1/attachment-sets?purposeKey=handoff",
        "/api/v1/holdings/acme/projects/project-1/attachment-sets/set-1/links?x=1",
        "/api/v1/holdings/acme/projects/project-1/attachment-links/link-1/status?x=1",
        "/api/v1/holdings/acme/projects/project-1/attachment-links/link-1/content?variant=original&variant=preview",
        "/api/v1/holdings/acme/projects/project-1/attachment-links/link-1/content?variant=other",
    ] {
        let (status, _, _) = http(addr, get(path, "acme.example.test")).await;
        assert_eq!(status, 422, "{path}");
    }

    let (status, _, _) = http(
        addr,
        get(
            "/api/v1/holdings/other/projects/project-1/attachment-sets",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 404);

    let fail = Arc::new(Probe {
        seen: Mutex::new(Vec::new()),
        fail: Some(FailureKind::Conflict),
    });
    let router = project_attachment_read_router(vec![ProjectAttachmentReadHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: fail,
    }])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server2 = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    let (status, _, body) = http(
        addr,
        get(
            "/api/v1/holdings/acme/projects/project-1/attachment-sets",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap()["error"]["code"],
        "SYNTHETIC_READ_FAILURE"
    );
    server.abort();
    server2.abort();
}
