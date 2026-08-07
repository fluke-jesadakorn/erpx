use chrono::Utc;
use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePools, PlatformIntegrationObservation, SqlxControlPlane,
};
use erpx_migration::migrate_control_plane;
use uuid::Uuid;

fn test_config() -> ControlPlaneDatabaseConfig {
    ControlPlaneDatabaseConfig {
        write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required"),
        read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required"),
    }
}

#[tokio::test]
#[ignore = "requires disposable control PostgreSQL databases"]
async fn integration_health_is_persisted_and_updated_without_provider_material() {
    let pools = ControlPlanePools::connect(&test_config())
        .await
        .expect("connect control databases");
    migrate_control_plane(&pools.write)
        .await
        .expect("migrate control write database");
    let store = SqlxControlPlane::new(pools.clone());
    let suffix = Uuid::new_v4().simple().to_string();
    let integration_key = format!("test_{}", &suffix[..12]);
    let first = PlatformIntegrationObservation {
        integration_key: integration_key.clone(),
        state: "not_probed".into(),
        capability: "Bounded acceptance probe".into(),
        checked_at: Utc::now(),
    };
    let rows = store
        .record_platform_integration_observations(&[first], "acceptance-test")
        .await
        .expect("persist first observation");
    assert!(
        rows.iter()
            .any(|row| row.integration_key == integration_key)
    );

    let updated_at = Utc::now();
    let updated = PlatformIntegrationObservation {
        integration_key: integration_key.clone(),
        state: "ready".into(),
        capability: "Bounded acceptance probe".into(),
        checked_at: updated_at,
    };
    let rows = store
        .record_platform_integration_observations(&[updated], "acceptance-test")
        .await
        .expect("update observation");
    let row = rows
        .iter()
        .find(|row| row.integration_key == integration_key)
        .expect("persisted observation");
    assert_eq!(row.state, "ready");
    assert_eq!(row.checked_at, updated_at);

    sqlx::query(
        "DELETE FROM erpx_control.platform_integration_observations
         WHERE integration_key = $1",
    )
    .bind(&integration_key)
    .execute(&pools.write)
    .await
    .expect("clean integration fixture");
}
