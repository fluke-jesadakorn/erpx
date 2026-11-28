//! Pure domain for the native R3 supplier-bill workspace read.
//!
//! This is an owner read projection only. The Account/Holding session, current
//! Company scope, the commercial purchased-package admission and the serializable
//! caller unit remain composition facts: composition supplies the actor, enforces
//! `supply.purchasing` history admission and opens the Holding unit, and the
//! owner only enforces `finance.read` and work-context currency for this bounded
//! list read. No row is invented and no authority is granted here.
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use serde_json::Value;

/// One projected supplier-bill row. The adapter maps every selected column to
/// the exact JSON object the retained Supply read returned; the domain never
/// reshapes it.
pub type Row = Value;

/// Account-derived facts for the selected work context. Composition builds this
/// from the current assignment; no field is accepted from the request.
#[derive(Clone, Debug)]
pub struct WorkspaceActor {
    pub user_id: String,
    pub company_id: CompanyId,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub permissions: Vec<String>,
    pub expires_at: i64,
}

impl WorkspaceActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    /// The retained `authorizeRead` gate for this route is `finance.read`.
    pub fn require_read(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has("finance.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "A current finance.read assignment is required to list supplier bills.",
            ));
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
}

/// The adapter always returns JSON objects. Reject anything else rather than
/// forwarding a malformed projection to the browser. This is a shape guard, not
/// a row cap: the retained read applies no LIMIT and a cap would change its
/// contract.
pub fn validate_rows(rows: &[Row]) -> Result<(), BusinessError> {
    if rows.iter().all(Value::is_object) {
        Ok(())
    } else {
        Err(BusinessError::new(
            FailureKind::Unavailable,
            "SUPPLIER_BILL_WORKSPACE_UNAVAILABLE",
            "The Procurement supplier-bill workspace is unavailable.",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

    fn actor(permissions: &[&str], expires_at: i64) -> WorkspaceActor {
        WorkspaceActor {
            user_id: "owner".into(),
            company_id: CompanyId::new("company").unwrap(),
            branch_id: None,
            department_id: None,
            permissions: permissions.iter().map(|value| (*value).to_owned()).collect(),
            expires_at,
        }
    }

    #[test]
    fn read_requires_finance_read() {
        assert!(actor(&[], NOW.get() + 60_000).require_read(NOW).is_err());
        assert!(actor(&["finance.read"], NOW.get() + 60_000)
            .require_read(NOW)
            .is_ok());
    }

    #[test]
    fn read_rejects_expired_context() {
        assert!(actor(&["finance.read"], NOW.get()).require_read(NOW).is_err());
    }

    #[test]
    fn rows_must_be_objects() {
        assert!(validate_rows(&[json!({"id": "bill-1"})]).is_ok());
        assert!(validate_rows(&[json!("bill-1")]).is_err());
    }
}
