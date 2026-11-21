//! Native Projects detail and WBS read policy tests.
use erpx_server_projects::public::project_wbs::{
    MilestoneRow, ProjectReadActor, ProjectRow, ProjectWbsReadFuture, ProjectWbsReadRepository,
    project_wbs,
};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

fn actor(permissions: &[&str], modules: &[&str]) -> ProjectReadActor {
    ProjectReadActor {
        user_id: "owner".into(),
        company_id: company(),
        branch_id: None,
        department_id: None,
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        modules: modules.iter().map(|value| (*value).to_owned()).collect(),
        expires_at: NOW.get() + 60_000,
    }
}

fn project() -> ProjectRow {
    ProjectRow {
        id: "project".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: Some("department".into()),
        title: "Reviewed project".into(),
        description: "Bounded project read fixture".into(),
        owner_user_id: "owner".into(),
        lifecycle_status: "active".into(),
        version: 4,
        updated_at: 10,
        is_owner: true,
        is_member: true,
    }
}

#[allow(clippy::too_many_arguments)]
fn milestone(
    id: &str,
    title: &str,
    parent: Option<&str>,
    ordinal: i64,
    due_at: i64,
    state: &str,
    dependencies: &[(&str, &str)],
    blocked: &[&str],
) -> MilestoneRow {
    let depends_on_ids = dependencies
        .iter()
        .map(|(id, _)| (*id).to_owned())
        .collect();
    let depends_on_titles = dependencies
        .iter()
        .map(|(_, title)| (*title).to_owned())
        .collect();
    let blocked_by_titles = blocked.iter().map(|title| (*title).to_owned()).collect();
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
        blocked_by_titles,
        blocked_by_count: blocked.len() as i64,
    }
}

struct Repo {
    project: Option<ProjectRow>,
    milestones: Vec<MilestoneRow>,
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

#[tokio::test]
async fn policy_requires_permission_module_and_current_context() {
    let repository = || Repo {
        project: Some(project()),
        milestones: Vec::new(),
        calls: 0,
    };

    let mut denied = repository();
    let error = project_wbs(&actor(&[], &["projects"]), &mut denied, "project", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Forbidden);
    assert_eq!(error.code, "PERMISSION_REQUIRED");
    assert_eq!(denied.calls, 0);

    let mut module = repository();
    let error = project_wbs(
        &actor(&["records.read"], &["finance"]),
        &mut module,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "MODULE_FORBIDDEN");
    assert_eq!(module.calls, 0);

    let mut expired = actor(&["records.read"], &["projects"]);
    expired.expires_at = NOW.get();
    let mut repository = repository();
    let error = project_wbs(&expired, &mut repository, "project", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Conflict);
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
    assert_eq!(repository.calls, 0);
}

#[tokio::test]
async fn read_returns_project_and_computed_overdue_milestones() {
    let mut repository = Repo {
        project: Some(project()),
        milestones: vec![
            milestone(
                "child",
                "Delivery",
                Some("root"),
                1,
                1_000,
                "planned",
                &[],
                &[],
            ),
            milestone("root", "Programme", None, 0, 2_000, "in_progress", &[], &[]),
        ],
        calls: 0,
    };
    let result = project_wbs(
        &actor(&["records.read"], &["projects"]),
        &mut repository,
        "project",
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.project.id, "project");
    assert_eq!(result.milestones.len(), 2);
    assert!(result.milestones[0].is_overdue);
    assert!(result.milestones[1].is_overdue);
}

#[tokio::test]
async fn company_scope_and_membership_fail_closed() {
    let mut missing = Repo {
        project: None,
        milestones: Vec::new(),
        calls: 0,
    };
    let error = project_wbs(
        &actor(&["records.read"], &["projects"]),
        &mut missing,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_NOT_FOUND");

    let mut hidden = project();
    hidden.is_owner = false;
    hidden.is_member = false;
    let mut repository = Repo {
        project: Some(hidden),
        milestones: Vec::new(),
        calls: 0,
    };
    let error = project_wbs(
        &actor(&["records.read"], &["projects"]),
        &mut repository,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind, FailureKind::NotFound);

    let mut foreign = project();
    foreign.company_id = "foreign".into();
    let mut repository = Repo {
        project: Some(foreign),
        milestones: Vec::new(),
        calls: 0,
    };
    let error = project_wbs(
        &actor(&["records.read"], &["projects"]),
        &mut repository,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_WBS_SOURCE_INVALID");

    let mut repository = Repo {
        project: Some(project()),
        milestones: vec![
            milestone(
                "foreign",
                "Foreign package",
                None,
                0,
                1,
                "planned",
                &[],
                &[],
            )
            .clone(),
        ],
        calls: 0,
    };
    repository.milestones[0].company_id = "foreign".into();
    let error = project_wbs(
        &actor(&["records.read"], &["projects"]),
        &mut repository,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_WBS_SOURCE_INVALID");
}

#[tokio::test]
async fn package_parent_and_depth_cycles_fail_closed() {
    let mut repository = Repo {
        project: Some(project()),
        milestones: vec![milestone(
            "child",
            "Delivery",
            Some("missing"),
            1,
            1,
            "planned",
            &[],
            &[],
        )],
        calls: 0,
    };
    let error = project_wbs(
        &actor(&["records.read"], &["projects"]),
        &mut repository,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_PACKAGE_PARENT_INVALID");

    let mut repository = Repo {
        project: Some(project()),
        milestones: vec![
            milestone("a", "Package A", Some("b"), 0, 1, "planned", &[], &[]),
            milestone("b", "Package B", Some("a"), 1, 2, "planned", &[], &[]),
        ],
        calls: 0,
    };
    let error = project_wbs(
        &actor(&["records.read"], &["projects"]),
        &mut repository,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_PACKAGE_CYCLE");

    let mut repository = Repo {
        project: Some(project()),
        milestones: vec![milestone(
            "self",
            "Self parent",
            Some("self"),
            0,
            1,
            "planned",
            &[],
            &[],
        )],
        calls: 0,
    };
    let error = project_wbs(
        &actor(&["records.read"], &["projects"]),
        &mut repository,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_WBS_SOURCE_INVALID");
}

#[tokio::test]
async fn dependency_references_and_cycles_fail_closed() {
    let mut repository = Repo {
        project: Some(project()),
        milestones: vec![milestone(
            "a",
            "Package A",
            None,
            0,
            1,
            "planned",
            &[("missing", "Missing package")],
            &[],
        )],
        calls: 0,
    };
    let error = project_wbs(
        &actor(&["records.read"], &["projects"]),
        &mut repository,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_DEPENDENCY_INVALID");

    let mut repository = Repo {
        project: Some(project()),
        milestones: vec![
            milestone(
                "a",
                "Package A",
                None,
                0,
                1,
                "planned",
                &[("b", "Package B")],
                &[],
            ),
            milestone(
                "b",
                "Package B",
                None,
                1,
                2,
                "planned",
                &[("a", "Package A")],
                &[],
            ),
        ],
        calls: 0,
    };
    let error = project_wbs(
        &actor(&["records.read"], &["projects"]),
        &mut repository,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_DEPENDENCY_CYCLE");
}

#[tokio::test]
async fn duplicate_dependency_titles_are_valid_when_ids_are_distinct() {
    let mut successor = milestone(
        "c",
        "Successor",
        None,
        2,
        3,
        "planned",
        &[("a", "Delivery"), ("b", "Delivery")],
        &["Delivery", "Delivery"],
    );
    successor.blocked_by_count = 2;
    let mut repository = Repo {
        project: Some(project()),
        milestones: vec![
            milestone("a", "Delivery", None, 0, 1, "planned", &[], &[]),
            milestone("b", "Delivery", None, 1, 2, "planned", &[], &[]),
            successor,
        ],
        calls: 0,
    };
    let result = project_wbs(
        &actor(&["records.read"], &["projects"]),
        &mut repository,
        "project",
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.milestones.len(), 3);
    assert_eq!(result.milestones[2].blocked_by_count, 2);
}

#[tokio::test]
async fn duplicate_and_oversized_sources_fail_closed() {
    let mut repository = Repo {
        project: Some(project()),
        milestones: vec![
            milestone("a", "Package A", None, 0, 1, "planned", &[], &[]),
            milestone("a", "Package A duplicate", None, 1, 2, "planned", &[], &[]),
        ],
        calls: 0,
    };
    let error = project_wbs(
        &actor(&["records.read"], &["projects"]),
        &mut repository,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_WBS_SOURCE_INVALID");

    let mut repository = Repo {
        project: Some(project()),
        milestones: (0..=1000)
            .map(|index| {
                milestone(
                    &format!("package-{index}"),
                    "Package",
                    None,
                    index,
                    index,
                    "planned",
                    &[],
                    &[],
                )
            })
            .collect(),
        calls: 0,
    };
    let error = project_wbs(
        &actor(&["records.read"], &["projects"]),
        &mut repository,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind, FailureKind::InvalidInput);
    assert_eq!(error.code, "PROJECT_WBS_TOO_LARGE");
}

#[tokio::test]
async fn invalid_state_completion_pair_and_blocked_facts_fail_closed() {
    let mut repository = Repo {
        project: Some(project()),
        milestones: vec![milestone(
            "a",
            "Package A",
            None,
            0,
            1,
            "archived",
            &[],
            &[],
        )],
        calls: 0,
    };
    let error = project_wbs(
        &actor(&["records.read"], &["projects"]),
        &mut repository,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_WBS_SOURCE_INVALID");

    let mut repository = Repo {
        project: Some(project()),
        milestones: vec![milestone(
            "a",
            "Package A",
            None,
            0,
            1,
            "completed",
            &[],
            &[],
        )],
        calls: 0,
    };
    repository.milestones[0].completed_at = None;
    let error = project_wbs(
        &actor(&["records.read"], &["projects"]),
        &mut repository,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_WBS_SOURCE_INVALID");

    let mut repository = Repo {
        project: Some(project()),
        milestones: vec![milestone(
            "a",
            "Package A",
            None,
            0,
            1,
            "planned",
            &[("b", "Package B")],
            &["Undeclared block"],
        )],
        calls: 0,
    };
    repository.milestones[0].depends_on_ids = vec!["b".into()];
    repository.milestones[0].depends_on_titles = vec!["Package B".into()];
    repository.milestones[0].blocked_by_titles = vec!["Undeclared block".into()];
    repository.milestones[0].blocked_by_count = 1;
    let error = project_wbs(
        &actor(&["records.read"], &["projects"]),
        &mut repository,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_WBS_SOURCE_INVALID");
}

#[tokio::test]
async fn completed_due_packages_are_not_overdue() {
    let completed = milestone("a", "Completed package", None, 0, 1, "completed", &[], &[]);
    let cancelled = milestone("b", "Cancelled package", None, 0, 1, "cancelled", &[], &[]);
    let mut repository = Repo {
        project: Some(project()),
        milestones: vec![completed, cancelled],
        calls: 0,
    };
    let rows = project_wbs(
        &actor(&["records.read"], &["projects"]),
        &mut repository,
        "project",
        NOW,
    )
    .await
    .unwrap();
    assert!(!rows.milestones[0].is_overdue);
    assert!(!rows.milestones[1].is_overdue);
    assert_eq!(
        RecordId::new("project").unwrap().as_str(),
        rows.project.id.as_str()
    );
}

#[tokio::test]
async fn stored_text_counts_chars_and_trims_only_js_whitespace() {
    // Stored project/milestone title and description originate from the
    // retained Zod `trim()` schemas, which measure Unicode scalars.
    for title in ["😀😀".to_owned(), "😀".repeat(200)] {
        let mut repository = Repo {
            project: Some(project()),
            milestones: vec![milestone("a", &title, None, 0, 1, "planned", &[], &[])],
            calls: 0,
        };
        project_wbs(&actor(&["records.read"], &["projects"]), &mut repository, "project", NOW)
            .await
            .unwrap();
    }
    for title in ["😀".repeat(201), "\u{feff}Package A".to_owned()] {
        let mut repository = Repo {
            project: Some(project()),
            milestones: vec![milestone("a", &title, None, 0, 1, "planned", &[], &[])],
            calls: 0,
        };
        let error = project_wbs(&actor(&["records.read"], &["projects"]), &mut repository, "project", NOW)
            .await
            .unwrap_err();
        assert_eq!(error.code, "PROJECT_WBS_SOURCE_INVALID");
    }
    let mut repository = Repo {
        project: Some(project()),
        milestones: vec![milestone("a", "\u{0085}Package A", None, 0, 1, "planned", &[], &[])],
        calls: 0,
    };
    project_wbs(&actor(&["records.read"], &["projects"]), &mut repository, "project", NOW)
        .await
        .unwrap();

    let mut repository = Repo {
        project: Some(project()),
        milestones: vec![milestone("a", "Package A", None, 0, 1, "planned", &[], &[])],
        calls: 0,
    };
    repository.milestones[0].description = "😀".repeat(10_000);
    project_wbs(&actor(&["records.read"], &["projects"]), &mut repository, "project", NOW)
        .await
        .unwrap();
    repository.milestones[0].description = "😀".repeat(10_001);
    let error = project_wbs(&actor(&["records.read"], &["projects"]), &mut repository, "project", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PROJECT_WBS_SOURCE_INVALID");
}

#[test]
fn serialization_contains_only_declared_wbs_facts() {
    let value = serde_json::to_value(erpx_server_projects::public::project_wbs::ProjectWbs {
        project: project(),
        milestones: vec![milestone("a", "Package A", None, 0, 1, "planned", &[], &[])],
    })
    .unwrap();
    let project_keys = value["project"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        project_keys,
        [
            "branch_id",
            "company_id",
            "department_id",
            "description",
            "id",
            "lifecycle_status",
            "owner_user_id",
            "title",
            "updated_at",
            "version"
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    );
    let milestone_keys = value["milestones"][0]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    assert!(milestone_keys.contains("depends_on_ids"));
    assert!(milestone_keys.contains("blocked_by_count"));
    assert!(milestone_keys.contains("company_id"));
    assert!(!project_keys.contains("is_owner"));
    assert!(!project_keys.contains("is_member"));
}
