//! Portable Support ticket lifecycle. No database, HTTP, clock or notification
//! effect is reachable here; adapters supply current facts and persist the plan.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

pub const MAX_TITLE: usize = 160;
pub const MAX_TEXT: usize = 4_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketStatus {
    Triage,
    Open,
    InProgress,
    WaitingCustomer,
    Resolved,
    Closed,
    Cancelled,
}

impl TicketStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Triage => "triage",
            Self::Open => "open",
            Self::InProgress => "in_progress",
            Self::WaitingCustomer => "waiting_customer",
            Self::Resolved => "resolved",
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
pub enum SourceKind {
    General,
    SalesOrder,
    PosSale,
    PmsRecord,
    Asset,
    FieldService,
}

impl SourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::SalesOrder => "sales_order",
            Self::PosSale => "pos_sale",
            Self::PmsRecord => "pms_record",
            Self::Asset => "asset",
            Self::FieldService => "field_service",
        }
    }
    pub fn order_backed(self) -> bool {
        matches!(self, Self::SalesOrder)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TicketPermissions {
    pub can_read: bool,
    pub can_create: bool,
    pub can_write: bool,
    pub can_manage: bool,
    pub can_work: bool,
}

#[derive(Clone, Debug)]
pub struct TicketActor {
    pub user_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub permissions: TicketPermissions,
    /// Resolved same-person identities (aliases/duplicates), never a caller label.
    pub same_person_ids: Vec<String>,
}

impl TicketActor {
    pub fn same_person(&self, user_id: &str) -> bool {
        user_id == self.user_id || self.same_person_ids.iter().any(|id| id == user_id)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TicketCreate {
    General {
        title: String,
        issue: String,
    },
    Order {
        #[serde(rename = "orderId")]
        order_id: String,
        #[serde(rename = "sourceVersion")]
        source_version: i64,
        title: String,
        issue: String,
        #[serde(rename = "ownerId")]
        owner_id: String,
        #[serde(rename = "dueDate")]
        due_date: String,
    },
    Source {
        source: TicketSourceReference,
        title: String,
        issue: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TicketSourceReference {
    pub kind: String,
    pub id: String,
    pub version: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case", deny_unknown_fields)]
pub enum TicketChange {
    Start {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
    },
    WaitCustomer {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
    },
    Resume {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
    },
    Resolve {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
    },
    RejectResolution {
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
    Triage {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
        #[serde(rename = "ownerId")]
        owner_id: String,
        #[serde(rename = "dueDate")]
        due_date: String,
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
    Confirm {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
        #[serde(rename = "noteId")]
        note_id: String,
        #[serde(rename = "noteVersion")]
        note_version: i64,
    },
    ConfirmGeneral {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
    },
    EditReport {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        title: String,
        issue: String,
        reason: String,
    },
    WithdrawReport {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
    },
    AddInformation {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
    },
}

impl TicketChange {
    pub fn expected_version(&self) -> i64 {
        match self {
            Self::Start { expected_version, .. }
            | Self::WaitCustomer { expected_version, .. }
            | Self::Resume { expected_version, .. }
            | Self::Resolve { expected_version, .. }
            | Self::RejectResolution { expected_version, .. }
            | Self::Reopen { expected_version, .. }
            | Self::Cancel { expected_version, .. }
            | Self::Triage { expected_version, .. }
            | Self::Reassign { expected_version, .. }
            | Self::Confirm { expected_version, .. }
            | Self::ConfirmGeneral { expected_version, .. }
            | Self::EditReport { expected_version, .. }
            | Self::WithdrawReport { expected_version, .. }
            | Self::AddInformation { expected_version, .. } => *expected_version,
        }
    }
    pub fn action(&self) -> &'static str {
        match self {
            Self::Start { .. } => "start",
            Self::WaitCustomer { .. } => "wait-customer",
            Self::Resume { .. } => "resume",
            Self::Resolve { .. } => "resolve",
            Self::RejectResolution { .. } => "reject-resolution",
            Self::Reopen { .. } => "reopen",
            Self::Cancel { .. } => "cancel",
            Self::Triage { .. } => "triage",
            Self::Reassign { .. } => "reassign",
            Self::Confirm { .. } => "confirm",
            Self::ConfirmGeneral { .. } => "confirm-general",
            Self::EditReport { .. } => "edit-report",
            Self::WithdrawReport { .. } => "withdraw-report",
            Self::AddInformation { .. } => "add-information",
        }
    }
    pub fn reason(&self) -> &str {
        match self {
            Self::Start { reason, .. }
            | Self::WaitCustomer { reason, .. }
            | Self::Resume { reason, .. }
            | Self::Resolve { reason, .. }
            | Self::RejectResolution { reason, .. }
            | Self::Reopen { reason, .. }
            | Self::Cancel { reason, .. }
            | Self::Triage { reason, .. }
            | Self::Reassign { reason, .. }
            | Self::Confirm { reason, .. }
            | Self::ConfirmGeneral { reason, .. }
            | Self::EditReport { reason, .. }
            | Self::WithdrawReport { reason, .. }
            | Self::AddInformation { reason, .. } => reason,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TicketSnapshot {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub source_kind: SourceKind,
    pub status: TicketStatus,
    pub owner_id: Option<String>,
    pub resolved_by: Option<String>,
    pub created_by: String,
    pub confirmation_note_id: Option<String>,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TicketCreatePlan {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub source_kind: SourceKind,
    pub source_record_json: Option<String>,
    pub order_id: Option<String>,
    pub customer_id: Option<String>,
    pub title: String,
    pub issue: String,
    pub owner_id: Option<String>,
    pub due_date: Option<String>,
    pub status: TicketStatus,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub event_action: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TicketChangePlan {
    pub id: String,
    pub company_id: String,
    pub action: &'static str,
    pub status: TicketStatus,
    pub owner_id: Option<String>,
    pub due_date: Option<String>,
    pub title: Option<String>,
    pub issue: Option<String>,
    pub version: i64,
    pub resolved_by: Option<String>,
    pub confirmation_note_id: Option<String>,
    pub reason: String,
    pub event_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TicketPlan {
    Create(TicketCreatePlan),
    Change(TicketChangePlan),
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

pub fn text(value: &str, field: &'static str, min: usize, max: usize) -> Result<String, BusinessError> {
    let trimmed = value.trim();
    if trimmed.len() < min || trimmed.len() > max {
        return Err(invalid(field, "Provide a bounded text value."));
    }
    Ok(trimmed.to_owned())
}

pub fn date(value: &str) -> Result<String, BusinessError> {
    let format = time::format_description::parse_borrowed::<2>("[year]-[month]-[day]")
        .map_err(|_| invalid("SUPPORT_DATE_INVALID", "Provide a valid due date."))?;
    time::Date::parse(value, &format)
        .map_err(|_| invalid("SUPPORT_DATE_INVALID", "Provide a valid due date."))?;
    Ok(value.to_owned())
}

fn version(value: i64) -> Result<i64, BusinessError> {
    if !(1..=9_007_199_254_740_991).contains(&value) {
        return Err(invalid("SUPPORT_VERSION_INVALID", "Provide a positive version."));
    }
    Ok(value)
}

#[derive(Serialize)]
struct SourceRecord<'a> {
    kind: &'a str,
    id: &'a str,
    version: i64,
    #[serde(rename = "companyId")]
    company_id: &'a str,
}

pub fn plan_create(
    actor: &TicketActor,
    key: &str,
    input: &TicketCreate,
    now: i64,
) -> Result<TicketPlan, BusinessError> {
    if !actor.permissions.can_write && !actor.permissions.can_create {
        return Err(forbidden(
            "SUPPORT_CREATE_REQUIRED",
            "Choose a role that can create Support tickets.",
        ));
    }
    let id = format!("ticket-{key}");
    let (title, issue) = match input {
        TicketCreate::General { title, issue } => (
            text(title, "SUPPORT_TITLE_INVALID", 3, MAX_TITLE)?,
            text(issue, "SUPPORT_ISSUE_INVALID", 3, MAX_TEXT)?,
        ),
        TicketCreate::Order {
            order_id,
            source_version,
            title,
            issue,
            owner_id,
            due_date,
        } => {
            text(order_id, "SUPPORT_ORDER_INVALID", 1, 200)?;
            version(*source_version)?;
            text(owner_id, "SUPPORT_OWNER_INVALID", 1, 200)?;
            date(due_date)?;
            (
                text(title, "SUPPORT_TITLE_INVALID", 3, MAX_TITLE)?,
                text(issue, "SUPPORT_ISSUE_INVALID", 3, MAX_TEXT)?,
            )
        }
        TicketCreate::Source {
            source,
            title,
            issue,
        } => {
            if !matches!(source.kind.as_str(), "pos_sale" | "pms_record" | "asset") {
                return Err(invalid(
                    "SUPPORT_SOURCE_INVALID",
                    "Choose a supported source record.",
                ));
            }
            text(&source.id, "SUPPORT_SOURCE_INVALID", 1, 200)?;
            version(source.version)?;
            (
                text(title, "SUPPORT_TITLE_INVALID", 3, MAX_TITLE)?,
                text(issue, "SUPPORT_ISSUE_INVALID", 3, MAX_TEXT)?,
            )
        }
    };
    let (source_kind, order_id, owner_id, due_date, status, event_action, source_record_json) =
        match input {
            TicketCreate::General { .. } => (
                SourceKind::General,
                None,
                None,
                None,
                TicketStatus::Triage,
                "report",
                None,
            ),
            TicketCreate::Order {
                order_id,
                owner_id,
                due_date,
                ..
            } => (
                SourceKind::SalesOrder,
                Some(order_id.clone()),
                Some(owner_id.clone()),
                Some(due_date.clone()),
                TicketStatus::Open,
                "create",
                None,
            ),
            TicketCreate::Source { source, .. } => (
                match source.kind.as_str() {
                    "pos_sale" => SourceKind::PosSale,
                    "pms_record" => SourceKind::PmsRecord,
                    _ => SourceKind::Asset,
                },
                None,
                None,
                None,
                TicketStatus::Triage,
                "report",
                Some(
                    serde_json::to_string(&SourceRecord {
                        kind: &source.kind,
                        id: &source.id,
                        version: source.version,
                        company_id: &actor.company_id,
                    })
                    .map_err(|_| invalid("SUPPORT_SOURCE_INVALID", "Choose a supported source record."))?,
                ),
            ),
        };
    Ok(TicketPlan::Create(TicketCreatePlan {
        id,
        company_id: actor.company_id.clone(),
        branch_id: actor.branch_id.clone(),
        department_id: actor.department_id.clone(),
        source_kind,
        source_record_json,
        order_id,
        customer_id: None,
        title,
        issue,
        owner_id,
        due_date,
        status,
        created_by: actor.user_id.clone(),
        created_at: now,
        updated_at: now,
        event_action,
    }))
}

pub fn plan_change(
    actor: &TicketActor,
    snapshot: &TicketSnapshot,
    input: &TicketChange,
    now: i64,
) -> Result<TicketPlan, BusinessError> {
    if !actor.permissions.can_read {
        return Err(forbidden(
            "SUPPORT_READ_REQUIRED",
            "Choose a role that can read Support tickets.",
        ));
    }
    if snapshot.company_id != actor.company_id {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "SUPPORT_TICKET_NOT_FOUND",
            "Ticket not found in this scope.",
        ));
    }
    if snapshot.source_kind == SourceKind::FieldService {
        return Err(conflict(
            "SOURCE_WORKSPACE_REQUIRED",
            "Continue this case in Field Service.",
        ));
    }
    if input.expected_version() != snapshot.version {
        return Err(conflict(
            "SUPPORT_STATE_CHANGED",
            "Review the current ticket state.",
        ));
    }
    let reason = text(input.reason(), "SUPPORT_REASON_INVALID", 3, MAX_TEXT)?;
    let reporting = matches!(
        input,
        TicketChange::EditReport { .. }
            | TicketChange::WithdrawReport { .. }
            | TicketChange::AddInformation { .. }
    );
    if reporting {
        if !actor.permissions.can_write {
            return Err(forbidden(
                "SUPPORT_REPORTER_REQUIRED",
                "Only the original reporter can change their report.",
            ));
        }
        if snapshot.created_by != actor.user_id {
            return Err(forbidden(
                "SUPPORT_REPORTER_REQUIRED",
                "Only the original reporter can change their report.",
            ));
        }
        if snapshot.status.terminal()
            || matches!(input, TicketChange::EditReport { .. })
                && snapshot.status != TicketStatus::Triage
        {
            return Err(conflict(
                "SUPPORT_REPORT_CHANGED",
                "This report is no longer editable. Add current information or ask Support to reopen it.",
            ));
        }
        let (status, title, issue) = match input {
            TicketChange::EditReport { title, issue, .. } => (
                snapshot.status,
                Some(text(title, "SUPPORT_TITLE_INVALID", 3, MAX_TITLE)?),
                Some(text(issue, "SUPPORT_ISSUE_INVALID", 3, MAX_TEXT)?),
            ),
            TicketChange::WithdrawReport { .. } => (TicketStatus::Cancelled, None, None),
            TicketChange::AddInformation { .. } => (snapshot.status, None, None),
            _ => unreachable!(),
        };
        return Ok(TicketPlan::Change(TicketChangePlan {
            id: snapshot.id.clone(),
            company_id: snapshot.company_id.clone(),
            action: input.action(),
            status,
            owner_id: snapshot.owner_id.clone(),
            due_date: None,
            title,
            issue,
            version: snapshot.version + 1,
            resolved_by: snapshot.resolved_by.clone(),
            confirmation_note_id: snapshot.confirmation_note_id.clone(),
            reason,
            event_at: now,
        }));
    }
    let manage = matches!(
        input,
        TicketChange::Triage { .. }
            | TicketChange::Reassign { .. }
            | TicketChange::Confirm { .. }
            | TicketChange::ConfirmGeneral { .. }
            | TicketChange::RejectResolution { .. }
            | TicketChange::Reopen { .. }
            | TicketChange::Cancel { .. }
    );
    if manage && !actor.permissions.can_manage {
        return Err(forbidden(
            "SUPPORT_MANAGE_REQUIRED",
            "Choose a role that can manage Support tickets.",
        ));
    }
    if !manage && !actor.permissions.can_work {
        return Err(forbidden(
            "SUPPORT_WORK_REQUIRED",
            "Choose a role that can work Support tickets.",
        ));
    }
    let owner = snapshot.owner_id.as_deref();
    if !manage && owner != Some(actor.user_id.as_str()) {
        return Err(forbidden(
            "SUPPORT_OWNER_REQUIRED",
            "The assigned support colleague must perform this action.",
        ));
    }
    let status = match input {
        TicketChange::Triage { .. } => {
            require_status(snapshot, &[TicketStatus::Triage])?;
            TicketStatus::Open
        }
        TicketChange::Start { .. } => {
            require_status(snapshot, &[TicketStatus::Open])?;
            TicketStatus::InProgress
        }
        TicketChange::WaitCustomer { .. } => {
            require_status(snapshot, &[TicketStatus::InProgress])?;
            TicketStatus::WaitingCustomer
        }
        TicketChange::Resume { .. } => {
            require_status(snapshot, &[TicketStatus::WaitingCustomer])?;
            TicketStatus::InProgress
        }
        TicketChange::Resolve { .. } => {
            require_status(snapshot, &[TicketStatus::InProgress])?;
            TicketStatus::Resolved
        }
        TicketChange::RejectResolution { .. } => {
            require_status(snapshot, &[TicketStatus::Resolved])?;
            TicketStatus::InProgress
        }
        TicketChange::Confirm { .. } => {
            require_status(snapshot, &[TicketStatus::Resolved])?;
            if snapshot.source_kind != SourceKind::SalesOrder {
                return Err(conflict(
                    "SUPPORT_CONFIRMATION_MISMATCH",
                    "Choose the confirmation method for this issue source.",
                ));
            }
            TicketStatus::Closed
        }
        TicketChange::ConfirmGeneral { .. } => {
            require_status(snapshot, &[TicketStatus::Resolved])?;
            if snapshot.source_kind == SourceKind::SalesOrder {
                return Err(conflict(
                    "SUPPORT_CONFIRMATION_MISMATCH",
                    "Choose the confirmation method for this issue source.",
                ));
            }
            TicketStatus::Closed
        }
        TicketChange::Reopen { .. } => {
            require_status(snapshot, &[TicketStatus::Closed])?;
            TicketStatus::InProgress
        }
        TicketChange::Cancel { .. } => {
            require_status(snapshot, &[TicketStatus::Triage, TicketStatus::Open])?;
            TicketStatus::Cancelled
        }
        TicketChange::Reassign { .. } => {
            if snapshot.status.terminal() {
                return Err(conflict(
                    "SUPPORT_STATE_CHANGED",
                    "Review the current ticket state.",
                ));
            }
            snapshot.status
        }
        _ => unreachable!(),
    };
    if matches!(
        input,
        TicketChange::Confirm { .. } | TicketChange::ConfirmGeneral { .. }
    ) {
        if owner.is_some_and(|owner| actor.same_person(owner))
            || snapshot
                .resolved_by
                .as_deref()
                .is_some_and(|resolver| actor.same_person(resolver))
        {
            return Err(forbidden(
                "INDEPENDENT_CONFIRMATION_REQUIRED",
                "Another support manager must review this resolution.",
            ));
        }
    }
    let (owner_id, due_date, title, issue, confirmation_note_id) = match input {
        TicketChange::Triage {
            owner_id, due_date, ..
        }
        | TicketChange::Reassign {
            owner_id, due_date, ..
        } => (
            Some(text(owner_id, "SUPPORT_OWNER_INVALID", 1, 200)?),
            Some(date(due_date)?),
            None,
            None,
            snapshot.confirmation_note_id.clone(),
        ),
        TicketChange::Confirm {
            note_id,
            note_version,
            ..
        } => {
            version(*note_version)?;
            (
                snapshot.owner_id.clone(),
                None,
                None,
                None,
                Some(text(note_id, "SUPPORT_NOTE_INVALID", 1, 200)?),
            )
        }
        _ => (
            snapshot.owner_id.clone(),
            None,
            None,
            None,
            snapshot.confirmation_note_id.clone(),
        ),
    };
    Ok(TicketPlan::Change(TicketChangePlan {
        id: snapshot.id.clone(),
        company_id: snapshot.company_id.clone(),
        action: input.action(),
        status,
        owner_id,
        due_date,
        title,
        issue,
        version: snapshot.version + 1,
        resolved_by: if matches!(input, TicketChange::Resolve { .. }) {
            Some(actor.user_id.clone())
        } else {
            snapshot.resolved_by.clone()
        },
        confirmation_note_id,
        reason,
        event_at: now,
    }))
}

fn require_status(
    snapshot: &TicketSnapshot,
    allowed: &[TicketStatus],
) -> Result<(), BusinessError> {
    if !allowed.contains(&snapshot.status) {
        return Err(conflict(
            "SUPPORT_STATE_CHANGED",
            "Review the current ticket state.",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TicketWorkItem {
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

pub fn work_items(
    actor: &TicketActor,
    rows: &[(TicketSnapshot, String)],
) -> Vec<TicketWorkItem> {
    if !actor.permissions.can_read && !actor.permissions.can_work {
        return Vec::new();
    }
    rows.iter()
        .filter(|(row, _)| row.company_id == actor.company_id)
        .map(|(row, title)| {
            let terminal = row.status.terminal();
            let triage = row.status == TicketStatus::Triage;
            let review = row.status == TicketStatus::Resolved;
            let can_act = if terminal {
                false
            } else if triage {
                actor.permissions.can_manage
            } else if review {
                actor.permissions.can_manage
                    && row
                        .owner_id
                        .as_deref()
                        .is_none_or(|owner| !actor.same_person(owner))
                    && row
                        .resolved_by
                        .as_deref()
                        .is_none_or(|resolver| !actor.same_person(resolver))
            } else {
                row.owner_id.as_deref() == Some(actor.user_id.as_str())
                    && actor.permissions.can_work
            };
            let queue = if terminal {
                "completed"
            } else if can_act {
                "todo"
            } else if triage || review {
                "waiting"
            } else {
                "waiting"
            };
            let responsible_queue = if triage {
                "Support triage".to_owned()
            } else if review {
                "Independent Support review".to_owned()
            } else {
                row.owner_id
                    .as_ref()
                    .map(|_| "Support agent".to_owned())
                    .unwrap_or_else(|| "Support triage".to_owned())
            };
            let mut relationships = Vec::new();
            if row.created_by == actor.user_id {
                relationships.push("requested".to_owned());
            }
            if row.owner_id.as_deref() == Some(actor.user_id.as_str()) {
                relationships.push("assigned".to_owned());
            }
            TicketWorkItem {
                id: row.id.clone(),
                title: title.clone(),
                detail: if row.source_kind.order_backed() {
                    "Customer support case".to_owned()
                } else {
                    "Issue reported for Support triage".to_owned()
                },
                href: format!("/support/{}", row.id),
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

    fn actor(can_manage: bool, can_work: bool, can_write: bool) -> TicketActor {
        TicketActor {
            user_id: "agent".into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            permissions: TicketPermissions {
                can_read: true,
                can_create: can_write,
                can_write,
                can_manage,
                can_work,
            },
            same_person_ids: vec![],
        }
    }
    fn snapshot(status: TicketStatus, source: SourceKind) -> TicketSnapshot {
        TicketSnapshot {
            id: "ticket-1".into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            source_kind: source,
            status,
            owner_id: Some("agent".into()),
            resolved_by: None,
            created_by: "reporter".into(),
            confirmation_note_id: None,
            version: 1,
        }
    }
    fn change(action: TicketChange) -> TicketChange {
        action
    }

    #[test]
    fn creates_a_general_report_in_triage() {
        let plan = plan_create(
            &actor(false, true, true),
            "key",
            &TicketCreate::General {
                title: "Printer offline".into(),
                issue: "The front desk printer is offline.".into(),
            },
            10,
        )
        .unwrap();
        let TicketPlan::Create(plan) = plan else {
            panic!()
        };
        assert_eq!(plan.status, TicketStatus::Triage);
        assert_eq!(plan.event_action, "report");
        assert_eq!(plan.created_by, "agent");
    }

    #[test]
    fn creates_an_order_ticket_open_with_owner_and_due_date() {
        let plan = plan_create(
            &actor(false, true, true),
            "key",
            &TicketCreate::Order {
                order_id: "order".into(),
                source_version: 1,
                title: "Installation help".into(),
                issue: "Customer needs installation help.".into(),
                owner_id: "agent".into(),
                due_date: "2026-11-10".into(),
            },
            10,
        )
        .unwrap();
        let TicketPlan::Create(plan) = plan else {
            panic!()
        };
        assert_eq!(plan.status, TicketStatus::Open);
        assert_eq!(plan.owner_id.as_deref(), Some("agent"));
        assert_eq!(plan.order_id.as_deref(), Some("order"));
    }

    #[test]
    fn rejects_unknown_source_kinds_and_bad_dates() {
        let source = TicketCreate::Source {
            source: TicketSourceReference {
                kind: "unknown".into(),
                id: "id".into(),
                version: 1,
            },
            title: "Title".into(),
            issue: "Issue".into(),
        };
        assert_eq!(
            plan_create(&actor(false, true, true), "key", &source, 1)
                .unwrap_err()
                .code,
            "SUPPORT_SOURCE_INVALID"
        );
        let order = TicketCreate::Order {
            order_id: "order".into(),
            source_version: 1,
            title: "Title".into(),
            issue: "Issue".into(),
            owner_id: "agent".into(),
            due_date: "2026-02-31".into(),
        };
        assert_eq!(
            plan_create(&actor(false, true, true), "key", &order, 1)
                .unwrap_err()
                .code,
            "SUPPORT_DATE_INVALID"
        );
    }

    #[test]
    fn enforces_owner_manage_and_independence_transitions() {
        let agent = actor(false, true, true);
        let resolver = actor(true, true, true);
        let mut row = snapshot(TicketStatus::Open, SourceKind::SalesOrder);
        let start = change(TicketChange::Start {
            expected_version: 1,
            reason: "Start assistance".into(),
        });
        let TicketPlan::Change(plan) = plan_change(&agent, &row, &start, 2).unwrap() else {
            panic!()
        };
        assert_eq!(plan.status, TicketStatus::InProgress);
        row.status = TicketStatus::Resolved;
        row.resolved_by = Some("agent".into());
        let confirm = change(TicketChange::Confirm {
            expected_version: 1,
            reason: "Independent review".into(),
            note_id: "note".into(),
            note_version: 1,
        });
        assert_eq!(
            plan_change(&resolver, &row, &confirm, 3).unwrap_err().code,
            "INDEPENDENT_CONFIRMATION_REQUIRED"
        );
        row.owner_id = Some("other".into());
        row.resolved_by = Some("other".into());
        let TicketPlan::Change(plan) = plan_change(&resolver, &row, &confirm, 3).unwrap() else {
            panic!()
        };
        assert_eq!(plan.status, TicketStatus::Closed);
        assert_eq!(plan.confirmation_note_id.as_deref(), Some("note"));
    }

    #[test]
    fn reporting_is_limited_to_the_original_reporter_and_triage() {
        let reporter = TicketActor {
            user_id: "reporter".into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            permissions: TicketPermissions {
                can_read: true,
                can_create: true,
                can_write: true,
                can_manage: false,
                can_work: false,
            },
            same_person_ids: vec![],
        };
        let mut row = snapshot(TicketStatus::Triage, SourceKind::General);
        row.created_by = "reporter".into();
        let edit = change(TicketChange::EditReport {
            expected_version: 1,
            title: "Corrected title".into(),
            issue: "Corrected issue".into(),
            reason: "Correct the report".into(),
        });
        let TicketPlan::Change(plan) = plan_change(&reporter, &row, &edit, 2).unwrap() else {
            panic!()
        };
        assert_eq!(plan.title.as_deref(), Some("Corrected title"));
        row.status = TicketStatus::Open;
        assert_eq!(
            plan_change(&reporter, &row, &edit, 2).unwrap_err().code,
            "SUPPORT_REPORT_CHANGED"
        );
        row.status = TicketStatus::Triage;
        assert_eq!(
            plan_change(&actor(true, true, true), &row, &edit, 2)
                .unwrap_err()
                .code,
            "SUPPORT_REPORTER_REQUIRED"
        );
    }

    #[test]
    fn rejects_field_service_and_stale_versions() {
        let mut row = snapshot(TicketStatus::Open, SourceKind::FieldService);
        let start = change(TicketChange::Start {
            expected_version: 1,
            reason: "Start".into(),
        });
        assert_eq!(
            plan_change(&actor(false, true, true), &row, &start, 2)
                .unwrap_err()
                .code,
            "SOURCE_WORKSPACE_REQUIRED"
        );
        row.source_kind = SourceKind::General;
        let stale = change(TicketChange::Start {
            expected_version: 2,
            reason: "Start".into(),
        });
        assert_eq!(
            plan_change(&actor(false, true, true), &row, &stale, 2)
                .unwrap_err()
                .code,
            "SUPPORT_STATE_CHANGED"
        );
    }

    #[test]
    fn work_items_separate_triage_review_and_terminal_states() {
        let manager = actor(true, true, true);
        let mut triage = snapshot(TicketStatus::Triage, SourceKind::General);
        triage.owner_id = None;
        let mut review = snapshot(TicketStatus::Resolved, SourceKind::SalesOrder);
        review.owner_id = Some("other".into());
        review.resolved_by = Some("other".into());
        let mut closed = snapshot(TicketStatus::Closed, SourceKind::General);
        closed.owner_id = None;
        let items = work_items(
            &manager,
            &[
                (triage, "Triage".into()),
                (review, "Review".into()),
                (closed, "Closed".into()),
            ],
        );
        assert_eq!(items[0].queue, "todo");
        assert_eq!(items[0].responsible_queue, "Support triage");
        assert_eq!(items[1].queue, "todo");
        assert_eq!(items[1].responsible_queue, "Independent Support review");
        assert_eq!(items[2].queue, "completed");
        assert!(!items[2].can_act);
    }
}
