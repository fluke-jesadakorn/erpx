//! Portable Records registry list, search and page policy.
//!
//! The registry is the Records-owned `workspace_records` collection for one
//! module section. This module owns the strict request bounds, opaque page
//! cursor, page assembly and wire shape. It never opens a transaction, reads
//! HTTP or admits a cross-owner source; composition supplies the current
//! authority and a guard implemented with owning public ports.
use crate::domain::record_context::{
    RecordContextActor, RecordRow, RecordView, valid_id, valid_module, validate_record,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};

pub const CONTRACT_VERSION: &str = "records.registry.v1";
/// Retained list-page size. A page is a bounded, opaque-cursor unit.
pub const PAGE_SIZE: usize = 30;
/// Hard upper bound for one owner SQL read. The application never asks for more.
pub const MAX_FETCH: usize = 100;
/// An unpaged registry response is still bounded; larger sections must be paged.
pub const MAX_LIST_ROWS: usize = 500;
/// A request cannot scan an unbounded number of hidden rows to fill one page.
pub const MAX_SCAN_ROWS: usize = 2_000;
pub const MAX_CURSOR: usize = 4_096;
pub const MAX_SECTION_ID: usize = 128;
pub const MAX_SEARCH_CHARS: usize = 200;
pub const MAX_SEARCH_BYTES: usize = 800;

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}
fn unavailable(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, code, message)
}
pub fn request_invalid() -> BusinessError {
    invalid(
        "RECORDS_REGISTRY_REQUEST_INVALID",
        "Choose a valid module section and search.",
    )
}
pub fn invalid_cursor() -> BusinessError {
    invalid("INVALID_CURSOR", "Restart this search to load results.")
}
pub fn registry_source_invalid() -> BusinessError {
    unavailable(
        "RECORDS_REGISTRY_SOURCE_INVALID",
        "The record registry is not available.",
    )
}

/// Exact validated registry query. Fields stay private so callers cannot
/// bypass the module, section, search and cursor bounds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryQuery {
    module_id: String,
    section_id: String,
    q: Option<String>,
    cursor: Option<String>,
}

impl RegistryQuery {
    pub fn page(
        module_id: &str,
        section_id: &str,
        q: Option<&str>,
        cursor: Option<&str>,
    ) -> Result<Self, BusinessError> {
        validate_module_and_section(module_id, section_id)?;
        let q = match q {
            Some("") => None,
            Some(value) if valid_search(value) => Some(value.to_owned()),
            Some(_) => return Err(request_invalid()),
            None => None,
        };
        let cursor = match cursor {
            Some("") => return Err(invalid_cursor()),
            Some(value) if value.len() <= MAX_CURSOR => Some(value.to_owned()),
            Some(_) => return Err(invalid_cursor()),
            None => None,
        };
        Ok(Self {
            module_id: module_id.to_owned(),
            section_id: section_id.to_owned(),
            q,
            cursor,
        })
    }

    pub fn list(module_id: &str, section_id: &str) -> Result<Self, BusinessError> {
        validate_module_and_section(module_id, section_id)?;
        Ok(Self {
            module_id: module_id.to_owned(),
            section_id: section_id.to_owned(),
            q: None,
            cursor: None,
        })
    }

    pub fn module_id(&self) -> &str {
        &self.module_id
    }

    pub fn section_id(&self) -> &str {
        &self.section_id
    }

    pub fn q(&self) -> Option<&str> {
        self.q.as_deref()
    }

    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }
}

fn validate_module_and_section(module_id: &str, section_id: &str) -> Result<(), BusinessError> {
    if !valid_module(module_id) || !valid_section(section_id) {
        return Err(request_invalid());
    }
    Ok(())
}

fn valid_section(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SECTION_ID
        && value.trim() == value
        && !value.bytes().any(|byte| byte.is_ascii_control())
}

fn valid_search(value: &str) -> bool {
    value.len() <= MAX_SEARCH_BYTES
        && value.chars().count() <= MAX_SEARCH_CHARS
        && !value
            .chars()
            .any(|character| character == '\0' || character.is_control())
}

/// Opaque cursor identity. `key` is the record `updated_at` in milliseconds
/// and `id` is the deterministic tie breaker, matching the retained list order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryCursor {
    pub key: i64,
    pub id: String,
    pub reverse: bool,
    pub scope: String,
}

/// Cursor scope binds the page to the current actor, module, section, query
/// and page size. The value is opaque and only ever compared for equality.
pub fn registry_scope(actor: &RecordContextActor, query: &RegistryQuery) -> String {
    json!([
        CONTRACT_VERSION,
        actor.user_id,
        actor.assignment_id,
        actor.holding_id,
        actor.company_id,
        actor.branch_id,
        actor.department_id,
        query.module_id,
        query.section_id,
        query.q,
        PAGE_SIZE,
    ])
    .to_string()
}

pub fn decode_cursor(raw: &str, scope: &str) -> Result<RegistryCursor, BusinessError> {
    if raw.is_empty() || raw.len() > MAX_CURSOR {
        return Err(invalid_cursor());
    }
    let bytes = b64url_decode(raw).ok_or_else(invalid_cursor)?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| invalid_cursor())?;
    let Some(object) = value.as_object() else {
        return Err(invalid_cursor());
    };
    if object.len() != 4 {
        return Err(invalid_cursor());
    }
    let (Some(key), Some(id), Some(reverse), Some(scope_value)) = (
        object.get("key").and_then(Value::as_str),
        object.get("id").and_then(Value::as_str),
        object.get("reverse").and_then(Value::as_bool),
        object.get("scope").and_then(Value::as_str),
    ) else {
        return Err(invalid_cursor());
    };
    let key = key
        .parse::<i64>()
        .ok()
        .filter(|value| *value >= 0)
        .ok_or_else(invalid_cursor)?;
    if !valid_id(id) || scope_value != scope {
        return Err(invalid_cursor());
    }
    Ok(RegistryCursor {
        key,
        id: id.to_owned(),
        reverse,
        scope: scope_value.to_owned(),
    })
}

pub fn encode_cursor(cursor: &RegistryCursor) -> String {
    let value = json!({
        "key": cursor.key.to_string(),
        "id": cursor.id,
        "reverse": cursor.reverse,
        "scope": cursor.scope,
    });
    serde_json::to_string(&value)
        .map(|text| b64url(text.as_bytes()))
        .unwrap_or_default()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryPage {
    pub items: Vec<RecordView>,
    pub next_cursor: Option<String>,
    pub previous_cursor: Option<String>,
    pub has_more: bool,
}

impl RegistryPage {
    pub fn to_wire(&self) -> Value {
        json!({
            "items": self.items.iter().map(RecordView::to_wire).collect::<Vec<_>>(),
            "nextCursor": self.next_cursor,
            "previousCursor": self.previous_cursor,
            "hasMore": self.has_more,
        })
    }
}

/// Validate one owner row before it is admitted to a list or page. A row from
/// another Company/branch/department, another section or an invalid field
/// document fails the whole read closed instead of being silently dropped.
pub fn validate_registry_row(
    row: &RecordRow,
    actor: &RecordContextActor,
    module_id: &str,
    section_id: &str,
) -> Result<RecordView, BusinessError> {
    if row.module_id != module_id || row.section_id != section_id {
        return Err(registry_source_invalid());
    }
    validate_record(row, actor, module_id, &row.id).map_err(|_| registry_source_invalid())
}

// -- Bounded, dependency-free base64url (unpadded) for opaque cursors. --------

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn b64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity((bytes.len() * 4).div_ceil(3));
    for chunk in bytes.chunks(3) {
        let first = chunk[0] as u32;
        let second = chunk.get(1).copied().unwrap_or(0) as u32;
        let third = chunk.get(2).copied().unwrap_or(0) as u32;
        let value = (first << 16) | (second << 8) | third;
        out.push(ALPHABET[((value >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((value >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((value >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(value & 63) as usize] as char);
        }
    }
    out
}

fn b64url_value(byte: u8) -> Option<u32> {
    match byte {
        b'A'..=b'Z' => Some((byte - b'A') as u32),
        b'a'..=b'z' => Some((byte - b'a') as u32 + 26),
        b'0'..=b'9' => Some((byte - b'0') as u32 + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

fn b64url_decode(value: &str) -> Option<Vec<u8>> {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for &byte in bytes {
        let value = b64url_value(byte)?;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    if buffer != 0 {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_cursor_round_trips_without_padding() {
        for value in [
            "a",
            "ab",
            "abc",
            "records-registry-page-cursor",
            "{\"key\":\"500\",\"id\":\"rec-1\",\"reverse\":false,\"scope\":\"x\"}",
        ] {
            let encoded = b64url(value.as_bytes());
            assert!(!encoded.contains('='));
            assert_eq!(b64url_decode(&encoded).unwrap(), value.as_bytes());
        }
        assert!(b64url_decode("a=").is_none());
        assert!(b64url_decode("a").is_none());
        assert!(b64url_decode("abcde").is_none());
    }

    #[test]
    fn query_bounds_are_strict() {
        assert!(RegistryQuery::page("operations", "overview", None, None).is_ok());
        assert!(RegistryQuery::page("Operations", "overview", None, None).is_err());
        assert!(RegistryQuery::page("operations", " overview", None, None).is_err());
        assert!(RegistryQuery::page("operations", "overview", Some(""), None).is_ok());
        assert!(RegistryQuery::page("operations", "overview", Some("needle"), None).is_ok());
        assert!(RegistryQuery::page("operations", "overview", Some("\0"), None).is_err());
        assert!(
            RegistryQuery::page("operations", "overview", Some(&"x".repeat(201)), None).is_err()
        );
        assert!(RegistryQuery::page("operations", "overview", None, Some("")).is_err());
        assert!(
            RegistryQuery::page("operations", "overview", None, Some(&"x".repeat(4_097))).is_err()
        );
    }
}
