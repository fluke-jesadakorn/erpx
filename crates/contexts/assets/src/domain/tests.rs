use chrono::{Duration, TimeZone, Utc};
use erpx_integration_contracts::assets::AssetCondition;
use erpx_kernel::AuthorityScope;
use uuid::Uuid;

use super::*;

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}

fn scope() -> AssetScope {
    AssetScope::try_new(AuthorityScope::Company {
        holding_id: id(1),
        company_id: id(2),
    })
    .unwrap()
}

fn asset() -> Asset {
    Asset::create(
        AssetId::try_from_uuid(id(10)).unwrap(),
        "AST-10",
        "Packing line",
        scope(),
        id(11),
        at(),
    )
    .unwrap()
}

#[test]
fn asset_rejects_non_exact_scope_and_nil_location() {
    assert_eq!(
        AssetScope::try_new(AuthorityScope::Holding { holding_id: id(1) }),
        Err(AssetDomainError::InvalidScope)
    );
    assert!(matches!(
        Asset::create(
            AssetId::try_from_uuid(id(10)).unwrap(),
            "AST-10",
            "Packing line",
            scope(),
            Uuid::nil(),
            at(),
        ),
        Err(AssetDomainError::NilIdentifier {
            field: "location_id"
        })
    ));
}

#[test]
fn service_and_maintenance_lifecycle_preserves_condition_invariants() {
    let mut aggregate = asset();
    aggregate.take_uncommitted_events();
    aggregate
        .place_in_service(at() + Duration::hours(1))
        .unwrap();
    assert_eq!(aggregate.condition(), AssetCondition::Operational);
    aggregate.mark_degraded(at() + Duration::hours(2)).unwrap();
    assert_eq!(aggregate.condition(), AssetCondition::Degraded);
    aggregate
        .begin_maintenance("REQ-100", "Inspect packing line", at() + Duration::hours(3))
        .unwrap();
    assert_eq!(aggregate.status(), AssetStatus::UnderMaintenance);
    assert_eq!(aggregate.condition(), AssetCondition::OutOfService);
    assert!(aggregate.validate().is_ok());
}

#[test]
fn maintenance_request_emits_owner_private_events_in_order() {
    let mut aggregate = asset();
    aggregate.take_uncommitted_events();
    aggregate
        .place_in_service(at() + Duration::hours(1))
        .unwrap();
    aggregate.take_uncommitted_events();
    aggregate
        .begin_maintenance("REQ-100", "Inspect packing line", at() + Duration::hours(2))
        .unwrap();
    let events = aggregate.take_uncommitted_events();
    assert!(matches!(
        events.as_slice(),
        [
            DomainEvent::ConditionChanged {
                condition: AssetCondition::OutOfService,
                ..
            },
            DomainEvent::MaintenanceRequested { request_code, title, .. }
        ] if request_code == "REQ-100" && title == "Inspect packing line"
    ));
}

#[test]
fn impossible_transition_is_rejected_without_changing_version() {
    let mut aggregate = asset();
    let version = aggregate.version();
    assert!(matches!(
        aggregate.return_to_service(at() + Duration::hours(1)),
        Err(AssetDomainError::InvalidTransition { .. })
    ));
    assert_eq!(aggregate.version(), version);
    assert_eq!(aggregate.status(), AssetStatus::Draft);
}

#[test]
fn retired_asset_cannot_reenter_service() {
    let mut aggregate = asset();
    aggregate.retire(at() + Duration::hours(1)).unwrap();
    assert_eq!(aggregate.condition(), AssetCondition::Retired);
    assert!(matches!(
        aggregate.place_in_service(at() + Duration::hours(2)),
        Err(AssetDomainError::InvalidTransition { .. })
    ));
}

#[test]
fn failed_version_advance_is_atomic() {
    let mut aggregate = asset();
    aggregate.version = u64::MAX;
    let before = aggregate.clone();
    assert_eq!(
        aggregate.place_in_service(at() + Duration::hours(1)),
        Err(AssetDomainError::VersionOverflow)
    );
    assert_eq!(aggregate, before);
}

#[test]
fn asset_exposes_stable_first_class_record_identity() {
    let asset_id = Uuid::new_v4();
    let value = Asset::create(
        AssetId::try_from_uuid(asset_id).unwrap(),
        "AST-REF",
        "Reference asset",
        scope(),
        id(11),
        at(),
    )
    .unwrap();
    let reference = value.record_ref().unwrap();
    assert_eq!(reference.id().as_uuid(), asset_id);
    assert_eq!(
        reference.canonical_detail_route(),
        format!("/asset/asset/{asset_id}")
    );
}
