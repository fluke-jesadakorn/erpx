//! sqlx execution for the Projects member-roster read.
//!
//! The retained TypeScript statement selected only `user_id` and filtered by
//! project id. The native statement keeps that projection and ordering and adds
//! a Company join to `staff_projects`, so a roster can never cross the selected
//! Company. No caller supplies SQL or a relation name.

use crate::application::project_members::{ProjectMemberFuture, ProjectMemberRepository};
use crate::domain::project_members::{ProjectMemberScope, unavailable};
use erpx_platform_runtime_server::public::postgres::DatabasePool;

const MEMBERS_SQL: &str = "SELECT m.user_id FROM staff_project_members m \
     JOIN staff_projects p ON p.id=m.project_id AND p.company_id=$1 \
     WHERE m.project_id=$2 ORDER BY m.user_id";

pub struct PgProjectMembers {
    pool: DatabasePool,
}

impl PgProjectMembers {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl ProjectMemberRepository for PgProjectMembers {
    fn user_ids<'a>(
        &'a self,
        scope: &'a ProjectMemberScope,
    ) -> ProjectMemberFuture<'a, Vec<String>> {
        Box::pin(async move {
            sqlx::query_scalar::<_, String>(MEMBERS_SQL)
                .bind(scope.company_id.as_str())
                .bind(scope.project_id.as_str())
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statement_is_a_bounded_company_scoped_read() {
        let upper = MEMBERS_SQL.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"));
        assert!(upper.contains("P.COMPANY_ID=$1"));
        assert!(upper.contains("M.PROJECT_ID=$2"));
        assert!(upper.contains("ORDER BY M.USER_ID"));
        assert!(!upper.contains("INSERT"));
        assert!(!upper.contains("UPDATE "));
        assert!(!upper.contains("DELETE"));
        assert!(!upper.contains("FOR UPDATE"));
        assert!(!upper.contains("SELECT *"));
        assert_eq!(upper.matches('$').count(), 2);
    }
}
