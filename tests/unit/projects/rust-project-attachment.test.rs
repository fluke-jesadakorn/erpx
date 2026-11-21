//! Native Projects attachment target policy and Files-port tests.
//!
//! These tests prove owner-only write authority, stale version refusal,
//! outsider read denial and the bound Files-port call order. They do not write
//! attachment rows; Files owns that ledger.
use erpx_server_projects::public::project_attachment::{
    PROJECT_ATTACHMENT_BIND_OPERATION, ProjectAttachmentActor, ProjectAttachmentBindingCommand,
    ProjectAttachmentBindingReceipt, ProjectAttachmentCommandReceipt,
    ProjectAttachmentFactsRepository, ProjectAttachmentFilesPort, ProjectAttachmentFuture,
    ProjectAttachmentGrant, ProjectAttachmentTargetFacts, StoredProjectAttachmentReceipt,
    authorize_project_attachment_read, bind_project_attachment, project_attachment_fingerprint,
    replay_project_attachment_binding,
};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, TimestampMillis};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor(user: &str) -> ProjectAttachmentActor {
    ProjectAttachmentActor {
        user_id: user.into(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        permissions: vec!["records.write".into(), "records.read".into()],
        modules: vec!["projects".into()],
        expires_at: NOW.get() + 60_000,
        active_staff: true,
        wbs_purchased: true,
    }
}

fn facts() -> ProjectAttachmentTargetFacts {
    ProjectAttachmentTargetFacts {
        company_id: "company".into(),
        target_id: "project".into(),
        owner_user_id: "owner".into(),
        lifecycle_status: "active".into(),
        version: 3,
        is_owner: true,
        is_member: true,
    }
}

fn command() -> ProjectAttachmentBindingCommand {
    ProjectAttachmentBindingCommand {
        target_type: "project".into(),
        target_id: "project".into(),
        expected_version: 3,
        purpose: "new-work".into(),
        link_ids: vec!["link-1".into()],
        caption: Some("Site evidence".into()),
    }
}

struct Repo {
    facts: Option<ProjectAttachmentTargetFacts>,
}

impl ProjectAttachmentFactsRepository for Repo {
    type Error = BusinessError;

    fn target<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
        _: &'a str,
    ) -> ProjectAttachmentFuture<'a, Option<ProjectAttachmentTargetFacts>, Self::Error> {
        Box::pin(async move { Ok(self.facts.clone()) })
    }
}

#[derive(Default)]
struct Files {
    bound: bool,
    advanced: bool,
    links: Vec<String>,
    receipt_links: Option<Vec<String>>,
}

impl ProjectAttachmentFilesPort for Files {
    type Error = BusinessError;

    fn bind<'a>(
        &'a mut self,
        _: ProjectAttachmentGrant,
        command: &'a ProjectAttachmentBindingCommand,
        _: i64,
    ) -> ProjectAttachmentFuture<'a, ProjectAttachmentBindingReceipt, Self::Error> {
        self.bound = true;
        self.links = command.link_ids.clone();
        let link_ids = self
            .receipt_links
            .clone()
            .unwrap_or_else(|| command.link_ids.clone());
        Box::pin(async move {
            Ok(ProjectAttachmentBindingReceipt {
                set_id: "set-1".into(),
                link_ids,
                target_version: 0,
            })
        })
    }

    fn advance_target<'a>(
        &'a mut self,
        _: &'a ProjectAttachmentGrant,
        _: i64,
    ) -> ProjectAttachmentFuture<'a, i64, Self::Error> {
        self.advanced = true;
        Box::pin(async move { Ok(4) })
    }
}

#[tokio::test]
async fn owner_binds_current_links_through_the_files_port() {
    let mut repository = Repo {
        facts: Some(facts()),
    };
    let mut files = Files::default();
    let receipt = bind_project_attachment(
        &actor("owner"),
        &mut repository,
        &mut files,
        &command(),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(receipt.set_id, "set-1");
    assert_eq!(receipt.target_version, 4);
    assert!(files.bound && files.advanced);
    assert_eq!(files.links, vec!["link-1".to_owned()]);
}

#[tokio::test]
async fn a_member_cannot_bind_project_files() {
    let mut repository = Repo {
        facts: Some(ProjectAttachmentTargetFacts {
            is_owner: false,
            owner_user_id: "owner".into(),
            ..facts()
        }),
    };
    let mut files = Files::default();
    let error = bind_project_attachment(
        &actor("member"),
        &mut repository,
        &mut files,
        &command(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_FILE_OWNER_REQUIRED");
    assert!(!files.bound);
}

#[tokio::test]
async fn a_closed_project_cannot_bind_files() {
    let mut repository = Repo {
        facts: Some(ProjectAttachmentTargetFacts {
            lifecycle_status: "handoff_pending".into(),
            ..facts()
        }),
    };
    let mut files = Files::default();
    let error = bind_project_attachment(
        &actor("owner"),
        &mut repository,
        &mut files,
        &command(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_FILE_OWNER_REQUIRED");
    assert!(!files.bound);
}

#[tokio::test]
async fn a_stale_target_version_cannot_bind_files() {
    let mut repository = Repo {
        facts: Some(ProjectAttachmentTargetFacts {
            version: 2,
            ..facts()
        }),
    };
    let mut files = Files::default();
    let error = bind_project_attachment(
        &actor("owner"),
        &mut repository,
        &mut files,
        &command(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_FILE_OWNER_REQUIRED");
    assert!(!files.bound);
}

#[tokio::test]
async fn duplicate_or_empty_links_are_rejected_before_files() {
    let mut repository = Repo {
        facts: Some(facts()),
    };
    let mut files = Files::default();
    let mut duplicate = command();
    duplicate.link_ids = vec!["link-1".into(), "link-1".into()];
    let error = bind_project_attachment(
        &actor("owner"),
        &mut repository,
        &mut files,
        &duplicate,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_ATTACHMENT_INVALID");
    assert!(!files.bound);
    let mut empty = command();
    empty.link_ids.clear();
    let error = bind_project_attachment(&actor("owner"), &mut repository, &mut files, &empty, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PROJECT_ATTACHMENT_INVALID");
}

#[tokio::test]
async fn a_mismatched_files_receipt_fails_closed() {
    let mut repository = Repo {
        facts: Some(facts()),
    };
    let mut files = Files {
        receipt_links: Some(vec!["other".into()]),
        ..Files::default()
    };
    let error = bind_project_attachment(
        &actor("owner"),
        &mut repository,
        &mut files,
        &command(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_ATTACHMENT_INVALID");
    assert!(!files.advanced);
}

#[tokio::test]
async fn members_read_and_outsiders_are_hidden() {
    let mut repository = Repo {
        facts: Some(ProjectAttachmentTargetFacts {
            is_owner: false,
            ..facts()
        }),
    };
    authorize_project_attachment_read(&actor("member"), &mut repository, "project", "project", NOW)
        .await
        .unwrap();
    repository.facts = Some(ProjectAttachmentTargetFacts {
        is_owner: false,
        is_member: false,
        ..facts()
    });
    let error = authorize_project_attachment_read(
        &actor("outsider"),
        &mut repository,
        "project",
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "ATTACHMENT_TARGET_NOT_FOUND");
}

fn retained_receipt() -> ProjectAttachmentCommandReceipt {
    ProjectAttachmentCommandReceipt {
        id: "attachment-1".into(),
        version: 4,
        status: "bound".into(),
        owner_version: 4,
        attachment_set_id: "set-1".into(),
        attachment_link_id: "link-1".into(),
    }
}

fn stored(
    actor: &ProjectAttachmentActor,
    set_id: &str,
    expected_version: i64,
) -> StoredProjectAttachmentReceipt {
    StoredProjectAttachmentReceipt {
        operation: PROJECT_ATTACHMENT_BIND_OPERATION.into(),
        record_id: set_id.into(),
        request_fingerprint: project_attachment_fingerprint(actor, set_id, expected_version)
            .unwrap(),
        result_json: Some(serde_json::to_string(&retained_receipt()).unwrap()),
    }
}

#[test]
fn replay_returns_the_prior_receipt_for_the_same_set_and_version() {
    let actor = actor("owner");
    let receipt =
        replay_project_attachment_binding(Some(stored(&actor, "set-1", 3)), &actor, "set-1", 3)
            .unwrap()
            .unwrap();
    assert_eq!(receipt, retained_receipt());
}

#[test]
fn replay_refuses_a_changed_set_or_version() {
    let actor = actor("owner");
    assert_eq!(
        replay_project_attachment_binding(Some(stored(&actor, "set-1", 3)), &actor, "set-1", 4,)
            .unwrap_err()
            .code,
        "ATTACHMENT_SET_CHANGED"
    );
    assert_eq!(
        replay_project_attachment_binding(Some(stored(&actor, "set-1", 3)), &actor, "set-2", 3,)
            .unwrap_err()
            .code,
        "ATTACHMENT_SET_CHANGED"
    );
}

#[test]
fn replay_refuses_another_operation_or_actor() {
    let owner = actor("owner");
    let mut other_operation = stored(&owner, "set-1", 3);
    other_operation.operation = "project.milestone.create".into();
    assert_eq!(
        replay_project_attachment_binding(Some(other_operation), &owner, "set-1", 3)
            .unwrap_err()
            .code,
        "IDEMPOTENCY_CONFLICT"
    );
    assert_eq!(
        replay_project_attachment_binding(
            Some(stored(&actor("other"), "set-1", 3)),
            &owner,
            "set-1",
            3,
        )
        .unwrap_err()
        .code,
        "IDEMPOTENCY_CONFLICT"
    );
}

#[test]
fn command_rejects_unsupported_targets() {
    let mut command = command();
    command.target_type = "staff_task".into();
    assert_eq!(
        command.validate().unwrap_err().code,
        "ATTACHMENT_TARGET_UNSUPPORTED"
    );
    assert!(RecordId::new("project").is_ok());
}
