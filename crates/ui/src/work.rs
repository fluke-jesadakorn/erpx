//! Target-neutral contracts for the role-aware ERPX work queue.
//!
//! The browser receives only bounded display fields and allow-listed targets.
//! Authorization remains server-owned: action capabilities are discoverability
//! hints and never grant permission to execute a command.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadModelAvailability {
    Available,
    Partial,
    Empty,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadModelFreshness {
    Live,
    Current,
    Stale,
    Unknown,
}

pub const MAX_WORK_ITEMS: usize = 48;
pub const MAX_WORK_SOURCE_CONTRIBUTIONS: usize = 12;
pub const MAX_WORK_ACTION_CAPABILITIES: usize = 16;
pub const MAX_WORK_TEXT_BYTES: usize = 512;
pub const MAX_WORK_REFERENCE_BYTES: usize = 128;
pub const MAX_WORK_PAGE_SIZE: u32 = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkQueueView {
    All,
    Mine,
    Team,
    Overdue,
    Approvals,
}

impl WorkQueueView {
    pub const fn query_value(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Mine => "mine",
            Self::Team => "team",
            Self::Overdue => "overdue",
            Self::Approvals => "approvals",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "all" => Some(Self::All),
            "mine" => Some(Self::Mine),
            "team" => Some(Self::Team),
            "overdue" => Some(Self::Overdue),
            "approvals" => Some(Self::Approvals),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkDomain {
    Operations,
    Support,
    Finance,
    People,
    Maintenance,
    Procurement,
}

impl WorkDomain {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Operations => "operations",
            Self::Support => "support",
            Self::Finance => "finance",
            Self::People => "people",
            Self::Maintenance => "maintenance",
            Self::Procurement => "procurement",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemKind {
    Approval,
    Exception,
    Incident,
    Task,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemSeverity {
    Critical,
    High,
    Medium,
    Low,
    Informational,
}

impl WorkItemSeverity {
    pub const fn rank(self) -> u8 {
        match self {
            Self::Critical => 0,
            Self::High => 1,
            Self::Medium => 2,
            Self::Low => 3,
            Self::Informational => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemStage {
    Open,
    InProgress,
    Waiting,
    Resolved,
    Closed,
}

impl WorkItemStage {
    pub const fn active(self) -> bool {
        matches!(self, Self::Open | Self::InProgress | Self::Waiting)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkSlaState {
    NoDueDate,
    Healthy,
    DueToday,
    Overdue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkRoute {
    PlatformSupport,
    PlatformIncidents,
    OperationsExceptions,
    FinanceClose,
    PeopleApprovals,
    MaintenanceWorkOrders,
    ProcurementOrders,
}

impl WorkRoute {
    pub const fn path(self) -> &'static str {
        match self {
            Self::PlatformSupport => "/platform/support",
            Self::PlatformIncidents => "/platform/incidents",
            Self::OperationsExceptions => "/operations/exceptions",
            Self::FinanceClose => "/finance/close",
            Self::PeopleApprovals => "/hr",
            Self::MaintenanceWorkOrders => "/maintenance",
            Self::ProcurementOrders => "/procurement",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkItemTarget {
    pub route: WorkRoute,
    pub record_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkItemReadModel {
    pub key: String,
    pub domain: WorkDomain,
    pub kind: WorkItemKind,
    pub title: String,
    pub summary: String,
    pub stage: WorkItemStage,
    pub status_label: String,
    pub severity: WorkItemSeverity,
    pub assignee: Option<String>,
    pub due_at: Option<String>,
    pub updated_at: Option<String>,
    pub sla: WorkSlaState,
    pub freshness: ReadModelFreshness,
    pub target: WorkItemTarget,
    pub action_capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourceContribution {
    pub domain: WorkDomain,
    pub label: String,
    pub availability: ReadModelAvailability,
    pub freshness: ReadModelFreshness,
    pub item_count: u32,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkQueueMeta {
    pub scope: String,
    pub as_of: String,
    pub freshness: ReadModelFreshness,
    pub availability: ReadModelAvailability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkQueueCounters {
    pub total: u32,
    pub overdue: u32,
    pub due_today: u32,
    pub approvals: u32,
    pub exceptions: u32,
    pub waiting: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkQueueReadModel {
    pub view: WorkQueueView,
    pub meta: WorkQueueMeta,
    pub counters: WorkQueueCounters,
    pub items: Vec<WorkItemReadModel>,
    pub contributions: Vec<WorkSourceContribution>,
    pub page_size: u32,
    pub returned_count: u32,
    pub next_cursor: Option<String>,
}

impl WorkQueueReadModel {
    pub fn is_bounded(&self) -> bool {
        self.page_size > 0
            && self.page_size <= MAX_WORK_PAGE_SIZE
            && self.returned_count == self.items.len() as u32
            && self.items.len() <= self.page_size as usize
            && self.items.len() <= MAX_WORK_ITEMS
            && self.contributions.len() <= MAX_WORK_SOURCE_CONTRIBUTIONS
            && valid_text(&self.meta.scope, MAX_WORK_TEXT_BYTES, false)
            && DateTime::parse_from_rfc3339(&self.meta.as_of).is_ok()
            && self.next_cursor.as_deref().is_none_or(valid_work_cursor)
            && self.items.iter().all(WorkItemReadModel::is_bounded)
            && self
                .contributions
                .iter()
                .all(WorkSourceContribution::is_bounded)
    }

    pub fn counters_for(items: &[WorkItemReadModel], _now: DateTime<Utc>) -> WorkQueueCounters {
        let active = items.iter().filter(|item| item.stage.active());
        let total = active.clone().count() as u32;
        let overdue = active
            .clone()
            .filter(|item| item.sla == WorkSlaState::Overdue)
            .count() as u32;
        let due_today = active
            .clone()
            .filter(|item| item.sla == WorkSlaState::DueToday)
            .count() as u32;
        let approvals = active
            .clone()
            .filter(|item| item.kind == WorkItemKind::Approval)
            .count() as u32;
        let exceptions = active
            .clone()
            .filter(|item| matches!(item.kind, WorkItemKind::Exception | WorkItemKind::Incident))
            .count() as u32;
        let waiting = active
            .filter(|item| item.stage == WorkItemStage::Waiting)
            .count() as u32;
        WorkQueueCounters {
            total,
            overdue,
            due_today,
            approvals,
            exceptions,
            waiting,
        }
    }
}

impl WorkItemReadModel {
    fn is_bounded(&self) -> bool {
        valid_text(&self.key, MAX_WORK_REFERENCE_BYTES, false)
            && valid_text(&self.title, MAX_WORK_TEXT_BYTES, false)
            && valid_text(&self.summary, MAX_WORK_TEXT_BYTES, true)
            && valid_text(&self.status_label, MAX_WORK_REFERENCE_BYTES, false)
            && self
                .assignee
                .as_deref()
                .is_none_or(|value| valid_text(value, MAX_WORK_REFERENCE_BYTES, false))
            && self.due_at.as_deref().is_none_or(valid_timestamp)
            && self.updated_at.as_deref().is_none_or(valid_timestamp)
            && self
                .target
                .record_ref
                .as_deref()
                .is_none_or(|value| valid_text(value, MAX_WORK_REFERENCE_BYTES, false))
            && self.action_capabilities.len() <= MAX_WORK_ACTION_CAPABILITIES
            && self
                .action_capabilities
                .iter()
                .all(|value| valid_capability(value))
    }
}

impl WorkSourceContribution {
    fn is_bounded(&self) -> bool {
        valid_text(&self.label, MAX_WORK_REFERENCE_BYTES, false)
            && valid_text(&self.message, MAX_WORK_TEXT_BYTES, true)
            && self.item_count <= MAX_WORK_ITEMS as u32
    }
}

pub fn sla_state(due_at: Option<&str>, now: DateTime<Utc>) -> WorkSlaState {
    let Some(due_at) = due_at.and_then(|value| DateTime::parse_from_rfc3339(value).ok()) else {
        return WorkSlaState::NoDueDate;
    };
    let due_at = due_at.with_timezone(&Utc);
    if due_at < now {
        WorkSlaState::Overdue
    } else if due_at.date_naive() == now.date_naive() {
        WorkSlaState::DueToday
    } else {
        WorkSlaState::Healthy
    }
}

pub fn valid_work_cursor(value: &str) -> bool {
    value
        .strip_prefix("offset:")
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|offset| offset <= 10_000)
}

fn valid_timestamp(value: &str) -> bool {
    value.len() <= 64 && DateTime::parse_from_rfc3339(value).is_ok()
}

fn valid_text(value: &str, max_bytes: usize, allow_empty: bool) -> bool {
    (allow_empty || !value.trim().is_empty())
        && value.len() <= max_bytes
        && !value.chars().any(char::is_control)
}

fn valid_capability(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
        && !value.to_ascii_lowercase().contains("token")
        && !value.to_ascii_lowercase().contains("secret")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: WorkItemKind, sla: WorkSlaState) -> WorkItemReadModel {
        WorkItemReadModel {
            key: "work-1".into(),
            domain: WorkDomain::Support,
            kind,
            title: "Review support incident".into(),
            summary: "Customer-visible service interruption".into(),
            stage: WorkItemStage::Open,
            status_label: "investigating".into(),
            severity: WorkItemSeverity::High,
            assignee: Some("principal-1".into()),
            due_at: Some("2026-08-08T12:00:00Z".into()),
            updated_at: Some("2026-08-08T10:00:00Z".into()),
            sla,
            freshness: ReadModelFreshness::Current,
            target: WorkItemTarget {
                route: WorkRoute::PlatformIncidents,
                record_ref: Some("incident-1".into()),
            },
            action_capabilities: vec!["support.incident.open".into()],
        }
    }

    #[test]
    fn counters_are_derived_only_from_active_authoritative_items() {
        let mut waiting = item(WorkItemKind::Approval, WorkSlaState::DueToday);
        waiting.key = "work-2".into();
        waiting.stage = WorkItemStage::Waiting;
        let counters = WorkQueueReadModel::counters_for(
            &[item(WorkItemKind::Incident, WorkSlaState::Overdue), waiting],
            "2026-08-08T11:00:00Z".parse().unwrap(),
        );
        assert_eq!(counters.total, 2);
        assert_eq!(counters.overdue, 1);
        assert_eq!(counters.due_today, 1);
        assert_eq!(counters.approvals, 1);
        assert_eq!(counters.exceptions, 1);
        assert_eq!(counters.waiting, 1);
    }

    #[test]
    fn queue_rejects_arbitrary_targets_cursors_and_credential_like_capabilities() {
        assert_eq!(WorkRoute::PlatformSupport.path(), "/platform/support");
        assert!(valid_work_cursor("offset:48"));
        assert!(!valid_work_cursor("https://example.com"));
        let mut unsafe_item = item(WorkItemKind::Task, WorkSlaState::NoDueDate);
        unsafe_item.action_capabilities = vec!["access_token.read".into()];
        assert!(!unsafe_item.is_bounded());
    }
}
