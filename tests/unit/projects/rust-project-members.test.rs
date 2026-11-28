//! Native Projects member-roster read: application port + payload parity.
//!
//! This target needs a `[[test]]` entry in modules/projects/server/Cargo.toml,
//! which is outside the worker allowed paths and is lead-owned. The in-crate
//! `#[cfg(test)]` modules cover the same facts without registration.
use erpx_projects_server::public::project_members::{
    ProjectMemberFuture, ProjectMemberRepository, ProjectMemberScope, read_project_members,
};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::json;

struct Repo(Vec<String>);

impl ProjectMemberRepository for Repo {
    fn user_ids<'a>(
        &'a self,
        _: &'a ProjectMemberScope,
    ) -> ProjectMemberFuture<'a, Vec<String>> {
        Box::pin(async move { Ok(self.0.clone()) })
    }
}

#[tokio::test]
async fn member_roster_matches_retained_wire_payload() {
    let scope = ProjectMemberScope::new("company", "project").unwrap();
    let members = read_project_members(
        &scope,
        &Repo(vec!["owner".into(), "member".into(), "reviewer".into()]),
    )
    .await
    .unwrap();
    assert_eq!(
        serde_json::to_value(members).unwrap(),
        json!([
            {"user_id": "owner"},
            {"user_id": "member"},
            {"user_id": "reviewer"}
        ])
    );
}

#[tokio::test]
async fn invalid_stored_identity_fails_closed() {
    let scope = ProjectMemberScope::new("company", "project").unwrap();
    let error = read_project_members(&scope, &Repo(vec!["bad\nid".into()]))
        .await
        .unwrap_err();
    assert_eq!(error.code, "PROJECT_MEMBERS_SOURCE_INVALID");
}

#[tokio::test]
async fn repository_failure_propagates_without_fallback() {
    struct Broken;
    impl ProjectMemberRepository for Broken {
        fn user_ids<'a>(
            &'a self,
            _: &'a ProjectMemberScope,
        ) -> ProjectMemberFuture<'a, Vec<String>> {
            Box::pin(async { Err(BusinessError::invalid("BROKEN", "broken")) })
        }
    }
    let scope = ProjectMemberScope::new("company", "project").unwrap();
    assert_eq!(
        read_project_members(&scope, &Broken).await.unwrap_err().code,
        "BROKEN"
    );
}
