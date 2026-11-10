//! Pure native service-team schema/local-rule checks against the TypeScript
//! definitions. The actual TS oracle remains in the crate domain module.
use erpx_server_support::public::service_teams::{
    RecordScope, ServiceTeamActor, in_service_workspace, normalize_service_team,
    validate_local_team_rules,
};
use serde_json::json;

const CATEGORIES: &[&str] = &["it", "facilities", "operations"];

fn team() -> serde_json::Value {
    json!({
        "name":"Desk team","active":true,"expectedVersion":1,
        "members":[{"userId":"lead","responsibility":"lead"}],
        "coverage":[{"category":"it","scopeKind":"holding","scopeNodeId":"company"}]
    })
}

#[test]
fn normalized_team_preserves_defaults_and_trimming() {
    let input = normalize_service_team(&team(), CATEGORIES).unwrap();
    assert_eq!(input.name, "Desk team");
    assert!(input.active);
    assert_eq!(input.expected_version, 1);
    assert_eq!(input.members[0].responsibility, "lead");
    assert_eq!(input.coverage[0].scope_kind, "holding");
}

#[test]
fn local_rules_reject_duplicates_and_missing_lead() {
    let mut duplicate = team();
    duplicate["members"] = json!([
        {"userId":"a","responsibility":"lead"},
        {"userId":"a","responsibility":"member"}
    ]);
    let input = normalize_service_team(&duplicate, CATEGORIES).unwrap();
    assert_eq!(validate_local_team_rules(&input), Err("DUPLICATE_MEMBER"));

    let mut no_lead = team();
    no_lead["members"] = json!([{"userId":"a","responsibility":"member"}]);
    let input = normalize_service_team(&no_lead, CATEGORIES).unwrap();
    assert_eq!(validate_local_team_rules(&input), Err("TEAM_LEAD_REQUIRED"));
}

#[test]
fn schema_rejects_unknown_category_and_empty_coverage() {
    let mut unknown = team();
    unknown["coverage"][0]["category"] = json!("unknown");
    assert_eq!(
        normalize_service_team(&unknown, CATEGORIES),
        Err("VALIDATION_FAILED")
    );
    let mut empty = team();
    empty["coverage"] = json!([]);
    assert_eq!(
        normalize_service_team(&empty, CATEGORIES),
        Err("VALIDATION_FAILED")
    );
}

#[test]
fn workspace_containment_matches_typescript() {
    let branch = ServiceTeamActor {
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        user_id: "owner".into(),
        holding_owner: true,
    };
    assert!(in_service_workspace(
        &branch,
        &RecordScope {
            company_id: "company".into(),
            branch_id: Some("branch".into()),
            department_id: Some("dept".into()),
        }
    ));
    assert!(!in_service_workspace(
        &branch,
        &RecordScope {
            company_id: "other".into(),
            branch_id: Some("branch".into()),
            department_id: None,
        }
    ));
}
