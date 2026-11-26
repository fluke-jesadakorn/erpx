//! HTTP parity register for the native production order-events read.
//! The owner executes the real SELECT; this test pins the returned payload to
//! the shape the retired TypeScript operation produced and proves the SQL is
//! the frozen `RD_ORDER_EVENTS` plan with only the bind marker changed.
//!
//! Lead integration needs one owner Cargo `[[test]]` entry for this file
//! (`autotests = false` in `packages/server-manufacturing/Cargo.toml`).

use erpx_server_manufacturing::public::native_command::PLAN_ORDER_EVENTS;
use erpx_server_manufacturing::public::order_events::{
    ORDER_EVENTS_SQL, OrderEventRow, OrderEventsRead, OrderEventsReadFuture, OrderEventsTarget,
    router_with_reader,
};
use erpx_server_runtime::public::http_support::serve;
use std::sync::Arc;

#[derive(Clone)]
struct Probe;

impl OrderEventsRead for Probe {
    fn events<'a>(&'a self, order_id: &'a str) -> OrderEventsReadFuture<'a, Vec<OrderEventRow>> {
        let id = order_id.to_owned();
        Box::pin(async move {
            Ok(vec![OrderEventRow {
                id: "event-1".into(),
                order_id: id,
                version: 7,
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

#[test]
fn owner_sql_is_the_frozen_read_plan() {
    assert_eq!(
        ORDER_EVENTS_SQL,
        PLAN_ORDER_EVENTS.sql.replace('?', "$1")
    );
    assert_eq!(ORDER_EVENTS_SQL.matches("$1").count(), 1);
}

async fn get(order_id: &str, host: &str) -> (u16, serde_json::Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let router = router_with_reader(
        Arc::new(Probe),
        vec![OrderEventsTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
        }],
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { let _ = serve(listener, router).await; });
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let raw = format!(
        "GET /api/v1/holdings/acme/production/orders/{order_id}/events HTTP/1.1\r\n\
         Host: {host}\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(raw.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    server.abort();
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

#[tokio::test]
async fn native_route_returns_the_complete_owner_payload() {
    let (status, body) = get("order-1", "acme.example.test").await;
    assert_eq!(status, 200);
    let row = &body["data"][0];
    assert_eq!(row["id"], "event-1");
    assert_eq!(row["order_id"], "order-1");
    assert_eq!(row["version"], 7);
    assert_eq!(row["action"], "issue");
    assert_eq!(row["actor_id"], "user-1");
    assert_eq!(row["reason"], "issued materials");
    assert_eq!(row["status"], "in_progress");
    assert_eq!(row["details_json"], "{}");
    assert_eq!(row["created_at"], 1_700_000_000_000i64);
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn unregistered_host_is_not_served() {
    let (status, body) = get("order-1", "attacker.example.test").await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}

#[tokio::test]
async fn oversized_order_id_is_not_served() {
    let long_id = "x".repeat(201);
    let (status, body) = get(&long_id, "acme.example.test").await;
    assert_eq!(status, 422);
    assert_eq!(
        body["error"]["code"],
        "MANUFACTURING_ORDER_EVENTS_REQUEST_INVALID"
    );
}
