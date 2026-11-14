//! Actual owner-local HTTP tests for native Projects graph and milestone commands.
use erpx_server_projects::public::project_commands_http::{
    ProjectCommandHttpResponse, ProjectCommandHttpTarget, ProjectMilestoneCommandHttpBackend,
    ProjectMilestoneSaveHttpRequest, ProjectMilestoneTransitionHttpRequest,
    project_milestone_commands_router,
};
use erpx_server_projects::public::project_graph_http::{
    ProjectGraphHttpBackend, ProjectGraphHttpFuture, ProjectGraphHttpRequest,
    ProjectGraphHttpResponse, ProjectGraphHttpTarget, project_graph_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Seen {
    saves: Vec<String>,
    transitions: Vec<String>,
    reads: Vec<String>,
}

struct GraphProbe {
    seen: Arc<Mutex<Seen>>,
    fail: bool,
}

impl ProjectGraphHttpBackend for GraphProbe {
    fn read<'a>(
        &'a self,
        request: ProjectGraphHttpRequest<'a>,
    ) -> ProjectGraphHttpFuture<'a, ProjectGraphHttpResponse> {
        Box::pin(async move {
            self.seen
                .lock()
                .unwrap()
                .reads
                .push(format!("{}:{}", request.holding_slug, request.project_id));
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "PROJECT_GRAPH_CHANGED",
                    "Synthetic changed graph.",
                ));
            }
            Ok(ProjectGraphHttpResponse {
                data: serde_json::json!({
                    "project": {"id": request.project_id},
                    "packages": []
                }),
                context: serde_json::json!({"userId": "user"}),
            })
        })
    }
}

struct CommandProbe {
    seen: Arc<Mutex<Seen>>,
    fail: bool,
}

impl ProjectMilestoneCommandHttpBackend for CommandProbe {
    fn save<'a>(
        &'a self,
        request: ProjectMilestoneSaveHttpRequest<'a>,
    ) -> erpx_server_projects::public::project_commands_http::ProjectCommandHttpFuture<
        'a,
        ProjectCommandHttpResponse,
    > {
        Box::pin(async move {
            self.seen.lock().unwrap().saves.push(format!(
                "{}:{}:{}:{}:{}:{}",
                request.holding_slug,
                request.project_id,
                request.milestone_id.unwrap_or("-"),
                request.key,
                request.input.title,
                request.input.due_at
            ));
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "PROJECT_MILESTONE_CHANGED",
                    "Synthetic milestone conflict.",
                ));
            }
            Ok(ProjectCommandHttpResponse {
                data: serde_json::json!({
                    "id": request.milestone_id.unwrap_or("milestone-command-1"),
                    "version": 1
                }),
                context: serde_json::json!({"userId": "user"}),
            })
        })
    }

    fn transition<'a>(
        &'a self,
        request: ProjectMilestoneTransitionHttpRequest<'a>,
    ) -> erpx_server_projects::public::project_commands_http::ProjectCommandHttpFuture<
        'a,
        ProjectCommandHttpResponse,
    > {
        Box::pin(async move {
            self.seen.lock().unwrap().transitions.push(format!(
                "{}:{}:{}:{}:{:?}",
                request.holding_slug, request.project_id, request.milestone_id, request.key,
                request.input.action
            ));
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "PROJECT_MILESTONE_CHANGED",
                    "Synthetic milestone conflict.",
                ));
            }
            Ok(ProjectCommandHttpResponse {
                data: serde_json::json!({
                    "id": request.milestone_id,
                    "version": 2,
                    "state": "in_progress"
                }),
                context: serde_json::json!({"userId": "user"}),
            })
        })
    }
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
        text.to_ascii_lowercase(),
    )
}

fn post(path: &str, body: &str, headers: &str) -> String {
    format!(
        "POST {path} HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://acme.example.test\r\n\
         Idempotency-Key: command-123\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",
        body.len()
    )
}

#[test]
fn command_target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(CommandProbe {
        seen: Arc::new(Mutex::new(Seen::default())),
        fail: false,
    });
    let target = || ProjectCommandHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(project_milestone_commands_router(vec![target()]).is_ok());
    assert!(project_milestone_commands_router(vec![target(), target()]).is_err());
    assert!(
        project_milestone_commands_router(vec![ProjectCommandHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
}

#[tokio::test]
async fn actual_command_router_forwards_create_revise_and_transition() {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let probe = Arc::new(CommandProbe {
        seen: seen.clone(),
        fail: false,
    });
    let router = project_milestone_commands_router(vec![ProjectCommandHttpTarget {
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

    let body = r#"{"title":"Delivery","description":"Bounded","dueAt":"2026-11-04T12:30:00.123+07:00","ownerUserId":"owner","parentMilestoneId":null,"ordinalPosition":2,"dependsOnMilestoneIds":[],"expectedVersion":1,"reason":"Milestone created by the project owner."}"#;
    let (status, value, headers) = http(
        addr,
        post(
            "/api/v1/holdings/acme/projects/project/milestones",
            body,
            "",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(value["data"]["id"], "milestone-command-1");
    assert_eq!(value["context"]["userId"], "user");
    assert!(headers.contains("cache-control: private, no-store"));
    assert!(headers.contains("referrer-policy: no-referrer"));
    assert!(headers.contains("x-content-type-options: nosniff"));
    let seen_guard = seen.lock().unwrap();
    assert_eq!(seen_guard.saves.len(), 1);
    assert!(seen_guard.saves[0].starts_with("acme:project:-:command-123:Delivery:"));
    assert!(seen_guard.saves[0].ends_with(":1793770200123"));
    drop(seen_guard);

    let (status, value, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/projects/project/milestones/m-child",
            r#"{"title":"Delivery revised","description":"","dueAt":"2026-11-04T00:00:00Z","ownerUserId":"owner","expectedVersion":2}"#,
            "",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(value["data"]["id"], "m-child");

    let (status, value, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/projects/project/milestones/m-child/transition",
            r#"{"action":"start","expectedVersion":2,"reason":"Start the reviewed milestone."}"#,
            "",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(value["data"]["state"], "in_progress");
    let seen_guard = seen.lock().unwrap();
    assert_eq!(seen_guard.transitions.len(), 1);
    assert!(seen_guard.transitions[0].contains("Start"));
    server.abort();
}

#[tokio::test]
async fn command_router_trims_only_js_whitespace_from_command_text() {
    // Zod `.trim()` strips U+FEFF but keeps U+0085; Rust `str::trim` does the
    // opposite, so the adapter must use the ECMAScript set.
    let seen = Arc::new(Mutex::new(Seen::default()));
    let probe = Arc::new(CommandProbe {
        seen: seen.clone(),
        fail: false,
    });
    let router = project_milestone_commands_router(vec![ProjectCommandHttpTarget {
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
    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/projects/project/milestones",
            r#"{"title":"\uFEFFDelivery\uFEFF","description":"\uFEFFBounded\uFEFF","dueAt":"2026-11-04T00:00:00Z","ownerUserId":"owner","reason":"\uFEFFMilestone created by the project owner.\uFEFF"}"#,
            "",
        ),
    )
    .await;
    assert_eq!(status, 200);
    {
        let seen_guard = seen.lock().unwrap();
        assert_eq!(seen_guard.saves.len(), 1);
        assert!(seen_guard.saves[0].contains(":Delivery:"));
        assert!(!seen_guard.saves[0].contains('\u{feff}'));
    }
    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/projects/project/milestones",
            r#"{"title":"\u0085Delivery","dueAt":"2026-11-04T00:00:00Z","ownerUserId":"owner"}"#,
            "",
        ),
    )
    .await;
    assert_eq!(status, 200);
    {
        let seen_guard = seen.lock().unwrap();
        assert_eq!(seen_guard.saves.len(), 2);
        assert!(seen_guard.saves[1].contains(":\u{0085}Delivery:"));
    }
    server.abort();
}

#[tokio::test]
async fn command_router_rejects_missing_origin_key_media_type_and_bad_input() {
    let probe = Arc::new(CommandProbe {
        seen: Arc::new(Mutex::new(Seen::default())),
        fail: false,
    });
    let router = project_milestone_commands_router(vec![ProjectCommandHttpTarget {
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
    let path = "/api/v1/holdings/acme/projects/project/milestones";
    let body = r#"{"title":"Delivery","dueAt":"2026-11-04T00:00:00Z","ownerUserId":"owner"}"#;

    let (status, value, _) = http(
        addr,
        format!("POST {path} HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://acme.example.test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "COMMAND_KEY_REQUIRED");

    let (status, value, _) = http(
        addr,
        format!("POST {path} HTTP/1.1\r\nHost: acme.example.test\r\nIdempotency-Key: command-123\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(value["error"]["code"], "ORIGIN_FORBIDDEN");

    let (status, value, _) = http(
        addr,
        format!("POST {path} HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://evil.example.test\r\nIdempotency-Key: command-123\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(value["error"]["code"], "ORIGIN_FORBIDDEN");

    let (status, value, _) = http(
        addr,
        format!("POST {path} HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://acme.example.test\r\nIdempotency-Key: command-123\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "PROJECTS_REQUEST_INVALID");

    let (status, value, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/projects/project/milestones?view=all",
            body,
            "",
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "PROJECTS_REQUEST_INVALID");

    let (status, value, _) = http(
        addr,
        post(
            path,
            r#"{"title":"Delivery","dueAt":"not-a-time","ownerUserId":"owner"}"#,
            "",
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "PROJECT_MILESTONE_INVALID");

    let (status, value, _) = http(
        addr,
        post(path, r#"{"title":"Delivery","dueAt":"2026-11-04T00:00:00Z","ownerUserId":"owner","extra":true}"#, ""),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "PROJECTS_REQUEST_INVALID");
    server.abort();
}

#[tokio::test]
async fn command_router_maps_backend_failures_without_fallback() {
    let probe = Arc::new(CommandProbe {
        seen: Arc::new(Mutex::new(Seen::default())),
        fail: true,
    });
    let router = project_milestone_commands_router(vec![ProjectCommandHttpTarget {
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
    let (status, value, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/projects/project/milestones/m-child/transition",
            r#"{"action":"complete","expectedVersion":2,"reason":"Complete the reviewed milestone."}"#,
            "",
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(value["error"]["code"], "PROJECT_MILESTONE_CHANGED");
    server.abort();
}

#[test]
fn graph_target_configuration_and_actual_read_are_bounded() {
    let backend = Arc::new(GraphProbe {
        seen: Arc::new(Mutex::new(Seen::default())),
        fail: false,
    });
    let target = || ProjectGraphHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(project_graph_router(vec![target()]).is_ok());
    assert!(project_graph_router(vec![target(), target()]).is_err());
}

#[tokio::test]
async fn actual_graph_router_retains_scope_and_maps_errors() {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let router = project_graph_router(vec![
        ProjectGraphHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(GraphProbe {
                seen: seen.clone(),
                fail: false,
            }),
        },
        ProjectGraphHttpTarget {
            host: "fail.example.test".into(),
            holding_slug: "fail".into(),
            origin: "https://fail.example.test".into(),
            backend: Arc::new(GraphProbe {
                seen: seen.clone(),
                fail: true,
            }),
        },
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    let get = |host: &str, path: &str| {
        format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n")
    };
    let (status, value, headers) = http(
        addr,
        get(
            "acme.example.test",
            "/api/v1/holdings/acme/projects/project/graph",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(value["data"]["project"]["id"], "project");
    assert!(headers.contains("cache-control: private, no-store"));

    let (status, value, _) = http(
        addr,
        get(
            "acme.example.test",
            "/api/v1/holdings/acme/projects/project/graph?view=all",
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "PROJECTS_REQUEST_INVALID");

    let (status, value, _) = http(
        addr,
        get(
            "fail.example.test",
            "/api/v1/holdings/fail/projects/project/graph",
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(value["error"]["code"], "PROJECT_GRAPH_CHANGED");
    assert_eq!(seen.lock().unwrap().reads.len(), 2);
    server.abort();
}
