use erpx_server_assets::public::maintenance::*;
use serde_json::Value;
fn condition(value: &str) -> MaintenanceAssetCondition {
    match value {
        "good" => MaintenanceAssetCondition::Good,
        "degraded" => MaintenanceAssetCondition::Degraded,
        "unsafe" => MaintenanceAssetCondition::Unsafe,
        _ => panic!("closed fixture condition"),
    }
}
fn lifecycle(value: &str) -> MaintenanceAssetLifecycle {
    match value {
        "registered" => MaintenanceAssetLifecycle::Registered,
        "in_service" => MaintenanceAssetLifecycle::InService,
        "maintenance" => MaintenanceAssetLifecycle::Maintenance,
        "retirement_pending" => MaintenanceAssetLifecycle::RetirementPending,
        "retired" => MaintenanceAssetLifecycle::Retired,
        _ => panic!("closed fixture lifecycle"),
    }
}
#[test]
fn b0_prepared_sql_scalar_cases_preserve_null_placement_and_cancel_truthiness() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../fixtures/assets/rust-maintenance-effects/conformance.v1.json"
    ))
    .unwrap();
    let rows = corpus["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 900);
    for row in rows {
        let old = &row["before"];
        let before = MaintenanceAssetState {
            version: old["version"].as_i64().unwrap(),
            condition: condition(old["condition"].as_str().unwrap()),
            lifecycle: lifecycle(old["lifecycle_status"].as_str().unwrap()),
            placement_present: !old["placement"].is_null(),
        };
        let action = match row["action"].as_str().unwrap() {
            "start" => MaintenancePhysicalAction::Start,
            "cancel" => MaintenancePhysicalAction::Cancel {
                downtime_started_at: row["downtime"].as_i64(),
            },
            value => MaintenancePhysicalAction::Complete {
                condition: condition(value.strip_prefix("complete-").unwrap()),
            },
        };
        let derived = maintenance_asset_after(&before, &action).unwrap();
        if matches!(action, MaintenancePhysicalAction::Start) && !row["refusal"].is_null() {
            assert_eq!(
                require_maintenance_start_source(&before, 7)
                    .unwrap_err()
                    .code,
                "CONFLICT"
            );
            continue;
        }
        if matches!(action, MaintenancePhysicalAction::Start) {
            require_maintenance_start_source(&before, 7).unwrap();
        }
        let actual = derived.as_ref().unwrap_or(&before);
        let expected = &row["after"];
        assert_eq!(
            actual.version,
            expected["version"].as_i64().unwrap(),
            "{row}"
        );
        assert_eq!(
            actual.condition.as_str(),
            expected["condition"].as_str().unwrap(),
            "{row}"
        );
        assert_eq!(
            actual.lifecycle.as_str(),
            expected["lifecycle_status"].as_str().unwrap(),
            "{row}"
        );
        let events = row["events"].as_array().unwrap();
        assert_eq!(events.len(), usize::from(derived.is_some()));
        if !events.is_empty() {
            assert_eq!(events[0]["action"].as_str().unwrap(), action.event_action());
        }
    }
}
#[test]
fn start_version_refusal_remains_a_separate_late_guard() {
    let before = MaintenanceAssetState {
        version: 3,
        condition: MaintenanceAssetCondition::Good,
        lifecycle: MaintenanceAssetLifecycle::Registered,
        placement_present: false,
    };
    assert_eq!(
        maintenance_asset_after(&before, &MaintenancePhysicalAction::Start)
            .unwrap()
            .unwrap()
            .version,
        4
    );
    assert_eq!(
        require_maintenance_start_source(&before, 2)
            .unwrap_err()
            .code,
        "CONFLICT"
    );
    let maximum = MaintenanceAssetState {
        version: 9_007_199_254_740_991,
        ..before
    };
    assert_eq!(
        maintenance_asset_after(&maximum, &MaintenancePhysicalAction::Start)
            .unwrap()
            .unwrap()
            .version,
        9_007_199_254_740_992
    );
}
