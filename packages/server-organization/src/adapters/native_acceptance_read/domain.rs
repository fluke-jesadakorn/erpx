//! Pure response types and invariants for the native Company acceptance read.
//!
//! This module owns no SQL and no transport. It describes the exact payload the
//! owner may return for `GET /organization/acceptance` and validates it before a
//! reply is released, so the TypeScript `companyBusinessAcceptance` authority
//! can be retired without dropping a field.

use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Serialize;

pub const MAX_ID: usize = 200;
pub const MAX_EVENTS: usize = 20_000;
pub const MAX_CONTEXT_VALUES: usize = 128;

/// Verified browser context injected by the native gateway as bounded headers.
/// The Company workspace (Holding) scope is the only accepted scope; the actor
/// facts are repeated here so no permission or same-person claim is inferred
/// from caller-controlled text.
#[derive(Clone, Debug)]
pub struct AcceptanceQuery {
    pub company: String,
    pub user: String,
    pub assignment: String,
    pub permissions: Vec<String>,
    pub person_ids: Vec<String>,
    pub scope: String,
    pub branch: Option<String>,
    pub department: Option<String>,
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "ORGANIZATION_ACCEPTANCE_READ_INVALID",
        "Use one bounded Company acceptance read.",
    )
}

fn forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "COMPANY_AUTHORITY_REQUIRED",
        "Open the Company workspace to review Company acceptance.",
    )
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

impl AcceptanceQuery {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_id(&self.company)
            || !valid_id(&self.user)
            || !valid_id(&self.assignment)
            || self.permissions.len() > MAX_CONTEXT_VALUES
            || self.person_ids.len() > MAX_CONTEXT_VALUES
            || self
                .permissions
                .iter()
                .chain(self.person_ids.iter())
                .any(|value| !valid_id(value))
            || self.branch.as_deref().is_some_and(|value| !valid_id(value))
            || self.department.as_deref().is_some_and(|value| !valid_id(value))
        {
            return Err(invalid());
        }
        if self.scope != "holding" || self.branch.is_some() || self.department.is_some() {
            return Err(forbidden());
        }
        Ok(())
    }

    /// Parity with `samePerson(actor, id)` in the retained application.
    pub fn same_person(&self, id: &str) -> bool {
        self.user == id || self.person_ids.iter().any(|value| value == id)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CompanyRow {
    pub id: String,
    pub display_name: String,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AcceptanceRow {
    pub company_id: String,
    pub status: String,
    pub version: i64,
    pub requested_by: String,
    pub reviewed_by: Option<String>,
    pub requested_at: i64,
    pub reviewed_at: Option<i64>,
    pub reason: String,
    pub review_reason: Option<String>,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EventRow {
    pub id: String,
    pub action: String,
    pub actor_id: String,
    pub version: i64,
    pub reason: String,
    pub created_at: i64,
}

/// All facts needed by the owner decision, gathered in one read unit.
#[derive(Clone, Debug)]
pub struct AcceptanceFacts {
    pub company: Option<CompanyRow>,
    pub branches: i64,
    pub levels: i64,
    pub ceo: i64,
    pub incomplete: i64,
    pub current: Option<AcceptanceRow>,
    pub events: Vec<EventRow>,
    pub authority: bool,
}

impl AcceptanceFacts {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.branches < 0
            || self.levels < 0
            || self.ceo < 0
            || self.incomplete < 0
            || self.events.len() > MAX_EVENTS
        {
            return Err(invalid());
        }
        if let Some(current) = &self.current {
            if !matches!(
                current.status.as_str(),
                "draft" | "submitted" | "rejected" | "accepted" | "revoked"
            ) || current.version < 1
            {
                return Err(invalid());
            }
        }
        for event in &self.events {
            if !matches!(
                event.action.as_str(),
                "submitted" | "rejected" | "accepted" | "revoked"
            ) || event.version < 1
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Check {
    pub id: String,
    pub label: String,
    pub ready: bool,
    pub owner: String,
    pub detail: String,
}

/// Complete payload of the retired `companyBusinessAcceptance` read.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceResult {
    pub company_id: String,
    pub status: String,
    pub version: i64,
    pub checks: Vec<Check>,
    pub blockers: Vec<String>,
    pub can_submit: bool,
    pub can_review: bool,
    pub can_revoke: bool,
    pub requested_by: Option<String>,
    pub reviewed_by: Option<String>,
    pub reason: Option<String>,
    pub review_reason: Option<String>,
    pub updated_at: Option<i64>,
    pub events: Vec<EventRow>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceEnvelope {
    pub data: AcceptanceResult,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query() -> AcceptanceQuery {
        AcceptanceQuery {
            company: "acme-co".into(),
            user: "user-1".into(),
            assignment: "assignment-1".into(),
            permissions: vec!["organization.manage".into()],
            person_ids: vec![],
            scope: "holding".into(),
            branch: None,
            department: None,
        }
    }

    #[test]
    fn bounded_company_context_is_accepted() {
        assert!(query().validate().is_ok());
    }

    #[test]
    fn narrow_scope_and_malformed_ids_fail_closed() {
        let mut scoped = query();
        scoped.scope = "branch".into();
        assert_eq!(scoped.validate(), Err(forbidden()));
        let mut branch = query();
        branch.branch = Some("branch-1".into());
        assert_eq!(branch.validate(), Err(forbidden()));
        let mut invalid_company = query();
        invalid_company.company = "../bad".into();
        assert_eq!(invalid_company.validate(), Err(invalid()));
    }

    #[test]
    fn same_person_includes_the_verified_person_user_ids() {
        let mut actor = query();
        assert!(actor.same_person("user-1"));
        assert!(!actor.same_person("person-user"));
        actor.person_ids.push("person-user".into());
        assert!(actor.same_person("person-user"));
    }
}
