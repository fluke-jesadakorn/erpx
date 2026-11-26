//! Owner-local axum ingress for the native Talent detail read.
//!
//! The handler never accepts SQL and never falls back to a TypeScript reader:
//! the injected owner reader executes the owner-authored SELECTs and returns
//! the complete detail payload. Target, host and scope isolation follow the
//! other mounted Talent owner routes.

use crate::application::native_talent_detail::{
    TalentDetailHandler, TalentDetailQuery, TalentDetailRead,
};
use crate::adapters::native_talent_detail::postgres::PgTalentDetailRead;
use crate::domain::native_talent_detail::{MAX_IDENTIFIER, invalid};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_ACTOR: HeaderName = HeaderName::from_static("x-erpx-context-actor");
const CONTEXT_BRANCH: HeaderName = HeaderName::from_static("x-erpx-context-branch");
const CONTEXT_DEPARTMENT: HeaderName = HeaderName::from_static("x-erpx-context-department");
const TALENT_DOCUMENTS: HeaderName = HeaderName::from_static("x-erpx-talent-documents");

/// Exact registered Holding target for this owner route.
pub struct TalentDetailTarget {
    pub host: String,
    pub holding_slug: String,
}

struct TargetHandlers {
    detail: TalentDetailHandler,
}

struct TalentDetailState {
    targets: BTreeMap<(String, String), TargetHandlers>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid_request() -> BusinessError {
    invalid()
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "TALENT_DETAIL_CONFIG_INVALID",
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
        return Err(invalid_request());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid_request()))
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

fn assemble(targets: BTreeMap<(String, String), TargetHandlers>) -> Result<Router, BusinessError> {
    if targets.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/talent/applications/{application_id}/detail",
            get(read_detail),
        )
        .with_state(Arc::new(TalentDetailState { targets })))
}

fn handlers(reader: Arc<dyn TalentDetailRead>) -> TargetHandlers {
    TargetHandlers {
        detail: TalentDetailHandler::new(reader),
    }
}

/// Build from one owner pool shared by all targets.
pub fn router(pool: DatabasePool, targets: Vec<TalentDetailTarget>) -> Result<Router, BusinessError> {
    router_for_holdings(targets.into_iter().map(|target| (target, pool.clone())).collect())
}

/// Build with one owner reader per target/pool, keyed by `(host, holding_slug)`.
pub fn router_for_holdings(
    targets: Vec<(TalentDetailTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, pool) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    handlers(Arc::new(PgTalentDetailRead::new(pool))),
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    assemble(configured)
}

/// Reader-injected variant used by unit tests and by a composition that already
/// owns the pooled reader instance. No TypeScript reader is ever consulted.
pub fn router_with_reader(
    reader: Arc<dyn TalentDetailRead>,
    targets: Vec<TalentDetailTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    handlers(reader.clone()),
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    assemble(configured)
}

fn resolve_target<'a>(
    state: &'a TalentDetailState,
    slug: &str,
    headers: &HeaderMap,
) -> Result<&'a TargetHandlers, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|host| !host.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn documents(value: Option<&str>) -> Result<bool, BusinessError> {
    match value {
        None | Some("0") | Some("false") => Ok(false),
        Some("1") | Some("true") => Ok(true),
        Some(_) => Err(invalid_request()),
    }
}

async fn read_detail(
    State(state): State<Arc<TalentDetailState>>,
    Path((slug, application_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let has_query = request.uri().query().is_some();
    let result = async {
        let handlers = resolve_target(&state, &slug, &headers)?;
        if has_query {
            return Err(invalid_request());
        }
        let company = unique_header(&headers, CONTEXT_COMPANY)?
            .ok_or_else(invalid_request)?;
        let actor = unique_header(&headers, CONTEXT_ACTOR)?.ok_or_else(invalid_request)?;
        let branch = unique_header(&headers, CONTEXT_BRANCH)?;
        let department = unique_header(&headers, CONTEXT_DEPARTMENT)?;
        let can_manage_documents = documents(unique_header(&headers, TALENT_DOCUMENTS)?)?;
        if company.is_empty()
            || company.len() > MAX_IDENTIFIER
            || actor.is_empty()
            || actor.len() > MAX_IDENTIFIER
        {
            return Err(invalid_request());
        }
        let query = TalentDetailQuery::new(
            &application_id,
            company,
            branch,
            department,
            actor,
            can_manage_documents,
        )?;
        let detail = handlers
            .detail
            .handle(query)
            .await?
            .ok_or_else(crate::domain::native_talent_detail::application_not_found)?;
        let body = serde_json::to_value(&detail).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "NATIVE_TALENT_DETAIL_UNAVAILABLE",
                "The native Talent detail read is unavailable.",
            )
        })?;
        Ok::<_, BusinessError>(serde_json::json!({"data": body}))
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
    use crate::application::native_talent_detail::TalentDetailReadFuture;
    use crate::domain::native_talent_detail::TalentDetail;
    use erpx_server_runtime::public::http_support::serve;
    use serde_json::json;

    #[derive(Clone)]
    struct Probe;

    impl TalentDetailRead for Probe {
        fn detail<'a>(
            &'a self,
            query: &'a TalentDetailQuery,
        ) -> TalentDetailReadFuture<'a, Option<TalentDetail>> {
            let id = query.id.clone();
            let actor = query.actor_id.clone();
            let manage = query.can_manage_documents;
            let missing = id == "app-missing";
            Box::pin(async move {
                if missing {
                    return Ok(None);
                }
                Ok(Some(TalentDetail {
                    application: json!({
                        "id": id, "status": "applied", "version": 1,
                        "job_title": "Engineer", "branch_id": "branch-1",
                        "candidate_name": "Ada", "candidate_email": "ada@example.test",
                        "owner_eligible": 1
                    }),
                    history: vec![json!({"version": 1, "action": "create", "status": "applied"})],
                    proposal: json!({"id": "proposal-1", "recommendation": "hire"}),
                    offer: json!({"id": "offer-1", "status": "draft"}),
                    handoff: json!({"id": "handoff-1", "offer_id": "offer-1"}),
                    actor_id: actor,
                    can_review: true,
                    can_manage_documents: manage,
                }))
            })
        }
    }

    fn target() -> TalentDetailTarget {
        TalentDetailTarget {
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
        (status, serde_json::from_str(body).unwrap_or(serde_json::Value::Null))
    }

    async fn serve_probe() -> (u16, tokio::task::JoinHandle<()>) {
        let router = router_with_reader(Arc::new(Probe), vec![target()]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { let _ = serve(listener, router).await; });
        (addr.port(), server)
    }

    fn request(host: &str, id: &str, extra: &str) -> String {
        format!(
            "GET /api/v1/holdings/acme/talent/applications/{id}/detail HTTP/1.1\r\n\
             Host: {host}\r\nAccept: application/json\r\nConnection: close\r\n\
             x-erpx-context-company: company-1\r\n\
             x-erpx-context-actor: user-1\r\n\
             x-erpx-context-branch: branch-1\r\n\
             x-erpx-talent-documents: 1\r\n{extra}\r\n"
        )
    }

    #[tokio::test]
    async fn detail_payload_matches_every_owner_field() {
        let (port, server) = serve_probe().await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("acme.example.test", "app-1", ""),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        let data = &body["data"];
        assert_eq!(data["application"]["id"], "app-1");
        assert_eq!(data["application"]["job_title"], "Engineer");
        assert_eq!(data["application"]["owner_eligible"], 1);
        assert_eq!(data["history"][0]["version"], 1);
        assert_eq!(data["proposal"]["id"], "proposal-1");
        assert_eq!(data["offer"]["id"], "offer-1");
        assert_eq!(data["handoff"]["offer_id"], "offer-1");
        assert_eq!(data["actorId"], "user-1");
        assert_eq!(data["canReview"], true);
        assert_eq!(data["canManageDocuments"], true);
    }

    #[tokio::test]
    async fn wrong_host_fails_closed() {
        let (port, server) = serve_probe().await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("attacker.example.test", "app-1", ""),
        )
        .await;
        server.abort();
        assert_eq!(status, 404);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn missing_application_fails_closed() {
        let (port, server) = serve_probe().await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("acme.example.test", "app-missing", ""),
        )
        .await;
        server.abort();
        assert_eq!(status, 404);
        assert_eq!(body["error"]["code"], "TALENT_APPLICATION_NOT_FOUND");
    }

    #[test]
    fn configuration_requires_exact_distinct_holdings() {
        let probe = Arc::new(Probe);
        assert!(router_with_reader(probe.clone(), vec![target()]).is_ok());
        assert!(router_with_reader(probe.clone(), vec![target(), target()]).is_err());
        assert!(router_with_reader(probe, vec![]).is_err());
    }
}
