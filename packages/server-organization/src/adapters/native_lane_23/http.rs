//! Owner-local axum ingress for the native Organization directory hierarchy.
//! The router transports facts only: exact registered Host/Holding target,
//! bounded JSON, fail-closed error mapping and no SQL of its own. The reader
//! port is supplied by the Pg owner adapter; composition mounts this router.
use super::application::{
    read_directory_hierarchy, read_executive_evidence, read_node_references,
    DirectoryHierarchyRead, ExecutiveEvidenceRead, ExecutiveEvidenceRequest, NodeReferenceRead,
};
use super::domain::ExecutiveEvidenceKind;
use super::postgres::PgDirectoryHierarchyRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Query, Request, Response, Router,
    State, StatusCode, body::to_bytes, header, routing::{get, post}, serde_json,
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

const BODY_LIMIT: usize = 16 * 1024;

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn DirectoryHierarchyRead>>,
    reference_targets: BTreeMap<(String, String), Arc<dyn NodeReferenceRead>>,
    executive_targets: BTreeMap<(String, String), Arc<dyn ExecutiveEvidenceRead>>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExecutiveEvidenceBody {
    kind: String,
    company_id: String,
    #[serde(default)]
    values: Vec<Option<String>>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    node_id: Option<String>,
    #[serde(default)]
    cursor_name: Option<String>,
    #[serde(default)]
    cursor_at: Option<i64>,
    #[serde(default)]
    cursor_id: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReferenceQuery {
    company_id: String,
    node_id: Option<String>,
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

fn valid_scope(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
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

fn router_with_maps(
    targets: BTreeMap<(String, String), Arc<dyn DirectoryHierarchyRead>>,
    reference_targets: BTreeMap<(String, String), Arc<dyn NodeReferenceRead>>,
    executive_targets: BTreeMap<(String, String), Arc<dyn ExecutiveEvidenceRead>>,
) -> Result<Router, BusinessError> {
    if targets.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/organization/directory-hierarchy",
            get(read_hierarchy),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/organization/node-references",
            get(read_references),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/organization/executive-evidence",
            post(read_evidence),
        )
        .with_state(Arc::new(LaneState {
            targets,
            reference_targets,
            executive_targets,
        })))
}

/// Reader-injection path used by the owner-local contract tests. Every target
/// carries its own reader, so distinct Holdings can use distinct pools.
#[cfg(test)]
fn router_with_readers(
    targets: Vec<(LaneTarget, Arc<dyn DirectoryHierarchyRead>)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    reader,
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    router_with_maps(configured, BTreeMap::new(), BTreeMap::new())
}

/// Full injection path: one reader for each family, keyed by the same
/// `(host, holding_slug)` pair so a Holding cannot reach another Holding's pool.
fn router_with_all(
    targets: Vec<(
        LaneTarget,
        Arc<dyn DirectoryHierarchyRead>,
        Arc<dyn NodeReferenceRead>,
        Arc<dyn ExecutiveEvidenceRead>,
    )>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    let mut references = BTreeMap::new();
    let mut executives = BTreeMap::new();
    for (target, reader, reference_reader, executive_reader) in targets {
        let key = (target.host.clone(), target.holding_slug.clone());
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured.insert(key.clone(), reader).is_some()
            || references.insert(key.clone(), reference_reader).is_some()
            || executives.insert(key, executive_reader).is_some()
        {
            return Err(configuration_invalid());
        }
    }
    router_with_maps(configured, references, executives)
}

#[cfg(test)]
fn router_with_reader(
    reader: Arc<dyn DirectoryHierarchyRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

/// Build the native read router. `pool` is the owner Pg pool; the lead mounts
/// the returned router and owns the registered target list.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let reader = Arc::new(PgDirectoryHierarchyRead::new(pool));
    router_with_all(
        targets
            .into_iter()
            .map(|target| {
                (
                    target,
                    reader.clone() as Arc<dyn DirectoryHierarchyRead>,
                    reader.clone() as Arc<dyn NodeReferenceRead>,
                    reader.clone() as Arc<dyn ExecutiveEvidenceRead>,
                )
            })
            .collect(),
    )
}

/// Build the native read router with one owner reader and database pool per
/// Holding. Each `(LaneTarget, DatabasePool)` pair is bound to a distinct
/// `PgDirectoryHierarchyRead`, keyed by `(host, holding_slug)` in both state
/// maps, so a request can only reach the pool registered for its Holding.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    router_with_all(
        targets
            .into_iter()
            .map(|(target, pool)| {
                let reader = Arc::new(PgDirectoryHierarchyRead::new(pool));
                (
                    target,
                    reader.clone() as Arc<dyn DirectoryHierarchyRead>,
                    reader.clone() as Arc<dyn NodeReferenceRead>,
                    reader.clone() as Arc<dyn ExecutiveEvidenceRead>,
                )
            })
            .collect(),
    )
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

async fn read_references(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    Query(query): Query<ReferenceQuery>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .reference_targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        if !valid_scope(&query.company_id)
            || query
                .node_id
                .as_deref()
                .is_some_and(|node| !valid_scope(node))
        {
            return Err(malformed());
        }
        let facts = read_node_references(
            reader.as_ref(),
            &query.company_id,
            query.node_id.as_deref(),
        )
        .await?;
        let data = serde_json::to_value(facts).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "ORGANIZATION_NODE_REFERENCES_RESPONSE_INVALID",
                "The native Organization node references could not be encoded.",
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

async fn decode_evidence(request: Request) -> Result<ExecutiveEvidenceBody, BusinessError> {
    if request.uri().query().is_some() {
        return Err(malformed());
    }
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| malformed())?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| malformed())?;
    if !value.is_object() {
        return Err(malformed());
    }
    serde_json::from_value::<ExecutiveEvidenceBody>(value).map_err(|_| malformed())
}

async fn read_evidence(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .executive_targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let body = decode_evidence(request).await?;
        let kind = ExecutiveEvidenceKind::parse(&body.kind).ok_or_else(malformed)?;
        let request = ExecutiveEvidenceRequest {
            kind,
            company_id: body.company_id,
            values: body.values,
            query: body.query,
            limit: body.limit,
            node_id: body.node_id,
            cursor_name: body.cursor_name,
            cursor_at: body.cursor_at,
            cursor_id: body.cursor_id,
        };
        let facts = read_executive_evidence(reader.as_ref(), &request).await?;
        let data = serde_json::to_value(facts).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "ORGANIZATION_EXECUTIVE_EVIDENCE_RESPONSE_INVALID",
                "The native Organization executive evidence could not be encoded.",
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
    use crate::adapters::native_lane_23::application::{
        DirectoryHierarchyFuture, ExecutiveEvidenceRead, NodeReferenceRead,
    };
    use crate::adapters::native_lane_23::domain::{
        AttachmentNodeFact, AudienceNode, DirectoryHierarchy, DirectoryLevel, DirectoryNode,
        DirectoryPosition, ExecutiveEvidence, NodeReferences, UnitReference,
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

    #[derive(Clone)]
    struct RefStub {
        result: Result<NodeReferences, BusinessError>,
    }

    impl NodeReferenceRead for RefStub {
        fn node_references<'a>(
            &'a self,
            _: &'a str,
            _: Option<&'a str>,
        ) -> DirectoryHierarchyFuture<'a, NodeReferences> {
            let result = self.result.clone();
            Box::pin(async move { result })
        }
    }

    fn references(id: &str, version: i64) -> NodeReferences {
        NodeReferences {
            node: Some(AudienceNode {
                id: id.into(),
                kind: "branch".into(),
                branch_id: None,
            }),
            units: vec![UnitReference {
                id: id.into(),
                name: "Branch".into(),
                kind: "branch".into(),
                branch_id: None,
            }],
            attachment: Some(AttachmentNodeFact {
                company_id: "c1".into(),
                branch_id: None,
                department_id: None,
                version,
                kind: "branch".into(),
            }),
        }
    }

    #[derive(Clone)]
    struct EvidenceStub {
        rows: Result<Vec<serde_json::Value>, BusinessError>,
    }

    impl ExecutiveEvidenceRead for EvidenceStub {
        fn evidence<'a>(
            &'a self,
            _: &'a ExecutiveEvidenceRequest,
        ) -> DirectoryHierarchyFuture<'a, ExecutiveEvidence> {
            let rows = self.rows.clone();
            Box::pin(async move { rows.map(|rows| ExecutiveEvidence { rows }) })
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

    #[tokio::test]
    async fn node_references_route_uses_each_holdings_reader() {
        let router = router_with_all(vec![
            (
                target(),
                Arc::new(Stub { result: Ok(facts()) }) as Arc<dyn DirectoryHierarchyRead>,
                Arc::new(RefStub {
                    result: Ok(references("a1", 1)),
                }) as Arc<dyn NodeReferenceRead>,
                Arc::new(EvidenceStub {
                    rows: Ok(vec![serde_json::json!({"id":"a1"})]),
                }) as Arc<dyn ExecutiveEvidenceRead>,
            ),
            (
                LaneTarget {
                    host: "northstar.example.test".into(),
                    holding_slug: "northstar".into(),
                },
                Arc::new(Stub { result: Ok(facts()) }) as Arc<dyn DirectoryHierarchyRead>,
                Arc::new(RefStub {
                    result: Ok(references("n1", 7)),
                }) as Arc<dyn NodeReferenceRead>,
                Arc::new(EvidenceStub {
                    rows: Ok(vec![serde_json::json!({"id":"n1"})]),
                }) as Arc<dyn ExecutiveEvidenceRead>,
            ),
        ])
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        let get = |host: &str, slug: &str, query: &str| {
            format!(
                "GET /api/v1/holdings/{slug}/organization/node-references?{query} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
            )
        };

        let acme = http(
            addr,
            get("acme.example.test", "acme", "companyId=c1&nodeId=a1"),
        )
        .await;
        let northstar = http(
            addr,
            get("northstar.example.test", "northstar", "companyId=c1&nodeId=n1"),
        )
        .await;
        assert_eq!(acme.0, 200);
        assert_eq!(acme.2["data"]["node"]["id"], "a1");
        assert_eq!(acme.2["data"]["attachment"]["version"], 1);
        assert_eq!(northstar.0, 200);
        assert_eq!(northstar.2["data"]["node"]["id"], "n1");
        assert_eq!(northstar.2["data"]["attachment"]["version"], 7);

        let crossed = http(addr, get("acme.example.test", "northstar", "companyId=c1")).await;
        assert_eq!(crossed.0, 404);
        let bad_scope = http(addr, get("acme.example.test", "acme", "companyId=bad%20id")).await;
        assert_eq!(bad_scope.0, 422);
        let _ = server.abort();
    }

    #[tokio::test]
    async fn two_targets_route_to_distinct_readers_and_pools() {
        // Stubs stand in for two independent Pg pools. `router_for_holdings`
        // supplies one `PgDirectoryHierarchyRead` per pool through this same
        // reader-injection path, keyed by (host, holding_slug).
        let mut acme = facts();
        acme.nodes[0].slug = "acme".into();
        let mut northstar = facts();
        northstar.nodes[0].id = "n2".into();
        northstar.nodes[0].slug = "northstar".into();
        northstar.levels[0].id = "l2".into();
        northstar.positions[0].id = "p2".into();

        let first: Arc<dyn DirectoryHierarchyRead> = Arc::new(Stub { result: Ok(acme) });
        let second: Arc<dyn DirectoryHierarchyRead> = Arc::new(Stub {
            result: Ok(northstar),
        });
        let router = router_with_readers(vec![
            (
                LaneTarget {
                    host: "acme.example.test".into(),
                    holding_slug: "acme".into(),
                },
                first,
            ),
            (
                LaneTarget {
                    host: "northstar.example.test".into(),
                    holding_slug: "northstar".into(),
                },
                second,
            ),
        ])
        .unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        let get = |host: &str, slug: &str| {
            format!(
                "GET /api/v1/holdings/{slug}/organization/directory-hierarchy HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
            )
        };

        let acme_reply = http(addr, get("acme.example.test", "acme")).await;
        let northstar_reply = http(addr, get("northstar.example.test", "northstar")).await;
        assert_eq!(acme_reply.0, 200);
        assert_eq!(northstar_reply.0, 200);
        assert_eq!(acme_reply.2["data"]["nodes"][0]["slug"], "acme");
        assert_eq!(northstar_reply.2["data"]["nodes"][0]["slug"], "northstar");
        assert_eq!(acme_reply.2["data"]["nodes"][0]["id"], "n1");
        assert_eq!(northstar_reply.2["data"]["nodes"][0]["id"], "n2");

        // A Holding cannot use another Holding's host/slug pair.
        let crossed = http(addr, get("acme.example.test", "northstar")).await;
        assert_eq!(crossed.0, 404);
        let _ = server.abort();
    }

    #[tokio::test]
    async fn executive_evidence_route_uses_each_holdings_reader() {
        let router = router_with_all(vec![(
            target(),
            Arc::new(Stub { result: Ok(facts()) }) as Arc<dyn DirectoryHierarchyRead>,
            Arc::new(RefStub {
                result: Ok(references("a1", 1)),
            }) as Arc<dyn NodeReferenceRead>,
            Arc::new(EvidenceStub {
                rows: Ok(vec![serde_json::json!({"id":"a1"})]),
            }) as Arc<dyn ExecutiveEvidenceRead>,
        )])
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        let body = r#"{"kind":"node-evidence","companyId":"c1","nodeId":"a1"}"#;
        let post = format!(
            "POST /api/v1/holdings/acme/organization/executive-evidence HTTP/1.1\r\nHost: acme.example.test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body,
        );
        let reply = http(addr, post).await;
        assert_eq!(reply.0, 200);
        assert_eq!(reply.2["data"]["rows"][0]["id"], "a1");

        let bad_body = r#"{"kind":"unknown","companyId":"c1"}"#;
        let bad_kind = format!(
            "POST /api/v1/holdings/acme/organization/executive-evidence HTTP/1.1\r\nHost: acme.example.test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            bad_body.len(),
            bad_body,
        );
        assert_eq!(http(addr, bad_kind).await.0, 422);
        let _ = server.abort();
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
