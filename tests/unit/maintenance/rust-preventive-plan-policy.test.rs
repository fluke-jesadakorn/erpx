//! Preventive-plan owner policy: independent activation, retained-occurrence
//! guard, eligible owner, state and version/CAS refusal order.
use erpx_server_maintenance::public::preventive_plan::*;
use erpx_server_shared::public::BusinessError;

fn actor(user: &str, review: bool) -> PreventivePlanActor {
    PreventivePlanActor {
        user_id: user.into(),
        can_review: review,
    }
}

fn plan() -> PreventivePlanFacts {
    PreventivePlanFacts {
        id: "plan".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        department_id: None,
        asset_id: "asset".into(),
        title: "Daily safety inspection".into(),
        priority: "normal".into(),
        owner_id: "owner".into(),
        interval_days: 1,
        time_zone: "America/New_York".into(),
        first_due_local: "2026-03-07T02:30".into(),
        next_due_local: "2026-03-07T02:30".into(),
        next_due_at: 1_772_868_600_000,
        lead_days: 0,
        status: "draft".into(),
        version: 1,
        created_by: "requester".into(),
        edited_by: "requester".into(),
        approved_by: None,
        approved_at: None,
        updated_at: 0,
        owner_eligible: true,
    }
}

fn definition() -> PreventivePlanDefinition {
    PreventivePlanDefinition {
        asset_id: "asset".into(),
        asset_version: 1,
        title: "Daily safety inspection".into(),
        priority: "normal".into(),
        owner_id: "owner".into(),
        interval_days: 1,
        first_due_at: 1_772_868_659_000,
        time_zone: "America/New_York".into(),
        lead_days: 0,
        expected_version: None,
        reason: "Reviewed preventive plan".into(),
    }
}

fn code<T: std::fmt::Debug>(result: Result<T, BusinessError>) -> String {
    result.unwrap_err().code.to_owned()
}

#[test]
fn create_is_version_one_draft_and_ignores_expected_version() {
    let save = require_save(&actor("requester", false), None, &definition()).unwrap();
    assert_eq!(save.next_version, 1);
    assert_eq!(save.status, "draft");
    let mut supplied = definition();
    supplied.expected_version = Some(9);
    assert_eq!(
        require_save(&actor("requester", false), None, &supplied)
            .unwrap()
            .next_version,
        1
    );
}

#[test]
fn revision_needs_a_draft_or_paused_same_asset_plan_and_current_version() {
    let mut expected = definition();
    expected.expected_version = Some(1);
    assert_eq!(code(require_save(&actor("requester", true), Some(&plan()), &definition())), "VERSION_REQUIRED");
    assert_eq!(
        require_save(&actor("requester", true), Some(&plan()), &expected)
            .unwrap()
            .next_version,
        2
    );
    let mut active = plan();
    active.status = "active".into();
    assert_eq!(
        code(require_save(&actor("requester", true), Some(&active), &expected)),
        "MAINTENANCE_PLAN_PAUSE_REQUIRED"
    );
    let mut foreign = expected.clone();
    foreign.asset_id = "other".into();
    assert_eq!(
        code(require_save(&actor("requester", true), Some(&plan()), &foreign)),
        "MAINTENANCE_PLAN_PAUSE_REQUIRED"
    );
    assert_eq!(
        code(require_save(&actor("someone", false), Some(&plan()), &expected)),
        "MAINTENANCE_PLAN_REVIEW_REQUIRED"
    );
    let mut stale = expected.clone();
    stale.expected_version = Some(4);
    assert_eq!(
        code(require_save(&actor("requester", true), Some(&plan()), &stale)),
        "CONFLICT"
    );
}

#[test]
fn activation_is_independent_and_requires_an_eligible_owner() {
    let current = plan();
    assert_eq!(
        code(require_action(&actor("reviewer", false), &current, &PreventivePlanAction::Activate, 1, 0)),
        "PERMISSION_REQUIRED"
    );
    assert_eq!(
        code(require_action(&actor("requester", true), &current, &PreventivePlanAction::Activate, 1, 0)),
        "INDEPENDENT_PLAN_REVIEW_REQUIRED"
    );
    let policy = require_action(
        &actor("reviewer", true),
        &current,
        &PreventivePlanAction::Activate,
        1,
        0,
    )
    .unwrap();
    assert_eq!(policy.status, "active");
    assert_eq!(policy.next_version, 2);
    assert!(policy.approve);
    let mut ineligible = current.clone();
    ineligible.owner_eligible = false;
    assert_eq!(
        code(require_action(&actor("reviewer", true), &ineligible, &PreventivePlanAction::Activate, 1, 0)),
        "CONFLICT"
    );
    let mut wrong_version = current.clone();
    wrong_version.version = 2;
    assert_eq!(
        code(require_action(&actor("reviewer", true), &wrong_version, &PreventivePlanAction::Activate, 1, 0)),
        "CONFLICT"
    );
}

#[test]
fn generation_requires_active_due_owner_or_reviewer_and_matching_version() {
    let mut current = plan();
    current.status = "active".into();
    assert_eq!(
        code(require_action(&actor("someone", false), &current, &PreventivePlanAction::Generate, 1, 0)),
        "MAINTENANCE_PLAN_OWNER_REQUIRED"
    );
    let now = current.next_due_at;
    let policy = require_action(
        &actor("owner", false),
        &current,
        &PreventivePlanAction::Generate,
        1,
        now,
    )
    .unwrap();
    assert_eq!(policy.next_version, 2);
    assert!(!policy.approve);
    assert_eq!(
        code(require_action(&actor("owner", false), &current, &PreventivePlanAction::Generate, 1, 0)),
        "MAINTENANCE_PLAN_NOT_DUE"
    );
    assert_eq!(
        code(require_action(&actor("owner", false), &current, &PreventivePlanAction::Generate, 9, 0)),
        "CONFLICT"
    );
    let mut overdue = current.clone();
    overdue.lead_days = 7;
    assert!(require_action(
        &actor("owner", false),
        &overdue,
        &PreventivePlanAction::Generate,
        1,
        now
    )
    .is_ok());
}

#[test]
fn pause_and_cancel_keep_the_retained_state_and_permission_rules() {
    let mut active = plan();
    active.status = "active".into();
    assert_eq!(
        code(require_action(&actor("owner", false), &active, &PreventivePlanAction::Pause, 1, 0)),
        "PERMISSION_REQUIRED"
    );
    assert_eq!(
        require_action(&actor("reviewer", true), &active, &PreventivePlanAction::Pause, 1, 0)
            .unwrap()
            .status,
        "paused"
    );
    let mut draft = active.clone();
    draft.status = "draft".into();
    assert_eq!(
        code(require_action(&actor("reviewer", true), &draft, &PreventivePlanAction::Pause, 1, 0)),
        "MAINTENANCE_PLAN_STATE"
    );
    assert_eq!(
        require_action(&actor("reviewer", true), &draft, &PreventivePlanAction::Cancel, 1, 0)
            .unwrap()
            .status,
        "cancelled"
    );
    let mut cancelled = active.clone();
    cancelled.status = "cancelled".into();
    assert_eq!(
        code(require_action(&actor("reviewer", true), &cancelled, &PreventivePlanAction::Cancel, 1, 0)),
        "MAINTENANCE_PLAN_STATE"
    );
}

#[test]
fn retained_occurrence_guard_compares_stored_local_minutes() {
    assert!(retained_occurrence_conflict(
        Some("2026-03-08T02:30"),
        "2026-03-07T02:30"
    ));
    assert!(retained_occurrence_conflict(
        Some("2026-03-08T02:30"),
        "2026-03-08T02:30"
    ));
    assert!(!retained_occurrence_conflict(
        Some("2026-03-07T02:30"),
        "2026-03-08T02:30"
    ));
    assert!(!retained_occurrence_conflict(None, "2026-03-08T02:30"));
}
