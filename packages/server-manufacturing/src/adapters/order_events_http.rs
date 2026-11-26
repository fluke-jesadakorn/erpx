//! Owner-local axum ingress for the production order-events owner read.
//!
//! Target/host isolation and error mapping follow the lane-28 owner routes and
//! the native production accounting-profiles reader. The handler never accepts
//! SQL and never falls back to a TypeScript reader: the injected owner reader
//! executes the single Manufacturing-owned SELECT and returns the complete
//! immutable history projection.

use crate::adapters::order_events_postgres::PgOrderEventsRead;
use crate::application::order_events::{
    OrderEventsHandler, OrderEventsQuery, OrderEventsRead,
};
use crate::domain::order_events::validate_order;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Exact registered Holding target for this owner route.
pub struct OrderEventsTarget {
    pub host: String,
    pub holding_slug: String,
}

struct TargetHandlers {
    events: OrderEventsHandler,
}

struct OrderEventsState {
    targets: BTreeMap<(String, String), TargetHandlers>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MANUFACTURING_ORDER_EVENTS_REQUEST_INVALID",
        "Use one bounded Manufacturing order-events read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "MANUFACTURING_ORDER_EVENTS_CONFIG_INVALID",
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
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
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
            "/api/v1/holdings/{holding_slug}/production/orders/{order_id}/events",
            get(order_events),
        )
        .with_state(Arc::new(OrderEventsState { targets })))
}

fn handlers(reader: Arc<dyn OrderEventsRead>) -> TargetHandlers {
    TargetHandlers {
        events: OrderEventsHandler::new(reader),
    }
}

/// Build from one owner pool shared by all targets.
pub fn router(pool: DatabasePool, targets: Vec<OrderEventsTarget>) -> Result<Router, BusinessError> {
    router_for_holdings(targets.into_iter().map(|target| (target, pool.clone())).collect())
}

/// Build with one owner reader per target/pool, keyed by `(host, holding_slug)`.
pub fn router_for_holdings(
    targets: Vec<(OrderEventsTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, pool) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    handlers(Arc::new(PgOrderEventsRead::new(pool))),
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    assemble(configured)
}

/// Reader-injected variant used by unit tests and by a composition that already
/// owns the pooled reader instance.
pub fn router_with_reader(
    reader: Arc<dyn OrderEventsRead>,
    targets: Vec<OrderEventsTarget>,
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
    state: &'a OrderEventsState,
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

async fn order_events(
    State(state): State<Arc<OrderEventsState>>,
    Path((slug, order_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let handlers = resolve_target(&state, &slug, &headers)?;
        validate_order(&order_id).map_err(|_| invalid())?;
        let query = OrderEventsQuery::new(&order_id).map_err(|_| invalid())?;
        let rows = handlers.events.handle(query).await?;
        let body = serde_json::to_value(&rows).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "MANUFACTURING_ORDER_EVENTS_UNAVAILABLE",
                "The native Manufacturing order-events read is unavailable.",
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
    use crate::domain::order_events::OrderEventRow;
    use erpx_server_runtime::public::http_support::serve;

    #[derive(Clone)]
    struct Probe;

    impl OrderEventsRead for Probe {
        fn events<'a>(
            &'a self,
            order_id: &'a str,
        ) -> crate::application::order_events::OrderEventsReadFuture<'a, Vec<OrderEventRow>>
        {
            let valid = validate_order(order_id).is_ok();
            Box::pin(async move {
                if !valid {
                    return Ok(vec![]);
                }
                Ok(vec![OrderEventRow {
                    id: "event-1".into(),
                    order_id: order_id.to_owned(),
                    version: 2,
                    action: "issue".into(),
                    actor_id: "user-1".into(),
                    reason: "issued materials".into(),
                    status: "in_progress".into(),
                    details_json: "{}".into(),
                    created_at: 1_700_000_000_000,
                }])
            })
        }
    }

    fn target() -> OrderEventsTarget {
        OrderEventsTarget {
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

    fn request(order_id: &str, host: &str) -> String {
        format!(
            "GET /api/v1/holdings/acme/production/orders/{order_id}/events HTTP/1.1\r\n\
             Host: {host}\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
        )
    }

    #[tokio::test]
    async fn order_events_payload_parity() {
        let (port, server) = serve_probe().await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("order-1", "acme.example.test"),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        let row = &body["data"][0];
        assert_eq!(row["id"], "event-1");
        assert_eq!(row["order_id"], "order-1");
        assert_eq!(row["version"], 2);
        assert_eq!(row["action"], "issue");
        assert_eq!(row["actor_id"], "user-1");
        assert_eq!(row["reason"], "issued materials");
        assert_eq!(row["status"], "in_progress");
        assert_eq!(row["details_json"], "{}");
        assert_eq!(row["created_at"], 1_700_000_000_000i64);
        assert_eq!(body["data"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn wrong_host_fails_closed() {
        let (port, server) = serve_probe().await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("order-1", "attacker.example.test"),
        )
        .await;
        server.abort();
        assert_eq!(status, 404);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn invalid_order_id_fails_closed() {
        let (port, server) = serve_probe().await;
        let long_id = "x".repeat(201);
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request(&long_id, "acme.example.test"),
        )
        .await;
        server.abort();
        assert_eq!(status, 422);
        assert_eq!(
            body["error"]["code"],
            "MANUFACTURING_ORDER_EVENTS_REQUEST_INVALID"
        );
    }

    #[test]
    fn configuration_requires_exact_distinct_holdings() {
        let probe = Arc::new(Probe);
        assert!(router_with_reader(probe.clone(), vec![target()]).is_ok());
        assert!(router_with_reader(probe.clone(), vec![target(), target()]).is_err());
        assert!(router_with_reader(probe, vec![]).is_err());
    }
}
