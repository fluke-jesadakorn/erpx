//! Portable Support SLA policy, resolution-time clock and alert rules. No
//! database, HTTP, clock or notification effect is reachable here.
use crate::domain::business_time::{
    BusinessCalendar, WorkingCalendar, add_business_time, elapsed_business_time,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

pub const MAX_REASON: usize = 2_000;
pub const MAX_NAME: usize = 160;
pub const MAX_TARGET_MINUTES: i64 = 28_800;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlaPolicy {
    pub calendar: BusinessCalendar,
    pub target_minutes: i64,
    pub pause_while_waiting: bool,
}

impl SlaPolicy {
    pub fn parse(self) -> Result<WorkingPolicy, BusinessError> {
        if !(1..=MAX_TARGET_MINUTES).contains(&self.target_minutes) {
            return Err(BusinessError::invalid(
                "SUPPORT_SLA_POLICY_INVALID",
                "Choose a resolution target between one minute and twenty working days.",
            ));
        }
        Ok(WorkingPolicy {
            calendar: self.calendar.clone().parse()?,
            policy: self,
        })
    }
}

#[derive(Clone, Debug)]
pub struct WorkingPolicy {
    pub policy: SlaPolicy,
    pub calendar: WorkingCalendar,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClockState {
    Running,
    Paused,
    Stopped,
}

impl ClockState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Stopped => "stopped",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportClock {
    pub snapshot: SlaPolicy,
    pub remaining_ms: i64,
    pub started_at: Option<i64>,
    pub due_at: Option<i64>,
    pub state: ClockState,
    pub breached_at: Option<i64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyStatus {
    Draft,
    Approved,
    Retired,
}

impl PolicyStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Approved => "approved",
            Self::Retired => "retired",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SlaPermissions {
    pub can_read: bool,
    pub can_manage: bool,
}

#[derive(Clone, Debug)]
pub struct SlaActor {
    pub user_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub permissions: SlaPermissions,
    pub same_person_ids: Vec<String>,
}

impl SlaActor {
    pub fn same_person(&self, user_id: &str) -> bool {
        user_id == self.user_id || self.same_person_ids.iter().any(|id| id == user_id)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PolicyCreate {
    pub name: String,
    pub policy: SlaPolicy,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case", deny_unknown_fields)]
pub enum PolicyChange {
    Approve {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
    },
    Retire {
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
    },
}

impl PolicyChange {
    pub fn expected_version(&self) -> i64 {
        match self {
            Self::Approve { expected_version, .. } | Self::Retire { expected_version, .. } => {
                *expected_version
            }
        }
    }
    pub fn action(&self) -> &'static str {
        match self {
            Self::Approve { .. } => "approve",
            Self::Retire { .. } => "retire",
        }
    }
    pub fn reason(&self) -> &str {
        match self {
            Self::Approve { reason, .. } | Self::Retire { reason, .. } => reason,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClockAttach {
    pub expected_version: i64,
    pub policy_id: String,
    pub policy_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AlertAcknowledge {
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicySnapshot {
    pub id: String,
    pub company_id: String,
    pub status: PolicyStatus,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TicketSnapshot {
    pub id: String,
    pub company_id: String,
    pub status: String,
    pub version: i64,
    pub sla_policy_id: Option<String>,
    pub clock: Option<SupportClock>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AlertSnapshot {
    pub id: String,
    pub ticket_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub version: i64,
    pub acknowledged_by: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyCreatePlan {
    pub id: String,
    pub company_id: String,
    pub name: String,
    pub policy_json: String,
    pub reason: String,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyChangePlan {
    pub id: String,
    pub company_id: String,
    pub action: &'static str,
    pub actor_id: String,
    pub status: PolicyStatus,
    pub approved_by: Option<String>,
    pub approved_at: Option<i64>,
    pub reason: String,
    pub version: i64,
    pub event_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClockAttachPlan {
    pub ticket_id: String,
    pub company_id: String,
    pub policy_id: String,
    pub policy_version: i64,
    pub clock_json: String,
    pub ticket_version: i64,
    pub reason: String,
    pub event_at: i64,
    pub actor_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AlertAcknowledgePlan {
    pub id: String,
    pub company_id: String,
    pub actor_id: String,
    pub reason: String,
    pub version: i64,
    pub event_at: i64,
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
            "SUPPORT_SLA_VERSION_INVALID",
            "Provide a positive version.",
        ));
    }
    Ok(value)
}

fn require_company_manager(actor: &SlaActor) -> Result<(), BusinessError> {
    if !actor.permissions.can_manage || !actor.permissions.can_read {
        return Err(forbidden(
            "SUPPORT_SLA_MANAGE_REQUIRED",
            "Choose a role that can manage Support SLA.",
        ));
    }
    if actor.branch_id.is_some() || actor.department_id.is_some() {
        return Err(forbidden(
            "SUPPORT_COMPANY_POLICY_REQUIRED",
            "Use a Company Support manager assignment to manage SLA policies.",
        ));
    }
    Ok(())
}

pub fn start_clock(policy: &SlaPolicy, now: i64) -> Result<SupportClock, BusinessError> {
    let working = policy.clone().parse()?;
    let remaining_ms = policy.target_minutes * 60_000;
    Ok(SupportClock {
        snapshot: working.policy,
        remaining_ms,
        started_at: Some(now),
        due_at: Some(add_business_time(&working.calendar, now, remaining_ms)?),
        state: ClockState::Running,
        breached_at: None,
    })
}

/// The ticket owner advances this inside its own versioned command; this
/// function never writes a ticket itself.
pub fn advance_clock(
    clock: &SupportClock,
    action: &str,
    now: i64,
) -> Result<SupportClock, BusinessError> {
    let working = clock.snapshot.clone().parse()?;
    if let Some(started) = clock.started_at
        && now < started
    {
        return Err(invalid(
            "SUPPORT_CLOCK_BACKWARDS",
            "Support clock cannot move backwards.",
        ));
    }
    let mut next = clock.clone();
    if clock.state == ClockState::Running
        && let (Some(started), Some(due)) = (clock.started_at, clock.due_at)
    {
        if clock.remaining_ms > 0 {
            let end = now.min(due).max(started);
            next.remaining_ms = (clock.remaining_ms
                - elapsed_business_time(&working.calendar, started, end)?)
            .max(0);
        }
        if now > due {
            next.breached_at.get_or_insert(due);
        }
        next.started_at = Some(now);
    }
    if matches!(action, "resolve" | "cancel") {
        next.state = ClockState::Stopped;
        next.started_at = None;
    } else if action == "wait-customer" && clock.snapshot.pause_while_waiting {
        next.state = ClockState::Paused;
        next.started_at = None;
        if next.breached_at.is_none() {
            next.due_at = None;
        }
    } else if matches!(action, "resume" | "reopen" | "reject-resolution")
        && clock.state != ClockState::Running
    {
        next.state = ClockState::Running;
        next.started_at = Some(now);
        next.due_at = Some(match next.breached_at {
            Some(breached) => breached,
            None => add_business_time(&working.calendar, now, next.remaining_ms)?,
        });
    }
    Ok(next)
}

pub fn plan_policy_create(
    actor: &SlaActor,
    key: &str,
    input: &PolicyCreate,
    now: i64,
) -> Result<PolicyCreatePlan, BusinessError> {
    require_company_manager(actor)?;
    let name = text(&input.name, "SUPPORT_POLICY_NAME_INVALID", 3, MAX_NAME)?;
    let reason = text(
        &input.reason,
        "SUPPORT_POLICY_REASON_INVALID",
        3,
        MAX_REASON,
    )?;
    let _ = start_clock(&input.policy, now)?;
    Ok(PolicyCreatePlan {
        id: format!("support-policy-{key}"),
        company_id: actor.company_id.clone(),
        name,
        policy_json: serde_json::to_string(&input.policy)
            .map_err(|_| invalid("SUPPORT_POLICY_INVALID", "Provide a valid policy."))?,
        reason,
        created_by: actor.user_id.clone(),
        created_at: now,
        updated_at: now,
    })
}

pub fn plan_policy_change(
    actor: &SlaActor,
    snapshot: &PolicySnapshot,
    input: &PolicyChange,
    now: i64,
) -> Result<PolicyChangePlan, BusinessError> {
    require_company_manager(actor)?;
    if snapshot.company_id != actor.company_id {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "SUPPORT_POLICY_NOT_FOUND",
            "Policy not found in this Company.",
        ));
    }
    if input.expected_version() != snapshot.version {
        return Err(conflict(
            "SUPPORT_POLICY_STATE",
            "Review the current policy state.",
        ));
    }
    let reason = text(
        input.reason(),
        "SUPPORT_POLICY_REASON_INVALID",
        3,
        MAX_REASON,
    )?;
    match input {
        PolicyChange::Approve { .. } => {
            if snapshot.status != PolicyStatus::Draft {
                return Err(conflict(
                    "SUPPORT_POLICY_STATE",
                    "Review the current policy state.",
                ));
            }
            if actor.same_person(&snapshot.created_by) {
                return Err(forbidden(
                    "INDEPENDENT_APPROVER_REQUIRED",
                    "Another Company Support manager must approve this policy.",
                ));
            }
            Ok(PolicyChangePlan {
                id: snapshot.id.clone(),
                company_id: snapshot.company_id.clone(),
                action: "approve",
                actor_id: actor.user_id.clone(),
                status: PolicyStatus::Approved,
                approved_by: Some(actor.user_id.clone()),
                approved_at: Some(now),
                reason,
                version: snapshot.version + 1,
                event_at: now,
            })
        }
        PolicyChange::Retire { .. } => {
            if snapshot.status == PolicyStatus::Retired {
                return Err(conflict(
                    "SUPPORT_POLICY_STATE",
                    "Review the current policy state.",
                ));
            }
            Ok(PolicyChangePlan {
                id: snapshot.id.clone(),
                company_id: snapshot.company_id.clone(),
                action: "retire",
                actor_id: actor.user_id.clone(),
                status: PolicyStatus::Retired,
                approved_by: snapshot.approved_by.clone(),
                approved_at: None,
                reason,
                version: snapshot.version + 1,
                event_at: now,
            })
        }
    }
}

pub fn plan_attach(
    actor: &SlaActor,
    ticket: &TicketSnapshot,
    policy: &PolicySnapshot,
    policy_value: &SlaPolicy,
    input: &ClockAttach,
    now: i64,
) -> Result<ClockAttachPlan, BusinessError> {
    if !actor.permissions.can_manage || !actor.permissions.can_read {
        return Err(forbidden(
            "SUPPORT_SLA_MANAGE_REQUIRED",
            "Choose a role that can manage Support SLA.",
        ));
    }
    version(input.expected_version)?;
    version(input.policy_version)?;
    if ticket.company_id != actor.company_id {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "SUPPORT_TICKET_NOT_FOUND",
            "Ticket not found in this scope.",
        ));
    }
    if ticket.version != input.expected_version {
        return Err(conflict(
            "SUPPORT_TICKET_STATE_CHANGED",
            "Review the current ticket state.",
        ));
    }
    if ticket.status != "open" || ticket.sla_policy_id.is_some() {
        return Err(conflict(
            "SUPPORT_TICKET_STATE_CHANGED",
            "Review the current ticket state.",
        ));
    }
    if policy.company_id != actor.company_id
        || policy.status != PolicyStatus::Approved
        || policy.version != input.policy_version
    {
        return Err(conflict(
            "SUPPORT_POLICY_STATE",
            "Review the current policy state.",
        ));
    }
    let reason = text(
        &input.reason,
        "SUPPORT_SLA_REASON_INVALID",
        3,
        MAX_REASON,
    )?;
    let clock = start_clock(policy_value, now)?;
    Ok(ClockAttachPlan {
        ticket_id: ticket.id.clone(),
        company_id: ticket.company_id.clone(),
        policy_id: policy.id.clone(),
        policy_version: policy.version,
        clock_json: clock_value(&clock)?,
        ticket_version: ticket.version + 1,
        reason,
        event_at: now,
        actor_id: actor.user_id.clone(),
    })
}

pub fn plan_alert_acknowledge(
    actor: &SlaActor,
    snapshot: &AlertSnapshot,
    input: &AlertAcknowledge,
    now: i64,
) -> Result<AlertAcknowledgePlan, BusinessError> {
    if !actor.permissions.can_manage || !actor.permissions.can_read {
        return Err(forbidden(
            "SUPPORT_SLA_MANAGE_REQUIRED",
            "Choose a role that can manage Support SLA.",
        ));
    }
    if snapshot.company_id != actor.company_id {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "SUPPORT_ALERT_NOT_FOUND",
            "Target escalation is not available in this scope.",
        ));
    }
    if snapshot.acknowledged_by.is_some() {
        return Err(conflict(
            "SUPPORT_ALERT_STATE",
            "This target escalation is already acknowledged.",
        ));
    }
    if input.expected_version != snapshot.version {
        return Err(conflict(
            "SUPPORT_ALERT_STATE",
            "Review the current target escalation.",
        ));
    }
    Ok(AlertAcknowledgePlan {
        id: snapshot.id.clone(),
        company_id: snapshot.company_id.clone(),
        actor_id: actor.user_id.clone(),
        reason: text(
            &input.reason,
            "SUPPORT_ALERT_REASON_INVALID",
            3,
            MAX_REASON,
        )?,
        version: snapshot.version + 1,
        event_at: now,
    })
}

pub fn clock_value(clock: &SupportClock) -> Result<String, BusinessError> {
    serde_json::to_string(clock)
        .map_err(|_| invalid("SUPPORT_CLOCK_INVALID", "Provide a valid support clock."))
}

pub fn parse_clock(value: &str) -> Result<SupportClock, BusinessError> {
    serde_json::from_str(value)
        .map_err(|_| invalid("SUPPORT_CLOCK_INVALID", "Provide a valid support clock."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> SlaPolicy {
        SlaPolicy {
            calendar: BusinessCalendar {
                timezone: "Asia/Bangkok".into(),
                weekdays: vec![1, 2, 3, 4, 5],
                opens_at_minute: 9 * 60,
                closes_at_minute: 17 * 60,
                holidays: vec![],
            },
            target_minutes: 60,
            pause_while_waiting: true,
        }
    }
    fn actor(user: &str, manage: bool) -> SlaActor {
        SlaActor {
            user_id: user.into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            permissions: SlaPermissions {
                can_read: true,
                can_manage: manage,
            },
            same_person_ids: vec![],
        }
    }
    fn policy_snapshot(status: PolicyStatus) -> PolicySnapshot {
        PolicySnapshot {
            id: "support-policy-1".into(),
            company_id: "company".into(),
            status,
            created_by: "creator".into(),
            approved_by: None,
            version: 1,
        }
    }
    fn ticket() -> TicketSnapshot {
        TicketSnapshot {
            id: "ticket-1".into(),
            company_id: "company".into(),
            status: "open".into(),
            version: 1,
            sla_policy_id: None,
            clock: None,
        }
    }
    const START: i64 = 1_793_757_600_000; // 2026-11-04T02:00:00Z

    #[test]
    fn starts_and_advances_a_paused_clock() {
        let clock = start_clock(&policy(), START).unwrap();
        assert_eq!(clock.state, ClockState::Running);
        assert_eq!(clock.due_at, Some(START + 60 * 60 * 1_000));
        let paused = advance_clock(&clock, "wait-customer", START + 30 * 60 * 1_000).unwrap();
        assert_eq!(paused.state, ClockState::Paused);
        assert_eq!(paused.remaining_ms, 30 * 60 * 1_000);
        assert_eq!(paused.due_at, None);
        let resumed = advance_clock(&paused, "resume", START + 60 * 60 * 1_000).unwrap();
        assert_eq!(resumed.state, ClockState::Running);
        assert_eq!(resumed.due_at, Some(START + 90 * 60 * 1_000));
        let stopped = advance_clock(&resumed, "resolve", START + 70 * 60 * 1_000).unwrap();
        assert_eq!(stopped.state, ClockState::Stopped);
        assert_eq!(stopped.started_at, None);
    }

    #[test]
    fn requires_an_independent_company_manager_to_approve() {
        let create = PolicyCreate {
            name: "Standard support".into(),
            policy: policy(),
            reason: "Draft the standard support target".into(),
        };
        let plan = plan_policy_create(&actor("creator", true), "key", &create, START).unwrap();
        assert_eq!(plan.id, "support-policy-key");
        let approve = PolicyChange::Approve {
            expected_version: 1,
            reason: "Independent approval".into(),
        };
        assert_eq!(
            plan_policy_change(&actor("creator", true), &policy_snapshot(PolicyStatus::Draft), &approve, START)
                .unwrap_err()
                .code,
            "INDEPENDENT_APPROVER_REQUIRED"
        );
        let plan = plan_policy_change(
            &actor("reviewer", true),
            &policy_snapshot(PolicyStatus::Draft),
            &approve,
            START,
        )
        .unwrap();
        assert_eq!(plan.status, PolicyStatus::Approved);
        let branch = SlaActor {
            branch_id: Some("branch".into()),
            ..actor("reviewer", true)
        };
        assert_eq!(
            plan_policy_create(&branch, "key", &create, START)
                .unwrap_err()
                .code,
            "SUPPORT_COMPANY_POLICY_REQUIRED"
        );
    }

    #[test]
    fn attaches_only_an_approved_current_policy_to_an_open_ticket() {
        let input = ClockAttach {
            expected_version: 1,
            policy_id: "support-policy-1".into(),
            policy_version: 1,
            reason: "Attach the standard target".into(),
        };
        let plan = plan_attach(
            &actor("manager", true),
            &ticket(),
            &policy_snapshot(PolicyStatus::Approved),
            &policy(),
            &input,
            START,
        )
        .unwrap();
        assert_eq!(plan.ticket_version, 2);
        assert_eq!(
            plan_attach(
                &actor("manager", true),
                &ticket(),
                &policy_snapshot(PolicyStatus::Draft),
                &policy(),
                &input,
                START
            )
            .unwrap_err()
            .code,
            "SUPPORT_POLICY_STATE"
        );
        let mut attached = ticket();
        attached.sla_policy_id = Some("support-policy-1".into());
        assert_eq!(
            plan_attach(
                &actor("manager", true),
                &attached,
                &policy_snapshot(PolicyStatus::Approved),
                &policy(),
                &input,
                START
            )
            .unwrap_err()
            .code,
            "SUPPORT_TICKET_STATE_CHANGED"
        );
    }

    #[test]
    fn acknowledges_an_open_alert_once() {
        let snapshot = AlertSnapshot {
            id: "support-breach-ticket-1".into(),
            ticket_id: "ticket-1".into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            version: 1,
            acknowledged_by: None,
        };
        let input = AlertAcknowledge {
            expected_version: 1,
            reason: "Take responsibility and follow the recovery plan".into(),
        };
        let plan = plan_alert_acknowledge(&actor("manager", true), &snapshot, &input, START).unwrap();
        assert_eq!(plan.version, 2);
        let mut done = snapshot;
        done.acknowledged_by = Some("manager".into());
        assert_eq!(
            plan_alert_acknowledge(&actor("manager", true), &done, &input, START)
                .unwrap_err()
                .code,
            "SUPPORT_ALERT_STATE"
        );
    }
}
