use std::collections::BTreeSet;

use chrono::{NaiveDate, TimeZone, Utc};
use erpx_integration_contracts::{
    Quantity, RecordVersionRef, StableRecordRef, catalog::ContextId, reference::RecordTypeId,
};
use erpx_kernel::{CorrelationId, RecordId, TenantScope};
use uuid::Uuid;

use super::*;

fn uuid(value: u128) -> Uuid {
    let mut bytes = value.to_be_bytes();
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}
fn at(hour: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 15, hour, 0, 0)
        .single()
        .unwrap()
}
fn day(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 8, day).unwrap()
}
fn scope() -> TenantScope {
    TenantScope::Branch {
        holding_id: uuid(1),
        company_id: uuid(2),
        branch_id: uuid(3),
    }
}
fn record(id: u128, owner: ContextId, kind: &str) -> StableRecordRef {
    StableRecordRef::try_new(
        RecordId::try_from_uuid(uuid(id)).unwrap(),
        owner,
        RecordTypeId::try_new(kind).unwrap(),
    )
    .unwrap()
}
fn version(record: StableRecordRef, version: u64) -> RecordVersionRef {
    RecordVersionRef::try_new(record, version).unwrap()
}
fn qty(units: i64) -> Quantity {
    Quantity::try_new(units, 3).unwrap()
}

fn frozen_scenario() -> PlanningScenario {
    let item = record(10, ContextId::Inventory, "inventory.lot");
    let location = record(11, ContextId::Organization, "organization.branch");
    let demand_source = version(record(12, ContextId::Sales, "sales.order"), 4);
    let supply_source = version(record(13, ContextId::Inventory, "inventory.document"), 6);
    let resource = record(14, ContextId::Assets, "asset.asset");
    let demand_id = DemandId::try_from_uuid(uuid(20)).unwrap();
    let mut modes = BTreeSet::new();
    modes.insert(PlanningMode::Mrp);
    modes.insert(PlanningMode::Drp);
    modes.insert(PlanningMode::Capacity);
    let mut value = PlanningScenario::draft(
        ScenarioId::try_from_uuid(uuid(30)).unwrap(),
        scope(),
        modes,
        99,
        vec![demand_source.clone(), supply_source.clone()],
        PlanningPolicy {
            policy_id: uuid(31),
            version: 7,
            sourcing: vec![SourcingRule {
                item: item.clone(),
                kind: RecommendationKind::Make,
                source_location: None,
                lead_time_days: 2,
            }],
            allow_late_recommendations: false,
        },
        vec![DemandFact {
            id: demand_id,
            item: item.clone(),
            location: location.clone(),
            quantity: qty(10_000),
            required_on: day(20),
            source: demand_source,
        }],
        vec![SupplyFact {
            id: SupplyId::try_from_uuid(uuid(21)).unwrap(),
            item,
            location,
            quantity: qty(4_000),
            available_on: day(19),
            source: supply_source,
        }],
        vec![CapacityBucket {
            resource: resource.clone(),
            bucket: day(19),
            available_seconds: 3_600,
        }],
        vec![CapacityRequirement {
            demand_id,
            resource,
            bucket: day(19),
            required_seconds: 7_200,
        }],
        at(1),
    )
    .unwrap();
    value.freeze(at(2)).unwrap();
    value
}

#[test]
fn deterministic_mrp_drp_and_capacity_results_use_pinned_inputs() {
    let mut scenario = frozen_scenario();
    scenario.mark_running(at(3)).unwrap();
    let first =
        PlanningRun::execute(RunId::try_from_uuid(uuid(40)).unwrap(), &scenario, at(4)).unwrap();
    let second =
        PlanningRun::execute(RunId::try_from_uuid(uuid(41)).unwrap(), &scenario, at(5)).unwrap();
    assert_eq!(first.input_digest, second.input_digest);
    assert_eq!(first.pegging[0].quantity, qty(4_000));
    assert_eq!(first.shortages[0].quantity, qty(6_000));
    assert_eq!(first.recommendations[0].id, second.recommendations[0].id);
    assert_eq!(first.overloads[0].required_seconds, 7_200);
}

#[test]
fn run_is_denied_until_scenario_is_frozen_and_running() {
    let scenario = frozen_scenario();
    assert_eq!(
        PlanningRun::execute(RunId::try_from_uuid(uuid(42)).unwrap(), &scenario, at(4)),
        Err(PlanningDomainError::ScenarioNotPinned)
    );
}

#[test]
fn failed_run_requires_reasoned_recovery_before_retry() {
    let mut scenario = frozen_scenario();
    scenario.mark_running(at(3)).unwrap();
    assert_eq!(
        scenario.mark_failed("short", at(4)),
        Err(PlanningDomainError::InvalidFailureReason)
    );
    scenario
        .mark_failed("capacity provider unavailable", at(4))
        .unwrap();
    assert_eq!(scenario.status(), ScenarioStatus::Failed);
    scenario
        .prepare_retry("provider recovered; replay pinned inputs", at(5))
        .unwrap();
    assert_eq!(scenario.status(), ScenarioStatus::Frozen);
}

#[test]
fn override_requires_reason_and_release_requires_approval() {
    let mut scenario = frozen_scenario();
    scenario.mark_running(at(3)).unwrap();
    let mut recommendation =
        PlanningRun::execute(RunId::try_from_uuid(uuid(43)).unwrap(), &scenario, at(4))
            .unwrap()
            .recommendations
            .remove(0);
    let invalid = RecommendationOverride {
        id: OverrideId::try_from_uuid(uuid(44)).unwrap(),
        previous_quantity: qty(6_000),
        revised_quantity: qty(5_000),
        reason: "short".into(),
        principal_id: uuid(45),
        at: at(5),
    };
    assert_eq!(
        recommendation.override_quantity(invalid),
        Err(PlanningDomainError::InvalidOverride)
    );
    let valid = RecommendationOverride {
        id: OverrideId::try_from_uuid(uuid(46)).unwrap(),
        previous_quantity: qty(6_000),
        revised_quantity: qty(5_000),
        reason: "Supplier confirmed alternate lot".into(),
        principal_id: uuid(45),
        at: at(5),
    };
    assert!(recommendation.override_quantity(valid).unwrap());
    assert_eq!(
        RecommendationRelease::from_approved(
            ReleaseId::try_from_uuid(uuid(47)).unwrap(),
            &recommendation,
            CorrelationId::try_from_uuid(uuid(48)).unwrap()
        ),
        Err(PlanningDomainError::RecommendationNotApproved)
    );
    recommendation.approve().unwrap();
    let release = RecommendationRelease::from_approved(
        ReleaseId::try_from_uuid(uuid(47)).unwrap(),
        &recommendation,
        CorrelationId::try_from_uuid(uuid(49)).unwrap(),
    )
    .unwrap();
    assert_eq!(release.recommendation_version, recommendation.version);
}
