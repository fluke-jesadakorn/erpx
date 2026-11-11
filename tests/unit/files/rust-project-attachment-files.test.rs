//! Files-side Projects attachment binding guards. No database or bytes involved.
use erpx_server_files::public::project_attachment::{
    PROJECT_ATTACHMENT_BINDING_TYPE, PROJECT_ATTACHMENT_PURPOSES,
    validate_project_attachment_binding,
};
use erpx_server_projects::public::project_attachment::{
    ProjectAttachmentBindingCommand, ProjectAttachmentGrant, ProjectAttachmentPurpose,
    ProjectAttachmentTargetType,
};
use erpx_server_shared::public::CompanyId;

fn grant_fixture() -> ProjectAttachmentGrant {
    ProjectAttachmentGrant {
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        target_type: ProjectAttachmentTargetType::Project,
        target_id: "project-1".into(),
        expected_version: 3,
        actor_id: "owner".into(),
        capability: "projects.wbs",
        purpose: ProjectAttachmentPurpose::NewWork,
        witness: "project-witness".into(),
    }
}

fn command_fixture() -> ProjectAttachmentBindingCommand {
    ProjectAttachmentBindingCommand {
        target_type: "project".into(),
        target_id: "project-1".into(),
        expected_version: 3,
        purpose: "new-work".into(),
        link_ids: vec!["link-1".into()],
        caption: Some("Site evidence".into()),
    }
}

#[test]
fn current_owner_links_pass_the_files_guard() {
    validate_project_attachment_binding(&grant_fixture(), &command_fixture()).unwrap();
    assert_eq!(PROJECT_ATTACHMENT_BINDING_TYPE, "record_document");
    assert_eq!(
        PROJECT_ATTACHMENT_PURPOSES,
        ["project_document", "deliverable", "handoff"]
    );
}

#[test]
fn unsupported_targets_fail_closed() {
    let mut command = command_fixture();
    command.target_type = "commerce_order".into();
    let error = validate_project_attachment_binding(&grant_fixture(), &command).unwrap_err();
    assert_eq!(error.code, "ATTACHMENT_TARGET_UNSUPPORTED");
}

#[test]
fn stale_versions_and_foreign_targets_are_rejected() {
    let mut command = command_fixture();
    command.expected_version = 2;
    assert_eq!(
        validate_project_attachment_binding(&grant_fixture(), &command)
            .unwrap_err()
            .code,
        "PROJECT_ATTACHMENT_INVALID"
    );

    let mut command = command_fixture();
    command.target_id = "other-project".into();
    assert_eq!(
        validate_project_attachment_binding(&grant_fixture(), &command)
            .unwrap_err()
            .code,
        "PROJECT_ATTACHMENT_INVALID"
    );
}

#[test]
fn an_empty_or_duplicate_link_set_is_rejected() {
    let mut command = command_fixture();
    command.link_ids.clear();
    assert_eq!(
        validate_project_attachment_binding(&grant_fixture(), &command)
            .unwrap_err()
            .code,
        "PROJECT_ATTACHMENT_INVALID"
    );

    let mut command = command_fixture();
    command.link_ids = vec!["link-1".into(), "link-1".into()];
    assert_eq!(
        validate_project_attachment_binding(&grant_fixture(), &command)
            .unwrap_err()
            .code,
        "PROJECT_ATTACHMENT_INVALID"
    );
}

#[test]
fn purpose_and_witness_must_match_the_authorized_grant() {
    let mut command = command_fixture();
    command.purpose = "history".into();
    assert_eq!(
        validate_project_attachment_binding(&grant_fixture(), &command)
            .unwrap_err()
            .code,
        "PROJECT_ATTACHMENT_INVALID"
    );

    let mut grant = grant_fixture();
    grant.witness.clear();
    assert_eq!(
        validate_project_attachment_binding(&grant, &command_fixture())
            .unwrap_err()
            .code,
        "PROJECT_ATTACHMENT_INVALID"
    );
}
