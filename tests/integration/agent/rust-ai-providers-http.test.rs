//! Actual owner-local AI provider HTTP router tests. These prove the retained
//! path/idempotency/origin contract only; current authority, credential sealing,
//! provider transport and evidence stay behind the backend.
use erpx_server_agent::public::providers::{
    AiProviderHttpBackend, AiProviderHttpCommandRequest, AiProviderHttpFuture,
    AiProviderHttpReadRequest, AiProviderHttpTarget, ai_provider_http_router,
};
use erpx_server_runtime::public::http_support::serve;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl AiProviderHttpBackend for Noop {
    fn read<'a>(
        &'a self,
        _: AiProviderHttpReadRequest<'a>,
    ) -> AiProviderHttpFuture<'a, serde_json::Value> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
    fn command<'a>(
        &'a self,
        _: AiProviderHttpCommandRequest<'a>,
    ) -> AiProviderHttpFuture<'a, serde_json::Value> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Seen {
    method: &'static str,
    host: String,
    holding: String,
    path: Vec<String>,
    key: Option<String>,
}

struct Probe {
    seen: Arc<Mutex<Vec<Seen>>>,
    fail: Option<BusinessError>,
}

impl AiProviderHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: AiProviderHttpReadRequest<'a>,
    ) -> AiProviderHttpFuture<'a, serde_json::Value> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(Seen {
                method: "read",
                host: request.host.into(),
                holding: request.holding_slug.into(),
                path: request.path,
                key: None,
            });
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(serde_json::json!({"connections": [], "selection": {"mode": "disabled"}}))
        })
    }
    fn command<'a>(
        &'a self,
        request: AiProviderHttpCommandRequest<'a>,
    ) -> AiProviderHttpFuture<'a, serde_json::Value> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(Seen {
                method: "command",
                host: request.host.into(),
                holding: request.holding_slug.into(),
                path: request.path,
                key: request.key.map(str::to_owned),
            });
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(serde_json::json!({"models": ["model-1"], "totalModels": 1}))
        })
    }
}

fn target(backend: Arc<dyn AiProviderHttpBackend>) -> AiProviderHttpTarget {
    AiProviderHttpTarget {
        host: "erpx-internal.example.test".into(),
        holding_slug: "erpx".into(),
        origin: "https://erpx-internal.example.test".into(),
        backend,
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend: Arc<dyn AiProviderHttpBackend> = Arc::new(Noop);
    assert!(ai_provider_http_router(vec![target(backend.clone())]).is_ok());
    assert!(
        ai_provider_http_router(vec![target(backend.clone()), target(backend.clone())]).is_err()
    );
    for invalid in [
        AiProviderHttpTarget {
            origin: "http://erpx-internal.example.test".into(),
            ..target(backend.clone())
        },
        AiProviderHttpTarget {
            origin: "https://erpx-internal.example.test/path".into(),
            ..target(backend.clone())
        },
        AiProviderHttpTarget {
            holding_slug: "ERPX".into(),
            ..target(backend.clone())
        },
        AiProviderHttpTarget {
            host: String::new(),
            ..target(backend.clone())
        },
    ] {
        assert!(ai_provider_http_router(vec![invalid]).is_err());
    }
}

async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, serde_json::Value) {
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
    )
}

fn get(path: &str, host: &str) -> String {
    format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n")
}
fn post(path: &str, host: &str, origin: &str, key: Option<&str>, body: &str) -> String {
    format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nOrigin: {origin}\r\nContent-Type: application/json\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        key.map(|key| format!("Idempotency-Key: {key}\r\n"))
            .unwrap_or_default(),
        body.len()
    )
}

#[tokio::test]
async fn provider_paths_keep_key_and_origin_rules() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = ai_provider_http_router(vec![target(Arc::new(Probe {
        seen: seen.clone(),
        fail: None,
    }))])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let base = "/api/v1/holdings/erpx/ai-providers";
    let (status, body) = http(addr, get(base, "erpx-internal.example.test")).await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["selection"]["mode"], "disabled");
    assert_eq!(
        seen.lock().unwrap().last().cloned(),
        Some(Seen {
            method: "read",
            host: "erpx-internal.example.test".into(),
            holding: "erpx".into(),
            path: vec![],
            key: None,
        })
    );

    let payload =
        r#"{"name":"Provider","baseUrl":"https://api.provider.com/v1","textModel":"model-1"}"#;
    let (status, body) = http(
        addr,
        post(
            base,
            "erpx-internal.example.test",
            "https://erpx-internal.example.test",
            None,
            payload,
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "COMMAND_KEY_REQUIRED");

    let (status, _) = http(
        addr,
        post(
            base,
            "erpx-internal.example.test",
            "https://other.example.test",
            Some("provider-key-1"),
            payload,
        ),
    )
    .await;
    assert_eq!(status, 403);

    let (status, body) = http(
        addr,
        post(
            base,
            "erpx-internal.example.test",
            "https://erpx-internal.example.test",
            Some("provider-key-1"),
            payload,
        ),
    )
    .await;
    assert_eq!(status, 201);
    assert_eq!(body["data"]["totalModels"], 1);

    let discover = "/api/v1/holdings/erpx/ai-providers/discover";
    let (status, _) = http(
        addr,
        post(
            discover,
            "erpx-internal.example.test",
            "https://erpx-internal.example.test",
            None,
            r#"{"baseUrl":"https://api.provider.com/v1","authMode":"none"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        seen.lock().unwrap().last().cloned(),
        Some(Seen {
            method: "command",
            host: "erpx-internal.example.test".into(),
            holding: "erpx".into(),
            path: vec!["discover".into()],
            key: None,
        })
    );

    let (status, _) = http(
        addr,
        post(
            "/api/v1/holdings/erpx/ai-providers/ai-1/test",
            "erpx-internal.example.test",
            "https://erpx-internal.example.test",
            Some("provider-key-2"),
            r#"{"expectedVersion":2}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        seen.lock().unwrap().last().cloned().unwrap().path,
        vec!["ai-1", "test"]
    );

    let (status, _) = http(
        addr,
        get(
            "/api/v1/holdings/erpx/ai-providers/ai-1/models",
            "erpx-internal.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        seen.lock().unwrap().last().cloned().unwrap().path,
        vec!["ai-1", "models"]
    );

    for invalid in [r#"[]"#, r#"null"#, r#""string""#, r#"not-json"#] {
        let (status, _) = http(
            addr,
            post(
                base,
                "erpx-internal.example.test",
                "https://erpx-internal.example.test",
                Some("provider-key-3"),
                invalid,
            ),
        )
        .await;
        assert_eq!(status, 422, "body: {invalid}");
    }
}
