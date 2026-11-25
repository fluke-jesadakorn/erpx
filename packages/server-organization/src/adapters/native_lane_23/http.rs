//! Owner-local axum ingress for the native Organization directory hierarchy.
//! The router transports facts only: exact registered Host/Holding target,
//! bounded JSON, fail-closed error mapping and no SQL of its own. The reader
//! port is supplied by the Pg owner adapter; composition mounts this router.
use super::application::{read_directory_hierarchy, DirectoryHierarchyRead};
use super::postgres::PgDirectoryHierarchyRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

/// An exact registered browser origin/Holding pair. Duplicates and loose
/// patterns are rejected so one Holding cannot read another's directory.
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn DirectoryHierarchyRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "ORGANIZATION_DIRECTORY_READ_REQUEST_INVALID",
        "Use one bounded Organization directory read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "ORGANIZATION_DIRECTORY_READ_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(malformed());
    }
    first
        .map(|value| value.to_str().map_err(|_| malformed()))
        .transpose()
}

fn status(kind: FailureKind) -> StatusCode {
    match kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn error_response(error: BusinessError) -> Response {
    (
        status(error.kind),
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn bounded(body: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(body)).into_response();
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    response
}

fn router_with_reader(
    reader: Arc<dyn DirectoryHierarchyRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    reader.clone(),
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/organization/directory-hierarchy",
            get(read_hierarchy),
        )
        .with_state(Arc::new(LaneState {
            targets: configured,
        })))
}

/// Build the native read router. `pool` is the owner Pg pool; the lead mounts
/// the returned router and owns the registered target list.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let reader: Arc<dyn DirectoryHierarchyRead> = Arc::new(PgDirectoryHierarchyRead::new(pool));
    router_with_reader(reader, targets)
}

async fn read_hierarchy(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        if request.uri().query().is_some() {
            return Err(malformed());
        }
        let facts = read_directory_hierarchy(reader.as_ref()).await?;
        let data = serde_json::to_value(facts).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "ORGANIZATION_DIRECTORY_HIERARCHY_RESPONSE_INVALID",
                "The native Organization directory hierarchy could not be encoded.",
            )
        })?;
        Ok::<_, BusinessError>(serde_json::json!({"data": data}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_23::application::DirectoryHierarchyFuture;
    use crate::adapters::native_lane_23::domain::{
        DirectoryHierarchy, DirectoryLevel, DirectoryNode, DirectoryPosition,
    };

    #[derive(Clone)]
    struct Stub {
        result: Result<DirectoryHierarchy, BusinessError>,
    }

    impl DirectoryHierarchyRead for Stub {
        fn read<'a>(&'a self) -> DirectoryHierarchyFuture<'a, DirectoryHierarchy> {
            let result = self.result.clone();
            Box::pin(async move { result })
        }
    }

    fn facts() -> DirectoryHierarchy {
        DirectoryHierarchy {
            nodes: vec![DirectoryNode {
                id: "n1".into(),
                company_id: "c1".into(),
                kind: "company".into(),
                slug: "acme".into(),
                display_name: "Acme".into(),
                parent_id: None,
                branch_id: None,
            }],
            levels: vec![DirectoryLevel {
                id: "l1".into(),
                company_id: "c1".into(),
                code: "L1".into(),
                name: "One".into(),
                level_number: Some(1),
            }],
            positions: vec![DirectoryPosition {
                id: "p1".into(),
                company_id: "c1".into(),
                branch_id: None,
                department_id: None,
                title: "CEO".into(),
                level_id: "l1".into(),
                reports_to_position_id: None,
                is_ceo: 1,
            }],
        }
    }

    fn target() -> LaneTarget {
        LaneTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
        }
    }

    #[test]
    fn target_configuration_requires_distinct_exact_holdings() {
        let reader: Arc<dyn DirectoryHierarchyRead> = Arc::new(Stub { result: Ok(facts()) });
        assert!(router_with_reader(reader.clone(), vec![target()]).is_ok());
        assert!(router_with_reader(reader.clone(), vec![target(), target()]).is_err());
        assert!(router_with_reader(reader.clone(), vec![]).is_err());
        assert!(
            router_with_reader(
                reader,
                vec![LaneTarget {
                    holding_slug: "Acme".into(),
                    ..target()
                }]
            )
            .is_err()
        );
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, String, serde_json::Value) {
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
            text.to_string(),
            serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
        )
    }

    #[tokio::test]
    async fn actual_router_returns_bounded_owner_facts_and_maps_failures() {
        let reader: Arc<dyn DirectoryHierarchyRead> = Arc::new(Stub { result: Ok(facts()) });
        let router = router_with_reader(reader, vec![target()]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });

        let ok = http(
            addr,
            "GET /api/v1/holdings/acme/organization/directory-hierarchy HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n".to_string(),
        )
        .await;
        assert_eq!(ok.0, 200);
        assert_eq!(ok.2["data"]["nodes"][0]["slug"], "acme");
        assert_eq!(ok.2["data"]["positions"][0]["is_ceo"], 1);
        assert!(ok.1.to_ascii_lowercase().contains("cache-control: private, no-store"));

        let wrong_host = http(
            addr,
            "GET /api/v1/holdings/acme/organization/directory-hierarchy HTTP/1.1\r\nHost: other.example.test\r\nConnection: close\r\n\r\n".to_string(),
        )
        .await;
        assert_eq!(wrong_host.0, 404);
        assert_eq!(wrong_host.2["error"]["code"], "NOT_FOUND");

        let _ = server.abort();
    }

    #[tokio::test]
    async fn reader_failure_fails_closed_with_503() {
        let reader: Arc<dyn DirectoryHierarchyRead> = Arc::new(Stub {
            result: Err(BusinessError::new(
                FailureKind::Unavailable,
                "NATIVE_READ_DOWN",
                "The native reader is unavailable.",
            )),
        });
        let router = router_with_reader(reader, vec![target()]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        let failed = http(
            addr,
            "GET /api/v1/holdings/acme/organization/directory-hierarchy HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n".to_string(),
        )
        .await;
        assert_eq!(failed.0, 503);
        assert_eq!(failed.2["error"]["code"], "NATIVE_READ_DOWN");
        let _ = server.abort();
    }
}
