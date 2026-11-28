//! Pure response types and invariants for the native Organization draft read.
//!
//! This module owns no SQL and no transport. It describes the exact payload the
//! owner may return for `GET /organization/drafts` and validates it before a
//! reply is released, so the TypeScript `getOrganizationDraft` authority can be
//! retired without dropping any field.

use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Serialize;
use serde_json::Value;

/// The recent-draft list is owner-capped by SQL at 20 rows. Bound the request
/// list too so a malformed owner read can never become an unbounded payload.
pub const MAX_RECENT: usize = 20;
pub const MAX_REQUESTS: usize = 20_000;
pub const MAX_ID: usize = 200;

/// Both permissions are required by the retained `draftAuthority` fence.
pub const REQUIRED_PERMISSIONS: [&str; 2] = ["people.manage", "organization.manage"];

/// Verified browser context, injected by the native gateway as bounded headers.
/// A missing or non-Holding scope fails closed; no permission is inferred here.
#[derive(Clone, Debug)]
pub struct DraftReadQuery {
    pub company: String,
    pub user: String,
    pub permissions: Vec<String>,
    pub scope: String,
    pub branch: Option<String>,
    pub department: Option<String>,
    pub draft_id: Option<String>,
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "ORGANIZATION_DRAFT_READ_INVALID",
        "Use one bounded Organization draft read.",
    )
}

fn forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "COMPANY_DRAFT_AUTHORITY_REQUIRED",
        "Open the Company workspace with Company HR authority to read drafts.",
    )
}

/// Bounded owner-scope identifier (same shape every other Organization native
/// route accepts). No caller text reaches SQL.
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

impl DraftReadQuery {
    /// Fail-closed request check applied before the owner reader is reachable.
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_id(&self.company)
            || !valid_id(&self.user)
            || self.permissions.len() > 128
            || self
                .permissions
                .iter()
                .any(|permission| !valid_id(permission))
            || self
                .draft_id
                .as_deref()
                .is_some_and(|id| !valid_id(id))
        {
            return Err(invalid());
        }
        // The retained authority requires the Company (Holding) workspace
        // scope: an actor with a branch or department assignment cannot read it.
        if self.scope != "holding" || self.branch.is_some() || self.department.is_some() {
            return Err(forbidden());
        }
        for required in REQUIRED_PERMISSIONS {
            if !self.permissions.iter().any(|permission| permission == required) {
                return Err(forbidden());
            }
        }
        Ok(())
    }
}

/// Full `organization_drafts` row. Field order matches the TypeScript
/// `SELECT *` projection so the emitted JSON is field-for-field identical.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DraftRow {
    pub id: String,
    pub company_id: String,
    pub owner_id: String,
    pub state: String,
    pub version: i32,
    pub definition: Value,
    pub created_at: i64,
    pub updated_at: i64,
    pub base_snapshot: Value,
}

/// `organization_draft_requests` projection; `item_keys` is a decoded JSON array.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RequestRow {
    pub id: String,
    pub item_keys: Value,
    pub state: String,
    pub last_error: Option<String>,
    pub change_id: Option<String>,
}

/// Recent submitted-draft projection (`jsonb_array_length(definition->'items')`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RecentRow {
    pub id: String,
    pub updated_at: i64,
    pub count: i64,
}

/// Complete payload of the retired `getOrganizationDraft` read.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct DraftReadResult {
    pub draft: Option<DraftRow>,
    pub requests: Vec<RequestRow>,
    pub recent: Vec<RecentRow>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DraftReadEnvelope {
    pub data: DraftReadResult,
}

impl RequestRow {
    fn validate(&self) -> Result<(), BusinessError> {
        if self.id.is_empty()
            || !self.item_keys.is_array()
            || !matches!(
                self.state.as_str(),
                "scheduled" | "waiting_approval" | "waiting_dependency" | "applied" | "needs_review"
            )
        {
            return Err(invalid());
        }
        Ok(())
    }
}

impl DraftReadResult {
    /// Owner-side shape check: a reader cannot leak an unbounded or malformed
    /// projection even if its SQL is later changed incorrectly.
    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.recent.len() > MAX_RECENT || self.requests.len() > MAX_REQUESTS {
            return Err(invalid());
        }
        if let Some(draft) = &self.draft {
            if draft.id.is_empty()
                || draft.company_id.is_empty()
                || draft.owner_id.is_empty()
                || !matches!(draft.state.as_str(), "draft" | "submitted" | "discarded")
                || !draft.definition.is_object()
                || !draft.base_snapshot.is_object()
                || draft.version < 1
            {
                return Err(invalid());
            }
        }
        for request in &self.requests {
            request.validate()?;
        }
        for row in &self.recent {
            if row.id.is_empty() || row.count < 0 {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query() -> DraftReadQuery {
        DraftReadQuery {
            company: "acme-co".into(),
            user: "user-1".into(),
            permissions: REQUIRED_PERMISSIONS.iter().map(|value| value.to_string()).collect(),
            scope: "holding".into(),
            branch: None,
            department: None,
            draft_id: None,
        }
    }

    fn result() -> DraftReadResult {
        DraftReadResult {
            draft: Some(DraftRow {
                id: "draft-1".into(),
                company_id: "acme-co".into(),
                owner_id: "user-1".into(),
                state: "draft".into(),
                version: 1,
                definition: serde_json::json!({"items": [], "reason": "Move"}),
                created_at: 1,
                updated_at: 2,
                base_snapshot: serde_json::json!({}),
            }),
            requests: vec![RequestRow {
                id: "request-1".into(),
                item_keys: serde_json::json!(["person:eng-1"]),
                state: "scheduled".into(),
                last_error: None,
                change_id: None,
            }],
            recent: vec![RecentRow {
                id: "draft-0".into(),
                updated_at: 1,
                count: 2,
            }],
        }
    }

    #[test]
    fn bounded_context_is_accepted() {
        assert!(query().validate().is_ok());
        let mut by_id = query();
        by_id.draft_id = Some("draft-1".into());
        assert!(by_id.validate().is_ok());
    }

    #[test]
    fn missing_permission_or_narrow_scope_fails_closed() {
        let mut missing = query();
        missing.permissions = vec!["people.manage".into()];
        assert_eq!(missing.validate(), Err(forbidden()));
        let mut scoped = query();
        scoped.scope = "branch".into();
        assert_eq!(scoped.validate(), Err(forbidden()));
        let mut with_branch = query();
        with_branch.branch = Some("branch-1".into());
        assert_eq!(with_branch.validate(), Err(forbidden()));
    }

    #[test]
    fn malformed_identifiers_fail_closed() {
        let mut bad = query();
        bad.company = "../bad".into();
        assert_eq!(bad.validate(), Err(invalid()));
        let mut bad = query();
        bad.draft_id = Some("a b".into());
        assert_eq!(bad.validate(), Err(invalid()));
    }

    #[test]
    fn complete_payload_is_stable_and_validates() {
        let payload = result();
        payload.validate().unwrap();
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["draft"]["definition"]["reason"], "Move");
        assert_eq!(json["requests"][0]["item_keys"][0], "person:eng-1");
        assert_eq!(json["recent"][0]["count"], 2);
    }

    #[test]
    fn malformed_owner_read_is_rejected() {
        let mut bad = result();
        bad.requests[0].item_keys = serde_json::json!("not-an-array");
        assert_eq!(bad.validate(), Err(invalid()));
        let mut bad = result();
        bad.draft.as_mut().unwrap().state = "unknown".into();
        assert_eq!(bad.validate(), Err(invalid()));
        let mut bad = result();
        bad.recent = (0..MAX_RECENT + 1)
            .map(|index| RecentRow {
                id: format!("draft-{index}"),
                updated_at: 1,
                count: 0,
            })
            .collect();
        assert_eq!(bad.validate(), Err(invalid()));
    }
}
