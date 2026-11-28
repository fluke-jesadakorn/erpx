//! Pure domain types for the native lane 11 person-identity read family.
//!
//! The axum owner owns the two `iam_members` queries, the dedupe/successor
//! projection and the wire shape. This module executes no SQL: it validates the
//! bounded Company scope and input identities, and enforces the projection
//! invariants the TypeScript callers used to assemble by hand.

use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Maximum identities accepted in one request. The TypeScript callers already
/// pass bounded lists; this cap keeps the rendered `IN (...)` bind list safe.
pub const MAX_IDS: usize = 200;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityIdsRequest {
    pub ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrincipalsRequest {
    pub ids: Vec<String>,
    #[serde(default)]
    pub snapshot: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityIdsResponse {
    pub identity_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrincipalPair {
    pub original: String,
    pub current: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrincipalsResponse {
    pub principals: Vec<PrincipalPair>,
}

/// A raw successor row returned by the owner SQL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuccessorRow {
    pub original_id: String,
    pub current_id: String,
}

/// The four owner-local directory configuration reads that used to be inline
/// in `adapters/iam-service-reads.ts`.
pub const DIRECTORY_KINDS: [&str; 4] = ["connections", "groups", "previews", "identities"];
const MAX_DIRECTORY_ROWS: usize = 500;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryConfigRequest {
    #[serde(default)]
    pub can_manage: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryConfigResponse {
    pub rows: Vec<Value>,
}

pub fn valid_directory_kind(kind: &str) -> bool {
    DIRECTORY_KINDS.contains(&kind)
}

impl DirectoryConfigResponse {
    /// The owner projection must be a bounded list of JSON objects with a
    /// stable identity. Fail closed rather than emit a partial configuration.
    pub fn validated(self) -> Result<Self, BusinessError> {
        if self.rows.len() > MAX_DIRECTORY_ROWS
            || self.rows.iter().any(|row| {
                !matches!(row, Value::Object(_))
                    || row
                        .get("id")
                        .and_then(Value::as_str)
                        .is_none_or(|id| id.is_empty() || id.len() > 200)
            })
        {
            return Err(unavailable());
        }
        Ok(self)
    }
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "IDENTITY_ACCESS_NATIVE_LANE_11_REQUEST_INVALID",
        "Use one bounded person identity read.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "IDENTITY_ACCESS_NATIVE_LANE_11_UNAVAILABLE",
        "The native person identity service is unavailable.",
    )
}

/// Published role definition shared by the IAM store and the IAM overview.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleRevision {
    pub id: String,
    pub name: String,
    pub company_id: String,
    pub revision: i64,
    pub built_in: bool,
    pub permissions: Vec<String>,
    pub grantable_permissions: Vec<String>,
    pub scopes: Vec<String>,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleRequest {
    pub id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RolesRequest {
    pub ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleResponse {
    pub role: Option<RoleRevision>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RolesResponse {
    pub roles: Vec<RoleRevision>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleIdsResponse {
    pub ids: Vec<String>,
}

impl RoleRevision {
    /// Fail closed on an impossible published definition rather than emitting
    /// a malformed permission set.
    pub fn validated(self) -> Result<Self, BusinessError> {
        if self.id.is_empty()
            || self.id.len() > 200
            || self.name.is_empty()
            || self.name.len() > 200
            || self.status != "active"
        {
            return Err(unavailable());
        }
        Ok(self)
    }
}

pub fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "IDENTITY_ACCESS_NATIVE_LANE_11_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

/// Company ids are opaque binds, never interpolated into SQL.
pub fn valid_company(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

/// User ids may be UUIDs, directory principals or local identifiers; only the
/// bound length and absence of control characters are asserted here.
pub fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| !b.is_ascii_control() && b != b'\'' && b != b'"')
}

/// Registered Holding slugs are lowercase, bounded path segments.
pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

pub fn validate_ids(ids: &[String]) -> Result<(), BusinessError> {
    if ids.is_empty() || ids.len() > MAX_IDS || ids.iter().any(|id| !valid_id(id)) {
        return Err(invalid());
    }
    Ok(())
}

/// Preserve first-seen order while deduplicating, exactly as the previous
/// `[...new Set([...ids, ...aliases])]` projection did.
pub fn identity_ids(ids: &[String], aliases: Vec<String>) -> IdentityIdsResponse {
    let mut seen = Vec::with_capacity(ids.len() + aliases.len());
    for id in ids.iter().cloned().chain(aliases) {
        if !seen.contains(&id) {
            seen.push(id);
        }
    }
    IdentityIdsResponse {
        identity_ids: seen,
    }
}

/// Restore `createCurrentPersonPrincipals`: every input id maps to itself unless
/// exactly one active successor row exists for that original id. Output order
/// follows the first occurrence of each input id.
pub fn principals(ids: &[String], rows: Vec<SuccessorRow>) -> PrincipalsResponse {
    let mut unique = Vec::with_capacity(ids.len());
    for id in ids {
        if !unique.contains(id) {
            unique.push(id.clone());
        }
    }
    let mut pairs = Vec::with_capacity(unique.len());
    for id in &unique {
        let mut matches = rows.iter().filter(|row| &row.original_id == id);
        let current = match (matches.next(), matches.next()) {
            (Some(row), None) => row.current_id.clone(),
            _ => id.clone(),
        };
        pairs.push(PrincipalPair {
            original: id.clone(),
            current,
        });
    }
    PrincipalsResponse {
        principals: pairs,
    }
}

impl IdentityIdsResponse {
    /// Fail closed if the projection lost an input identity.
    pub fn validated(self, ids: &[String]) -> Result<Self, BusinessError> {
        if ids.iter().any(|id| !self.identity_ids.contains(id)) {
            return Err(unavailable());
        }
        Ok(self)
    }
}

impl PrincipalsResponse {
    /// Fail closed on a malformed map: exactly one entry per distinct input,
    /// with a non-empty original and current.
    pub fn validated(self, ids: &[String]) -> Result<Self, BusinessError> {
        let mut unique = Vec::with_capacity(ids.len());
        for id in ids {
            if !unique.contains(id) {
                unique.push(id.clone());
            }
        }
        if self.principals.len() != unique.len()
            || self
                .principals
                .iter()
                .zip(unique.iter())
                .any(|(pair, id)| &pair.original != id || pair.current.is_empty())
        {
            return Err(unavailable());
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_company_and_inputs() {
        assert!(valid_company("acme:company-01"));
        assert!(!valid_company("bad/id?"));
        assert!(!valid_company(&"x".repeat(201)));
        assert!(valid_id("user@example.test"));
        assert!(!valid_id(""));
        assert!(!valid_id("bad\nid"));
        assert!(validate_ids(&["a".into(), "b".into()]).is_ok());
        assert!(validate_ids(&[]).is_err());
        assert!(validate_ids(&vec!["a".into(); MAX_IDS + 1]).is_err());
    }

    #[test]
    fn identity_ids_dedupe_preserves_first_seen_order() {
        let ids = vec!["u1".to_owned(), "u2".to_owned()];
        let projected = identity_ids(&ids, vec!["u2".to_owned(), "u3".to_owned()]);
        assert_eq!(
            projected,
            IdentityIdsResponse {
                identity_ids: vec!["u1".into(), "u2".into(), "u3".into()],
            }
        );
        assert!(projected.validated(&ids).is_ok());
    }

    #[test]
    fn principals_use_identity_unless_exactly_one_successor() {
        let ids = vec!["old".to_owned(), "solo".to_owned(), "none".to_owned()];
        let rows = vec![
            SuccessorRow { original_id: "old".into(), current_id: "a".into() },
            SuccessorRow { original_id: "old".into(), current_id: "b".into() },
            SuccessorRow { original_id: "solo".into(), current_id: "new".into() },
        ];
        let projected = principals(&ids, rows);
        assert_eq!(
            projected,
            PrincipalsResponse {
                principals: vec![
                    PrincipalPair { original: "old".into(), current: "old".into() },
                    PrincipalPair { original: "solo".into(), current: "new".into() },
                    PrincipalPair { original: "none".into(), current: "none".into() },
                ],
            }
        );
        assert!(projected.validated(&ids).is_ok());
    }

    #[test]
    fn malformed_projections_fail_closed() {
        let ids = vec!["a".to_owned()];
        assert_eq!(
            IdentityIdsResponse { identity_ids: vec![] }.validated(&ids),
            Err(unavailable())
        );
        assert_eq!(
            PrincipalsResponse { principals: vec![] }.validated(&ids),
            Err(unavailable())
        );
    }
}
