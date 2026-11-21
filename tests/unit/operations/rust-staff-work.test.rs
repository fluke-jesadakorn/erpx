//! Portable Staff Work task rules. Database behaviour lives in the PostgreSQL
//! companion test.
use erpx_server_operations::public::staff_tasks::{
    ApprovalView, DecisionFact, DelegateTaskCommand, SaveTaskCommand, StaffTaskActor,
    TaskAccessFacts, TaskAction, TaskRow, TransitionTaskCommand, can_attach, can_configure,
    delegation_record_id, permitted_actions, plan_transition, require_scope,
};

fn actor() -> StaffTaskActor {
    StaffTaskActor {
        user_id: "creator".into(),
        assignment_id: "assignment-creator".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: Some("department".into()),
        permissions: vec!["records.read".into(), "records.write".into()],
        modules: vec!["operations".into()],
        expires_at: 9_999_999_999_999,
    }
}

fn task() -> TaskRow {
    TaskRow {
        id: "task-1".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: Some("department".into()),
        title: "Inspect deliveries".into(),
        description: "Check the receiving log".into(),
        creator_user_id: "creator".into(),
        assignee_user_id: "worker".into(),
        due_at: Some(1_800_000_000_000),
        project_id: None,
        start_approvers_json: "[]".into(),
        completion_approvers_json: "[]".into(),
        state: "ready".into(),
        blocked_reason: None,
        revision: 1,
        version: 3,
        template_id: None,
        template_version: None,
        schedule_id: None,
        occurrence_at: None,
        completion_note: String::new(),
        active_delegation_id: None,
        active_delegate_user_id: None,
        active_delegate_name: None,
        active_delegate_expires_at: None,
        active_delegate_reason: None,
        active_delegate_version: None,
        created_at: 1,
        updated_at: 2,
    }
}

fn approval(can_decide: bool, phase: &str) -> ApprovalView {
    ApprovalView {
        state: "pending".into(),
        phase: Some(phase.into()),
        can_decide,
        blocked_reason: None,
        steps: serde_json::json!([]),
    }
}
fn decision(phase: &str, final_decision: bool) -> DecisionFact {
    DecisionFact {
        phase: phase.into(),
        final_decision,
    }
}

fn command(action: TaskAction, note: &str) -> TransitionTaskCommand {
    TransitionTaskCommand {
        action,
        expected_version: 3,
        note: note.into(),
        attachment_set_ids: Vec::new(),
    }
}

#[test]
fn actor_requires_current_operations_write_context() {
    let mut first = actor();
    assert!(first.require_read(1_000).is_ok());
    assert!(first.require_write(1_000).is_ok());
    first.modules.clear();
    assert_eq!(
        first.require_write(1_000).unwrap_err().code,
        "MODULE_FORBIDDEN"
    );
    let mut second = actor();
    second.expires_at = 999;
    assert_eq!(
        second.require_write(1_000).unwrap_err().code,
        "WORK_CONTEXT_CHANGED"
    );
    let mut third = actor();
    third.permissions.retain(|value| value != "records.write");
    assert_eq!(
        third.require_write(1_000).unwrap_err().code,
        "PERMISSION_REQUIRED"
    );
}

#[test]
fn save_command_validates_due_date_and_approval_route() {
    let base = SaveTaskCommand {
        title: "Inspect deliveries".into(),
        description: String::new(),
        assignee_user_id: "worker".into(),
        project_id: None,
        start_approvers: Vec::new(),
        completion_approvers: Vec::new(),
        due_at: None,
        expected_version: 1,
        reason: String::new(),
    };
    assert!(base.validate().is_ok());
    assert_eq!(
        base.due_millis().unwrap(),
        None,
        "an ordinary task may omit a due date"
    );
    let mut reviewed = base.clone();
    reviewed.completion_approvers = vec!["head".into()];
    assert_eq!(reviewed.validate().unwrap_err().code, "TASK_DUE_REQUIRED");
    reviewed.due_at = Some("2026-10-01T02:00:00Z".into());
    assert_eq!(
        reviewed.due_millis().unwrap(),
        Some(1_790_820_000_000),
        "RFC3339 UTC"
    );
    reviewed.due_at = Some("2026-10-01T09:00:00+07:00".into());
    assert_eq!(reviewed.due_millis().unwrap(), Some(1_790_820_000_000));
    reviewed.due_at = Some("2026-10-01T02:00:00".into());
    assert_eq!(reviewed.validate().unwrap_err().code, "TASK_FIELDS_INVALID");
    reviewed.due_at = Some("2026-10-01T02:00:00Z".into());
    reviewed.title = " x ".into();
    assert_eq!(
        reviewed.validate().unwrap_err().code,
        "OPERATIONS_INPUT_INVALID"
    );
}

#[test]
fn scope_rejects_foreign_company_and_selected_branch() {
    let actor = actor();
    assert!(require_scope(&actor, &task()).is_ok());
    let mut foreign = task();
    foreign.company_id = "other".into();
    assert_eq!(
        require_scope(&actor, &foreign).unwrap_err().code,
        "TASK_NOT_FOUND"
    );
    let mut branch = task();
    branch.branch_id = Some("other-branch".into());
    assert_eq!(
        require_scope(&actor, &branch).unwrap_err().code,
        "TASK_SCOPE_REQUIRED"
    );
    branch.branch_id = None;
    assert!(
        require_scope(&actor, &branch).is_ok(),
        "company-wide tasks remain visible"
    );
}

#[test]
fn ordinary_task_transitions_follow_the_owner_state_machine() {
    let task = task();
    let assigned = TaskAccessFacts {
        same_assignee: true,
        ..TaskAccessFacts::default()
    };
    assert!(can_attach(&task, &assigned).unwrap());
    let plan = plan_transition(
        &task,
        &command(TaskAction::Start, ""),
        &assigned,
        None,
        None,
    )
    .unwrap();
    assert_eq!(plan.state, "in_progress");
    assert_eq!(plan.version, 4);

    let mut in_progress = task.clone();
    in_progress.state = "in_progress".into();
    let plan = plan_transition(
        &in_progress,
        &command(TaskAction::Complete, ""),
        &assigned,
        None,
        None,
    )
    .unwrap();
    assert_eq!(plan.state, "completed");
    assert_eq!(
        plan_transition(
            &in_progress,
            &command(TaskAction::Cancel, ""),
            &assigned,
            None,
            None
        )
        .unwrap_err()
        .code,
        "TASK_ACTION_UNAVAILABLE"
    );
}

#[test]
fn reviewed_tasks_require_manage_and_approval_decisions() {
    let mut task = task();
    task.state = "draft".into();
    task.start_approvers_json = r#"["head"]"#.into();
    task.completion_approvers_json = r#"["head"]"#.into();
    let creator = TaskAccessFacts {
        same_creator: true,
        ..TaskAccessFacts::default()
    };
    assert!(can_configure(&creator));
    let plan = plan_transition(
        &task,
        &command(TaskAction::Submit, ""),
        &creator,
        None,
        None,
    )
    .unwrap();
    assert_eq!(plan.state, "start_approval");

    let mut approval_task = task.clone();
    approval_task.state = "completion_approval".into();
    let head = TaskAccessFacts {
        approver: true,
        ..TaskAccessFacts::default()
    };
    assert_eq!(
        plan_transition(
            &approval_task,
            &command(TaskAction::Approve, "Verified"),
            &head,
            Some(&approval(true, "completion")),
            Some(&decision("completion", true))
        )
        .unwrap()
        .state,
        "completed"
    );
    assert_eq!(
        plan_transition(
            &approval_task,
            &command(TaskAction::Reject, "Missing evidence"),
            &head,
            Some(&approval(true, "completion")),
            Some(&decision("completion", true))
        )
        .unwrap()
        .state,
        "rework"
    );
    assert_eq!(
        plan_transition(
            &approval_task,
            &command(TaskAction::Approve, "Verified"),
            &head,
            Some(&approval(false, "completion")),
            Some(&decision("completion", true))
        )
        .unwrap_err()
        .code,
        "TASK_ACTION_UNAVAILABLE"
    );
}

#[test]
fn cancel_requires_manager_and_reason_and_permitted_actions_are_bounded() {
    let task = task();
    let creator = TaskAccessFacts {
        same_creator: true,
        ..TaskAccessFacts::default()
    };
    assert_eq!(
        plan_transition(
            &task,
            &command(TaskAction::Cancel, ""),
            &creator,
            None,
            None
        )
        .unwrap_err()
        .code,
        "REASON_REQUIRED"
    );
    assert_eq!(
        plan_transition(
            &task,
            &command(TaskAction::Cancel, "Duplicate"),
            &TaskAccessFacts::default(),
            None,
            None
        )
        .unwrap_err()
        .code,
        "TASK_ACTION_UNAVAILABLE"
    );
    assert_eq!(
        plan_transition(
            &task,
            &command(TaskAction::Cancel, "Duplicate"),
            &creator,
            None,
            None
        )
        .unwrap()
        .state,
        "cancelled"
    );
    let actions = permitted_actions(&task, &creator, None, None).unwrap();
    assert!(actions.contains(&"revise".to_owned()));
    assert!(actions.contains(&"delegate".to_owned()));
    assert!(actions.contains(&"cancel".to_owned()));
    assert!(!actions.contains(&"start".to_owned()));
}

#[test]
fn delegate_command_enforces_90_day_expiry_and_identifier() {
    let now = 1_790_000_000_000;
    let input = DelegateTaskCommand {
        delegate_user_id: "delegate".into(),
        expires_at: "2026-12-01T00:00:00Z".into(),
        expected_version: 3,
        reason: "Annual leave".into(),
    };
    assert!(input.validate(now).is_ok());
    let mut expired = input.clone();
    expired.expires_at = "2020-01-01T00:00:00Z".into();
    assert_eq!(
        expired.validate(now).unwrap_err().code,
        "TASK_DELEGATE_EXPIRY"
    );
    let mut far = input.clone();
    far.expires_at = "2027-12-01T00:00:00Z".into();
    assert_eq!(far.validate(now).unwrap_err().code, "TASK_DELEGATE_EXPIRY");
    assert_eq!(delegation_record_id("key-1"), "task-delegation-key-1");
}
