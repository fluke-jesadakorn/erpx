//! Lane 39 Brand domain: pure response types, validation and invariants for the
//! owner-executed workspace-choice publication read.
//!
//! No SQL, no transport and no I/O live here. The only Brand-owned relation in
//! this capability is `organization_brand_profiles`; the holding-scope rows are
//! returned for every caller and branch rows are bounded by the supplied Company
//! identities, preserving the retired TypeScript read exactly.

use erpx_server_shared::public::BusinessError;

/// Bounded request size. A workspace never selects more than this many
/// Companies; the retired owner read applied the same ceiling.
pub const MAX_COMPANY_IDS: usize = 1000;
pub const MAX_ID_LEN: usize = 200;

/// The two publication scope kinds the workspace presentation understands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScopeKind {
    Holding,
    Branch,
}

impl ScopeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Holding => "holding",
            Self::Branch => "branch",
        }
    }

    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "holding" => Ok(Self::Holding),
            "branch" => Ok(Self::Branch),
            _ => Err(invalid(
                "BRAND_WORKSPACE_CHOICE_RESPONSE_INVALID",
                "The native Brand workspace choice returned an unknown scope kind.",
            )),
        }
    }
}

/// One publication choice. Field names mirror the owner wire contract used by
/// the workspace presentation (`scope_kind`, `scope_id`, `primary_publication_id`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceChoice {
    pub scope_kind: ScopeKind,
    pub scope_id: String,
    pub primary_publication_id: Option<String>,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

/// A bounded identifier: non-empty, length-limited and free of control, path
/// and quote characters. Only Company identities from the verified work context
/// reach this validator; a request may never supply SQL or a relation name.
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_LEN
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

/// Validate the caller-supplied Company identities before they become bind
/// values. Empty, oversized or malformed sets fail closed.
pub fn validate_company_ids(ids: &[String]) -> Result<(), BusinessError> {
    if ids.is_empty() || ids.len() > MAX_COMPANY_IDS {
        return Err(invalid(
            "BRAND_WORKSPACE_CHOICE_REQUEST_INVALID",
            "Use one bounded set of Company identities.",
        ));
    }
    if !ids.iter().all(|id| valid_id(id)) {
        return Err(invalid(
            "BRAND_WORKSPACE_CHOICE_REQUEST_INVALID",
            "Use one bounded set of Company identities.",
        ));
    }
    Ok(())
}

/// Validate a row projected by the postgres adapter before it is rendered.
fn valid_row(row: &WorkspaceChoice) -> bool {
    valid_id(&row.scope_id)
        && row
            .primary_publication_id
            .as_deref()
            .map(valid_id)
            .unwrap_or(true)
}

/// Project the validated rows into the exact response shape. Any invariant
/// violation fails closed rather than emitting a partial choice.
pub fn project(rows: Vec<WorkspaceChoice>) -> Result<Vec<WorkspaceChoice>, BusinessError> {
    if rows.len() > MAX_COMPANY_IDS.saturating_mul(2) {
        return Err(invalid(
            "BRAND_WORKSPACE_CHOICE_RESPONSE_INVALID",
            "The native Brand workspace choice returned too many rows.",
        ));
    }
    for row in &rows {
        if !valid_row(row) {
            return Err(invalid(
                "BRAND_WORKSPACE_CHOICE_RESPONSE_INVALID",
                "The native Brand workspace choice returned an invalid row.",
            ));
        }
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn company_ids_are_bounded_and_malformed_sets_fail_closed() {
        assert!(validate_company_ids(&["co-a".into(), "co_b:1".into()]).is_ok());
        assert_eq!(
            validate_company_ids(&[]).unwrap_err().code,
            "BRAND_WORKSPACE_CHOICE_REQUEST_INVALID"
        );
        assert_eq!(
            validate_company_ids(&["bad id".into()]).unwrap_err().code,
            "BRAND_WORKSPACE_CHOICE_REQUEST_INVALID"
        );
        assert_eq!(
            validate_company_ids(&vec!["co".into(); MAX_COMPANY_IDS + 1])
                .unwrap_err()
                .code,
            "BRAND_WORKSPACE_CHOICE_REQUEST_INVALID"
        );
    }

    #[test]
    fn scope_kind_only_accepts_the_two_owner_kinds() {
        assert_eq!(ScopeKind::parse("holding").unwrap(), ScopeKind::Holding);
        assert_eq!(ScopeKind::parse("branch").unwrap(), ScopeKind::Branch);
        assert!(ScopeKind::parse("department").is_err());
    }

    #[test]
    fn projection_refuses_invalid_rows() {
        let valid = WorkspaceChoice {
            scope_kind: ScopeKind::Holding,
            scope_id: "holding-1".into(),
            primary_publication_id: Some("pub-1".into()),
        };
        assert_eq!(project(vec![valid]).unwrap().len(), 1);
        let invalid_row = WorkspaceChoice {
            scope_kind: ScopeKind::Branch,
            scope_id: String::new(),
            primary_publication_id: None,
        };
        assert_eq!(
            project(vec![invalid_row]).unwrap_err().code,
            "BRAND_WORKSPACE_CHOICE_RESPONSE_INVALID"
        );
    }
}
