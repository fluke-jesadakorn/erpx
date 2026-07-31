use std::env;
use std::sync::Arc;

use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePools, MAX_TENANT_REGISTRY_PAGE_SIZE, SqlxControlPlane,
    SqlxTenantStoreRegistry, TenantStoreRegistration,
};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use uuid::Uuid;

fn required(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} is required"))
}

#[tokio::test]
#[ignore = "requires two disposable control PostgreSQL databases; URLs are never printed"]
async fn tenant_registry_page_is_bounded_ordered_and_exclusive() {
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
    let registry = SqlxTenantStoreRegistry::new(store.clone());
    let token = Uuid::new_v4().simple().to_string();
    let tenant_ids = [
        format!("zzzz-tenant-page-{token}-a"),
        format!("zzzz-tenant-page-{token}-b"),
        format!("zzzz-tenant-page-{token}-c"),
    ];
    for tenant_id in &tenant_ids {
        registry
            .register(TenantStoreRegistration {
                tenant_id: tenant_id.clone(),
                holding_ref: format!("holding-{tenant_id}"),
                write_database_key: format!("write-{tenant_id}"),
                read_database_key: format!("read-{tenant_id}"),
                storage_namespace: format!("storage-{tenant_id}"),
                credential_key_ref: format!("credential-{tenant_id}"),
                backup_chain_ref: format!("backup-{tenant_id}"),
                state: "active".into(),
                registry_version: 1,
            })
            .await
            .expect("register tenant");
    }

    assert!(registry.list_page(None, 0).await.is_err());
    assert!(
        registry
            .list_page(None, MAX_TENANT_REGISTRY_PAGE_SIZE + 1)
            .await
            .is_err()
    );
    let anchor = format!("zzzz-tenant-page-{token}-");
    let first = registry
        .list_page(Some(&anchor), 2)
        .await
        .expect("first page");
    assert_eq!(
        first
            .registrations
            .iter()
            .map(|row| row.tenant_id.clone())
            .collect::<Vec<_>>(),
        tenant_ids[..2].to_vec()
    );
    assert!(first.has_more);
    let cursor = first.next_after_tenant_id.as_deref().expect("next cursor");
    let second = registry
        .list_page(Some(cursor), 2)
        .await
        .expect("second page");
    assert_eq!(
        second
            .registrations
            .iter()
            .map(|row| row.tenant_id.clone())
            .collect::<Vec<_>>(),
        tenant_ids[2..].to_vec()
    );
    assert!(!second.has_more);
    assert!(second.next_after_tenant_id.is_none());
    store.pools().write.close().await;
    store.pools().read.close().await;
}
