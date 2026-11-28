#[allow(
    dead_code,
    reason = "shared real native Account/SSO fixture serves several tests"
)]
#[path = "../rust-account-revocation.rs"]
pub mod account;
#[path = "private-session-config.rs"]
mod config;
pub fn cookie(proof: &erpx_identity_access_server::public::PresentedSecrets) -> String {
    config::cookie(proof)
}
#[allow(
    dead_code,
    reason = "direct database lease helper is used by sibling fixtures"
)]
pub async fn database(
    pool: &erpx_platform_runtime_server::public::postgres::DatabasePool,
) -> serde_json::Value {
    config::database(pool).await
}
pub async fn document(system: &account::System) -> serde_json::Value {
    config::document(&system.control, &system.holdings).await
}
