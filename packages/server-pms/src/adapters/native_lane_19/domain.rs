//! Pure response types, validation and invariants for the lane 19 PMS evidence read.
//!
//! No SQL and no transport live here. The canonical guest-account terminology
//! conversion is the projection calculation moved out of the retired TypeScript
//! adapter; it is deterministic and fails closed on conflicting legacy keys.

use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceRequest {
    pub company: String,
    pub branch: Option<String>,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryEvidence {
    pub id: String,
    pub journal_id: Option<String>,
    pub data: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuestAccountEvidence {
    pub id: String,
    pub property_id: String,
    pub hotel: String,
    pub business_date: String,
    pub currency: String,
    pub account: Value,
    pub entries: Vec<EntryEvidence>,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PMS_EVIDENCE_REQUEST_INVALID",
        "Use one bounded PMS guest-account evidence request.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_NATIVE_LANE_19_UNAVAILABLE",
        "The native PMS evidence service is unavailable.",
    )
}

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PMS_NOT_FOUND",
        "Guest Account not found in this workspace.",
    )
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' || byte == b'.' || byte == b':')
        && RecordId::new(value).is_ok()
}

pub fn valid_company(value: &str) -> bool {
    valid_id(value)
}

pub fn valid_branch(value: &str) -> bool {
    valid_id(value)
}

/// Company is mandatory, branch is an optional exact workspace scope and the
/// record id identifies exactly one guest account.
pub fn validate_request(request: &EvidenceRequest) -> Result<(), BusinessError> {
    if !valid_company(&request.company)
        || !request.branch.as_deref().is_none_or(|branch| valid_branch(branch) && !branch.is_empty())
        || !valid_id(&request.id)
    {
        return Err(invalid());
    }
    Ok(())
}

const RENAMES: &[(&str, &str)] = &[
    ("folio", "guestAccount"),
    ("folios", "guestAccounts"),
    ("folioId", "guestAccountId"),
    ("toFolioId", "toGuestAccountId"),
    ("folioIds", "guestAccountIds"),
    ("folio_id", "guest_account_id"),
    ("folio_entry_id", "guest_account_entry_id"),
    ("folioEntryId", "guestAccountEntryId"),
];

fn command(value: &str) -> String {
    let value = if let Some(rest) = value.strip_prefix("pms.folio.") {
        format!("pms.guest_account.{rest}")
    } else if let Some(rest) = value.strip_prefix("folio.") {
        format!("guest_account.{rest}")
    } else {
        value.to_owned()
    };
    value.replace("stay.route_folio", "stay.route_guest_account")
}

fn conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_GUEST_ACCOUNT_REFERENCE_CONFLICT",
        "Stored guest-account terminology conflicts.",
    )
}

/// Canonicalize stored legacy terminology recursively. Keys are renamed, the
/// closed operation/kind/section values are rewritten, and a conflicting
/// duplicate after rename is rejected rather than silently overwritten.
pub fn canonical_guest_account(value: Value) -> Result<Value, BusinessError> {
    let mut value = value;
    canonicalize(&mut value)?;
    Ok(value)
}

fn canonicalize(value: &mut Value) -> Result<(), BusinessError> {
    match value {
        Value::Array(items) => {
            for item in items {
                canonicalize(item)?;
            }
            Ok(())
        }
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, mut item) in std::mem::take(map) {
                let next = match key.as_str() {
                    "operation" | "eventType" => match &item {
                        Value::String(operation) => Value::String(command(operation)),
                        _ => {
                            canonicalize(&mut item)?;
                            item
                        }
                    },
                    "kind" | "recordKind" => match &item {
                        Value::String(kind) if kind == "folio" => Value::String("guest_account".to_owned()),
                        _ => {
                            canonicalize(&mut item)?;
                            item
                        }
                    },
                    "section" => match &item {
                        Value::String(section) if section == "folios" => Value::String("guest-accounts".to_owned()),
                        _ => {
                            canonicalize(&mut item)?;
                            item
                        }
                    },
                    _ => {
                        canonicalize(&mut item)?;
                        item
                    }
                };
                let name = RENAMES
                    .iter()
                    .find(|(from, _)| *from == key.as_str())
                    .map(|(_, to)| (*to).to_owned())
                    .unwrap_or(key);
                if let Some(existing) = out.get(&name) {
                    if *existing != next {
                        return Err(conflict());
                    }
                }
                out.insert(name, next);
            }
            *map = out;
            Ok(())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonicalizes_legacy_keys_and_values_recursively() {
        let raw = json!({
            "folioId": "old-1",
            "kind": "folio",
            "section": "folios",
            "operation": "pms.folio.post",
            "nested": [{"toFolioId": "old-2", "eventType": "stay.route_folio"}]
        });
        let value = canonical_guest_account(raw).expect("canonical");
        assert_eq!(
            value,
            json!({
                "guestAccountId": "old-1",
                "kind": "guest_account",
                "section": "guest-accounts",
                "operation": "pms.guest_account.post",
                "nested": [{"toGuestAccountId": "old-2", "eventType": "stay.route_guest_account"}]
            })
        );
    }

    #[test]
    fn rejects_conflicting_legacy_and_current_keys() {
        let raw = json!({"folioId": "legacy", "guestAccountId": "current"});
        assert_eq!(
            canonical_guest_account(raw).unwrap_err().code,
            "PMS_GUEST_ACCOUNT_REFERENCE_CONFLICT"
        );
    }

    #[test]
    fn validates_bounded_requests() {
        assert!(validate_request(&EvidenceRequest {
            company: "company-1".into(),
            branch: None,
            id: "acct-1".into(),
        })
        .is_ok());
        assert!(validate_request(&EvidenceRequest {
            company: "company-1".into(),
            branch: Some(String::new()),
            id: "acct-1".into(),
        })
        .is_err());
        assert!(validate_request(&EvidenceRequest {
            company: "company-1".into(),
            branch: None,
            id: "acct/../1".into(),
        })
        .is_err());
    }
}
