//! Pure Projects member-roster facts for the project detail route family.
//!
//! The retained TypeScript operation is a fixed membership read
//! (`SELECT user_id FROM staff_project_members WHERE project_id=? ORDER BY user_id`)
//! surfaced on the project detail page as `members: {user_id}[]`. This module
//! owns only the identity facts and the exact wire shape: Company and project
//! identifiers are bounded here, stored rows are validated, and the ordered
//! roster is projected without changing a field. No SQL, HTTP or I/O appears
//! in this layer.

use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind, RecordId};
use serde::Serialize;
use std::collections::BTreeSet;

/// One member row. The wire key deliberately stays `user_id`: the retained
/// presenter reads `members.map((member) => member.user_id)`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProjectMemberRow {
    pub user_id: String,
}

/// Company and project identity for one roster read. Composition resolves the
/// session and the selected work context; the owner never accepts a caller
/// supplied Company that has not been checked against that context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectMemberScope {
    pub company_id: CompanyId,
    pub project_id: RecordId,
}

impl ProjectMemberScope {
    pub fn new(company_id: &str, project_id: &str) -> Result<Self, BusinessError> {
        Ok(Self {
            company_id: CompanyId::new(company_id).map_err(|_| scope_invalid())?,
            project_id: RecordId::new(project_id).map_err(|_| scope_invalid())?,
        })
    }
}

pub fn scope_invalid() -> BusinessError {
    BusinessError::invalid(
        "PROJECT_MEMBERS_SCOPE_INVALID",
        "Use the current Company and one project for the member roster.",
    )
}

pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROJECT_MEMBERS_SOURCE_INVALID",
        "The current project member roster is invalid.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROJECT_MEMBERS_UNAVAILABLE",
        "The current project member roster could not be read.",
    )
}

/// Validate the stored ordered user ids and project them one-for-one. The SQL
/// adapter supplies the `ORDER BY user_id` sequence; this function never sorts
/// or renames a row, so the complete retained payload is preserved.
pub fn project_member_rows(user_ids: Vec<String>) -> Result<Vec<ProjectMemberRow>, BusinessError> {
    let mut seen = BTreeSet::new();
    user_ids
        .into_iter()
        .map(|user_id| {
            RecordId::new(user_id.as_str()).map_err(|_| source_invalid())?;
            if !seen.insert(user_id.clone()) {
                return Err(source_invalid());
            }
            Ok(ProjectMemberRow { user_id })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_rejects_blank_identity() {
        assert!(ProjectMemberScope::new("", "project").is_err());
        assert!(ProjectMemberScope::new("company", "").is_err());
        assert_eq!(
            ProjectMemberScope::new("company", "project")
                .unwrap()
                .project_id
                .as_str(),
            "project"
        );
    }

    #[test]
    fn ordered_rows_are_projected_one_for_one() {
        let rows = project_member_rows(vec!["a".into(), "b".into()]).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].user_id, "a");
        assert_eq!(rows[1].user_id, "b");
        assert_eq!(
            serde_json::to_value(&rows).unwrap(),
            serde_json::json!([{"user_id": "a"}, {"user_id": "b"}])
        );
    }

    #[test]
    fn malformed_and_duplicate_rows_fail_closed() {
        assert!(project_member_rows(vec!["a".into(), "a".into()]).is_err());
        assert!(project_member_rows(vec!["bad\nid".into()]).is_err());
        assert!(project_member_rows(Vec::new()).unwrap().is_empty());
    }
}
