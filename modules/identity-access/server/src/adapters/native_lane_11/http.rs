//! Owner-local axum router for the lane 11 person-identity reads.
//!
//! The route is keyed by an exact registered Host/Holding pair, accepts only a
//! bounded Company scope header and a bounded id list, and never trusts a
//! caller-supplied row, SQL or predicate. Missing configuration, malformed
//! input and persistence faults fail closed. No fallback read exists in TS.

use crate::adapters::native_lane_11::{
    application::{
        load_directory_config, load_identity_ids, load_principals, load_role, load_role_ids,
        load_roles,
    },
    domain::{
        DirectoryConfigRequest, IdentityIdsRequest, PrincipalsRequest, RolesRequest,
        configuration_invalid, invalid, missing, unavailable, valid_company, valid_slug,
    },
    postgres::PgPersonIdentityQuery,
    PersonIdentityQuery,
};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, post, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const MAX_BODY_BYTES: usize = 64 * 1024;

/// Exact registered Holding target. Composition supplies these; the router
/// never infers a target from an untrusted path or header.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn PersonIdentityQuery>>,
}

/// Validate and map each `(host, holding_slug)` to its own reader. This is the
/// single state construction shared by every constructor below.
fn configured_targets(
    entries: impl IntoIterator<Item = (LaneTarget, Arc<dyn PersonIdentityQuery>)>,
) -> Result<BTreeMap<(String, String), Arc<dyn PersonIdentityQuery>>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in entries {
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
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(configured)
}

fn routes(configured: BTreeMap<(String, String), Arc<dyn PersonIdentityQuery>>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/identity-access/person/identity-ids",
            post(identity_ids),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/identity-access/person/principals",
            post(principals),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/identity-access/directory/{kind}",
            post(directory_config),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/identity-access/role/{id}",
            post(role),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/identity-access/roles",
            post(roles),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/identity-access/role-ids",
            post(role_ids),
        )
        .with_state(Arc::new(LaneState {
            targets: configured,
        }))
}

/// Reader-injection core: one distinct reader per registered Holding. Tests use
/// this path to prove two targets reach two independent readers without a DB.
pub fn router_with_readers(
    entries: Vec<(LaneTarget, Arc<dyn PersonIdentityQuery>)>,
) -> Result<Router, BusinessError> {
    Ok(routes(configured_targets(entries)?))
}

/// Build the router over one shared injected reader. Retained for tests and for
/// callers that serve a single Holding from one pool.
pub fn router_with_reader(
    reader: Arc<dyn PersonIdentityQuery>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

/// Build the native read router with one owner Pg reader. The lead mounts it;
/// this module does not edit composition.
pub fn router(
    pool: DatabasePool,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgPersonIdentityQuery::new(pool)), targets)
}

/// Mount entry point with one Pg reader — and therefore one pool — per
/// registered Holding. `pools[i + 1]` is the i-th Holding's database, so each
/// `(host, holding_slug)` key owns its own reader and no Holding can reach
/// another Holding's data. The lead mounts this router in composition.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                let reader: Arc<dyn PersonIdentityQuery> =
                    Arc::new(PgPersonIdentityQuery::new(pool));
                (target, reader)
            })
            .collect(),
    )
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| !b.is_ascii_control() && b != b'\'' && b != b'"')
}

async fn parse_body<T: DeserializeOwned>(request: Request) -> Result<T, BusinessError> {
    let bytes = to_bytes(request.into_body(), MAX_BODY_BYTES)
        .await
        .map_err(|_| invalid())?;
    serde_json::from_slice(&bytes).map_err(|_| invalid())
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

async fn identity_ids(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let (host, company) = {
            let headers = request.headers();
            let host = unique_header(headers, header::HOST)?
                .filter(|host| !host.is_empty())
                .ok_or_else(missing)?
                .to_owned();
            let company = unique_header(headers, CONTEXT_COMPANY)?
                .filter(|value| valid_company(value))
                .ok_or_else(invalid)?
                .to_owned();
            (host, company)
        };
        let reader = state
            .targets
            .get(&(host, slug))
            .ok_or_else(missing)?
            .clone();
        let body: IdentityIdsRequest = parse_body(request).await?;
        if body.ids.iter().any(|id| !valid_id(id)) {
            return Err(invalid());
        }
        let result = load_identity_ids(reader.as_ref(), &company, &body.ids).await?;
        serde_json::to_value(result).map_err(|_| unavailable())
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn principals(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let (host, company) = {
            let headers = request.headers();
            let host = unique_header(headers, header::HOST)?
                .filter(|host| !host.is_empty())
                .ok_or_else(missing)?
                .to_owned();
            let company = unique_header(headers, CONTEXT_COMPANY)?
                .filter(|value| valid_company(value))
                .ok_or_else(invalid)?
                .to_owned();
            (host, company)
        };
        let reader = state
            .targets
            .get(&(host, slug))
            .ok_or_else(missing)?
            .clone();
        let body: PrincipalsRequest = parse_body(request).await?;
        if body.ids.iter().any(|id| !valid_id(id)) {
            return Err(invalid());
        }
        let result = load_principals(reader.as_ref(), &company, &body.ids, body.snapshot).await?;
        serde_json::to_value(result).map_err(|_| unavailable())
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn directory_config(
    State(state): State<Arc<LaneState>>,
    Path((slug, kind)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async {
        let (host, company) = {
            let headers = request.headers();
            let host = unique_header(headers, header::HOST)?
                .filter(|host| !host.is_empty())
                .ok_or_else(missing)?
                .to_owned();
            let company = unique_header(headers, CONTEXT_COMPANY)?
                .filter(|value| valid_company(value))
                .ok_or_else(invalid)?
                .to_owned();
            (host, company)
        };
        let reader = state
            .targets
            .get(&(host, slug))
            .ok_or_else(missing)?
            .clone();
        let body: DirectoryConfigRequest = parse_body(request).await?;
        let result = load_directory_config(reader.as_ref(), &company, &kind, body.can_manage).await?;
        serde_json::to_value(result).map_err(|_| unavailable())
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn role(
    State(state): State<Arc<LaneState>>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let (host, company) = {
            let host = unique_header(&headers, header::HOST)?
                .filter(|host| !host.is_empty())
                .ok_or_else(missing)?
                .to_owned();
            let company = unique_header(&headers, CONTEXT_COMPANY)?
                .filter(|value| valid_company(value))
                .ok_or_else(invalid)?
                .to_owned();
            (host, company)
        };
        let reader = state
            .targets
            .get(&(host, slug))
            .ok_or_else(missing)?
            .clone();
        if !valid_id(&id) {
            return Err(invalid());
        }
        let result = load_role(reader.as_ref(), &company, &id).await?;
        serde_json::to_value(result).map_err(|_| unavailable())
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn roles(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let (host, company) = {
            let headers = request.headers();
            let host = unique_header(headers, header::HOST)?
                .filter(|host| !host.is_empty())
                .ok_or_else(missing)?
                .to_owned();
            let company = unique_header(headers, CONTEXT_COMPANY)?
                .filter(|value| valid_company(value))
                .ok_or_else(invalid)?
                .to_owned();
            (host, company)
        };
        let reader = state
            .targets
            .get(&(host, slug))
            .ok_or_else(missing)?
            .clone();
        let body: RolesRequest = parse_body(request).await?;
        if body.ids.iter().any(|id| !valid_id(id)) {
            return Err(invalid());
        }
        let result = load_roles(reader.as_ref(), &company, &body.ids).await?;
        serde_json::to_value(result).map_err(|_| unavailable())
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn role_ids(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let (host, company) = {
            let host = unique_header(&headers, header::HOST)?
                .filter(|host| !host.is_empty())
                .ok_or_else(missing)?
                .to_owned();
            let company = unique_header(&headers, CONTEXT_COMPANY)?
                .filter(|value| valid_company(value))
                .ok_or_else(invalid)?
                .to_owned();
            (host, company)
        };
        let reader = state
            .targets
            .get(&(host, slug))
            .ok_or_else(missing)?
            .clone();
        let result = load_role_ids(reader.as_ref(), &company).await?;
        serde_json::to_value(result).map_err(|_| unavailable())
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
    use crate::adapters::native_lane_11::{
        application::PersonIdentityFuture,
        domain::{RoleRevision, SuccessorRow, unavailable},
    };
    use std::sync::Mutex;

    #[derive(Default)]
    struct Captured {
        company: Vec<String>,
        ids: Vec<Option<Vec<String>>>,
    }

    struct Probe {
        captured: Arc<Mutex<Captured>>,
        aliases: Vec<String>,
    }

    impl PersonIdentityQuery for Probe {
        fn aliases<'a>(
            &'a self,
            company: &'a str,
            ids: &'a [String],
        ) -> PersonIdentityFuture<'a, Vec<String>> {
            let captured = self.captured.clone();
            let company = company.to_owned();
            let ids = ids.to_vec();
            let aliases = self.aliases.clone();
            Box::pin(async move {
                let mut guard = captured.lock().unwrap();
                guard.company.push(company);
                guard.ids.push(Some(ids));
                Ok(aliases)
            })
        }

        fn successors<'a>(
            &'a self,
            company: &'a str,
            ids: Option<&'a [String]>,
        ) -> PersonIdentityFuture<'a, Vec<SuccessorRow>> {
            let captured = self.captured.clone();
            let company = company.to_owned();
            let ids = ids.map(<[String]>::to_vec);
            Box::pin(async move {
                let mut guard = captured.lock().unwrap();
                guard.company.push(company);
                guard.ids.push(ids);
                Ok(vec![SuccessorRow {
                    original_id: "old".into(),
                    current_id: "new".into(),
                }])
            })
        }

        fn directory_rows<'a>(
            &'a self,
            company: &'a str,
            kind: &'a str,
            can_manage: bool,
        ) -> PersonIdentityFuture<'a, Vec<serde_json::Value>> {
            let captured = self.captured.clone();
            let company = company.to_owned();
            let kind = kind.to_owned();
            Box::pin(async move {
                let mut guard = captured.lock().unwrap();
                guard.company.push(company);
                guard.ids.push(None);
                let projection = if can_manage { "full" } else { "restricted" };
                Ok(vec![serde_json::json!({
                    "id": format!("{kind}-{projection}"),
                    "name": "Directory",
                })])
            })
        }

        fn role_one<'a>(
            &'a self,
            _company: &'a str,
            id: &'a str,
        ) -> PersonIdentityFuture<'a, Option<RoleRevision>> {
            let role = probe_role(id);
            Box::pin(async move { Ok(role) })
        }

        fn role_list<'a>(
            &'a self,
            _company: &'a str,
            ids: &'a [String],
        ) -> PersonIdentityFuture<'a, Vec<RoleRevision>> {
            let roles = ids.iter().filter_map(|id| probe_role(id)).collect();
            Box::pin(async move { Ok(roles) })
        }

        fn role_ids<'a>(&'a self, _company: &'a str) -> PersonIdentityFuture<'a, Vec<String>> {
            Box::pin(async { Ok(vec!["employee".into()]) })
        }
    }

    fn probe_role(id: &str) -> Option<RoleRevision> {
        (id == "employee").then(|| RoleRevision {
            id: id.to_owned(),
            name: "Employee".into(),
            company_id: "acme:co".into(),
            revision: 2,
            built_in: true,
            permissions: vec!["iam.read".into()],
            grantable_permissions: vec![],
            scopes: vec!["holding".into()],
            status: "active".into(),
        })
    }

    struct Failing;

    impl PersonIdentityQuery for Failing {
        fn aliases<'a>(
            &'a self,
            _company: &'a str,
            _ids: &'a [String],
        ) -> PersonIdentityFuture<'a, Vec<String>> {
            Box::pin(async { Err(unavailable()) })
        }

        fn successors<'a>(
            &'a self,
            _company: &'a str,
            _ids: Option<&'a [String]>,
        ) -> PersonIdentityFuture<'a, Vec<SuccessorRow>> {
            Box::pin(async { Err(unavailable()) })
        }

        fn directory_rows<'a>(
            &'a self,
            _company: &'a str,
            _kind: &'a str,
            _can_manage: bool,
        ) -> PersonIdentityFuture<'a, Vec<serde_json::Value>> {
            Box::pin(async { Err(unavailable()) })
        }

        fn role_one<'a>(
            &'a self,
            _company: &'a str,
            _id: &'a str,
        ) -> PersonIdentityFuture<'a, Option<RoleRevision>> {
            Box::pin(async { Err(unavailable()) })
        }

        fn role_list<'a>(
            &'a self,
            _company: &'a str,
            _ids: &'a [String],
        ) -> PersonIdentityFuture<'a, Vec<RoleRevision>> {
            Box::pin(async { Err(unavailable()) })
        }

        fn role_ids<'a>(&'a self, _company: &'a str) -> PersonIdentityFuture<'a, Vec<String>> {
            Box::pin(async { Err(unavailable()) })
        }
    }

    fn target() -> LaneTarget {
        LaneTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
        }
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

    async fn serve(reader: Arc<dyn PersonIdentityQuery>) -> (u16, tokio::task::JoinHandle<()>) {
        let router = router_with_reader(reader, vec![target()]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        (addr.port(), server)
    }

    #[test]
    fn public_surface_reexports_the_constructors() {
        // `src/public/native_lane_11.rs` re-exports this module with `*`; the
        // per-Holding mount must therefore be reachable from the public surface.
        let _ = crate::public::native_lane_11::router_for_holdings;
        let _ = crate::public::native_lane_11::router_with_readers;
        let _ = crate::public::native_lane_11::router_with_reader;
        let _ = crate::public::native_lane_11::router;
    }

    #[test]
    fn configuration_requires_exact_distinct_holdings() {
        let probe = Arc::new(Probe {
            captured: Arc::new(Mutex::new(Captured::default())),
            aliases: vec!["alias".into()],
        });
        assert!(router_with_reader(probe.clone(), vec![target()]).is_ok());
        assert!(router_with_reader(probe.clone(), vec![target(), target()]).is_err());
        assert!(router_with_reader(probe.clone(), vec![]).is_err());
        assert!(router_with_reader(
            probe,
            vec![LaneTarget {
                holding_slug: "BAD".into(),
                ..target()
            }]
        )
        .is_err());
        // The per-Holding entry point applies the same configuration guard.
        assert!(router_for_holdings(Vec::new()).is_err());
    }

    #[tokio::test]
    async fn identity_ids_returns_exact_projection_and_scope() {
        let captured = Arc::new(Mutex::new(Captured::default()));
        let (port, server) = serve(Arc::new(Probe {
            captured: captured.clone(),
            aliases: vec!["alias".into()],
        }))
        .await;
        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let body = r#"{"ids":["user"]}"#;
        let response = http(
            addr,
            format!(
                "POST /api/v1/holdings/acme/identity-access/person/identity-ids HTTP/1.1\r\nHost: acme.example.test\r\nx-erpx-context-company: acme:co\r\ncontent-type: application/json\r\ncontent-length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            ),
        )
        .await;
        assert_eq!(response.0, 200);
        assert_eq!(response.1, serde_json::json!({"identityIds":["user","alias"]}));
        let guard = captured.lock().unwrap();
        assert_eq!(guard.company, vec!["acme:co".to_owned()]);
        assert_eq!(guard.ids, vec![Some(vec!["user".to_owned()])]);
        drop(guard);
        server.abort();
    }

    #[tokio::test]
    async fn principals_returns_exact_projection_and_snapshot_flag() {
        let captured = Arc::new(Mutex::new(Captured::default()));
        let (port, server) = serve(Arc::new(Probe {
            captured: captured.clone(),
            aliases: vec!["alias".into()],
        }))
        .await;
        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let body = r#"{"ids":["old"],"snapshot":false}"#;
        let response = http(
            addr,
            format!(
                "POST /api/v1/holdings/acme/identity-access/person/principals HTTP/1.1\r\nHost: acme.example.test\r\nx-erpx-context-company: acme:co\r\ncontent-type: application/json\r\ncontent-length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            ),
        )
        .await;
        assert_eq!(response.0, 200);
        assert_eq!(
            response.1,
            serde_json::json!({"principals":[{"original":"old","current":"new"}]})
        );
        let guard = captured.lock().unwrap();
        assert_eq!(guard.ids, vec![Some(vec!["old".to_owned()])]);
        drop(guard);
        server.abort();
    }

    #[tokio::test]
    async fn directory_config_returns_owner_rows_and_rejects_unknown_kind() {
        let captured = Arc::new(Mutex::new(Captured::default()));
        let (port, server) = serve(Arc::new(Probe {
            captured: captured.clone(),
            aliases: vec!["alias".into()],
        }))
        .await;
        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let post = |kind: &str, payload: &str| {
            format!(
                "POST /api/v1/holdings/acme/identity-access/directory/{kind} HTTP/1.1\r\nHost: acme.example.test\r\nx-erpx-context-company: acme:co\r\ncontent-type: application/json\r\ncontent-length: {}\r\nConnection: close\r\n\r\n{payload}",
                payload.len()
            )
        };
        let body = r#"{"canManage":true}"#;
        let (status, value) = http(addr, post("connections", body)).await;
        assert_eq!(status, 200);
        assert_eq!(
            value,
            serde_json::json!({"rows":[{"id":"connections-full","name":"Directory"}]})
        );
        let (status, _) = http(addr, post("not-a-kind", body)).await;
        assert_eq!(status, 422);
        let guard = captured.lock().unwrap();
        // The unknown-kind request is rejected before reaching the owner reader.
        assert_eq!(guard.company, vec!["acme:co".to_owned()]);
        assert_eq!(guard.ids, vec![None]);
        drop(guard);
        server.abort();
    }

    #[tokio::test]
    async fn role_reads_serve_the_exact_published_projection() {
        let (port, server) = serve(Arc::new(Probe {
            captured: Arc::new(Mutex::new(Captured::default())),
            aliases: vec![],
        }))
        .await;
        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let post = |path: &str, payload: &str| {
            format!(
                "POST /api/v1/holdings/acme/identity-access/{path} HTTP/1.1\r\nHost: acme.example.test\r\nx-erpx-context-company: acme:co\r\ncontent-type: application/json\r\ncontent-length: {}\r\nConnection: close\r\n\r\n{payload}",
                payload.len()
            )
        };
        let definition = serde_json::json!({
            "id": "employee",
            "name": "Employee",
            "companyId": "acme:co",
            "revision": 2,
            "builtIn": true,
            "permissions": ["iam.read"],
            "grantablePermissions": [],
            "scopes": ["holding"],
            "status": "active",
        });
        let (status, value) = http(addr, post("role/employee", "{}")).await;
        assert_eq!(status, 200);
        assert_eq!(value, serde_json::json!({"role": definition}));
        let (status, value) = http(addr, post("roles", r#"{"ids":["employee"]}"#)).await;
        assert_eq!(status, 200);
        assert_eq!(value, serde_json::json!({"roles": [definition]}));
        let (status, value) = http(addr, post("role-ids", "{}")).await;
        assert_eq!(status, 200);
        assert_eq!(value, serde_json::json!({"ids": ["employee"]}));
        let (status, _) = http(addr, post("roles", r#"{"ids":"#)).await;
        assert_eq!(status, 422);
        server.abort();
    }

    /// Reader injection is the testable equivalent of `router_for_holdings`:
    /// each target/pool pair becomes one independent reader in the state map.
    #[tokio::test]
    async fn per_holding_readers_are_distinct() {
        let seen_a = Arc::new(Mutex::new(Captured::default()));
        let seen_b = Arc::new(Mutex::new(Captured::default()));
        let reader_a: Arc<dyn PersonIdentityQuery> = Arc::new(Probe {
            captured: seen_a.clone(),
            aliases: vec!["acme-alias".into()],
        });
        let reader_b: Arc<dyn PersonIdentityQuery> = Arc::new(Probe {
            captured: seen_b.clone(),
            aliases: vec!["northstar-alias".into()],
        });
        let router = router_with_readers(vec![
            (
                LaneTarget {
                    host: "acme.example.test".into(),
                    holding_slug: "acme".into(),
                },
                reader_a,
            ),
            (
                LaneTarget {
                    host: "northstar.example.test".into(),
                    holding_slug: "northstar".into(),
                },
                reader_b,
            ),
        ])
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        let post = |host: &str, slug: &str, payload: &str| {
            format!(
                "POST /api/v1/holdings/{slug}/identity-access/person/identity-ids HTTP/1.1\r\nHost: {host}\r\nx-erpx-context-company: {slug}:co\r\ncontent-type: application/json\r\ncontent-length: {}\r\nConnection: close\r\n\r\n{payload}",
                payload.len()
            )
        };
        let body = r#"{"ids":["user"]}"#;
        let (status, value) = http(addr, post("acme.example.test", "acme", body)).await;
        assert_eq!(status, 200);
        assert_eq!(value, serde_json::json!({"identityIds":["user","acme-alias"]}));
        let (status, value) = http(addr, post("northstar.example.test", "northstar", body)).await;
        assert_eq!(status, 200);
        assert_eq!(value, serde_json::json!({"identityIds":["user","northstar-alias"]}));
        // A mismatched Host/slug pair cannot reach the other Holding's reader.
        let (status, _) = http(addr, post("acme.example.test", "northstar", body)).await;
        assert_eq!(status, 404);
        let guard_a = seen_a.lock().unwrap();
        let guard_b = seen_b.lock().unwrap();
        assert_eq!(guard_a.company, vec!["acme:co".to_owned()]);
        assert_eq!(guard_b.company, vec!["northstar:co".to_owned()]);
        assert_eq!(guard_a.ids.len(), 1);
        assert_eq!(guard_b.ids.len(), 1);
        drop(guard_a);
        drop(guard_b);
        server.abort();
    }

    #[tokio::test]
    async fn missing_scope_unknown_target_and_reader_fault_fail_closed() {
        let (port, server) = serve(Arc::new(Failing)).await;
        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let body = r#"{"ids":["user"]}"#;
        let request = |host: &str, company: Option<&str>, path: &str, payload: &str| {
            format!(
                "POST /api/v1/holdings/acme/identity-access/person/{path} HTTP/1.1\r\nHost: {host}\r\n{}content-type: application/json\r\ncontent-length: {}\r\nConnection: close\r\n\r\n{payload}",
                company
                    .map(|value| format!("x-erpx-context-company: {value}\r\n"))
                    .unwrap_or_default(),
                payload.len()
            )
        };
        let missing_company = http(addr, request("acme.example.test", None, "identity-ids", body)).await;
        assert_eq!(missing_company.0, 422);
        let unknown = http(addr, request("other.example.test", Some("acme:co"), "identity-ids", body)).await;
        assert_eq!(unknown.0, 404);
        let malformed = http(addr, request("acme.example.test", Some("acme:co"), "principals", r#"{"ids":"#)).await;
        assert_eq!(malformed.0, 422);
        let failed = http(addr, request("acme.example.test", Some("acme:co"), "identity-ids", body)).await;
        assert_eq!(failed.0, 503);
        server.abort();
    }
}
