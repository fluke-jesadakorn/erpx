//! Portable Support evidence binding and withdrawal. Files owns the set, link
//! and attachment rows; Support owns the ticket evidence event and version.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

pub const MAX_REASON: usize = 2_000;
pub const ALLOWED_PURPOSES: [&str; 3] = ["support_evidence", "internal_evidence", "customer_shareable"];

#[derive(Clone, Debug)]
pub struct EvidenceActor {
    pub user_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub can_read: bool,
    pub can_work: bool,
    pub can_manage: bool,
    pub same_person_ids: Vec<String>,
}

impl EvidenceActor {
    pub fn same_person(&self, user_id: &str) -> bool {
        user_id == self.user_id || self.same_person_ids.iter().any(|id| id == user_id)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceBind {
    pub expected_version: i64,
    pub attachment_set_ids: Vec<String>,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceRevoke {
    pub expected_version: i64,
    pub attachment_id: String,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceTicket {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub status: String,
    pub source_kind: String,
    pub owner_id: Option<String>,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceLinkFacts {
    pub id: String,
    pub attachment_id: String,
    pub source_state: String,
    pub checksum_sha256: String,
    pub media_type: String,
    pub bytes: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceSetFacts {
    pub id: String,
    pub company_id: String,
    pub target_type: String,
    pub target_id: String,
    pub component_kind: String,
    pub component_ref: Option<String>,
    pub purpose_key: String,
    pub expected_target_version: i64,
    pub state: String,
    pub required_count: i64,
    pub links: Vec<EvidenceLinkFacts>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceBindPlan {
    pub ticket_id: String,
    pub company_id: String,
    pub ticket_version: i64,
    pub set_id: String,
    pub binding_id: String,
    pub manifest_json: String,
    pub links: Vec<String>,
    pub reason: String,
    pub event_at: i64,
    pub actor_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceRevokePlan {
    pub ticket_id: String,
    pub company_id: String,
    pub ticket_version: i64,
    pub attachment_id: String,
    pub reason: String,
    pub event_at: i64,
    pub actor_id: String,
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

fn text(value: &str, code: &'static str, min: usize, max: usize) -> Result<String, BusinessError> {
    let trimmed = value.trim();
    if trimmed.len() < min || trimmed.len() > max {
        return Err(invalid(code, "Provide a bounded text value."));
    }
    Ok(trimmed.to_owned())
}

fn version(value: i64) -> Result<i64, BusinessError> {
    if !(1..=9_007_199_254_740_991).contains(&value) {
        return Err(invalid(
            "SUPPORT_EVIDENCE_VERSION_INVALID",
            "Support evidence requires the current case version.",
        ));
    }
    Ok(value)
}

pub fn evidence_active(status: &str) -> bool {
    matches!(status, "open" | "in_progress" | "waiting_customer")
}

fn authorize(actor: &EvidenceActor, ticket: &EvidenceTicket) -> Result<(), BusinessError> {
    if !actor.can_read {
        return Err(forbidden(
            "SUPPORT_EVIDENCE_READ_REQUIRED",
            "Choose a role that can read Support evidence.",
        ));
    }
    if ticket.source_kind == "field_service" {
        return Err(conflict(
            "SOURCE_WORKSPACE_REQUIRED",
            "Continue this case in Field Service.",
        ));
    }
    if ticket.company_id != actor.company_id {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "SUPPORT_TICKET_NOT_FOUND",
            "Ticket not found in this scope.",
        ));
    }
    let owner = ticket
        .owner_id
        .as_deref()
        .is_some_and(|owner| actor.same_person(owner));
    if !actor.can_manage && !(actor.can_work && owner) {
        return Err(forbidden(
            "SUPPORT_EVIDENCE_OWNER",
            "Only the assigned colleague or a Support manager may change case evidence.",
        ));
    }
    if !evidence_active(&ticket.status) {
        return Err(conflict(
            "SUPPORT_EVIDENCE_LOCKED",
            "Evidence is frozen once a resolution is proposed. Return the case for rework before changing it.",
        ));
    }
    Ok(())
}

pub fn plan_bind(
    actor: &EvidenceActor,
    ticket: &EvidenceTicket,
    set: &EvidenceSetFacts,
    input: &EvidenceBind,
    key: &str,
    now: i64,
) -> Result<EvidenceBindPlan, BusinessError> {
    authorize(actor, ticket)?;
    version(input.expected_version)?;
    if input.attachment_set_ids.len() != 1 {
        return Err(invalid(
            "ATTACHMENT_SETS_REQUIRED",
            "Choose exactly one reviewed file set.",
        ));
    }
    if ticket.version != input.expected_version {
        return Err(conflict(
            "SUPPORT_EVIDENCE_CHANGED",
            "The case changed before evidence was bound.",
        ));
    }
    if set.company_id != actor.company_id
        || set.target_type != "support_ticket"
        || set.target_id != ticket.id
    {
        return Err(invalid(
            "ATTACHMENT_SET_TARGET",
            "A file set belongs to another record.",
        ));
    }
    if set.component_kind != "action"
        || !ALLOWED_PURPOSES.contains(&set.purpose_key.as_str())
        || set.component_ref.as_deref() != Some(&format!("support-evidence:{}", set.purpose_key))
    {
        return Err(invalid(
            "ATTACHMENT_BINDING_CONTEXT",
            "Choose only files prepared for this exact domain action and component.",
        ));
    }
    if set.state != "draft" {
        return Err(conflict(
            "ATTACHMENT_SET_LOCKED",
            "A file set was already bound or closed.",
        ));
    }
    if set.expected_target_version != input.expected_version {
        return Err(conflict(
            "ATTACHMENT_SET_STALE",
            "Create a file set for the current record version.",
        ));
    }
    if set.links.is_empty() {
        return Err(invalid(
            "ATTACHMENT_SET_EMPTY",
            "Bind at least one available file.",
        ));
    }
    if set
        .links
        .iter()
        .any(|link| link.source_state != "available")
    {
        return Err(conflict(
            "ATTACHMENT_SCAN_PENDING",
            "Every active file must pass malware scanning before this action.",
        ));
    }
    if set.links.len() < usize::try_from(set.required_count.max(0)).unwrap_or(0) {
        return Err(invalid(
            "ATTACHMENT_REQUIRED",
            "The reviewed file set does not meet its required count.",
        ));
    }
    let manifest: Vec<serde_json::Value> = set
        .links
        .iter()
        .map(|link| {
            serde_json::json!({
                "linkId": link.id,
                "sourceId": link.attachment_id,
                "purposeKey": set.purpose_key,
                "checksumSha256": link.checksum_sha256,
                "mediaType": link.media_type,
                "bytes": link.bytes,
            })
        })
        .collect();
    Ok(EvidenceBindPlan {
        ticket_id: ticket.id.clone(),
        company_id: ticket.company_id.clone(),
        ticket_version: ticket.version + 1,
        set_id: set.id.clone(),
        binding_id: format!("support-evidence:{key}"),
        manifest_json: serde_json::to_string(&manifest)
            .map_err(|_| invalid("SUPPORT_EVIDENCE_INVALID", "Provide valid evidence."))?,
        links: set.links.iter().map(|link| link.id.clone()).collect(),
        reason: text(
            &input.reason,
            "SUPPORT_EVIDENCE_REASON_INVALID",
            3,
            MAX_REASON,
        )?,
        event_at: now,
        actor_id: actor.user_id.clone(),
    })
}

pub fn plan_revoke(
    actor: &EvidenceActor,
    ticket: &EvidenceTicket,
    input: &EvidenceRevoke,
    now: i64,
) -> Result<EvidenceRevokePlan, BusinessError> {
    authorize(actor, ticket)?;
    version(input.expected_version)?;
    text(
        &input.attachment_id,
        "SUPPORT_EVIDENCE_ATTACHMENT_INVALID",
        1,
        200,
    )?;
    if ticket.version != input.expected_version {
        return Err(conflict(
            "SUPPORT_EVIDENCE_CHANGED",
            "The case changed before evidence was withdrawn.",
        ));
    }
    Ok(EvidenceRevokePlan {
        ticket_id: ticket.id.clone(),
        company_id: ticket.company_id.clone(),
        ticket_version: ticket.version + 1,
        attachment_id: input.attachment_id.clone(),
        reason: text(
            &input.reason,
            "SUPPORT_EVIDENCE_REASON_INVALID",
            3,
            MAX_REASON,
        )?,
        event_at: now,
        actor_id: actor.user_id.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(user: &str, manage: bool) -> EvidenceActor {
        EvidenceActor {
            user_id: user.into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            can_read: true,
            can_work: true,
            can_manage: manage,
            same_person_ids: vec![],
        }
    }
    fn ticket(status: &str) -> EvidenceTicket {
        EvidenceTicket {
            id: "ticket-1".into(),
            company_id: "company".into(),
            branch_id: Some("branch".into()),
            department_id: None,
            status: status.into(),
            source_kind: "sales_order".into(),
            owner_id: Some("agent".into()),
            version: 1,
        }
    }
    fn set(state: &str) -> EvidenceSetFacts {
        EvidenceSetFacts {
            id: "set-1".into(),
            company_id: "company".into(),
            target_type: "support_ticket".into(),
            target_id: "ticket-1".into(),
            component_kind: "action".into(),
            component_ref: Some("support-evidence:internal_evidence".into()),
            purpose_key: "internal_evidence".into(),
            expected_target_version: 1,
            state: state.into(),
            required_count: 1,
            links: vec![EvidenceLinkFacts {
                id: "link-1".into(),
                attachment_id: "attachment-1".into(),
                source_state: "available".into(),
                checksum_sha256: "a".repeat(64),
                media_type: "application/pdf".into(),
                bytes: 128,
            }],
        }
    }
    fn bind() -> EvidenceBind {
        EvidenceBind {
            expected_version: 1,
            attachment_set_ids: vec!["set-1".into()],
            reason: "Internal case evidence".into(),
        }
    }

    #[test]
    fn binds_a_reviewed_action_set_for_the_owner() {
        let plan = plan_bind(&actor("agent", false), &ticket("open"), &set("draft"), &bind(), "key", 10)
            .unwrap();
        assert_eq!(plan.ticket_version, 2);
        assert_eq!(plan.binding_id, "support-evidence:key");
        assert!(plan.manifest_json.contains("\"purposeKey\":\"internal_evidence\""));
        assert_eq!(
            plan_bind(&actor("other", false), &ticket("open"), &set("draft"), &bind(), "key", 10)
                .unwrap_err()
                .code,
            "SUPPORT_EVIDENCE_OWNER"
        );
    }

    #[test]
    fn rejects_stale_wrong_context_and_unavailable_evidence() {
        let owner = actor("agent", false);
        let mut stale = ticket("open");
        stale.version = 2;
        assert_eq!(
            plan_bind(&owner, &stale, &set("draft"), &bind(), "key", 10)
                .unwrap_err()
                .code,
            "SUPPORT_EVIDENCE_CHANGED"
        );
        let mut wrong = set("draft");
        wrong.purpose_key = "other".into();
        assert_eq!(
            plan_bind(&owner, &ticket("open"), &wrong, &bind(), "key", 10)
                .unwrap_err()
                .code,
            "ATTACHMENT_BINDING_CONTEXT"
        );
        let mut pending = set("draft");
        pending.links[0].source_state = "quarantined".into();
        assert_eq!(
            plan_bind(&owner, &ticket("open"), &pending, &bind(), "key", 10)
                .unwrap_err()
                .code,
            "ATTACHMENT_SCAN_PENDING"
        );
    }

    #[test]
    fn locks_evidence_after_resolution_and_revokes_once() {
        let owner = actor("agent", false);
        assert_eq!(
            plan_bind(&owner, &ticket("resolved"), &set("draft"), &bind(), "key", 10)
                .unwrap_err()
                .code,
            "SUPPORT_EVIDENCE_LOCKED"
        );
        let plan = plan_revoke(
            &owner,
            &ticket("open"),
            &EvidenceRevoke {
                expected_version: 1,
                attachment_id: "attachment-1".into(),
                reason: "Wrong screenshot replaced".into(),
            },
            20,
        )
        .unwrap();
        assert_eq!(plan.ticket_version, 2);
        assert_eq!(plan.attachment_id, "attachment-1");
    }

    #[test]
    fn a_manager_can_bind_for_another_owner_but_a_field_case_cannot() {
        let plan = plan_bind(&actor("manager", true), &ticket("open"), &set("draft"), &bind(), "key", 10)
            .unwrap();
        assert_eq!(plan.actor_id, "manager");
        let mut field = ticket("open");
        field.source_kind = "field_service".into();
        assert_eq!(
            plan_bind(&actor("manager", true), &field, &set("draft"), &bind(), "key", 10)
                .unwrap_err()
                .code,
            "SOURCE_WORKSPACE_REQUIRED"
        );
    }
}
