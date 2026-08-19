use chrono::{Duration, TimeZone, Utc};
use erpx_integration_contracts::{DisplayName, ReferenceCode, assets};
use erpx_kernel::{AuthorityScope, EventId};
use uuid::Uuid;

use super::*;

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}

fn scope() -> AuthorityScope {
    AuthorityScope::Company {
        holding_id: id(1),
        company_id: id(2),
    }
}

fn asset_fact(condition: assets::AssetCondition) -> assets::AssetConditionFact {
    assets::AssetConditionFact {
        asset: assets::AssetRef::try_from_uuid(id(10)).unwrap(),
        asset_code: ReferenceCode::try_new("AST-10").unwrap(),
        name: DisplayName::try_new("Packing line").unwrap(),
        location: assets::AssetLocationRef::try_from_uuid(id(11)).unwrap(),
        condition,
    }
}

fn request() -> MaintenanceRequest {
    MaintenanceRequest::try_new(
        EventId::try_from_uuid(id(20)).unwrap(),
        3,
        assets::AssetRef::try_from_uuid(id(10)).unwrap(),
        "REQ-100",
    )
    .unwrap()
}

fn work_order() -> WorkOrder {
    WorkOrder::open(
        WorkOrderId::try_from_uuid(id(30)).unwrap(),
        scope(),
        request(),
        asset_fact(assets::AssetCondition::OutOfService),
        "Replace drive belt",
        at(),
    )
    .unwrap()
}

#[test]
fn opening_rejects_retired_or_mismatched_asset_fact() {
    assert_eq!(
        WorkOrder::open(
            WorkOrderId::try_from_uuid(id(30)).unwrap(),
            scope(),
            request(),
            asset_fact(assets::AssetCondition::Retired),
            "Replace drive belt",
            at(),
        ),
        Err(MaintenanceDomainError::AssetRetired)
    );
    let mut other = asset_fact(assets::AssetCondition::OutOfService);
    other.asset = assets::AssetRef::try_from_uuid(id(99)).unwrap();
    assert_eq!(
        WorkOrder::open(
            WorkOrderId::try_from_uuid(id(30)).unwrap(),
            scope(),
            request(),
            other,
            "Replace drive belt",
            at(),
        ),
        Err(MaintenanceDomainError::AssetFactMismatch)
    );
}

#[test]
fn lifecycle_requires_schedule_before_start_and_start_before_complete() {
    let mut aggregate = work_order();
    assert!(matches!(
        aggregate.start(at() + Duration::hours(1)),
        Err(MaintenanceDomainError::InvalidTransition { .. })
    ));
    assert_eq!(aggregate.status(), WorkOrderStatus::Planned);
    aggregate
        .schedule(at() + Duration::hours(2), at() + Duration::hours(1))
        .unwrap();
    aggregate.start(at() + Duration::hours(2)).unwrap();
    aggregate.complete(at() + Duration::hours(3)).unwrap();
    assert_eq!(aggregate.status(), WorkOrderStatus::Completed);
    assert!(aggregate.validate().is_ok());
}

#[test]
fn failed_transition_does_not_partially_mutate_timestamps() {
    let mut aggregate = work_order();
    let version = aggregate.version();
    assert!(aggregate.start(at() + Duration::hours(1)).is_err());
    assert_eq!(aggregate.version(), version);
    assert_eq!(aggregate.started_at(), None);
}

#[test]
fn downtime_ids_and_intervals_are_unique() {
    let mut aggregate = work_order();
    aggregate
        .schedule(at() + Duration::hours(1), at() + Duration::minutes(30))
        .unwrap();
    aggregate.start(at() + Duration::hours(1)).unwrap();
    aggregate
        .record_downtime(
            Downtime {
                downtime_id: id(40),
                started_at: at() + Duration::hours(1),
                ended_at: at() + Duration::hours(2),
                reason: "belt replacement".into(),
            },
            at() + Duration::hours(2),
        )
        .unwrap();
    assert_eq!(
        aggregate.record_downtime(
            Downtime {
                downtime_id: id(41),
                started_at: at() + Duration::minutes(90),
                ended_at: at() + Duration::minutes(150),
                reason: "overlap".into(),
            },
            at() + Duration::hours(3),
        ),
        Err(MaintenanceDomainError::OverlappingDowntime)
    );
}

#[test]
fn request_rejects_zero_source_version() {
    assert_eq!(
        MaintenanceRequest::try_new(
            EventId::try_from_uuid(id(20)).unwrap(),
            0,
            assets::AssetRef::try_from_uuid(id(10)).unwrap(),
            "REQ-100",
        ),
        Err(MaintenanceDomainError::InvalidSourceVersion)
    );
}

#[test]
fn downtime_must_be_observed_within_execution_and_command_time() {
    let mut aggregate = work_order();
    aggregate
        .schedule(at() + Duration::hours(1), at() + Duration::minutes(30))
        .unwrap();
    aggregate.start(at() + Duration::hours(1)).unwrap();
    let before = aggregate.clone();
    assert_eq!(
        aggregate.record_downtime(
            Downtime {
                downtime_id: id(50),
                started_at: at() + Duration::minutes(59),
                ended_at: at() + Duration::minutes(90),
                reason: "pre-start interval".into(),
            },
            at() + Duration::hours(2),
        ),
        Err(MaintenanceDomainError::InvalidWindow)
    );
    assert_eq!(aggregate, before);

    assert_eq!(
        aggregate.record_downtime(
            Downtime {
                downtime_id: id(51),
                started_at: at() + Duration::hours(1),
                ended_at: at() + Duration::hours(3),
                reason: "future interval".into(),
            },
            at() + Duration::hours(2),
        ),
        Err(MaintenanceDomainError::InvalidWindow)
    );
    assert_eq!(aggregate, before);
}

#[test]
fn work_order_exposes_stable_first_class_record_identity() {
    let work_order_id = Uuid::new_v4();
    let value = WorkOrder::open(
        WorkOrderId::try_from_uuid(work_order_id).unwrap(),
        scope(),
        request(),
        asset_fact(assets::AssetCondition::OutOfService),
        "Reference work order",
        at(),
    )
    .unwrap();
    let reference = value.record_ref().unwrap();
    assert_eq!(reference.id().as_uuid(), work_order_id);
    assert_eq!(
        reference.canonical_detail_route(),
        format!("/maintenance/work-order/{work_order_id}")
    );
}
