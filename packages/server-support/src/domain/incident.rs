//! Portable Support incident lifecycle. No database, HTTP, clock or notification
//! effect is reachable here; adapters supply current facts and persist the plan.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

pub const MAX_TITLE: usize = 160;
pub const MAX_TEXT: usize = 4_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentStatus {
    Open,
    Investigating,
    Mitigated,
    Closed,
    Cancelled,
}

impl IncidentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Investigating => "investigating",
            Self::Mitigated => "mitigated",
            Self::Closed => "closed",
            Self::Cancelled => "cancelled",
        }
    }
    pub fn terminal(self) -> bool {
        matches!(self, Self::Closed | Self::Cancelled)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentSeverity {
    Critical,
    High,
    Normal,
}

impl IncidentSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::High => "high",
            Self::Normal => "normal",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IncidentPermissions {
    pub can_read: bool,
    pub can_work: bool,
    pub can_manage: bool,
}

#[derive(Clone, Debug)]
pub struct IncidentActor {
    pub user_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub permissions: IncidentPermissions,
    /// Resolved same-person identities (aliases/duplicates), never a caller label.
    pub same_person_ids: Vec<String>,
}

impl IncidentActor {
    pub fn same_person(&self, user_id: &str) -> bool {
        user_id == self.user_id || self.same_person_ids.iter().any(|id| id == user_id)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IncidentCreate {
    pub ticket_id: String,
    pub ticket_version: i64,
    pub title: String,
    pub impact: String,
    pub severity: IncidentSeverity,
    pub owner_id: String,
    pub due_date: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case", deny_unknown_fields)]
pub enum IncidentChange {
    Start {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
    },
    Update {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
    },
    Mitigate {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
        #[serde(rename = "rootCause")]
        root_cause: String,
        mitigation: String,
    },
    Reject {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
    },
    Close {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
    },
    Reopen {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
    },
    Cancel {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
    },
    Reassign {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
        #[serde(rename = "ownerId")]
        owner_id: String,
        #[serde(rename = "dueDate")]
        due_date: String,
    },
    Link {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
        #[serde(rename = "ticketId")]
        ticket_id: String,
        #[serde(rename = "ticketVersion")]
        ticket_version: i64,
    },
    Unlink {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
        #[serde(rename = "ticketId")]
        ticket_id: String,
    },
}

impl IncidentChange {
    pub fn expected_version(&self) -> i64 {
        match self {
            Self::Start { expected_version, .. }
            | Self::Update { expected_version, .. }
            | Self::Mitigate { expected_version, .. }
            | Self::Reject { expected_version, .. }
            | Self::Close { expected_version, .. }
            | Self::Reopen { expected_version, .. }
            | Self::Cancel { expected_version, .. }
            | Self::Reassign { expected_version, .. }
            | Self::Link { expected_version, .. }
            | Self::Unlink { expected_version, .. } => *expected_version,
        }
    }
    pub fn action(&self) -> &'static str {
        match self {
            Self::Start { .. } => "start",
            Self::Update { .. } => "update",
            Self::Mitigate { .. } => "mitigate",
            Self::Reject { .. } => "reject",
            Self::Close { .. } => "close",
            Self::Reopen { .. } => "reopen",
            Self::Cancel { .. } => "cancel",
            Self::Reassign { .. } => "reassign",
            Self::Link { .. } => "link",
            Self::Unlink { .. } => "unlink",
        }
    }
    pub fn reason(&self) -> &str {
        match self {
            Self::Start { reason, .. }
            | Self::Update { reason, .. }
            | Self::Mitigate { reason, .. }
            | Self::Reject { reason, .. }
            | Self::Close { reason, .. }
            | Self::Reopen { reason, .. }
            | Self::Cancel { reason, .. }
            | Self::Reassign { reason, .. }
            | Self::Link { reason, .. }
            | Self::Unlink { reason, .. } => reason,
        }
    }
    pub fn manage(&self) -> bool {
        matches!(
            self,
            Self::Reject { .. }
                | Self::Close { .. }
                | Self::Reopen { .. }
                | Self::Cancel { .. }
                | Self::Reassign { .. }
                | Self::Link { .. }
                | Self::Unlink { .. }
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncidentSnapshot {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub status: IncidentStatus,
    pub severity: IncidentSeverity,
    pub owner_id: String,
    pub mitigated_by: Option<String>,
    pub created_by: String,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncidentCreatePlan {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub ticket_id: String,
    pub ticket_version: i64,
    pub title: String,
    pub impact: String,
    pub severity: IncidentSeverity,
    pub status: IncidentStatus,
    pub owner_id: String,
    pub due_date: String,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncidentChangePlan {
    pub id: String,
    pub company_id: String,
    pub action: &'static str,
    pub status: IncidentStatus,
    pub owner_id: String,
    pub due_date: Option<String>,
    pub root_cause: Option<String>,
    pub mitigation: Option<String>,
    pub mitigated_by: Option<String>,
    pub ticket_id: Option<String>,
    pub ticket_version: Option<i64>,
    pub reason: String,
    pub version: i64,
    pub event_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IncidentPlan {
    Create(IncidentCreatePlan),
    Change(IncidentChangePlan),
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

fn date(value: &str) -> Result<String, BusinessError> {
    let format = time::format_description::parse_borrowed::<2>("[year]-[month]-[day]")
        .map_err(|_| invalid("SUPPORT_INCIDENT_DATE_INVALID", "Provide a valid due date."))?;
    time::Date::parse(value, &format)
        .map_err(|_| invalid("SUPPORT_INCIDENT_DATE_INVALID", "Provide a valid due date."))?;
    Ok(value.to_owned())
}

fn version(value: i64) -> Result<i64, BusinessError> {
    if !(1..=9_007_199_254_740_991).contains(&value) {
        return Err(invalid(
            "SUPPORT_INCIDENT_VERSION_INVALID",
            "Provide a positive version.",
        ));
    }
    Ok(value)
}

pub fn plan_create(
    actor: &IncidentActor,
    key: &str,
    input: &IncidentCreate,
    now: i64,
) -> Result<IncidentPlan, BusinessError> {
    if !actor.permissions.can_manage {
        return Err(forbidden(
            "SUPPORT_INCIDENT_MANAGE_REQUIRED",
            "Choose a role that can manage Support incidents.",
        ));
    }
    text(&input.ticket_id, "SUPPORT_INCIDENT_SOURCE_INVALID", 1, 200)?;
    version(input.ticket_version)?;
    text(&input.owner_id, "SUPPORT_INCIDENT_OWNER_INVALID", 1, 200)?;
    date(&input.due_date)?;
    Ok(IncidentPlan::Create(IncidentCreatePlan {
        id: format!("incident-{key}"),
        company_id: actor.company_id.clone(),
        branch_id: actor.branch_id.clone(),
        department_id: actor.department_id.clone(),
        ticket_id: input.ticket_id.clone(),
        ticket_version: input.ticket_version,
        title: text(&input.title, "SUPPORT_INCIDENT_TITLE_INVALID", 3, MAX_TITLE)?,
        impact: text(&input.impact, "SUPPORT_INCIDENT_IMPACT_INVALID", 3, MAX_TEXT)?,
        severity: input.severity,
        status: IncidentStatus::Open,
        owner_id: input.owner_id.clone(),
        due_date: input.due_date.clone(),
        created_by: actor.user_id.clone(),
        created_at: now,
        updated_at: now,
    }))
}

pub fn plan_change(
    actor: &IncidentActor,
    snapshot: &IncidentSnapshot,
    input: &IncidentChange,
    now: i64,
) -> Result<IncidentPlan, BusinessError> {
    if !actor.permissions.can_read {
        return Err(forbidden(
            "SUPPORT_INCIDENT_READ_REQUIRED",
            "Choose a role that can read Support incidents.",
        ));
    }
    if snapshot.company_id != actor.company_id {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "SUPPORT_INCIDENT_NOT_FOUND",
            "Incident not found in this scope.",
        ));
    }
    if input.expected_version() != snapshot.version {
        return Err(conflict(
            "SUPPORT_INCIDENT_STATE_CHANGED",
            "Review the current incident state.",
        ));
    }
    let reason = text(input.reason(), "SUPPORT_INCIDENT_REASON_INVALID", 3, MAX_TEXT)?;
    if input.manage() {
        if !actor.permissions.can_manage {
            return Err(forbidden(
                "SUPPORT_INCIDENT_MANAGE_REQUIRED",
                "Choose a role that can manage Support incidents.",
            ));
        }
    } else {
        if !actor.permissions.can_work {
            return Err(forbidden(
                "SUPPORT_INCIDENT_WORK_REQUIRED",
                "Choose a role that can work Support incidents.",
            ));
        }
        if !actor.same_person(&snapshot.owner_id) {
            return Err(forbidden(
                "SUPPORT_INCIDENT_OWNER_REQUIRED",
                "The assigned incident owner must perform this action.",
            ));
        }
    }
    let status = match input {
        IncidentChange::Start { .. } => {
            require_status(snapshot, &[IncidentStatus::Open])?;
            IncidentStatus::Investigating
        }
        IncidentChange::Update { .. } => {
            require_status(
                snapshot,
                &[IncidentStatus::Open, IncidentStatus::Investigating],
            )?;
            snapshot.status
        }
        IncidentChange::Mitigate { .. } => {
            require_status(snapshot, &[IncidentStatus::Investigating])?;
            IncidentStatus::Mitigated
        }
        IncidentChange::Reject { .. } => {
            require_status(snapshot, &[IncidentStatus::Mitigated])?;
            IncidentStatus::Investigating
        }
        IncidentChange::Close { .. } => {
            require_status(snapshot, &[IncidentStatus::Mitigated])?;
            if actor.same_person(&snapshot.owner_id)
                || snapshot
                    .mitigated_by
                    .as_deref()
                    .is_some_and(|reviewer| actor.same_person(reviewer))
            {
                return Err(forbidden(
                    "INDEPENDENT_APPROVER_REQUIRED",
                    "Another manager must review the incident mitigation.",
                ));
            }
            IncidentStatus::Closed
        }
        IncidentChange::Reopen { .. } => {
            require_status(snapshot, &[IncidentStatus::Closed])?;
            IncidentStatus::Investigating
        }
        IncidentChange::Cancel { .. } => {
            require_status(
                snapshot,
                &[
                    IncidentStatus::Open,
                    IncidentStatus::Investigating,
                    IncidentStatus::Mitigated,
                ],
            )?;
            IncidentStatus::Cancelled
        }
        IncidentChange::Reassign { .. } => {
            if snapshot.status.terminal() {
                return Err(conflict(
                    "SUPPORT_INCIDENT_STATE_CHANGED",
                    "Review the current incident state.",
                ));
            }
            snapshot.status
        }
        IncidentChange::Link { .. } | IncidentChange::Unlink { .. } => {
            if snapshot.status.terminal() {
                return Err(conflict(
                    "SUPPORT_INCIDENT_STATE_CHANGED",
                    "Review the current incident state.",
                ));
            }
            snapshot.status
        }
    };
    let (owner_id, due_date, root_cause, mitigation, mitigated_by, ticket_id, ticket_version) =
        match input {
            IncidentChange::Reassign {
                owner_id, due_date, ..
            } => (
                text(owner_id, "SUPPORT_INCIDENT_OWNER_INVALID", 1, 200)?,
                Some(date(due_date)?),
                None,
                None,
                snapshot.mitigated_by.clone(),
                None,
                None,
            ),
            IncidentChange::Mitigate {
                root_cause,
                mitigation,
                ..
            } => (
                snapshot.owner_id.clone(),
                None,
                Some(text(
                    root_cause,
                    "SUPPORT_INCIDENT_ROOT_CAUSE_INVALID",
                    3,
                    MAX_TEXT,
                )?),
                Some(text(
                    mitigation,
                    "SUPPORT_INCIDENT_MITIGATION_INVALID",
                    3,
                    MAX_TEXT,
                )?),
                Some(actor.user_id.clone()),
                None,
                None,
            ),
            IncidentChange::Reject { .. } => (
                snapshot.owner_id.clone(),
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            IncidentChange::Link {
                ticket_id,
                ticket_version,
                ..
            } => (
                snapshot.owner_id.clone(),
                None,
                None,
                None,
                snapshot.mitigated_by.clone(),
                Some(text(
                    ticket_id,
                    "SUPPORT_INCIDENT_SOURCE_INVALID",
                    1,
                    200,
                )?),
                Some(version(*ticket_version)?),
            ),
            IncidentChange::Unlink { ticket_id, .. } => (
                snapshot.owner_id.clone(),
                None,
                None,
                None,
                snapshot.mitigated_by.clone(),
                Some(text(
                    ticket_id,
                    "SUPPORT_INCIDENT_SOURCE_INVALID",
                    1,
                    200,
                )?),
                None,
            ),
            _ => (
                snapshot.owner_id.clone(),
                None,
                None,
                None,
                snapshot.mitigated_by.clone(),
                None,
                None,
            ),
        };
    Ok(IncidentPlan::Change(IncidentChangePlan {
        id: snapshot.id.clone(),
        company_id: snapshot.company_id.clone(),
        action: input.action(),
        status,
        owner_id,
        due_date,
        root_cause,
        mitigation,
        mitigated_by,
        ticket_id,
        ticket_version,
        reason,
        version: snapshot.version + 1,
        event_at: now,
    }))
}

fn require_status(
    snapshot: &IncidentSnapshot,
    allowed: &[IncidentStatus],
) -> Result<(), BusinessError> {
    if !allowed.contains(&snapshot.status) {
        return Err(conflict(
            "SUPPORT_INCIDENT_STATE_CHANGED",
            "Review the current incident state.",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncidentWorkItem {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub href: String,
    pub status: String,
    pub queue: String,
    pub can_act: bool,
    pub responsible_queue: String,
    pub relationships: Vec<String>,
}

pub fn work_items(actor: &IncidentActor, rows: &[(IncidentSnapshot, String)]) -> Vec<IncidentWorkItem> {
    if !actor.permissions.can_read && !actor.permissions.can_work {
        return Vec::new();
    }
    rows.iter()
        .filter(|(row, _)| row.company_id == actor.company_id)
        .map(|(row, title)| {
            let done = row.status.terminal();
            let review = row.status == IncidentStatus::Mitigated;
            let can_act = !done
                && if review {
                    actor.permissions.can_manage
                        && !actor.same_person(&row.owner_id)
                        && row
                            .mitigated_by
                            .as_deref()
                            .is_none_or(|reviewer| !actor.same_person(reviewer))
                } else {
                    actor.permissions.can_work && actor.same_person(&row.owner_id)
                };
            let queue = if done {
                "completed"
            } else if can_act {
                "todo"
            } else {
                "waiting"
            };
            let responsible_queue = if review {
                "Independent incident review".to_owned()
            } else {
                "Incident owner".to_owned()
            };
            let mut relationships = Vec::new();
            if row.created_by == actor.user_id {
                relationships.push("requested".to_owned());
            }
            if actor.same_person(&row.owner_id) {
                relationships.push("assigned".to_owned());
            }
            IncidentWorkItem {
                id: row.id.clone(),
                title: title.clone(),
                detail: "Shared Support incident".to_owned(),
                href: format!("/support/incidents/{}", row.id),
                status: row.status.as_str().to_owned(),
                queue: queue.to_owned(),
                can_act,
                responsible_queue,
                relationships,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(can_manage: bool, can_work: bool, user: &str) -> IncidentActor {
        IncidentActor {
            user_id: user.into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            permissions: IncidentPermissions {
                can_read: true,
                can_work,
                can_manage,
            },
            same_person_ids: vec![],
        }
    }
    fn snapshot(status: IncidentStatus) -> IncidentSnapshot {
        IncidentSnapshot {
            id: "incident-1".into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            status,
            severity: IncidentSeverity::High,
            owner_id: "agent".into(),
            mitigated_by: None,
            created_by: "manager".into(),
            version: 1,
        }
    }

    #[test]
    fn parses_the_browser_change_wire() {
        let value: IncidentChange = serde_json::from_str(
            r#"{"action":"start","expectedVersion":1,"reason":"Start the HTTP incident"}"#,
        )
        .unwrap();
        assert_eq!(value.action(), "start");
        let value: IncidentChange = serde_json::from_str(
            r#"{"action":"mitigate","expectedVersion":2,"reason":"Mitigate","rootCause":"Cause","mitigation":"Fix"}"#,
        )
        .unwrap();
        assert_eq!(value.action(), "mitigate");
    }

    #[test]
    fn creates_an_open_incident_from_a_sales_case() {
        let plan = plan_create(
            &actor(true, false, "manager"),
            "key",
            &IncidentCreate {
                ticket_id: "ticket-1".into(),
                ticket_version: 1,
                title: "Checkout outage".into(),
                impact: "Customers cannot complete checkout.".into(),
                severity: IncidentSeverity::Critical,
                owner_id: "agent".into(),
                due_date: "2026-11-10".into(),
            },
            10,
        )
        .unwrap();
        let IncidentPlan::Create(plan) = plan else {
            panic!()
        };
        assert_eq!(plan.status, IncidentStatus::Open);
        assert_eq!(plan.severity, IncidentSeverity::Critical);
        assert_eq!(plan.ticket_id, "ticket-1");
    }

    #[test]
    fn enforces_owner_and_independent_close() {
        let agent = actor(false, true, "agent");
        let manager = actor(true, false, "manager");
        let mut row = snapshot(IncidentStatus::Open);
        let start = IncidentChange::Start {
            expected_version: 1,
            reason: "Start investigation".into(),
        };
        let IncidentPlan::Change(plan) = plan_change(&agent, &row, &start, 2).unwrap() else {
            panic!()
        };
        assert_eq!(plan.status, IncidentStatus::Investigating);
        row.status = IncidentStatus::Investigating;
        let mitigate = IncidentChange::Mitigate {
            expected_version: 1,
            reason: "Mitigation proposed".into(),
            root_cause: "Expired certificate".into(),
            mitigation: "Rotate the certificate".into(),
        };
        let IncidentPlan::Change(plan) = plan_change(&agent, &row, &mitigate, 3).unwrap() else {
            panic!()
        };
        assert_eq!(plan.status, IncidentStatus::Mitigated);
        assert_eq!(plan.mitigated_by.as_deref(), Some("agent"));
        row.status = IncidentStatus::Mitigated;
        row.mitigated_by = Some("agent".into());
        let close = IncidentChange::Close {
            expected_version: 1,
            reason: "Independent review".into(),
        };
        let resolver = actor(true, false, "agent");
        assert_eq!(
            plan_change(&resolver, &row, &close, 4).unwrap_err().code,
            "INDEPENDENT_APPROVER_REQUIRED"
        );
        row.owner_id = "other".into();
        row.mitigated_by = Some("other".into());
        let IncidentPlan::Change(plan) = plan_change(&manager, &row, &close, 4).unwrap() else {
            panic!()
        };
        assert_eq!(plan.status, IncidentStatus::Closed);
    }

    #[test]
    fn rejects_stale_and_terminal_changes() {
        let manager = actor(true, false, "manager");
        let mut row = snapshot(IncidentStatus::Open);
        let stale = IncidentChange::Start {
            expected_version: 2,
            reason: "Start".into(),
        };
        assert_eq!(
            plan_change(&manager, &row, &stale, 2).unwrap_err().code,
            "SUPPORT_INCIDENT_STATE_CHANGED"
        );
        row.status = IncidentStatus::Closed;
        let cancel = IncidentChange::Cancel {
            expected_version: 1,
            reason: "Cancel".into(),
        };
        assert_eq!(
            plan_change(&manager, &row, &cancel, 2).unwrap_err().code,
            "SUPPORT_INCIDENT_STATE_CHANGED"
        );
    }

    #[test]
    fn work_items_separate_investigation_and_independent_review() {
        let manager = actor(true, false, "manager");
        let mut open = snapshot(IncidentStatus::Open);
        open.owner_id = "agent".into();
        let mut review = snapshot(IncidentStatus::Mitigated);
        review.owner_id = "agent".into();
        review.mitigated_by = Some("agent".into());
        let mut closed = snapshot(IncidentStatus::Closed);
        closed.owner_id = "agent".into();
        let items = work_items(
            &manager,
            &[
                (open, "Open".into()),
                (review, "Review".into()),
                (closed, "Closed".into()),
            ],
        );
        assert_eq!(items[0].queue, "waiting");
        assert_eq!(items[1].queue, "todo");
        assert_eq!(items[1].responsible_queue, "Independent incident review");
        assert_eq!(items[2].queue, "completed");
        assert!(!items[2].can_act);
    }
}
