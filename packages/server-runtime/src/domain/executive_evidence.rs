//! Pure domain for the native Executive evidence-access audit command side.
//!
//! This module owns the exact retained `executive_evidence_access_events`
//! validation and the deterministic audit descriptor. It performs no IO, opens
//! no transaction and emits no SQL: the application layer turns its typed plan
//! into repository descriptors and the owner adapter supplies the SQL.
use erpx_server_shared::public::BusinessError;
use serde_json::Value;

pub const EVIDENCE_ACCESS_OPERATION: &str = "executive.evidence.access";
pub const EVIDENCE_ACCESS_RECORD_TYPE: &str = "executive_evidence_access_events";
const MAX_PROVIDER: usize = 120;
const MAX_METRIC: usize = 120;
const MAX_ACTION: usize = 24;
const MAX_FIELD_IDS: usize = 32 * 1024;
const MAX_QUERY_HASH: usize = 128;
const MAX_RECORDS: usize = 1_000;
const MAX_RECORD_ID: usize = 320;

/// One audited evidence record. Kept byte-compatible with the retained D1
/// `jsonb_to_recordset` shape (`record_id`, `record_version`).
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct EvidenceRecord {
    pub record_id: String,
    pub record_version: i64,
}

/// A validated write input. The retained TS writers send exactly these fields;
/// `action` is restricted to `search`/`open` by the PostgreSQL CHECK.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceAccessInput {
    pub provider: String,
    pub metric: String,
    pub action: String,
    pub field_ids: String,
    pub query_hash: String,
    pub records: Vec<EvidenceRecord>,
}

impl EvidenceAccessInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let provider = text(raw, "provider", MAX_PROVIDER)?;
        let metric = text(raw, "metric", MAX_METRIC)?;
        let action = text(raw, "action", MAX_ACTION)?;
        if action != "search" && action != "open" {
            return Err(invalid());
        }
        let field_ids = text(raw, "fieldIds", MAX_FIELD_IDS)?;
        let query_hash = text(raw, "queryHash", MAX_QUERY_HASH)?;
        if query_hash.is_empty()
            || query_hash.len() > MAX_QUERY_HASH
            || !query_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(invalid());
        }
        if serde_json::from_str::<Value>(&field_ids).is_err() {
            return Err(invalid());
        }
        let records = match raw.get("records") {
            Some(Value::Array(items)) => {
                if items.len() > MAX_RECORDS {
                    return Err(invalid());
                }
                items
                    .iter()
                    .map(|item| {
                        let record_id = text(item, "recordId", MAX_RECORD_ID)?;
                        let record_version = match item.get("recordVersion") {
                            Some(Value::Number(number)) => number.as_i64(),
                            _ => None,
                        }
                        .filter(|value| *value >= 0)
                        .ok_or_else(invalid)?;
                        Ok(EvidenceRecord {
                            record_id,
                            record_version,
                        })
                    })
                    .collect::<Result<Vec<_>, BusinessError>>()?
            }
            _ => return Err(invalid()),
        };
        // The retained table CHECK requires a concrete record for `search`.
        if action == "search" && records.is_empty() {
            return Err(invalid());
        }
        Ok(Self {
            provider,
            metric,
            action,
            field_ids,
            query_hash,
            records,
        })
    }
}

fn text(raw: &Value, field: &str, max: usize) -> Result<String, BusinessError> {
    match raw.get(field) {
        Some(Value::String(value)) if !value.is_empty() && value.len() <= max => Ok(value.clone()),
        _ => Err(invalid()),
    }
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "EXECUTIVE_EVIDENCE_REQUEST_INVALID",
        "Use one bounded Executive evidence-access command.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "EXECUTIVE_EVIDENCE_UNAVAILABLE",
        "The native Executive evidence service is unavailable.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn raw(action: &str, records: Value) -> Value {
        json!({
            "provider": "brand-profile-records",
            "metric": "brand-profiles",
            "action": action,
            "fieldIds": "[\"name\"]",
            "queryHash": "abc123",
            "records": records,
        })
    }

    #[test]
    fn parses_search_and_open() {
        let search = EvidenceAccessInput::parse(&raw(
            "search",
            json!([{"recordId":"rec-1","recordVersion":2}]),
        ))
        .unwrap();
        assert_eq!(search.action, "search");
        assert_eq!(search.records.len(), 1);
        let open = EvidenceAccessInput::parse(&raw("open", json!([]))).unwrap();
        assert!(open.records.is_empty());
    }

    #[test]
    fn rejects_invalid_action_and_missing_search_record() {
        assert!(
            EvidenceAccessInput::parse(&raw("delete", json!([{"recordId":"r","recordVersion":1}])))
                .is_err()
        );
        assert!(EvidenceAccessInput::parse(&raw("search", json!([]))).is_err());
        assert!(
            EvidenceAccessInput::parse(&raw("open", json!([{"recordId":"r","recordVersion":-1}])))
                .is_err()
        );
    }

    #[test]
    fn rejects_bad_query_hash_and_field_ids() {
        let mut value = raw("open", json!([]));
        value["queryHash"] = json!("not-hex!");
        assert!(EvidenceAccessInput::parse(&value).is_err());
        let mut value = raw("open", json!([]));
        value["fieldIds"] = json!("not json");
        assert!(EvidenceAccessInput::parse(&value).is_err());
    }
}
