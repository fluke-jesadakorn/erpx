//! HTTP parity register for the native production accounting-profiles read.
//! The owner executes the real SELECT; this test pins the returned payload to
//! the shape the retired TypeScript operation produced and proves the SQL is
//! the frozen `RD_ACCOUNTING_PROFILES` plan with only the bind marker changed.
//!
//! Lead integration needs one owner Cargo `[[test]]` entry for this file
//! (`autotests = false` in `modules/manufacturing/server/Cargo.toml`).

use erpx_manufacturing_server::public::native_command::PLAN_ACCOUNTING_PROFILES;
use erpx_manufacturing_server::public::production_profiles::{
    ACCOUNTING_PROFILES_SQL, AccountingProfileRow, AccountingProfilesRead,
    AccountingProfilesReadFuture, ProductionProfilesTarget, router_with_reader,
};
use erpx_platform_runtime_server::public::http_support::serve;
use std::sync::Arc;

#[derive(Clone)]
struct Probe;

impl AccountingProfilesRead for Probe {
    fn profiles<'a>(
        &'a self,
        company_id: &'a str,
    ) -> AccountingProfilesReadFuture<'a, Vec<AccountingProfileRow>> {
        let company = company_id.to_owned();
        Box::pin(async move {
            Ok(vec![AccountingProfileRow {
                id: format!("profile-{company}"),
                status: "approved".into(),
                version: 7,
                currency: "THB".into(),
                chart_id: "chart-1".into(),
                inventory_account_id: "inv".into(),
                wip_account_id: "wip".into(),
                labor_account_id: "labor".into(),
                overhead_account_id: "overhead".into(),
            }])
        })
    }
}

#[test]
fn owner_sql_is_the_frozen_read_plan() {
    assert_eq!(ACCOUNTING_PROFILES_SQL, PLAN_ACCOUNTING_PROFILES.sql.replace('?', "$1"));
    assert_eq!(ACCOUNTING_PROFILES_SQL.matches("$1").count(), 1);
}

async fn get(path: &str, host: &str, company: &str) -> (u16, serde_json::Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let router = router_with_reader(
        Arc::new(Probe),
        vec![ProductionProfilesTarget {
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
        "GET /api/v1/holdings/acme/production/{path} HTTP/1.1\r\n\
         Host: {host}\r\nAccept: application/json\r\nConnection: close\r\n\
         x-erpx-context-company: {company}\r\n\r\n"
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
    let (status, body) = get("accounting-profiles", "acme.example.test", "company").await;
    assert_eq!(status, 200);
    let row = &body["data"][0];
    assert_eq!(row["id"], "profile-company");
    assert_eq!(row["status"], "approved");
    assert_eq!(row["version"], 7);
    assert_eq!(row["currency"], "THB");
    assert_eq!(row["chart_id"], "chart-1");
    assert_eq!(row["inventory_account_id"], "inv");
    assert_eq!(row["wip_account_id"], "wip");
    assert_eq!(row["labor_account_id"], "labor");
    assert_eq!(row["overhead_account_id"], "overhead");
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn unregistered_host_is_not_served() {
    let (status, body) = get("accounting-profiles", "attacker.example.test", "company").await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}
