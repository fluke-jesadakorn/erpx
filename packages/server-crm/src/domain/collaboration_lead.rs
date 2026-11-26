//! Pure domain facts for the native CRM collaboration-lead read.
//!
//! This is the owner slice behind the retired TypeScript plan/execute read
//! `crm_rd_collab_notification_lead` (`SELECT lead_id FROM crm_lead_activities
//! WHERE id=? AND company_id=?`). The old path planned that statement in Rust
//! and prepared/executed it in TypeScript on the caller's open unit; this slice
//! replaces it with a real sqlx execution in the CRM owner and the exact
//! `{lead_id}` (or `null`) payload of `nativeCrmReadFirst`.
//!
//! The read is ledger-free: composition supplies the caller's retained
//! serializable unit so the row observes uncommitted effects, and this module
//! enforces the actor read policy and the row shape before presentation. It
//! never writes an activity or emits a receipt/audit/outbox.
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use serde::Serialize;

/// Current selected work context of the reader. Composition supplies it; the
/// domain never mints a user, Company, permission, module or expiry.
#[derive(Clone, Debug)]
pub struct CollaborationLeadActor {
    pub user_id: String,
    pub company_id: CompanyId,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub expires_at: i64,
}

impl CollaborationLeadActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn has_module(&self, module: &str) -> bool {
        self.modules.is_empty() || self.modules.iter().any(|value| value == module)
    }

    /// The collaboration witness requires `commerce.read`, the CRM module and a
    /// work context that has not expired. Fails closed on any missing fact.
    pub fn require_read(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has("commerce.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "CRM_PERMISSION_REQUIRED",
                "Your selected role does not grant CRM read access.",
            ));
        }
        if !self.has_module("crm") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "CRM_MODULE_FORBIDDEN",
                "Your current role cannot access the CRM pipeline.",
            ));
        }
        if now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        Ok(())
    }
}

pub(crate) fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "CRM_COLLAB_LEAD_REQUEST_INVALID",
        "Use one bounded CRM collaboration-lead read.",
    )
}

pub(crate) fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CRM_COLLAB_LEAD_SOURCE_INVALID",
        "The current CRM collaboration-lead source is invalid.",
    )
}

/// One `crm_lead_activities` row. The only field is `lead_id`, matching the
/// retired `nativeCrmReadFirst<{lead_id:string}>` payload exactly.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CollaborationLeadRow {
    pub lead_id: String,
}

impl CollaborationLeadRow {
    fn validate(&self) -> Result<(), BusinessError> {
        if RecordId::new(self.lead_id.as_str()).is_err() {
            return Err(source_invalid());
        }
        Ok(())
    }
}

/// Validate the requested activity identity and the complete owner row source.
/// A missing row stays `None` (`nativeCrmReadFirst` returned `null`).
pub fn validate_lead(
    record_id: &str,
    source: Option<CollaborationLeadRow>,
) -> Result<Option<CollaborationLeadRow>, BusinessError> {
    if RecordId::new(record_id).is_err() {
        return Err(request_invalid());
    }
    if let Some(row) = source.as_ref() {
        row.validate()?;
    }
    Ok(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor() -> CollaborationLeadActor {
        CollaborationLeadActor {
            user_id: "user-1".to_owned(),
            company_id: CompanyId::new("company").expect("company"),
            permissions: vec!["commerce.read".to_owned()],
            modules: vec!["crm".to_owned()],
            expires_at: 500,
        }
    }

    fn row() -> CollaborationLeadRow {
        CollaborationLeadRow {
            lead_id: "lead-1".to_owned(),
        }
    }

    #[test]
    fn read_requires_read_permission_module_and_live_context() {
        let now = TimestampMillis::new(100);
        assert!(actor().require_read(now).is_ok());

        let mut denied = actor();
        denied.permissions = vec!["commerce.sales".to_owned()];
        assert_eq!(
            denied.require_read(now).unwrap_err().code,
            "CRM_PERMISSION_REQUIRED"
        );

        let mut module = actor();
        module.modules = vec!["marketing".to_owned()];
        assert_eq!(
            module.require_read(now).unwrap_err().code,
            "CRM_MODULE_FORBIDDEN"
        );

        assert_eq!(
            actor().require_read(TimestampMillis::new(501)).unwrap_err().code,
            "WORK_CONTEXT_CHANGED"
        );
    }

    #[test]
    fn validation_preserves_the_retired_row_or_null_payload() {
        assert_eq!(
            validate_lead("activity-1", Some(row())).expect("row"),
            Some(row())
        );
        assert_eq!(validate_lead("activity-1", None).expect("null"), None);
    }

    #[test]
    fn validation_rejects_malformed_request_and_row() {
        assert_eq!(
            validate_lead("", Some(row())).unwrap_err().code,
            "CRM_COLLAB_LEAD_REQUEST_INVALID"
        );
        assert_eq!(
            validate_lead("activity-1", Some(CollaborationLeadRow {
                lead_id: String::new(),
            }))
            .unwrap_err()
            .code,
            "CRM_COLLAB_LEAD_SOURCE_INVALID"
        );
    }

    #[test]
    fn serialized_row_has_exactly_the_typescript_lead_id_field() {
        let value = serde_json::to_value(row()).expect("serialize");
        let object = value.as_object().expect("object");
        let mut fields: Vec<&str> = object.keys().map(String::as_str).collect();
        fields.sort_unstable();
        assert_eq!(fields, vec!["lead_id"]);
        assert_eq!(value["lead_id"], "lead-1");
    }
}
