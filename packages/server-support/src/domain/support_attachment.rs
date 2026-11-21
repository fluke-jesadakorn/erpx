//! Support-owned attachment staging policy for the native Files intake.
//!
//! Files owns the set/link/source ledger and the private byte store. Support
//! owns the ticket status, version, responsibility and evidence audience rules.
//! These functions are transport- and persistence-independent; the PostgreSQL
//! adapter supplies the current ticket row and eligibility in the caller's unit.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

pub const SUPPORT_EVIDENCE_CAPABILITY: &str = "support.evidence";
pub const SUPPORT_ATTACHMENT_PURPOSES: [&str; 3] =
    ["support_evidence", "internal_evidence", "customer_shareable"];
pub const SUPPORT_ATTACHMENT_COMPONENT_KIND: &str = "action";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupportAttachmentActor {
    pub user_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub can_read: bool,
    pub can_work: bool,
    pub can_manage: bool,
    pub same_person_ids: Vec<String>,
}

impl SupportAttachmentActor {
    pub fn same_person(&self, user_id: &str) -> bool {
        user_id == self.user_id || self.same_person_ids.iter().any(|id| id == user_id)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupportAttachmentTicket {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub status: String,
    pub source_kind: String,
    pub owner_id: Option<String>,
    pub version: i64,
    /// The stored ticket owner is still the current eligible assignee.
    pub owner_eligible: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupportAttachmentStage {
    Read,
    Write,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupportAttachmentSetRequest {
    pub target_type: String,
    pub target_id: String,
    pub component_kind: String,
    #[serde(default)]
    pub component_ref: Option<String>,
    pub purpose_key: String,
    pub expected_target_version: i64,
}

pub fn valid_purpose(purpose: &str) -> bool {
    SUPPORT_ATTACHMENT_PURPOSES.contains(&purpose)
}

pub fn component_ref(purpose: &str) -> String {
    format!("support-evidence:{purpose}")
}

/// The exact domain action/component pair the native Support evidence command
/// accepts when it later binds the reviewed set.
pub fn valid_binding_context(purpose: &str, component_kind: &str, reference: Option<&str>) -> bool {
    let expected = component_ref(purpose);
    valid_purpose(purpose)
        && component_kind == SUPPORT_ATTACHMENT_COMPONENT_KIND
        && reference == Some(expected.as_str())
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}
fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}
fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

/// Current ticket responsibility. `expected_version` is the retained Files set
/// version and must match the ticket for every write.
pub fn authorize(
    actor: &SupportAttachmentActor,
    ticket: &SupportAttachmentTicket,
    purpose: &str,
    expected_version: Option<i64>,
    stage: SupportAttachmentStage,
) -> Result<(), BusinessError> {
    if !valid_purpose(purpose) {
        return Err(invalid(
            "ATTACHMENT_POLICY_UNKNOWN",
            "This file purpose is not registered for the selected record and context.",
        ));
    }
    if ticket.company_id != actor.company_id {
        return Err(forbidden(
            "ATTACHMENT_SCOPE_FORBIDDEN",
            "The file belongs to another Company.",
        ));
    }
    if !actor.can_read {
        return Err(forbidden(
            "ATTACHMENT_FORBIDDEN",
            "Your current assignment cannot read this case.",
        ));
    }
    if let (Some(current), Some(ticket)) = (&actor.branch_id, &ticket.branch_id)
        && current != ticket
    {
        return Err(forbidden(
            "ATTACHMENT_SCOPE_FORBIDDEN",
            "The case is outside your Branch scope.",
        ));
    }
    if let (Some(current), Some(ticket)) = (&actor.department_id, &ticket.department_id)
        && current != ticket
    {
        return Err(forbidden(
            "ATTACHMENT_SCOPE_FORBIDDEN",
            "The case is outside your Department scope.",
        ));
    }
    if !matches!(
        ticket.status.as_str(),
        "open" | "in_progress" | "waiting_customer"
    ) {
        return Err(conflict(
            "SUPPORT_EVIDENCE_CLOSED",
            "This case no longer accepts evidence changes.",
        ));
    }
    if stage == SupportAttachmentStage::Read {
        return Ok(());
    }
    if ticket.source_kind == "field_service" {
        return Err(conflict(
            "SOURCE_WORKSPACE_REQUIRED",
            "Continue this case in Field Service.",
        ));
    }
    let assigned = actor.can_work
        && ticket
            .owner_id
            .as_deref()
            .is_some_and(|owner| actor.same_person(owner) && ticket.owner_eligible);
    if purpose == "customer_shareable" {
        if !actor.can_manage {
            return Err(forbidden(
                "ATTACHMENT_CUSTOMER_SHARE_REVIEW",
                "Only a Support manager may stage customer-shareable evidence.",
            ));
        }
    } else if !actor.can_manage && !assigned {
        return Err(forbidden(
            "ATTACHMENT_FORBIDDEN",
            "Current Support responsibility is required.",
        ));
    }
    if let Some(expected) = expected_version
        && expected != ticket.version
    {
        return Err(conflict(
            "SUPPORT_EVIDENCE_CHANGED",
            "The case changed. Review its current version before adding files.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor() -> SupportAttachmentActor {
        SupportAttachmentActor {
            user_id: "user-1".into(),
            company_id: "company-1".into(),
            branch_id: Some("branch-1".into()),
            department_id: None,
            can_read: true,
            can_work: true,
            can_manage: false,
            same_person_ids: vec![],
        }
    }
    fn ticket() -> SupportAttachmentTicket {
        SupportAttachmentTicket {
            id: "ticket-1".into(),
            company_id: "company-1".into(),
            branch_id: Some("branch-1".into()),
            department_id: None,
            status: "open".into(),
            source_kind: "manual".into(),
            owner_id: Some("user-1".into()),
            version: 7,
            owner_eligible: true,
        }
    }

    #[test]
    fn assigned_colleague_may_stage_internal_evidence() {
        assert_eq!(
            authorize(&actor(), &ticket(), "internal_evidence", Some(7), SupportAttachmentStage::Write),
            Ok(())
        );
    }

    #[test]
    fn customer_shareable_requires_a_manager() {
        assert_eq!(
            authorize(&actor(), &ticket(), "customer_shareable", Some(7), SupportAttachmentStage::Write),
            Err(forbidden(
                "ATTACHMENT_CUSTOMER_SHARE_REVIEW",
                "Only a Support manager may stage customer-shareable evidence."
            ))
        );
    }

    #[test]
    fn stale_version_and_closed_or_field_cases_fail_closed() {
        assert_eq!(
            authorize(&actor(), &ticket(), "internal_evidence", Some(6), SupportAttachmentStage::Write),
            Err(conflict(
                "SUPPORT_EVIDENCE_CHANGED",
                "The case changed. Review its current version before adding files."
            ))
        );
        let mut closed = ticket();
        closed.status = "resolved".into();
        assert!(authorize(&actor(), &closed, "internal_evidence", Some(7), SupportAttachmentStage::Write).is_err());
        let mut field = ticket();
        field.source_kind = "field_service".into();
        assert!(authorize(&actor(), &field, "internal_evidence", Some(7), SupportAttachmentStage::Write).is_err());
    }

    #[test]
    fn another_owner_is_not_assigned_but_a_manager_is_allowed() {
        let mut other = ticket();
        other.owner_id = Some("user-2".into());
        assert!(authorize(&actor(), &other, "internal_evidence", Some(7), SupportAttachmentStage::Write).is_err());
        let mut manager = actor();
        manager.can_manage = true;
        assert_eq!(
            authorize(&manager, &other, "internal_evidence", Some(7), SupportAttachmentStage::Write),
            Ok(())
        );
    }

    #[test]
    fn binding_context_matches_the_native_evidence_command() {
        assert!(valid_binding_context(
            "customer_shareable",
            "action",
            Some("support-evidence:customer_shareable")
        ));
        assert!(!valid_binding_context(
            "customer_shareable",
            "record",
            Some("support-evidence:customer_shareable")
        ));
        assert!(!valid_binding_context(
            "customer_shareable",
            "action",
            Some("support-evidence:internal_evidence")
        ));
    }
}
