use erpx_infrastructure::{
    ControlPlanePools, HoldingStoreRegistrationDetail, SqlxControlPlane, SqlxHoldingStoreRegistry,
};
use sqlx::PgPool;
use std::sync::Arc;

#[test]
fn detail_dto_contains_only_safe_registry_metadata() {
    let detail = HoldingStoreRegistrationDetail {
        holding_id: "holding-opaque-1".into(),
        data_profile_key: "holding-opaque-1".into(),
        state: "active".into(),
        registry_version: 3,
        write_registered: true,
        read_registered: true,
        storage_registered: true,
        credential_binding_registered: true,
        backup_binding_registered: true,
    };
    let rendered = format!("{detail:?}");
    assert!(!rendered.contains("postgres://"));
    assert!(!rendered.contains("password"));
    assert!(!rendered.contains("database_key"));
    assert!(rendered.contains("holding-opaque-1"));
}

#[tokio::test]
async fn invalid_detail_holding_id_fails_before_database_access() {
    let control = Arc::new(SqlxControlPlane::new(ControlPlanePools {
        write: PgPool::connect_lazy("postgres://unused").expect("lazy write"),
        read: PgPool::connect_lazy("postgres://unused-read").expect("lazy read"),
    }));
    let registry = SqlxHoldingStoreRegistry::new(control);
    assert!(registry.get_detail("holding/with/path").await.is_err());
    assert!(registry.get_detail("").await.is_err());
}
