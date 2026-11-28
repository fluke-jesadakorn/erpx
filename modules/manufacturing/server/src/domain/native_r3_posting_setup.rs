//! Pure domain for the native R3 Manufacturing posting-setup owner slice.
//!
//! The Account session, holding-scope conditions, current Company Finance
//! authority, purchased-package admission and the serializable caller-owned
//! Holding transaction remain composition facts. This module only states the
//! owner permission gates and a shape guard for the read projection.
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use serde_json::Value;

/// One row from the retained owner reads.
pub type Row = Value;

/// Account-derived facts for the selected work context. Composition builds
/// this from the current assignment; nothing is accepted from the request.
#[derive(Clone, Debug)]
pub struct PostingSetupActor {
    pub user_id: String,
    pub company_id: CompanyId,
    pub permissions: Vec<String>,
    pub expires_at: i64,
}

impl PostingSetupActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn require(&self, permissions: &[&str], message: &'static str, now: TimestampMillis) -> Result<(), BusinessError> {
        if !permissions.iter().all(|permission| self.has(permission)) {
            return Err(BusinessError::new(FailureKind::Forbidden, "PERMISSION_REQUIRED", message));
        }
        if self.expires_at <= now.get() {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "WORK_CONTEXT_EXPIRED",
                "The selected work context expired. Refresh the page and retry.",
            ));
        }
        Ok(())
    }

    /// Retained `finance.read` + `finance.write` gate for the setup read.
    pub fn require_read(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.require(
            &["finance.read", "finance.write"],
            "A current finance.read and finance.write assignment is required to read production posting setup.",
            now,
        )
    }

    /// Retained command gate for the setup write.
    pub fn require_write(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.require(
            &["finance.read", "finance.write"],
            "A current finance.read and finance.write assignment is required to configure production posting setup.",
            now,
        )
    }
}

pub fn data_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MANUFACTURING_POSTING_SETUP_UNAVAILABLE",
        "The Manufacturing posting setup is unavailable.",
    )
}

/// Shape guard for the owner-owned configuration/reference facts. The
/// `available`, `configured`, `version`, `canConfigure`, `contextKey` and
/// refusal fields are composition facts and are deliberately not checked here.
pub fn validate_rows(data: &Value) -> Result<(), BusinessError> {
    let object = data.as_object().ok_or_else(data_invalid)?;
    if !object.contains_key("configuration") {
        return Err(data_invalid());
    }
    if !object.get("charts").is_some_and(Value::is_array) || !object.get("accounts").is_some_and(Value::is_array) {
        return Err(data_invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

    fn actor(permissions: &[&str]) -> PostingSetupActor {
        PostingSetupActor {
            user_id: "owner".into(),
            company_id: CompanyId::new("company").unwrap(),
            permissions: permissions.iter().map(|value| (*value).to_owned()).collect(),
            expires_at: NOW.get() + 60_000,
        }
    }

    #[test]
    fn read_requires_finance_read_and_write() {
        assert!(actor(&[]).require_read(NOW).is_err());
        assert!(actor(&["finance.read"]).require_read(NOW).is_err());
        assert!(actor(&["finance.read", "finance.write"]).require_read(NOW).is_ok());
    }

    #[test]
    fn expired_context_is_rejected() {
        let mut value = actor(&["finance.read", "finance.write"]);
        value.expires_at = NOW.get();
        assert!(value.require_write(NOW).is_err());
    }

    #[test]
    fn projection_must_have_expected_shape() {
        let rows = json!({"configuration": null, "charts": [], "accounts": []});
        assert!(validate_rows(&rows).is_ok());
        assert!(validate_rows(&json!({"charts": []})).is_err());
        assert!(validate_rows(&json!({"configuration": null, "charts": {}, "accounts": []})).is_err());
    }
}
