//! Portable Records reference-search policy.
//!
//! The search read is Records-owned: it selects Company/scope-limited
//! `workspace_records` rows and canonicalises the legacy term/needle shaping.
//! Cross-owner admission (which rows and which principals the current actor may
//! see) is supplied by composition-owned ports, never by this module. No SQL,
//! no HTTP and no I/O live here.

use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;

/// Contract marker for the native Records search read.
pub const CONTRACT_VERSION: &str = "records.search.v1";
/// Final reply bound. The legacy reader sliced to twelve hits.
pub const MAX_HITS: usize = 12;
/// Hard upper bound on owner rows scanned before admission. A page can never
/// scan an unbounded number of hidden rows to fill the reply.
pub const MAX_SCAN: usize = 300;
/// Strict request bounds.
pub const MAX_QUERY_CHARS: usize = 200;
pub const MAX_QUERY_BYTES: usize = 800;
pub const MAX_ID: usize = 200;

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}

pub fn request_invalid() -> BusinessError {
    invalid(
        "RECORDS_SEARCH_REQUEST_INVALID",
        "Choose a valid search term and scope.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_RECORDS_SEARCH_UNAVAILABLE",
        "The native Records search is unavailable.",
    )
}

pub fn configuration_invalid() -> BusinessError {
    invalid(
        "NATIVE_RECORDS_SEARCH_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

/// Exact registered Holding target the native route accepts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchTarget {
    pub host: String,
    pub holding_slug: String,
}

/// The current, composition-resolved actor. Fields stay bounded by the HTTP
/// adapter and are re-used by the owner SQL scope and the admission guard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchActor {
    pub user_id: String,
    pub assignment_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    /// The modules the current assignment may read. Bound by the HTTP adapter
    /// and used as a bind parameter, never inlined into SQL text.
    pub modules: Vec<String>,
}

pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID
        && !value.contains('\0')
        && !value.bytes().any(|byte| byte.is_ascii_control())
}

impl SearchActor {
    /// Reject an actor with missing or malformed scope before any read starts.
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_id(&self.user_id)
            || !valid_id(&self.assignment_id)
            || !valid_id(&self.company_id)
            || self.branch_id.as_deref().is_some_and(|value| !valid_id(value))
            || self
                .department_id
                .as_deref()
                .is_some_and(|value| !valid_id(value))
            || self.modules.iter().any(|value| !valid_id(value))
            || self.modules.len() > 128
        {
            return Err(request_invalid());
        }
        Ok(())
    }
}

/// Validated search query. The raw term is kept private so a caller cannot
/// bypass the length/control bounds or the retained `@` normalisation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchQuery {
    raw: String,
}

impl SearchQuery {
    pub fn new(raw: &str) -> Result<Self, BusinessError> {
        if raw.is_empty()
            || raw.len() > MAX_QUERY_BYTES
            || raw.chars().count() > MAX_QUERY_CHARS
            || raw.chars().any(|c| c == '\0' || c.is_control())
        {
            return Err(request_invalid());
        }
        Ok(Self {
            raw: raw.to_owned(),
        })
    }

    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// Legacy record term: one leading `@` is stripped, the rest is wrapped in
    /// SQL `ILIKE` wildcards. This is bound as a parameter, never inlined.
    pub fn term(&self) -> String {
        format!("%{}%", self.raw.strip_prefix('@').unwrap_or(&self.raw))
    }

    /// Legacy principal needle: strip `@`, then an optional `user/` prefix, and
    /// lowercase before the control-side principal lookup.
    pub fn needle(&self) -> String {
        let rest = self.raw.strip_prefix('@').unwrap_or(&self.raw);
        rest.strip_prefix("user/").unwrap_or(rest).to_lowercase()
    }
}

/// One owner-local record hit. Field names are the retained snake_case SQL
/// projection names; the axum reply is byte-identical to the legacy payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordHit {
    pub module_id: String,
    pub id: String,
    pub title: String,
    pub status: String,
    pub version: i64,
}

impl RecordHit {
    pub fn to_wire(&self) -> Value {
        serde_json::json!({
            "kind": "record",
            "module_id": self.module_id,
            "id": self.id,
            "title": self.title,
            "status": self.status,
            "version": self.version,
        })
    }
}

/// One admitted principal hit. Field names are the retained contract names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrincipalHit {
    pub target_id: String,
    pub label: String,
}

impl PrincipalHit {
    pub fn to_wire(&self) -> Value {
        serde_json::json!({
            "kind": "person",
            "target_id": self.target_id,
            "label": self.label,
        })
    }
}

/// Final owner response envelope. Exactly one key under `data`, matching the
/// legacy `{ data: hits }` reply before the presentation link projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchResponse {
    pub hits: Vec<Value>,
}

impl SearchResponse {
    pub fn value(self) -> Value {
        serde_json::json!({"data": self.hits})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn term_and_needle_match_the_legacy_normalisation() {
        let plain = SearchQuery::new("Desk").unwrap();
        assert_eq!(plain.term(), "%Desk%");
        assert_eq!(plain.needle(), "desk");
        let mention = SearchQuery::new("@user/Alice").unwrap();
        assert_eq!(mention.term(), "%user/Alice%");
        assert_eq!(mention.needle(), "alice");
        let record = SearchQuery::new("@operations/rec-1").unwrap();
        assert_eq!(record.term(), "%operations/rec-1%");
        assert_eq!(record.needle(), "operations/rec-1");
    }

    #[test]
    fn query_bounds_are_strict() {
        assert!(SearchQuery::new("needle").is_ok());
        assert!(SearchQuery::new("").is_err());
        assert!(SearchQuery::new("\0").is_err());
        assert!(SearchQuery::new(&"x".repeat(201)).is_err());
    }

    #[test]
    fn wire_fields_are_the_exact_legacy_shape() {
        let record = RecordHit {
            module_id: "operations".into(),
            id: "rec-1".into(),
            title: "Desk".into(),
            status: "draft".into(),
            version: 3,
        }
        .to_wire();
        let object = record.as_object().unwrap();
        assert_eq!(object.len(), 6);
        for key in ["kind", "module_id", "id", "title", "status", "version"] {
            assert!(object.contains_key(key));
        }
        assert_eq!(record["kind"], "record");
        let person = PrincipalHit {
            target_id: "u1".into(),
            label: "Alice".into(),
        }
        .to_wire();
        assert_eq!(person["kind"], "person");
        assert_eq!(person.as_object().unwrap().len(), 3);
        assert!(record.to_string().find("href").is_none());
    }

    #[test]
    fn actor_scope_is_bounded() {
        let actor = SearchActor {
            user_id: "u1".into(),
            assignment_id: "a1".into(),
            company_id: "c1".into(),
            branch_id: Some("b1".into()),
            department_id: None,
            modules: vec!["operations".into()],
        };
        assert!(actor.validate().is_ok());
        assert!(SearchActor {
            user_id: "".into(),
            ..actor
        }
        .validate()
        .is_err());
    }
}
