//! Pure domain facts for the native CRM lead case-event history read.
//!
//! This is the owner slice behind `GET /api/v1/holdings/{holdingSlug}/crm?id=`
//! (`crmDetail.history`). It is a read only: it never writes, replays or
//! amends an immutable case event. Composition retains the Account session,
//! selected Company/branch/department scope, `sales.crm` purchase admission and
//! the serializable owner unit; this module enforces the actor read policy,
//! `commerce.read`, the CRM module, work-context currency and every row shape
//! before presentation.
//!
//! The serialized row is byte-for-byte the retained TypeScript
//! `crmCaseEventReadSql('*', 'lead_id=?', ' ORDER BY created_at DESC,id DESC')`
//! projection: `id`, `lead_id`, `opportunity_id`, `version`, `action`,
//! `actor_id`, `reason`, `snapshot_json`, `created_at`.

use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use serde::Serialize;
use std::collections::BTreeSet;

const ACTION_LIMIT: usize = 120;
const REASON_LIMIT: usize = 20_000;

/// Current selected work context of the reader. Composition supplies it; the
/// domain never mints a user, Company, permission, module or expiry.
#[derive(Clone, Debug)]
pub struct CrmHistoryActor {
    pub user_id: String,
    pub company_id: CompanyId,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub expires_at: i64,
}

impl CrmHistoryActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn has_module(&self, module: &str) -> bool {
        self.modules.is_empty() || self.modules.iter().any(|value| value == module)
    }

    /// A history read requires `commerce.read`, the CRM module and a work
    /// context that has not expired. Fails closed on any missing fact.
    pub fn require_history(&self, now: TimestampMillis) -> Result<(), BusinessError> {
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
        "CRM_HISTORY_REQUEST_INVALID",
        "Use one bounded CRM case-event history request.",
    )
}

pub(crate) fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CRM_HISTORY_SOURCE_INVALID",
        "The current CRM case-event history source is invalid.",
    )
}

fn valid_identifier(value: &str) -> bool {
    RecordId::new(value).is_ok()
}

fn valid_text(value: &str, limit: usize) -> bool {
    !value.is_empty() && value.len() <= limit && !value.chars().any(char::is_control)
}

/// One immutable `crm_case_events` row. Field names and JSON types match the
/// retired TypeScript `SELECT *` row exactly; no field is added, dropped or
/// renamed. `snapshot_json` stays a JSON value so a JSONB object is presented
/// as an object, as the TypeScript driver did.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CrmCaseEventRow {
    pub id: String,
    pub lead_id: String,
    pub opportunity_id: Option<String>,
    pub version: i64,
    pub action: String,
    pub actor_id: String,
    pub reason: String,
    pub snapshot_json: serde_json::Value,
    pub created_at: i64,
}

impl CrmCaseEventRow {
    /// Reject a malformed or out-of-scope owner row instead of presenting it.
    pub fn validate(&self, lead_id: &str) -> Result<(), BusinessError> {
        if self.lead_id != lead_id
            || !valid_identifier(&self.id)
            || !valid_identifier(&self.actor_id)
            || !valid_text(&self.action, ACTION_LIMIT)
            || self.reason.len() > REASON_LIMIT
            || self.reason.chars().any(char::is_control)
            || self.version < 1
            || self.created_at < 0
            || self
                .opportunity_id
                .as_deref()
                .is_some_and(|value| !valid_identifier(value))
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

/// Validate the requested lead and the complete owner row source, preserving
/// the exact TypeScript `created_at DESC, id DESC` presentation order.
pub fn validate_history(
    lead_id: &str,
    source: Vec<CrmCaseEventRow>,
) -> Result<Vec<CrmCaseEventRow>, BusinessError> {
    if !valid_identifier(lead_id) {
        return Err(request_invalid());
    }
    let mut seen = BTreeSet::new();
    let mut previous: Option<(i64, &str)> = None;
    for row in &source {
        row.validate(lead_id)?;
        if !seen.insert(row.id.as_str()) {
            return Err(source_invalid());
        }
        let current = (row.created_at, row.id.as_str());
        if previous.is_some_and(|previous| current >= previous) {
            return Err(source_invalid());
        }
        previous = Some(current);
    }
    Ok(source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_shared::public::CompanyId;

    fn actor(now: i64) -> CrmHistoryActor {
        CrmHistoryActor {
            user_id: "user-1".to_owned(),
            company_id: CompanyId::new("company").expect("company"),
            permissions: vec!["commerce.read".to_owned()],
            modules: vec!["crm".to_owned()],
            expires_at: now + 1,
        }
    }

    fn row(id: &str, created_at: i64) -> CrmCaseEventRow {
        CrmCaseEventRow {
            id: id.to_owned(),
            lead_id: "lead-1".to_owned(),
            opportunity_id: None,
            version: 1,
            action: "create".to_owned(),
            actor_id: "user-1".to_owned(),
            reason: String::new(),
            snapshot_json: serde_json::json!({"title":"Lead"}),
            created_at,
        }
    }

    #[test]
    fn history_read_requires_read_permission_module_and_live_context() {
        let now = TimestampMillis::new(100);
        assert!(actor(100).require_history(now).is_ok());

        let mut denied = actor(100);
        denied.permissions = vec!["commerce.sales".to_owned()];
        assert_eq!(
            denied.require_history(now).unwrap_err().code,
            "CRM_PERMISSION_REQUIRED"
        );

        let mut module = actor(100);
        module.modules = vec!["marketing".to_owned()];
        assert_eq!(
            module.require_history(now).unwrap_err().code,
            "CRM_MODULE_FORBIDDEN"
        );

        assert_eq!(
            actor(100)
                .require_history(TimestampMillis::new(101))
                .unwrap_err()
                .code,
            "WORK_CONTEXT_CHANGED"
        );
    }

    #[test]
    fn validation_preserves_the_retired_descending_payload() {
        let rows = validate_history("lead-1", vec![row("event-2", 20), row("event-1", 10)])
            .expect("valid descending source");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "event-2");
        assert_eq!(rows[1].id, "event-1");
    }

    #[test]
    fn validation_rejects_out_of_scope_malformed_and_repeated_rows() {
        assert_eq!(
            validate_history("", vec![row("event-1", 10)])
                .unwrap_err()
                .code,
            "CRM_HISTORY_REQUEST_INVALID"
        );
        let mut wrong = row("event-1", 10);
        wrong.lead_id = "other".to_owned();
        assert_eq!(
            validate_history("lead-1", vec![wrong]).unwrap_err().code,
            "CRM_HISTORY_SOURCE_INVALID"
        );
        assert_eq!(
            validate_history("lead-1", vec![row("event-1", 10), row("event-1", 9)])
                .unwrap_err()
                .code,
            "CRM_HISTORY_SOURCE_INVALID"
        );
        assert_eq!(
            validate_history("lead-1", vec![row("event-1", 10), row("event-2", 10)])
                .unwrap_err()
                .code,
            "CRM_HISTORY_SOURCE_INVALID"
        );
    }

    #[test]
    fn serialized_row_has_exactly_the_typescript_select_star_fields() {
        let value = serde_json::to_value(row("event-1", 10)).expect("serialize");
        let object = value.as_object().expect("object");
        let mut fields: Vec<&str> = object.keys().map(String::as_str).collect();
        fields.sort_unstable();
        assert_eq!(
            fields,
            vec![
                "action",
                "actor_id",
                "created_at",
                "id",
                "lead_id",
                "opportunity_id",
                "reason",
                "snapshot_json",
                "version",
            ]
        );
        assert_eq!(value["lead_id"], "lead-1");
        assert_eq!(value["version"], 1);
        assert_eq!(value["snapshot_json"]["title"], "Lead");
    }
}
