//! Concrete composition-ready Staff Work sources.
//!
//! Every cross-owner fact must come from the owning port; an absent delegate
//! refuses with its own named code. Only the two owner-local facts (ordinary
//! approval view and an empty approval route) resolve without a delegate.
#![allow(clippy::field_reassign_with_default)]
use erpx_server_operations::public::staff_tasks::{
    ApprovalView, AttachmentRow, DecisionFact, FailClosedStaffTaskFactDelegates,
    PgStaffTaskSources, PostFact, ProjectFact, RouteFact, StaffFact, StaffTaskActor,
    StaffTaskFactDelegates, StaffTaskFuture, StaffTaskSources, TaskRow, TaskScope,
};
use erpx_server_shared::public::BusinessError;

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
        description: String::new(),
        creator_user_id: "creator".into(),
        assignee_user_id: "worker".into(),
        due_at: Some(1_800_000_000_000),
        project_id: Some("project-1".into()),
        start_approvers_json: "[]".into(),
        completion_approvers_json: "[]".into(),
        state: "ready".into(),
        blocked_reason: None,
        revision: 1,
        version: 1,
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
        updated_at: 1,
    }
}

fn reviewed() -> TaskRow {
    let mut row = task();
    row.start_approvers_json = r#"["head"]"#.into();
    row.completion_approvers_json = r#"["head"]"#.into();
    row.state = "completion_approval".into();
    row
}

fn scope() -> TaskScope {
    TaskScope {
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: Some("department".into()),
    }
}

/// A delegate that serves every fact from a fabricated owner answer, so the
/// forwarding behaviour of the concrete source can be asserted without a
/// database.
struct Facts;

impl StaffTaskFactDelegates for Facts {
    fn active_staff<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _company_id: &'a str,
        _user_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<StaffFact>, BusinessError> {
        Box::pin(async {
            Ok(Some(StaffFact {
                branch_id: Some("branch".into()),
                department_id: Some("department".into()),
            }))
        })
    }

    fn project<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _company_id: &'a str,
        project_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<ProjectFact>, BusinessError> {
        let project_id = project_id.to_owned();
        Box::pin(async move {
            Ok(Some(ProjectFact {
                owner_user_id: "owner".into(),
                title: format!("Project {project_id}"),
            }))
        })
    }

    fn same_person<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        actor: &'a StaffTaskActor,
        user_id: &'a str,
    ) -> StaffTaskFuture<'a, bool, BusinessError> {
        Box::pin(async move { Ok(actor.user_id == user_id) })
    }

    fn same_person_between<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _company_id: &'a str,
        user_a: &'a str,
        user_b: &'a str,
    ) -> StaffTaskFuture<'a, bool, BusinessError> {
        Box::pin(async move { Ok(user_a == user_b) })
    }

    fn readable_post<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _actor: &'a StaffTaskActor,
        post_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<PostFact>, BusinessError> {
        let post_id = post_id.to_owned();
        Box::pin(async move {
            Ok(Some(PostFact {
                id: post_id,
                author_id: "author".into(),
            }))
        })
    }

    fn task_route<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _scope: &'a TaskScope,
        _creator_user_id: &'a str,
        _assignee_user_id: &'a str,
        phase: &'a str,
        _approvers: &'a [String],
        _service_task: bool,
    ) -> StaffTaskFuture<'a, RouteFact, BusinessError> {
        let phase = phase.to_owned();
        Box::pin(async move {
            Ok(RouteFact {
                blocked_reason: Some(format!("blocked in {phase}")),
            })
        })
    }

    fn approval_view<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _actor: &'a StaffTaskActor,
        task: &'a TaskRow,
    ) -> StaffTaskFuture<'a, ApprovalView, BusinessError> {
        let task_id = task.id.clone();
        Box::pin(async move {
            Ok(ApprovalView {
                state: "pending".into(),
                phase: Some("completion".into()),
                can_decide: true,
                blocked_reason: None,
                steps: serde_json::json!([{"taskId": task_id}]),
            })
        })
    }

    fn open_approval<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _key: &'a str,
        _task: &'a TaskRow,
        _phase: &'a str,
        _route: &'a RouteFact,
        _version: i64,
    ) -> StaffTaskFuture<'a, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }

    fn prepare_decision<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _key: &'a str,
        _task: &'a TaskRow,
        approve: bool,
        _note: &'a str,
        _expected_version: i64,
    ) -> StaffTaskFuture<'a, DecisionFact, BusinessError> {
        Box::pin(async move {
            Ok(DecisionFact {
                phase: "completion".into(),
                final_decision: approve,
            })
        })
    }

    fn close_approvals<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _key: &'a str,
        _task_id: &'a str,
        _reason: &'a str,
        _expected_version: i64,
    ) -> StaffTaskFuture<'a, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }

    fn attachments<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _actor: &'a StaffTaskActor,
        task_id: &'a str,
    ) -> StaffTaskFuture<'a, Vec<AttachmentRow>, BusinessError> {
        let task_id = task_id.to_owned();
        Box::pin(async move {
            Ok(vec![AttachmentRow {
                id: format!("attachment-{task_id}"),
                original_name: "evidence.pdf".into(),
                bytes: 64,
                media_type: "application/pdf".into(),
            }])
        })
    }

    fn person_name<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _company_id: &'a str,
        user_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<String>, BusinessError> {
        let user_id = user_id.to_owned();
        Box::pin(async move { Ok(Some(format!("Name {user_id}"))) })
    }

    fn feature_enabled<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _company_id: &'a str,
    ) -> StaffTaskFuture<'a, bool, BusinessError> {
        Box::pin(async { Ok(true) })
    }
}

#[tokio::test]
async fn concrete_sources_forward_each_current_owner_fact() {
    let actor = actor();
    let mut sources = PgStaffTaskSources::new(Facts);
    let staff = sources
        .active_staff(None, "company", "worker")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(staff.branch_id.as_deref(), Some("branch"));
    assert_eq!(staff.department_id.as_deref(), Some("department"));
    let project = sources
        .project(None, "company", "project-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(project.owner_user_id, "owner");
    assert_eq!(project.title, "Project project-1");
    assert!(sources.same_person(None, &actor, "creator").await.unwrap());
    assert!(!sources.same_person(None, &actor, "worker").await.unwrap());
    assert!(
        sources
            .same_person_between(None, "company", "a", "a")
            .await
            .unwrap()
    );
    assert!(
        !sources
            .same_person_between(None, "company", "a", "b")
            .await
            .unwrap()
    );
    let post = sources
        .readable_post(None, &actor, "post-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(post.id, "post-1");
    assert_eq!(post.author_id, "author");
    let route = sources
        .task_route(
            None,
            &scope(),
            "creator",
            "worker",
            "start",
            &["head".to_owned()],
            false,
        )
        .await
        .unwrap();
    assert_eq!(route.blocked_reason.as_deref(), Some("blocked in start"));
    let approval = sources
        .approval_view(None, &actor, &reviewed())
        .await
        .unwrap();
    assert_eq!(approval.state, "pending");
    assert!(approval.can_decide);
    assert_eq!(approval.phase.as_deref(), Some("completion"));
    sources
        .open_approval(
            None,
            "key-1",
            &reviewed(),
            "completion",
            &RouteFact {
                blocked_reason: None,
            },
            2,
        )
        .await
        .unwrap();
    let decision = sources
        .prepare_decision(None, "key-1", &reviewed(), true, "Verified", 1)
        .await
        .unwrap();
    assert_eq!(decision.phase, "completion");
    assert!(decision.final_decision);
    sources
        .close_approvals(None, "key-1", "task-1", "Duplicate", 1)
        .await
        .unwrap();
    let attachments = sources.attachments(None, &actor, "task-1").await.unwrap();
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].id, "attachment-task-1");
    assert_eq!(
        sources
            .person_name(None, "company", "worker")
            .await
            .unwrap()
            .as_deref(),
        Some("Name worker")
    );
    assert!(sources.feature_enabled(None, "company").await.unwrap());
}

#[tokio::test]
async fn ordinary_task_presentation_resolves_without_a_workflow_delegate() {
    let actor = actor();
    let mut sources = PgStaffTaskSources::new(FailClosedStaffTaskFactDelegates);
    let approval = sources
        .task_route(
            None,
            &scope(),
            "creator",
            "worker",
            "completion",
            &[],
            false,
        )
        .await
        .unwrap();
    assert!(
        approval.blocked_reason.is_none(),
        "an empty route has nothing to validate"
    );
    let view = sources.approval_view(None, &actor, &task()).await.unwrap();
    assert_eq!(view.state, "not_required");
    assert!(view.phase.is_none());
    assert!(!view.can_decide);
    assert!(view.blocked_reason.is_none());
    assert_eq!(view.steps, serde_json::json!([]));
}

#[tokio::test]
async fn each_absent_delegate_fails_closed_with_its_named_code() {
    let actor = actor();
    let row = reviewed();
    let route = RouteFact {
        blocked_reason: None,
    };
    let mut sources = PgStaffTaskSources::fail_closed();
    assert_eq!(
        sources
            .active_staff(None, "company", "worker")
            .await
            .unwrap_err()
            .code,
        "TASK_PEOPLE_ACTIVE_STAFF_SOURCE_REQUIRED"
    );
    assert_eq!(
        sources
            .project(None, "company", "project-1")
            .await
            .unwrap_err()
            .code,
        "TASK_PROJECT_SOURCE_REQUIRED"
    );
    assert_eq!(
        sources
            .same_person(None, &actor, "creator")
            .await
            .unwrap_err()
            .code,
        "TASK_IDENTITY_SAME_PERSON_SOURCE_REQUIRED"
    );
    assert_eq!(
        sources
            .same_person_between(None, "company", "a", "b")
            .await
            .unwrap_err()
            .code,
        "TASK_IDENTITY_SAME_PERSON_BETWEEN_SOURCE_REQUIRED"
    );
    assert_eq!(
        sources
            .readable_post(None, &actor, "post-1")
            .await
            .unwrap_err()
            .code,
        "TASK_COLLABORATION_POST_SOURCE_REQUIRED"
    );
    assert_eq!(
        sources
            .task_route(
                None,
                &scope(),
                "creator",
                "worker",
                "start",
                &["head".to_owned()],
                false,
            )
            .await
            .unwrap_err()
            .code,
        "TASK_WORKFLOW_ROUTE_SOURCE_REQUIRED"
    );
    assert_eq!(
        sources
            .approval_view(None, &actor, &row)
            .await
            .unwrap_err()
            .code,
        "TASK_WORKFLOW_APPROVAL_VIEW_SOURCE_REQUIRED"
    );
    assert_eq!(
        sources
            .open_approval(None, "key-1", &row, "start", &route, 2)
            .await
            .unwrap_err()
            .code,
        "TASK_WORKFLOW_OPEN_APPROVAL_SOURCE_REQUIRED"
    );
    assert_eq!(
        sources
            .prepare_decision(None, "key-1", &row, true, "Verified", 1)
            .await
            .unwrap_err()
            .code,
        "TASK_WORKFLOW_DECISION_SOURCE_REQUIRED"
    );
    assert_eq!(
        sources
            .close_approvals(None, "key-1", "task-1", "Duplicate", 1)
            .await
            .unwrap_err()
            .code,
        "TASK_WORKFLOW_CLOSE_APPROVAL_SOURCE_REQUIRED"
    );
    assert_eq!(
        sources
            .attachments(None, &actor, "task-1")
            .await
            .unwrap_err()
            .code,
        "TASK_FILES_ATTACHMENT_SOURCE_REQUIRED"
    );
    assert_eq!(
        sources
            .person_name(None, "company", "worker")
            .await
            .unwrap_err()
            .code,
        "TASK_PEOPLE_NAME_SOURCE_REQUIRED"
    );
    assert_eq!(
        sources
            .feature_enabled(None, "company")
            .await
            .unwrap_err()
            .code,
        "TASK_PMS_FEATURE_SOURCE_REQUIRED"
    );
}
