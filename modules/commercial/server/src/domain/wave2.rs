//! Pure domain planner for the native Commercial wave2 grant-save write.
//!
//! This module owns the retained `company_package_grants` / grant-event write
//! decision. It performs no IO, opens no transaction and emits no SQL: the
//! application layer turns its typed plan into a repository descriptor and the
//! owner adapter supplies the only SQL. The TypeScript adapter no longer writes
//! these rows itself (see `adapters/native-wave2.ts`).
use erpx_platform_shared_server::public::BusinessError;
use serde_json::Value;

pub const GRANT_SAVE_OPERATION: &str = "commercial.wave2.grant.save";
pub const GRANT_RECORD_TYPE: &str = "company_package_grants";

const MAX_ID: usize = 200;
const MAX_TEXT: usize = 4000;

/// A validated grant-save write. The aggregate key is `company_id + package_id`;
/// every field mirrors the retained camelCase input exactly once.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GrantSaveInput {
    pub company_id: String,
    pub package_id: String,
    pub package_version: i64,
    pub state: String,
    pub starts_at: i64,
    pub ends_at: Option<i64>,
    pub version: i64,
    pub reason: String,
    pub evidence: String,
    pub updated_at: i64,
    pub actor_id: String,
    pub idempotency_key: String,
    pub request_hash: String,
    pub before_json: Option<String>,
    pub after_json: String,
}

/// Pure plan: the write is an idempotent upsert bound to one Company/package.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GrantSavePlan {
    pub expected_version: i64,
    pub next_version: i64,
}

impl GrantSaveInput {
    /// Parse and validate one retained grant-save input. Unknown fields are
    /// ignored here but rejected at the HTTP boundary; this is the pure guard.
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let company_id = text(raw, "companyId", MAX_ID)?;
        let package_id = text(raw, "packageId", MAX_ID)?;
        let state = text(raw, "state", 40)?;
        if !matches!(
            state.as_str(),
            "active" | "suspended" | "revoked" | "expired" | "pending"
        ) {
            return Err(invalid());
        }
        let package_version = integer(raw, "packageVersion")?;
        let starts_at = integer(raw, "startsAt")?;
        let version = integer(raw, "version")?;
        if package_version < 0 || version < 1 {
            return Err(invalid());
        }
        let ends_at = match raw.get("endsAt") {
            None | Some(Value::Null) => None,
            Some(Value::Number(number)) => number.as_i64(),
            _ => return Err(invalid()),
        };
        let reason = text(raw, "reason", MAX_TEXT)?;
        let evidence = text(raw, "evidence", MAX_TEXT)?;
        let updated_at = integer(raw, "updatedAt")?;
        let actor_id = text(raw, "actorId", MAX_ID)?;
        let idempotency_key = text(raw, "idempotencyKey", 128)?;
        if idempotency_key.len() < 16
            || !idempotency_key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err(invalid());
        }
        let request_hash = text(raw, "requestHash", 128)?;
        if request_hash.len() != 64 || !request_hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid());
        }
        let before_json = match raw.get("beforeJson") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) if value.len() <= MAX_TEXT => Some(value.clone()),
            _ => return Err(invalid()),
        };
        let after_json = match raw.get("afterJson") {
            Some(Value::String(value)) if !value.is_empty() && value.len() <= MAX_TEXT => {
                value.clone()
            }
            _ => return Err(invalid()),
        };
        Ok(Self {
            company_id,
            package_id,
            package_version,
            state,
            starts_at,
            ends_at,
            version,
            reason,
            evidence,
            updated_at,
            actor_id,
            idempotency_key,
            request_hash,
            before_json,
            after_json,
        })
    }

    pub fn record_id(&self) -> String {
        format!("{}:{}", self.company_id, self.package_id)
    }

    /// The retained optimistic rule: an upsert must advance the stored version.
    pub fn plan(&self) -> Result<GrantSavePlan, BusinessError> {
        if self.version < 1 {
            return Err(invalid());
        }
        Ok(GrantSavePlan {
            expected_version: self.version - 1,
            next_version: self.version,
        })
    }
}

fn text(raw: &Value, field: &str, max: usize) -> Result<String, BusinessError> {
    match raw.get(field) {
        Some(Value::String(value)) if !value.is_empty() && value.len() <= max => Ok(value.clone()),
        _ => Err(invalid()),
    }
}

fn integer(raw: &Value, field: &str) -> Result<i64, BusinessError> {
    match raw.get(field) {
        Some(Value::Number(number)) => number.as_i64().ok_or_else(invalid),
        _ => Err(invalid()),
    }
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCIAL_GRANT_SAVE_INVALID",
        "Review the Company package grant write.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        erpx_platform_shared_server::public::FailureKind::Unavailable,
        "COMMERCIAL_WAVE2_UNAVAILABLE",
        "The native Commercial service is unavailable.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn raw() -> Value {
        json!({
            "companyId": "company-1",
            "packageId": "supply.receiving",
            "packageVersion": 1,
            "state": "active",
            "startsAt": 1000,
            "endsAt": null,
            "version": 1,
            "reason": "grant",
            "evidence": "reviewed",
            "updatedAt": 1001,
            "actorId": "user-1",
            "idempotencyKey": "grant-save-key-0123456789",
            "requestHash": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "beforeJson": null,
            "afterJson": "{\"state\":\"active\"}"
        })
    }

    #[test]
    fn parses_a_valid_grant_save() {
        let input = GrantSaveInput::parse(&raw()).unwrap();
        assert_eq!(input.company_id, "company-1");
        assert_eq!(input.record_id(), "company-1:supply.receiving");
        assert_eq!(input.plan().unwrap().next_version, 1);
    }

    #[test]
    fn rejects_invalid_state_hash_and_version() {
        let mut value = raw();
        value["state"] = json!("bogus");
        assert!(GrantSaveInput::parse(&value).is_err());
        let mut value = raw();
        value["requestHash"] = json!("short");
        assert!(GrantSaveInput::parse(&value).is_err());
        let mut value = raw();
        value["version"] = json!(0);
        assert!(GrantSaveInput::parse(&value).is_err());
        assert!(
            GrantSaveInput::parse(&value).unwrap_err().kind
                == erpx_platform_shared_server::public::FailureKind::InvalidInput
        );
    }
}
