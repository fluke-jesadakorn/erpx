//! Native Projects graph read policy and projection tests.
use erpx_server_projects::public::project_graph::{
    GraphActual, GraphTask, ProjectGraphActor, ProjectGraphReadRepository, ProjectGraphFuture,
    project_graph,
};
use erpx_server_projects::public::project_wbs::{
    MilestoneRow, ProjectReadActor, ProjectRow, ProjectWbsReadFuture, ProjectWbsReadRepository,
};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, TimestampMillis,
};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

fn actor(permissions: &[&str], modules: &[&str], costs: bool) -> ProjectGraphActor {
    ProjectGraphActor {
        read: ProjectReadActor {
            user_id: "owner".into(),
            company_id: company(),
            branch_id: Some(erpx_server_shared::public::RecordId::new("branch").unwrap()),
            department_id: Some(erpx_server_shared::public::RecordId::new("department").unwrap()),
            permissions: permissions.iter().map(|value| (*value).to_owned()).collect(),
            modules: modules.iter().map(|value| (*value).to_owned()).collect(),
            expires_at: NOW.get() + 60_000,
        },
        can_read_cost_history: costs,
    }
}

fn project() -> ProjectRow {
    ProjectRow {
        id: "project".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: Some("department".into()),
        title: "Reviewed project".into(),
        description: "Bounded graph fixture".into(),
        owner_user_id: "owner".into(),
        lifecycle_status: "active".into(),
        version: 4,
        updated_at: 10,
        is_owner: true,
        is_member: true,
    }
}

fn milestone(
    id: &str,
    title: &str,
    parent: Option<&str>,
    ordinal: i64,
    due_at: i64,
    state: &str,
    dependencies: &[(&str, &str)],
) -> MilestoneRow {
    let depends_on_ids = dependencies
        .iter()
        .map(|(id, _)| (*id).to_owned())
        .collect();
    let depends_on_titles = dependencies
        .iter()
        .map(|(_, title)| (*title).to_owned())
        .collect();
    MilestoneRow {
        id: id.into(),
        company_id: "company".into(),
        project_id: "project".into(),
        parent_milestone_id: parent.map(str::to_owned),
        ordinal_position: ordinal,
        title: title.into(),
        description: String::new(),
        due_at,
        owner_user_id: "owner".into(),
        owner_name: None,
        state: state.into(),
        is_overdue: false,
        version: 1,
        created_by: "owner".into(),
        completed_at: (state == "completed").then_some(20),
        created_at: 10,
        updated_at: 10,
        depends_on_ids,
        depends_on_titles,
        blocked_by_titles: Vec::new(),
        blocked_by_count: 0,
    }
}

fn task(id: &str, milestone: Option<&str>, binding: Option<i64>) -> GraphTask {
    GraphTask {
        id: id.into(),
        title: format!("Task {id}"),
        state: "in_progress".into(),
        version: 2,
        assignee_user_id: "owner".into(),
        milestone_id: milestone.map(str::to_owned),
        binding_version: binding,
    }
}

fn work_log(id: &str, milestone_id: &str, minutes: i64, is_actor: bool) -> GraphActual {
    GraphActual {
        id: id.into(),
        milestone_id: milestone_id.into(),
        source_kind: "work_log".into(),
        source_id: format!("source-{id}"),
        source_version: 3,
        minutes: Some(minutes),
        amount_minor: None,
        currency: None,
        user_id: Some(if is_actor { "owner" } else { "other" }.into()),
        current_source_version: 3,
        source_state: "approved".into(),
        journal_status: None,
        journal_id: None,
        is_actor,
    }
}

fn cost(id: &str, milestone_id: &str, amount: i64) -> GraphActual {
    GraphActual {
        id: id.into(),
        milestone_id: milestone_id.into(),
        source_kind: "cost_allocation".into(),
        source_id: format!("source-{id}"),
        source_version: 2,
        minutes: None,
        amount_minor: Some(amount),
        currency: Some("THB".into()),
        user_id: None,
        current_source_version: 2,
        source_state: "approved".into(),
        journal_status: Some("posted".into()),
        journal_id: Some("journal".into()),
        is_actor: false,
    }
}

struct Repo {
    project: Option<ProjectRow>,
    milestones: Vec<MilestoneRow>,
    tasks: Vec<GraphTask>,
    actuals: Vec<GraphActual>,
    calls: usize,
}

impl ProjectWbsReadRepository for Repo {
    type Error = BusinessError;

    fn project<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> ProjectWbsReadFuture<'a, Option<ProjectRow>, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.project.clone()) })
    }

    fn milestones<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> ProjectWbsReadFuture<'a, Vec<MilestoneRow>, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.milestones.clone()) })
    }
}

impl ProjectGraphReadRepository for Repo {
    fn tasks<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> ProjectGraphFuture<'a, Vec<GraphTask>, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.tasks.clone()) })
    }

    fn actuals<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> ProjectGraphFuture<'a, Vec<GraphActual>, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.actuals.clone()) })
    }
}

fn repo() -> Repo {
    Repo {
        project: Some(project()),
        milestones: vec![
            milestone("root", "Programme", None, 0, 2_000, "in_progress", &[]),
            milestone("child", "Delivery", Some("root"), 1, 1_000, "planned", &[]),
        ],
        tasks: vec![task("bound", Some("child"), Some(1)), task("loose", None, None)],
        actuals: vec![
            work_log("time", "child", 90, true),
            cost("cost", "child", 12_500),
        ],
        calls: 0,
    }
}

#[tokio::test]
async fn graph_policy_requires_permission_module_and_current_context() {
    let mut denied = repo();
    let error = project_graph(&actor(&[], &["projects"], true), &mut denied, "project", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Forbidden);
    assert_eq!(error.code, "PERMISSION_REQUIRED");
    assert_eq!(denied.calls, 0);

    let mut module = repo();
    let error = project_graph(
        &actor(&["records.read"], &["finance"], true),
        &mut module,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "MODULE_FORBIDDEN");
    assert_eq!(module.calls, 0);

    let mut expired = actor(&["records.read"], &["projects"], true);
    expired.read.expires_at = NOW.get();
    let mut repository = repo();
    let error = project_graph(&expired, &mut repository, "project", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Conflict);
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
    assert_eq!(repository.calls, 0);
}

#[tokio::test]
async fn graph_numbers_packages_tasks_edges_and_current_totals() {
    let mut repository = repo();
    let graph = project_graph(
        &actor(&["records.read", "finance.read"], &["projects"], true),
        &mut repository,
        "project",
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(graph.project.id, "project");
    assert_eq!(graph.packages.len(), 2);
    assert_eq!(graph.packages[0].wbs, "1");
    assert_eq!(graph.packages[0].depth, 0);
    assert_eq!(graph.packages[1].wbs, "1.1");
    assert_eq!(graph.packages[1].depth, 1);
    assert_eq!(graph.packages[1].approved_minutes, 90);
    assert_eq!(graph.packages[1].cost_by_currency.get("THB"), Some(&"12500".into()));
    assert_eq!(graph.totals.approved_minutes, 90);
    assert_eq!(graph.totals.cost_by_currency.get("THB"), Some(&"12500".into()));
    assert_eq!(graph.unassigned_tasks.len(), 1);
    assert_eq!(graph.unassigned_tasks[0].id, "loose");
    assert!(graph.can_read_costs);
    assert!(graph.is_project_owner);
    assert!(graph.edges.iter().any(|edge| edge.from == "root"
        && edge.to == "child"
        && edge.kind == "contains"));
    assert!(graph.edges.iter().any(|edge| edge.from == "child"
        && edge.to == "bound"
        && edge.kind == "task"));
}

#[tokio::test]
async fn graph_hides_cost_and_other_member_time_without_current_access() {
    let mut member_project = project();
    member_project.is_owner = false;
    member_project.is_member = true;
    let mut repository = Repo {
        project: Some(member_project),
        milestones: vec![milestone("root", "Programme", None, 0, 2_000, "planned", &[])],
        tasks: vec![],
        actuals: vec![
            work_log("own", "root", 30, true),
            work_log("other", "root", 45, false),
            cost("cost", "root", 12_500),
        ],
        calls: 0,
    };
    let graph = project_graph(
        &actor(&["records.read"], &["projects"], false),
        &mut repository,
        "project",
        NOW,
    )
    .await
    .unwrap();
    assert!(!graph.can_read_costs);
    assert_eq!(graph.actuals.len(), 1);
    assert_eq!(graph.actuals[0].id, "own");
    assert_eq!(graph.packages[0].approved_minutes, 30);
    assert!(graph.packages[0].cost_by_currency.is_empty());
    assert!(graph.totals.cost_by_currency.is_empty());
}

#[tokio::test]
async fn graph_company_scope_and_hidden_projects_fail_closed() {
    let mut hidden_project = project();
    hidden_project.is_owner = false;
    hidden_project.is_member = false;
    let mut hidden = Repo {
        project: Some(hidden_project),
        milestones: vec![],
        tasks: vec![],
        actuals: vec![],
        calls: 0,
    };
    let error = project_graph(
        &actor(&["records.read"], &["projects"], true),
        &mut hidden,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_NOT_FOUND");

    let mut foreign_project = project();
    foreign_project.company_id = "foreign".into();
    let mut foreign = Repo {
        project: Some(foreign_project),
        milestones: vec![],
        tasks: vec![],
        actuals: vec![],
        calls: 0,
    };
    let error = project_graph(
        &actor(&["records.read"], &["projects"], true),
        &mut foreign,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_GRAPH_SOURCE_INVALID");

    let mut missing = Repo {
        project: None,
        milestones: vec![],
        tasks: vec![],
        actuals: vec![],
        calls: 0,
    };
    let error = project_graph(
        &actor(&["records.read"], &["projects"], true),
        &mut missing,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_NOT_FOUND");
}

#[tokio::test]
async fn graph_rejects_invalid_task_bindings_and_orphaned_actuals() {
    let mut bound_to_missing = repo();
    bound_to_missing.tasks[0].milestone_id = Some("missing".into());
    let error = project_graph(
        &actor(&["records.read"], &["projects"], true),
        &mut bound_to_missing,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_GRAPH_SOURCE_INVALID");

    let mut unbound_with_version = repo();
    unbound_with_version.tasks[1].binding_version = Some(1);
    let error = project_graph(
        &actor(&["records.read"], &["projects"], true),
        &mut unbound_with_version,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_GRAPH_SOURCE_INVALID");

    let mut orphan = repo();
    orphan.actuals[0].milestone_id = "missing".into();
    let error = project_graph(
        &actor(&["records.read"], &["projects"], true),
        &mut orphan,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_GRAPH_SOURCE_INVALID");

    let mut duplicate_source = repo();
    duplicate_source.actuals.push(work_log("duplicate", "root", 10, true));
    duplicate_source.actuals[2].source_id = duplicate_source.actuals[0].source_id.clone();
    duplicate_source.actuals[2].source_kind = "work_log".into();
    let error = project_graph(
        &actor(&["records.read", "finance.read"], &["projects"], true),
        &mut duplicate_source,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_GRAPH_SOURCE_INVALID");
}

#[tokio::test]
async fn graph_rejects_invalid_actual_values_and_duplicate_ids() {
    let mut invalid_minutes = repo();
    invalid_minutes.actuals[0].minutes = Some(0);
    let error = project_graph(
        &actor(&["records.read"], &["projects"], true),
        &mut invalid_minutes,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_GRAPH_SOURCE_INVALID");

    let mut invalid_currency = repo();
    invalid_currency.actuals[1].currency = Some("thb".into());
    let error = project_graph(
        &actor(&["records.read"], &["projects"], true),
        &mut invalid_currency,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_GRAPH_SOURCE_INVALID");

    let mut duplicate_id = repo();
    duplicate_id.actuals[1].id = "time".into();
    let error = project_graph(
        &actor(&["records.read"], &["projects"], true),
        &mut duplicate_id,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_GRAPH_SOURCE_INVALID");
}

#[tokio::test]
async fn graph_rejects_duplicate_and_oversized_task_sources() {
    let mut duplicate = repo();
    duplicate.tasks[1].id = "bound".into();
    let error = project_graph(
        &actor(&["records.read"], &["projects"], true),
        &mut duplicate,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_GRAPH_SOURCE_INVALID");

    let mut oversized = repo();
    oversized.tasks = (0..=1000)
        .map(|index| task(&format!("task-{index}"), None, None))
        .collect();
    let error = project_graph(
        &actor(&["records.read"], &["projects"], true),
        &mut oversized,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind, FailureKind::InvalidInput);
    assert_eq!(error.code, "PROJECT_GRAPH_TOO_LARGE");
}

#[tokio::test]
async fn graph_cycles_and_orphan_parents_fail_closed() {
    let mut cycle = repo();
    cycle.milestones[0].parent_milestone_id = Some("child".into());
    let error = project_graph(
        &actor(&["records.read"], &["projects"], true),
        &mut cycle,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_PACKAGE_CYCLE");

    let mut orphan = repo();
    orphan.milestones[1].parent_milestone_id = Some("missing".into());
    let error = project_graph(
        &actor(&["records.read"], &["projects"], true),
        &mut orphan,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_PACKAGE_PARENT_INVALID");
}

#[test]
fn graph_serialization_uses_bounded_camel_case_summary_fields() {
    let graph = erpx_server_projects::public::project_graph::ProjectGraph {
        project: project(),
        packages: Vec::new(),
        tasks: Vec::new(),
        actuals: vec![work_log("time", "root", 60, true)],
        edges: Vec::new(),
        unassigned_tasks: Vec::new(),
        can_read_costs: true,
        is_project_owner: true,
        totals: erpx_server_projects::public::project_graph::GraphTotals {
            approved_minutes: 60,
            cost_by_currency: Default::default(),
        },
    };
    let value = serde_json::to_value(graph).unwrap();
    assert!(value.get("unassignedTasks").is_some());
    assert!(value.get("canReadCosts").is_some());
    assert!(value.get("isProjectOwner").is_some());
    assert!(value["totals"].get("approvedMinutes").is_some());
    assert!(value["actuals"][0].get("is_actor").is_none());
    assert!(value["actuals"][0].get("source_kind").is_some());
}
