//! Staff Work application use cases over in-memory owner ports. No database is
//! implied: the real PostgreSQL adapter has its own companion test.
use erpx_server_operations::public::staff_tasks::{
    AlertPlan, AlertTask, ApprovalView, AttachmentRow, DecisionFact, DelegateTaskCommand,
    DelegationFact, DelegationHistoryRow, PostFact, ProjectFact, RevokeTaskDelegationCommand,
    RouteFact, SaveTaskCommand, StaffFact, StaffTaskActor, StaffTaskFuture, StaffTaskRepository,
    StaffTaskSources, StoredTaskReceipt, TaskAccessFacts, TaskAction, TaskReceipt, TaskRow,
    TransitionTaskCommand, delegate_task, escalate_staff_tasks, list_tasks, revoke_task_delegation,
    save_task, task_detail, transition_task,
};
use erpx_server_shared::public::BusinessError;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
struct Repo {
    tasks: HashMap<String, TaskRow>,
    receipts: HashMap<String, StoredTaskReceipt>,
    alerts: HashSet<String>,
    alert_rows: Vec<AlertTask>,
    writes: usize,
}
impl Repo {
    #[allow(clippy::too_many_arguments)]
    fn store(
        &mut self,
        key: &str,
        plan: &TaskReceipt,
        operation: &str,
        record: &str,
        fp: &str,
        company: &str,
        branch: Option<&str>,
        department: Option<&str>,
    ) {
        self.writes += 1;
        self.receipts.insert(
            key.into(),
            StoredTaskReceipt {
                operation: operation.into(),
                record_id: record.into(),
                request_fingerprint: fp.into(),
                result_json: Some(serde_json::to_string(plan).unwrap()),
                company_id: company.into(),
                branch_id: branch.map(Into::into),
                department_id: department.map(Into::into),
                actor_id: "creator".into(),
            },
        );
    }
}
impl StaffTaskRepository for Repo {
    type Error = BusinessError;
    fn stored_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> StaffTaskFuture<'a, Option<StoredTaskReceipt>, Self::Error> {
        Box::pin(async move { Ok(self.receipts.get(key).cloned()) })
    }
    fn task<'a>(
        &'a mut self,
        _actor: &'a StaffTaskActor,
        id: &'a str,
        _now: i64,
    ) -> StaffTaskFuture<'a, Option<TaskRow>, Self::Error> {
        Box::pin(async move { Ok(self.tasks.get(id).cloned()) })
    }
    fn list<'a>(
        &'a mut self,
        _actor: &'a StaffTaskActor,
        project_id: Option<&'a str>,
    ) -> StaffTaskFuture<'a, Vec<TaskRow>, Self::Error> {
        Box::pin(async move {
            Ok(self
                .tasks
                .values()
                .filter(|task| project_id.is_none_or(|id| task.project_id.as_deref() == Some(id)))
                .cloned()
                .collect())
        })
    }
    fn delegation<'a>(
        &'a mut self,
        _company: &'a str,
        _task: &'a str,
        id: &'a str,
        _now: i64,
    ) -> StaffTaskFuture<'a, Option<DelegationFact>, Self::Error> {
        Box::pin(async move {
            Ok(if id == "delegation-1" {
                Some(DelegationFact {
                    id: id.into(),
                    delegate_user_id: "delegate".into(),
                    version: 1,
                })
            } else {
                None
            })
        })
    }
    fn delegation_history<'a>(
        &'a mut self,
        _company: &'a str,
        _task: &'a str,
    ) -> StaffTaskFuture<'a, Vec<DelegationHistoryRow>, Self::Error> {
        Box::pin(async move { Ok(Vec::new()) })
    }
    fn save<'a>(
        &'a mut self,
        plan: erpx_server_operations::public::staff_tasks::SaveTaskPlan,
    ) -> StaffTaskFuture<'a, TaskReceipt, Self::Error> {
        Box::pin(async move {
            let row = TaskRow {
                id: plan.record_id.clone(),
                company_id: plan.company_id.clone(),
                branch_id: plan.branch_id.clone(),
                department_id: plan.department_id.clone(),
                title: plan.input.title.trim().into(),
                description: plan.input.description.trim().into(),
                creator_user_id: "creator".into(),
                assignee_user_id: plan.input.assignee_user_id.clone(),
                due_at: plan.due_at,
                project_id: plan.input.project_id.clone(),
                start_approvers_json: serde_json::to_string(&plan.input.start_approvers).unwrap(),
                completion_approvers_json: serde_json::to_string(&plan.input.completion_approvers)
                    .unwrap(),
                state: plan.state.clone(),
                blocked_reason: None,
                revision: 1,
                version: plan.version,
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
                created_at: plan.now,
                updated_at: plan.now,
            };
            self.tasks.insert(row.id.clone(), row);
            self.store(
                plan.key.as_str(),
                &plan.receipt,
                &plan.operation,
                &plan.record_id,
                &plan.request_fingerprint,
                &plan.company_id,
                plan.branch_id.as_deref(),
                plan.department_id.as_deref(),
            );
            Ok(plan.receipt)
        })
    }
    fn transition<'a>(
        &'a mut self,
        plan: erpx_server_operations::public::staff_tasks::TransitionTaskPlan,
    ) -> StaffTaskFuture<'a, TaskReceipt, Self::Error> {
        Box::pin(async move {
            let row = self.tasks.get_mut(&plan.record_id).unwrap();
            row.state = plan.plan.state.into();
            row.version = plan.plan.version;
            row.completion_note = plan.input.note.trim().into();
            self.store(
                plan.key.as_str(),
                &plan.receipt,
                &plan.operation,
                &plan.record_id,
                &plan.request_fingerprint,
                &plan.company_id,
                plan.branch_id.as_deref(),
                plan.department_id.as_deref(),
            );
            Ok(plan.receipt)
        })
    }
    fn delegate<'a>(
        &'a mut self,
        plan: erpx_server_operations::public::staff_tasks::DelegateTaskPlan,
    ) -> StaffTaskFuture<'a, TaskReceipt, Self::Error> {
        Box::pin(async move {
            let row = self.tasks.get_mut(&plan.task_id).unwrap();
            row.version = plan.version;
            row.active_delegation_id = Some(plan.delegation_id.clone());
            row.active_delegate_user_id = Some(plan.delegate_user_id.clone());
            row.active_delegate_expires_at = Some(plan.expires_at);
            row.active_delegate_reason = Some(plan.reason.clone());
            row.active_delegate_version = Some(1);
            self.store(
                plan.key.as_str(),
                &plan.receipt,
                &plan.operation,
                &plan.record_id,
                &plan.request_fingerprint,
                &plan.company_id,
                None,
                None,
            );
            Ok(plan.receipt)
        })
    }
    fn revoke<'a>(
        &'a mut self,
        plan: erpx_server_operations::public::staff_tasks::RevokeDelegationPlan,
    ) -> StaffTaskFuture<'a, TaskReceipt, Self::Error> {
        Box::pin(async move {
            let row = self.tasks.get_mut(&plan.task_id).unwrap();
            row.version = plan.version;
            row.active_delegation_id = None;
            row.active_delegate_user_id = None;
            self.store(
                plan.key.as_str(),
                &plan.receipt,
                &plan.operation,
                &plan.record_id,
                &plan.request_fingerprint,
                &plan.company_id,
                None,
                None,
            );
            Ok(plan.receipt)
        })
    }
    fn alert_rows<'a>(
        &'a mut self,
        _now: i64,
        _window: i64,
    ) -> StaffTaskFuture<'a, Vec<AlertTask>, Self::Error> {
        Box::pin(async move { Ok(self.alert_rows.clone()) })
    }
    fn stage_alert<'a>(&'a mut self, plan: AlertPlan) -> StaffTaskFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            Ok(self.alerts.insert(format!(
                "{}-{}-{}-{}",
                plan.task_id, plan.version, plan.user_id, plan.phase
            )))
        })
    }
}

struct Sources;
impl StaffTaskSources for Sources {
    type Error = BusinessError;
    fn active_staff<'a>(
        &'a mut self,
        _company: &'a str,
        user: &'a str,
    ) -> StaffTaskFuture<'a, Option<StaffFact>, Self::Error> {
        Box::pin(async move {
            Ok(match user {
                "creator" | "worker" | "delegate" | "head" => Some(StaffFact {
                    branch_id: Some("branch".into()),
                    department_id: Some("department".into()),
                }),
                _ => None,
            })
        })
    }
    fn project<'a>(
        &'a mut self,
        _company: &'a str,
        _id: &'a str,
    ) -> StaffTaskFuture<'a, Option<ProjectFact>, Self::Error> {
        Box::pin(async move { Ok(None) })
    }
    fn same_person<'a>(
        &'a mut self,
        actor: &'a StaffTaskActor,
        user: &'a str,
    ) -> StaffTaskFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(actor.user_id == user) })
    }
    fn same_person_between<'a>(
        &'a mut self,
        _company: &'a str,
        a: &'a str,
        b: &'a str,
    ) -> StaffTaskFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(a == b) })
    }
    fn readable_post<'a>(
        &'a mut self,
        _actor: &'a StaffTaskActor,
        _id: &'a str,
    ) -> StaffTaskFuture<'a, Option<PostFact>, Self::Error> {
        Box::pin(async move { Ok(None) })
    }
    fn task_route<'a>(
        &'a mut self,
        _scope: &'a erpx_server_operations::public::staff_tasks::TaskScope,
        _creator: &'a str,
        _assignee: &'a str,
        _phase: &'a str,
        _approvers: &'a [String],
        _service: bool,
    ) -> StaffTaskFuture<'a, RouteFact, Self::Error> {
        Box::pin(async move {
            Ok(RouteFact {
                blocked_reason: None,
            })
        })
    }
    fn approval_view<'a>(
        &'a mut self,
        _actor: &'a StaffTaskActor,
        _task: &'a TaskRow,
    ) -> StaffTaskFuture<'a, ApprovalView, Self::Error> {
        Box::pin(async move {
            Ok(ApprovalView {
                state: "pending".into(),
                phase: None,
                can_decide: false,
                blocked_reason: None,
                steps: serde_json::json!([]),
            })
        })
    }
    fn open_approval<'a>(
        &'a mut self,
        _key: &'a str,
        _task: &'a TaskRow,
        _phase: &'a str,
        _route: &'a RouteFact,
        _version: i64,
    ) -> StaffTaskFuture<'a, (), Self::Error> {
        Box::pin(async move { Ok(()) })
    }
    fn prepare_decision<'a>(
        &'a mut self,
        _key: &'a str,
        _task: &'a TaskRow,
        _approve: bool,
        _note: &'a str,
        _version: i64,
    ) -> StaffTaskFuture<'a, DecisionFact, Self::Error> {
        Box::pin(async move {
            Ok(DecisionFact {
                phase: "completion".into(),
                final_decision: true,
            })
        })
    }
    fn close_approvals<'a>(
        &'a mut self,
        _key: &'a str,
        _task: &'a str,
        _reason: &'a str,
        _version: i64,
    ) -> StaffTaskFuture<'a, (), Self::Error> {
        Box::pin(async move { Ok(()) })
    }
    fn attachments<'a>(
        &'a mut self,
        _actor: &'a StaffTaskActor,
        _task: &'a str,
    ) -> StaffTaskFuture<'a, Vec<AttachmentRow>, Self::Error> {
        Box::pin(async move { Ok(Vec::new()) })
    }
    fn person_name<'a>(
        &'a mut self,
        _company: &'a str,
        _user: &'a str,
    ) -> StaffTaskFuture<'a, Option<String>, Self::Error> {
        Box::pin(async move { Ok(None) })
    }
    fn feature_enabled<'a>(
        &'a mut self,
        _company: &'a str,
    ) -> StaffTaskFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(true) })
    }
}

fn actor(user: &str) -> StaffTaskActor {
    StaffTaskActor {
        user_id: user.into(),
        assignment_id: format!("assignment-{user}"),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: Some("department".into()),
        permissions: vec!["records.read".into(), "records.write".into()],
        modules: vec!["operations".into()],
        expires_at: 9_999_999_999_999,
    }
}
fn save(title: &str) -> SaveTaskCommand {
    SaveTaskCommand {
        title: title.into(),
        description: String::new(),
        assignee_user_id: "worker".into(),
        project_id: None,
        start_approvers: Vec::new(),
        completion_approvers: Vec::new(),
        due_at: Some("2026-10-01T02:00:00Z".into()),
        expected_version: 1,
        reason: String::new(),
    }
}

#[tokio::test]
async fn save_replays_the_same_key_and_conflicts_on_a_different_body() {
    let now = 1_790_000_000_000;
    let mut repo = Repo::default();
    let mut sources = Sources;
    let created = save_task(
        &actor("creator"),
        &mut repo,
        &mut sources,
        None,
        &save("Inspect deliveries"),
        "key-0001",
        false,
        now,
    )
    .await
    .unwrap();
    assert_eq!(created, TaskReceipt::new("task-key-0001", "ready", 1));
    assert_eq!(repo.writes, 1);
    let replay = save_task(
        &actor("creator"),
        &mut repo,
        &mut sources,
        None,
        &save("Inspect deliveries"),
        "key-0001",
        false,
        now,
    )
    .await
    .unwrap();
    assert_eq!(replay, created);
    assert_eq!(repo.writes, 1, "a replay never writes a second owner row");
    let conflict = save_task(
        &actor("creator"),
        &mut repo,
        &mut sources,
        None,
        &save("Different title"),
        "key-0001",
        false,
        now,
    )
    .await
    .unwrap_err();
    assert_eq!(conflict.code, "IDEMPOTENCY_CONFLICT");
}

#[tokio::test]
async fn save_requires_a_current_assignee_and_visibility_for_reads() {
    let now = 1_790_000_000_000;
    let mut repo = Repo::default();
    let mut sources = Sources;
    let mut input = save("Inspect deliveries");
    input.assignee_user_id = "outsider".into();
    let error = save_task(
        &actor("creator"),
        &mut repo,
        &mut sources,
        None,
        &input,
        "key-0002",
        false,
        now,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "TASK_ASSIGNEE_INVALID");
    save_task(
        &actor("creator"),
        &mut repo,
        &mut sources,
        None,
        &save("Inspect deliveries"),
        "key-0003",
        false,
        now,
    )
    .await
    .unwrap();
    let tasks = list_tasks(&actor("creator"), &mut repo, &mut sources, None, now)
        .await
        .unwrap();
    assert_eq!(tasks.len(), 1);
    assert!(
        list_tasks(&actor("outsider"), &mut repo, &mut sources, None, now)
            .await
            .unwrap()
            .is_empty(),
        "an unrelated actor sees no owner task"
    );
    let detail = task_detail(
        &actor("creator"),
        &mut repo,
        &mut sources,
        "task-key-0003",
        now,
    )
    .await
    .unwrap();
    assert_eq!(detail.task.id, "task-key-0003");
    assert!(!detail.permitted_actions.contains(&"start".to_owned()));
}

#[tokio::test]
async fn transition_enforces_current_version_and_records_the_state() {
    let now = 1_790_000_000_000;
    let mut repo = Repo::default();
    let mut sources = Sources;
    save_task(
        &actor("creator"),
        &mut repo,
        &mut sources,
        None,
        &save("Inspect deliveries"),
        "key-0004",
        false,
        now,
    )
    .await
    .unwrap();
    let action = TransitionTaskCommand {
        action: TaskAction::Start,
        expected_version: 1,
        note: String::new(),
        attachment_set_ids: Vec::new(),
    };
    let started = transition_task(
        &actor("worker"),
        &mut repo,
        &mut sources,
        "task-key-0004",
        &action,
        "key-0005",
        now,
    )
    .await
    .unwrap();
    assert_eq!(started.state, "in_progress");
    let mut complete = TransitionTaskCommand {
        action: TaskAction::Complete,
        expected_version: 1,
        note: String::new(),
        attachment_set_ids: Vec::new(),
    };
    let stale = transition_task(
        &actor("worker"),
        &mut repo,
        &mut sources,
        "task-key-0004",
        &complete,
        "key-0006",
        now,
    )
    .await
    .unwrap_err();
    assert_eq!(stale.code, "TASK_VERSION_CHANGED");
    complete.expected_version = 2;
    assert_eq!(
        transition_task(
            &actor("worker"),
            &mut repo,
            &mut sources,
            "task-key-0004",
            &complete,
            "key-0007",
            now
        )
        .await
        .unwrap()
        .state,
        "completed"
    );
    complete.expected_version = 3;
    assert_eq!(
        transition_task(
            &actor("worker"),
            &mut repo,
            &mut sources,
            "task-key-0004",
            &complete,
            "key-0008",
            now
        )
        .await
        .unwrap_err()
        .code,
        "TASK_TERMINAL"
    );
}

#[tokio::test]
async fn delegation_and_revocation_are_owner_commands() {
    let now = 1_790_000_000_000;
    let mut repo = Repo::default();
    let mut sources = Sources;
    save_task(
        &actor("creator"),
        &mut repo,
        &mut sources,
        None,
        &save("Inspect deliveries"),
        "key-0009",
        false,
        now,
    )
    .await
    .unwrap();
    let delegate = DelegateTaskCommand {
        delegate_user_id: "delegate".into(),
        expires_at: "2026-12-01T00:00:00Z".into(),
        expected_version: 1,
        reason: "Annual leave".into(),
    };
    let delegated = delegate_task(
        &actor("creator"),
        &mut repo,
        &mut sources,
        "task-key-0009",
        &delegate,
        "key-0010",
        now,
    )
    .await
    .unwrap();
    assert_eq!(delegated.state, "delegated");
    assert_eq!(delegated.version, 2);
    let revoke = RevokeTaskDelegationCommand {
        delegation_id: "delegation-1".into(),
        expected_version: 2,
        reason: "Returned".into(),
    };
    let revoked = revoke_task_delegation(
        &actor("creator"),
        &mut repo,
        &mut sources,
        "task-key-0009",
        &revoke,
        "key-0011",
        now,
    )
    .await
    .unwrap();
    assert_eq!(revoked.state, "revoked");
    assert_eq!(
        revoke_task_delegation(
            &actor("creator"),
            &mut repo,
            &mut sources,
            "task-key-0009",
            &revoke,
            "key-0012",
            now
        )
        .await
        .unwrap(),
        revoked,
        "an idempotent replay returns the retained receipt"
    );
}

#[allow(clippy::field_reassign_with_default)]
#[tokio::test]
async fn alerts_are_counted_once_per_task_version_and_user() {
    let now = 1_790_000_000_000;
    let mut repo = Repo::default();
    repo.alert_rows = vec![AlertTask {
        id: "task-alert".into(),
        company_id: "company".into(),
        title: "Review receiving exception".into(),
        due_at: now + 3_600_000,
        version: 4,
        assignee_user_id: "worker".into(),
        creator_user_id: "creator".into(),
        project_id: None,
        project_owner_id: None,
        delegate_user_id: None,
        approvers: vec!["head".into()],
    }];
    let mut sources = Sources;
    let first = escalate_staff_tasks(&mut repo, &mut sources, now, 86_400_000)
        .await
        .unwrap();
    assert_eq!(first.due_soon, 2);
    assert_eq!(first.overdue, 0);
    let second = escalate_staff_tasks(&mut repo, &mut sources, now, 86_400_000)
        .await
        .unwrap();
    assert_eq!(second.due_soon, 0);
    assert_eq!(second.overdue, 0);
}

#[allow(dead_code)]
fn facts() -> TaskAccessFacts {
    TaskAccessFacts::default()
}
