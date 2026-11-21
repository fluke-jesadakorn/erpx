#[allow(
    dead_code,
    reason = "actual Account/SSO/HTTP fixtures shared by MFA and personal security tests"
)]
#[path = "rust-account-device-http.rs"]
pub mod http;
use erpx_server_identity_access::public::{account_security::*, *};
pub use http::revocation as fixture;
use sqlx::Row;
use std::sync::Arc;

pub struct Services {
    pub keys: Arc<AccountMfaKeyring>,
    pub service: Arc<AccountSecurity>,
}
pub fn services(system: &fixture::System) -> Services {
    services_with_keys(
        system,
        Arc::new(
            AccountMfaKeyring::new("current".into(), vec![("current".into(), [7; 32])]).unwrap(),
        ),
    )
}
pub fn services_with_keys(system: &fixture::System, keys: Arc<AccountMfaKeyring>) -> Services {
    let environment = "development".to_string();
    let mfa = PostgresAccountMfa::new(
        system.control.clone(),
        environment.clone(),
        keys.clone(),
        system.scenario.clock.clone(),
        fixture::fixture::account::holding_registry(),
    )
    .unwrap();
    let security = PostgresAccountSecurity::new(
        system.control.clone(),
        environment,
        system.scenario.clock.clone(),
        fixture::invalidator(&system.holdings, system.scenario.clock.clone()),
        keys.clone(),
        fixture::fixture::account::holding_registry(),
    )
    .unwrap();
    Services {
        keys,
        service: Arc::new(
            AccountSecurity::new(
                fixture::fixture::account::policy(),
                Arc::new(mfa),
                Arc::new(security),
                Arc::new(SystemAuthenticationSecrets),
            )
            .unwrap(),
        ),
    }
}
pub async fn code(
    system: &fixture::System,
    keys: &AccountMfaKeyring,
    user: &str,
    offset: i64,
) -> String {
    use erpx_server_shared::public::Clock;
    let envelope: String = sqlx::query_scalar("SELECT secret FROM iam_mfa WHERE user_id=$1")
        .bind(user)
        .fetch_one(system.control.raw())
        .await
        .unwrap();
    keys.open(user, &envelope)
        .unwrap()
        .code(system.scenario.clock.now().get() / 30_000 + offset)
        .unwrap()
}
pub async fn enabled(system: &fixture::System) -> (bool, i64, i64) {
    let row=sqlx::query("SELECT m.enabled,m.last_counter,(SELECT count(*) FROM iam_mfa_sessions WHERE user_id=m.user_id) verifications FROM iam_mfa m WHERE user_id='fixture-a'").fetch_one(system.control.raw()).await.unwrap();
    (
        row.get("enabled"),
        row.get("last_counter"),
        row.get("verifications"),
    )
}
pub async fn verified(system: &fixture::System, services: &Services, proof: &PresentedSecrets) {
    services
        .service
        .enroll(proof, RequestAudience::Account)
        .await
        .unwrap();
    let code = code(system, &services.keys, "fixture-a", 0).await;
    services
        .service
        .verify(proof, RequestAudience::Account, code)
        .await
        .unwrap();
}
pub fn input(seed: &SecuritySeed, family: Option<String>) -> RevokeSecurity {
    RevokeSecurity {
        account_key: seed.account_key.clone(),
        expected_revision: seed.revision.clone(),
        scope: if family.is_some() { "family" } else { "all" }.into(),
        family,
        reason: "Synthetic security review".into(),
    }
}
pub fn jar(proof: &PresentedSecrets) -> http::Jar {
    let mut jar = http::Jar::default();
    if let Some(browser) = &proof.browser {
        jar.0.insert(http::GROUP.into(), browser.expose().into());
    }
    if let Some(active) = &proof.active {
        jar.0.insert(http::ACTIVE.into(), active.expose().into());
    }
    for (handle, value) in &proof.saved {
        jar.0
            .insert(format!("{}{handle}", http::SAVED), value.expose().into());
    }
    jar
}
