//! Pure transport facts for the native R3 leave-requests owner slice.
//!
//! The TypeScript routes wrap every response in `{data: ...}` (plus the
//! paged list's `hasMore`/`nextCursor`). These types and validators own that
//! shape, the bounded list filter, the keyset cursor and the error codes the
//! retired routes returned. No SQL and no other owner's facts live here.

use base64::{Engine, alphabet, engine::general_purpose::URL_SAFE_NO_PAD};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

pub const MAX_BODY: usize = 64 * 1024;
pub const MAX_LIMIT: usize = 50;
pub const MAX_CURSOR: usize = 512;
pub const MAX_KEY: usize = 180;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaveListFilter {
    All,
    Drafts,
    Review,
    Approved,
    Closed,
}

impl LeaveListFilter {
    /// `leaveListFilterSchema` is a strict enum with an `all` default.
    pub fn parse(raw: Option<&str>) -> Result<Self, BusinessError> {
        match raw.unwrap_or("all") {
            "all" => Ok(Self::All),
            "drafts" => Ok(Self::Drafts),
            "review" => Ok(Self::Review),
            "approved" => Ok(Self::Approved),
            "closed" => Ok(Self::Closed),
            _ => Err(invalid("VALIDATION_FAILED", "Invalid leave list filter.")),
        }
    }

    /// The exact predicate retained from `listOwn` in the TypeScript adapter.
    pub fn clause(self) -> &'static str {
        match self {
            Self::All => "TRUE",
            Self::Drafts => "r.state='draft'",
            Self::Review => "(r.state='pending_approval' OR r.cancellation_state='pending_review')",
            Self::Approved => "(r.state='approved' AND r.cancellation_state IS DISTINCT FROM 'pending_review')",
            Self::Closed => "r.state NOT IN ('draft','pending_approval','approved')",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LeaveCursor {
    pub at: i64,
    pub id: String,
}

impl LeaveCursor {
    /// The retired adapter encoded `JSON.stringify({at, id})` with base64url.
    pub fn decode(raw: &str) -> Result<Self, BusinessError> {
        if raw.is_empty() || raw.len() > MAX_CURSOR {
            return Err(invalid("VALIDATION_FAILED", "Invalid leave cursor."));
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(raw)
            .map_err(|_| invalid("VALIDATION_FAILED", "Invalid leave cursor."))?;
        let cursor: Self = serde_json::from_slice(&bytes)
            .map_err(|_| invalid("VALIDATION_FAILED", "Invalid leave cursor."))?;
        if cursor.at < 0 || cursor.id.is_empty() || cursor.id.len() > 180 {
            return Err(invalid("VALIDATION_FAILED", "Invalid leave cursor."));
        }
        Ok(cursor)
    }

    pub fn encode(&self) -> String {
        let bytes = serde_json::to_vec(self).unwrap_or_default();
        URL_SAFE_NO_PAD.encode(bytes)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct LeaveListRow {
    pub id: String,
    pub number: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub start_segment: Option<String>,
    pub end_segment: Option<String>,
    pub calculated_units: Option<f64>,
    pub days: i64,
    pub state: String,
    pub cancellation_state: Option<String>,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub decision_note: Option<String>,
    pub leave_type_name: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LeaveListEnvelope {
    pub data: Vec<LeaveListRow>,
    #[serde(rename = "hasMore")]
    pub has_more: bool,
    #[serde(rename = "nextCursor")]
    pub next_cursor: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaveCommandKind {
    Create,
    Preview,
    Edit,
    Transition,
    CancellationReview,
    Approve,
}

impl LeaveCommandKind {
    /// Exact idempotency error code used by each retired route.
    pub fn key_code(self) -> Option<&'static str> {
        match self {
            Self::Preview => None,
            Self::Create | Self::Transition => Some("COMMAND_KEY_REQUIRED"),
            Self::Edit | Self::CancellationReview | Self::Approve => {
                Some("IDEMPOTENCY_KEY_REQUIRED")
            }
        }
    }
}

pub fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}

pub fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Leave request not found.")
}

pub fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}

pub fn configuration_invalid() -> BusinessError {
    invalid(
        "PEOPLE_PAYROLL_LEAVE_R3_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 180
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

/// The retired GET accepts `limit`, `cursor` and `filter` only for the paged
/// branch: the presence of any one of them selects the paged read. A duplicate
/// or unknown key fails closed before any SQL is issued.
pub fn page_query(raw: Option<&str>) -> Result<Option<(LeaveListFilter, usize, Option<LeaveCursor>)>, BusinessError> {
    let mut found: Vec<(String, String)> = Vec::new();
    for pair in raw.unwrap_or("").split('&').filter(|part| !part.is_empty()) {
        match pair.split_once('=') {
            Some((name, value)) if matches!(name, "limit" | "cursor" | "filter") => {
                found.push((name.to_owned(), value.to_owned()))
            }
            _ => return Err(invalid("VALIDATION_FAILED", "Invalid leave list query.")),
        }
    }
    if found.is_empty() {
        return Ok(None);
    }
    let mut limit = None;
    let mut cursor = None;
    let mut filter = None;
    for (name, value) in found {
        match name.as_str() {
            "limit" => {
                if limit.replace(value).is_some() {
                    return Err(invalid("VALIDATION_FAILED", "Duplicate limit."));
                }
            }
            "cursor" => {
                if cursor.replace(value).is_some() {
                    return Err(invalid("VALIDATION_FAILED", "Duplicate cursor."));
                }
            }
            "filter" => {
                if filter.replace(value).is_some() {
                    return Err(invalid("VALIDATION_FAILED", "Duplicate filter."));
                }
            }
            _ => {}
        }
    }
    let limit = match limit {
        Some(raw) => {
            let value: usize = raw
                .parse()
                .map_err(|_| invalid("VALIDATION_FAILED", "Invalid limit."))?;
            if !(1..=MAX_LIMIT).contains(&value) {
                return Err(invalid("VALIDATION_FAILED", "Invalid limit."));
            }
            value
        }
        None => 10,
    };
    let cursor = cursor.map(|value| LeaveCursor::decode(&value)).transpose()?;
    let filter = LeaveListFilter::parse(filter.as_deref())?;
    Ok(Some((filter, limit, cursor)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_keep_the_retained_predicates() {
        assert_eq!(LeaveListFilter::parse(None).unwrap().clause(), "TRUE");
        assert_eq!(LeaveListFilter::parse(Some("review")).unwrap().clause(), "(r.state='pending_approval' OR r.cancellation_state='pending_review')");
        assert!(LeaveListFilter::parse(Some("unknown")).is_err());
    }

    #[test]
    fn cursor_round_trips_as_base64url_json() {
        let cursor = LeaveCursor { at: 1_700_000_000_000, id: "leave-1".into() };
        let encoded = cursor.encode();
        assert!(!encoded.contains('=') && !encoded.contains('+') && !encoded.contains('/'));
        let decoded = LeaveCursor::decode(&encoded).unwrap();
        assert_eq!(decoded.at, cursor.at);
        assert_eq!(decoded.id, cursor.id);
        assert!(LeaveCursor::decode("not-json").is_err());
    }

    #[test]
    fn page_query_rejects_unknown_and_duplicate_keys() {
        assert!(page_query(Some("limit=10&filter=drafts")).unwrap().is_some());
        assert!(page_query(Some("limit=1&limit=2")).is_err());
        assert!(page_query(Some("other=1")).is_err());
        assert!(page_query(Some("limit=51")).is_err());
    }

    #[test]
    fn command_keys_match_each_retired_route() {
        assert_eq!(LeaveCommandKind::Create.key_code(), Some("COMMAND_KEY_REQUIRED"));
        assert_eq!(LeaveCommandKind::Preview.key_code(), None);
        assert_eq!(LeaveCommandKind::Approve.key_code(), Some("IDEMPOTENCY_KEY_REQUIRED"));
    }
}
