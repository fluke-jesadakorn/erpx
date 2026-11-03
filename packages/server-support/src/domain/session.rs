//! Portable temporary Support access sessions and the capabilities issued from
//! them. No database, HTTP, clock or notification effect is reachable here.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

pub const MAX_REASON: usize = 2_000;
/// A temporary access session can never outlive the next eight hours.
pub const SESSION_WINDOW_MS: i64 = 8 * 60 * 60 * 1_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Pending,
    Approved,
    Rejected,
    Revoked,
}

impl SessionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Revoked => "revoked",
        }
    }
    pub fn active(self) -> bool {
        matches!(self, Self::Pending | Self::Approved)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityCode {
    CustomerOrderContext,
    TicketEvidenceMetadata,
}

impl CapabilityCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CustomerOrderContext => "customer_order_context",
            Self::TicketEvidenceMetadata => "ticket_evidence_metadata",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionPermissions {
    pub can_read: bool,
    pub can_work: bool,
    pub can_manage: bool,
}

#[derive(Clone, Debug)]
pub struct SessionActor {
    pub user_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub permissions: SessionPermissions,
    /// Resolved same-person identities (aliases/duplicates), never a caller label.
    pub same_person_ids: Vec<String>,
}

impl SessionActor {
    pub fn same_person(&self, user_id: &str) -> bool {
        user_id == self.user_id || self.same_person_ids.iter().any(|id| id == user_id)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionRequest {
    pub ticket_id: String,
    pub ticket_version: i64,
    pub requested_for: String,
    pub expires_at: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case", deny_unknown_fields)]
pub enum SessionAction {
    Approve {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
    },
    Reject {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
    },
    Revoke {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
    },
    IssueCapability {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        code: CapabilityCode,
        reason: String,
    },
    RevokeCapability {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        #[serde(rename = "capabilityId")]
        capability_id: String,
        #[serde(rename = "expectedCapabilityVersion")]
        expected_capability_version: i64,
        reason: String,
    },
}

impl SessionAction {
    pub fn expected_version(&self) -> i64 {
        match self {
            Self::Approve { expected_version, .. }
            | Self::Reject { expected_version, .. }
            | Self::Revoke { expected_version, .. }
            | Self::IssueCapability { expected_version, .. }
            | Self::RevokeCapability { expected_version, .. } => *expected_version,
        }
    }
    pub fn action(&self) -> &'static str {
        match self {
            Self::Approve { .. } => "approve",
            Self::Reject { .. } => "reject",
            Self::Revoke { .. } => "revoke",
            Self::IssueCapability { .. } => "issue-capability",
            Self::RevokeCapability { .. } => "revoke-capability",
        }
    }
    pub fn reason(&self) -> &str {
        match self {
            Self::Approve { reason, .. }
            | Self::Reject { reason, .. }
            | Self::Revoke { reason, .. }
            | Self::IssueCapability { reason, .. }
            | Self::RevokeCapability { reason, .. } => reason,
        }
    }
    pub fn session_event(&self) -> Option<&'static str> {
        match self {
            Self::Approve { .. } => Some("approve"),
            Self::Reject { .. } => Some("reject"),
            Self::Revoke { .. } => Some("revoke"),
            Self::IssueCapability { .. } | Self::RevokeCapability { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionSnapshot {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub ticket_id: String,
    pub ticket_version: i64,
    pub requested_for: String,
    pub status: SessionStatus,
    pub created_by: String,
    pub reviewed_by: Option<String>,
    pub expires_at: i64,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionRequestPlan {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub ticket_id: String,
    pub ticket_version: i64,
    pub requested_for: String,
    pub reason: String,
    pub expires_at: i64,
    pub status: SessionStatus,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionChangePlan {
    pub id: String,
    pub company_id: String,
    pub action: &'static str,
    pub status: SessionStatus,
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<i64>,
    pub revoked_by: Option<String>,
    pub revoked_at: Option<i64>,
    pub capability_code: Option<CapabilityCode>,
    pub capability_id: Option<String>,
    pub expected_capability_version: Option<i64>,
    pub reason: String,
    pub version: i64,
    pub event_at: i64,
    pub expires_at: i64,
}

impl SessionChangePlan {
    pub fn session_event(&self) -> Option<&'static str> {
        match self.action {
            "approve" => Some("approve"),
            "reject" => Some("reject"),
            "revoke" => Some("revoke"),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionPlan {
    Request(SessionRequestPlan),
    Change(SessionChangePlan),
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

fn text(value: &str, field: &'static str, min: usize, max: usize) -> Result<String, BusinessError> {
    let trimmed = value.trim();
    if trimmed.len() < min || trimmed.len() > max {
        return Err(invalid(field, "Provide a bounded text value."));
    }
    Ok(trimmed.to_owned())
}

fn version(value: i64) -> Result<i64, BusinessError> {
    if !(1..=9_007_199_254_740_991).contains(&value) {
        return Err(invalid(
            "SUPPORT_SESSION_VERSION_INVALID",
            "Provide a positive version.",
        ));
    }
    Ok(value)
}

pub fn plan_request(
    actor: &SessionActor,
    key: &str,
    input: &SessionRequest,
    now: i64,
) -> Result<SessionPlan, BusinessError> {
    if !actor.permissions.can_work || !actor.permissions.can_read {
        return Err(forbidden(
            "SUPPORT_SESSION_WORK_REQUIRED",
            "Choose a role that can work Support sessions.",
        ));
    }
    if input.requested_for != actor.user_id {
        return Err(forbidden(
            "SUPPORT_SESSION_SELF_REQUEST_REQUIRED",
            "A support colleague must request their own temporary session.",
        ));
    }
    text(&input.ticket_id, "SUPPORT_SESSION_TICKET_INVALID", 1, 200)?;
    version(input.ticket_version)?;
    text(&input.requested_for, "SUPPORT_SESSION_TARGET_INVALID", 1, 200)?;
    let reason = text(
        &input.reason,
        "SUPPORT_SESSION_REASON_INVALID",
        3,
        MAX_REASON,
    )?;
    if input.expires_at <= now || input.expires_at > now + SESSION_WINDOW_MS {
        return Err(invalid(
            "SUPPORT_SESSION_EXPIRY_INVALID",
            "Session expiry must be within the next eight hours.",
        ));
    }
    Ok(SessionPlan::Request(SessionRequestPlan {
        id: format!("support-session-{key}"),
        company_id: actor.company_id.clone(),
        branch_id: actor.branch_id.clone().unwrap_or_default(),
        department_id: actor.department_id.clone(),
        ticket_id: input.ticket_id.clone(),
        ticket_version: input.ticket_version,
        requested_for: input.requested_for.clone(),
        reason,
        expires_at: input.expires_at,
        status: SessionStatus::Pending,
        created_by: actor.user_id.clone(),
        created_at: now,
        updated_at: now,
    }))
}

pub fn plan_change(
    actor: &SessionActor,
    snapshot: &SessionSnapshot,
    input: &SessionAction,
    now: i64,
) -> Result<SessionPlan, BusinessError> {
    if !actor.permissions.can_read {
        return Err(forbidden(
            "SUPPORT_SESSION_READ_REQUIRED",
            "Choose a role that can read Support sessions.",
        ));
    }
    if snapshot.company_id != actor.company_id {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "SUPPORT_SESSION_NOT_FOUND",
            "Support session not found in this scope.",
        ));
    }
    if input.expected_version() != snapshot.version {
        return Err(conflict(
            "SUPPORT_SESSION_STATE_CHANGED",
            "Review the current support session state.",
        ));
    }
    if !actor.permissions.can_manage {
        return Err(forbidden(
            "SUPPORT_SESSION_MANAGE_REQUIRED",
            "Support management permission is required.",
        ));
    }
    let reason = text(
        input.reason(),
        "SUPPORT_SESSION_REASON_INVALID",
        3,
        MAX_REASON,
    )?;
    let expired = snapshot.expires_at <= now;
    let (status, reviewed_by, reviewed_at, revoked_by, revoked_at) = match input {
        SessionAction::Approve { .. } | SessionAction::Reject { .. } => {
            require_status(snapshot, SessionStatus::Pending)?;
            if expired {
                return Err(conflict(
                    "SUPPORT_SESSION_EXPIRED",
                    "This temporary session has expired.",
                ));
            }
            if actor.same_person(&snapshot.created_by)
                || actor.same_person(&snapshot.requested_for)
            {
                return Err(forbidden(
                    "INDEPENDENT_SESSION_REVIEWER_REQUIRED",
                    "Another manager must review this temporary session.",
                ));
            }
            let status = if matches!(input, SessionAction::Approve { .. }) {
                SessionStatus::Approved
            } else {
                SessionStatus::Rejected
            };
            (
                status,
                Some(actor.user_id.clone()),
                Some(now),
                None,
                None,
            )
        }
        SessionAction::Revoke { .. } => {
            require_status(snapshot, SessionStatus::Approved)?;
            if expired {
                return Err(conflict(
                    "SUPPORT_SESSION_EXPIRED",
                    "This temporary session has expired.",
                ));
            }
            (
                SessionStatus::Revoked,
                snapshot.reviewed_by.clone(),
                snapshot.reviewed_by.as_ref().map(|_| now),
                Some(actor.user_id.clone()),
                Some(now),
            )
        }
        SessionAction::IssueCapability { .. } => {
            require_status(snapshot, SessionStatus::Approved)?;
            if expired {
                return Err(conflict(
                    "SUPPORT_SESSION_EXPIRED",
                    "This temporary session has expired.",
                ));
            }
            (
                SessionStatus::Approved,
                snapshot.reviewed_by.clone(),
                None,
                None,
                None,
            )
        }
        SessionAction::RevokeCapability {
            capability_id,
            expected_capability_version,
            ..
        } => {
            text(
                capability_id,
                "SUPPORT_SESSION_CAPABILITY_INVALID",
                1,
                200,
            )?;
            version(*expected_capability_version)?;
            (
                SessionStatus::Approved,
                snapshot.reviewed_by.clone(),
                None,
                None,
                None,
            )
        }
    };
    let (capability_code, capability_id, expected_capability_version) = match input {
        SessionAction::IssueCapability { code, .. } => (Some(*code), None, None),
        SessionAction::RevokeCapability {
            capability_id,
            expected_capability_version,
            ..
        } => (
            None,
            Some(capability_id.clone()),
            Some(*expected_capability_version),
        ),
        _ => (None, None, None),
    };
    Ok(SessionPlan::Change(SessionChangePlan {
        id: snapshot.id.clone(),
        company_id: snapshot.company_id.clone(),
        action: input.action(),
        status,
        reviewed_by,
        reviewed_at,
        revoked_by,
        revoked_at,
        capability_code,
        capability_id,
        expected_capability_version,
        reason,
        version: snapshot.version + 1,
        event_at: now,
        expires_at: snapshot.expires_at,
    }))
}

fn require_status(
    snapshot: &SessionSnapshot,
    expected: SessionStatus,
) -> Result<(), BusinessError> {
    if snapshot.status != expected {
        return Err(conflict(
            "SUPPORT_SESSION_STATE_CHANGED",
            "Review the current support session state.",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionProjection {
    pub id: String,
    pub ticket_id: String,
    pub requested_for: String,
    pub status: SessionStatus,
    pub effective_status: &'static str,
    pub expires_at: i64,
    pub version: i64,
    pub can_review: bool,
    pub can_manage: bool,
}

/// The workspace projection derives effective expiry and independent review
/// eligibility; it never trusts a stored boolean.
pub fn project(
    actor: &SessionActor,
    rows: &[SessionSnapshot],
    now: i64,
) -> Vec<SessionProjection> {
    rows.iter()
        .filter(|row| row.company_id == actor.company_id)
        .map(|row| {
            let expired = row.expires_at <= now;
            let eligible = actor.permissions.can_manage;
            let can_review = row.status == SessionStatus::Pending
                && !expired
                && !actor.same_person(&row.created_by)
                && !actor.same_person(&row.requested_for)
                && eligible;
            let can_manage = row.status == SessionStatus::Approved && !expired && eligible;
            SessionProjection {
                id: row.id.clone(),
                ticket_id: row.ticket_id.clone(),
                requested_for: row.requested_for.clone(),
                status: row.status,
                effective_status: if row.status.active() && expired {
                    "expired"
                } else {
                    row.status.as_str()
                },
                expires_at: row.expires_at,
                version: row.version,
                can_review,
                can_manage,
            }
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionWorkItem {
    pub id: String,
    pub title: String,
    pub href: String,
    pub status: String,
    pub queue: String,
    pub can_act: bool,
}

pub fn work_items(
    actor: &SessionActor,
    rows: &[SessionProjection],
) -> Vec<SessionWorkItem> {
    if !actor.permissions.can_read && !actor.permissions.can_work {
        return Vec::new();
    }
    rows.iter()
        .map(|row| {
            let pending = row.status == SessionStatus::Pending;
            let active = row.effective_status == "approved";
            let can_act = if pending {
                row.can_review
            } else if active {
                row.can_manage
            } else {
                false
            };
            SessionWorkItem {
                id: row.id.clone(),
                title: "Temporary customer context access".to_owned(),
                href: format!("/support/sessions/{}", row.id),
                status: row.effective_status.to_owned(),
                queue: if can_act {
                    "todo".to_owned()
                } else if row.status.active() {
                    "waiting".to_owned()
                } else {
                    "completed".to_owned()
                },
                can_act,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(can_manage: bool, user: &str) -> SessionActor {
        SessionActor {
            user_id: user.into(),
            company_id: "company".into(),
            branch_id: Some("branch".into()),
            department_id: None,
            permissions: SessionPermissions {
                can_read: true,
                can_work: true,
                can_manage,
            },
            same_person_ids: vec![],
        }
    }
    fn snapshot(status: SessionStatus) -> SessionSnapshot {
        SessionSnapshot {
            id: "support-session-1".into(),
            company_id: "company".into(),
            branch_id: Some("branch".into()),
            department_id: None,
            ticket_id: "ticket-1".into(),
            ticket_version: 1,
            requested_for: "agent".into(),
            status,
            created_by: "agent".into(),
            reviewed_by: None,
            expires_at: 1_000,
            version: 1,
        }
    }
    fn request() -> SessionRequest {
        SessionRequest {
            ticket_id: "ticket-1".into(),
            ticket_version: 1,
            requested_for: "agent".into(),
            expires_at: 900,
            reason: "Temporary customer context".into(),
        }
    }

    #[test]
    fn requests_own_session_within_the_eight_hour_window() {
        let plan = plan_request(&actor(false, "agent"), "key", &request(), 100).unwrap();
        let SessionPlan::Request(plan) = plan else {
            panic!()
        };
        assert_eq!(plan.status, SessionStatus::Pending);
        assert_eq!(plan.expires_at, 900);
        let late = SessionRequest {
            expires_at: 100 + SESSION_WINDOW_MS + 1,
            ..request()
        };
        assert_eq!(
            plan_request(&actor(false, "agent"), "key", &late, 100)
                .unwrap_err()
                .code,
            "SUPPORT_SESSION_EXPIRY_INVALID"
        );
        let other = SessionRequest {
            requested_for: "someone".into(),
            ..request()
        };
        assert_eq!(
            plan_request(&actor(false, "agent"), "key", &other, 100)
                .unwrap_err()
                .code,
            "SUPPORT_SESSION_SELF_REQUEST_REQUIRED"
        );
    }

    #[test]
    fn requires_an_independent_manager_to_approve_and_revoke() {
        let agent = actor(false, "agent");
        let manager = actor(true, "manager");
        let pending = snapshot(SessionStatus::Pending);
        let approve = SessionAction::Approve {
            expected_version: 1,
            reason: "Approve the temporary session".into(),
        };
        let requester_manager = actor(true, "agent");
        assert_eq!(
            plan_change(&requester_manager, &pending, &approve, 200)
                .unwrap_err()
                .code,
            "INDEPENDENT_SESSION_REVIEWER_REQUIRED"
        );
        let mut other = pending.clone();
        other.created_by = "other".into();
        other.requested_for = "other".into();
        let SessionPlan::Change(plan) = plan_change(&manager, &other, &approve, 200).unwrap()
        else {
            panic!()
        };
        assert_eq!(plan.status, SessionStatus::Approved);
        assert_eq!(plan.reviewed_by.as_deref(), Some("manager"));
        let approved = snapshot(SessionStatus::Approved);
        let revoke = SessionAction::Revoke {
            expected_version: 1,
            reason: "Revoke the temporary session".into(),
        };
        let SessionPlan::Change(plan) = plan_change(&manager, &approved, &revoke, 200).unwrap()
        else {
            panic!()
        };
        assert_eq!(plan.status, SessionStatus::Revoked);
        assert_eq!(plan.revoked_by.as_deref(), Some("manager"));
        assert_eq!(agent.user_id, "agent");
    }

    #[test]
    fn rejects_stale_expired_and_owner_actions() {
        let manager = actor(true, "manager");
        let mut pending = snapshot(SessionStatus::Pending);
        pending.created_by = "other".into();
        pending.requested_for = "other".into();
        let approve = SessionAction::Approve {
            expected_version: 2,
            reason: "Approve the temporary session".into(),
        };
        assert_eq!(
            plan_change(&manager, &pending, &approve, 200)
                .unwrap_err()
                .code,
            "SUPPORT_SESSION_STATE_CHANGED"
        );
        let approve = SessionAction::Approve {
            expected_version: 1,
            reason: "Approve the temporary session".into(),
        };
        assert_eq!(
            plan_change(&manager, &pending, &approve, 1_000)
                .unwrap_err()
                .code,
            "SUPPORT_SESSION_EXPIRED"
        );
        let agent = actor(false, "agent");
        assert_eq!(
            plan_change(&agent, &pending, &approve, 200)
                .unwrap_err()
                .code,
            "SUPPORT_SESSION_MANAGE_REQUIRED"
        );
    }

    #[test]
    fn plans_capability_issue_and_revoke_without_session_events() {
        let manager = actor(true, "manager");
        let approved = snapshot(SessionStatus::Approved);
        let issue = SessionAction::IssueCapability {
            expected_version: 1,
            code: CapabilityCode::CustomerOrderContext,
            reason: "Issue the customer order context".into(),
        };
        let SessionPlan::Change(plan) = plan_change(&manager, &approved, &issue, 200).unwrap()
        else {
            panic!()
        };
        assert_eq!(plan.capability_code, Some(CapabilityCode::CustomerOrderContext));
        assert_eq!(plan.session_event(), None);
        assert_eq!(plan.version, 2);
        let revoke = SessionAction::RevokeCapability {
            expected_version: 2,
            capability_id: "support-capability-1".into(),
            expected_capability_version: 1,
            reason: "Revoke the customer order context".into(),
        };
        let mut reviewed = approved.clone();
        reviewed.version = 2;
        let SessionPlan::Change(plan) = plan_change(&manager, &reviewed, &revoke, 300).unwrap()
        else {
            panic!()
        };
        assert_eq!(plan.capability_id.as_deref(), Some("support-capability-1"));
        assert_eq!(plan.expected_capability_version, Some(1));
        assert_eq!(plan.session_event(), None);
    }

    #[test]
    fn projects_expiry_and_independent_review_work() {
        let manager = actor(true, "manager");
        let mut pending = snapshot(SessionStatus::Pending);
        pending.created_by = "agent".into();
        pending.requested_for = "agent".into();
        let rows = project(&manager, &[pending.clone()], 200);
        assert!(rows[0].can_review);
        assert_eq!(rows[0].effective_status, "pending");
        let expired = project(&manager, &[pending], 1_000);
        assert_eq!(expired[0].effective_status, "expired");
        assert!(!expired[0].can_review);
        let items = work_items(&manager, &expired);
        assert_eq!(items[0].queue, "waiting");
        assert!(!items[0].can_act);
    }
}
