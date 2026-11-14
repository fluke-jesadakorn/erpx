//! Native manual routine run application use cases over in-memory owner ports.
//! The PostgreSQL adapter and HTTP transport have their own companion tests.
use erpx_server_operations::public::routine_commands::{
    RoutineTemplateRow, RunRoutineTemplateCommand, run_fingerprint, run_template,
};
use erpx_server_operations::public::staff_tasks::{
    ApprovalView, AttachmentRow, DecisionFact, DelegationFact, DelegationHistoryRow, PostFact,
    ProjectFact, RouteFact, SaveTaskPlan, StaffFact, StaffTaskActor, StaffTaskFuture,
    StaffTaskRepository, StaffTaskSources, StoredTaskReceipt, TaskReceipt, TaskRow, TaskScope,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

const NOW: i64 = 1_788_000_000_000;

#[derive(Default)]
struct State {
    receipts: HashMap<String, StoredTaskReceipt>,
    tasks: HashMap<String, TaskRow>,
    plans: Vec<SaveTaskPlan>,
    saves: usize,
}

#[derive(Default)]
struct Repo {
    state: Mutex<State>,
}

impl Repo {
    fn store(&self, plan: &SaveTaskPlan) {
        let mut state = self.state.lock().unwrap();
        state.saves += 1;
        state.plans.push(plan.clone());
        let row = TaskRow {
            id: plan.record_id.clone(),
            company_id: plan.company_id.clone(),
            branch_id: plan.branch_id.clone(),
            department_id: plan.department_id.clone(),
            title: plan.input.title.trim().into(),
            description: plan.input.description.trim().into(),
            creator_user_id: plan.actor_id.clone(),
            assignee_user_id: plan.input.assignee_user_id.clone(),
            due_at: plan.due_at,
            project_id: plan.input.project_id.clone(),
            start_approvers_json: serde_json::to_string(&plan.input.start_approvers).unwrap(),
            completion_approvers_json: serde_json::to_string(&plan.input.completion_approvers)
                .unwrap(),
            state: plan.state.clone(),
            blocked_reason: plan.blocked_reason.clone(),
            revision: 1,
            version: plan.version,
            template_id: plan.template_id.clone(),
            template_version: plan.template_version,
            schedule_id: plan.schedule_id.clone(),
            occurrence_at: plan.occurrence_at,
            completion_note: String::new(),
            active_delegation_id: None,
            active_delegate_user_id: None,
            active_delegate_name: None,
            active_delegate_expires_at: None,
            active_delegate_reason: None,
            active_delegate_version: None,
            created_at: plan.now,
            updated_at: plan.now,
        };
        state.tasks.insert(row.id.clone(), row);
        state.receipts.insert(
            plan.key.as_str().into(),
            StoredTaskReceipt {
                operation: plan.operation.clone(),
                record_id: plan.record_id.clone(),
                request_fingerprint: plan.request_fingerprint.clone(),
                result_json: Some(serde_json::to_string(&plan.receipt).unwrap()),
                company_id: plan.company_id.clone(),
                branch_id: plan.authority_branch_id.clone(),
                department_id: plan.authority_department_id.clone(),
                actor_id: plan.actor_id.clone(),
            },
        );
    }
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "TEST_SOURCE_UNAVAILABLE",
        "The test source is unavailable.",
    )
}

impl StaffTaskRepository for Repo {
    type Error = BusinessError;

    fn connection(&mut self) -> Option<&mut sqlx::PgConnection> {
        None
    }
    fn stored_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> StaffTaskFuture<'a, Option<StoredTaskReceipt>, Self::Error> {
        Box::pin(async move { Ok(self.state.lock().unwrap().receipts.get(key).cloned()) })
    }
    fn task<'a>(
        &'a mut self,
        actor: &'a StaffTaskActor,
        id: &'a str,
        _now: i64,
    ) -> StaffTaskFuture<'a, Option<TaskRow>, Self::Error> {
        Box::pin(async move {
            Ok(self
                .state
                .lock()
                .unwrap()
                .tasks
                .get(id)
                .filter(|row| row.company_id == actor.company_id)
                .cloned())
        })
    }
    fn list<'a>(
        &'a mut self,
        _actor: &'a StaffTaskActor,
        _project_id: Option<&'a str>,
    ) -> StaffTaskFuture<'a, Vec<TaskRow>, Self::Error> {
        Box::pin(async move { Ok(Vec::new()) })
    }
    fn delegation<'a>(
        &'a mut self,
        _company_id: &'a str,
        _task_id: &'a str,
        _delegation_id: &'a str,
        _now: i64,
    ) -> StaffTaskFuture<'a, Option<DelegationFact>, Self::Error> {
        Box::pin(async move { Ok(None) })
    }
    fn delegation_history<'a>(
        &'a mut self,
        _company_id: &'a str,
        _task_id: &'a str,
    ) -> StaffTaskFuture<'a, Vec<DelegationHistoryRow>, Self::Error> {
        Box::pin(async move { Ok(Vec::new()) })
    }
    fn save<'a>(
        &'a mut self,
        plan: SaveTaskPlan,
    ) -> StaffTaskFuture<'a, TaskReceipt, Self::Error> {
        Box::pin(async move {
            self.store(&plan);
            Ok(plan.receipt)
        })
    }
    fn transition<'a>(
        &'a mut self,
        _plan: erpx_server_operations::public::staff_tasks::TransitionTaskPlan,
    ) -> StaffTaskFuture<'a, TaskReceipt, Self::Error> {
        Box::pin(async move { Err(unavailable()) })
    }
    fn delegate<'a>(
        &'a mut self,
        _plan: erpx_server_operations::public::staff_tasks::DelegateTaskPlan,
    ) -> StaffTaskFuture<'a, TaskReceipt, Self::Error> {
        Box::pin(async move { Err(unavailable()) })
    }
    fn revoke<'a>(
        &'a mut self,
        _plan: erpx_server_operations::public::staff_tasks::RevokeDelegationPlan,
    ) -> StaffTaskFuture<'a, TaskReceipt, Self::Error> {
        Box::pin(async move { Err(unavailable()) })
    }
    fn alert_rows<'a>(
        &'a mut self,
        _now: i64,
        _warning_window_ms: i64,
    ) -> StaffTaskFuture<'a, Vec<erpx_server_operations::public::staff_tasks::AlertTask>, Self::Error>
    {
        Box::pin(async move { Ok(Vec::new()) })
    }
    fn stage_alert<'a>(
        &'a mut self,
        _plan: erpx_server_operations::public::staff_tasks::AlertPlan,
    ) -> StaffTaskFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(false) })
    }
}

struct Sources {
    inactive: HashSet<String>,
    blocked: Option<String>,
    project_missing: bool,
    calls: Mutex<Vec<String>>,
}

impl Default for Sources {
    fn default() -> Self {
        Self {
            inactive: HashSet::new(),
            blocked: None,
            project_missing: false,
            calls: Mutex::new(Vec::new()),
        }
    }
}

impl StaffTaskSources for Sources {
    type Error = BusinessError;

    fn active_staff<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _company_id: &'a str,
        user_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<StaffFact>, Self::Error> {
        Box::pin(async move {
            Ok((!self.inactive.contains(user_id)).then(|| StaffFact {
                branch_id: Some("branch".into()),
                department_id: Some("department".into()),
            }))
        })
    }
    fn project<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _company_id: &'a str,
        _project_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<ProjectFact>, Self::Error> {
        Box::pin(async move {
            Ok((!self.project_missing).then(|| ProjectFact {
                owner_user_id: "owner".into(),
                title: "Project".into(),
            }))
        })
    }
    fn same_person<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _actor: &'a StaffTaskActor,
        _user_id: &'a str,
    ) -> StaffTaskFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(true) })
    }
    fn same_person_between<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _company_id: &'a str,
        _user_a: &'a str,
        _user_b: &'a str,
    ) -> StaffTaskFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(true) })
    }
    fn readable_post<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _actor: &'a StaffTaskActor,
        _post_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<PostFact>, Self::Error> {
        Box::pin(async move { Ok(None) })
    }
    fn task_route<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _scope: &'a TaskScope,
        _creator_user_id: &'a str,
        _assignee_user_id: &'a str,
        phase: &'a str,
        approvers: &'a [String],
        _service_task: bool,
    ) -> StaffTaskFuture<'a, RouteFact, Self::Error> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push(format!("route:{phase}:{}", approvers.len()));
            Ok(RouteFact {
                blocked_reason: self.blocked.clone(),
            })
        })
    }
    fn approval_view<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _actor: &'a StaffTaskActor,
        _task: &'a TaskRow,
    ) -> StaffTaskFuture<'a, ApprovalView, Self::Error> {
        Box::pin(async move { Err(unavailable()) })
    }
    fn open_approval<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        key: &'a str,
        task: &'a TaskRow,
        phase: &'a str,
        _route: &'a RouteFact,
        version: i64,
    ) -> StaffTaskFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(format!(
                "approval:{key}:{}:{phase}:{version}",
                task.id
            ));
            Ok(())
        })
    }
    fn prepare_decision<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _key: &'a str,
        _task: &'a TaskRow,
        _approve: bool,
        _note: &'a str,
        _expected_version: i64,
    ) -> StaffTaskFuture<'a, DecisionFact, Self::Error> {
        Box::pin(async move { Err(unavailable()) })
    }
    fn close_approvals<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _key: &'a str,
        _task_id: &'a str,
        _reason: &'a str,
        _expected_version: i64,
    ) -> StaffTaskFuture<'a, (), Self::Error> {
        Box::pin(async move { Err(unavailable()) })
    }
    fn attachments<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _actor: &'a StaffTaskActor,
        _task_id: &'a str,
    ) -> StaffTaskFuture<'a, Vec<AttachmentRow>, Self::Error> {
        Box::pin(async move { Ok(Vec::new()) })
    }
    fn person_name<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _company_id: &'a str,
        _user_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<String>, Self::Error> {
        Box::pin(async move { Ok(None) })
    }
    fn feature_enabled<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _company_id: &'a str,
    ) -> StaffTaskFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(true) })
    }
}

fn actor() -> StaffTaskActor {
    StaffTaskActor {
        user_id: "runner".into(),
        assignment_id: "assignment".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: Some("department".into()),
        permissions: vec!["records.write".into()],
        modules: vec!["operations".into()],
        expires_at: 9_999_999_999_999,
    }
}

fn template() -> RoutineTemplateRow {
    RoutineTemplateRow {
        id: "template".into(),
        company_id: "company".into(),
        branch_id: Some("template-branch".into()),
        department_id: Some("template-department".into()),
        title: "Routine task".into(),
        description: "Run once".into(),
        creator_user_id: "creator".into(),
        assignee_user_id: "assignee".into(),
        project_id: None,
        start_approvers_json: "[]".into(),
        completion_approvers_json: r#"["reviewer"]"#.into(),
        published: 1,
        version: 3,
        created_at: NOW,
        updated_at: NOW,
    }
}

fn input() -> RunRoutineTemplateCommand {
    RunRoutineTemplateCommand {
        due_at: "2026-12-01T02:00:00Z".into(),
    }
}

#[tokio::test]
async fn creates_a_ready_occurrence_from_the_template_snapshot() {
    let mut repository = Repo::default();
    let mut sources = Sources::default();
    let receipt = run_template(
        &actor(),
        &mut repository,
        &mut sources,
        Some(&template()),
        "template",
        &input(),
        "run-key-0001",
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(
        receipt,
        TaskReceipt::new("task-run-key-0001", "ready", 1)
    );
    let state = repository.state.lock().unwrap();
    let plan = state.plans.first().unwrap();
    assert_eq!(plan.operation, "routine.run");
    assert_eq!(plan.branch_id.as_deref(), Some("template-branch"));
    assert_eq!(plan.department_id.as_deref(), Some("template-department"));
    assert_eq!(plan.template_id.as_deref(), Some("template"));
    assert_eq!(plan.template_version, Some(3));
    assert_eq!(plan.state, "ready");
    assert_eq!(plan.notification.as_deref(), Some("Routine task assigned to you"));
    assert_eq!(plan.creator_notification, None);
    let row = state.tasks.get("task-run-key-0001").unwrap();
    assert_eq!(row.creator_user_id, "runner");
    assert_eq!(row.assignee_user_id, "assignee");
    assert_eq!(row.template_version, Some(3));
    assert_eq!(row.blocked_reason, None);
    drop(state);
    assert_eq!(
        sources.calls.lock().unwrap().as_slice(),
        ["route:completion:1"]
    );
}

#[tokio::test]
async fn opens_the_start_approval_for_the_occurrence_version() {
    let mut template = template();
    template.start_approvers_json = r#"["start-reviewer"]"#.into();
    let mut repository = Repo::default();
    let mut sources = Sources::default();
    let receipt = run_template(
        &actor(),
        &mut repository,
        &mut sources,
        Some(&template),
        "template",
        &input(),
        "run-key-0002",
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(
        receipt,
        TaskReceipt::new("task-run-key-0002", "start_approval", 1)
    );
    assert!(sources.calls.lock().unwrap().contains(
        &"approval:run-key-0002:task-run-key-0002:start:1".to_owned()
    ));
}

#[tokio::test]
async fn keeps_a_blocked_route_in_draft_and_notifies_the_creator() {
    let mut repository = Repo::default();
    let mut sources = Sources {
        blocked: Some("The approval route is unavailable.".into()),
        ..Sources::default()
    };
    let receipt = run_template(
        &actor(),
        &mut repository,
        &mut sources,
        Some(&template()),
        "template",
        &input(),
        "run-key-0003",
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(receipt, TaskReceipt::new("task-run-key-0003", "draft", 1));
    let state = repository.state.lock().unwrap();
    let plan = state.plans.first().unwrap();
    assert_eq!(
        plan.blocked_reason.as_deref(),
        Some("The approval route is unavailable.")
    );
    assert_eq!(
        plan.notification.as_deref(),
        Some("Routine task blocked: correction required")
    );
    assert_eq!(
        plan.creator_notification.as_deref(),
        Some("Routine blocked: The approval route is unavailable.")
    );
    drop(state);
    assert!(!sources
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|call| call.starts_with("approval:")));
}

#[tokio::test]
async fn treats_an_unavailable_project_and_inactive_assignee_as_draft_blockers() {
    for (project_missing, inactive) in [(true, false), (false, true)] {
        let mut repository = Repo::default();
        let mut sources = Sources {
            project_missing,
            ..Sources::default()
        };
        if inactive {
            sources.inactive.insert("assignee".into());
        }
        let mut template = template();
        template.project_id = Some("project".into());
        let receipt = run_template(
            &actor(),
            &mut repository,
            &mut sources,
            Some(&template),
            "template",
            &input(),
            "run-key-0004",
            NOW,
        )
        .await
        .unwrap();
        assert_eq!(receipt.state, "draft");
        let state = repository.state.lock().unwrap();
        let blocked = state.plans[0].blocked_reason.as_deref().unwrap();
        if project_missing {
            assert_eq!(blocked, "The project is unavailable.");
        } else {
            assert_eq!(blocked, "Choose a current active staff member.");
        }
    }
}

#[tokio::test]
async fn replays_the_same_key_and_rejects_a_different_routine_request() {
    let mut repository = Repo::default();
    let mut sources = Sources::default();
    let first = run_template(
        &actor(),
        &mut repository,
        &mut sources,
        Some(&template()),
        "template",
        &input(),
        "run-key-0005",
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(
        first,
        TaskReceipt::new("task-run-key-0005", "ready", 1)
    );
    let replay = run_template(
        &actor(),
        &mut repository,
        &mut sources,
        None,
        "template",
        &input(),
        "run-key-0005",
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(replay, first);
    assert_eq!(repository.state.lock().unwrap().saves, 1);
    let conflict = run_template(
        &actor(),
        &mut repository,
        &mut sources,
        Some(&template()),
        "template",
        &RunRoutineTemplateCommand {
            due_at: "2026-12-02T02:00:00Z".into(),
        },
        "run-key-0005",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(conflict.code, "IDEMPOTENCY_CONFLICT");
}

#[tokio::test]
async fn refuses_unpublished_and_foreign_templates_before_any_owner_write() {
    let mut repository = Repo::default();
    let mut sources = Sources::default();
    let mut unpublished = template();
    unpublished.published = 0;
    let error = run_template(
        &actor(),
        &mut repository,
        &mut sources,
        Some(&unpublished),
        "template",
        &input(),
        "run-key-0006",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "TEMPLATE_UNPUBLISHED");
    let mut foreign = template();
    foreign.company_id = "other".into();
    let error = run_template(
        &actor(),
        &mut repository,
        &mut sources,
        Some(&foreign),
        "template",
        &input(),
        "run-key-0007",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "TEMPLATE_NOT_FOUND");
    assert_eq!(repository.state.lock().unwrap().saves, 0);
}

#[tokio::test]
async fn fingerprint_covers_exactly_the_template_and_due_instant() {
    assert_eq!(
        run_fingerprint("task-run-key-0001", "template", "2026-12-01T02:00:00Z").unwrap(),
        "routine.run\ntask-run-key-0001\n{\"templateId\":\"template\",\"dueAt\":\"2026-12-01T02:00:00Z\"}"
    );
}
