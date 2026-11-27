use erpx_server_commercial::public::company_plugins::{
    PluginPin, install_company_plugin, list_company_plugins, verify_plugin_version,
    withdraw_company_plugin,
};
use erpx_server_shared::public::{CompanyId, UserId};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

#[tokio::test]
async fn withdrawal_and_retry_respect_the_installation_version_and_company() {
    let Ok(uri) = std::env::var("ERPX_TEST_OASIS_PLUGIN_DATABASE") else {
        return;
    };
    let pool = PgPool::connect(&uri).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(database, "erpx_stage_oasis_demo_h002");

    let files = include_bytes!("../../../contracts/plugins/site-measurement.example.json");
    let pin = PluginPin {
        id: "site-measurement".into(),
        version: "1.0.0".into(),
        repository_id: 42,
        repository: "example/test-plugin".into(),
        commit: "a".repeat(40),
        sha256: hex::encode(Sha256::digest(files)),
    };
    let version = verify_plugin_version(pin, 42, files).unwrap();
    let company_a = CompanyId::new("plugin-test-a").unwrap();
    let company_b = CompanyId::new("plugin-test-b").unwrap();
    let actor = UserId::new("plugin-test-admin").unwrap();
    let a = install_company_plugin(&mut tx, &company_a, &actor, &version, 100)
        .await
        .unwrap();
    let b = install_company_plugin(&mut tx, &company_b, &actor, &version, 100)
        .await
        .unwrap();
    assert_eq!(a.installation_version, 1);
    assert_eq!(b.installation_version, 1);
    assert_eq!(
        install_company_plugin(&mut tx, &company_a, &actor, &version, 120)
            .await
            .unwrap()
            .installation_version,
        1
    );
    withdraw_company_plugin(&mut tx, &company_a, &actor, &a.id, 1, 150)
        .await
        .unwrap();
    assert!(
        list_company_plugins(&mut tx, &company_a)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        list_company_plugins(&mut tx, &company_b)
            .await
            .unwrap()
            .len(),
        1
    );
    let replacement = install_company_plugin(&mut tx, &company_a, &actor, &version, 200)
        .await
        .unwrap();
    assert_eq!(replacement.installation_version, 2);
    withdraw_company_plugin(&mut tx, &company_a, &actor, &a.id, 1, 250)
        .await
        .unwrap();
    assert_eq!(
        list_company_plugins(&mut tx, &company_a).await.unwrap()[0].installation_version,
        2
    );
    tx.rollback().await.unwrap();
}
