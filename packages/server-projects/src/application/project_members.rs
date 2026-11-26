//! Native Projects member-roster read use case.
//!
//! The repository port owns one bounded read on the caller's unit; the adapter
//! executes the real Company-scoped SQL. This layer carries no SQL, no session
//! and no fallback: it validates the scope, delegates the ordered row read and
//! maps the stored rows into the exact retained `ProjectMemberRow[]` payload.

use crate::domain::project_members::{
    ProjectMemberRow, ProjectMemberScope, project_member_rows,
};
use erpx_server_shared::public::BusinessError;
use std::future::Future;
use std::pin::Pin;

pub type ProjectMemberFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub trait ProjectMemberRepository: Send + Sync {
    /// Ordered user ids for one current Company project.
    fn user_ids<'a>(&'a self, scope: &'a ProjectMemberScope) -> ProjectMemberFuture<'a, Vec<String>>;
}

/// Read-only query: no command key, receipt, audit or outbox write. The
/// complete result is the retained `{user_id}[]` member roster.
pub async fn read_project_members<R: ProjectMemberRepository>(
    scope: &ProjectMemberScope,
    repository: &R,
) -> Result<Vec<ProjectMemberRow>, BusinessError> {
    let user_ids = repository.user_ids(scope).await?;
    project_member_rows(user_ids)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    async fn returns_complete_ordered_roster() {
        let scope = ProjectMemberScope::new("company", "project").unwrap();
        let rows = read_project_members(&scope, &Repo(vec!["owner".into(), "member".into()]))
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].user_id, "owner");
        assert_eq!(rows[1].user_id, "member");
    }

    #[tokio::test]
    async fn repository_failure_propagates() {
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
}
