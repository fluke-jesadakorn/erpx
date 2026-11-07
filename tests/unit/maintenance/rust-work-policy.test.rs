use erpx_server_maintenance::public::*;
use erpx_server_shared::public::{FailureKind, UserId};
use serde_json::{Value, json};
fn corpus() -> Value {
    serde_json::from_str(include_str!(
        "../../../fixtures/maintenance/rust-work/conformance.v1.json"
    ))
    .unwrap()
}
fn facts(row: &Value) -> MaintenanceWorkFacts {
    MaintenanceWorkFacts {
        owner_id: row["owner_id"].as_str().unwrap().to_owned(),
        status: row["status"].as_str().unwrap().into(),
        owner_eligible: row["owner_eligible"].as_i64().unwrap() > 0,
        reviewer_eligible: row["can_review"].as_i64().unwrap() > 0,
        scheduled_at: row["scheduled_at"].as_i64(),
        downtime_started_at: row["downtime_started_at"].as_i64(),
    }
}
fn view(s: MaintenanceState) -> Value {
    json!({"status":s.status,"owner":s.owner_id.as_str(),"scheduled":s.scheduled_at,"downtimeStart":s.downtime_started_at,"downtimeEnd":s.downtime_ended_at})
}
fn transition(action: &str) -> MaintenanceTransition {
    match action {
        "schedule" => MaintenanceTransition::Schedule {
            owner_id: "new-owner".into(),
            scheduled_at: 200,
        },
        "start" => MaintenanceTransition::Start,
        "block" => MaintenanceTransition::Block,
        "resume" => MaintenanceTransition::Resume,
        "complete" => MaintenanceTransition::Complete,
        "issue-part" => MaintenanceTransition::IssuePart,
        "cancel" => MaintenanceTransition::Cancel,
        _ => panic!("unreviewed source action"),
    }
}
#[test]
fn every_original_issue_part_eligibility_and_state_combination_matches_the_pure_policy() {
    let data = corpus();
    assert_eq!(data["node"], "v26.11.1");
    assert_eq!(data["rows"].as_array().unwrap().len(), 121);
    let cases: Vec<_> = data["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["id"].as_str().unwrap().starts_with("authority-"))
        .collect();
    assert_eq!(cases.len(), 112);
    for row in cases {
        let source = facts(&row["source"]);
        let actor = MaintenanceActorFacts {
            user_id: UserId::new("actor").unwrap(),
            can_review: row["options"]["review"].as_bool().unwrap(),
        };
        match require_maintenance_transition(&actor, &source, &MaintenanceTransition::IssuePart) {
            Err(error) => {
                assert_eq!(
                    error.code,
                    row["expected"]["error"]["code"].as_str().unwrap(),
                    "{}",
                    row["id"]
                );
                assert_eq!(
                    match error.kind {
                        FailureKind::Forbidden => 403,
                        FailureKind::Conflict => 409,
                        _ => panic!("unreviewed policy error"),
                    },
                    row["expected"]["error"]["status"].as_i64().unwrap()
                );
            }
            Ok(()) => {
                assert!(row["expected"]["error"].is_null(), "{}", row["id"]);
                let call = row["expected"]["calls"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|call| call[0] == "change")
                    .unwrap();
                assert_eq!(
                    view(maintenance_state(
                        &source,
                        &MaintenanceTransition::IssuePart,
                        call[4].as_i64().unwrap()
                    )),
                    call[3],
                    "{}",
                    row["id"]
                );
            }
        }
    }
}
#[test]
fn all_original_work_state_transitions_preserve_nullable_and_epoch_zero_downtime() {
    let data = corpus();
    let cases = data["transitions"].as_array().unwrap();
    assert_eq!(cases.len(), 21);
    for row in cases {
        let action = transition(row["action"].as_str().unwrap());
        assert_eq!(json!(action.allowed_states()), row["allowed"]);
        let source = MaintenanceWorkFacts {
            owner_id: "actor".into(),
            status: "in_progress".into(),
            owner_eligible: true,
            reviewer_eligible: false,
            scheduled_at: Some(1_800_000_000_000),
            downtime_started_at: row["downtime"].as_i64(),
        };
        assert_eq!(
            view(maintenance_state(&source, &action, 300)),
            row["expected"],
            "{} {}",
            row["action"],
            row["downtime"]
        );
    }
}
