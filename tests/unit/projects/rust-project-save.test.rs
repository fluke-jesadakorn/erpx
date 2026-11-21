//! Native Projects create/edit command policy, replay, validation and
//! purchase/staff-fact mapping tests.
use erpx_server_projects::public::project_commands::ProjectCommandProject;
use erpx_server_projects::public::project_save::{
    ProjectSaveFuture, ProjectSavePurchase, SaveProjectActor, SaveProjectCommand,
    SaveProjectPlan, SaveProjectReceipt, SaveProjectRepository, StoredProjectReceipt,
    save_project,
};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, TimestampMillis};
use std::collections::BTreeSet;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor() -> SaveProjectActor {
    SaveProjectActor {
        command: erpx_server_projects::public::project_commands::ProjectCommandActor {
            user_id: "owner".into(),
            company_id: CompanyId::new("company").unwrap(),
            branch_id: None,
            department_id: None,
            assignment_id: "assignment".into(),
            permissions: vec!["records.write".into()],
            modules: vec!["projects".into()],
            expires_at: NOW.get() + 60_000,
            active_staff: true,
            wbs_new_work: true,
            wbs_settle_existing: true,
        },
        staff_branch_id: Some("staff-branch".into()),
        staff_department_id: Some("staff-department".into()),
        active_staff_users: ["owner", "member", "second-member"]
            .into_iter()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>(),
        purchase: ProjectSavePurchase {
            new_work: true,
            settle_existing: true,
            history: true,
        },
    }
}

fn project() -> ProjectCommandProject {
    ProjectCommandProject {
        id: "project".into(),
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
        owner_user_id: "owner".into(),
        lifecycle_status: "active".into(),
        version: 3,
        is_owner: true,
        is_member: true,
    }
}

fn input() -> SaveProjectCommand {
    SaveProjectCommand {
        title: "Customer delivery".into(),
        description: "Bounded project brief".into(),
        member_user_ids: vec!["member".into(), "member".into()],
        expected_version: 3,
    }
}

#[derive(Default)]
struct Repo {
    project: Option<ProjectCommandProject>,
    stored: Option<StoredProjectReceipt>,
    next_receipt: Option<SaveProjectReceipt>,
    saves: usize,
    calls: usize,
    saved: Option<SaveProjectPlan>,
}

impl SaveProjectRepository for Repo {
    type Error = BusinessError;

    fn stored_receipt<'a>(
        &'a mut self,
        _: &'a str,
    ) -> ProjectSaveFuture<'a, Option<StoredProjectReceipt>, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.stored.clone()) })
    }

    fn project<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> ProjectSaveFuture<'a, Option<ProjectCommandProject>, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.project.clone()) })
    }

    fn save<'a>(
        &'a mut self,
        plan: SaveProjectPlan,
    ) -> ProjectSaveFuture<'a, SaveProjectReceipt, Self::Error> {
        self.saves += 1;
        self.saved = Some(plan.clone());
        Box::pin(async move {
            Ok(self.next_receipt.clone().unwrap_or(SaveProjectReceipt {
                id: plan.project_id,
                version: plan.version,
            }))
        })
    }
}

fn repo() -> Repo {
    Repo {
        project: Some(project()),
        ..Repo::default()
    }
}

#[tokio::test]
async fn create_stages_retained_branch_owner_version_and_deduplicated_members() {
    let mut repository = repo();
    let receipt = save_project(&actor(), &mut repository, None, &input(), "command-1", NOW)
        .await
        .unwrap();
    assert_eq!(receipt.id, "project-command-1");
    assert_eq!(receipt.version, 1);
    assert_eq!(repository.saves, 1);
    let plan = repository.saved.unwrap();
    assert_eq!(plan.operation, "project.create");
    assert_eq!(plan.project_version, 0);
    assert_eq!(plan.version, 1);
    assert_eq!(plan.branch_id.as_deref(), Some("staff-branch"));
    assert_eq!(plan.department_id.as_deref(), Some("staff-department"));
    assert_eq!(plan.actor_id, "owner");
    assert_eq!(plan.project_id, "project-command-1");
    assert_eq!(plan.member_user_ids, vec!["member"]);
    assert!(
        plan.request_fingerprint
            .contains("\"operation\":\"project.create\"")
    );
    assert_eq!(plan.now, NOW.get());
}

#[tokio::test]
async fn create_always_starts_at_version_one_even_with_a_larger_expected_version() {
    let mut repository = repo();
    let mut command = input();
    command.expected_version = 9;
    let receipt = save_project(&actor(), &mut repository, None, &command, "command-1", NOW)
        .await
        .unwrap();
    assert_eq!(receipt.version, 1);
    let plan = repository.saved.unwrap();
    assert_eq!(plan.project_version, 0);
    assert_eq!(plan.version, 1);
}

#[tokio::test]
async fn edit_uses_the_exact_version_guard_and_bumps_the_project_version() {
    let mut repository = repo();
    let receipt = save_project(
        &actor(),
        &mut repository,
        Some("project"),
        &input(),
        "command-2",
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(receipt.id, "project");
    assert_eq!(receipt.version, 4);
    let plan = repository.saved.unwrap();
    assert_eq!(plan.operation, "project.update");
    assert_eq!(plan.project_version, 3);
    assert_eq!(plan.version, 4);
    // The retained revision keeps the current branch/department untouched.
    assert_eq!(plan.branch_id, None);
    assert_eq!(plan.department_id, None);
}

#[tokio::test]
async fn stale_or_ahead_expected_version_is_a_conflict_before_any_write() {
    for expected in [2, 4] {
        let mut repository = repo();
        let mut command = input();
        command.expected_version = expected;
        let error = save_project(
            &actor(),
            &mut repository,
            Some("project"),
            &command,
            "command-2",
            NOW,
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind, erpx_server_shared::public::FailureKind::Conflict);
        assert_eq!(error.code, "CONFLICT");
        assert_eq!(repository.saves, 0);
    }
}

#[tokio::test]
async fn only_the_project_owner_may_edit_and_closed_projects_stay_frozen() {
    let mut member = repo();
    member.project.as_mut().unwrap().is_owner = false;
    let error = save_project(
        &actor(),
        &mut member,
        Some("project"),
        &input(),
        "command-2",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind, erpx_server_shared::public::FailureKind::Forbidden);
    assert_eq!(error.code, "PROJECT_OWNER_REQUIRED");
    assert_eq!(member.saves, 0);

    for lifecycle in ["handoff_pending", "completed", "cancelled"] {
        let mut frozen = repo();
        frozen.project.as_mut().unwrap().lifecycle_status = lifecycle.into();
        let error = save_project(
            &actor(),
            &mut frozen,
            Some("project"),
            &input(),
            "command-2",
            NOW,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "PROJECT_STRUCTURE_FROZEN");
        assert_eq!(frozen.saves, 0);
    }

    let mut hidden = repo();
    hidden.project.as_mut().unwrap().is_owner = false;
    hidden.project.as_mut().unwrap().is_member = false;
    let error = save_project(
        &actor(),
        &mut hidden,
        Some("project"),
        &input(),
        "command-2",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_NOT_FOUND");

    let mut missing = repo();
    missing.project = None;
    let error = save_project(
        &actor(),
        &mut missing,
        Some("project"),
        &input(),
        "command-2",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_NOT_FOUND");
}

#[tokio::test]
async fn replay_returns_the_retained_receipt_and_requires_history() {
    let mut repository = repo();
    let receipt = save_project(&actor(), &mut repository, None, &input(), "command-1", NOW)
        .await
        .unwrap();
    let stored = repository.saved.clone().unwrap();
    let fingerprint = stored.request_fingerprint.clone();

    let mut replay = Repo {
        stored: Some(StoredProjectReceipt {
            operation: "project.create".into(),
            record_id: receipt.id.clone(),
            request_fingerprint: fingerprint,
            result_json: Some(serde_json::to_string(&receipt).unwrap()),
        }),
        ..Repo::default()
    };
    let value = save_project(&actor(), &mut replay, None, &input(), "command-1", NOW)
        .await
        .unwrap();
    assert_eq!(value, receipt);
    // A replay reads no project and writes nothing.
    assert_eq!(replay.calls, 1);
    assert_eq!(replay.saves, 0);

    let mut no_history = actor();
    no_history.purchase.history = false;
    let mut replay = Repo {
        stored: Some(StoredProjectReceipt {
            operation: "project.create".into(),
            record_id: receipt.id.clone(),
            request_fingerprint: stored.request_fingerprint,
            result_json: Some(serde_json::to_string(&receipt).unwrap()),
        }),
        ..Repo::default()
    };
    let error = save_project(&no_history, &mut replay, None, &input(), "command-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.kind, erpx_server_shared::public::FailureKind::Forbidden);
    assert_eq!(error.code, "PACKAGE_REQUIRED");
    assert!(error.message.contains("retained history"));
    assert_eq!(replay.saves, 0);
}

#[tokio::test]
async fn a_reused_key_with_a_different_operation_record_or_input_conflicts() {
    let mut repository = repo();
    let receipt = save_project(&actor(), &mut repository, None, &input(), "command-1", NOW)
        .await
        .unwrap();
    let fingerprint = repository.saved.unwrap().request_fingerprint;

    for stored in [
        StoredProjectReceipt {
            operation: "project.update".into(),
            record_id: receipt.id.clone(),
            request_fingerprint: fingerprint.clone(),
            result_json: Some(serde_json::to_string(&receipt).unwrap()),
        },
        StoredProjectReceipt {
            operation: "project.create".into(),
            record_id: "project-other".into(),
            request_fingerprint: fingerprint.clone(),
            result_json: Some(serde_json::to_string(&receipt).unwrap()),
        },
        StoredProjectReceipt {
            operation: "project.create".into(),
            record_id: receipt.id.clone(),
            request_fingerprint: "different".into(),
            result_json: Some(serde_json::to_string(&receipt).unwrap()),
        },
    ] {
        let mut replay = Repo {
            stored: Some(stored),
            ..Repo::default()
        };
        let error = save_project(&actor(), &mut replay, None, &input(), "command-1", NOW)
            .await
            .unwrap_err();
        assert_eq!(error.code, "COMMAND_KEY_CONFLICT");
    }

    let mut incomplete = Repo {
        stored: Some(StoredProjectReceipt {
            operation: "project.create".into(),
            record_id: receipt.id.clone(),
            request_fingerprint: fingerprint.clone(),
            result_json: None,
        }),
        ..Repo::default()
    };
    let error = save_project(&actor(), &mut incomplete, None, &input(), "command-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PROJECT_COMMAND_RECEIPT_INCOMPLETE");

    let mut invalid = Repo {
        stored: Some(StoredProjectReceipt {
            operation: "project.create".into(),
            record_id: receipt.id.clone(),
            request_fingerprint: fingerprint,
            result_json: Some("{}".into()),
        }),
        ..Repo::default()
    };
    let error = save_project(&actor(), &mut invalid, None, &input(), "command-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PROJECT_COMMAND_RECEIPT_INVALID");
}

#[tokio::test]
async fn purchase_denial_maps_to_the_retained_package_codes() {
    let mut denied = actor();
    denied.purchase.new_work = false;
    denied.purchase.settle_existing = true;
    let mut repository = repo();
    let error = save_project(&denied, &mut repository, None, &input(), "command-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.kind, erpx_server_shared::public::FailureKind::Forbidden);
    assert_eq!(error.code, "PACKAGE_REQUIRED");
    assert_eq!(repository.saves, 0);

    // The retained update purpose is `new-work` too; settlement alone does not
    // admit an edit.
    let mut edit_denied = actor();
    edit_denied.purchase.new_work = false;
    let mut repository = repo();
    let error = save_project(
        &edit_denied,
        &mut repository,
        Some("project"),
        &input(),
        "command-2",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PACKAGE_REQUIRED");
    assert_eq!(repository.saves, 0);
}

#[tokio::test]
async fn every_member_requires_a_current_active_staff_row() {
    let mut repository = repo();
    let mut command = input();
    command.member_user_ids = vec!["member".into(), "stranger".into()];
    let error = save_project(&actor(), &mut repository, None, &command, "command-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(
        error.kind,
        erpx_server_shared::public::FailureKind::Conflict
    );
    assert_eq!(error.code, "STAFF_LINK_REQUIRED");
    assert_eq!(repository.saves, 0);

    let mut no_staff = actor();
    no_staff.command.active_staff = false;
    let mut repository = repo();
    let error = save_project(&no_staff, &mut repository, None, &input(), "command-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "STAFF_LINK_REQUIRED");
    assert_eq!(repository.saves, 0);
}

#[tokio::test]
async fn input_shape_matches_the_retained_schema() {
    let cases: Vec<SaveProjectCommand> = vec![
        SaveProjectCommand {
            title: "a".into(),
            ..input()
        },
        SaveProjectCommand {
            title: "😀".repeat(201),
            ..input()
        },
        SaveProjectCommand {
            title: " untrimmed".into(),
            ..input()
        },
        SaveProjectCommand {
            description: "😀".repeat(10_001),
            ..input()
        },
        SaveProjectCommand {
            expected_version: 0,
            ..input()
        },
        SaveProjectCommand {
            // 201 members exceeds the retained array bound.
            member_user_ids: (0..201).map(|index| format!("member-{index}")).collect(),
            ..input()
        },
        SaveProjectCommand {
            member_user_ids: vec![String::new()],
            ..input()
        },
        SaveProjectCommand {
            member_user_ids: vec!["m".repeat(161)],
            ..input()
        },
    ];
    for command in cases {
        let mut repository = repo();
        let error = save_project(&actor(), &mut repository, None, &command, "command-1", NOW)
            .await
            .unwrap_err();
        assert_eq!(error.code, "PROJECT_INVALID");
        assert_eq!(repository.saves, 0);
    }

    // Boundary values that the retained schema accepts.
    for command in [
        SaveProjectCommand {
            title: "ab".into(),
            ..input()
        },
        SaveProjectCommand {
            title: "😀".repeat(200),
            ..input()
        },
        SaveProjectCommand {
            description: "😀".repeat(10_000),
            ..input()
        },
        SaveProjectCommand {
            member_user_ids: (0..200).map(|index| format!("member-{index}")).collect(),
            ..input()
        },
    ] {
        let mut repository = repo();
        let mut boundary_actor = actor();
        boundary_actor.active_staff_users = command.member_user_ids.iter().cloned().collect();
        save_project(
            &boundary_actor,
            &mut repository,
            None,
            &command,
            "command-1",
            NOW,
        )
        .await
        .unwrap();
        assert_eq!(repository.saves, 1);
    }
}

#[tokio::test]
async fn command_key_and_record_identity_are_bounded() {
    let mut repository = repo();
    let error = save_project(&actor(), &mut repository, None, &input(), "short", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "COMMAND_KEY_REQUIRED");
    assert_eq!(repository.calls, 0);

    // A one-character key is rejected before any receipt read.
    let mut repository = repo();
    let error = save_project(&actor(), &mut repository, None, &input(), "bad key!", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "COMMAND_KEY_REQUIRED");

    let mut repository = repo();
    let error = save_project(&actor(), &mut repository, None, &input(), &"k".repeat(180), NOW)
        .await
        .unwrap();
    assert_eq!(error.id, format!("project-{}", "k".repeat(180)));
    assert!(RecordId::new(error.id).is_ok());
}

#[tokio::test]
async fn receipt_must_echo_the_planned_identity_and_version() {
    for receipt in [
        SaveProjectReceipt {
            id: "project-command-1".into(),
            version: 2,
        },
        SaveProjectReceipt {
            id: "project-other".into(),
            version: 1,
        },
    ] {
        let mut repository = Repo {
            next_receipt: Some(receipt),
            ..repo()
        };
        let error = save_project(&actor(), &mut repository, None, &input(), "command-1", NOW)
            .await
            .unwrap_err();
        assert_eq!(error.code, "PROJECT_COMMAND_RECEIPT_INVALID");
    }
}
