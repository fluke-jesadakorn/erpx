use erpx_server_identity_access::public::PresentedSecrets;
use erpx_server_runtime::public::postgres::DatabasePool;
use sqlx::{ConnectOptions, postgres::PgConnectOptions};
use std::str::FromStr;
pub async fn database(pool: &DatabasePool) -> serde_json::Value {
    let name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(pool.raw())
        .await
        .unwrap();
    let token = name.strip_prefix("erpx_dev_rust_").unwrap();
    let mut url = PgConnectOptions::from_str(&std::env::var("ERPX_TEST_ADMIN_URL").unwrap())
        .unwrap()
        .database(&name)
        .to_url_lossy();
    url.set_query(None);
    serde_json::json!({"url":url.as_str(),"leaseToken":token})
}
pub fn cookie(proof: &PresentedSecrets) -> String {
    let mut values = Vec::new();
    if let Some(browser) = &proof.browser {
        values.push(format!("__Host-erpx-device={}", browser.expose()))
    }
    if let Some(active) = &proof.active {
        values.push(format!(
            "__Secure-erpx-account.session_token={}",
            active.expose()
        ))
    }
    for (handle, secret) in &proof.saved {
        values.push(format!(
            "__Secure-erpx-account.session_token_multi-{handle}={}",
            secret.expose()
        ))
    }
    values.join("; ")
}

pub async fn document(control: &DatabasePool, holdings: &[DatabasePool; 2]) -> serde_json::Value {
    serde_json::json!({"version":1,"mode":"development","transport":"direct-loopback","listen":"127.0.0.1:43117","accountHost":"account.example.test","requireEmailVerification":true,"passwordWorkers":1,"maximumRequests":8,"requestTimeoutMs":5000,"control":database(control).await,"holdings":[{"holdingId":"h001","slug":"acme","host":"acme.example.test","writeBinding":"HOLDING_H001_DB","readBinding":"HOLDING_H001_READ_DB","filesBinding":"HOLDING_H001_FILES","database":database(&holdings[0]).await},{"holdingId":"h002","slug":"northstar","host":"northstar.example.test","writeBinding":"HOLDING_H002_DB","readBinding":"HOLDING_H002_READ_DB","filesBinding":"HOLDING_H002_FILES","database":database(&holdings[1]).await}]})
}
