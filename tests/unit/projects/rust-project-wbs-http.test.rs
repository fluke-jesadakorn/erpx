//! Actual owner-local HTTP router tests for the native Projects WBS read.
use erpx_server_projects::public::project_wbs_http::{
    ProjectWbsHttpBackend, ProjectWbsHttpFuture, ProjectWbsHttpRequest, ProjectWbsHttpResponse,
    ProjectWbsHttpTarget, projects_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl ProjectWbsHttpBackend for Noop {
    fn read<'a>(
        &'a self,
        _: ProjectWbsHttpRequest<'a>,
    ) -> ProjectWbsHttpFuture<'a, ProjectWbsHttpResponse> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
}

struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: bool,
}

impl ProjectWbsHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: ProjectWbsHttpRequest<'a>,
    ) -> ProjectWbsHttpFuture<'a, ProjectWbsHttpResponse> {
        Box::pin(async move {
            self.seen
                .lock()
                .unwrap()
                .push(format!("read:{}", request.project_id));
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "PROJECT_WBS_CHANGED",
                    "Synthetic changed project structure.",
                ));
            }
            Ok(ProjectWbsHttpResponse {
                data: serde_json::json!({
                    "project": {"id": request.project_id},
                    "milestones": []
                }),
                context: serde_json::json!({"userId": "user"}),
            })
        })
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(Noop);
    let target = || ProjectWbsHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(projects_router(vec![target()]).is_ok());
    assert!(projects_router(vec![target(), target()]).is_err());
    assert!(
        projects_router(vec![ProjectWbsHttpTarget {
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

#[tokio::test]
async fn actual_router_retains_target_project_and_error_mapping() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let probe = Arc::new(Probe {
        seen: seen.clone(),
        fail: false,
    });
    let failing = Arc::new(Probe {
        seen: seen.clone(),
        fail: true,
    });
    let router = projects_router(vec![
        ProjectWbsHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: probe,
        },
        ProjectWbsHttpTarget {
            host: "fail.example.test".into(),
            holding_slug: "fail".into(),
            origin: "https://fail.example.test".into(),
            backend: failing,
        },
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    let get = |path: &str| {
        format!("GET {path} HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n")
    };
    let (status, body) = http(addr, get("/api/v1/holdings/acme/projects/project/wbs")).await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["project"]["id"], "project");
    assert_eq!(body["data"]["milestones"], serde_json::json!([]));
    assert_eq!(body["context"]["userId"], "user");

    let oversized = "a".repeat(201);
    let (status, body) = http(
        addr,
        get(&format!("/api/v1/holdings/acme/projects/{oversized}/wbs")),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "PROJECTS_REQUEST_INVALID");

    let (status, body) = http(
        addr,
        get("/api/v1/holdings/acme/projects/project/wbs?view=all"),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "PROJECTS_REQUEST_INVALID");

    let (status, _) = http(addr, get("/api/v1/holdings/missing/projects/project/wbs")).await;
    assert_eq!(status, 404);

    let (status, body) = http(
        addr,
        "GET /api/v1/holdings/fail/projects/project/wbs HTTP/1.1\r\nHost: fail.example.test\r\nConnection: close\r\n\r\n".into(),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "PROJECT_WBS_CHANGED");
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen, vec!["read:project", "read:project"]);
    server.abort();
}

#[tokio::test]
async fn response_headers_are_private_and_bounded() {
    let probe = Arc::new(Probe {
        seen: Arc::new(Mutex::new(Vec::new())),
        fail: false,
    });
    let router = projects_router(vec![ProjectWbsHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: probe,
    }])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(
            b"GET /api/v1/holdings/acme/projects/project/wbs HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n",
        )
        .await
        .unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let text = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
    assert!(text.contains("cache-control: private, no-store"));
    assert!(text.contains("referrer-policy: no-referrer"));
    assert!(text.contains("x-content-type-options: nosniff"));
    server.abort();
}
