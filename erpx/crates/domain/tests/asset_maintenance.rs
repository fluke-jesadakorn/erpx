use chrono::NaiveDate;
use erpx_domain::asset_maintenance::{
    Asset, AssetMaintenanceError, AssetScope, MaintenanceKind, MaintenanceWorkOrder,
};
use erpx_domain::contracts::{ActorAttribution, ActorKind, CommandContext, TenantScope};
use uuid::Uuid;

fn context(scope: TenantScope, key: &str) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "user:asset-maintenance-test".into(),
            kind: ActorKind::TenantUser,
            session_id: Some(Uuid::from_u128(90)),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope,
        "asset maintenance domain test",
    )
    .expect("test command context is valid")
}

fn valid_asset() -> Asset {
    let scope = AssetScope::new(
        Uuid::from_u128(1),
        Uuid::from_u128(2),
        Some(Uuid::from_u128(3)),
    )
    .expect("test asset scope is valid");
    Asset::new(
        &context(scope.tenant_scope(), "asset-create"),
        Uuid::from_u128(4),
        "Forklift",
        "material_handling",
        scope,
        NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid acquisition date"),
        10_000,
        1_000,
        60,
        "employee:warehouse",
        "warehouse-a",
    )
    .expect("test asset is valid")
}

#[test]
fn maintenance_creation_rejects_an_invalid_source_asset_before_copying_it() {
    let mut source = valid_asset();
    // This is a shape a deserialized or tampered snapshot could expose: an
    // active asset without the required service date.
    source.status = erpx_domain::asset_maintenance::AssetStatus::InService;
    let before = source.clone();

    let result = MaintenanceWorkOrder::new(
        &context(source.scope.tenant_scope(), "maintenance-create"),
        Uuid::from_u128(5),
        &source,
        MaintenanceKind::Corrective,
        "Repair motor",
    );

    assert_eq!(source, before, "source asset must remain untouched");
    assert!(matches!(
        result,
        Err(AssetMaintenanceError::InvalidDeterministicField {
            field: "placed_in_service_on"
        })
    ));
}
