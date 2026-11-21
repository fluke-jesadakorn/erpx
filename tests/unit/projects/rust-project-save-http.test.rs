//! Actual owner-local HTTP router tests for native Projects create/edit.
use erpx_server_projects::public::project_save_http::{
    ProjectSaveHttpBackend, ProjectSaveHttpFuture, ProjectSaveHttpRequest,
    ProjectSaveHttpResponse, ProjectSaveHttpTarget, project_save_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl ProjectSaveHttpBackend for Noop {
    fn save<'a>(
        &'a self,
        _: ProjectSaveHttpRequest<'a>,
    ) -> ProjectSaveHttpFuture<'a, ProjectSaveHttpResponse> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
}

#[derive(Clone)]
struct Seen {
    project_id: Option<String>,
    title: String,
    description: String,
    members: Vec<String>,
    expected_version: i64,
    key: String,
    holding_slug: String,
}

struct Probe {
    seen: Arc<Mutex<Vec<Seen>>>,
    fail: Option<BusinessError>,
}

impl ProjectSaveHttpBackend for Probe {
    fn save<'a>(
        &'a self,
        request: ProjectSaveHttpRequest<'a>,
    ) -> ProjectSaveHttpFuture<'a, ProjectSaveHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(Seen {
                project_id: request.project_id.map(str::to_owned),
                title: request.input.title.clone(),
                description: request.input.description.clone(),
                members: request.input.member_user_ids.clone(),
                expected_version: request.input.expected_version,
                key: request.key.to_owned(),
                holding_slug: request.holding_slug.to_owned(),
            });
            if let Some(error) = &self.fail {
                return Err(error.clone());
            }
            Ok(ProjectSaveHttpResponse {
                data: serde_json::json!({
                    "id": request.project_id.unwrap_or("project-command"),
                    "version": 1,
                }),
                context: serde_json::json!({"userId": "user"}),
            })
        })
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(Noop);
    let target = || ProjectSaveHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(project_save_router(vec![target()]).is_ok());
    assert!(project_save_router(vec![target(), target()]).is_err());
    assert!(
        project_save_router(vec![ProjectSaveHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        project_save_router(vec![ProjectSaveHttpTarget {
            holding_slug: String::new(),
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

fn post(path: &str, host: &str, origin: &str, key: Option<&str>, body: &str) -> String {
    let key = key.map_or_else(String::new, |key| format!("Idempotency-Key: {key}\r\n"));
    format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nOrigin: {origin}\r\n{key}\
         Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

const BODY: &str = r#"{"title":"Customer delivery","description":"Bounded brief","memberUserIds":["member","member"],"expectedVersion":3}"#;

async fn server(probe: Arc<Probe>) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let router = project_save_router(vec![ProjectSaveHttpTarget {
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
    (addr, server)
}

#[tokio::test]
async fn create_and_edit_dispatch_the_normalized_owner_input() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let probe = Arc::new(Probe {
        seen: seen.clone(),
        fail: None,
    });
    let (addr, server) = server(probe).await;

    let (status, body) = http(
        addr,
        post(
            "/api/v1/holdings/acme/projects",
            "acme.example.test",
            "https://acme.example.test",
            Some("command-key-1"),
            BODY,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["version"], 1);
    assert_eq!(body["context"]["userId"], "user");
    let response = body.as_object().unwrap();
    assert_eq!(response.len(), 2);
    assert!(response.contains_key("data") && response.contains_key("context"));

    let (status, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/projects/project-1",
            "acme.example.test",
            "https://acme.example.test",
            Some("command-key-2"),
            r#"{"title":"Edited delivery","memberUserIds":[],"expectedVersion":4}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);

    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].project_id, None);
    assert_eq!(seen[0].holding_slug, "acme");
    assert_eq!(seen[0].key, "command-key-1");
    assert_eq!(seen[0].title, "Customer delivery");
    assert_eq!(seen[0].description, "Bounded brief");
    assert_eq!(seen[0].members, vec!["member", "member"]);
    assert_eq!(seen[0].expected_version, 3);
    assert_eq!(seen[1].project_id.as_deref(), Some("project-1"));
    assert_eq!(seen[1].description, "");
    assert!(seen[1].members.is_empty());
    assert_eq!(seen[1].expected_version, 4);
    server.abort();
}

#[tokio::test]
async fn transport_facts_are_exact_and_required() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let probe = Arc::new(Probe {
        seen: seen.clone(),
        fail: None,
    });
    let (addr, server) = server(probe).await;
    let path = "/api/v1/holdings/acme/projects";

    let (status, body) = http(
        addr,
        post(
            path,
            "other.example.test",
            "https://acme.example.test",
            Some("command-key-1"),
            BODY,
        ),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let (status, body) = http(
        addr,
        post(
            path,
            "acme.example.test",
            "https://evil.example.test",
            Some("command-key-1"),
            BODY,
        ),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(body["error"]["code"], "ORIGIN_FORBIDDEN");

    let (status, body) = http(addr, post(path, "acme.example.test", "https://acme.example.test", None, BODY)).await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "COMMAND_KEY_REQUIRED");

    let (status, body) = http(
        addr,
        format!(
            "POST {path} HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://acme.example.test\r\n\
             Idempotency-Key: command-key-1\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n{BODY}",
            BODY.len()
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "PROJECTS_REQUEST_INVALID");

    let (status, body) = http(
        addr,
        post(
            "/api/v1/holdings/acme/projects?view=all",
            "acme.example.test",
            "https://acme.example.test",
            Some("command-key-1"),
            BODY,
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "PROJECTS_REQUEST_INVALID");

    let oversized = "a".repeat(201);
    let (status, body) = http(
        addr,
        post(
            &format!("/api/v1/holdings/acme/projects/{oversized}"),
            "acme.example.test",
            "https://acme.example.test",
            Some("command-key-1"),
            BODY,
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "PROJECTS_REQUEST_INVALID");

    // Unknown fields are denied at the owner boundary.
    let (status, body) = http(
        addr,
        post(
            path,
            "acme.example.test",
            "https://acme.example.test",
            Some("command-key-1"),
            r#"{"title":"Customer delivery","ownerUserId":"intruder"}"#,
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "PROJECTS_REQUEST_INVALID");

    // Retained Zod trimming applies before the owner call.
    let (status, _) = http(
        addr,
        post(
            path,
            "acme.example.test",
            "https://acme.example.test",
            Some("command-key-3"),
            "{\"title\":\"  Customer delivery  \",\"description\":\"  Bounded  \"}",
        ),
    )
    .await;
    assert_eq!(status, 200);
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].title, "Customer delivery");
    assert_eq!(seen[0].description, "Bounded");
    assert_eq!(seen[0].key, "command-key-3");
    server.abort();
}

#[tokio::test]
async fn body_is_bounded_to_eight_kibibytes() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let probe = Arc::new(Probe {
        seen: seen.clone(),
        fail: None,
    });
    let (addr, server) = server(probe).await;
    let body = format!(
        r#"{{"title":"Customer delivery","description":"{}"}}"#,
        "a".repeat(9 * 1024)
    );
    let (status, value) = http(
        addr,
        post(
            "/api/v1/holdings/acme/projects",
            "acme.example.test",
            "https://acme.example.test",
            Some("command-key-1"),
            &body,
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "PROJECTS_REQUEST_INVALID");
    assert!(seen.lock().unwrap().is_empty());
    server.abort();
}

#[tokio::test]
async fn owner_errors_map_to_status_and_code_and_headers_stay_private() {
    let probe = Arc::new(Probe {
        seen: Arc::new(Mutex::new(Vec::new())),
        fail: Some(BusinessError::new(
            FailureKind::Forbidden,
            "PACKAGE_REQUIRED",
            "This Project work requires its purchased capability.",
        )),
    });
    let (addr, server) = server(probe).await;
    let (status, body) = http(
        addr,
        post(
            "/api/v1/holdings/acme/projects",
            "acme.example.test",
            "https://acme.example.test",
            Some("command-key-1"),
            BODY,
        ),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(body["error"]["code"], "PACKAGE_REQUIRED");
    assert_eq!(
        body["error"]["message"],
        "This Project work requires its purchased capability."
    );
    server.abort();
}

#[tokio::test]
async fn successful_responses_are_private_and_unbuffered_by_proxies() {
    let probe = Arc::new(Probe {
        seen: Arc::new(Mutex::new(Vec::new())),
        fail: None,
    });
    let (addr, server) = server(probe).await;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(
            post(
                "/api/v1/holdings/acme/projects",
                "acme.example.test",
                "https://acme.example.test",
                Some("command-key-1"),
                BODY,
            )
            .as_bytes(),
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
