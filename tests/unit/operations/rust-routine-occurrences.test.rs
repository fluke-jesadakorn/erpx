//! Native scheduled routine occurrence planning over owner-local facts.
use erpx_server_operations::public::routine_occurrences::{
    OccurrenceSourceFacts, due_at_iso, plan_occurrence,
};
use erpx_server_operations::public::routine_commands::{
    RoutineScheduleRow, RoutineTemplateRow,
};

const AT: i64 = 1_788_000_000_000;

fn template(start: &[&str], completion: &[&str], project: Option<&str>) -> RoutineTemplateRow {
    RoutineTemplateRow {
        id: "template".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: Some("department".into()),
        title: "Daily checks".into(),
        description: "Check the room".into(),
        creator_user_id: "creator".into(),
        assignee_user_id: "assignee".into(),
        project_id: project.map(str::to_owned),
        start_approvers_json: serde_json::to_string(start).unwrap(),
        completion_approvers_json: serde_json::to_string(completion).unwrap(),
        published: 1,
        version: 3,
        created_at: 1,
        updated_at: 1,
    }
}

fn schedule() -> RoutineScheduleRow {
    RoutineScheduleRow {
        id: "schedule".into(),
        template_id: "template".into(),
        company_id: "company".into(),
        frequency: "daily".into(),
        timezone: "Asia/Bangkok".into(),
        occurrence_time: "09:00".into(),
        start_date: "2026-01-01".into(),
        end_date: None,
        weekday: None,
        month_day: None,
        due_offset_hours: 24,
        state: "active".into(),
        next_occurrence_at: Some(AT),
        paused_at: None,
        last_generated_at: None,
        last_error: None,
        version: 2,
        created_at: 1,
        updated_at: 1,
    }
}

fn facts() -> OccurrenceSourceFacts {
    OccurrenceSourceFacts {
        pause_exists: false,
        pause_ended_at: None,
        prior: false,
        start_blocked: None,
        completion_blocked: None,
        project_available: true,
    }
}

#[test]
fn plans_a_ready_occurrence_with_the_retained_identity_and_due_offset() {
    let plan = plan_occurrence(&template(&[], &[], None), &schedule(), &facts(), AT).unwrap();
    assert_eq!(plan.key, format!("occurrence-schedule-{AT}"));
    assert_eq!(plan.task_id, format!("task-schedule-{AT}"));
    assert_eq!(plan.next, Some(AT + 55_200_000));
    assert_eq!(plan.due_at, AT + 86_400_000);
    assert_eq!(plan.state, "ready");
    assert_eq!(plan.blocked_reason, None);
    assert_eq!(plan.notification, "Routine task assigned to you");
    assert_eq!(plan.creator_notification, None);
    assert!(!plan.open_start_approval);
}

#[test]
fn plans_start_approval_and_opens_the_retained_start_route() {
    let plan = plan_occurrence(
        &template(&["start-reviewer"], &["completion-reviewer"], None),
        &schedule(),
        &facts(),
        AT,
    )
    .unwrap();
    assert_eq!(plan.state, "start_approval");
    assert!(plan.open_start_approval);
    assert_eq!(plan.start_approvers, vec!["start-reviewer"]);
    assert_eq!(plan.completion_approvers, vec!["completion-reviewer"]);
}

#[test]
fn blocked_or_unavailable_sources_become_drafts_with_both_notices() {
    let mut blocked = facts();
    blocked.completion_blocked = Some("Choose current active staff as approvers.".into());
    let plan = plan_occurrence(
        &template(&["start-reviewer"], &["completion-reviewer"], None),
        &schedule(),
        &blocked,
        AT,
    )
    .unwrap();
    assert_eq!(plan.state, "draft");
    assert_eq!(
        plan.blocked_reason.as_deref(),
        Some("Choose current active staff as approvers.")
    );
    assert_eq!(
        plan.notification,
        "Routine task blocked: correction required"
    );
    assert_eq!(
        plan.creator_notification.as_deref(),
        Some("Routine blocked: Choose current active staff as approvers.")
    );

    let mut missing_project = facts();
    missing_project.project_available = false;
    let plan = plan_occurrence(
        &template(&[], &[], Some("project")),
        &schedule(),
        &missing_project,
        AT,
    )
    .unwrap();
    assert_eq!(plan.state, "draft");
    assert_eq!(
        plan.blocked_reason.as_deref(),
        Some("The project is unavailable.")
    );
}

#[test]
fn pause_and_prior_advance_without_generating_and_use_the_ended_pause_cursor() {
    let mut paused = facts();
    paused.pause_exists = true;
    paused.pause_ended_at = Some(AT + 3 * 86_400_000);
    let plan = plan_occurrence(&template(&[], &[], None), &schedule(), &paused, AT).unwrap();
    assert_eq!(plan.state, "");
    assert_eq!(plan.next, Some(AT + 314_400_000));
    assert!(!plan.open_start_approval);
    assert_eq!(plan.notification, "");

    let mut prior = facts();
    prior.prior = true;
    let plan = plan_occurrence(&template(&[], &[], None), &schedule(), &prior, AT).unwrap();
    assert_eq!(plan.state, "");
    assert_eq!(plan.next, Some(AT + 55_200_000));
}

#[test]
fn an_ended_rule_advances_the_cursor_to_none() {
    let mut ended = schedule();
    ended.end_date = Some("2026-08-29".into());
    let plan = plan_occurrence(&template(&[], &[], None), &ended, &facts(), AT).unwrap();
    assert_eq!(plan.next, None);
}

#[test]
fn due_at_iso_round_trips_the_exact_owner_millisecond() {
    assert_eq!(due_at_iso(AT).unwrap(), "2026-08-29T10:40:00Z");
}
