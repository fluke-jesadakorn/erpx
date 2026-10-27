use erpx_server_collaboration::public::maintenance_notifications::*;
use erpx_server_shared::public::{
    CommandKey, CompanyId, HoldingId, RecordId, TimestampMillis, UserId,
};
use serde_json::{Value, json};
fn facts() -> MaintenanceNotificationFacts {
    MaintenanceNotificationFacts {
        owner_key: CommandKey::new("command-key").unwrap(),
        kind: MaintenanceNotificationKind::Updated,
        holding_id: HoldingId::new("holding").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        work_id: RecordId::new("work").unwrap(),
        owner_user_id: UserId::new("owner").unwrap(),
        work_version_after: 8,
        updated_at_after: TimestampMillis::new(1800000000007),
    }
}
#[test]
fn original_node_sql_rows_match_both_messages_full_outer_keys_unicode_and_versions() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../fixtures/collaboration/rust-maintenance-notifications/conformance.v1.json"
    ))
    .unwrap();
    let rows = corpus["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 16);
    for row in rows {
        let f = &row["facts"];
        let facts = MaintenanceNotificationFacts {
            owner_key: CommandKey::new(row["key"].as_str().unwrap()).unwrap(),
            kind: if row["created"].as_bool().unwrap() {
                MaintenanceNotificationKind::Created
            } else {
                MaintenanceNotificationKind::Updated
            },
            company_id: CompanyId::new(f["company_id"].as_str().unwrap()).unwrap(),
            work_id: RecordId::new(f["id"].as_str().unwrap()).unwrap(),
            owner_user_id: UserId::new(f["owner_id"].as_str().unwrap()).unwrap(),
            work_version_after: f["version"].as_i64().unwrap(),
            updated_at_after: TimestampMillis::new(f["updated_at"].as_i64().unwrap()),
            ..facts()
        };
        let plan = derive_maintenance_notification(
            &facts,
            &CommandKey::new(row["key"].as_str().unwrap()).unwrap(),
            if row["created"].as_bool().unwrap() {
                MaintenanceNotificationKind::Created
            } else {
                MaintenanceNotificationKind::Updated
            },
        )
        .unwrap();
        let expected = json!({"id":plan.id().as_str(),"company_id":facts.company_id.as_str(),"user_id":facts.owner_user_id.as_str(),"record_type":"maintenance_work","record_id":facts.work_id.as_str(),"record_version":facts.work_version_after,"message":plan.message(),"created_at":facts.updated_at_after.get(),"read_at":null});
        assert_eq!(expected, row["expected"]);
        assert_eq!(plan.event_id(), format!("legacy:{}", plan.id().as_str()));
    }
}
#[test]
fn data_keeps_every_valid_key_without_truncation_and_has_no_clock_or_fallback_recipient() {
    for length in 8..=180 {
        let key = CommandKey::new("x".repeat(length)).unwrap();
        let f = MaintenanceNotificationFacts {
            owner_key: key.clone(),
            ..facts()
        };
        let n = derive_maintenance_notification(&f, &key, MaintenanceNotificationKind::Updated)
            .unwrap();
        assert_eq!(
            n.id().as_str(),
            format!("maintenance-notice-{}", key.as_str())
        );
        assert_eq!(n.facts(), &f);
        assert_eq!(n.id().as_str().len(), 19 + length);
    }
}
#[test]
fn source_key_and_action_are_required_bindings_rather_than_caller_selected_labels() {
    let f = facts();
    for (key, kind) in [
        (
            CommandKey::new("different-key").unwrap(),
            MaintenanceNotificationKind::Updated,
        ),
        (f.owner_key.clone(), MaintenanceNotificationKind::Created),
    ] {
        assert!(
            matches!(derive_maintenance_notification(&f,&key,kind),Err(e)if e.code=="MAINTENANCE_NOTIFICATION_BINDING_REQUIRED")
        );
    }
}
#[test]
fn invalid_version_and_time_never_mint_derived_notification_data() {
    for f in [
        MaintenanceNotificationFacts {
            work_version_after: 0,
            ..facts()
        },
        MaintenanceNotificationFacts {
            work_version_after: 9_007_199_254_740_993,
            ..facts()
        },
        MaintenanceNotificationFacts {
            updated_at_after: TimestampMillis::new(-1),
            ..facts()
        },
        MaintenanceNotificationFacts {
            updated_at_after: TimestampMillis::new(9_007_199_254_740_992),
            ..facts()
        },
    ] {
        assert!(
            matches!(derive_maintenance_notification(&f,&CommandKey::new("command-key").unwrap(),MaintenanceNotificationKind::Updated),Err(e)if e.code=="MAINTENANCE_NOTIFICATION_INPUT_INVALID")
        );
    }
}
#[test]
fn provisional_next_version_is_only_prepared_data_until_actual_owner_apply() {
    let f = MaintenanceNotificationFacts {
        work_version_after: 9_007_199_254_740_992,
        ..facts()
    };
    assert!(
        derive_maintenance_notification(
            &f,
            &CommandKey::new("command-key").unwrap(),
            MaintenanceNotificationKind::Updated
        )
        .is_ok()
    );
}
