//! Native Projects milestone command policy, replay and validation tests.
use erpx_server_projects::public::project_commands::{
    DependencyFact, MilestoneAction, MilestoneCommandReceipt, ParentMilestone,
    ProjectCommandActor, ProjectCommandFuture, ProjectCommandMilestone, ProjectCommandProject,
    ProjectMilestoneCommandRepository, SaveMilestoneCommand, SaveMilestonePlan,
    StoredMilestoneReceipt, TransitionMilestoneCommand, TransitionMilestonePlan, save_milestone,
    transition_milestone,
};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, TimestampMillis,
};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor() -> ProjectCommandActor {
    ProjectCommandActor {
        user_id: "owner".into(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: Some(erpx_server_shared::public::RecordId::new("branch").unwrap()),
        department_id: Some(erpx_server_shared::public::RecordId::new("department").unwrap()),
        assignment_id: "assignment".into(),
        permissions: vec!["records.write".into()],
        modules: vec!["projects".into()],
        expires_at: NOW.get() + 60_000,
        active_staff: true,
        wbs_new_work: true,
        wbs_settle_existing: true,
    }
}

fn project() -> ProjectCommandProject {
    ProjectCommandProject {
        id: "project".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: Some("department".into()),
        owner_user_id: "owner".into(),
        lifecycle_status: "active".into(),
        version: 4,
        is_owner: true,
        is_member: true,
    }
}

fn milestone() -> ProjectCommandMilestone {
    ProjectCommandMilestone {
        id: "m-child".into(),
        company_id: "company".into(),
        project_id: "project".into(),
        parent_milestone_id: None,
        ordinal_position: 1,
        title: "Delivery".into(),
        description: "Current milestone".into(),
        due_at: 1_000,
        owner_user_id: "owner".into(),
        state: "planned".into(),
        version: 2,
        completed_at: None,
    }
}

fn save_input() -> SaveMilestoneCommand {
    SaveMilestoneCommand {
        title: "Delivery".into(),
        description: "Bounded milestone".into(),
        due_at: 2_000,
        owner_user_id: "owner".into(),
        parent_milestone_id: None,
        parent_supplied: true,
        ordinal_position: Some(2),
        depends_on_milestone_ids: Some(Vec::new()),
        expected_version: 1,
        reason: "Milestone detail reviewed by the project owner.".into(),
    }
}

fn transition_input(action: MilestoneAction) -> TransitionMilestoneCommand {
    TransitionMilestoneCommand {
        action,
        expected_version: 2,
        reason: "Milestone transition reviewed by the project owner.".into(),
    }
}

#[derive(Default)]
struct Repo {
    project: Option<ProjectCommandProject>,
    milestone: Option<ProjectCommandMilestone>,
    member: bool,
    parent: Option<ParentMilestone>,
    parent_cycle: bool,
    dependencies: Vec<String>,
    dependency_facts: Vec<DependencyFact>,
    dependency_cycle: bool,
    incomplete: Vec<String>,
    active_successor: bool,
    completed_successor: bool,
    stored: Option<StoredMilestoneReceipt>,
    next_receipt: Option<MilestoneCommandReceipt>,
    saves: usize,
    transitions: usize,
    calls: usize,
    saved: Option<SaveMilestonePlan>,
    transitioned: Option<TransitionMilestonePlan>,
}

impl ProjectMilestoneCommandRepository for Repo {
    type Error = BusinessError;

    fn stored_receipt<'a>(
        &'a mut self,
        _: &'a str,
    ) -> ProjectCommandFuture<'a, Option<StoredMilestoneReceipt>, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.stored.clone()) })
    }

    fn project<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> ProjectCommandFuture<'a, Option<ProjectCommandProject>, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.project.clone()) })
    }

    fn milestone<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> ProjectCommandFuture<'a, Option<ProjectCommandMilestone>, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.milestone.clone()) })
    }

    fn member<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> ProjectCommandFuture<'a, bool, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.member) })
    }

    fn parent<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> ProjectCommandFuture<'a, Option<ParentMilestone>, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.parent.clone()) })
    }

    fn parent_cycle<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
        _: &'a str,
    ) -> ProjectCommandFuture<'a, bool, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.parent_cycle) })
    }

    fn dependencies<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> ProjectCommandFuture<'a, Vec<String>, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.dependencies.clone()) })
    }

    fn dependency_facts<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a [String],
    ) -> ProjectCommandFuture<'a, Vec<DependencyFact>, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.dependency_facts.clone()) })
    }

    fn dependency_cycle<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
        _: &'a [String],
    ) -> ProjectCommandFuture<'a, bool, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.dependency_cycle) })
    }

    fn incomplete_prerequisites<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> ProjectCommandFuture<'a, Vec<String>, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.incomplete.clone()) })
    }

    fn active_successor<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> ProjectCommandFuture<'a, bool, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.active_successor) })
    }

    fn completed_successor<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> ProjectCommandFuture<'a, bool, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.completed_successor) })
    }

    fn save<'a>(
        &'a mut self,
        plan: SaveMilestonePlan,
    ) -> ProjectCommandFuture<'a, MilestoneCommandReceipt, Self::Error> {
        self.saves += 1;
        self.saved = Some(plan.clone());
        Box::pin(async move {
            Ok(self.next_receipt.clone().unwrap_or(MilestoneCommandReceipt {
                id: plan.milestone_id,
                version: plan.version,
                state: None,
            }))
        })
    }

    fn transition<'a>(
        &'a mut self,
        plan: TransitionMilestonePlan,
    ) -> ProjectCommandFuture<'a, MilestoneCommandReceipt, Self::Error> {
        self.transitions += 1;
        let receipt = self.next_receipt.clone().unwrap_or(MilestoneCommandReceipt {
            id: plan.milestone.id.clone(),
            version: plan.version,
            state: Some(plan.next_state.clone()),
        });
        self.transitioned = Some(plan.clone());
        Box::pin(async move { Ok(receipt) })
    }
}

fn repo() -> Repo {
    Repo {
        project: Some(project()),
        milestone: Some(milestone()),
        member: true,
        ..Repo::default()
    }
}

#[tokio::test]
async fn command_policy_requires_write_module_staff_and_current_context() {
    let mut denied = actor();
    denied.permissions.clear();
    let mut repository = repo();
    let error = save_milestone(&denied, &mut repository, "project", None, &save_input(), "command-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Forbidden);
    assert_eq!(error.code, "PERMISSION_REQUIRED");
    assert_eq!(repository.calls, 0);

    let mut module = actor();
    module.modules = vec!["finance".into()];
    let mut repository = repo();
    let error = save_milestone(&module, &mut repository, "project", None, &save_input(), "command-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "MODULE_FORBIDDEN");

    let mut expired = actor();
    expired.expires_at = NOW.get();
    let mut repository = repo();
    let error = save_milestone(&expired, &mut repository, "project", None, &save_input(), "command-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");

    let mut staff = actor();
    staff.active_staff = false;
    let mut repository = repo();
    let error = save_milestone(&staff, &mut repository, "project", None, &save_input(), "command-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "STAFF_LINK_REQUIRED");

    let mut purchase = actor();
    purchase.wbs_new_work = false;
    let mut repository = repo();
    let error = save_milestone(&purchase, &mut repository, "project", None, &save_input(), "command-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PACKAGE_REQUIRED");
}

#[tokio::test]
async fn command_rejects_invalid_input_and_missing_sources() {
    let mut invalid = save_input();
    invalid.title = " ".into();
    let mut repository = repo();
    let error = save_milestone(&actor(), &mut repository, "project", None, &invalid, "command-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PROJECT_MILESTONE_INVALID");

    let mut invalid_dependencies = save_input();
    invalid_dependencies.depends_on_milestone_ids = Some(vec!["a".into(), "a".into()]);
    let mut repository = repo();
    let error = save_milestone(
        &actor(),
        &mut repository,
        "project",
        None,
        &invalid_dependencies,
        "command-1",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_DEPENDENCY_INVALID");

    let mut missing = repo();
    missing.project = None;
    let error = save_milestone(&actor(), &mut missing, "project", None, &save_input(), "command-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PROJECT_NOT_FOUND");

    let mut hidden = repo();
    hidden.project.as_mut().unwrap().is_owner = false;
    hidden.project.as_mut().unwrap().is_member = false;
    let error = save_milestone(&actor(), &mut hidden, "project", None, &save_input(), "command-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PROJECT_NOT_FOUND");
}

#[tokio::test]
async fn create_command_checks_owner_member_parent_and_dependencies() {
    let mut non_owner = repo();
    non_owner.project.as_mut().unwrap().is_owner = false;
    let error = save_milestone(&actor(), &mut non_owner, "project", None, &save_input(), "command-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PROJECT_OWNER_REQUIRED");

    let mut frozen = repo();
    frozen.project.as_mut().unwrap().lifecycle_status = "completed".into();
    let error = save_milestone(&actor(), &mut frozen, "project", None, &save_input(), "command-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PROJECT_STRUCTURE_FROZEN");

    let mut not_member = repo();
    not_member.member = false;
    let error = save_milestone(&actor(), &mut not_member, "project", None, &save_input(), "command-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PROJECT_MEMBER_REQUIRED");

    let mut terminal_parent = repo();
    terminal_parent.parent = Some(ParentMilestone {
        id: "m-parent".into(),
        state: "completed".into(),
        version: 1,
    });
    let mut input = save_input();
    input.parent_milestone_id = Some("m-parent".into());
    let error = save_milestone(&actor(), &mut terminal_parent, "project", None, &input, "command-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PROJECT_PACKAGE_PARENT_TERMINAL");

    let mut missing_parent = repo();
    let error = save_milestone(&actor(), &mut missing_parent, "project", None, &input, "command-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PROJECT_PACKAGE_PARENT_INVALID");

    let mut cycle_parent = repo();
    cycle_parent.parent = Some(ParentMilestone {
        id: "m-parent".into(),
        state: "planned".into(),
        version: 1,
    });
    cycle_parent.parent_cycle = true;
    let error = save_milestone(&actor(), &mut cycle_parent, "project", None, &input, "command-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PROJECT_PACKAGE_CYCLE_OR_DEPTH");

    let mut cancelled_dependency = repo();
    let mut dependency_input = save_input();
    dependency_input.depends_on_milestone_ids = Some(vec!["m-pre".into()]);
    cancelled_dependency.dependency_facts = vec![DependencyFact {
        id: "m-pre".into(),
        state: "cancelled".into(),
    }];
    let error = save_milestone(
        &actor(),
        &mut cancelled_dependency,
        "project",
        None,
        &dependency_input,
        "command-1",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_DEPENDENCY_CANCELLED");

    let mut missing_dependency = repo();
    let error = save_milestone(
        &actor(),
        &mut missing_dependency,
        "project",
        None,
        &dependency_input,
        "command-1",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_DEPENDENCY_SCOPE_INVALID");

    let mut cycle_dependency = repo();
    cycle_dependency.dependency_facts = vec![DependencyFact {
        id: "m-pre".into(),
        state: "planned".into(),
    }];
    cycle_dependency.dependency_cycle = true;
    let error = save_milestone(
        &actor(),
        &mut cycle_dependency,
        "project",
        None,
        &dependency_input,
        "command-1",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_DEPENDENCY_CYCLE");
}

#[tokio::test]
async fn create_command_stages_bounded_plan_and_returns_exact_receipt() {
    let mut repository = repo();
    let receipt = save_milestone(
        &actor(),
        &mut repository,
        "project",
        None,
        &save_input(),
        "command-1",
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(receipt.id, "milestone-command-1");
    assert_eq!(receipt.version, 1);
    assert_eq!(receipt.state, None);
    assert_eq!(repository.saves, 1);
    let plan = repository.saved.unwrap();
    assert_eq!(plan.project_version, 4);
    assert_eq!(plan.ordinal_position, 2);
    assert_eq!(plan.operation, "project.milestone.create");
    assert!(plan.request_fingerprint.contains("project.milestone.create"));
}

#[tokio::test]
async fn revise_command_preserves_current_parent_and_dependencies_and_cas() {
    let mut repository = repo();
    let mut input = save_input();
    input.parent_supplied = false;
    input.ordinal_position = None;
    input.depends_on_milestone_ids = None;
    input.expected_version = 2;
    repository.dependencies = vec!["m-pre".into()];
    repository.dependency_facts = vec![DependencyFact {
        id: "m-pre".into(),
        state: "planned".into(),
    }];
    let receipt = save_milestone(
        &actor(),
        &mut repository,
        "project",
        Some("m-child"),
        &input,
        "command-2",
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(receipt.id, "m-child");
    assert_eq!(receipt.version, 3);
    let plan = repository.saved.unwrap();
    assert_eq!(plan.parent_milestone_id, None);
    assert_eq!(plan.depends_on_milestone_ids, vec!["m-pre"]);
    assert_eq!(plan.ordinal_position, 1);

    let mut conflict = repo();
    let mut stale = save_input();
    stale.expected_version = 1;
    let error = save_milestone(
        &actor(),
        &mut conflict,
        "project",
        Some("m-child"),
        &stale,
        "command-2",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_MILESTONE_CHANGED");

    let mut terminal = repo();
    terminal.milestone.as_mut().unwrap().state = "completed".into();
    terminal.milestone.as_mut().unwrap().completed_at = Some(1);
    let mut input = save_input();
    input.expected_version = 2;
    let error = save_milestone(
        &actor(),
        &mut terminal,
        "project",
        Some("m-child"),
        &input,
        "command-2",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_MILESTONE_TERMINAL");
}

#[tokio::test]
async fn transition_command_enforces_state_prerequisites_and_successors() {
    let mut start = repo();
    let receipt = transition_milestone(
        &actor(),
        &mut start,
        "project",
        "m-child",
        &transition_input(MilestoneAction::Start),
        "command-3",
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(receipt.state.as_deref(), Some("in_progress"));
    assert_eq!(receipt.version, 3);

    let mut invalid = repo();
    invalid.milestone.as_mut().unwrap().state = "completed".into();
    invalid.milestone.as_mut().unwrap().completed_at = Some(1);
    let error = transition_milestone(
        &actor(),
        &mut invalid,
        "project",
        "m-child",
        &transition_input(MilestoneAction::Start),
        "command-3",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_MILESTONE_TRANSITION_INVALID");

    let mut prerequisites = repo();
    prerequisites.incomplete = vec!["Predecessor".into()];
    let error = transition_milestone(
        &actor(),
        &mut prerequisites,
        "project",
        "m-child",
        &transition_input(MilestoneAction::Complete),
        "command-3",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_DEPENDENCIES_INCOMPLETE");

    let mut used = repo();
    used.active_successor = true;
    let error = transition_milestone(
        &actor(),
        &mut used,
        "project",
        "m-child",
        &transition_input(MilestoneAction::Cancel),
        "command-3",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_MILESTONE_REQUIRED_PREREQUISITE");

    let mut completed = repo();
    completed.milestone.as_mut().unwrap().state = "completed".into();
    completed.milestone.as_mut().unwrap().completed_at = Some(1);
    completed.completed_successor = true;
    let error = transition_milestone(
        &actor(),
        &mut completed,
        "project",
        "m-child",
        &transition_input(MilestoneAction::Reopen),
        "command-3",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_MILESTONE_DEPENDENT_COMPLETED");
}

#[tokio::test]
async fn command_text_counts_chars_and_trims_only_js_whitespace() {
    // Retained Zod schemas (`title`/`description`/`reason`) measure
    // Unicode scalars after ECMAScript `trim()`: a single
    // astral scalar is 1 char, U+FEFF is trimmed, U+0085 is not.
    for title in ["😀😀".to_owned(), "😀".repeat(200), "ก".repeat(200)] {
        let mut input = save_input();
        input.title = title;
        let mut repository = repo();
        save_milestone(&actor(), &mut repository, "project", None, &input, "command-1", NOW)
            .await
            .unwrap();
        assert_eq!(repository.saves, 1);
    }
    for title in [
        "😀".repeat(201),
        "ก".repeat(201),
        "\u{feff}Delivery".to_owned(),
        "Delivery\u{feff}".to_owned(),
        " Delivery".to_owned(),
    ] {
        let mut input = save_input();
        input.title = title;
        let mut repository = repo();
        let error = save_milestone(&actor(), &mut repository, "project", None, &input, "command-1", NOW)
            .await
            .unwrap_err();
        assert_eq!(error.code, "PROJECT_MILESTONE_INVALID");
        assert_eq!(repository.saves, 0);
    }
    // U+0085 is Unicode White_Space (Rust `trim` strips it) but JavaScript
    // `trim` keeps it, so it is part of the retained title.
    let mut input = save_input();
    input.title = "\u{0085}Delivery".into();
    let mut repository = repo();
    save_milestone(&actor(), &mut repository, "project", None, &input, "command-1", NOW)
        .await
        .unwrap();

    // Reason: "ab" + one astral scalar is 3 chars (passes minimum 3).
    let mut input = save_input();
    input.reason = "ab😀".into();
    let mut repository = repo();
    save_milestone(&actor(), &mut repository, "project", None, &input, "command-1", NOW)
        .await
        .unwrap();
    let mut input = save_input();
    input.reason = "\u{feff}Milestone detail reviewed.".into();
    let mut repository = repo();
    let error = save_milestone(&actor(), &mut repository, "project", None, &input, "command-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PROJECT_MILESTONE_INVALID");

    // Description: 10,000 astral scalars are exactly 10,000 chars.
    let mut input = save_input();
    input.description = "😀".repeat(10_000);
    let mut repository = repo();
    save_milestone(&actor(), &mut repository, "project", None, &input, "command-1", NOW)
        .await
        .unwrap();
    let mut input = save_input();
    input.description = "😀".repeat(10_001);
    let mut repository = repo();
    let error = save_milestone(&actor(), &mut repository, "project", None, &input, "command-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PROJECT_MILESTONE_INVALID");
}

#[tokio::test]
async fn replay_returns_retained_receipt_without_writes_and_rejects_conflicts() {
    let mut repository = repo();
    repository.stored = Some(StoredMilestoneReceipt {
        operation: "project.milestone.create".into(),
        record_id: "milestone-command-1".into(),
        request_fingerprint: String::new(),
        result_json: Some(
            serde_json::json!({"id": "milestone-command-1", "version": 1}).to_string(),
        ),
    });
    let mut input = save_input();
    input.title = "Different".into();
    let error = save_milestone(
        &actor(),
        &mut repository,
        "project",
        None,
        &input,
        "command-1",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "COMMAND_KEY_CONFLICT");
    assert_eq!(repository.saves, 0);

    let mut repository = repo();
    repository.stored = Some(StoredMilestoneReceipt {
        operation: "project.milestone.create".into(),
        record_id: "milestone-command-1".into(),
        request_fingerprint: String::new(),
        result_json: Some(
            serde_json::json!({"id": "milestone-command-1", "version": 1}).to_string(),
        ),
    });
    let mut repository_ok = repository;
    let error = save_milestone(
        &actor(),
        &mut repository_ok,
        "project",
        None,
        &save_input(),
        "command-1",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "COMMAND_KEY_CONFLICT");

    let mut valid = repo();
    let fingerprint = erpx_server_projects::public::project_commands::fingerprint(
        "project.milestone.create",
        "project",
        "milestone-command-1",
        &serde_json::to_value(save_input()).unwrap(),
    )
    .unwrap();
    valid.stored = Some(StoredMilestoneReceipt {
        operation: "project.milestone.create".into(),
        record_id: "milestone-command-1".into(),
        request_fingerprint: fingerprint,
        result_json: Some(
            serde_json::json!({"id": "milestone-command-1", "version": 1}).to_string(),
        ),
    });
    let receipt = save_milestone(
        &actor(),
        &mut valid,
        "project",
        None,
        &save_input(),
        "command-1",
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(receipt.id, "milestone-command-1");
    assert_eq!(valid.saves, 0);
}
