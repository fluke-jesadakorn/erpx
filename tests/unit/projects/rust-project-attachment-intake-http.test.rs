//! Owner-local HTTP router tests for native Projects attachment intake.
use erpx_server_projects::public::project_attachment_intake_http::{
    ProjectAttachmentCompleteHttpRequest, ProjectAttachmentIntakeHttpBackend,
    ProjectAttachmentIntakeHttpFuture, ProjectAttachmentIntakeHttpResponse,
    ProjectAttachmentIntakeHttpTarget, ProjectAttachmentReserveHttpRequest,
    ProjectAttachmentSetCreateHttpRequest, ProjectAttachmentUploadStatusHttpRequest,
    project_attachment_intake_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Probe {
    seen: Mutex<Vec<String>>,
    fail: Mutex<Option<BusinessError>>,
}

impl Probe {
    fn failure(&self) -> Result<(), BusinessError> {
        match self.fail.lock().unwrap().clone() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

fn context() -> serde_json::Value {
    serde_json::json!({"userId": "user", "companyId": "company"})
}

impl ProjectAttachmentIntakeHttpBackend for Probe {
    fn create<'a>(
        &'a self,
        request: ProjectAttachmentSetCreateHttpRequest<'a>,
    ) -> ProjectAttachmentIntakeHttpFuture<'a, ProjectAttachmentIntakeHttpResponse> {
        Box::pin(async move {
            self.failure()?;
            self.seen.lock().unwrap().push(format!(
                "create:{}:{}:{}:{}:{}",
                request.host,
                request.holding_slug,
                request.key,
                request.input.target_id,
                request.input.expected_target_version
            ));
            Ok(ProjectAttachmentIntakeHttpResponse {
                data: serde_json::json!({
                    "id": "set-1",
                    "targetType": "project",
                    "targetId": request.input.target_id,
                    "componentKind": "record",
                    "componentRef": null,
                    "purposeKey": request.input.purpose_key,
                    "expectedTargetVersion": request.input.expected_target_version,
                    "policyVersion": 1,
                    "classification": "internal",
                    "requiredCount": 0,
                    "maxFiles": 50,
                    "state": "draft",
                    "createdAt": 1,
                    "updatedAt": 1,
                    "links": [],
                }),
                context: context(),
            })
        })
    }

    fn reserve<'a>(
        &'a self,
        request: ProjectAttachmentReserveHttpRequest<'a>,
    ) -> ProjectAttachmentIntakeHttpFuture<'a, ProjectAttachmentIntakeHttpResponse> {
        Box::pin(async move {
            self.failure()?;
            self.seen.lock().unwrap().push(format!(
                "reserve:{}:{}:{}:{}:{}",
                request.set_id,
                request.key,
                request.input.filename,
                request.input.media_type,
                request.input.bytes
            ));
            Ok(ProjectAttachmentIntakeHttpResponse {
                data: serde_json::json!({
                    "uploadIntentId": "upload-1",
                    "attachmentLinkId": "link-1",
                    "uploadUrl": "/api/v1/holdings/acme/project-attachments/uploads/upload-1/content",
                    "expiresAt": 10,
                }),
                context: context(),
            })
        })
    }

    fn complete<'a>(
        &'a self,
        request: ProjectAttachmentCompleteHttpRequest<'a>,
    ) -> ProjectAttachmentIntakeHttpFuture<'a, ProjectAttachmentIntakeHttpResponse> {
        Box::pin(async move {
            self.failure()?;
            self.seen.lock().unwrap().push(format!(
                "complete:{}:{}:{}:{}",
                request.upload_id,
                request.content_type,
                request.bytes.len(),
                request.bytes.first().copied().unwrap_or_default()
            ));
            Ok(ProjectAttachmentIntakeHttpResponse {
                data: serde_json::json!({
                    "uploadIntentId": request.upload_id,
                    "attachmentLinkId": "link-1",
                    "attachmentSetId": "set-1",
                    "sourceId": "attachment-upload-1",
                    "state": "quarantined",
                    "status": "scan_pending",
                    "scan": {
                        "attempts": 1,
                        "lastResult": "pending",
                        "lastError": null,
                        "signature": null,
                        "definitionUpdatedAt": null,
                        "scannedAt": null,
                    },
                    "replay": false,
                }),
                context: context(),
            })
        })
    }

    fn status<'a>(
        &'a self,
        request: ProjectAttachmentUploadStatusHttpRequest<'a>,
    ) -> ProjectAttachmentIntakeHttpFuture<'a, ProjectAttachmentIntakeHttpResponse> {
        Box::pin(async move {
            self.failure()?;
            self.seen
                .lock()
                .unwrap()
                .push(format!("status:{}", request.upload_id));
            Ok(ProjectAttachmentIntakeHttpResponse {
                data: serde_json::json!({
                    "uploadIntentId": request.upload_id,
                    "attachmentLinkId": "link-1",
                    "attachmentSetId": "set-1",
                    "sourceId": null,
                    "sourceState": null,
                    "linkState": "draft",
                    "status": "reserved",
                    "scan": {
                        "attempts": 0,
                        "lastResult": null,
                        "lastError": null,
                        "signature": null,
                        "definitionUpdatedAt": null,
                        "scannedAt": null,
                    },
                    "bindable": false,
                    "scannedAt": null,
                    "expiresAt": 10,
                }),
                context: context(),
            })
        })
    }
}

fn target(backend: Arc<dyn ProjectAttachmentIntakeHttpBackend>) -> ProjectAttachmentIntakeHttpTarget {
    ProjectAttachmentIntakeHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend,
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend: Arc<dyn ProjectAttachmentIntakeHttpBackend> = Arc::new(Probe::default());
    assert!(project_attachment_intake_router(vec![target(backend.clone())]).is_ok());
    assert!(
        project_attachment_intake_router(vec![
            target(backend.clone()),
            target(backend.clone())
        ])
        .is_err()
    );
    assert!(
        project_attachment_intake_router(vec![ProjectAttachmentIntakeHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target(backend.clone())
        }])
        .is_err()
    );
    assert!(
        project_attachment_intake_router(vec![ProjectAttachmentIntakeHttpTarget {
            holding_slug: "ACME".into(),
            ..target(backend)
        }])
        .is_err()
    );
}

async fn http(addr: std::net::SocketAddr, raw: &[u8]) -> (u16, serde_json::Value, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream.write_all(raw).await.unwrap();
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
        text.to_ascii_lowercase(),
    )
}

fn request(
    method: &str,
    path: &str,
    origin: Option<&str>,
    key: Option<&str>,
    content_type: Option<&str>,
    body: &[u8],
) -> Vec<u8> {
    let mut raw = format!("{method} {path} HTTP/1.1\r\nHost: acme.example.test\r\n");
    if let Some(origin) = origin {
        raw.push_str(&format!("Origin: {origin}\r\n"));
    }
    if let Some(key) = key {
        raw.push_str(&format!("Idempotency-Key: {key}\r\n"));
    }
    if let Some(content_type) = content_type {
        raw.push_str(&format!("Content-Type: {content_type}\r\n"));
    }
    raw.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    ));
    let mut bytes = raw.into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

const CREATE: &str = r#"{"targetType":"project","targetId":"project-1","componentKind":"record","purposeKey":"project_document","expectedTargetVersion":3}"#;
const RESERVE: &str = r#"{"filename":"plan.pdf","mediaType":"application/pdf","bytes":5}"#;

#[tokio::test]
async fn forwards_bounded_intake_transport_and_rejects_ambiguous_requests() {
    let probe = Arc::new(Probe::default());
    let router = project_attachment_intake_router(vec![target(probe.clone())]).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let base = "/api/v1/holdings/acme/project-attachments";
    let (status, body, headers) = http(
        addr,
        &request(
            "POST",
            &format!("{base}/attachment-sets"),
            Some("https://acme.example.test"),
            Some("create-key-0001"),
            Some("application/json"),
            CREATE.as_bytes(),
        ),
    )
    .await;
    assert_eq!(status, 201);
    assert_eq!(body["data"]["id"], "set-1");
    assert_eq!(body["context"]["userId"], "user");
    assert!(headers.contains("cache-control: private, no-store"));
    assert!(headers.contains("referrer-policy: no-referrer"));
    assert!(headers.contains("x-content-type-options: nosniff"));

    let (status, body, _) = http(
        addr,
        &request(
            "POST",
            &format!("{base}/attachment-sets/set-1/files"),
            Some("https://acme.example.test"),
            Some("reserve-key-0001"),
            Some("application/json"),
            RESERVE.as_bytes(),
        ),
    )
    .await;
    assert_eq!(status, 201);
    assert_eq!(body["data"]["uploadIntentId"], "upload-1");

    let (status, body, _) = http(
        addr,
        &request(
            "PUT",
            &format!("{base}/uploads/upload-1/content"),
            Some("https://acme.example.test"),
            None,
            Some("application/pdf"),
            b"%PDF-",
        ),
    )
    .await;
    assert_eq!(status, 202);
    assert_eq!(body["data"]["status"], "scan_pending");

    let (status, body, _) = http(
        addr,
        &request(
            "GET",
            &format!("{base}/uploads/upload-1/status"),
            None,
            None,
            None,
            b"",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["status"], "reserved");

    assert_eq!(
        probe.seen.lock().unwrap().as_slice(),
        [
            "create:acme.example.test:acme:create-key-0001:project-1:3",
            "reserve:set-1:reserve-key-0001:plan.pdf:application/pdf:5",
            "complete:upload-1:application/pdf:5:37",
            "status:upload-1",
        ]
    );

    for (raw, expected) in [
        (
            request(
                "POST",
                &format!("{base}/attachment-sets"),
                Some("https://other.example.test"),
                Some("create-key-0001"),
                Some("application/json"),
                CREATE.as_bytes(),
            ),
            403,
        ),
        (
            request(
                "POST",
                &format!("{base}/attachment-sets"),
                Some("https://acme.example.test"),
                None,
                Some("application/json"),
                CREATE.as_bytes(),
            ),
            422,
        ),
        (
            request(
                "POST",
                &format!("{base}/attachment-sets"),
                Some("https://acme.example.test"),
                Some("create-key-0001"),
                Some("text/plain"),
                CREATE.as_bytes(),
            ),
            422,
        ),
        (
            request(
                "POST",
                &format!("{base}/attachment-sets?target=2"),
                Some("https://acme.example.test"),
                Some("create-key-0001"),
                Some("application/json"),
                CREATE.as_bytes(),
            ),
            422,
        ),
        (
            request(
                "POST",
                &format!("{base}/attachment-sets"),
                Some("https://acme.example.test"),
                Some("create-key-0001"),
                Some("application/json"),
                br#"{"targetType":"project","targetId":"project-1","componentKind":"record","purposeKey":"payment_slip","expectedTargetVersion":3}"#,
            ),
            422,
        ),
        (
            request(
                "PUT",
                &format!("{base}/uploads/upload-1/content"),
                Some("https://other.example.test"),
                None,
                Some("application/pdf"),
                b"%PDF-",
            ),
            403,
        ),
        (
            request(
                "GET",
                &format!("{base}/uploads/upload-1/status"),
                None,
                None,
                None,
                b"x",
            ),
            422,
        ),
    ] {
        let (status, _, _) = http(addr, &raw).await;
        assert_eq!(status, expected);
    }

    *probe.fail.lock().unwrap() = Some(BusinessError::new(
        FailureKind::Conflict,
        "ATTACHMENT_TARGET_CHANGED",
        "The record changed. Refresh before adding files.",
    ));
    let (status, body, _) = http(
        addr,
        &request(
            "POST",
            &format!("{base}/attachment-sets"),
            Some("https://acme.example.test"),
            Some("create-key-0002"),
            Some("application/json"),
            CREATE.as_bytes(),
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "ATTACHMENT_TARGET_CHANGED");

    server.abort();
}
