use std::env;
use std::sync::Arc;

use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePools, HoldingStoreRegistration,
    MAX_HOLDING_REGISTRY_PAGE_SIZE, SqlxControlPlane, SqlxHoldingStoreRegistry,
};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use uuid::Uuid;

fn required(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} is required"))
}

#[tokio::test]
#[ignore = "requires two disposable control PostgreSQL databases; URLs are never printed"]
async fn holding_registry_page_is_bounded_ordered_and_exclusive() {
    let config = ControlPlaneDatabaseConfig {
        write_url: required("ERPX_TEST_CONTROL_WRITE_DATABASE_URL"),
        read_url: required("ERPX_TEST_CONTROL_READ_DATABASE_URL"),
    };
    let pools = ControlPlanePools::connect(&config)
        .await
        .expect("control pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("control write migration");
    migrate_control_read(&pools.read)
        .await
        .expect("control read migration");
    let store = Arc::new(SqlxControlPlane::new(pools));
    let registry = SqlxHoldingStoreRegistry::new(store.clone());
    let token = Uuid::new_v4().simple().to_string();
    let holding_ids = [
        format!("zzzz-holding-page-{token}-a"),
        format!("zzzz-holding-page-{token}-b"),
        format!("zzzz-holding-page-{token}-c"),
    ];
    for holding_id in &holding_ids {
        registry
            .register(HoldingStoreRegistration {
                holding_id: holding_id.clone(),
                data_profile_key: format!("holding-{holding_id}"),
                write_database_key: format!("write-{holding_id}"),
                read_database_key: format!("read-{holding_id}"),
                storage_namespace: format!("storage-{holding_id}"),
                credential_key_ref: format!("credential-{holding_id}"),
                backup_chain_ref: format!("backup-{holding_id}"),
                state: "active".into(),
                registry_version: 1,
            })
            .await
            .expect("register holding");
    }

    assert!(registry.list_page(None, 0).await.is_err());
    assert!(
        registry
            .list_page(None, MAX_HOLDING_REGISTRY_PAGE_SIZE + 1)
            .await
            .is_err()
    );
    let anchor = format!("zzzz-holding-page-{token}-");
    let first = registry
        .list_page(Some(&anchor), 2)
        .await
        .expect("first page");
    assert_eq!(
        first
            .registrations
            .iter()
            .map(|row| row.holding_id.clone())
            .collect::<Vec<_>>(),
        holding_ids[..2].to_vec()
    );
    assert!(first.has_more);
    let cursor = first.next_after_holding_id.as_deref().expect("next cursor");
    let second = registry
        .list_page(Some(cursor), 2)
        .await
        .expect("second page");
    assert_eq!(
        second
            .registrations
            .iter()
            .map(|row| row.holding_id.clone())
            .collect::<Vec<_>>(),
        holding_ids[2..].to_vec()
    );
    assert!(!second.has_more);
    assert!(second.next_after_holding_id.is_none());
    store.pools().write.close().await;
    store.pools().read.close().await;
}
