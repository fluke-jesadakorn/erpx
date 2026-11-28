//! Pure response and authority types for the native Holding people directory.
//!
//! The retired TypeScript path (`holding-people-directory.ts`,
//! `application/directory-projection.ts`) produced `HoldingPeopleDirectory`
//! from owner SQL. Every field below is declared in the same order and renamed
//! to the same camelCase keys so `serde_json` emits byte-compatible payloads.

use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};
use serde::Serialize;

/// A verified Account work scope. Composition resolves it through Identity;
/// the router never mints, widens or accepts an unverified scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HoldingDirectoryScope {
    pub holding_id: String,
    pub binding_id: String,
    pub company_id: String,
    pub scope_holding_id: Option<String>,
    pub scope_company_id: Option<String>,
    pub assignment_id: String,
    pub user_id: String,
    pub role: String,
    pub assignment_scope: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DirectoryCompany {
    pub id: String,
    pub slug: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryNode {
    pub id: String,
    pub company_id: String,
    pub kind: String,
    pub slug: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub branch_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryLevel {
    pub id: String,
    pub company_id: String,
    pub code: String,
    pub name: String,
    pub number: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryPosition {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub title: String,
    pub level_id: String,
    pub reports_to_position_id: Option<String>,
    pub is_ceo: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryPerson {
    pub id: String,
    pub person_id: String,
    pub user_id: Option<String>,
    pub engagement_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub position_id: Option<String>,
    pub legal_name: String,
    pub job_title: String,
    pub work_email: Option<String>,
    pub work_phone: Option<String>,
}

/// Exact `HoldingPeopleDirectory` payload retired from the TypeScript route.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct HoldingPeopleDirectory {
    pub companies: Vec<DirectoryCompany>,
    pub nodes: Vec<DirectoryNode>,
    pub levels: Vec<DirectoryLevel>,
    pub positions: Vec<DirectoryPosition>,
    pub people: Vec<DirectoryPerson>,
}

/// All owner facts read inside one Holding unit, including the assignment
/// authority witness used by the application guard.
#[derive(Clone, Debug, Default)]
pub struct HoldingDirectoryFacts {
    pub assignment_active: bool,
    pub companies: Vec<DirectoryCompany>,
    pub nodes: Vec<DirectoryNode>,
    pub levels: Vec<DirectoryLevel>,
    pub positions: Vec<DirectoryPosition>,
    pub people: Vec<DirectoryPerson>,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_HOLDING_DIRECTORY_REQUEST_INVALID",
        "Use one unambiguous registered Holding people-directory request.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_PAYROLL_HOLDING_DIRECTORY_UNAVAILABLE",
        "The native Holding people directory is unavailable.",
    )
}

pub fn scope_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "HOLDING_SCOPE_REQUIRED",
        "Select an authorized Holding workspace.",
    )
}

pub fn assignment_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "HOLDING_DIRECTORY_FORBIDDEN",
        "Your work assignment is no longer active in this Holding.",
    )
}

/// Identities are bound values, never SQL fragments.
pub fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

pub fn valid_assignment_scope(scope: &str) -> bool {
    matches!(scope, "holding" | "branch" | "department")
}

/// The exact scope guard from `assertHoldingDirectoryScope`.
pub fn scoped(scope: &HoldingDirectoryScope) -> bool {
    scope.binding_id == scope.holding_id
        && scope.scope_holding_id.as_deref() == Some(scope.holding_id.as_str())
        && scope.scope_company_id.as_deref() == Some(scope.company_id.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> HoldingDirectoryScope {
        HoldingDirectoryScope {
            holding_id: "h1".into(),
            binding_id: "h1".into(),
            company_id: "c1".into(),
            scope_holding_id: Some("h1".into()),
            scope_company_id: Some("c1".into()),
            assignment_id: "a1".into(),
            user_id: "user-1".into(),
            role: "holding_owner".into(),
            assignment_scope: "holding".into(),
        }
    }

    #[test]
    fn identities_are_bounded() {
        assert!(valid_id("user-1"));
        assert!(valid_id("comp_1.acme:worker"));
        assert!(!valid_id(""));
        assert!(!valid_id("bad id"));
        assert!(!valid_id("x'; DROP TABLE hr_people --"));
        assert!(valid_slug("acme"));
        assert!(!valid_slug("Acme"));
        assert!(valid_assignment_scope("holding"));
        assert!(!valid_assignment_scope("platform"));
    }

    #[test]
    fn cross_holding_scope_fails_closed() {
        assert!(scoped(&scope()));
        let mut other = scope();
        other.scope_company_id = Some("c2".into());
        assert!(!scoped(&other));
        let mut other = scope();
        other.binding_id = "h2".into();
        assert!(!scoped(&other));
        let mut other = scope();
        other.scope_holding_id = None;
        assert!(!scoped(&other));
    }
}
