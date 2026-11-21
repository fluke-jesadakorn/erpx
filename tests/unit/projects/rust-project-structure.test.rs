//! Native Projects work-structure and handoff command policy tests.
//!
//! These tests prove the owner decisions, CAS/version checks, independent
//! review rules, handoff snapshot/digest and replay contract against a fake
//! repository. Persisted PostgreSQL effects are covered by the project
//! structure integration target.
use erpx_server_projects::public::project_commands::ProjectCommandActor;
use erpx_server_projects::public::project_structure::{
    ActualSourceKind, ApprovedCost, ApprovedTime, ProjectHandoffFact, ProjectPackageFact,
    ProjectStructureActor, ProjectStructureCommand, ProjectStructureCommandRepository,
    ProjectStructureFuture, ProjectStructureReceipt, ProjectStructureTask, ProjectTaskBinding,
    StoredStructureReceipt, StructurePlan, StructurePlanAction, execute_structure_command,
};
use erpx_server_projects::public::project_graph::{GraphActual, GraphTask};
use erpx_server_projects::public::project_wbs::{MilestoneRow, ProjectRow};
use erpx_server_shared::public::{
    BusinessError, CompanyId, RecordId, TimestampMillis,
};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);
const KEY: &str = "key-structure-0001";

fn actor(user: &str, permissions: &[&str]) -> ProjectStructureActor {
    ProjectStructureActor {
        command: ProjectCommandActor {
            user_id: user.into(),
            company_id: CompanyId::new("company").unwrap(),
            branch_id: None,
            department_id: None,
            assignment_id: "assignment".into(),
            permissions: permissions.iter().map(|value| (*value).to_owned()).collect(),
            modules: vec!["projects".into()],
            expires_at: NOW.get() + 60_000,
            active_staff: true,
            wbs_new_work: true,
            wbs_settle_existing: true,
        },
        cost_history: true,
    }
}

fn project(lifecycle: &str, owner: &str) -> ProjectRow {
    ProjectRow {
        id: "project".into(),
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
        title: "Project".into(),
        description: "Current project".into(),
        owner_user_id: owner.into(),
        lifecycle_status: lifecycle.into(),
        version: 4,
        updated_at: 10,
        is_owner: owner != "member",
        is_member: true,
    }
}

fn command_project(lifecycle: &str, owner: &str) -> erpx_server_projects::public::project_commands::ProjectCommandProject {
    erpx_server_projects::public::project_commands::ProjectCommandProject {
        id: "project".into(),
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
        owner_user_id: owner.into(),
        lifecycle_status: lifecycle.into(),
        version: 4,
        is_owner: owner != "member",
        is_member: true,
    }
}

fn milestone() -> MilestoneRow {
    MilestoneRow {
        id: "m-root".into(),
        company_id: "company".into(),
        project_id: "project".into(),
        parent_milestone_id: None,
        ordinal_position: 0,
        title: "Delivery".into(),
        description: "Current package".into(),
        due_at: 1_000,
        owner_user_id: "owner".into(),
        owner_name: None,
        state: "planned".into(),
        is_overdue: false,
        version: 1,
        created_by: "owner".into(),
        completed_at: None,
        created_at: 10,
        updated_at: 10,
        depends_on_ids: Vec::new(),
        depends_on_titles: Vec::new(),
        blocked_by_titles: Vec::new(),
        blocked_by_count: 0,
    }
}

fn task() -> GraphTask {
    GraphTask {
        id: "task".into(),
        title: "Task".into(),
        state: "in_progress".into(),
        version: 2,
        assignee_user_id: "member".into(),
        milestone_id: Some("m-root".into()),
        binding_version: Some(1),
    }
}

fn actual() -> GraphActual {
    GraphActual {
        id: "actual".into(),
        milestone_id: "m-root".into(),
        source_kind: "work_log".into(),
        source_id: "time".into(),
        source_version: 1,
        minutes: Some(60),
        amount_minor: None,
        currency: None,
        user_id: Some("member".into()),
        current_source_version: 1,
        source_state: "approved".into(),
        journal_status: None,
        journal_id: None,
        is_actor: false,
    }
}

fn handoff(status: &str) -> ProjectHandoffFact {
    ProjectHandoffFact {
        id: "handoff".into(),
        status: status.into(),
        version: 2,
        recipient_id: "member".into(),
        prepared_by: "owner".into(),
        snapshot_sha256: "a".repeat(64),
    }
}

#[derive(Default)]
struct Repo {
    project: Option<erpx_server_projects::public::project_commands::ProjectCommandProject>,
    stored: Option<StoredStructureReceipt>,
    task: Option<ProjectStructureTask>,
    binding: Option<ProjectTaskBinding>,
    package: Option<ProjectPackageFact>,
    time: Option<ApprovedTime>,
    cost: Option<ApprovedCost>,
    ready: bool,
    recipient: bool,
    handoff: Option<ProjectHandoffFact>,
    commits: usize,
    last: Option<StructurePlan>,
}

impl ProjectStructureCommandRepository for Repo {
    type Error = BusinessError;

    fn stored_receipt<'a>(
        &'a mut self,
        _: &'a str,
    ) -> ProjectStructureFuture<'a, Option<StoredStructureReceipt>, Self::Error> {
        Box::pin(async move { Ok(self.stored.clone()) })
    }

    fn project<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> ProjectStructureFuture<'a, Option<erpx_server_projects::public::project_commands::ProjectCommandProject>, Self::Error> {
        Box::pin(async move { Ok(self.project.clone()) })
    }

    fn task<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> ProjectStructureFuture<'a, Option<ProjectStructureTask>, Self::Error> {
        Box::pin(async move { Ok(self.task.clone()) })
    }

    fn task_binding<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> ProjectStructureFuture<'a, Option<ProjectTaskBinding>, Self::Error> {
        Box::pin(async move { Ok(self.binding.clone()) })
    }

    fn package<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> ProjectStructureFuture<'a, Option<ProjectPackageFact>, Self::Error> {
        Box::pin(async move { Ok(self.package.clone()) })
    }

    fn approved_time<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
        _: i64,
    ) -> ProjectStructureFuture<'a, Option<ApprovedTime>, Self::Error> {
        Box::pin(async move { Ok(self.time.clone()) })
    }

    fn approved_cost<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
        _: i64,
    ) -> ProjectStructureFuture<'a, Option<ApprovedCost>, Self::Error> {
        Box::pin(async move { Ok(self.cost.clone()) })
    }

    fn handoff_readiness<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> ProjectStructureFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(self.ready) })
    }

    fn handoff<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> ProjectStructureFuture<'a, Option<ProjectHandoffFact>, Self::Error> {
        Box::pin(async move { Ok(self.handoff.clone()) })
    }

    fn recipient_member<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> ProjectStructureFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(self.recipient) })
    }

    fn snapshot_rows<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> ProjectStructureFuture<
        'a,
        (ProjectRow, Vec<MilestoneRow>, Vec<GraphTask>, Vec<GraphActual>),
        Self::Error,
    > {
        Box::pin(async move {
            Ok((
                project("active", "owner"),
                vec![milestone()],
                vec![task()],
                vec![actual()],
            ))
        })
    }

    fn commit<'a>(
        &'a mut self,
        plan: StructurePlan,
    ) -> ProjectStructureFuture<'a, ProjectStructureReceipt, Self::Error> {
        Box::pin(async move {
            self.commits += 1;
            self.last = Some(plan.clone());
            Ok(plan.result)
        })
    }
}

fn repo() -> Repo {
    Repo {
        project: Some(command_project("active", "owner")),
        task: Some(ProjectStructureTask {
            id: "task".into(),
            version: 2,
            project_id: "project".into(),
        }),
        binding: Some(ProjectTaskBinding {
            milestone_id: "m-root".into(),
            version: 1,
        }),
        package: Some(ProjectPackageFact {
            id: "m-root".into(),
            version: 3,
            state: "in_progress".into(),
        }),
        ..Repo::default()
    }
}

#[tokio::test]
async fn assign_task_bumps_binding_version_and_commits_project() {
    let mut repository = repo();
    let input = ProjectStructureCommand::AssignTask {
        expected_version: 4,
        task_id: "task".into(),
        task_version: 2,
        milestone_id: "m-root".into(),
        milestone_version: 3,
        binding_version: 1,
        reason: "Assign the current task.".into(),
    };
    let receipt = execute_structure_command(
        &actor("owner", &["records.write"]),
        &mut repository,
        "project",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(receipt.version, 5);
    assert_eq!(receipt.task_id.as_deref(), Some("task"));
    assert_eq!(receipt.binding_version, Some(2));
    let plan = repository.last.unwrap();
    assert!(matches!(
        plan.action,
        StructurePlanAction::AssignTask { .. }
    ));
    assert_eq!(plan.next_version, 5);
}

#[tokio::test]
async fn member_cannot_change_structure() {
    let mut repository = repo();
    repository.project = Some(command_project("active", "owner"));
    let input = ProjectStructureCommand::AssignTask {
        expected_version: 4,
        task_id: "task".into(),
        task_version: 2,
        milestone_id: "m-root".into(),
        milestone_version: 3,
        binding_version: 1,
        reason: "Assign the current task.".into(),
    };
    let error = execute_structure_command(
        &actor("member", &["records.write"]),
        &mut repository,
        "project",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_OWNER_REQUIRED");
    assert_eq!(repository.commits, 0);
}

#[tokio::test]
async fn stale_project_version_is_a_conflict() {
    let mut repository = repo();
    let input = ProjectStructureCommand::UnassignTask {
        expected_version: 3,
        task_id: "task".into(),
        task_version: 2,
        binding_version: 1,
        reason: "Remove the task from its package.".into(),
    };
    let error = execute_structure_command(
        &actor("owner", &["records.write"]),
        &mut repository,
        "project",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CONFLICT");
}

#[tokio::test]
async fn stale_task_binding_is_rejected() {
    let mut repository = repo();
    let input = ProjectStructureCommand::UnassignTask {
        expected_version: 4,
        task_id: "task".into(),
        task_version: 2,
        binding_version: 2,
        reason: "Remove the task from its package.".into(),
    };
    let error = execute_structure_command(
        &actor("owner", &["records.write"]),
        &mut repository,
        "project",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_TASK_BINDING_CHANGED");
}

#[tokio::test]
async fn link_requires_an_approved_time_source() {
    let mut repository = repo();
    let input = ProjectStructureCommand::LinkActual {
        expected_version: 4,
        milestone_id: "m-root".into(),
        milestone_version: 3,
        source_kind: "work_log".into(),
        source_id: "time".into(),
        source_version: 1,
        reason: "Link the approved time.".into(),
    };
    let error = execute_structure_command(
        &actor("owner", &["records.write"]),
        &mut repository,
        "project",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_TIME_SOURCE_CHANGED");
}

#[tokio::test]
async fn cost_link_requires_finance_write() {
    let mut repository = repo();
    repository.cost = Some(ApprovedCost {
        amount_minor: 1_000,
        currency: "USD".into(),
    });
    let input = ProjectStructureCommand::LinkActual {
        expected_version: 4,
        milestone_id: "m-root".into(),
        milestone_version: 3,
        source_kind: "cost_allocation".into(),
        source_id: "cost".into(),
        source_version: 1,
        reason: "Link the approved cost.".into(),
    };
    let error = execute_structure_command(
        &actor("owner", &["records.write", "finance.read"]),
        &mut repository,
        "project",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "FINANCE_PERMISSION_REQUIRED");
}

#[tokio::test]
async fn handoff_requires_an_independent_recipient() {
    let mut repository = repo();
    let input = ProjectStructureCommand::PrepareHandoff {
        expected_version: 4,
        recipient_id: "owner".into(),
        evidence: "All work is complete.".into(),
        reason: "Prepare the independent handoff.".into(),
    };
    let error = execute_structure_command(
        &actor("owner", &["records.write"]),
        &mut repository,
        "project",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "INDEPENDENT_HANDOFF_REQUIRED");
}

#[tokio::test]
async fn handoff_requires_ready_work() {
    let mut repository = repo();
    repository.recipient = true;
    let input = ProjectStructureCommand::PrepareHandoff {
        expected_version: 4,
        recipient_id: "member".into(),
        evidence: "All work is complete.".into(),
        reason: "Prepare the independent handoff.".into(),
    };
    let error = execute_structure_command(
        &actor("owner", &["records.write"]),
        &mut repository,
        "project",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_HANDOFF_INCOMPLETE");
}

#[tokio::test]
async fn prepare_handoff_retains_a_bounded_snapshot_digest() {
    let mut repository = repo();
    repository.recipient = true;
    repository.ready = true;
    let input = ProjectStructureCommand::PrepareHandoff {
        expected_version: 4,
        recipient_id: "member".into(),
        evidence: "All work is complete.".into(),
        reason: "Prepare the independent handoff.".into(),
    };
    let receipt = execute_structure_command(
        &actor("owner", &["records.write"]),
        &mut repository,
        "project",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(receipt.status, "handoff_pending");
    assert_eq!(receipt.snapshot_sha256.as_ref().unwrap().len(), 64);
    assert!(receipt.handoff_id.as_deref().unwrap().starts_with("project-handoff-"));
    match repository.last.unwrap().action {
        StructurePlanAction::PrepareHandoff {
            snapshot_json,
            snapshot_sha256,
            ..
        } => {
            assert!(snapshot_json.contains("\"packages\""));
            assert_eq!(snapshot_sha256.len(), 64);
        }
        other => panic!("unexpected plan {other:?}"),
    }
}

#[tokio::test]
async fn preparer_cannot_accept_a_handoff() {
    let mut repository = repo();
    repository.project = Some(command_project("handoff_pending", "owner"));
    repository.handoff = Some(handoff("pending"));
    let input = ProjectStructureCommand::AcceptHandoff {
        expected_version: 4,
        handoff_id: "handoff".into(),
        handoff_version: 2,
        reason: "Accept the completed project.".into(),
    };
    let error = execute_structure_command(
        &actor("owner", &["records.write"]),
        &mut repository,
        "project",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_RECIPIENT_REQUIRED");
}

#[tokio::test]
async fn recipient_accept_completes_the_project() {
    let mut repository = repo();
    repository.project = Some(command_project("handoff_pending", "owner"));
    repository.handoff = Some(handoff("pending"));
    let input = ProjectStructureCommand::AcceptHandoff {
        expected_version: 4,
        handoff_id: "handoff".into(),
        handoff_version: 2,
        reason: "Accept the completed project.".into(),
    };
    let receipt = execute_structure_command(
        &actor("member", &["records.write"]),
        &mut repository,
        "project",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(receipt.status, "completed");
    assert_eq!(receipt.snapshot_sha256.as_ref().unwrap().len(), 64);
}

#[tokio::test]
async fn only_the_preparer_cancels_a_pending_handoff() {
    let mut repository = repo();
    repository.project = Some(command_project("handoff_pending", "owner"));
    repository.handoff = Some(handoff("pending"));
    let input = ProjectStructureCommand::CancelHandoff {
        expected_version: 4,
        handoff_id: "handoff".into(),
        handoff_version: 2,
        reason: "Withdraw the pending handoff.".into(),
    };
    let error = execute_structure_command(
        &actor("member", &["records.write"]),
        &mut repository,
        "project",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_HANDOFF_PREPARER_REQUIRED");
}

#[tokio::test]
async fn recipient_reopens_an_accepted_project() {
    let mut repository = repo();
    repository.project = Some(command_project("completed", "owner"));
    repository.handoff = Some(handoff("accepted"));
    let input = ProjectStructureCommand::Reopen {
        expected_version: 4,
        handoff_id: "handoff".into(),
        handoff_version: 2,
        reason: "Reopen the project for correction.".into(),
    };
    let receipt = execute_structure_command(
        &actor("member", &["records.write"]),
        &mut repository,
        "project",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(receipt.status, "active");
    assert!(matches!(
        repository.last.unwrap().action,
        StructurePlanAction::Reopen { .. }
    ));
}

#[tokio::test]
async fn owner_cannot_reopen_an_accepted_project() {
    let mut repository = repo();
    repository.project = Some(command_project("completed", "owner"));
    repository.handoff = Some(handoff("accepted"));
    let input = ProjectStructureCommand::Reopen {
        expected_version: 4,
        handoff_id: "handoff".into(),
        handoff_version: 2,
        reason: "Reopen the project for correction.".into(),
    };
    let error = execute_structure_command(
        &actor("owner", &["records.write"]),
        &mut repository,
        "project",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_RECIPIENT_REOPEN_REQUIRED");
}

#[tokio::test]
async fn replay_returns_the_retained_receipt_without_committing() {
    let mut repository = repo();
    let prior = ProjectStructureReceipt {
        id: "project".into(),
        version: 5,
        status: "active".into(),
        task_id: Some("task".into()),
        binding_version: Some(2),
        actual_id: None,
        handoff_id: None,
        snapshot_sha256: None,
    };
    let input = ProjectStructureCommand::AssignTask {
        expected_version: 4,
        task_id: "task".into(),
        task_version: 2,
        milestone_id: "m-root".into(),
        milestone_version: 3,
        binding_version: 1,
        reason: "Assign the current task.".into(),
    };
    let fingerprint = erpx_server_projects::public::project_structure::structure_fingerprint(
        &input,
        "project",
    )
    .unwrap();
    repository.stored = Some(StoredStructureReceipt {
        operation: "project.structure.assign-task".into(),
        record_id: "project".into(),
        request_fingerprint: fingerprint,
        result_json: Some(serde_json::to_string(&prior).unwrap()),
    });
    let receipt = execute_structure_command(
        &actor("owner", &["records.write"]),
        &mut repository,
        "project",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(receipt, prior);
    assert_eq!(repository.commits, 0);
}

#[tokio::test]
async fn reused_key_with_a_different_request_is_a_conflict() {
    let mut repository = repo();
    let prior = ProjectStructureReceipt {
        id: "project".into(),
        version: 5,
        status: "active".into(),
        task_id: None,
        binding_version: None,
        actual_id: None,
        handoff_id: None,
        snapshot_sha256: None,
    };
    repository.stored = Some(StoredStructureReceipt {
        operation: "project.structure.assign-task".into(),
        record_id: "project".into(),
        request_fingerprint: "different".into(),
        result_json: Some(serde_json::to_string(&prior).unwrap()),
    });
    let input = ProjectStructureCommand::UnassignTask {
        expected_version: 4,
        task_id: "task".into(),
        task_version: 2,
        binding_version: 1,
        reason: "Remove the task from its package.".into(),
    };
    let error = execute_structure_command(
        &actor("owner", &["records.write"]),
        &mut repository,
        "project",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "COMMAND_KEY_CONFLICT");
}

#[test]
fn health_source_kind_round_trips() {
    assert_eq!(
        ActualSourceKind::parse("work_log").unwrap().as_str(),
        "work_log"
    );
    assert!(ActualSourceKind::parse("journal").is_err());
    let _ = RecordId::new("project").unwrap();
}
